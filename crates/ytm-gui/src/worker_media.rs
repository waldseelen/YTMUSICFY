use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::InnerTubeClientTrait;
use ytm_store::{ExportFormat, SnapshotStore};

use crate::worker::BackendState;
use crate::worker_exec::backup_inner;
use crate::worker_msgs::{WorkerCmd, WorkerEvent};

/// Spawn non-blocking interactive commands (Search, Browse, WatchNext, Lyrics, Snapshots, Like).
pub(crate) fn handle_interactive_cmd(
    state: &mut BackendState,
    cmd: WorkerCmd,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) {
    match cmd {
        WorkerCmd::RefreshSnapshots => {
            let store = Arc::clone(&state.store);
            let tx = tx.clone();
            let ctx = ctx.clone();
            tokio::spawn(async move {
                let res = store.list_snapshots(None).await.map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::SnapshotsLoaded(res));
                ctx.request_repaint();
            });
            return;
        }
        WorkerCmd::LoadSnapshot(id) => {
            let store = Arc::clone(&state.store);
            let tx = tx.clone();
            let ctx = ctx.clone();
            tokio::spawn(async move {
                let res = store.load_snapshot(&id).await.map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::SnapshotDetailLoaded(res));
                ctx.request_repaint();
            });
            return;
        }
        _ => {}
    }

    let Some(client) = state.client.as_ref().map(Arc::clone) else {
        let msg = "Not authenticated: sign in under Settings & Auth first.".to_string();
        match cmd {
            WorkerCmd::LoadBrowse {
                browse_id,
                params: _,
            } => {
                if let Ok(Some(cached)) = state.store.load_cached_browse_page(&browse_id) {
                    let _ = tx.send(WorkerEvent::BrowseCacheHydrated(cached));
                }
                let _ = tx.send(WorkerEvent::BrowseLoaded {
                    browse_id,
                    result: Err(msg),
                });
            }
            WorkerCmd::LoadLyrics { .. } => {
                let _ = tx.send(WorkerEvent::LyricsLoaded(Err(msg)));
            }
            WorkerCmd::LoadLyricsPayload { .. } => {
                if let WorkerCmd::LoadLyricsPayload { video_id, .. } = cmd {
                    if let Ok(Some(cached)) = state.store.load_cached_lyrics(&video_id) {
                        let _ = tx.send(WorkerEvent::LyricsPayloadLoaded {
                            video_id,
                            result: Ok(Some(cached)),
                        });
                    } else {
                        let _ = tx.send(WorkerEvent::LyricsPayloadLoaded {
                            video_id,
                            result: Err(msg),
                        });
                    }
                }
            }
            WorkerCmd::SetLikeStatus { video_id, liked } => {
                let _ = tx.send(WorkerEvent::LikeStatusResult {
                    video_id,
                    liked,
                    result: Err(msg),
                });
            }
            _ => {
                let _ = tx.send(WorkerEvent::StatusToast {
                    ok: false,
                    message: msg,
                });
            }
        }
        return;
    };

    let store = Arc::clone(&state.store);
    let tx = tx.clone();
    let ctx = ctx.clone();
    tokio::spawn(async move {
        match cmd {
            WorkerCmd::SearchTracks(query) => {
                let res = client
                    .search_tracks(query.trim(), 8)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::SearchResults(res));
            }
            WorkerCmd::FetchSearchSuggestions { req_id, query } => {
                let res = client
                    .get_search_suggestions(query.trim())
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::SearchSuggestionsLoaded {
                    req_id,
                    result: res,
                });
            }
            WorkerCmd::SearchInstantLive { req_id, query } => {
                let res = client
                    .search_catalog(query.trim(), ytm_core::SearchFilter::Top, None)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::InstantLiveLoaded {
                    req_id,
                    result: res,
                });
            }
            WorkerCmd::SearchCatalog {
                query,
                filter,
                continuation,
            } => {
                let append = continuation.is_some();
                let res = client
                    .search_catalog(query.trim(), filter, continuation.as_deref())
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::CatalogSearchResults {
                    query,
                    filter,
                    append,
                    result: res,
                });
            }
            WorkerCmd::SetLikeStatus { video_id, liked } => {
                let res = client
                    .set_like_status(&video_id, liked)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::LikeStatusResult {
                    video_id,
                    liked,
                    result: res,
                });
            }
            WorkerCmd::LoadBrowse { browse_id, params } => {
                if params.is_none()
                    && let Ok(Some(cached)) = store.load_cached_browse_page(&browse_id)
                {
                    let _ = tx.send(WorkerEvent::BrowseCacheHydrated(cached));
                    ctx.request_repaint();
                }
                let res = client
                    .get_browse_page_with_params(&browse_id, params.as_deref())
                    .await
                    .map_err(|e| e.to_string());
                if params.is_none()
                    && let Ok(page) = &res
                {
                    let _ = store.save_cached_browse_page(page);
                }
                let _ = tx.send(WorkerEvent::BrowseLoaded {
                    browse_id,
                    result: res,
                });
            }
            WorkerCmd::LoadBrowseContinuation { browse_id, token } => {
                let res = client
                    .get_browse_continuation(&token)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::BrowseContinuationLoaded {
                    browse_id,
                    result: res,
                });
            }
            WorkerCmd::LoadWatchNext {
                video_id,
                playlist_id,
                seed_radio,
            } => {
                let res = client
                    .get_watch_next(&video_id, playlist_id.as_deref())
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::WatchNextLoaded {
                    seed_radio,
                    result: res,
                });
            }
            WorkerCmd::LoadLyrics { browse_id } => {
                let res = client
                    .get_lyrics(&browse_id)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::LyricsLoaded(res));
            }
            WorkerCmd::LoadLyricsPayload {
                video_id,
                browse_id,
            } => {
                if let Ok(Some(cached)) = store.load_cached_lyrics(&video_id) {
                    let _ = tx.send(WorkerEvent::LyricsPayloadLoaded {
                        video_id: video_id.clone(),
                        result: Ok(Some(cached)),
                    });
                    ctx.request_repaint();
                }
                let res = client
                    .get_lyrics_payload(&browse_id)
                    .await
                    .map_err(|e| e.to_string());
                if let Ok(Some(payload)) = &res {
                    let _ = store.save_cached_lyrics(&video_id, Some(&browse_id), payload);
                }
                if res.is_ok() || store.load_cached_lyrics(&video_id).ok().flatten().is_none() {
                    let _ = tx.send(WorkerEvent::LyricsPayloadLoaded {
                        video_id,
                        result: res,
                    });
                }
            }
            _ => {}
        }
        ctx.request_repaint();
    });
}

