use eframe::egui;

use crate::navigation::ToastState;
use crate::state::{ConfirmedMutation, PendingMutation};
use crate::sync_queue::{StagedEntry, StagedOp, SyncQueue};
use crate::views::components::post_preview_meta;

pub fn show(
    ui: &mut egui::Ui,
    queue: &mut SyncQueue,
    pending_modal: &mut Option<PendingMutation>,
    toast: &mut Option<ToastState>,
) {
    let entries = queue.entries().to_vec();
    let selected_entries = queue.selected_entries();
    let can_sync = !selected_entries.is_empty();
    let affected = queue.affected_playlist_ids();
    let total_chunks: usize = selected_entries
        .iter()
        .map(|e| post_preview_meta(&e.op, 50).1)
        .sum();

    ui.horizontal_wrapped(|ui| {
        ui.heading(format!(
            "Review & Dispatch ({} selected / {} staged)",
            selected_entries.len(),
            entries.len()
        ));
        ui.label(format!(
            "Playlists: {} | Est. HTTP POSTs: {}",
            affected.len(),
            total_chunks
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(can_sync, egui::Button::new("Sync Selected..."))
                .on_hover_text("Inspect & confirm remote POSTs (Ctrl+Enter)")
                .clicked()
            {
                let has_delete = selected_entries
                    .iter()
                    .any(|e| matches!(e.op, StagedOp::DeletePlaylist { .. }));
                *pending_modal = Some(PendingMutation {
                    title: if has_delete {
                        "Confirm Sync (Includes Playlist Deletion)".to_string()
                    } else {
                        "Confirm Sync to YouTube Music".to_string()
                    },
                    summary: format!(
                        "Save mandatory pre-sync SQLite snapshots for {} playlist(s) and dispatch {} selected action(s) (~{} HTTP POST chunk(s))?",
                        affected.len(),
                        selected_entries.len(),
                        total_chunks
                    ),
                    action: ConfirmedMutation::ExecuteSync {
                        entries: selected_entries.clone(),
                    },
                });
            }
            if ui
                .add_enabled(!entries.is_empty(), egui::Button::new("Discard All"))
                .clicked()
            {
                queue.clear();
                *toast = Some(ToastState::info(true, "Cleared all staged Sync actions."));
            }
            if !entries.is_empty() {
                let all_sel = selected_entries.len() == entries.len();
                if ui
                    .small_button(if all_sel { "Select None" } else { "Select All" })
                    .clicked()
                {
                    for e in &entries {
                        if e.selected == all_sel {
                            queue.toggle_selected(e.id);
                        }
                    }
                }
            }
        });
    });
    ui.separator();

    if entries.is_empty() {
        ui.label(
            "No staged changes yet. Edit playlists, drag tracks, sort columns, or stage plans from Studio tabs to preview exact POST payloads here before sending.",
        );
        return;
    }

    let mut toggle_id = None;
    let mut remove_id = None;
    let mut move_op: Option<(u64, isize)> = None;
    let total = entries.len();

    egui::ScrollArea::vertical()
        .id_salt("sync_review_cards_scroll")
        .show(ui, |ui| {
            for (idx, entry) in entries.iter().enumerate() {
                render_staged_card(
                    ui,
                    idx,
                    total,
                    entry,
                    &mut toggle_id,
                    &mut move_op,
                    &mut remove_id,
                );
                ui.add_space(4.0);
            }
        });

    if let Some(id) = toggle_id {
        queue.toggle_selected(id);
    }
    if let Some((id, delta)) = move_op
        && let Err(reason) = queue.move_entry(id, delta)
    {
        *toast = Some(ToastState::info(false, reason));
    }
    if let Some(id) = remove_id {
        queue.remove_entry(id);
        *toast = Some(ToastState::info(
            true,
            "Removed staged item and updated local draft.",
        ));
    }
}

