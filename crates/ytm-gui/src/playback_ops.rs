use ytm_core::Track;

use crate::app::YtmGuiApp;
use crate::navigation::ToastState;
use crate::state::{AsyncState, GuiTab, WorkerCmd};
use crate::views::player_bar::PlayerBarOutcome;

/// High-level playback or navigation intent emitted by table rows, cards, or player bar.
#[derive(Debug, Clone)]
pub enum PlayAction {
    PlayFromList {
        tracks: Vec<Track>,
        start_idx: usize,
    },
    PlayNext(Track),
    AppendQueue(Track),
    ToggleLike(Track),
    StartRadio(Track),
    OpenBrowse(String),
}

/// Dispatch playback, queue, like, radio, and browse intents onto `app`.
pub fn dispatch_play_actions(actions: Vec<PlayAction>, app: &mut YtmGuiApp) {
    for action in actions {
        match action {
            PlayAction::PlayFromList { tracks, start_idx } => {
                let seed = current_unix_secs();
                if let Some(track) = app.play_queue.start_with(tracks, start_idx, seed).cloned() {
                    start_playing_track(&track, app);
                }
            }
            PlayAction::PlayNext(track) => {
                let title = track.title.clone();
                if app.play_queue.play_next(track.clone()) {
                    start_playing_track(&track, app);
                }
                app.toast = Some(ToastState::info(true, format!("Queued next: {title}")));
            }
            PlayAction::AppendQueue(track) => {
                let title = track.title.clone();
                if app.play_queue.append(track.clone()) {
                    start_playing_track(&track, app);
                }
                app.toast = Some(ToastState::info(true, format!("Added to queue: {title}")));
            }
            PlayAction::ToggleLike(track) => {
                let next_liked = !app.play_queue.is_liked(&track.video_id);
                app.play_queue.set_liked(&track.video_id, next_liked);
                let _ = app.cmd_tx.send(WorkerCmd::SetLikeStatus {
                    video_id: track.video_id,
                    liked: next_liked,
                });
            }
            PlayAction::StartRadio(track) => {
                if app.play_queue.current_track().map(|t| t.video_id.as_str())
                    != Some(&track.video_id)
                {
                    app.play_queue.start_with(vec![track.clone()], 0, 1);
                    start_playing_track(&track, app);
                }
                let _ = app.cmd_tx.send(WorkerCmd::LoadWatchNext {
                    video_id: track.video_id,
                    playlist_id: None,
                    seed_radio: true,
                });
                app.toast = Some(ToastState::info(true, "Fetching Radio station tracks..."));
            }
            PlayAction::OpenBrowse(browse_id) => {
                app.browse_target = browse_id.clone();
                app.browse_state = AsyncState::Loading(format!("Loading {browse_id}..."));
                app.active_tab = GuiTab::Browse;
                let _ = app.cmd_tx.send(WorkerCmd::LoadBrowse { browse_id });
            }
        }
    }
}

/// Start playback of `track` and request watch-next metadata for lyrics handle.
pub fn start_playing_track(track: &Track, app: &mut YtmGuiApp) {
    app.last_ended_counter = app.player.snapshot().ended_counter;
    app.lyrics_browse_id = None;
    app.lyrics_state = AsyncState::Idle;
    app.player.play_track(track);
    let _ = app.cmd_tx.send(WorkerCmd::LoadWatchNext {
        video_id: track.video_id.clone(),
        playlist_id: None,
        seed_radio: false,
    });
}

/// Auto-advance queue when `mpv` reports natural end-of-file (`eof`).
pub fn poll_track_end_advance(app: &mut YtmGuiApp) {
    let snap = app.player.snapshot();
    if snap.ended_counter > app.last_ended_counter {
        app.last_ended_counter = snap.ended_counter;
        if let Some(next_track) = app.play_queue.advance().cloned() {
            start_playing_track(&next_track, app);
        } else {
            app.player.stop();
        }
    }
}

/// Apply transport, seek, volume, shuffle/repeat, and tab-jump actions from `player_bar`.
pub fn apply_player_bar_outcome(out: PlayerBarOutcome, app: &mut YtmGuiApp) {
    if out.toggle_pause {
        app.player.toggle_pause();
    }
    if out.stop {
        app.player.stop();
    }
    if let Some(sec) = out.seek_to {
        app.player.seek_absolute(sec);
    }
    if let Some(vol) = out.volume_to {
        app.player.set_volume(vol);
    }
    if out.toggle_shuffle {
        app.play_queue.toggle_shuffle(current_unix_secs());
    }
    if out.cycle_repeat {
        app.play_queue.cycle_repeat();
    }
    if out.previous
        && let Some(prev) = app.play_queue.previous().cloned()
    {
        start_playing_track(&prev, app);
    }
    if out.next
        && let Some(nxt) = app.play_queue.advance().cloned()
    {
        start_playing_track(&nxt, app);
    }
    if out.open_queue_lyrics {
        app.active_tab = GuiTab::QueueLyrics;
    }
    dispatch_play_actions(out.actions, app);
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(7, |d| d.as_secs())
}
