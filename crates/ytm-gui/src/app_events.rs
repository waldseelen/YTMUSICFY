use crate::app::YtmGuiApp;
use crate::i18n::{Key, tr};
use crate::state::{AsyncState, DispatchEntryOutcome, GuiTab, WorkerCmd, WorkerEvent};

pub(crate) use crate::app_frame::finish_frame;

/// Apply a single background worker event onto `YtmGuiApp` state.
pub(crate) fn apply_worker_event(app: &mut YtmGuiApp, evt: WorkerEvent) {
    match evt {
        WorkerEvent::SessionLoaded(res) => {
            let failed = res.is_err();
            if !failed {
                app.offline_mode = false;
                app.session_refresh.finish_ok();
            }
            app.session = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            if failed && app.library.data().is_none() && app.playlist_cache.is_empty() {
                app.active_tab = GuiTab::Auth;
            }
        }
        WorkerEvent::LibraryCacheHydrated(list) => {
            if !matches!(app.library, AsyncState::Ready(_)) && !list.is_empty() {
                app.library = AsyncState::Ready(list);
            }
        }
        WorkerEvent::LibraryLoaded(res) => {
            if let Ok(mut list) = res {
                app.offline_mode = false;
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
                if app.library.data().is_some() {
                    app.offline_mode = true;
                    let now = app.now_ms();
                    let msg = tr(app.lang(), Key::OfflineUsingCache).to_string();
                    app.status_bus.push_hint(msg, now);
                } else {
                    app.library = AsyncState::Failed(e);
                }
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
                    app.draft_cache.invalidate_playlist(&pl.id);
                    app.playlist_cache.insert(pl.id.clone(), pl);
                }
                Err(e) => {
                    if !app.playlist_cache.contains_key(&playlist_id) {
                        app.draft_cache.invalidate_playlist(&playlist_id);
                    }
                    app.playlist_errors.insert(playlist_id, e);
                }
            }
        }
        WorkerEvent::SearchResults(res) => {
            app.search_results = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
        }
        WorkerEvent::SearchSuggestionsLoaded { req_id, result } => {
            if let Ok(items) = result {
                let now = app.now_ms();
                app.search_controller.apply_suggestions(req_id, items, now);
            }
        }
        WorkerEvent::InstantLiveLoaded { req_id, result } => {
            if let Ok(page) = result {
                app.search_controller.apply_live_results(req_id, page);
            }
        }
        WorkerEvent::SessionRefreshProgress(st) => {
            app.session_refresh = st;
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
            if !ok && app.session_refresh.is_busy() {
                app.session_refresh.finish_err(&message);
            }
            let now = app.now_ms();
            app.status_bus.push_info(ok, message, now);
        }
        WorkerEvent::SyncEntryFinished { entry_id, outcome } => {
            if matches!(outcome, DispatchEntryOutcome::Succeeded { .. }) {
                app.sync_queue.remove_entries(&[entry_id]);
                app.draft_cache.invalidate_all();
            }
        }
        WorkerEvent::LikeStatusResult {
            video_id,
            liked,
            result,
        } => match result {
            Ok(()) => {
                for lm_id in ["LM", "VLLM"] {
                    if let Some(lm) = app.playlist_cache.get_mut(lm_id) {
                        if !liked {
                            lm.tracks.retain(|t| t.video_id != video_id);
                            lm.track_count = lm.tracks.len();
                        }
                        app.draft_cache.invalidate_playlist(lm_id);
                    }
                }
                let msg = if liked {
                    "Added track to Liked Songs."
                } else {
                    "Removed track from Liked Songs."
                };
                let now = app.now_ms();
                app.status_bus.push_info(true, msg, now);
            }
            Err(err) => {
                app.play_queue.set_liked(&video_id, !liked);
                let now = app.now_ms();
                app.status_bus
                    .push_info(false, format!("Like update failed: {err}"), now);
            }
        },
        WorkerEvent::MutationFinished {
            ok,
            message,
            reload_ids,
        } => {
            app.batch_progress = None;
            let now = app.now_ms();
            app.status_bus.push_info(ok, message, now);
            if ok {
                for id in reload_ids {
                    if id == "__LIBRARY__" {
                        app.draft_cache.invalidate_all();
                        let _ = app.cmd_tx.send(WorkerCmd::RefreshLibrary);
                    } else {
                        app.playlist_cache.remove(&id);
                        app.draft_cache.invalidate_playlist(&id);
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
                    let now = app.now_ms();
                    app.status_bus.push_info(
                        true,
                        format!("Radio seeded with {cnt} track(s)."),
                        now,
                    );
                }
                // Lyrics are fetched lazily by `views::queue_lyrics` when visible (no wasted POSTs).
                app.lyrics_video_id = None;
                app.lyrics_payload = AsyncState::Idle;
            } else if seed_radio && let Err(err) = result {
                let now = app.now_ms();
                app.status_bus
                    .push_info(false, format!("Radio load failed: {err}"), now);
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
                if pl.id.eq_ignore_ascii_case("LM") {
                    app.play_queue.seed_liked_from_tracks(&pl.tracks);
                }
                if let AsyncState::Ready(list) = &mut app.library
                    && let Some(sum) = list.iter_mut().find(|s| s.id == pl.id)
                    && sum.track_count.is_none()
                {
                    sum.track_count = Some(pl.tracks.len());
                }
                app.playlist_cache.entry(pl.id.clone()).or_insert(pl);
            }
            app.draft_cache.invalidate_all();
        }
        WorkerEvent::PlaylistManifestLoaded {
            playlist_id,
            manifest,
        } => {
            app.playlist_manifests.insert(playlist_id, manifest);
        }
        WorkerEvent::StagedOpsLoaded(entries) => {
            app.sync_queue.load_entries(entries);
            app.mark_staged_persisted();
            app.draft_cache.invalidate_all();
        }
        WorkerEvent::MutationPolicyLoaded(policy) => {
            app.mutation_policy = policy;
        }
        WorkerEvent::JobStarted { job_id, label } => {
            app.active_job_id = Some(job_id);
            app.batch_progress = Some((0, 1, label));
        }
        WorkerEvent::JobEnded(job_id) => {
            if app.active_job_id == Some(job_id) {
                app.active_job_id = None;
                app.batch_progress = None;
            }
        }
        WorkerEvent::CatalogSearchResults { append, result, .. } => match result {
            Ok(page) => {
                if append && let AsyncState::Ready(existing) = &mut app.catalog_results {
                    existing.tracks.extend(page.tracks);
                    existing.cards.extend(page.cards);
                    existing.continuation = page.continuation;
                } else {
                    app.catalog_results = AsyncState::Ready(page);
                }
            }
            Err(e) => {
                app.catalog_results = AsyncState::Failed(e);
            }
        },
        WorkerEvent::BrowseContinuationLoaded { browse_id, result } => match result {
            Ok(more) => {
                if let AsyncState::Ready(page) = &mut app.browse_state
                    && page.browse_id == browse_id
                {
                    page.sections.extend(more.sections);
                    page.tracks.extend(more.tracks);
                    page.continuation = more.continuation;
                }
            }
            Err(e) => {
                let now = app.now_ms();
                app.status_bus
                    .push_info(false, format!("Browse load more failed: {e}"), now);
            }
        },
        WorkerEvent::LyricsPayloadLoaded { video_id, result } => {
            let cur_vid = app.play_queue.current_track().map(|t| t.video_id.as_str());
            if cur_vid == Some(video_id.as_str()) {
                app.lyrics_video_id = Some(video_id);
                app.lyrics_payload = result.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
        }
        WorkerEvent::PlaybackSessionLoaded(raw_json) => {
            if let Ok(snap) =
                serde_json::from_str::<crate::play_queue::PlayQueueSnapshot>(&raw_json)
            {
                app.play_queue.restore_from_snapshot(&snap);
                app.player.set_volume(snap.volume);
            }
        }
        WorkerEvent::GuiConfigLoaded(cfg) => {
            app.mutation_policy = cfg.mutation_policy;
            app.audio_quality = cfg.audio_quality;
            app.search_controller.load_recent(&cfg.recent_searches);
            app.gui_config = cfg;
        }
        WorkerEvent::SystemThemeChanged(dark) => {
            app.theme_state.os_dark = dark;
        }
    }
}