fn render_staged_card(
    ui: &mut egui::Ui,
    idx: usize,
    total: usize,
    entry: &StagedEntry,
    toggle_id: &mut Option<u64>,
    move_op: &mut Option<(u64, isize)>,
    remove_id: &mut Option<u64>,
) {
    let (icon, color) = op_badge(&entry.op);
    let (endpoint, chunks) = post_preview_meta(&entry.op, 50);

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            let mut checked = entry.selected;
            if ui.checkbox(&mut checked, "").changed() {
                *toggle_id = Some(entry.id);
            }
            ui.monospace(format!("#{}", idx + 1));
            ui.colored_label(color, format!("{icon} {}", entry.op.kind_label()));
            ui.strong(entry.op.target_label());
            ui.weak(format!("· {endpoint} ({chunks} chunk(s))"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Discard").clicked() {
                    *remove_id = Some(entry.id);
                }
                if ui
                    .add_enabled(idx + 1 < total, egui::Button::new("↓").small())
                    .on_hover_text("Move down in execution order")
                    .clicked()
                {
                    *move_op = Some((entry.id, 1));
                }
                if ui
                    .add_enabled(idx > 0, egui::Button::new("↑").small())
                    .on_hover_text("Move up in execution order")
                    .clicked()
                {
                    *move_op = Some((entry.id, -1));
                }
            });
        });

        ui.indent(("sync_card_body", entry.id), |ui| {
            ui.label(entry.op.summary());
            egui::CollapsingHeader::new("Pre-Flight Diff & POST Payload Preview")
                .id_salt(("sync_card_preview", entry.id))
                .default_open(false)
                .show(ui, |ui| {
                    render_op_diff_details(ui, &entry.op);
                    let json_id = egui::Id::new(("sync_raw_json", entry.id));
                    let mut show_json = ui
                        .ctx()
                        .memory(|m| m.data.get_temp::<bool>(json_id).unwrap_or(false));
                    if ui
                        .checkbox(&mut show_json, "Show Raw JSON Payload")
                        .changed()
                    {
                        ui.ctx()
                            .memory_mut(|m| m.data.insert_temp(json_id, show_json));
                    }
                    if show_json && let Ok(pretty) = serde_json::to_string_pretty(&entry.op) {
                        egui::ScrollArea::vertical()
                            .id_salt(("sync_json_scroll", entry.id))
                            .max_height(140.0)
                            .show(ui, |ui| {
                                ui.monospace(pretty);
                            });
                    }
                });
        });
    });
}

