use std::collections::HashMap;
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::Playlist;
use ytm_ops::plan_restore_diff;
use ytm_store::{Snapshot, SnapshotMetadata};

use crate::navigation::ToastState;
use crate::state::{AsyncState, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};
pub use crate::views::genre_split::show_genre_split;

pub fn show_snapshots(
    ui: &mut egui::Ui,
    snapshots_state: &AsyncState<Vec<SnapshotMetadata>>,
    detail_state: &mut AsyncState<Snapshot>,
    playlist_cache: &HashMap<String, Playlist>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.horizontal(|ui| {
        if ui.button("Refresh Snapshots").clicked() {
            let _ = cmd_tx.send(WorkerCmd::RefreshSnapshots);
        }
        if let Some(snap) = detail_state.data()
            && ui.button("Stage Restore to Sync").clicked()
        {
            let eid = sync_queue.push(StagedOp::RestoreSnapshot {
                snapshot_id: snap.metadata.id.clone(),
                playlist_id: snap.playlist.id.clone(),
                playlist_title: snap.playlist.title.clone(),
            });
            *toast = Some(ToastState::staged(
                format!(
                    "Staged restore of '{}' onto '{}' in Sync.",
                    snap.metadata.id, snap.playlist.title
                ),
                Some(eid),
            ));
        }
    });
    ui.separator();

    let Some(snaps) = snapshots_state.data() else {
        ui.label("Loading local snapshots...");
        return;
    };

    let list_h = if detail_state.data().is_some() {
        (ui.available_height() * 0.45).max(120.0)
    } else {
        ui.available_height().max(160.0)
    };
    ui.allocate_ui(egui::vec2(ui.available_width(), list_h), |ui| {
        TableBuilder::new(ui)
            .id_salt("snapshots_tbl")
            .striped(true)
            .column(Column::initial(180.0).clip(true))
            .column(Column::initial(150.0).clip(true))
            .column(Column::initial(110.0))
            .column(Column::exact(50.0))
            .column(Column::remainder())
            .header(20.0, |mut h| {
                for name in ["Snapshot ID", "Playlist", "Reason", "Items", "Inspect"] {
                    h.col(|ui| {
                        ui.strong(name);
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, snaps.len(), |mut row| {
                    let s = &snaps[row.index()];
                    let mut open_snap = false;
                    row.col(|ui| {
                        if ui
                            .add(egui::Label::new(&s.id).sense(egui::Sense::click()))
                            .on_hover_text("Double-click to inspect snapshot diff")
                            .double_clicked()
                        {
                            open_snap = true;
                        }
                    });
                    row.col(|ui| {
                        ui.label(&s.playlist_title);
                    });
                    row.col(|ui| {
                        ui.label(&s.reason);
                    });
                    row.col(|ui| {
                        ui.label(s.item_count.to_string());
                    });
                    row.col(|ui| {
                        if ui.button("Inspect").clicked() {
                            open_snap = true;
                        }
                    });
                    if open_snap {
                        *detail_state = AsyncState::Loading("Loading snapshot...".to_string());
                        let _ = cmd_tx.send(WorkerCmd::LoadSnapshot(s.id.clone()));
                    }
                });
            });
    });

    if let Some(snap) = detail_state.data() {
        ui.separator();
        ui.strong(format!(
            "Snapshot Diff Preview: {} | {} ({} tracks)",
            snap.metadata.id,
            snap.playlist.title,
            snap.playlist.tracks.len()
        ));
        if let Some(live) = playlist_cache.get(&snap.playlist.id) {
            let diff = plan_restore_diff(live, &snap.playlist);
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(90, 200, 120),
                    format!("+Add: {}", diff.added_tracks.len()),
                );
                ui.colored_label(
                    egui::Color32::from_rgb(235, 100, 100),
                    format!("-Remove: {}", diff.removed_tracks.len()),
                );
                ui.label(format!(
                    "Target Order: {} tracks",
                    diff.target_video_order.len()
                ));
            });
            egui::ScrollArea::vertical()
                .id_salt("snap_diff_scroll")
                .show(ui, |ui| {
                    for t in &diff.added_tracks {
                        ui.colored_label(
                            egui::Color32::from_rgb(90, 200, 120),
                            format!("+ {} — {}", t.title, t.primary_artist_name().unwrap_or("-")),
                        );
                    }
                    for t in &diff.removed_tracks {
                        ui.colored_label(
                            egui::Color32::from_rgb(235, 100, 100),
                            format!("- {} — {}", t.title, t.primary_artist_name().unwrap_or("-")),
                        );
                    }
                    if diff.added_tracks.is_empty() && diff.removed_tracks.is_empty() {
                        ui.weak("Same track set as cached playlist (order-only restore if positions differ).");
                    }
                });
        } else {
            ui.weak("Load this playlist in Inspector to preview live vs snapshot diff.");
        }
    }
}
