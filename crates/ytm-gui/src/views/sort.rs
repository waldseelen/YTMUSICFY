use std::collections::HashMap;
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::Playlist;
use ytm_ops::{SortDirection, SortField, SortKey, SortPlan};

use crate::navigation::ToastState;
use crate::state::{AsyncState, SortForm, WorkerCmd};
use crate::sync_queue::SyncQueue;
use crate::views::async_banner;

const ALL_FIELDS: [SortField; 8] = [
    SortField::Artist,
    SortField::Album,
    SortField::Title,
    SortField::Year,
    SortField::Duration,
    SortField::Genre,
    SortField::Type,
    SortField::Shuffle,
];

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    selected_playlist_id: Option<&str>,
    form: &mut SortForm,
    plan_state: &mut AsyncState<SortPlan>,
    playlist_cache: &HashMap<String, Playlist>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    let mut remove_idx = None;
    let mut swap_pair: Option<(usize, usize)> = None;
    let key_count = form.keys.len();
    for (idx, key) in form.keys.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("Key {}:", idx + 1));
            egui::ComboBox::from_id_salt(format!("sort_field_{idx}"))
                .selected_text(format!("{:?}", key.field))
                .show_ui(ui, |ui| {
                    for f in ALL_FIELDS {
                        ui.selectable_value(&mut key.field, f, format!("{f:?}"));
                    }
                });
            egui::ComboBox::from_id_salt(format!("sort_dir_{idx}"))
                .selected_text(format!("{:?}", key.direction))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut key.direction, SortDirection::Asc, "Asc");
                    ui.selectable_value(&mut key.direction, SortDirection::Desc, "Desc");
                });
            if ui
                .add_enabled(idx > 0, egui::Button::new("↑"))
                .on_hover_text("Higher sort priority")
                .clicked()
            {
                swap_pair = Some((idx, idx - 1));
            }
            if ui
                .add_enabled(idx + 1 < key_count, egui::Button::new("↓"))
                .on_hover_text("Lower sort priority")
                .clicked()
            {
                swap_pair = Some((idx, idx + 1));
            }
            if key_count > 1 && ui.button("✕").clicked() {
                remove_idx = Some(idx);
            }
        });
    }
    if let Some((a, b)) = swap_pair {
        form.keys.swap(a, b);
    }
    if let Some(i) = remove_idx {
        form.keys.remove(i);
    }

    ui.horizontal(|ui| {
        if ui.button("+ Add Key").clicked() {
            form.keys.push(SortKey {
                field: SortField::Title,
                direction: SortDirection::Asc,
            });
        }
        ui.checkbox(&mut form.artist_spread, "Artist spread");
        ui.checkbox(&mut form.use_seed, "Shuffle seed:");
        if form.use_seed {
            ui.add(egui::DragValue::new(&mut form.seed));
        }
    });

    ui.horizontal(|ui| {
        let can_plan = selected_playlist_id.is_some() && !plan_state.is_loading();
        if ui
            .add_enabled(
                can_plan,
                egui::Button::new("Compute LIS Minimal Moves (Dry-Run)"),
            )
            .clicked()
            && let Some(pid) = selected_playlist_id
        {
            *plan_state = AsyncState::Loading("Computing minimal LIS moves...".to_string());
            let _ = cmd_tx.send(WorkerCmd::PlanSort {
                playlist_id: pid.to_string(),
                spec: form.to_spec(),
            });
        }

        if let Some(plan) = plan_state.data()
            && !plan.moves.is_empty()
            && ui.button("Stage Sort Moves to Sync").clicked()
        {
            let target_sids: Vec<String> = plan
                .target_order
                .iter()
                .filter_map(|t| t.set_video_id.clone())
                .collect();
            match sync_queue.stage_reorder(
                &plan.playlist_id,
                &plan.playlist_title,
                target_sids,
                playlist_cache,
            ) {
                Ok(n) => {
                    *toast = Some(ToastState::staged(
                        format!(
                            "Staged {n} LIS moves for '{}' in Sync.",
                            plan.playlist_title
                        ),
                        sync_queue.last_entry_id(),
                    ));
                }
                Err(e) => *toast = Some(ToastState::info(false, e)),
            }
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
        ui.label("Compute LIS Minimal Moves to preview the exact target track order.");
        return;
    };

    ui.label(format!(
        "Total: {} | Anchored (LIS): {} | API Moves Required: {}",
        plan.target_order.len(),
        plan.anchored_count,
        plan.moves.len()
    ));

    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .column(Column::exact(48.0))
        .column(Column::exact(48.0))
        .column(Column::initial(160.0).clip(true))
        .column(Column::remainder())
        .header(22.0, |mut header| {
            for h in ["New #", "Old #", "Artist", "Title"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|body| {
            body.rows(20.0, plan.target_order.len(), |mut row| {
                let idx = row.index();
                let t = &plan.target_order[idx];
                row.col(|ui| {
                    ui.label((idx + 1).to_string());
                });
                row.col(|ui| {
                    ui.label((t.position + 1).to_string());
                });
                row.col(|ui| {
                    ui.label(t.primary_artist_name().unwrap_or("-"));
                });
                row.col(|ui| {
                    ui.label(&t.title);
                });
            });
        });
}