fn render_op_diff_details(ui: &mut egui::Ui, op: &StagedOp) {
    const MAX_PREVIEW_ROWS: usize = 25;
    match op {
        StagedOp::AddTracks {
            tracks,
            allow_duplicates,
            ..
        } => {
            ui.small(format!(
                "Allow duplicates: {} | Tracks (+{}):",
                allow_duplicates,
                tracks.len()
            ));
            for t in tracks.iter().take(MAX_PREVIEW_ROWS) {
                ui.colored_label(
                    egui::Color32::from_rgb(90, 200, 120),
                    format!(
                        "+ {} — {} [{}]",
                        t.title,
                        t.primary_artist_name().unwrap_or("-"),
                        t.video_id
                    ),
                );
            }
            if tracks.len() > MAX_PREVIEW_ROWS {
                ui.weak(format!("... and {} more", tracks.len() - MAX_PREVIEW_ROWS));
            }
        }
        StagedOp::RemoveTracks { tracks, .. } => {
            for t in tracks.iter().take(MAX_PREVIEW_ROWS) {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 100, 100),
                    format!(
                        "- {} — {} [setVideoId: {}]",
                        t.title,
                        t.primary_artist_name().unwrap_or("-"),
                        t.set_video_id.as_deref().unwrap_or("MISSING")
                    ),
                );
            }
            if tracks.len() > MAX_PREVIEW_ROWS {
                ui.weak(format!("... and {} more", tracks.len() - MAX_PREVIEW_ROWS));
            }
        }
        StagedOp::MoveTracks {
            source_playlist_title,
            target_playlist_title,
            tracks,
            ..
        } => {
            ui.small(format!(
                "Atomic Move: Add to '{target_playlist_title}' -> Verify -> Remove from '{source_playlist_title}'"
            ));
            for t in tracks.iter().take(MAX_PREVIEW_ROWS) {
                ui.colored_label(
                    egui::Color32::from_rgb(95, 180, 240),
                    format!(
                        "⇄ {} — {} [{}]",
                        t.title,
                        t.primary_artist_name().unwrap_or("-"),
                        t.video_id
                    ),
                );
            }
        }
        StagedOp::Reorder {
            moves,
            target_set_video_ids,
            ..
        } => {
            ui.small(format!(
                "Target items: {} | Planned LIS moves: {} (recomputed against live setVideoIds at dispatch)",
                target_set_video_ids.len(),
                moves.len()
            ));
        }
        StagedOp::CreatePlaylist {
            title,
            description,
            privacy,
            tracks,
        } => {
            ui.small(format!(
                "Title: '{title}' | Privacy: {privacy:?} | Desc: '{description}' | Tracks: {}",
                tracks.len()
            ));
        }
        StagedOp::UpdatePlaylistMeta {
            title,
            description,
            privacy,
            ..
        } => {
            if let Some(t) = title {
                ui.label(format!("title -> '{t}'"));
            }
            if let Some(d) = description {
                ui.label(format!("description -> '{d}'"));
            }
            if let Some(p) = privacy {
                ui.label(format!("privacy -> {p:?}"));
            }
        }
        StagedOp::DeletePlaylist {
            playlist_id,
            playlist_title,
        } => {
            ui.colored_label(
                egui::Color32::from_rgb(235, 95, 95),
                format!(
                    "PERMANENT DELETE '{playlist_title}' ({playlist_id}) after mandatory SQLite snapshot."
                ),
            );
        }
        StagedOp::UpgradeTracks {
            upgrades,
            preserve_position,
            ..
        } => {
            ui.small(format!("Preserve original position: {preserve_position}"));
            for (old, atv) in upgrades.iter().take(MAX_PREVIEW_ROWS) {
                ui.colored_label(
                    egui::Color32::from_rgb(95, 180, 240),
                    format!("⇄ {} ({}) -> {} (ATV)", old.title, old.video_id, atv.title),
                );
            }
        }
        StagedOp::RestoreSnapshot {
            snapshot_id,
            playlist_title,
            ..
        } => {
            ui.label(format!(
                "Restore snapshot '{snapshot_id}' onto '{playlist_title}' (pre-restore snapshot saved first)."
            ));
        }
        StagedOp::SetLikeStatus {
            video_id,
            title,
            liked,
        } => {
            ui.label(format!(
                "{} '{title}' ({video_id})",
                if *liked { "Like" } else { "Remove Like" }
            ));
        }
    }
}

fn op_badge(op: &StagedOp) -> (&'static str, egui::Color32) {
    match op {
        StagedOp::AddTracks { .. } | StagedOp::CreatePlaylist { .. } => {
            ("+", egui::Color32::from_rgb(90, 200, 120))
        }
        StagedOp::RemoveTracks { .. } | StagedOp::DeletePlaylist { .. } => {
            ("-", egui::Color32::from_rgb(235, 100, 100))
        }
        StagedOp::Reorder { .. }
        | StagedOp::UpgradeTracks { .. }
        | StagedOp::MoveTracks { .. }
        | StagedOp::RestoreSnapshot { .. } => ("⇄", egui::Color32::from_rgb(95, 180, 240)),
        StagedOp::UpdatePlaylistMeta { .. } => ("✎", egui::Color32::from_rgb(235, 190, 80)),
        StagedOp::SetLikeStatus { liked, .. } => (
            if *liked { "♥" } else { "♡" },
            egui::Color32::from_rgb(235, 120, 160),
        ),
    }
}
