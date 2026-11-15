use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::mpsc::Sender;

use anyhow::{Context, Result};
use eframe::egui;
use ytm_core::{InnerTubeClient, InnerTubeClientTrait, Playlist, ReqwestTransport};
use ytm_ops::compute_minimal_moves;
use ytm_store::{ExportFormat, SnapshotStore, SqliteStore, export_playlist_to_path};

use crate::state::{ConfirmedMutation, WorkerEvent};
use crate::sync_queue::{StagedEntry, StagedOp};

pub async fn load_playlist<C: InnerTubeClientTrait>(
    client: &C,
    id_or_lm: &str,
) -> Result<Playlist> {
    if id_or_lm.eq_ignore_ascii_case("LM") {
        Ok(client.get_liked_songs().await?)
    } else {
        Ok(client.get_playlist(id_or_lm).await?)
    }
}

pub fn emit_progress(
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
    done: usize,
    total: usize,
    message: impl Into<String>,
) {
    let _ = tx.send(WorkerEvent::Progress {
        done,
        total,
        message: message.into(),
    });
    ctx.request_repaint();
}

pub async fn run_confirmed_mutation<C: InnerTubeClientTrait>(
    client: &C,
    store: &SqliteStore,
    mutation: ConfirmedMutation,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) -> Result<(String, Vec<String>)> {
    match mutation {
        ConfirmedMutation::RestoreSnapshot { snapshot_id } => {
            execute_restore_snapshot(client, store, &snapshot_id).await
        }
        ConfirmedMutation::ExecuteSync { entries } => {
            execute_sync_queue(client, store, &entries, tx, ctx).await
        }
        ConfirmedMutation::DeletePlaylist {
            playlist_id,
            playlist_title,
        } => {
            if let Ok(live) = load_playlist(client, &playlist_id).await {
                let _ = store.save_snapshot(&live, "pre-delete").await;
            }
            client.delete_playlist(&playlist_id).await?;
            Ok((
                format!("Deleted playlist '{playlist_title}' ({playlist_id})"),
                vec!["__LIBRARY__".to_string()],
            ))
        }
    }
}

async fn execute_sync_queue<C: InnerTubeClientTrait>(
    client: &C,
    store: &SqliteStore,
    entries: &[StagedEntry],
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) -> Result<(String, Vec<String>)> {
    let total = entries.len().max(1);
    let mut snapshotted: HashSet<String> = HashSet::new();
    let mut touched_ids: Vec<String> = Vec::new();
    let mut created_any = false;

    for (idx, entry) in entries.iter().enumerate() {
        emit_progress(
            tx,
            ctx,
            idx,
            total,
            format!("Sync {}/{total}: {}", idx + 1, entry.op.summary()),
        );
        if let Some(pid) = entry.op.target_playlist_id()
            && !snapshotted.contains(pid)
        {
            let live = load_playlist(client, pid).await?;
            store.save_snapshot(&live, "pre-sync").await?;
            snapshotted.insert(pid.to_string());
            touched_ids.push(pid.to_string());
        }

        match &entry.op {
            StagedOp::Reorder {
                playlist_id, moves, ..
            } => {
                if !moves.is_empty() {
                    client.move_playlist_items(playlist_id, moves).await?;
                }
            }
            StagedOp::AddTracks {
                playlist_id,
                tracks,
                allow_duplicates,
                ..
            } => {
                let ids: Vec<String> = tracks
                    .iter()
                    .filter(|t| !t.video_id.is_empty())
                    .map(|t| t.video_id.clone())
                    .collect();
                if !ids.is_empty() {
                    client
                        .add_playlist_items(playlist_id, &ids, *allow_duplicates)
                        .await?;
                }
            }
            StagedOp::RemoveTracks {
                playlist_id,
                tracks,
                ..
            } => {
                let pairs: Vec<(String, String)> = tracks
                    .iter()
                    .filter_map(|t| t.set_video_id.clone().map(|sid| (t.video_id.clone(), sid)))
                    .collect();
                if !pairs.is_empty() {
                    client.remove_playlist_items(playlist_id, &pairs).await?;
                }
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
                    .await?;
                created_any = true;
            }
            StagedOp::UpgradeTracks {
                playlist_id,
                upgrades,
                ..
            } => {
                let add_ids: Vec<String> = upgrades
                    .iter()
                    .map(|(_, atv)| atv.video_id.clone())
                    .collect();
                let remove_pairs: Vec<(String, String)> = upgrades
                    .iter()
                    .filter_map(|(old, _)| {
                        old.set_video_id
                            .clone()
                            .map(|sid| (old.video_id.clone(), sid))
                    })
                    .collect();
                if !add_ids.is_empty() {
                    client
                        .add_playlist_items(playlist_id, &add_ids, false)
                        .await?;
                }
                if !remove_pairs.is_empty() {
                    client
                        .remove_playlist_items(playlist_id, &remove_pairs)
                        .await?;
                }
            }
        }
    }

    if created_any {
        touched_ids.push("__LIBRARY__".to_string());
    }
    emit_progress(tx, ctx, total, total, "Sync complete".to_string());
    Ok((
        format!(
            "Synced {} queued action(s) to YouTube Music.",
            entries.len()
        ),
        touched_ids,
    ))
}