/// Spawn a cancellable background backup job (Lane 2).
pub(crate) fn spawn_backup_job(
    state: &mut BackendState,
    target: String,
    format: ExportFormat,
    out_dir: PathBuf,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) {
    let Some(client) = state.client.as_ref().map(Arc::clone) else {
        return;
    };
    let (job_id, _cancel) = state.allocate_job();
    let store = Arc::clone(&state.store);
    let tx = tx.clone();
    let ctx = ctx.clone();
    let _ = tx.send(WorkerEvent::JobStarted {
        job_id,
        label: format!("Backup {target}"),
    });
    tokio::spawn(async move {
        let msg = backup_inner(
            client.as_ref(),
            &store,
            &target,
            format,
            &out_dir,
            &tx,
            &ctx,
        )
        .await;
        let _ = tx.send(match msg {
            Ok(m) => WorkerEvent::StatusToast {
                ok: true,
                message: m,
            },
            Err(e) => WorkerEvent::StatusToast {
                ok: false,
                message: e.to_string(),
            },
        });
        if let Ok(snaps) = store.list_snapshots(None).await {
            let _ = tx.send(WorkerEvent::SnapshotsLoaded(Ok(snaps)));
        }
        let _ = tx.send(WorkerEvent::JobEnded(job_id));
        ctx.request_repaint();
    });
}
