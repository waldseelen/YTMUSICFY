use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::str::FromStr;

use anyhow::{Result, bail};
use ytm_core::{InnerTubeClientTrait, MusicVideoType, MutationBatchReport, Track};
use ytm_enrich::MetadataProvider;
use ytm_ops::{
    DedupeConfig, HealthIssueKind, HealthReport, MatchMode, SetAlgebraOp, SortField, SortSpec,
    normalize_track, plan_dedupe, plan_set_op, plan_sort, scan_playlist_health,
};
use ytm_store::{EnrichmentCacheStore, SnapshotStore};

use crate::cli::{DedupeArgs, HealthArgs, SetArgs, SortArgs};
use crate::render::{
    make_progress_bar, render_dedupe_plan, render_diff_plan, render_fetch_manifest_summary,
    render_health_report, render_mutation_batch_report, render_sort_plan,
};
use crate::runner::{
    RuntimeContext, ensure_manifest_safe_for_execute, load_playlist_with_manifest,
};

fn emit_batch_report<W: Write>(
    out: &mut W,
    label: &str,
    report: &MutationBatchReport,
) -> Result<()> {
    writeln!(out, "{}", render_mutation_batch_report(label, report))?;
    if !report.is_complete_success() {
        bail!(
            "{label} batch incomplete ({}/{} chunks succeeded): {}",
            report.succeeded_chunks(),
            report.total_chunks,
            report.first_error().unwrap_or("unknown chunk error")
        );
    }
    Ok(())
}

pub async fn handle_dedupe<C, S, P, V, W>(
    ctx: &mut RuntimeContext<'_, C, S, P, V, W>,
    args: &DedupeArgs,
) -> Result<()>
where
    C: InnerTubeClientTrait,
    S: SnapshotStore + EnrichmentCacheStore,
    W: Write,
{
    let fetched = load_playlist_with_manifest(ctx.client, &args.playlist).await?;
    let playlist = fetched.playlist;
    writeln!(
        ctx.out,
        "{}",
        render_fetch_manifest_summary(&fetched.manifest)
    )?;

    let cfg = DedupeConfig {
        fuzzy_threshold: args.threshold,
        duration_tolerance_sec: args.duration_tol,
        ignore_duration: args.ignore_duration,
        keep: args.keep.into(),
        exact_only: args.exact_only,
    };
    let plan = plan_dedupe(&playlist, &cfg)?;
    writeln!(ctx.out, "{}", render_dedupe_plan(&plan, args.execute))?;

    if !args.execute {
        return Ok(());
    }
    ensure_manifest_safe_for_execute(&fetched.manifest, args.force_incomplete, "dedupe")?;
    if plan.total_removals == 0 {
        return Ok(());
    }

    ctx.store.save_snapshot(&playlist, "pre-dedupe").await?;
    let pairs: Vec<(String, String)> = plan
        .clusters
        .iter()
        .flat_map(|c| c.removals.iter())
        .filter_map(|t| t.set_video_id.clone().map(|sid| (t.video_id.clone(), sid)))
        .collect();
    let report = ctx
        .client
        .remove_playlist_items_report(&playlist.id, &pairs)
        .await?;
    emit_batch_report(ctx.out, "dedupe-remove", &report)?;
    writeln!(ctx.out, "Removed {} duplicate items.", pairs.len())?;
    Ok(())
}

