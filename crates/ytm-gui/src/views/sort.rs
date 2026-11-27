use std::collections::HashMap;
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::{FetchManifest, Playlist, PlaylistAccess};
use ytm_ops::{SortDirection, SortField, SortKey, SortPlan};

use crate::icons::{Icon, icon_button};
use crate::navigation::ToastState;
use crate::state::{AsyncState, SortForm, WorkerCmd};
use crate::sync_queue::SyncQueue;
use crate::views::async_banner;
use crate::views::components::{
    hash_form, load_plan_fingerprint, make_fingerprint, store_plan_fingerprint,
};

const FP_SALT: &str = "sort_plan_fp";

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
    active_playlist: Option<&Playlist>,
    manifest: Option<&FetchManifest>,
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
            if icon_button(ui, Icon::ArrowUp, 12.0, idx > 0, "Higher sort priority").clicked() {
                swap_pair = Some((idx, idx - 1));
            }
            if icon_button(
                ui,
                Icon::ArrowDown,
                12.0,
                idx + 1 < key_count,
                "Lower sort priority",
            )
            .clicked()
            {
                swap_pair = Some((idx, idx + 1));
            }
            if key_count > 1 && icon_button(ui, Icon::Close, 12.0, true, "Remove key").clicked() {
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

    let keys_sig: Vec<(String, String)> = form
        .keys
        .iter()
        .map(|k| (format!("{:?}", k.field), format!("{:?}", k.direction)))
        .collect();
    let form_sig = hash_form(&(keys_sig, form.artist_spread, form.use_seed, form.seed));
    let current_fp =
        selected_playlist_id.map(|pid| make_fingerprint(pid, sync_queue.revision(), form_sig));
    let stored_fp = load_plan_fingerprint(ui.ctx(), FP_SALT);
    let plan_is_fresh = current_fp.is_some() && current_fp == stored_fp;

    let manifest_locked = manifest.is_some_and(|m| m.completeness.is_mutation_locked());
    let editable = active_playlist.is_some_and(|p| p.access() == PlaylistAccess::EditableOwned)
        && !manifest_locked;

    ui.horizontal_wrapped(|ui| {
        let can_plan = selected_playlist_id.is_some() && !plan_state.is_loading();
        if ui
            .add_enabled(
                can_plan,
                egui::Button::new("Compute LIS Minimal Moves (Dry-Run)"),
            )
            .clicked()
            && let Some(pid) = selected_playlist_id
        {
            if let Some(fp) = &current_fp {
                store_plan_fingerprint(ui.ctx(), FP_SALT, fp.clone());
            }
            *plan_state = AsyncState::Loading("Computing minimal LIS moves...".to_string());
            let _ = cmd_tx.send(WorkerCmd::PlanSort {
                playlist_id: pid.to_string(),
                spec: form.to_spec(),
            });
        }

        if let Some(plan) = plan_state.data()
            && !plan.moves.is_empty()
        {
            let can_stage = editable && plan_is_fresh;
            if ui
                .add_enabled(can_stage, egui::Button::new("Stage Sort Moves to Sync"))
                .clicked()
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
            if !plan_is_fresh {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 180, 70),
                    "Sort parameters or draft changed — re-compute to stage.",
                );
            } else if !editable {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 100, 100),
                    if manifest_locked {
                        "Fetch truncated/degraded — refresh playlist before staging."
                    } else {
                        "Read-only / LM playlist — staging disabled."
                    },
                );
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
