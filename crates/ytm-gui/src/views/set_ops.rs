use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::{FetchManifest, PlaylistAccess, PlaylistPrivacy, PlaylistSummary};
use ytm_ops::{DiffPlan, MatchMode, SetAlgebraOp};

use crate::navigation::ToastState;
use crate::playback_ops::PlayAction;
use crate::state::{AsyncState, DragTrackPayload, SetOpsForm, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::async_banner;
use crate::views::components::{
    hash_form, load_plan_fingerprint, make_fingerprint, store_plan_fingerprint,
};

const FP_SALT: &str = "set_ops_plan_fp";
const TITLE_SALT: &str = "set_ops_new_title";

fn playlist_picker(
    ui: &mut egui::Ui,
    id_salt: &str,
    label: &str,
    value: &mut String,
    playlists: &[PlaylistSummary],
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let selected_label = playlists
            .iter()
            .find(|p| &p.id == value)
            .map_or_else(|| value.as_str(), |p| p.title.as_str());
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(if selected_label.is_empty() {
                "Select..."
            } else {
                selected_label
            })
            .show_ui(ui, |ui| {
                for pl in playlists {
                    ui.selectable_value(value, pl.id.clone(), format!("{} ({})", pl.title, pl.id));
                }
            });
    });
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    playlists: &[PlaylistSummary],
    manifests: &HashMap<String, FetchManifest>,
    form: &mut SetOpsForm,
    plan_state: &mut AsyncState<DiffPlan>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
    cmd_tx: &Sender<WorkerCmd>,
) -> Vec<PlayAction> {
    let mut actions = Vec::new();
    ui.horizontal(|ui| {
        ui.radio_value(&mut form.op, SetAlgebraOp::Diff, "Diff (A \\ B)");
        ui.radio_value(&mut form.op, SetAlgebraOp::Intersect, "Intersect (A ∩ B)");
        ui.radio_value(&mut form.op, SetAlgebraOp::Merge, "Merge (A ∪ B)");
        ui.checkbox(&mut form.fuzzy, "Fuzzy normalized");
    });

    playlist_picker(
        ui,
        "set_pl_a",
        "Playlist A:",
        &mut form.playlist_a,
        playlists,
    );
    playlist_picker(
        ui,
        "set_pl_b",
        "Playlist B:",
        &mut form.playlist_b,
        playlists,
    );
    playlist_picker(
        ui,
        "set_pl_target",
        "Target (--into, optional):",
        &mut form.target_playlist,
        playlists,
    );

    let title_id = egui::Id::new(TITLE_SALT);
    let mut custom_title: String = ui
        .ctx()
        .memory(|m| m.data.get_temp::<String>(title_id))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("New playlist name (optional):");
        ui.add(
            egui::TextEdit::singleline(&mut custom_title)
                .desired_width(240.0)
                .hint_text("Auto: Set Diff (N)"),
        );
    });
    ui.ctx()
        .memory_mut(|m| m.data.insert_temp(title_id, custom_title.clone()));

    let form_sig = hash_form(&(
        format!("{:?}", form.op),
        &form.playlist_a,
        &form.playlist_b,
        form.fuzzy,
        &form.target_playlist,
    ));
    let current_fp = make_fingerprint(&form.playlist_a, sync_queue.revision(), form_sig);
    let plan_is_fresh = load_plan_fingerprint(ui.ctx(), FP_SALT).as_ref() == Some(&current_fp);

    let target_editable = !form.target_playlist.is_empty()
        && playlists
            .iter()
            .any(|p| p.id == form.target_playlist && p.access() == PlaylistAccess::EditableOwned)
        && !manifests
            .get(&form.target_playlist)
            .is_some_and(|m| m.completeness.is_mutation_locked());

    ui.horizontal_wrapped(|ui| {
        let ready =
            !form.playlist_a.is_empty() && !form.playlist_b.is_empty() && !plan_state.is_loading();
        if ui
            .add_enabled(ready, egui::Button::new("Compute Set Plan (Dry-Run)"))
            .clicked()
        {
            store_plan_fingerprint(ui.ctx(), FP_SALT, current_fp.clone());
            *plan_state = AsyncState::Loading("Computing set operation...".to_string());
            let target = (!form.target_playlist.is_empty()).then(|| form.target_playlist.clone());
            let _ = cmd_tx.send(WorkerCmd::PlanSetOp {
                op: form.op,
                mode: if form.fuzzy {
                    MatchMode::FuzzyNormalized
                } else {
                    MatchMode::ExactVideoId
                },
                playlist_a: form.playlist_a.clone(),
                playlist_b: form.playlist_b.clone(),
                target,
            });
        }

        if let Some(plan) = plan_state.data()
            && !form.target_playlist.is_empty()
            && !plan.add_video_ids.is_empty()
        {
            if ui
                .add_enabled(
                    plan_is_fresh && target_editable,
                    egui::Button::new("Stage Insert to Sync"),
                )
                .clicked()
            {
                let target_title = playlists
                    .iter()
                    .find(|p| p.id == form.target_playlist)
                    .map_or_else(|| form.target_playlist.clone(), |p| p.title.clone());
                let want_ids: HashSet<&str> =
                    plan.add_video_ids.iter().map(String::as_str).collect();
                let mut seen = HashSet::new();
                let tracks: Vec<_> = plan
                    .result_tracks
                    .iter()
                    .filter(|t| {
                        want_ids.contains(t.video_id.as_str()) && seen.insert(t.video_id.clone())
                    })
                    .cloned()
                    .collect();
                let count = tracks.len();
                let eid = sync_queue.push(StagedOp::AddTracks {
                    playlist_id: form.target_playlist.clone(),
                    playlist_title: target_title.clone(),
                    tracks,
                    allow_duplicates: false,
                });
                *toast = Some(ToastState::staged(
                    format!("Staged +{count} tracks for '{target_title}' in Sync."),
                    Some(eid),
                ));
            }
            if !target_editable {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 100, 100),
                    "Target is read-only / LM / locked.",
                );
            }
        }

        if let Some(plan) = plan_state.data()
            && !plan.result_tracks.is_empty()
        {
            if ui
                .add_enabled(
                    plan_is_fresh,
                    egui::Button::new("Stage Result as New Playlist"),
                )
                .clicked()
            {
                let title = if custom_title.trim().is_empty() {
                    format!("Set {:?} ({})", form.op, plan.result_tracks.len())
                } else {
                    custom_title.trim().to_string()
                };
                let eid = sync_queue.push(StagedOp::CreatePlaylist {
                    title: title.clone(),
                    description: "Generated by ytm-gui Set Algebra".to_string(),
                    privacy: PlaylistPrivacy::Private,
                    tracks: plan.result_tracks.clone(),
                });
                *toast = Some(ToastState::staged(
                    format!("Staged new playlist '{title}' in Sync."),
                    Some(eid),
                ));
            }
            if !plan_is_fresh {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 180, 70),
                    "Inputs changed — re-compute to stage.",
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
        ui.label("Select Playlist A & B and click Compute Set Plan.");
        return actions;
    };

    ui.label(format!(
        "Result tracks: {} (drag ⋮⋮ onto any playlist)",
        plan.result_tracks.len()
    ));
    TableBuilder::new(ui)
        .id_salt("set_ops_tbl")
        .striped(true)
        .resizable(true)
        .column(Column::exact(24.0))
        .column(Column::exact(28.0))
        .column(Column::exact(42.0))
        .column(Column::initial(170.0).clip(true))
        .column(Column::remainder())
        .header(22.0, |mut header| {
            for h in ["", "", "#", "Artist", "Title"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|body| {
            body.rows(22.0, plan.result_tracks.len(), |mut row| {
                let idx = row.index();
                let t = &plan.result_tracks[idx];
                row.col(|ui| {
                    let drag = ui
                        .dnd_drag_source(
                            egui::Id::new(("setop_row", idx, &t.video_id)),
                            DragTrackPayload {
                                source_playlist_id: "__SET_OP__".to_string(),
                                tracks: vec![t.clone()],
                            },
                            |ui| ui.small("⋮⋮"),
                        )
                        .response;
                    if drag.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }
                });
                row.col(|ui| {
                    if ui.small_button("▶").on_hover_text("Play track").clicked() {
                        actions.push(PlayAction::PlayFromList {
                            tracks: plan.result_tracks.clone(),
                            start_idx: idx,
                        });
                    }
                });
                row.col(|ui| {
                    ui.label((idx + 1).to_string());
                });
                row.col(|ui| {
                    ui.label(t.primary_artist_name().unwrap_or("-"));
                });
                row.col(|ui| {
                    ui.label(&t.title);
                });
            });
        });
    actions
}
