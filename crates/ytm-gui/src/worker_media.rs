use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::time::Duration;

use eframe::egui;
use ytm_core::InnerTubeClientTrait;
use ytm_store::SnapshotStore;

use crate::worker::BackendState;
use crate::worker_exec::{emit_progress, load_playlist};
use crate::worker_msgs::{WorkerCmd, WorkerEvent};

/// Execute playlist metadata update, like toggle, browse, radio `next`, lyrics, or repertoire indexing.
pub(crate) async fn handle_extended_cmd(
    state: &mut BackendState,
    cmd: WorkerCmd,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) {
    let Some(client) = &state.client else {
        let msg = "Not authenticated: sign in under Auth tab first.".to_string();
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
            _ => {
                let _ = tx.send(WorkerEvent::StatusToast {
                    ok: false,
                    message: msg,
                });
            }
        }
        return;
    };

    match cmd {
        WorkerCmd::UpdatePlaylistMeta {
            playlist_id,
            title,
            description,
            privacy,
        } => {
            if let Ok(live) = client.get_playlist(&playlist_id).await {
                let _ = state.store.save_snapshot(&live, "pre-meta").await;
            }
            let event = match client
                .update_playlist_metadata(
                    &playlist_id,
                    title.as_deref(),
                    description.as_deref(),
                    privacy,
                )
                .await
            {
                Ok(()) => WorkerEvent::MutationFinished {
                    ok: true,
                    message: "Playlist details updated.".to_string(),
                    reload_ids: vec!["__LIBRARY__".to_string(), playlist_id],
                },
                Err(e) => WorkerEvent::MutationFinished {
                    ok: false,
                    message: format!("Metadata update failed: {e}"),
                    reload_ids: Vec::new(),
                },
            };
            let _ = tx.send(event);
        }
        WorkerCmd::SetLikeStatus { video_id, liked } => {
            let res = client.set_like_status(&video_id, liked).await;
            let event = match res {
                Ok(()) => WorkerEvent::StatusToast {
                    ok: true,
                    message: if liked {
                        "Added track to Liked Songs.".to_string()
                    } else {
                        "Removed track from Liked Songs.".to_string()
                    },
                },
                Err(e) => WorkerEvent::StatusToast {
                    ok: false,
                    message: format!("Like update failed: {e}"),
                },
            };
            let _ = tx.send(event);
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
        WorkerCmd::IndexAllPlaylists { missing_only } => {
            let cached_ids: HashSet<String> = if missing_only {
                state
                    .store
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
                return;
            }
            let mut indexed = 0usize;
            for (idx, sum) in targets.iter().enumerate() {
                emit_progress(
                    tx,
                    ctx,
                    idx,
                    total,
                    format!("Indexing {}/{total}: {}", idx + 1, sum.title),
                );
                let result = load_playlist(client, &sum.id)
                    .await
                    .map_err(|e| e.to_string());
                if let Ok(pl) = &result {
                    let _ = state.store.upsert_cached_playlist(pl);
                    indexed += 1;
                }
                let _ = tx.send(WorkerEvent::PlaylistLoaded {
                    playlist_id: sum.id.clone(),
                    result,
                });
                if idx + 1 < total {
                    tokio::time::sleep(Duration::from_millis(350)).await;
                }
            }
            emit_progress(tx, ctx, total, total, "Indexing complete".to_string());
            let _ = tx.send(WorkerEvent::StatusToast {
                ok: true,
                message: format!(
                    "Indexed {indexed}/{total} playlist(s) locally for instant search."
                ),
            });
        }
        _ => {}
    }
}
