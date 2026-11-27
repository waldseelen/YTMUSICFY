use std::fs;
use std::path::Path;
use std::sync::mpsc::Sender;

use anyhow::{Context, Result};
use chrono::Utc;
use eframe::egui;
use ytm_core::{
    InnerTubeClient, InnerTubeClientTrait, Playlist, PlaylistFetchResult, ReqwestTransport,
};
use ytm_ops::{compute_restore_moves, plan_restore_diff};
use ytm_store::{
    AppPaths, CachedPlaylistRecord, ExportFormat, PersistedStagedOp, SnapshotStore, SqliteStore,
    export_playlist_to_path,
};

use crate::auth_store::{load_credentials_secure, write_mpv_cookie_jar};
use crate::state::{ConfirmedMutation, GuiAppConfig, MutationPolicy, WorkerEvent};
use crate::sync_queue::{StagedEntry, StagedOp};

#[path = "worker_sync.rs"]
mod worker_sync;

pub async fn load_playlist_with_manifest<C: InnerTubeClientTrait>(
    client: &C,
    id_or_lm: &str,
) -> Result<PlaylistFetchResult> {
    if id_or_lm.eq_ignore_ascii_case("LM") {
        Ok(client.get_liked_songs_with_manifest().await?)
    } else {
        Ok(client.get_playlist_with_manifest(id_or_lm).await?)
    }
}

pub async fn load_playlist<C: InnerTubeClientTrait>(
    client: &C,
    id_or_lm: &str,
) -> Result<Playlist> {
    Ok(load_playlist_with_manifest(client, id_or_lm)
        .await?
        .playlist)
}

pub(crate) fn sync_library_cache(store: &SqliteStore, list: &[ytm_core::PlaylistSummary]) {
    let _ = store.save_library_summaries(list);
    let keep: Vec<String> = list.iter().map(|s| s.id.clone()).collect();
    let _ = store.prune_missing_cached_playlists(&keep);
}

pub(crate) fn cache_fetched_playlist(
    store: &SqliteStore,
    fetched: &PlaylistFetchResult,
    tx: &Sender<WorkerEvent>,
) {
    let record = CachedPlaylistRecord {
        playlist: fetched.playlist.clone(),
        manifest: Some(fetched.manifest.clone()),
        fetched_at: Utc::now().timestamp(),
    };
    let _ = store.upsert_cached_playlist_record(&record);
    let _ = tx.send(WorkerEvent::PlaylistManifestLoaded {
        playlist_id: fetched.playlist.id.clone(),
        manifest: fetched.manifest.clone(),
    });
}

