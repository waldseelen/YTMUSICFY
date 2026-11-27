use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

use anyhow::Result;
use eframe::egui;
use ytm_core::{InnerTubeClientTrait, PlaylistFetchResult};
use ytm_ops::compute_minimal_moves;
use ytm_store::{SnapshotStore, SqliteStore};

use crate::state::{DispatchEntryOutcome, WorkerEvent};
use crate::sync_queue::{StagedEntry, StagedOp};
use crate::worker_exec::{
    cache_fetched_playlist, emit_progress, execute_restore_snapshot, load_playlist_with_manifest,
    persist_staged_entries,
};

#[path = "worker_sync_ops.rs"]
pub(super) mod ops;

pub(crate) async fn execute_sync_queue<C: InnerTubeClientTrait>(
    client: &C,
    store: &SqliteStore,
    entries: &[StagedEntry],
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) -> Result<(String, Vec<String>)> {
    let total = entries.len().max(1);
    let mut snapshotted: HashSet<String> = HashSet::new();
    let mut touched_ids: Vec<String> = Vec::new();
    let mut completed_ids: HashSet<u64> = HashSet::new();
    let mut created_any = false;

    for (idx, entry) in entries.iter().enumerate() {
        emit_progress(
            tx,
            ctx,
            idx,
            total,
            format!("Sync {}/{total}: {}", idx + 1, entry.op.summary()),
        );
        let outcome = execute_single_entry(
            client,
            store,
            entry,
            &mut snapshotted,
            &mut touched_ids,
            &mut created_any,
            tx,
        )
        .await;
        let _ = tx.send(WorkerEvent::SyncEntryFinished {
            entry_id: entry.id,
            outcome: outcome.clone(),
        });

        match outcome {
            DispatchEntryOutcome::Succeeded { .. } => {
                completed_ids.insert(entry.id);
                let remaining: Vec<StagedEntry> = entries
                    .iter()
                    .filter(|e| !completed_ids.contains(&e.id))
                    .cloned()
                    .collect();
                let _ = persist_staged_entries(store, &remaining);
            }
            DispatchEntryOutcome::AbortedDrift { reason, .. } => {
                anyhow::bail!("{reason}");
            }
            DispatchEntryOutcome::Failed { error, .. } => {
                anyhow::bail!("{error}");
            }
        }
    }

    if created_any && !touched_ids.iter().any(|id| id == "__LIBRARY__") {
        touched_ids.push("__LIBRARY__".to_string());
    }
    emit_progress(tx, ctx, total, total, "Sync complete".to_string());
    Ok((
        format!(
            "Synced {} queued action(s) to YouTube Music.",
            completed_ids.len()
        ),
        touched_ids,
    ))
}

fn preflight_pids(op: &StagedOp) -> Vec<&str> {
    match op {
        StagedOp::MoveTracks {
            source_playlist_id,
            target_playlist_id,
            ..
        } => {
            if source_playlist_id == target_playlist_id {
                vec![source_playlist_id.as_str()]
            } else {
                vec![source_playlist_id.as_str(), target_playlist_id.as_str()]
            }
        }
        StagedOp::CreatePlaylist { .. } | StagedOp::SetLikeStatus { .. } => Vec::new(),
        other => other.target_playlist_id().into_iter().collect(),
    }
}

