use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_ops::{DedupeConfig, DedupePlan, KeeperStrategy};

use crate::navigation::ToastState;
use crate::state::{AsyncState, DedupeForm, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::async_banner;

pub fn show(
    ui: &mut egui::Ui,
    selected_playlist_id: Option<&str>,
    form: &mut DedupeForm,
    plan_state: &mut AsyncState<DedupePlan>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut form.threshold, 0.70..=1.0).text("Threshold"));
        ui.add(egui::DragValue::new(&mut form.duration_tol).suffix("s"));
        ui.checkbox(&mut form.ignore_duration, "Ignore duration");
        ui.checkbox(&mut form.exact_only, "Exact only");
        egui::ComboBox::from_id_salt("dedupe_keep")
            .selected_text(format!("{:?}", form.keep))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut form.keep, KeeperStrategy::Quality, "Quality");
                ui.selectable_value(&mut form.keep, KeeperStrategy::Oldest, "Oldest");
                ui.selectable_value(&mut form.keep, KeeperStrategy::Newest, "Newest");
            });
    });

    ui.horizontal(|ui| {
        let can_scan = selected_playlist_id.is_some() && !plan_state.is_loading();
        if ui
            .add_enabled(can_scan, egui::Button::new("Scan Duplicates (Dry-Run)"))
            .clicked()
            && let Some(pid) = selected_playlist_id
        {
            *plan_state = AsyncState::Loading("Scanning duplicates...".to_string());
            let _ = cmd_tx.send(WorkerCmd::PlanDedupe {
                playlist_id: pid.to_string(),
                config: DedupeConfig {
                    fuzzy_threshold: form.threshold,
                    duration_tolerance_sec: form.duration_tol,
                    ignore_duration: form.ignore_duration,
                    keep: form.keep,
                    exact_only: form.exact_only,
                },
            });
        }

        if let Some(plan) = plan_state.data()
            && plan.total_removals > 0
            && ui.button("Stage Removals to Sync").clicked()
        {
            let tracks: Vec<_> = plan
                .clusters
                .iter()
                .flat_map(|c| c.removals.iter())
                .filter(|t| t.set_video_id.is_some())
                .cloned()
                .collect();
            let count = tracks.len();
            let eid = sync_queue.push(StagedOp::RemoveTracks {
                playlist_id: plan.playlist_id.clone(),
                playlist_title: plan.playlist_title.clone(),
                tracks,
            });
            *toast = Some(ToastState::staged(
                format!(
                    "Staged {count} duplicate removals for '{}' in Sync.",
                    plan.playlist_title
                ),
                Some(eid),
            ));
        }
    });

    let (loading, error) = match plan_state {
        AsyncState::Loading(m) => (Some(m.as_str()), None),
        AsyncState::Failed(e) => (None, Some(e.as_str())),
        _ => (None, None),
    };
    async_banner(ui, loading, error);
    ui.separator();

    let Some(plan) = plan_state.data() else {
        ui.label("Run Scan Duplicates to preview clusters before touching YouTube Music.");
        return;
    };

    ui.label(format!(
        "Clusters: {} | Removals: {} | Skipped (missing setVideoId): {}",
        plan.clusters.len(),
        plan.total_removals,
        plan.skipped_without_set_id
    ));

    let mut flat_rows = Vec::new();
    for c in &plan.clusters {
        flat_rows.push(("KEEP", &c.keeper, c.reason.clone()));
        for r in &c.removals {
            flat_rows.push(("REMOVE", r, format!("sim {:.2}", c.similarity)));
        }
    }

    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .column(Column::exact(68.0))
        .column(Column::initial(220.0).clip(true))
        .column(Column::initial(160.0).clip(true))
        .column(Column::exact(50.0))
        .column(Column::remainder())
        .header(22.0, |mut header| {
            for h in ["Action", "Title", "Artist", "Type", "Reason"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|body| {
            body.rows(20.0, flat_rows.len(), |mut row| {
                let (action, track, reason) = &flat_rows[row.index()];
                let color = if *action == "KEEP" {
                    egui::Color32::from_rgb(90, 200, 120)
                } else {
                    egui::Color32::from_rgb(235, 100, 100)
                };
                row.col(|ui| {
                    ui.colored_label(color, *action);
                });
                row.col(|ui| {
                    ui.label(&track.title);
                });
                row.col(|ui| {
                    ui.label(track.primary_artist_name().unwrap_or("-"));
                });
                row.col(|ui| {
                    ui.label(track.music_video_type.short_label());
                });
                row.col(|ui| {
                    ui.label(reason);
                });
            });
        });
}