async fn execute_restore_snapshot<C: InnerTubeClientTrait>(
    client: &C,
    store: &SqliteStore,
    snapshot_id: &str,
) -> Result<(String, Vec<String>)> {
    let snap = store
        .load_snapshot(snapshot_id)
        .await
        .with_context(|| format!("loading snapshot {snapshot_id}"))?;
    let live = load_playlist(client, &snap.playlist.id).await?;

    let mut desired_counts: HashMap<&str, usize> = HashMap::new();
    for t in &snap.playlist.tracks {
        if !t.video_id.is_empty() {
            *desired_counts.entry(t.video_id.as_str()).or_insert(0) += 1;
        }
    }

    let mut kept_counts: HashMap<String, usize> = HashMap::new();
    let mut to_remove: Vec<(String, String)> = Vec::new();
    for t in &live.tracks {
        let want = desired_counts
            .get(t.video_id.as_str())
            .copied()
            .unwrap_or(0);
        let seen = kept_counts.entry(t.video_id.clone()).or_insert(0);
        if *seen < want {
            *seen += 1;
        } else if let Some(sid) = &t.set_video_id {
            to_remove.push((t.video_id.clone(), sid.clone()));
        }
    }

    let mut remaining_kept = kept_counts;
    let mut to_add: Vec<String> = Vec::new();
    for t in &snap.playlist.tracks {
        if t.video_id.is_empty() {
            continue;
        }
        let rem = remaining_kept.get_mut(&t.video_id).map(|c| {
            if *c > 0 {
                *c -= 1;
                true
            } else {
                false
            }
        });
        if rem != Some(true) {
            to_add.push(t.video_id.clone());
        }
    }

    store
        .save_snapshot(&live, &format!("pre-restore:{}", snap.metadata.id))
        .await?;
    if !to_remove.is_empty() {
        client.remove_playlist_items(&live.id, &to_remove).await?;
    }
    if !to_add.is_empty() {
        let allow_dupes = desired_counts.values().any(|&cnt| cnt > 1);
        client
            .add_playlist_items(&live.id, &to_add, allow_dupes)
            .await?;
    }

    let updated = load_playlist(client, &live.id).await?;
    let mut set_queues: HashMap<&str, VecDeque<String>> = HashMap::new();
    let mut current_set_ids = Vec::new();
    for t in &updated.tracks {
        if let Some(sid) = &t.set_video_id {
            current_set_ids.push(sid.clone());
            set_queues
                .entry(t.video_id.as_str())
                .or_default()
                .push_back(sid.clone());
        }
    }
    let mut target_set_ids = Vec::new();
    for t in &snap.playlist.tracks {
        if let Some(sid) = set_queues
            .get_mut(t.video_id.as_str())
            .and_then(VecDeque::pop_front)
        {
            target_set_ids.push(sid);
        }
    }
    if current_set_ids.len() == target_set_ids.len() && !target_set_ids.is_empty() {
        let (_, moves) = compute_minimal_moves(&current_set_ids, &target_set_ids);
        if !moves.is_empty() {
            client.move_playlist_items(&live.id, &moves).await?;
        }
    }

    Ok((
        format!(
            "Restored snapshot '{}' onto '{}'.",
            snap.metadata.id, snap.playlist.title
        ),
        vec![live.id],
    ))
}

pub async fn backup_inner(
    client: &InnerTubeClient<ReqwestTransport>,
    store: &SqliteStore,
    target: &str,
    format: ExportFormat,
    out_dir: &Path,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) -> Result<String> {
    let ids = if target.eq_ignore_ascii_case("all") {
        client
            .get_library_playlists()
            .await?
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>()
    } else {
        vec![target.to_string()]
    };
    let ext = match format {
        ExportFormat::Json => "json",
        ExportFormat::Csv => "csv",
        ExportFormat::M3u8 => "m3u8",
    };
    let total = ids.len().max(1);
    for (idx, id) in ids.iter().enumerate() {
        emit_progress(
            tx,
            ctx,
            idx,
            total,
            format!("Backing up {}/{total}: {id}", idx + 1),
        );
        let pl = load_playlist(client, id).await?;
        store.save_snapshot(&pl, "backup").await?;
        let safe: String = pl
            .id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        export_playlist_to_path(&pl, &out_dir.join(format!("{safe}.{ext}")), format)?;
    }
    Ok(format!(
        "Backed up {} playlist(s) to {}",
        ids.len(),
        out_dir.display()
    ))
}
