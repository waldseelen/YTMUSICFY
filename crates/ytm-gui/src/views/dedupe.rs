use std::collections::HashSet;
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::{FetchManifest, Playlist, PlaylistAccess};
use ytm_ops::{DedupeConfig, DedupePlan, KeeperStrategy};

use crate::navigation::ToastState;
use crate::playback_ops::PlayAction;
use crate::state::{AsyncState, DedupeForm, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::async_banner;
use crate::views::components::{
    hash_form, load_plan_fingerprint, make_fingerprint, store_plan_fingerprint,
};

const FP_SALT: &str = "dedupe_plan_fp";
const PICK_SALT: &str = "dedupe_cluster_excluded";

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    active_playlist: Option<&Playlist>,
    manifest: Option<&FetchManifest>,
    selected_playlist_id: Option<&str>,
    form: &mut DedupeForm,
    plan_state: &mut AsyncState<DedupePlan>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
    cmd_tx: &Sender<WorkerCmd>,
) -> Vec<PlayAction> {
    let mut actions = Vec::new();
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

    let form_sig = hash_form(&(
        (form.threshold * 1000.0).round() as i64,
        form.duration_tol,
        form.ignore_duration,
        form.exact_only,
        format!("{:?}", form.keep),
    ));
    let current_fp = selected_playlist_id
        .map(|pid| make_fingerprint(pid, sync_queue.revision(), form_sig));
    let stored_fp = load_plan_fingerprint(ui.ctx(), FP_SALT);
    let plan_is_fresh = current_fp.is_some() && current_fp == stored_fp;

    let manifest_locked = manifest.is_some_and(|m| m.completeness.is_mutation_locked());
    let editable = active_playlist
        .is_some_and(|p| p.access() == PlaylistAccess::EditableOwned)
        && !manifest_locked;

    let excluded_id = egui::Id::new(PICK_SALT);
    let mut excluded: HashSet<usize> = ui
        .ctx()
        .memory(|m| m.data.get_temp::<HashSet<usize>>(excluded_id))
        .unwrap_or_default();

    ui.horizontal_wrapped(|ui| {
        let can_scan = selected_playlist_id.is_some() && !plan_state.is_loading();
        if ui
            .add_enabled(can_scan, egui::Button::new("Scan Duplicates (Dry-Run)"))
            .clicked()
            && let Some(pid) = selected_playlist_id
        {
            if let Some(fp) = &current_fp {
                store_plan_fingerprint(ui.ctx(), FP_SALT, fp.clone());
            }
            excluded.clear();
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
        {
            let selected_removals: Vec<_> = plan
                .clusters
                .iter()
                .enumerate()
                .filter(|(cidx, _)| !excluded.contains(cidx))
                .flat_map(|(_, c)| c.removals.iter())
                .filter(|t| t.set_video_id.is_some())
                .cloned()
                .collect();
            let can_stage = editable && plan_is_fresh && !selected_removals.is_empty();
            let btn = ui.add_enabled(
                can_stage,
                egui::Button::new(format!(
                    "Stage {} Removal(s) to Sync",
                    selected_removals.len()
                )),
            );
            if btn.clicked() {
                let count = selected_removals.len();
                let eid = sync_queue.push(StagedOp::RemoveTracks {
                    playlist_id: plan.playlist_id.clone(),
                    playlist_title: plan.playlist_title.clone(),
                    tracks: selected_removals,
                });
                *toast = Some(ToastState::staged(
                    format!(
                        "Staged {count} duplicate removals for '{}' in Sync.",
                        plan.playlist_title
                    ),
                    Some(eid),
                ));
            }
            if !plan_is_fresh {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 180, 70),
                    "Parameters or draft changed — re-scan to stage.",
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
        ui.label("Run Scan Duplicates to preview clusters before touching YouTube Music.");
        return actions;
    };

    ui.label(format!(
        "Clusters: {} | Removals: {} | Skipped (missing setVideoId): {}",
        plan.clusters.len(),
        plan.total_removals,
        plan.skipped_without_set_id
    ));

    let mut flat_rows = Vec::new();
    for (cidx, c) in plan.clusters.iter().enumerate() {
        flat_rows.push((cidx, true, "KEEP", &c.keeper, c.reason.clone()));
        for r in &c.removals {
            flat_rows.push((cidx, false, "REMOVE", r, format!("sim {:.2}", c.similarity)));
        }
    }

    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .column(Column::exact(28.0))
        .column(Column::exact(28.0))
        .column(Column::exact(68.0))
        .column(Column::initial(220.0).clip(true))
        .column(Column::initial(160.0).clip(true))
        .column(Column::exact(50.0))
        .column(Column::remainder())
        .header(22.0, |mut header| {
            for h in ["Use", "", "Action", "Title", "Artist", "Type", "Reason"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|body| {
            body.rows(22.0, flat_rows.len(), |mut row| {
                let (cidx, is_keeper, action, track, reason) = &flat_rows[row.index()];
                let color = if *is_keeper {
                    egui::Color32::from_rgb(90, 200, 120)
                } else {
                    egui::Color32::from_rgb(235, 100, 100)
                };
                row.col(|ui| {
                    if *is_keeper {
                        let mut include = !excluded.contains(cidx);
                        if ui.checkbox(&mut include, "").changed() {
                            if include {
                                excluded.remove(cidx);
                            } else {
                                excluded.insert(*cidx);
                            }
                        }
                    }
                });
                row.col(|ui| {
                    if ui.small_button("▶").on_hover_text("Audition track").clicked() {
                        actions.push(PlayAction::PlayFromList {
                            tracks: vec![(*track).clone()],
                            start_idx: 0,
                        });
                    }
                });
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

    ui.ctx()
        .memory_mut(|m| m.data.insert_temp(excluded_id, excluded));
    actions
}
