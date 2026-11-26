use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::time::Duration;

use eframe::egui;
use ytm_core::InnerTubeClientTrait;
use ytm_store::SnapshotStore;

use crate::worker::BackendState;
use crate::worker_exec::{cache_fetched_playlist, emit_progress, load_playlist_with_manifest};
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
            WorkerCmd::LoadBrowse { browse_id } => {
                let _ = tx.send(WorkerEvent::BrowseLoaded {
                    browse_id,
                    result: Err(msg),
                });
            }
            WorkerCmd::LoadLyrics { .. } => {
                let _ = tx.send(WorkerEvent::LyricsLoaded(Err(msg)));
            }
            WorkerCmd::LoadLyricsPayload { video_id, .. } => {
                let _ = tx.send(WorkerEvent::LyricsPayloadLoaded {
                    video_id,
                    result: Err(msg),
                });
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
            WorkerCmd::LoadBrowse { browse_id } => {
                let res = client
                    .get_browse_page(&browse_id)
                    .await
                    .map_err(|e| e.to_string());
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
                let res = client
                    .get_lyrics_payload(&browse_id)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::LyricsPayloadLoaded {
                    video_id,
                    result: res,
                });
            }
            _ => {}
        }
        ctx.request_repaint();
    });
}

/// Spawn a cancellable background repertoire indexing job (Lane 2).
pub(crate) fn spawn_index_all_job(
    state: &mut BackendState,
    missing_only: bool,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) {
    let Some(client) = state.client.as_ref().map(Arc::clone) else {
        let _ = tx.send(WorkerEvent::StatusToast {
            ok: false,
            message: "Not authenticated: sign in under Settings & Auth first.".to_string(),
        });
        return;
    };
    let (job_id, cancel) = state.allocate_job();
    let store = Arc::clone(&state.store);
    let tx = tx.clone();
    let ctx = ctx.clone();
    let _ = tx.send(WorkerEvent::JobStarted {
        job_id,
        label: "Index Repertoire".to_string(),
    });

    tokio::spawn(async move {
        let cached_ids: HashSet<String> = if missing_only {
            store
                .load_all_cached_playlists()
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.id)
                .collect()
        } else {
            HashSet::new()
        };
        let libs = match client.get_library_playlists().await {
            Ok(l) => l,
            Err(e) => {
                let _ = tx.send(WorkerEvent::StatusToast {
                    ok: false,
                    message: format!("Repertoire index failed: {e}"),
                });
                let _ = tx.send(WorkerEvent::JobEnded(job_id));
                ctx.request_repaint();
                return;
            }
        };
        let targets: Vec<_> = libs
            .into_iter()
            .filter(|p| !missing_only || !cached_ids.contains(&p.id))
            .collect();
        let total = targets.len();
        if total == 0 {
            let _ = tx.send(WorkerEvent::StatusToast {
                ok: true,
                message: "All playlists are already indexed locally.".to_string(),
            });
            let _ = tx.send(WorkerEvent::JobEnded(job_id));
            ctx.request_repaint();
            return;
        }
        let mut indexed = 0usize;
        for (idx, sum) in targets.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                emit_progress(&tx, &ctx, total, total, "Cancelled".to_string());
                let _ = tx.send(WorkerEvent::StatusToast {
                    ok: true,
                    message: format!("Indexing cancelled after {indexed}/{total} playlist(s)."),
                });
                let _ = tx.send(WorkerEvent::JobEnded(job_id));
                ctx.request_repaint();
                return;
            }
            emit_progress(
                &tx,
                &ctx,
                idx,
                total,
                format!("Indexing {}/{total}: {}", idx + 1, sum.title),
            );
            let result = match load_playlist_with_manifest(client.as_ref(), &sum.id).await {
                Ok(fetched) => {
                    cache_fetched_playlist(&store, &fetched, &tx);
                    indexed += 1;
                    Ok(fetched.playlist)
                }
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(WorkerEvent::PlaylistLoaded {
                playlist_id: sum.id.clone(),
                result,
            });
            if idx + 1 < total {
                tokio::time::sleep(Duration::from_millis(350)).await;
            }
        }
        emit_progress(&tx, &ctx, total, total, "Indexing complete".to_string());
        let _ = tx.send(WorkerEvent::StatusToast {
            ok: true,
            message: format!("Indexed {indexed}/{total} playlist(s) locally for instant search."),
        });
        let _ = tx.send(WorkerEvent::JobEnded(job_id));
        ctx.request_repaint();
    });
}
