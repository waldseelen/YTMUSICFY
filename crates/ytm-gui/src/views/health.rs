use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::Playlist;
use ytm_ops::HealthReport;

use crate::state::{AsyncState, ConfirmedMutation, HealthForm, PendingMutation, WorkerCmd};
use crate::views::async_banner;

pub fn show(
    ui: &mut egui::Ui,
    active_playlist: Option<&Playlist>,
    selected_playlist_id: Option<&str>,
    form: &mut HealthForm,
    report_state: &mut AsyncState<HealthReport>,
    pending_modal: &mut Option<PendingMutation>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut form.find_official, "Search official ATV replacements");
        ui.checkbox(&mut form.prune_unavailable, "Prune unavailable");
        ui.checkbox(&mut form.upgrade, "Upgrade UGC/OMV -> ATV");
    });

    ui.horizontal(|ui| {
        let can_scan = selected_playlist_id.is_some() && !report_state.is_loading();
        if ui
            .add_enabled(can_scan, egui::Button::new("Scan Playlist Health"))
            .clicked()
            && let Some(pid) = selected_playlist_id
        {
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
            && ui.button("Apply Prune / Upgrades...").clicked()
        {
            *pending_modal = Some(PendingMutation {
                title: "Confirm Health Cleanup".to_string(),
                summary: format!(
                    "Save pre-health snapshot and apply cleanup on '{}' (issues={}, upgrades={})?",
                    report.playlist_title,
                    report.issues.len(),
                    report.upgrade_candidates.len()
                ),
                action: ConfirmedMutation::Health {
                    playlist: pl.clone(),
                    prune_unavailable: form.prune_unavailable,
                    upgrade: form.upgrade,
                    report: report.clone(),
                },
            });
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
        return;
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
            .column(Column::exact(120.0))
            .column(Column::initial(220.0).clip(true))
            .column(Column::remainder())
            .header(20.0, |mut h| {
                for name in ["VideoID", "Title", "Issue"] {
                    h.col(|ui| {
                        ui.strong(name);
                    });
                }
            })
            .body(|body| {
                body.rows(20.0, report.issues.len(), |mut row| {
                    let issue = &report.issues[row.index()];
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
            .column(Column::initial(170.0).clip(true))
            .column(Column::initial(170.0).clip(true))
            .column(Column::remainder())
            .header(20.0, |mut h| {
                for name in ["Source (UGC/OMV)", "Official ATV", "Sim"] {
                    h.col(|ui| {
                        ui.strong(name);
                    });
                }
            })
            .body(|body| {
                body.rows(20.0, report.upgrade_candidates.len(), |mut row| {
                    let c = &report.upgrade_candidates[row.index()];
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
}
