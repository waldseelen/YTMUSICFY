use crate::app::YtmGuiApp;
use crate::navigation::ToastState;
use crate::state::{AsyncState, GuiTab, WorkerCmd, WorkerEvent};

/// Apply a single background worker event onto `YtmGuiApp` state.
pub(crate) fn apply_worker_event(app: &mut YtmGuiApp, evt: WorkerEvent) {
    match evt {
        WorkerEvent::SessionLoaded(res) => {
            let failed = res.is_err();
            app.session = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            if failed {
                app.active_tab = GuiTab::Auth;
            }
        }
        WorkerEvent::LibraryLoaded(res) => {
            if let Ok(mut list) = res {
                for s in &mut list {
                    if s.track_count.is_none()
                        && let Some(cached) = app.playlist_cache.get(&s.id)
                    {
                        s.track_count = Some(cached.tracks.len());
                    }
                }
                if let Some(sel) = &app.selected_playlist_id
                    && !sel.eq_ignore_ascii_case("LM")
                    && !list.iter().any(|s| &s.id == sel)
                {
                    app.selected_playlist_id = None;
                }
                app.library = AsyncState::Ready(list);
            } else if let Err(e) = res {
                app.library = AsyncState::Failed(e);
            }
        }
        WorkerEvent::PlaylistLoaded {
            playlist_id,
            result,
        } => {
            app.loading_playlists.remove(&playlist_id);
            match result {
                Ok(pl) => {
                    app.playlist_errors.remove(&playlist_id);
                    if pl.id.eq_ignore_ascii_case("LM") {
                        app.play_queue.seed_liked_from_tracks(&pl.tracks);
                    }
                    if let AsyncState::Ready(list) = &mut app.library
                        && let Some(sum) = list.iter_mut().find(|s| s.id == pl.id)
                    {
                        sum.track_count = Some(pl.tracks.len());
                    }
                    app.playlist_cache.insert(pl.id.clone(), pl);
                }
                Err(e) => {
                    app.playlist_errors.insert(playlist_id, e);
                }
            }
        }
        WorkerEvent::SearchResults(res) => {
            app.search_results = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
        }
        WorkerEvent::Progress {
            done,
            total,
            message,
        } => {
            app.batch_progress = if done >= total {
                None
            } else {
                Some((done, total, message))
            };
        }
        WorkerEvent::DedupePlanned(res) => {
            app.dedupe_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
        }
        WorkerEvent::SortPlanned(res) => {
            app.batch_progress = None;
            app.sort_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
        }
        WorkerEvent::SetOpPlanned(res) => {
            app.set_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
        }
        WorkerEvent::HealthScanned(res) => {
            app.batch_progress = None;
            app.health_report = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
        }
        WorkerEvent::GenreSplitPlanned(res) => {
            app.batch_progress = None;
            app.genre_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
        }
        WorkerEvent::SnapshotsLoaded(res) => {
            app.snapshots = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
        }
        WorkerEvent::SnapshotDetailLoaded(res) => {
            app.snapshot_detail = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
        }
        WorkerEvent::StatusToast { ok, message } => {
            app.batch_progress = None;
            app.toast = Some(ToastState::info(ok, message));
        }
        WorkerEvent::MutationFinished {
            ok,
            message,
            reload_ids,
        } => {
            app.batch_progress = None;
            app.toast = Some(ToastState::info(ok, message));
            if ok {
                app.sync_queue.clear();
                for id in reload_ids {
                    if id == "__LIBRARY__" {
                        let _ = app.cmd_tx.send(WorkerCmd::RefreshLibrary);
                    } else {
                        app.playlist_cache.remove(&id);
                        if app.loading_playlists.insert(id.clone()) {
                            let _ = app.cmd_tx.send(WorkerCmd::LoadPlaylist(id));
                        }
                    }
                }
            }
        }
        WorkerEvent::BrowseLoaded { browse_id, result } => {
            if browse_id == app.browse_target || matches!(app.browse_state, AsyncState::Loading(_))
            {
                app.browse_state = result.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
        }
        WorkerEvent::WatchNextLoaded { seed_radio, result } => {
            if let Ok(wn) = result {
                app.lyrics_browse_id = wn.lyrics_browse_id.clone();
                if seed_radio && !wn.tracks.is_empty() {
                    let cnt = wn.tracks.len();
                    app.play_queue.replace_upcoming(wn.tracks);
                    app.toast = Some(ToastState::info(
                        true,
                        format!("Radio seeded with {cnt} track(s)."),
                    ));
                }
                if let Some(lbid) = wn.lyrics_browse_id {
                    app.lyrics_state = AsyncState::Loading("Loading lyrics...".to_string());
                    let _ = app.cmd_tx.send(WorkerCmd::LoadLyrics { browse_id: lbid });
                }
            } else if seed_radio && let Err(err) = result {
                app.toast = Some(ToastState::info(false, format!("Radio load failed: {err}")));
            }
        }
        WorkerEvent::LyricsLoaded(res) => {
            app.lyrics_state = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
        }
        WorkerEvent::AudioConfigUpdated {
            cookie_jar,
            quality,
            preferred_browser,
        } => {
            app.cookie_bridge_active = cookie_jar.is_some();
            app.audio_quality = quality;
            if let Some(browser) = preferred_browser {
                app.auth_browser = browser;
            }
            app.player.configure(cookie_jar, quality);
        }
        WorkerEvent::PlaylistCacheHydrated(cached) => {
            for pl in cached {
                if let AsyncState::Ready(list) = &mut app.library
                    && let Some(sum) = list.iter_mut().find(|s| s.id == pl.id)
                    && sum.track_count.is_none()
                {
                    sum.track_count = Some(pl.tracks.len());
                }
                app.playlist_cache.entry(pl.id.clone()).or_insert(pl);
            }
        }
    }
}
