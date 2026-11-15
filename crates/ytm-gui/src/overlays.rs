use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, SessionStatus};
use ytm_store::ExportFormat;

use crate::state::{AsyncState, ConfirmedMutation, DropPrompt, PendingMutation, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};

pub fn show_top_bar(
    ui: &mut egui::Ui,
    session: &AsyncState<SessionStatus>,
    library: &mut AsyncState<Vec<PlaylistSummary>>,
    export_fmt: ExportFormat,
    export_dir: &str,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.horizontal(|ui| {
        ui.strong("ytm-architect");
        ui.separator();
        match session {
            AsyncState::Ready(s) => {
                ui.colored_label(
                    egui::Color32::from_rgb(90, 200, 120),
                    format!(
                        "{} ({})",
                        s.account_name.as_deref().unwrap_or("Signed in"),
                        s.channel_handle.as_deref().unwrap_or("-")
                    ),
                );
            }
            AsyncState::Loading(m) => {
                ui.spinner();
                ui.label(m);
            }
            AsyncState::Failed(_) => {
                ui.colored_label(egui::Color32::from_rgb(235, 160, 60), "Not authenticated");
            }
            AsyncState::Idle => {}
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Backup All").clicked() {
                let _ = cmd_tx.send(WorkerCmd::BackupPlaylist {
                    target: "all".to_string(),
                    format: export_fmt,
                    out_dir: PathBuf::from(export_dir.trim()),
                });
            }
            if ui.button("Refresh Library").clicked() {
                *library = AsyncState::Loading("Refreshing library...".to_string());
                let _ = cmd_tx.send(WorkerCmd::RefreshLibrary);
            }
        });
    });
}

pub fn show_drop_prompt(
    ctx: &egui::Context,
    drop_prompt: &mut Option<DropPrompt>,
    playlist_cache: &HashMap<String, Playlist>,
    library: &AsyncState<Vec<PlaylistSummary>>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<(bool, String)>,
) {
    let Some(prompt) = drop_prompt.clone() else {
        return;
    };
    let src_can_remove = playlist_cache
        .get(&prompt.source_playlist_id)
        .is_some_and(|p| p.owned && !p.id.eq_ignore_ascii_case("LM"));
    let target_editable = library
        .data()
        .and_then(|list| list.iter().find(|p| p.id == prompt.target_playlist_id))
        .is_some_and(|p| p.owned && !p.id.eq_ignore_ascii_case("LM"));

    egui::Area::new(egui::Id::new("dnd_copy_move_popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(prompt.pos + egui::vec2(8.0, 8.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.strong(format!(
                    "Drop {} track(s) -> {}",
                    prompt.tracks.len(),
                    prompt.target_playlist_title
                ));
                ui.separator();
                if ui
                    .add_enabled(
                        target_editable,
                        egui::Button::new("Copy Here (Keep in Source)"),
                    )
                    .clicked()
                {
                    let count = prompt.tracks.len();
                    sync_queue.push(StagedOp::AddTracks {
                        playlist_id: prompt.target_playlist_id.clone(),
                        playlist_title: prompt.target_playlist_title.clone(),
                        tracks: prompt.tracks.clone(),
                        allow_duplicates: false,
                    });
                    *toast = Some((
                        true,
                        format!(
                            "Staged Copy (+{count}) into '{}' in Sync.",
                            prompt.target_playlist_title
                        ),
                    ));
                    *drop_prompt = None;
                }
                if ui
                    .add_enabled(
                        target_editable && src_can_remove,
                        egui::Button::new("Move Here (Copy + Remove Source)"),
                    )
                    .clicked()
                {
                    let count = prompt.tracks.len();
                    sync_queue.push(StagedOp::AddTracks {
                        playlist_id: prompt.target_playlist_id.clone(),
                        playlist_title: prompt.target_playlist_title.clone(),
                        tracks: prompt.tracks.clone(),
                        allow_duplicates: false,
                    });
                    sync_queue.push(StagedOp::RemoveTracks {
                        playlist_id: prompt.source_playlist_id.clone(),
                        playlist_title: prompt.source_playlist_title.clone(),
                        tracks: prompt.tracks.clone(),
                    });
                    *toast = Some((
                        true,
                        format!(
                            "Staged Move ({count} track(s) {} -> {}) in Sync.",
                            prompt.source_playlist_title, prompt.target_playlist_title
                        ),
                    ));
                    *drop_prompt = None;
                }
                if ui.button("Cancel").clicked() {
                    *drop_prompt = None;
                }
            });
        });
}

pub fn show_confirmation_modal(
    ctx: &egui::Context,
    pending_modal: &mut Option<PendingMutation>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    let Some(pending) = pending_modal.clone() else {
        return;
    };
    egui::Window::new(&pending.title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(&pending.summary);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Confirm & Execute").clicked() {
                    let action = match &pending.action {
                        ConfirmedMutation::ExecuteSync { entries } => {
                            ConfirmedMutation::ExecuteSync {
                                entries: entries.clone(),
                            }
                        }
                        ConfirmedMutation::RestoreSnapshot { snapshot_id } => {
                            ConfirmedMutation::RestoreSnapshot {
                                snapshot_id: snapshot_id.clone(),
                            }
                        }
                    };
                    let _ = cmd_tx.send(WorkerCmd::ExecuteMutation(action));
                    *pending_modal = None;
                }
                if ui.button("Cancel").clicked() {
                    *pending_modal = None;
                }
            });
        });
}
