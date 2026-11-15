use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::navigation::ToastState;
use crate::state::{ConfirmedMutation, PendingMutation};
use crate::sync_queue::{StagedOp, SyncQueue};

pub fn show(
    ui: &mut egui::Ui,
    queue: &mut SyncQueue,
    pending_modal: &mut Option<PendingMutation>,
    toast: &mut Option<ToastState>,
) {
    let can_sync = !queue.is_empty();
    let entries = queue.entries().to_vec();
    let affected = queue.affected_playlist_ids();

    ui.horizontal(|ui| {
        ui.heading(format!("Pending Sync Queue ({})", entries.len()));
        ui.label(format!("Playlists touched: {}", affected.len()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(can_sync, egui::Button::new("Sync Now..."))
                .on_hover_text("Shortcut: Ctrl+Enter")
                .clicked()
            {
                *pending_modal = Some(PendingMutation {
                    title: "Confirm Sync to YouTube Music".to_string(),
                    summary: format!(
                        "Save pre-sync SQLite snapshots for {} playlist(s) and execute {} queued action(s)?",
                        affected.len(),
                        entries.len()
                    ),
                    action: ConfirmedMutation::ExecuteSync {
                        entries: entries.clone(),
                    },
                });
            }
            if ui
                .add_enabled(can_sync, egui::Button::new("Discard All"))
                .clicked()
            {
                queue.clear();
                *toast = Some(ToastState::info(true, "Cleared all staged Sync actions."));
            }
        });
    });
    ui.separator();

    if entries.is_empty() {
        ui.label(
            "No staged changes yet. Drag tracks, sort columns, compare playlists, or queue plans from other tabs to preview here before syncing.",
        );
        return;
    }

    let mut remove_id = None;
    let mut move_op: Option<(u64, isize)> = None;
    let total = entries.len();
    TableBuilder::new(ui)
        .id_salt("sync_queue_tbl")
        .striped(true)
        .resizable(true)
        .column(Column::exact(36.0))
        .column(Column::initial(130.0))
        .column(Column::initial(180.0).clip(true))
        .column(Column::remainder().clip(true))
        .column(Column::exact(126.0))
        .header(22.0, |mut h| {
            for col in ["#", "Action", "Playlist", "Details", "Order / Drop"] {
                h.col(|ui| {
                    ui.strong(col);
                });
            }
        })
        .body(|body| {
            body.rows(22.0, entries.len(), |mut row| {
                let idx = row.index();
                let entry = &entries[idx];
                row.col(|ui| {
                    ui.label((idx + 1).to_string());
                });
                row.col(|ui| {
                    let (icon, color) = match &entry.op {
                        StagedOp::AddTracks { .. } | StagedOp::CreatePlaylist { .. } => {
                            ("+", egui::Color32::from_rgb(90, 200, 120))
                        }
                        StagedOp::RemoveTracks { .. } => {
                            ("-", egui::Color32::from_rgb(235, 100, 100))
                        }
                        StagedOp::Reorder { .. } | StagedOp::UpgradeTracks { .. } => {
                            ("⇄", egui::Color32::from_rgb(95, 180, 240))
                        }
                    };
                    ui.colored_label(color, format!("{icon} {}", entry.op.kind_label()));
                });
                row.col(|ui| {
                    ui.label(entry.op.target_label());
                });
                row.col(|ui| {
                    ui.label(entry.op.summary());
                });
                row.col(|ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(idx > 0, egui::Button::new("↑"))
                            .on_hover_text("Move up in execution order")
                            .clicked()
                        {
                            move_op = Some((entry.id, -1));
                        }
                        if ui
                            .add_enabled(idx + 1 < total, egui::Button::new("↓"))
                            .on_hover_text("Move down in execution order")
                            .clicked()
                        {
                            move_op = Some((entry.id, 1));
                        }
                        if ui.button("Discard").clicked() {
                            remove_id = Some(entry.id);
                        }
                    });
                });
            });
        });

    if let Some((id, delta)) = move_op {
        queue.move_entry(id, delta);
    }
    if let Some(id) = remove_id {
        queue.remove_entry(id);
        *toast = Some(ToastState::info(
            true,
            "Removed staged item and updated local draft.",
        ));
    }
}
