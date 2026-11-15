use std::sync::mpsc::Sender;

use ytm_core::InnerTubeClientTrait;
use ytm_store::SnapshotStore;

use crate::worker::BackendState;
use crate::worker_msgs::{WorkerCmd, WorkerEvent};

/// Execute playlist metadata update, like toggle, browse, radio `next`, or lyrics fetch.
pub(crate) async fn handle_extended_cmd(
    state: &mut BackendState,
    cmd: WorkerCmd,
    tx: &Sender<WorkerEvent>,
) {
    let Some(client) = &state.client else {
        let _ = tx.send(WorkerEvent::StatusToast {
            ok: false,
            message: "Not authenticated: sign in under Auth tab first.".to_string(),
        });
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
        _ => {}
    }
}