pub(crate) fn persist_staged_entries(store: &SqliteStore, entries: &[StagedEntry]) -> Result<()> {
    let now = Utc::now().timestamp();
    let rows: Vec<PersistedStagedOp> = entries
        .iter()
        .enumerate()
        .map(|(seq, entry)| {
            Ok(PersistedStagedOp {
                entry_id: entry.id,
                seq,
                selected: entry.selected,
                op_json: serde_json::to_string(&entry.op)?,
                created_at: now,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    store.replace_staged_ops(&rows)?;
    Ok(())
}

pub(crate) fn load_persisted_staged_entries(store: &SqliteStore) -> Result<Vec<StagedEntry>> {
    let rows = store.load_staged_ops()?;
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        if let Ok(op) = serde_json::from_str::<StagedOp>(&row.op_json) {
            entries.push(StagedEntry {
                id: row.entry_id,
                op,
                selected: row.selected,
            });
        }
    }
    Ok(entries)
}

pub(crate) fn load_config(paths: &AppPaths) -> GuiAppConfig {
    let path = paths.config_file();
    let mut cfg = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str::<GuiAppConfig>(&s).ok())
            .unwrap_or_default()
    } else {
        GuiAppConfig::default()
    };
    if cfg.enrichment.lastfm_api_key.is_none()
        && let Ok(val) = std::env::var("YTM_LASTFM_API_KEY")
        && !val.trim().is_empty()
    {
        cfg.enrichment.lastfm_api_key = Some(val.trim().to_string());
    }
    cfg
}

pub(crate) fn load_saved_client(
    paths: &AppPaths,
    cfg: &GuiAppConfig,
) -> Result<InnerTubeClient<ReqwestTransport>> {
    let creds = load_credentials_secure(paths)?;
    let _ = write_mpv_cookie_jar(paths, &creds);
    let transport = ReqwestTransport::new(cfg.innertube.max_retries)?;
    Ok(InnerTubeClient::new(
        transport,
        creds,
        cfg.innertube.clone(),
    ))
}

pub(crate) fn hydrate_gui_startup_state(
    store: &SqliteStore,
    policy: MutationPolicy,
    tx: &Sender<WorkerEvent>,
) {
    let _ = tx.send(WorkerEvent::MutationPolicyLoaded(policy));
    if let Ok(records) = store.load_all_cached_playlist_records() {
        for rec in records {
            if let Some(manifest) = rec.manifest {
                let _ = tx.send(WorkerEvent::PlaylistManifestLoaded {
                    playlist_id: rec.playlist.id,
                    manifest,
                });
            }
        }
    }
    if let Ok(entries) = load_persisted_staged_entries(store)
        && !entries.is_empty()
    {
        let _ = tx.send(WorkerEvent::StagedOpsLoaded(entries));
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
            worker_sync::execute_sync_queue(client, store, &entries, tx, ctx).await
        }
        ConfirmedMutation::DeletePlaylist {
            playlist_id,
            playlist_title,
        } => {
            let fetched = load_playlist_with_manifest(client, &playlist_id).await?;
            if fetched.manifest.completeness.is_mutation_locked() {
                anyhow::bail!(
                    "Refusing to delete '{}': fetch incomplete ({:?}).",
                    playlist_title,
                    fetched.manifest.completeness
                );
            }
            store
                .save_snapshot(&fetched.playlist, "pre-delete")
                .await
                .with_context(|| format!("mandatory pre-delete snapshot for '{playlist_id}'"))?;
            client.delete_playlist(&playlist_id).await?;
            let _ = store.delete_cached_playlist(&playlist_id);
            Ok((
                format!("Deleted playlist '{playlist_title}' ({playlist_id})"),
                vec!["__LIBRARY__".to_string()],
            ))
        }
    }
}

pub(crate) async fn execute_restore_snapshot<C: InnerTubeClientTrait>(
    client: &C,
    store: &SqliteStore,
    snapshot_id: &str,
) -> Result<(String, Vec<String>)> {
    let snap = store
        .load_snapshot(snapshot_id)
        .await
        .with_context(|| format!("loading snapshot {snapshot_id}"))?;
    let fetched = load_playlist_with_manifest(client, &snap.playlist.id).await?;
    if fetched.manifest.completeness.is_mutation_locked() {
        anyhow::bail!(
            "Refusing snapshot restore onto '{}': fetch incomplete ({:?}).",
            snap.playlist.title,
            fetched.manifest.completeness
        );
    }
    let live = fetched.playlist;
    let diff = plan_restore_diff(&live, &snap.playlist);

    store
        .save_snapshot(&live, &format!("pre-restore:{}", snap.metadata.id))
        .await
        .context("mandatory pre-restore snapshot")?;
    if !diff.to_remove.is_empty() {
        let rep = client
            .remove_playlist_items_report(&live.id, &diff.to_remove)
            .await?;
        if !rep.is_complete_success() {
            anyhow::bail!(
                "{}",
                rep.first_error()
                    .unwrap_or("Snapshot restore remove failed")
            );
        }
    }
    if !diff.to_add.is_empty() {
        let rep = client
            .add_playlist_items_report(&live.id, &diff.to_add, diff.allow_duplicates)
            .await?;
        if !rep.is_complete_success() {
            anyhow::bail!(
                "{}",
                rep.first_error().unwrap_or("Snapshot restore add failed")
            );
        }
    }

    let updated = load_playlist(client, &live.id).await?;
    let moves = compute_restore_moves(&updated, &snap.playlist);
    if !moves.is_empty() {
        let rep = client.move_playlist_items_report(&live.id, &moves).await?;
        if !rep.is_complete_success() {
            anyhow::bail!(
                "{}",
                rep.first_error()
                    .unwrap_or("Snapshot restore reorder failed")
            );
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

#[cfg(test)]
#[path = "worker_exec_artist_tests.rs"]
mod artist_tests;
#[cfg(test)]
#[path = "worker_exec_mock.rs"]
mod mock;
#[cfg(test)]
#[path = "worker_exec_tests.rs"]
mod tests;
