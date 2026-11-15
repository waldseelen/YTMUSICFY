use std::path::PathBuf;
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::Playlist;
use ytm_enrich::GenreSplitPlan;
use ytm_store::{Snapshot, SnapshotMetadata};

use crate::state::{AsyncState, ConfirmedMutation, GenreSplitForm, PendingMutation, WorkerCmd};
use crate::views::async_banner;

pub fn show_genre_split(
    ui: &mut egui::Ui,
    selected_playlist_id: Option<&str>,
    form: &mut GenreSplitForm,
    plan_state: &mut AsyncState<(Playlist, GenreSplitPlan)>,
    pending_modal: &mut Option<PendingMutation>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.horizontal(|ui| {
        ui.label("Min tracks/genre:");
        ui.add(egui::DragValue::new(&mut form.min_tracks).range(1..=100));
        ui.label("Custom genres.toml (optional):");
        ui.text_edit_singleline(&mut form.custom_genres_toml);
    });

    ui.horizontal(|ui| {
        let can_plan = selected_playlist_id.is_some() && !plan_state.is_loading();
        if ui
            .add_enabled(
                can_plan,
                egui::Button::new("Enrich & Preview Split (Dry-Run)"),
            )
            .clicked()
            && let Some(pid) = selected_playlist_id
        {
            *plan_state =
                AsyncState::Loading("Enriching genres (SQLite + MusicBrainz)...".to_string());
            let custom = (!form.custom_genres_toml.trim().is_empty())
                .then(|| PathBuf::from(form.custom_genres_toml.trim()));
            let _ = cmd_tx.send(WorkerCmd::PlanGenreSplit {
                playlist_id: pid.to_string(),
                min_tracks: form.min_tracks,
                custom_genres_path: custom,
            });
        }

        if let Some((source_pl, plan)) = plan_state.data()
            && plan.buckets.iter().any(|b| !b.tracks_to_add.is_empty())
            && ui.button("Create / Update Genre Playlists...").clicked()
        {
            *pending_modal = Some(PendingMutation {
                title: "Confirm Genre Split".to_string(),
                summary: format!(
                    "Save snapshot and create/append {} genre buckets from '{}'?",
                    plan.buckets.len(),
                    source_pl.title
                ),
                action: ConfirmedMutation::GenreSplit {
                    source_playlist: source_pl.clone(),
                    plan: plan.clone(),
                },
            });
        }
    });

    let (loading, error) = match plan_state {
        AsyncState::Loading(m) => (Some(m.as_str()), None),
        AsyncState::Failed(e) => (None, Some(e.as_str())),
        _ => (None, None),
    };
    async_banner(ui, loading, error);
    ui.separator();

    let Some((_, plan)) = plan_state.data() else {
        ui.label("Click Enrich & Preview Split to partition the selected playlist by genre.");
        return;
    };

    ui.label(format!(
        "Buckets: {} | Skipped (< {} tracks): {}",
        plan.buckets.len(),
        form.min_tracks,
        plan.skipped_small_buckets
    ));

    TableBuilder::new(ui)
        .striped(true)
        .column(Column::initial(110.0))
        .column(Column::initial(200.0).clip(true))
        .column(Column::exact(70.0))
        .column(Column::exact(65.0))
        .column(Column::remainder())
        .header(22.0, |mut h| {
            for name in ["Genre", "Target Playlist", "Action", "To Add", "Existing"] {
                h.col(|ui| {
                    ui.strong(name);
                });
            }
        })
        .body(|body| {
            body.rows(20.0, plan.buckets.len(), |mut row| {
                let b = &plan.buckets[row.index()];
                let action = if b.existing_playlist_id.is_some() {
                    "APPEND"
                } else {
                    "CREATE"
                };
                row.col(|ui| {
                    ui.label(&b.genre);
                });
                row.col(|ui| {
                    ui.label(&b.target_playlist_title);
                });
                row.col(|ui| {
                    ui.label(action);
                });
                row.col(|ui| {
                    ui.label(b.tracks_to_add.len().to_string());
                });
                row.col(|ui| {
                    ui.label(b.already_present_count.to_string());
                });
            });
        });
}

pub fn show_snapshots(
    ui: &mut egui::Ui,
    snapshots_state: &AsyncState<Vec<SnapshotMetadata>>,
    detail_state: &mut AsyncState<Snapshot>,
    pending_modal: &mut Option<PendingMutation>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.horizontal(|ui| {
        if ui.button("Refresh Snapshots").clicked() {
            let _ = cmd_tx.send(WorkerCmd::RefreshSnapshots);
        }
        if let Some(snap) = detail_state.data()
            && ui.button("Restore Selected Snapshot...").clicked()
        {
            *pending_modal = Some(PendingMutation {
                title: "Confirm Snapshot Restore".to_string(),
                summary: format!(
                    "Save pre-restore snapshot and restore '{}' ({} tracks) onto '{}'?",
                    snap.metadata.id,
                    snap.playlist.tracks.len(),
                    snap.playlist.title
                ),
                action: ConfirmedMutation::RestoreSnapshot {
                    snapshot_id: snap.metadata.id.clone(),
                },
            });
        }
    });
    ui.separator();

    let Some(snaps) = snapshots_state.data() else {
        ui.label("Loading local snapshots...");
        return;
    };

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
                row.col(|ui| {
                    ui.label(&s.id);
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
                        *detail_state = AsyncState::Loading("Loading snapshot...".to_string());
                        let _ = cmd_tx.send(WorkerCmd::LoadSnapshot(s.id.clone()));
                    }
                });
            });
        });

    if let Some(snap) = detail_state.data() {
        ui.add_space(6.0);
        ui.strong(format!(
            "Selected: {} | {} ({} tracks)",
            snap.metadata.id,
            snap.playlist.title,
            snap.playlist.tracks.len()
        ));
    }
}