async fn execute_single_entry<C: InnerTubeClientTrait>(
    client: &C,
    store: &SqliteStore,
    entry: &StagedEntry,
    snapshotted: &mut HashSet<String>,
    touched_ids: &mut Vec<String>,
    created_any: &mut bool,
    tx: &Sender<WorkerEvent>,
) -> DispatchEntryOutcome {
    let pids = preflight_pids(&entry.op);
    let mut live_by_id: HashMap<String, PlaylistFetchResult> = HashMap::new();

    for pid in &pids {
        let fetched = match load_playlist_with_manifest(client, pid).await {
            Ok(f) => f,
            Err(e) => {
                return ops::failed_outcome(
                    entry.id,
                    0,
                    1,
                    format!("Pre-flight fetch '{pid}': {e}"),
                );
            }
        };
        if fetched.manifest.completeness.is_mutation_locked() {
            return DispatchEntryOutcome::AbortedDrift {
                entry_id: entry.id,
                reason: format!(
                    "Playlist '{pid}' fetch is incomplete ({:?}); mutation locked.",
                    fetched.manifest.completeness
                ),
            };
        }
        if !snapshotted.contains(*pid) {
            if let Err(e) = store.save_snapshot(&fetched.playlist, "pre-sync").await {
                return ops::failed_outcome(
                    entry.id,
                    0,
                    1,
                    format!("Mandatory pre-sync snapshot failed for '{pid}': {e}"),
                );
            }
            snapshotted.insert((*pid).to_string());
        }
        if !touched_ids.iter().any(|id| id == pid) {
            touched_ids.push((*pid).to_string());
        }
        live_by_id.insert((*pid).to_string(), fetched);
    }

    let exec_res = dispatch_op(
        client,
        store,
        &entry.op,
        &live_by_id,
        touched_ids,
        created_any,
        tx,
    )
    .await;
    let (chunks_sent, confirmed_items) = match exec_res {
        Ok(counts) => counts,
        Err(DispatchStop::Drift(reason)) => {
            return DispatchEntryOutcome::AbortedDrift {
                entry_id: entry.id,
                reason,
            };
        }
        Err(DispatchStop::Fail(completed_chunks, total_chunks, error)) => {
            return DispatchEntryOutcome::Failed {
                entry_id: entry.id,
                completed_chunks,
                total_chunks,
                error,
            };
        }
    };

    let mut readback_verified = true;
    if !matches!(entry.op, StagedOp::DeletePlaylist { .. }) {
        for pid in &pids {
            match load_playlist_with_manifest(client, pid).await {
                Ok(after) => {
                    cache_fetched_playlist(store, &after, tx);
                    let _ = tx.send(WorkerEvent::PlaylistLoaded {
                        playlist_id: (*pid).to_string(),
                        result: Ok(after.playlist),
                    });
                }
                Err(_) => readback_verified = false,
            }
        }
    }

    DispatchEntryOutcome::Succeeded {
        entry_id: entry.id,
        chunks_sent,
        confirmed_items,
        readback_verified,
    }
}

enum DispatchStop {
    Drift(String),
    Fail(usize, usize, String),
}

