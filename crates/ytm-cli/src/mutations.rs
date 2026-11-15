use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::str::FromStr;

use anyhow::{Result, bail};
use ytm_core::{InnerTubeClientTrait, Track};
use ytm_enrich::MetadataProvider;
use ytm_ops::{
    DedupeConfig, MatchMode, SetAlgebraOp, SortField, SortSpec, normalize_track, plan_dedupe,
    plan_set_op, plan_sort, scan_playlist_health,
};
use ytm_store::{EnrichmentCacheStore, SnapshotStore};

use crate::cli::{DedupeArgs, HealthArgs, SetArgs, SortArgs};
use crate::render::{
    make_progress_bar, render_dedupe_plan, render_diff_plan, render_health_report, render_sort_plan,
};
use crate::runner::{RuntimeContext, load_playlist_by_id};

pub async fn handle_dedupe<C, S, P, V, W>(
    ctx: &mut RuntimeContext<'_, C, S, P, V, W>,
    args: &DedupeArgs,
) -> Result<()>
where
    C: InnerTubeClientTrait,
    S: SnapshotStore + EnrichmentCacheStore,
    W: Write,
{
    let playlist = load_playlist_by_id(ctx.client, &args.playlist).await?;
    let cfg = DedupeConfig {
        fuzzy_threshold: args.threshold,
        duration_tolerance_sec: args.duration_tol,
        ignore_duration: args.ignore_duration,
        keep: args.keep.into(),
        exact_only: args.exact_only,
    };
    let plan = plan_dedupe(&playlist, &cfg)?;
    writeln!(ctx.out, "{}", render_dedupe_plan(&plan, args.execute))?;

    if !args.execute || plan.total_removals == 0 {
        return Ok(());
    }

    ctx.store.save_snapshot(&playlist, "pre-dedupe").await?;
    let pairs: Vec<(String, String)> = plan
        .clusters
        .iter()
        .flat_map(|c| c.removals.iter())
        .filter_map(|t| t.set_video_id.clone().map(|sid| (t.video_id.clone(), sid)))
        .collect();
    let res = ctx
        .client
        .remove_playlist_items(&playlist.id, &pairs)
        .await?;
    writeln!(
        ctx.out,
        "Removed {} duplicate items (status: {}).",
        pairs.len(),
        res.status
    )?;
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
    let playlist = load_playlist_by_id(ctx.client, &args.playlist).await?;
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

    if !args.execute || plan.moves.is_empty() {
        return Ok(());
    }

    ctx.store.save_snapshot(&playlist, "pre-sort").await?;
    let results = ctx
        .client
        .move_playlist_items(&playlist.id, &plan.moves)
        .await?;
    writeln!(
        ctx.out,
        "Applied {} LIS minimal moves across {} edit responses.",
        plan.moves.len(),
        results.len()
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
    let pl_a = load_playlist_by_id(ctx.client, &args.playlist_a).await?;
    let pl_b = load_playlist_by_id(ctx.client, &args.playlist_b).await?;
    let mode = if args.fuzzy {
        MatchMode::FuzzyNormalized
    } else {
        MatchMode::ExactVideoId
    };
    let op: SetAlgebraOp = args.op.into();
    let plan = plan_set_op(op, mode, &pl_a, &pl_b, args.into.clone(), 0.90)?;
    writeln!(ctx.out, "{}", render_diff_plan(&plan, args.execute))?;

    if !args.execute {
        return Ok(());
    }

    let Some(target_id) = args.into.as_deref() else {
        bail!("--into <playlist_id> is required when using --execute with `set`");
    };
    if plan.add_video_ids.is_empty() {
        writeln!(ctx.out, "No tracks to add into {target_id}.")?;
        return Ok(());
    }

    let target_pl = load_playlist_by_id(ctx.client, target_id).await?;
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

    let res = ctx
        .client
        .add_playlist_items(target_id, &missing_ids, false)
        .await?;
    writeln!(
        ctx.out,
        "Inserted {} tracks into {} (status: {}).",
        missing_ids.len(),
        target_id,
        res.status
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
        let playlist = load_playlist_by_id(ctx.client, &target_id).await?;
        let mut search_map: HashMap<String, Vec<Track>> = HashMap::new();

        if args.find_official || args.upgrade {
            for track in &playlist.tracks {
                if !matches!(
                    track.music_video_type,
                    ytm_core::MusicVideoType::UserGenerated
                        | ytm_core::MusicVideoType::OfficialMusicVideo
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
                let is_unavail = !matches!(issue.kind, ytm_ops::HealthIssueKind::MissingSetVideoId);
                if is_unavail && let Some(sid) = &issue.track.set_video_id {
                    remove_pairs.push((issue.track.video_id.clone(), sid.clone()));
                }
            }
        }
        if args.upgrade {
            for cand in &report.upgrade_candidates {
                if let Some(sid) = &cand.source_track.set_video_id {
                    remove_pairs.push((cand.source_track.video_id.clone(), sid.clone()));
                }
            }
        }

        if add_ids.is_empty() && remove_pairs.is_empty() {
            continue;
        }

        ctx.store.save_snapshot(&playlist, "pre-health").await?;
        if !add_ids.is_empty() {
            ctx.client
                .add_playlist_items(&playlist.id, &add_ids, false)
                .await?;
        }
        if !remove_pairs.is_empty() {
            ctx.client
                .remove_playlist_items(&playlist.id, &remove_pairs)
                .await?;
        }
        writeln!(
            ctx.out,
            "Health execute complete for '{}': added {} ATVs, removed {} items.",
            playlist.title,
            add_ids.len(),
            remove_pairs.len()
        )?;
    }

    Ok(())
}
