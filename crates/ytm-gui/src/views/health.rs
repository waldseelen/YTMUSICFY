use std::collections::HashSet;
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::{FetchManifest, Playlist, PlaylistAccess};
use ytm_ops::{HealthIssueKind, HealthReport};

use crate::icons::{Icon, icon_text_button};
use crate::navigation::ToastState;
use crate::playback_ops::PlayAction;
use crate::state::{AsyncState, HealthForm, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::async_banner;
use crate::views::components::{
    hash_form, load_plan_fingerprint, make_fingerprint, store_plan_fingerprint,
};

const FP_SALT: &str = "health_plan_fp";
const ISSUE_EXCL_SALT: &str = "health_issue_excl";
const UPG_EXCL_SALT: &str = "health_upg_excl";
const PRESERVE_POS_SALT: &str = "health_preserve_pos";

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    active_playlist: Option<&Playlist>,
    manifest: Option<&FetchManifest>,
    selected_playlist_id: Option<&str>,
    form: &mut HealthForm,
    report_state: &mut AsyncState<HealthReport>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
    cmd_tx: &Sender<WorkerCmd>,
) -> Vec<PlayAction> {
    let mut actions = Vec::new();
    let pos_id = egui::Id::new(PRESERVE_POS_SALT);
    let mut preserve_pos = ui
        .ctx()
        .memory(|m| m.data.get_temp::<bool>(pos_id).unwrap_or(true));

    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut form.find_official, "Search official ATV replacements");
        ui.checkbox(&mut form.prune_unavailable, "Prune unavailable");
        ui.checkbox(&mut form.upgrade, "Upgrade UGC/OMV -> ATV");
        ui.checkbox(&mut preserve_pos, "Preserve track position");
    });
    ui.ctx()
        .memory_mut(|m| m.data.insert_temp(pos_id, preserve_pos));

    let form_sig = hash_form(&form.find_official);
    let current_fp =
        selected_playlist_id.map(|pid| make_fingerprint(pid, sync_queue.revision(), form_sig));
    let stored_fp = load_plan_fingerprint(ui.ctx(), FP_SALT);
    let plan_is_fresh = current_fp.is_some() && current_fp == stored_fp;

    let manifest_locked = manifest.is_some_and(|m| m.completeness.is_mutation_locked());
    let editable = active_playlist.is_some_and(|p| p.access() == PlaylistAccess::EditableOwned)
        && !manifest_locked;

    let issue_excl_id = egui::Id::new(ISSUE_EXCL_SALT);
    let upg_excl_id = egui::Id::new(UPG_EXCL_SALT);
    let mut issue_excl: HashSet<usize> = ui
        .ctx()
        .memory(|m| m.data.get_temp::<HashSet<usize>>(issue_excl_id))
        .unwrap_or_default();
    let mut upg_excl: HashSet<usize> = ui
        .ctx()
        .memory(|m| m.data.get_temp::<HashSet<usize>>(upg_excl_id))
        .unwrap_or_default();

    ui.horizontal_wrapped(|ui| {
        let can_scan = selected_playlist_id.is_some() && !report_state.is_loading();
        if ui
            .add_enabled(can_scan, egui::Button::new("Scan Playlist Health"))
            .clicked()
            && let Some(pid) = selected_playlist_id
        {
            if let Some(fp) = &current_fp {
                store_plan_fingerprint(ui.ctx(), FP_SALT, fp.clone());
            }
            issue_excl.clear();
            upg_excl.clear();
            *report_state = AsyncState::Loading("Scanning playlist health...".to_string());
            let _ = cmd_tx.send(WorkerCmd::ScanHealth {
                playlist_id: pid.to_string(),
                find_official: form.find_official,
            });
        }

        if let Some(report) = report_state.data()
            && let Some(pl) = active_playlist
            && pl.id == report.playlist_id
            && (form.prune_unavailable || form.upgrade)
            && (!report.issues.is_empty() || !report.upgrade_candidates.is_empty())
        {
            let can_stage = editable && plan_is_fresh;
            if ui
                .add_enabled(
                    can_stage,
                    egui::Button::new("Stage Selected Cleanup / Upgrades to Sync"),
                )
                .clicked()
            {
                let mut staged = 0usize;
                let mut last_id = None;
                if form.prune_unavailable {
                    let removals: Vec<_> = report
                        .issues
                        .iter()
                        .enumerate()
                        .filter(|(idx, i)| {
                            !issue_excl.contains(idx)
                                && !matches!(i.kind, HealthIssueKind::MissingSetVideoId)
                                && i.track.set_video_id.is_some()
                        })
                        .map(|(_, i)| i.track.clone())
                        .collect();
                    if !removals.is_empty() {
                        last_id = Some(sync_queue.push(StagedOp::RemoveTracks {
                            playlist_id: pl.id.clone(),
                            playlist_title: pl.title.clone(),
                            tracks: removals,
                        }));
                        staged += 1;
                    }
                }
                if form.upgrade && !report.upgrade_candidates.is_empty() {
                    let upgrades: Vec<_> = report
                        .upgrade_candidates
                        .iter()
                        .enumerate()
                        .filter(|(idx, c)| {
                            !upg_excl.contains(idx) && c.source_track.set_video_id.is_some()
                        })
                        .map(|(_, c)| (c.source_track.clone(), c.official_atv.clone()))
                        .collect();
                    if !upgrades.is_empty() {
                        last_id = Some(sync_queue.push(StagedOp::UpgradeTracks {
                            playlist_id: pl.id.clone(),
                            playlist_title: pl.title.clone(),
                            upgrades,
                            preserve_position: preserve_pos,
                        }));
                        staged += 1;
                    }
                }
                if staged > 0 {
                    *toast = Some(ToastState::staged(
                        format!(
                            "Staged {staged} health action(s) for '{}' in Sync.",
                            pl.title
                        ),
                        last_id,
                    ));
                }
            }
            if !plan_is_fresh {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 180, 70),
                    "Scan stale — re-scan to stage.",
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

    let (loading, error) = match report_state {
        AsyncState::Loading(m) => (Some(m.as_str()), None),
        AsyncState::Failed(e) => (None, Some(e.as_str())),
        _ => (None, None),
    };
    async_banner(ui, loading, error);
    ui.separator();

    let Some(report) = report_state.data() else {
        ui.label("Run Scan Playlist Health to detect deleted/private tracks and ATV upgrades.");
        return actions;
    };

    ui.label(format!(
        "Checked: {} | Issues: {} | Official ATV Upgrades: {}",
        report.total_tracks,
        report.issues.len(),
        report.upgrade_candidates.len()
    ));

    if !report.issues.is_empty() {
        ui.strong("Health Issues");
        TableBuilder::new(ui)
            .id_salt("health_issues_tbl")
            .striped(true)
            .column(Column::exact(28.0))
            .column(Column::exact(120.0))
            .column(Column::initial(220.0).clip(true))
            .column(Column::remainder())
            .header(20.0, |mut h| {
                for name in ["Use", "VideoID", "Title", "Issue"] {
                    h.col(|ui| {
                        ui.strong(name);
                    });
                }
            })
            .body(|body| {
                body.rows(20.0, report.issues.len(), |mut row| {
                    let idx = row.index();
                    let issue = &report.issues[idx];
                    row.col(|ui| {
                        let mut include = !issue_excl.contains(&idx);
                        if ui.checkbox(&mut include, "").changed() {
                            if include {
                                issue_excl.remove(&idx);
                            } else {
                                issue_excl.insert(idx);
                            }
                        }
                    });
                    row.col(|ui| {
                        ui.label(&issue.track.video_id);
                    });
                    row.col(|ui| {
                        ui.label(&issue.track.title);
                    });
                    row.col(|ui| {
                        ui.label(format!("{:?}", issue.kind));
                    });
                });
            });
    }

    if !report.upgrade_candidates.is_empty() {
        ui.add_space(6.0);
        ui.strong("Official ATV Upgrade Candidates");
        TableBuilder::new(ui)
            .id_salt("health_upgrades_tbl")
            .striped(true)
            .column(Column::exact(28.0))
            .column(Column::exact(52.0))
            .column(Column::initial(170.0).clip(true))
            .column(Column::initial(170.0).clip(true))
            .column(Column::remainder())
            .header(20.0, |mut h| {
                for name in ["Use", "Play", "Source (UGC/OMV)", "Official ATV", "Sim"] {
                    h.col(|ui| {
                        ui.strong(name);
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, report.upgrade_candidates.len(), |mut row| {
                    let idx = row.index();
                    let c = &report.upgrade_candidates[idx];
                    row.col(|ui| {
                        let mut include = !upg_excl.contains(&idx);
                        if ui.checkbox(&mut include, "").changed() {
                            if include {
                                upg_excl.remove(&idx);
                            } else {
                                upg_excl.insert(idx);
                            }
                        }
                    });
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            if icon_text_button(ui, Icon::Play, "1", 11.0, true)
                                .on_hover_text("Play source UGC/OMV")
                                .clicked()
                            {
                                actions.push(PlayAction::PlayFromList {
                                    tracks: vec![c.source_track.clone()],
                                    start_idx: 0,
                                });
                            }
                            if icon_text_button(ui, Icon::Play, "2", 11.0, true)
                                .on_hover_text("Play Official ATV")
                                .clicked()
                            {
                                actions.push(PlayAction::PlayFromList {
                                    tracks: vec![c.official_atv.clone()],
                                    start_idx: 0,
                                });
                            }
                        });
                    });
                    row.col(|ui| {
                        ui.label(&c.source_track.title);
                    });
                    row.col(|ui| {
                        ui.label(&c.official_atv.title);
                    });
                    row.col(|ui| {
                        ui.label(format!("{:.2}", c.similarity));
                    });
                });
            });
    }

    ui.ctx()
        .memory_mut(|m| m.data.insert_temp(issue_excl_id, issue_excl));
    ui.ctx()
        .memory_mut(|m| m.data.insert_temp(upg_excl_id, upg_excl));
    actions
}