async fn dispatch_op<C: InnerTubeClientTrait>(
    client: &C,
    store: &SqliteStore,
    op: &StagedOp,
    live_by_id: &HashMap<String, PlaylistFetchResult>,
    touched_ids: &mut Vec<String>,
    created_any: &mut bool,
    tx: &Sender<WorkerEvent>,
) -> Result<(usize, usize), DispatchStop> {
    match op {
        StagedOp::Reorder {
            playlist_id,
            target_set_video_ids,
            ..
        } => {
            let live = &live_by_id[playlist_id].playlist;
            let live_sids: Vec<String> = live
                .tracks
                .iter()
                .filter_map(|t| t.set_video_id.clone())
                .collect();
            if !ops::same_multiset(&live_sids, target_set_video_ids)
                || live_sids.len() != live.tracks.len()
            {
                return Err(DispatchStop::Drift(format!(
                    "Playlist '{playlist_id}' changed remotely since staging; re-stage Sort."
                )));
            }
            let (_, moves) = compute_minimal_moves(&live_sids, target_set_video_ids);
            if moves.is_empty() {
                Ok((0, 0))
            } else {
                ops::report_result(client.move_playlist_items_report(playlist_id, &moves).await)
                    .map_err(|(c, t, e)| DispatchStop::Fail(c, t, e))
            }
        }
        StagedOp::AddTracks {
            playlist_id,
            tracks,
            allow_duplicates,
            ..
        } => {
            let ids =
                ops::collect_add_ids(tracks, &live_by_id[playlist_id].playlist, *allow_duplicates);
            ops::add_tracks_resilient(client, playlist_id, &ids, *allow_duplicates, tx)
                .await
                .map_err(|(c, t, e)| DispatchStop::Fail(c, t, e))
        }
        StagedOp::RemoveTracks {
            playlist_id,
            tracks,
            ..
        } => {
            let pairs = ops::resolve_remove_pairs(tracks, &live_by_id[playlist_id].playlist);
            if pairs.is_empty() {
                Ok((0, 0))
            } else {
                ops::report_result(
                    client
                        .remove_playlist_items_report(playlist_id, &pairs)
                        .await,
                )
                .map_err(|(c, t, e)| DispatchStop::Fail(c, t, e))
            }
        }
        StagedOp::MoveTracks {
            source_playlist_id,
            target_playlist_id,
            tracks,
            ..
        } => {
            let add_ids: Vec<String> = tracks
                .iter()
                .filter(|t| !t.video_id.is_empty())
                .map(|t| t.video_id.clone())
                .collect();
            let remove_pairs =
                ops::resolve_remove_pairs(tracks, &live_by_id[source_playlist_id].playlist);
            ops::execute_move_tracks(
                client,
                source_playlist_id,
                target_playlist_id,
                &add_ids,
                &remove_pairs,
            )
            .await
            .map_err(|(c, t, e)| DispatchStop::Fail(c, t, e))
        }
        StagedOp::CreatePlaylist {
            title,
            description,
            privacy,
            tracks,
        } => {
            let ids: Vec<String> = tracks
                .iter()
                .filter(|t| !t.video_id.is_empty())
                .map(|t| t.video_id.clone())
                .collect();
            client
                .create_playlist(title, description, *privacy, &ids)
                .await
                .map(|_| {
                    *created_any = true;
                    (1, ids.len())
                })
                .map_err(|e| DispatchStop::Fail(0, 1, e.to_string()))
        }
        StagedOp::UpdatePlaylistMeta {
            playlist_id,
            title,
            description,
            privacy,
            ..
        } => client
            .update_playlist_metadata(
                playlist_id,
                title.as_deref(),
                description.as_deref(),
                *privacy,
            )
            .await
            .map(|()| {
                *created_any = true;
                (1, 1)
            })
            .map_err(|e| DispatchStop::Fail(0, 1, e.to_string())),
        StagedOp::DeletePlaylist { playlist_id, .. } => client
            .delete_playlist(playlist_id)
            .await
            .map(|()| {
                let _ = store.delete_cached_playlist(playlist_id);
                *created_any = true;
                (1, 1)
            })
            .map_err(|e| DispatchStop::Fail(0, 1, e.to_string())),
        StagedOp::RestoreSnapshot { snapshot_id, .. } => {
            execute_restore_snapshot(client, store, snapshot_id)
                .await
                .map(|_| (1, 1))
                .map_err(|e| DispatchStop::Fail(0, 1, e.to_string()))
        }
        StagedOp::SetLikeStatus {
            video_id, liked, ..
        } => client
            .set_like_status(video_id, *liked)
            .await
            .map(|()| {
                if !touched_ids.iter().any(|id| id == "LM") {
                    touched_ids.push("LM".to_string());
                }
                (1, 1)
            })
            .map_err(|e| DispatchStop::Fail(0, 1, e.to_string())),
        StagedOp::UpgradeTracks {
            playlist_id,
            upgrades,
            preserve_position,
            ..
        } => ops::execute_upgrade_tracks(
            client,
            playlist_id,
            upgrades,
            *preserve_position,
            &live_by_id[playlist_id].playlist,
        )
        .await
        .map_err(|(c, t, e)| DispatchStop::Fail(c, t, e)),
    }
}