pub async fn handle_sort<C, S, P, V, W>(
    ctx: &mut RuntimeContext<'_, C, S, P, V, W>,
    args: &SortArgs,
) -> Result<()>
where
    C: InnerTubeClientTrait,
    S: SnapshotStore + EnrichmentCacheStore,
    P: MetadataProvider,
    W: Write,
{
    let fetched = load_playlist_with_manifest(ctx.client, &args.playlist).await?;
    let playlist = fetched.playlist;
    writeln!(
        ctx.out,
        "{}",
        render_fetch_manifest_summary(&fetched.manifest)
    )?;

    let mut spec = SortSpec::from_str(&args.by)?;
    spec.seed = args.seed;

    let genre_map = if spec.keys.iter().any(|k| k.field == SortField::Genre) {
        let mut map = HashMap::with_capacity(playlist.tracks.len());
        let pb = make_progress_bar(playlist.tracks.len() as u64, "Enriching genres for sort");
        for track in &playlist.tracks {
            let enriched = ctx.enricher.enrich_track(track).await?;
            map.insert(track.video_id.clone(), enriched.primary_genre);
            pb.inc(1);
        }
        pb.finish_and_clear();
        Some(map)
    } else {
        None
    };

    let plan = plan_sort(&playlist, &spec, genre_map.as_ref())?;
    writeln!(ctx.out, "{}", render_sort_plan(&plan, args.execute))?;

    if !args.execute {
        return Ok(());
    }
    ensure_manifest_safe_for_execute(&fetched.manifest, args.force_incomplete, "sort")?;
    if plan.moves.is_empty() {
        return Ok(());
    }

    ctx.store.save_snapshot(&playlist, "pre-sort").await?;
    let report = ctx
        .client
        .move_playlist_items_report(&playlist.id, &plan.moves)
        .await?;
    emit_batch_report(ctx.out, "sort-move", &report)?;
    writeln!(
        ctx.out,
        "Applied {} LIS minimal moves across {} chunks.",
        plan.moves.len(),
        report.total_chunks
    )?;
    Ok(())
}

pub async fn handle_set<C, S, P, V, W>(
    ctx: &mut RuntimeContext<'_, C, S, P, V, W>,
    args: &SetArgs,
) -> Result<()>
where
    C: InnerTubeClientTrait,
    S: SnapshotStore + EnrichmentCacheStore,
    W: Write,
{
    let fetched_a = load_playlist_with_manifest(ctx.client, &args.playlist_a).await?;
    let fetched_b = load_playlist_with_manifest(ctx.client, &args.playlist_b).await?;
    let mode = if args.fuzzy {
        MatchMode::FuzzyNormalized
    } else {
        MatchMode::ExactVideoId
    };
    let op: SetAlgebraOp = args.op.into();
    let plan = plan_set_op(
        op,
        mode,
        &fetched_a.playlist,
        &fetched_b.playlist,
        args.into.clone(),
        0.90,
    )?;
    writeln!(ctx.out, "{}", render_diff_plan(&plan, args.execute))?;

    if !args.execute {
        return Ok(());
    }
    ensure_manifest_safe_for_execute(&fetched_a.manifest, args.force_incomplete, "set (A)")?;
    ensure_manifest_safe_for_execute(&fetched_b.manifest, args.force_incomplete, "set (B)")?;

    let Some(target_id) = args.into.as_deref() else {
        bail!("--into <playlist_id> is required when using --execute with `set`");
    };
    if plan.add_video_ids.is_empty() {
        writeln!(ctx.out, "No tracks to add into {target_id}.")?;
        return Ok(());
    }

    let target_fetched = load_playlist_with_manifest(ctx.client, target_id).await?;
    ensure_manifest_safe_for_execute(
        &target_fetched.manifest,
        args.force_incomplete,
        "set (target)",
    )?;
    let target_pl = target_fetched.playlist;
    ctx.store.save_snapshot(&target_pl, "pre-set").await?;
    let existing: HashSet<&str> = target_pl
        .tracks
        .iter()
        .map(|t| t.video_id.as_str())
        .collect();
    let missing_ids: Vec<String> = plan
        .add_video_ids
        .iter()
        .filter(|id| !existing.contains(id.as_str()))
        .cloned()
        .collect();

    if missing_ids.is_empty() {
        writeln!(
            ctx.out,
            "All {} result tracks already present in {target_id}.",
            plan.add_video_ids.len()
        )?;
        return Ok(());
    }

    let report = ctx
        .client
        .add_playlist_items_report(target_id, &missing_ids, false)
        .await?;
    emit_batch_report(ctx.out, "set-add", &report)?;
    writeln!(
        ctx.out,
        "Inserted {} tracks into {target_id}.",
        missing_ids.len()
    )?;
    Ok(())
}

