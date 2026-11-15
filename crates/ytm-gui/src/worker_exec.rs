use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use anyhow::{Context, Result};
use ytm_core::{
    InnerTubeClient, InnerTubeClientTrait, Playlist, PlaylistPrivacy, ReqwestTransport,
    parse_auth_input,
};
use ytm_ops::{HealthIssueKind, compute_minimal_moves};
use ytm_store::{
    AppPaths, ExportFormat, SnapshotStore, SqliteStore, export_playlist_to_path, write_secret_file,
};

use crate::state::{ConfirmedMutation, GuiAppConfig};

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

pub async fn run_confirmed_mutation<C: InnerTubeClientTrait>(
    client: &C,
    store: &SqliteStore,
    mutation: ConfirmedMutation,
) -> Result<(String, Option<String>)> {
    match mutation {
        ConfirmedMutation::Dedupe(plan) => {
            let live = load_playlist(client, &plan.playlist_id).await?;
            store.save_snapshot(&live, "pre-dedupe").await?;
            let pairs: Vec<(String, String)> = plan
                .clusters
                .iter()
                .flat_map(|c| c.removals.iter())
                .filter_map(|t| t.set_video_id.clone().map(|sid| (t.video_id.clone(), sid)))
                .collect();
            if !pairs.is_empty() {
                client.remove_playlist_items(&live.id, &pairs).await?;
            }
            Ok((
                format!(
                    "Removed {} duplicates from '{}'.",
                    pairs.len(),
                    plan.playlist_title
                ),
                Some(plan.playlist_id),
            ))
        }
        ConfirmedMutation::Sort(plan) => {
            let live = load_playlist(client, &plan.playlist_id).await?;
            store.save_snapshot(&live, "pre-sort").await?;
            if !plan.moves.is_empty() {
                client.move_playlist_items(&live.id, &plan.moves).await?;
            }
            Ok((
                format!(
                    "Applied {} LIS moves on '{}'.",
                    plan.moves.len(),
                    plan.playlist_title
                ),
                Some(plan.playlist_id),
            ))
        }
        ConfirmedMutation::SetInsert {
            target_id,
            video_ids,
        } => {
            let target_pl = load_playlist(client, &target_id).await?;
            store.save_snapshot(&target_pl, "pre-set").await?;
            let existing: HashSet<&str> = target_pl
                .tracks
                .iter()
                .map(|t| t.video_id.as_str())
                .collect();
            let missing: Vec<String> = video_ids
                .into_iter()
                .filter(|id| !existing.contains(id.as_str()))
                .collect();
            let count = missing.len();
            if !missing.is_empty() {
                client
                    .add_playlist_items(&target_id, &missing, false)
                    .await?;
            }
            Ok((
                format!("Inserted {count} tracks into '{target_id}'."),
                Some(target_id),
            ))
        }
        ConfirmedMutation::Health {
            playlist,
            prune_unavailable,
            upgrade,
            report,
        } => {
            let add_ids: Vec<String> = if upgrade {
                report
                    .upgrade_candidates
                    .iter()
                    .map(|c| c.official_atv.video_id.clone())
                    .collect()
            } else {
                Vec::new()
            };
            let mut remove_pairs: Vec<(String, String)> = Vec::new();
            if prune_unavailable {
                for issue in &report.issues {
                    if !matches!(issue.kind, HealthIssueKind::MissingSetVideoId)
                        && let Some(sid) = &issue.track.set_video_id
                    {
                        remove_pairs.push((issue.track.video_id.clone(), sid.clone()));
                    }
                }
            }
            if upgrade {
                for cand in &report.upgrade_candidates {
                    if let Some(sid) = &cand.source_track.set_video_id {
                        remove_pairs.push((cand.source_track.video_id.clone(), sid.clone()));
                    }
                }
            }
            if !add_ids.is_empty() || !remove_pairs.is_empty() {
                store.save_snapshot(&playlist, "pre-health").await?;
                if !add_ids.is_empty() {
                    client
                        .add_playlist_items(&playlist.id, &add_ids, false)
                        .await?;
                }
                if !remove_pairs.is_empty() {
                    client
                        .remove_playlist_items(&playlist.id, &remove_pairs)
                        .await?;
                }
            }
            Ok((
                format!(
                    "Health applied on '{}': +{} ATV, -{} removed.",
                    playlist.title,
                    add_ids.len(),
                    remove_pairs.len()
                ),
                Some(playlist.id),
            ))
        }
        ConfirmedMutation::GenreSplit {
            source_playlist,
            plan,
        } => {
            let mut snapshotted_source = false;
            let mut created = 0usize;
            let mut updated = 0usize;
            for bucket in &plan.buckets {
                if bucket.tracks_to_add.is_empty() {
                    continue;
                }
                if !snapshotted_source {
                    store
                        .save_snapshot(&source_playlist, "pre-split-genre")
                        .await?;
                    snapshotted_source = true;
                }
                let ids: Vec<String> = bucket
                    .tracks_to_add
                    .iter()
                    .map(|t| t.video_id.clone())
                    .collect();
                if let Some(existing_id) = &bucket.existing_playlist_id {
                    let target_pl = load_playlist(client, existing_id).await?;
                    store
                        .save_snapshot(&target_pl, "pre-split-genre-target")
                        .await?;
                    client.add_playlist_items(existing_id, &ids, false).await?;
                    updated += 1;
                } else {
                    client
                        .create_playlist(
                            &bucket.target_playlist_title,
                            "Generated by ytm-gui split-genre",
                            PlaylistPrivacy::Private,
                            &ids,
                        )
                        .await?;
                    created += 1;
                }
            }
            Ok((
                format!("Genre split complete: created {created}, updated {updated} playlists."),
                Some(source_playlist.id),
            ))
        }
        ConfirmedMutation::RestoreSnapshot { snapshot_id } => {
            execute_restore_snapshot(client, store, &snapshot_id).await
        }
    }
}

async fn execute_restore_snapshot<C: InnerTubeClientTrait>(
    client: &C,
    store: &SqliteStore,
    snapshot_id: &str,
) -> Result<(String, Option<String>)> {
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
        Some(live.id),
    ))
}

pub async fn import_auth_inner(
    paths: &AppPaths,
    cfg: &GuiAppConfig,
    raw: &str,
) -> Result<(InnerTubeClient<ReqwestTransport>, ytm_core::SessionStatus)> {
    let creds = parse_auth_input(raw)?;
    let transport = ReqwestTransport::new(cfg.innertube.max_retries)?;
    let client = InnerTubeClient::new(transport, creds.clone(), cfg.innertube.clone());
    let status = client.verify_session().await?;
    if !status.authenticated {
        anyhow::bail!("session canary returned unauthenticated");
    }
    paths.ensure_dirs()?;
    write_secret_file(&paths.auth_file(), &serde_json::to_vec_pretty(&creds)?)?;
    Ok((client, status))
}

pub async fn backup_inner(
    client: &InnerTubeClient<ReqwestTransport>,
    store: &SqliteStore,
    target: &str,
    format: ExportFormat,
    out_dir: &Path,
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
    for id in &ids {
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
