use std::fmt::Write as _;

use comfy_table::{Cell, ContentArrangement, Table, presets::UTF8_FULL};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use ytm_core::{FetchCompleteness, FetchManifest, MutationBatchReport, Playlist, PlaylistSummary};
use ytm_enrich::GenreSplitPlan;
use ytm_ops::{DedupePlan, DiffPlan, HealthReport, SortPlan};
use ytm_store::{Snapshot, SnapshotMetadata};

/// Creates an `indicatif` progress bar bound to stderr when interactive.
#[must_use]
pub fn make_progress_bar(len: u64, message: &str) -> ProgressBar {
    let pb = if len == 0 {
        ProgressBar::new_spinner()
    } else {
        ProgressBar::new(len)
    };
    pb.set_draw_target(ProgressDrawTarget::stderr());
    let style = ProgressStyle::with_template("{spinner:.cyan} [{pos}/{len}] {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_bar());
    pb.set_style(style);
    pb.set_message(message.to_string());
    pb
}

fn new_table(headers: &[&str]) -> Table {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(headers.iter().copied());
    table
}

/// Renders a concise single-line [`FetchManifest`] summary (`declared vs fetched vs unavailable vs dropped`).
#[must_use]
pub fn render_fetch_manifest_summary(manifest: &FetchManifest) -> String {
    let declared = manifest
        .declared_track_count
        .map_or_else(|| "?".to_string(), |n| n.to_string());
    let (dropped, status) = match &manifest.completeness {
        FetchCompleteness::Complete => (0usize, "Complete".to_string()),
        FetchCompleteness::PartialHidden {
            hidden_estimate, ..
        } => (0usize, format!("PartialHidden(hidden~{hidden_estimate})")),
        FetchCompleteness::Truncated {
            pages_fetched,
            max_pages,
        } => (
            0usize,
            format!("Truncated({pages_fetched}/{max_pages} pages)"),
        ),
        FetchCompleteness::Degraded { dropped_renderers } => (
            dropped_renderers.len(),
            format!("Degraded({})", dropped_renderers.join(",")),
        ),
    };
    format!(
        "Manifest: declared={declared} fetched={} unavailable={} dropped={dropped} status={status}",
        manifest.fetched_tracks, manifest.unavailable_tracks
    )
}

/// Renders per-chunk stats for a [`MutationBatchReport`].
#[must_use]
pub fn render_mutation_batch_report(label: &str, report: &MutationBatchReport) -> String {
    let err_suffix = report
        .first_error()
        .map_or_else(String::new, |e| format!(" | first_error={e}"));
    format!(
        "Batch[{label}]: chunks_succeeded={}/{} failed={} confirmed_added={}{err_suffix}",
        report.succeeded_chunks(),
        report.total_chunks,
        report.failed_chunks(),
        report.added_items().len(),
    )
}

/// Renders library playlists overview table.
#[must_use]
pub fn render_playlists_table(playlists: &[PlaylistSummary]) -> String {
    let mut table = new_table(&["ID", "Title", "Tracks", "Owned", "Liked"]);
    for pl in playlists {
        let count = pl
            .track_count
            .map_or_else(|| "?".to_string(), |c| c.to_string());
        table.add_row(vec![
            Cell::new(&pl.id),
            Cell::new(&pl.title),
            Cell::new(count),
            Cell::new(if pl.owned { "yes" } else { "no" }),
            Cell::new(if pl.is_liked_music { "LM" } else { "" }),
        ]);
    }
    table.to_string()
}

/// Renders detailed playlist tracks, type, and playability table.
#[must_use]
pub fn render_inspect_table(playlist: &Playlist) -> String {
    let mut out = format!(
        "Playlist: {} ({}) | owned={} | tracks={}\n",
        playlist.title,
        playlist.id,
        playlist.owned,
        playlist.tracks.len()
    );
    let mut table = new_table(&[
        "#", "VideoID", "Artist", "Title", "Dur(s)", "Type", "Status",
    ]);
    for track in &playlist.tracks {
        let artist = track.primary_artist_name().unwrap_or("-");
        let dur = track
            .duration_sec
            .map_or_else(|| "-".to_string(), |s| s.to_string());
        let status = if track.availability.is_available() {
            "OK".to_string()
        } else {
            format!("{:?}", track.availability)
        };
        table.add_row(vec![
            Cell::new(track.position + 1),
            Cell::new(&track.video_id),
            Cell::new(artist),
            Cell::new(&track.title),
            Cell::new(dur),
            Cell::new(track.music_video_type.short_label()),
            Cell::new(status),
        ]);
    }
    let _ = writeln!(out, "{table}");
    out
}

/// Renders saved pre-mutation snapshot metadata table.
#[must_use]
pub fn render_snapshots_table(snapshots: &[SnapshotMetadata]) -> String {
    let mut table = new_table(&[
        "Snapshot ID",
        "Playlist ID",
        "Title",
        "Reason",
        "Items",
        "Created (UTC)",
    ]);
    for snap in snapshots {
        table.add_row(vec![
            Cell::new(&snap.id),
            Cell::new(&snap.playlist_id),
            Cell::new(&snap.playlist_title),
            Cell::new(&snap.reason),
            Cell::new(snap.item_count),
            Cell::new(snap.created_at.format("%Y-%m-%d %H:%M:%S")),
        ]);
    }
    table.to_string()
}

/// Renders detailed track contents of a single snapshot.
#[must_use]
pub fn render_snapshot_detail(snapshot: &Snapshot) -> String {
    let mut out = format!(
        "Snapshot {} | created={} | reason={}\n",
        snapshot.metadata.id,
        snapshot.metadata.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
        snapshot.metadata.reason
    );
    out.push_str(&render_inspect_table(&snapshot.playlist));
    out
}

/// Renders a deduplication cluster preview or execution diff.
#[must_use]
pub fn render_dedupe_plan(plan: &DedupePlan, execute: bool) -> String {
    let mode = if execute { "EXECUTE" } else { "DRY-RUN" };
    let mut out = format!(
        "[{mode}] Dedupe '{}' ({}): {} clusters, {} removals, {} skipped (missing setVideoId)\n",
        plan.playlist_title,
        plan.playlist_id,
        plan.clusters.len(),
        plan.total_removals,
        plan.skipped_without_set_id
    );
    if plan.clusters.is_empty() {
        return out;
    }
    let mut table = new_table(&["Action", "VideoID", "Artist", "Title", "Type", "Reason"]);
    for cluster in &plan.clusters {
        table.add_row(vec![
            Cell::new("KEEP"),
            Cell::new(&cluster.keeper.video_id),
            Cell::new(cluster.keeper.primary_artist_name().unwrap_or("-")),
            Cell::new(&cluster.keeper.title),
            Cell::new(cluster.keeper.music_video_type.short_label()),
            Cell::new(&cluster.reason),
        ]);
        for rem in &cluster.removals {
            table.add_row(vec![
                Cell::new("REMOVE"),
                Cell::new(&rem.video_id),
                Cell::new(rem.primary_artist_name().unwrap_or("-")),
                Cell::new(&rem.title),
                Cell::new(rem.music_video_type.short_label()),
                Cell::new(format!("sim={:.2}", cluster.similarity)),
            ]);
        }
    }
    let _ = writeln!(out, "{table}");
    out
}

/// Renders minimal LIS sort move preview.
#[must_use]
pub fn render_sort_plan(plan: &SortPlan, execute: bool) -> String {
    let mode = if execute { "EXECUTE" } else { "DRY-RUN" };
    let mut out = format!(
        "[{mode}] Sort '{}' ({}): total={}, anchored(LIS)={}, moves={}\n",
        plan.playlist_title,
        plan.playlist_id,
        plan.target_order.len(),
        plan.anchored_count,
        plan.moves.len()
    );
    if plan.target_order.is_empty() {
        return out;
    }
    let mut table = new_table(&["New #", "Old #", "Artist", "Title", "Year", "Type"]);
    for (idx, t) in plan.target_order.iter().enumerate().take(50) {
        let yr = t
            .release_year
            .map_or_else(|| "-".to_string(), |y| y.to_string());
        table.add_row(vec![
            Cell::new(idx + 1),
            Cell::new(t.position + 1),
            Cell::new(t.primary_artist_name().unwrap_or("-")),
            Cell::new(&t.title),
            Cell::new(yr),
            Cell::new(t.music_video_type.short_label()),
        ]);
    }
    let _ = writeln!(out, "{table}");
    out
}

/// Renders set algebra (`diff`, `intersect`, `merge`) preview.
#[must_use]
pub fn render_diff_plan(plan: &DiffPlan, execute: bool) -> String {
    let mode = if execute { "EXECUTE" } else { "DRY-RUN" };
    let mut out = format!(
        "[{mode}] Set {:?} ({:?}): A={} vs B={} -> {} tracks",
        plan.op,
        plan.match_mode,
        plan.source_a_id,
        plan.source_b_id,
        plan.result_tracks.len()
    );
    if let Some(target) = &plan.target_playlist_id {
        let _ = write!(out, " (into {target})");
    }
    out.push('\n');
    let mut table = new_table(&["#", "VideoID", "Artist", "Title"]);
    for (idx, t) in plan.result_tracks.iter().enumerate() {
        table.add_row(vec![
            Cell::new(idx + 1),
            Cell::new(&t.video_id),
            Cell::new(t.primary_artist_name().unwrap_or("-")),
            Cell::new(&t.title),
        ]);
    }
    let _ = writeln!(out, "{table}");
    out
}

/// Renders playlist health scan & official ATV upgrade candidates.
#[must_use]
pub fn render_health_report(report: &HealthReport, execute: bool) -> String {
    let mode = if execute { "EXECUTE" } else { "DRY-RUN" };
    let mut out = format!(
        "[{mode}] Health '{}' ({}): checked={} | issues={} | upgrades={}\n",
        report.playlist_title,
        report.playlist_id,
        report.total_tracks,
        report.issues.len(),
        report.upgrade_candidates.len()
    );
    if !report.issues.is_empty() {
        let mut issues = new_table(&["#", "VideoID", "Title", "Issue"]);
        for item in &report.issues {
            issues.add_row(vec![
                Cell::new(item.track.position + 1),
                Cell::new(&item.track.video_id),
                Cell::new(&item.track.title),
                Cell::new(format!("{:?}", item.kind)),
            ]);
        }
        let _ = writeln!(out, "{issues}");
    }
    if !report.upgrade_candidates.is_empty() {
        let mut up = new_table(&[
            "Source VideoID",
            "Source Type",
            "Official ATV ID",
            "Official Title",
            "Sim",
        ]);
        for cand in &report.upgrade_candidates {
            up.add_row(vec![
                Cell::new(&cand.source_track.video_id),
                Cell::new(cand.source_track.music_video_type.short_label()),
                Cell::new(&cand.official_atv.video_id),
                Cell::new(&cand.official_atv.title),
                Cell::new(format!("{:.2}", cand.similarity)),
            ]);
        }
        let _ = writeln!(out, "{up}");
    }
    out
}

/// Renders genre split bucket breakdown.
#[must_use]
pub fn render_genre_split_plan(plan: &GenreSplitPlan, execute: bool) -> String {
    let mode = if execute { "EXECUTE" } else { "DRY-RUN" };
    let mut out = format!(
        "[{mode}] Split-Genre '{}' ({}): buckets={} | skipped_small={}\n",
        plan.source_playlist_title,
        plan.source_playlist_id,
        plan.buckets.len(),
        plan.skipped_small_buckets
    );
    if plan.buckets.is_empty() {
        return out;
    }
    let mut table = new_table(&["Genre", "Target Playlist", "Action", "To Add", "Existing"]);
    for bucket in &plan.buckets {
        let action = if bucket.existing_playlist_id.is_some() {
            "APPEND"
        } else {
            "CREATE"
        };
        table.add_row(vec![
            Cell::new(&bucket.genre),
            Cell::new(&bucket.target_playlist_title),
            Cell::new(action),
            Cell::new(bucket.tracks_to_add.len()),
            Cell::new(bucket.already_present_count),
        ]);
    }
    let _ = writeln!(out, "{table}");
    out
}