async fn execute_health_mutations<C, S, P, V, W>(
    ctx: &mut RuntimeContext<'_, C, S, P, V, W>,
    playlist: &ytm_core::Playlist,
    report: &HealthReport,
    args: &HealthArgs,
) -> Result<()>
where
    C: InnerTubeClientTrait,
    S: SnapshotStore + EnrichmentCacheStore,
    W: Write,
{
    let add_ids: Vec<String> = if args.upgrade {
        report
            .upgrade_candidates
            .iter()
            .map(|c| c.official_atv.video_id.clone())
            .collect()
    } else {
        Vec::new()
    };

    let mut remove_pairs: Vec<(String, String)> = Vec::new();
    if args.prune_unavailable {
        for issue in &report.issues {
            let is_unavail = !matches!(issue.kind, HealthIssueKind::MissingSetVideoId);
            if is_unavail && let Some(sid) = &issue.track.set_video_id {
                remove_pairs.push((issue.track.video_id.clone(), sid.clone()));
            }
        }
    }

    if add_ids.is_empty() && remove_pairs.is_empty() {
        return Ok(());
    }

    ctx.store.save_snapshot(playlist, "pre-health").await?;
    let mut add_err: Option<anyhow::Error> = None;
    let mut confirmed_added = 0usize;

    if !add_ids.is_empty() {
        let add_report = ctx
            .client
            .add_playlist_items_report(&playlist.id, &add_ids, false)
            .await?;
        let mut confirmed_counts: HashMap<String, usize> = HashMap::new();
        for item in add_report.added_items() {
            *confirmed_counts.entry(item.video_id).or_insert(0) += 1;
        }
        for cand in &report.upgrade_candidates {
            let atv_id = &cand.official_atv.video_id;
            let confirmed = confirmed_counts.get_mut(atv_id).is_some_and(|c| {
                if *c > 0 {
                    *c -= 1;
                    true
                } else {
                    false
                }
            });
            if confirmed && let Some(sid) = &cand.source_track.set_video_id {
                remove_pairs.push((cand.source_track.video_id.clone(), sid.clone()));
                confirmed_added += 1;
            }
        }
        if let Err(err) = emit_batch_report(ctx.out, "health-add", &add_report) {
            add_err = Some(err);
        }
    }

    if !remove_pairs.is_empty() {
        let rem_report = ctx
            .client
            .remove_playlist_items_report(&playlist.id, &remove_pairs)
            .await?;
        emit_batch_report(ctx.out, "health-remove", &rem_report)?;
    }

    if let Some(err) = add_err {
        return Err(err);
    }

    writeln!(
        ctx.out,
        "Health execute complete for '{}': added {} ATVs, removed {} items.",
        playlist.title,
        confirmed_added,
        remove_pairs.len()
    )?;
    Ok(())
}

pub async fn handle_health<C, S, P, V, W>(
    ctx: &mut RuntimeContext<'_, C, S, P, V, W>,
    args: &HealthArgs,
) -> Result<()>
where
    C: InnerTubeClientTrait,
    S: SnapshotStore + EnrichmentCacheStore,
    W: Write,
{
    let targets = if args.target.eq_ignore_ascii_case("all") {
        ctx.client
            .get_library_playlists()
            .await?
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>()
    } else {
        vec![args.target.clone()]
    };

    for target_id in targets {
        let fetched = load_playlist_with_manifest(ctx.client, &target_id).await?;
        let playlist = fetched.playlist;
        writeln!(
            ctx.out,
            "{}",
            render_fetch_manifest_summary(&fetched.manifest)
        )?;

        let mut search_map: HashMap<String, Vec<Track>> = HashMap::new();
        if args.find_official || args.upgrade {
            for track in &playlist.tracks {
                if !matches!(
                    track.music_video_type,
                    MusicVideoType::UserGenerated | MusicVideoType::OfficialMusicVideo
                ) {
                    continue;
                }
                let norm = normalize_track(track);
                let query = format!("{} {}", norm.artist_norm, norm.title_norm);
                let found = ctx.client.search_tracks(query.trim(), 5).await?;
                search_map.insert(track.video_id.clone(), found);
            }
        }

        let report = scan_playlist_health(&playlist, &search_map, 0.88);
        writeln!(ctx.out, "{}", render_health_report(&report, args.execute))?;

        if !args.execute || (!args.prune_unavailable && !args.upgrade) {
            continue;
        }
        ensure_manifest_safe_for_execute(&fetched.manifest, args.force_incomplete, "health")?;
        execute_health_mutations(ctx, &playlist, &report, args).await?;
    }

    Ok(())
}
