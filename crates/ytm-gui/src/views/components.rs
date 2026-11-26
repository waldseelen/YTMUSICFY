use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use chrono::{DateTime, Utc};
use eframe::egui;
use ytm_core::{FetchCompleteness, FetchManifest};

use crate::state::{DraftRevision, PlanFingerprint};
use crate::sync_queue::StagedOp;

/// Compute a deterministic 64-bit hash for any form state to detect stale dry-run plans.
#[must_use]
pub fn hash_form<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Build a `PlanFingerprint` from playlist ID, current `DraftRevision`, and form hash.
#[must_use]
pub fn make_fingerprint(
    playlist_id: &str,
    revision: DraftRevision,
    form_hash: u64,
) -> PlanFingerprint {
    PlanFingerprint {
        playlist_id: playlist_id.to_string(),
        draft_revision: revision,
        form_hash,
    }
}

/// Store the latest scanned `PlanFingerprint` in egui memory under `salt`.
pub fn store_plan_fingerprint(ctx: &egui::Context, salt: &str, fp: PlanFingerprint) {
    ctx.memory_mut(|m| m.data.insert_temp(egui::Id::new(salt), fp));
}

/// Load the stored `PlanFingerprint` from egui memory under `salt`.
#[must_use]
pub fn load_plan_fingerprint(ctx: &egui::Context, salt: &str) -> Option<PlanFingerprint> {
    ctx.memory(|m| m.data.get_temp::<PlanFingerprint>(egui::Id::new(salt)))
}

/// Format a UTC timestamp as a concise relative age string (e.g. `"2m ago"`, `"3d ago"`).
#[must_use]
pub fn format_relative_age(ts: DateTime<Utc>) -> String {
    let secs = Utc::now().signed_duration_since(ts).num_seconds().max(0);
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// Render a clickable Fetch Health Badge with a `FetchManifest` detail popover.
pub fn show_fetch_badge(ui: &mut egui::Ui, manifest: Option<&FetchManifest>) {
    let Some(m) = manifest else {
        ui.colored_label(egui::Color32::GRAY, "Cached (no manifest)");
        return;
    };
    let age = format_relative_age(m.fetched_at);
    let (label, color) = match &m.completeness {
        FetchCompleteness::Complete => {
            let decl = m.declared_track_count.unwrap_or(m.fetched_tracks);
            (
                format!("✓ {}/{} Complete · {age}", m.fetched_tracks, decl),
                egui::Color32::from_rgb(90, 200, 120),
            )
        }
        FetchCompleteness::PartialHidden {
            declared,
            fetched,
            hidden_estimate,
        } => (
            format!("⚠ {fetched}/{declared} ({hidden_estimate} hidden) · {age}"),
            egui::Color32::from_rgb(235, 180, 70),
        ),
        FetchCompleteness::Truncated {
            pages_fetched,
            max_pages,
        } => (
            format!("🔒 Truncated ({pages_fetched}/{max_pages} pages) — Refresh"),
            egui::Color32::from_rgb(235, 95, 95),
        ),
        FetchCompleteness::Degraded { dropped_renderers } => (
            format!(
                "🔒 Degraded ({} dropped) — Refresh",
                dropped_renderers.len()
            ),
            egui::Color32::from_rgb(235, 95, 95),
        ),
    };

    let btn = egui::Button::new(egui::RichText::new(label).color(color).small());
    let resp = ui.add(btn).on_hover_text("Click to inspect FetchManifest");
    let popup_id = ui.make_persistent_id(("fetch_manifest_popup", m.fetched_at.timestamp()));
    if resp.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }
    egui::popup_below_widget(
        ui,
        popup_id,
        &resp,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(270.0);
            ui.strong("FetchManifest Telemetry");
            ui.separator();
            ui.label(format!(
                "Declared by server: {}",
                m.declared_track_count
                    .map_or_else(|| "unknown".to_string(), |n| n.to_string())
            ));
            ui.label(format!("Fetched tracks: {}", m.fetched_tracks));
            ui.label(format!(
                "Available: {} | Unavailable: {}",
                m.available_tracks, m.unavailable_tracks
            ));
            ui.label(format!("Missing setVideoId: {}", m.missing_set_video_id));
            ui.label(format!("Pages fetched: {}", m.pages_fetched));
            ui.label(format!("Fetched: {age}"));
            if let FetchCompleteness::Degraded { dropped_renderers } = &m.completeness {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 95, 95),
                    format!("Dropped renderers: {}", dropped_renderers.join(", ")),
                );
            }
            if m.completeness.is_mutation_locked() {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 95, 95),
                    "Mutations are HARD-LOCKED until cleanly re-fetched.",
                );
            }
        },
    );
}

/// Return `(innertube_endpoint, estimated_chunks)` for Pre-Flight POST preview.
#[must_use]
pub fn post_preview_meta(op: &StagedOp, chunk_size: usize) -> (&'static str, usize) {
    let cs = chunk_size.max(1);
    let chunks = |n: usize| n.div_ceil(cs).max(1);
    match op {
        StagedOp::AddTracks { tracks, .. } => ("POST browse/edit_playlist", chunks(tracks.len())),
        StagedOp::RemoveTracks { tracks, .. } => {
            ("POST browse/edit_playlist", chunks(tracks.len()))
        }
        StagedOp::MoveTracks { tracks, .. } => (
            "POST browse/edit_playlist (Add+Remove)",
            chunks(tracks.len()) * 2,
        ),
        StagedOp::Reorder { moves, .. } => ("POST browse/edit_playlist", chunks(moves.len())),
        StagedOp::CreatePlaylist { tracks, .. } => (
            "POST playlist/create (+chunks)",
            1 + tracks.len().saturating_sub(cs).div_ceil(cs),
        ),
        StagedOp::UpdatePlaylistMeta { .. } => ("POST browse/edit_playlist", 1),
        StagedOp::DeletePlaylist { .. } => ("POST playlist/delete", 1),
        StagedOp::UpgradeTracks {
            upgrades,
            preserve_position,
            ..
        } => {
            let mult = if *preserve_position { 3 } else { 2 };
            (
                "POST browse/edit_playlist (Upgrade)",
                chunks(upgrades.len()) * mult,
            )
        }
        StagedOp::RestoreSnapshot { .. } => ("POST browse/edit_playlist (Restore)", 2),
        StagedOp::SetLikeStatus { liked, .. } => (
            if *liked {
                "POST like/like"
            } else {
                "POST like/removelike"
            },
            1,
        ),
    }
}
