use std::sync::mpsc::Sender;

use ytm_core::Track;

use crate::app::YtmGuiApp;
use crate::navigation::ToastState;
use crate::play_queue::PlayQueue;
use crate::state::{AsyncState, GuiTab, MutationPolicy, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};
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
    HarvestArtist {
        artist_id: String,
        artist_name: String,
    },
    OpenBrowse(String),
    OpenBrowseWithParams {
        browse_id: String,
        params: Option<String>,
    },
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
                handle_toggle_like(
                    track,
                    app.mutation_policy,
                    &mut app.play_queue,
                    &mut app.sync_queue,
                    &mut app.toast,
                    &app.cmd_tx,
                );
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
            PlayAction::HarvestArtist {
                artist_id,
                artist_name,
            } => {
                let _ = app.cmd_tx.send(WorkerCmd::HarvestArtist {
                    artist_id,
                    options: ytm_core::ArtistHarvestOptions::default(),
                });
                app.toast = Some(ToastState::info(
                    true,
                    format!("Harvesting all releases for {artist_name}..."),
                ));
            }
            PlayAction::OpenBrowse(browse_id) => {
                open_browse_target(browse_id, None, app);
            }
            PlayAction::OpenBrowseWithParams { browse_id, params } => {
                open_browse_target(browse_id, params, app);
            }
        }
    }
}

fn open_browse_target(browse_id: String, params: Option<String>, app: &mut YtmGuiApp) {
    app.browse_target = browse_id.clone();
    app.browse_state = AsyncState::Loading(format!("Loading {browse_id}..."));
    app.active_tab = GuiTab::Browse;
    let _ = app.cmd_tx.send(WorkerCmd::LoadBrowse { browse_id, params });
}

/// Start playback of `track`, reset lyrics state, persist playback session, and request watch-next metadata.
pub fn start_playing_track(track: &Track, app: &mut YtmGuiApp) {
    app.last_ended_counter = app.player.snapshot().ended_counter;
    app.last_prefetched_video_id = None;
    app.lyrics_browse_id = None;
    app.lyrics_state = AsyncState::Idle;
    app.lyrics_payload = AsyncState::Idle;
    app.lyrics_video_id = Some(track.video_id.clone());
    app.player.play_track(track);
    let snap = app.play_queue.to_snapshot(app.player.snapshot().volume);
    if let Ok(json) = serde_json::to_string(&snap) {
        let _ = app.cmd_tx.send(WorkerCmd::SavePlaybackSession(json));
    }
    // Record play history
    let artists_json =
        serde_json::to_string(&track.artists.iter().map(|a| &a.name).collect::<Vec<_>>())
            .unwrap_or_default();
    let _ = app.cmd_tx.send(WorkerCmd::RecordPlayHistory {
        video_id: track.video_id.clone(),
        title: track.title.clone(),
        artists_json,
        album: track.album.as_ref().map(|a| a.name.clone()),
        duration_sec: track.duration_sec,
    });
    let _ = app.cmd_tx.send(WorkerCmd::LoadWatchNext {
        video_id: track.video_id.clone(),
        playlist_id: None,
        seed_radio: false,
    });
}

/// Auto-advance queue on `mpv` EOF and prefetch next track ~15 seconds before end.
pub fn poll_track_end_advance(app: &mut YtmGuiApp) {
    let snap = app.player.snapshot();
    if snap.ended_counter > app.last_ended_counter {
        app.last_ended_counter = snap.ended_counter;
        if let Some(next_track) = app.play_queue.advance().cloned() {
            start_playing_track(&next_track, app);
        } else {
            app.player.stop();
        }
        return;
    }
    if snap.duration_sec > 20.0
        && (snap.duration_sec - snap.position_sec) <= 15.0
        && let Some(next) = app.play_queue.peek_next().cloned()
        && app.last_prefetched_video_id.as_deref() != Some(next.video_id.as_str())
    {
        app.last_prefetched_video_id = Some(next.video_id.clone());
        app.player.prefetch_next(&next.video_id);
    }
}

/// Apply transport, seek, volume, mute, stage-add, shuffle/repeat, and tab-jump actions from `player_bar`.
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
        app.muted = vol <= 0.1;
        if vol > 0.1 {
            app.prev_volume = vol;
        }
        app.player.set_volume(vol);
    }
    if out.toggle_mute {
        if app.muted {
            app.player.set_volume(app.prev_volume.max(10.0));
            app.muted = false;
        } else {
            app.prev_volume = app.player.snapshot().volume;
            app.player.set_volume(0.0);
            app.muted = true;
        }
    }
    if let Some((pid, ptitle, track)) = out.stage_add {
        let title = track.title.clone();
        let eid = app.sync_queue.push(StagedOp::AddTracks {
            playlist_id: pid,
            playlist_title: ptitle.clone(),
            tracks: vec![track],
            allow_duplicates: false,
        });
        app.toast = Some(ToastState::staged(
            format!("Staged '{title}' → '{ptitle}'."),
            Some(eid),
        ));
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
    if out.toggle_queue_drawer {
        app.right_drawer.toggle_queue();
    }
    if out.toggle_lyrics_drawer {
        app.right_drawer.toggle_lyrics();
    }
    // Audio effects
    if let Some(filters) = out.apply_audio_filters {
        app.audio_effects.eq_preset = crate::audio_fx::EqPreset::ALL
            .iter()
            .find(|p| p.filter_string() == filters)
            .copied()
            .unwrap_or_default();
        app.player.set_audio_filters(filters);
    }
    if let Some(speed) = out.set_speed {
        app.audio_effects.speed = crate::audio_fx::PlaybackSpeed::new(speed);
        app.player.set_playback_speed(speed);
    }
    if let Some(duration) = out.set_sleep_timer {
        app.audio_effects.sleep_timer = crate::audio_fx::SleepTimerState::new(duration);
        app.player.set_sleep_timer(duration);
        app.toast = Some(crate::navigation::ToastState::info(
            true,
            format!("Sleep timer set for {} min.", duration.as_secs() / 60),
        ));
    }
    if out.cancel_sleep_timer {
        let _ = app.audio_effects.sleep_timer.cancel();
        app.player.cancel_sleep_timer();
        app.toast = Some(crate::navigation::ToastState::info(
            true,
            "Sleep timer cancelled.",
        ));
    }
    if out.open_lightbox
        && let Some(track) = app.play_queue.current_track()
        && let Some(url) = track.effective_thumbnail_url()
    {
        let sub = track.primary_artist_name().unwrap_or_default().to_string();
        app.lightbox.open(track.title.clone(), sub, url);
    }
    if let Some(url) = out.open_ytm_url {
        let browser = app.gui_config.preferred_external_browser.as_deref();
        if let Err(err) = crate::browser_launch::open_url_in_browser(&url, browser) {
            app.toast = Some(ToastState::info(
                false,
                format!("Open browser failed: {err}"),
            ));
        }
    }
    if let Some(url) = out.copy_ytm_url {
        app.pending_copy_url = Some(url);
        app.toast = Some(ToastState::info(
            true,
            crate::i18n::tr(app.lang(), crate::i18n::Key::LinkCopiedToast),
        ));
    }
    if out.apply_pending_configure {
        app.player.apply_pending_configure();
    }
    dispatch_play_actions(out.actions, app);
}

/// Poll OS media keys (`MPRIS2` / `SMTC`) and synchronize track metadata/playback state.
pub fn poll_os_media_bridge(app: &mut YtmGuiApp) {
    for ev in app.media_bridge.poll_events() {
        match ev {
            crate::media_controls::MediaKeyEvent::TogglePause
            | crate::media_controls::MediaKeyEvent::Play
            | crate::media_controls::MediaKeyEvent::Pause => {
                app.player.toggle_pause();
            }
            crate::media_controls::MediaKeyEvent::Next => {
                if let Some(nxt) = app.play_queue.advance().cloned() {
                    start_playing_track(&nxt, app);
                }
            }
            crate::media_controls::MediaKeyEvent::Previous => {
                if let Some(prev) = app.play_queue.previous().cloned() {
                    start_playing_track(&prev, app);
                }
            }
            crate::media_controls::MediaKeyEvent::Stop => app.player.stop(),
        }
    }
    app.media_bridge
        .sync_state(app.play_queue.current_track(), &app.player.snapshot());
}

/// Toggle mute/unmute preserving the previous non-zero volume.
pub fn toggle_mute_state(app: &mut YtmGuiApp) {
    if app.muted {
        app.player.set_volume(app.prev_volume.max(10.0));
        app.muted = false;
    } else {
        app.prev_volume = app.player.snapshot().volume;
        app.player.set_volume(0.0);
        app.muted = true;
    }
}

/// Enforces `MutationPolicy` when toggling like/unlike status for a track.
pub(crate) fn handle_toggle_like(
    track: Track,
    policy: MutationPolicy,
    play_queue: &mut PlayQueue,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    let next_liked = !play_queue.is_liked(&track.video_id);
    match policy {
        MutationPolicy::StrictLock => {
            let verb = if next_liked { "Like" } else { "Unlike" };
            let title = track.title.clone();
            let eid = sync_queue.push(StagedOp::SetLikeStatus {
                video_id: track.video_id,
                title: track.title,
                liked: next_liked,
            });
            *toast = Some(ToastState::staged(
                format!("Staged {verb} '{title}' in Sync (StrictLock)."),
                Some(eid),
            ));
        }
        MutationPolicy::QuickLike => {
            play_queue.set_liked(&track.video_id, next_liked);
            let _ = cmd_tx.send(WorkerCmd::SetLikeStatus {
                video_id: track.video_id,
                liked: next_liked,
            });
        }
    }
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(7, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use ytm_core::{MusicVideoType, TrackAvailability};

    use super::*;

    fn mk_track(vid: &str) -> Track {
        Track {
            video_id: vid.to_string(),
            set_video_id: None,
            position: 0,
            title: format!("Song {vid}"),
            artists: Vec::new(),
            album: None,
            duration_sec: Some(180),
            release_year: None,
            music_video_type: MusicVideoType::AudioTrack,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        }
    }

    #[test]
    fn strict_lock_stages_set_like_status_without_emitting_worker_cmd() {
        let (tx, rx) = mpsc::channel();
        let mut pq = PlayQueue::default();
        let mut sq = SyncQueue::default();
        let mut toast = None;

        handle_toggle_like(
            mk_track("V1"),
            MutationPolicy::StrictLock,
            &mut pq,
            &mut sq,
            &mut toast,
            &tx,
        );
        assert!(rx.try_recv().is_err());
        assert!(!pq.is_liked("V1"));
        assert_eq!(sq.len(), 1);
        assert!(matches!(
            sq.entries()[0].op,
            StagedOp::SetLikeStatus {
                ref video_id,
                liked: true,
                ..
            } if video_id == "V1"
        ));
    }

    #[test]
    fn quick_like_dispatches_worker_cmd_and_toggles_optimistically() {
        let (tx, rx) = mpsc::channel();
        let mut pq = PlayQueue::default();
        let mut sq = SyncQueue::default();
        let mut toast = None;

        handle_toggle_like(
            mk_track("V2"),
            MutationPolicy::QuickLike,
            &mut pq,
            &mut sq,
            &mut toast,
            &tx,
        );
        assert!(pq.is_liked("V2"));
        assert!(sq.is_empty());
        assert!(matches!(
            rx.try_recv(),
            Ok(WorkerCmd::SetLikeStatus {
                video_id,
                liked: true
            }) if video_id == "V2"
        ));
    }
}
