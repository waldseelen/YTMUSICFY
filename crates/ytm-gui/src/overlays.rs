use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, SessionStatus, Track};
use ytm_store::ExportFormat;

use crate::interaction::status_badge;
use crate::navigation::{NavHistory, ToastState};
use crate::state::{AsyncState, ConfirmedMutation, DropPrompt, GuiTab, PendingMutation, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};

#[allow(clippy::too_many_arguments)]
pub fn show_top_bar(
    ui: &mut egui::Ui,
    session: &AsyncState<SessionStatus>,
    library: &mut AsyncState<Vec<PlaylistSummary>>,
    active_tab: &mut GuiTab,
    selected_playlist_id: &mut Option<String>,
    nav_history: &mut NavHistory,
    ui_scale: &mut f32,
    export_fmt: ExportFormat,
    export_dir: &str,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(nav_history.can_back(), egui::Button::new("◀"))
            .on_hover_text("Back (Alt+Left / Mouse Back)")
            .clicked()
            && let Some(entry) = nav_history.go_back()
        {
            *active_tab = entry.tab;
            *selected_playlist_id = entry.playlist_id;
        }
        if ui
            .add_enabled(nav_history.can_forward(), egui::Button::new("▶"))
            .on_hover_text("Forward (Alt+Right / Mouse Forward)")
            .clicked()
            && let Some(entry) = nav_history.go_forward()
        {
            *active_tab = entry.tab;
            *selected_playlist_id = entry.playlist_id;
        }
        ui.strong("ytm-architect");
        ui.separator();
        match session {
            AsyncState::Ready(s) => {
                ui.colored_label(
                    egui::Color32::from_rgb(90, 200, 120),
                    status_badge(
                        true,
                        &format!(
                            "{} ({})",
                            s.account_name.as_deref().unwrap_or("Signed in"),
                            s.channel_handle.as_deref().unwrap_or("-")
                        ),
                    ),
                );
            }
            AsyncState::Loading(m) => {
                ui.spinner();
                ui.label(m);
            }
            AsyncState::Failed(_) => {
                ui.colored_label(egui::Color32::from_rgb(235, 160, 60), "⚠ Not authenticated");
                if ui.small_button("Go to Auth →").clicked() {
                    *active_tab = GuiTab::Auth;
                }
            }
            AsyncState::Idle => {}
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Backup All")
                .on_hover_text("Export all playlists + SQLite snapshots")
                .clicked()
            {
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
            if (*ui_scale - 1.0).abs() > 0.01
                && ui
                    .small_button(format!("{}%", (*ui_scale * 100.0).round()))
                    .on_hover_text("Reset UI zoom (Ctrl+0)")
                    .clicked()
            {
                *ui_scale = 1.0;
            }
        });
    });
}

fn check_drop_permissions(
    source_id: &str,
    target_id: &str,
    playlist_cache: &HashMap<String, Playlist>,
    library: &AsyncState<Vec<PlaylistSummary>>,
) -> (bool, bool) {
    let src_can_remove = playlist_cache
        .get(source_id)
        .is_some_and(|p| p.owned && !p.id.eq_ignore_ascii_case("LM"));
    let target_editable = library
        .data()
        .and_then(|list| list.iter().find(|p| p.id == target_id))
        .is_some_and(|p| p.owned && !p.id.eq_ignore_ascii_case("LM"));
    (target_editable, src_can_remove)
}

fn stage_copy_tracks(
    sync_queue: &mut SyncQueue,
    target_id: &str,
    target_title: &str,
    tracks: Vec<Track>,
    toast: &mut Option<ToastState>,
) {
    let count = tracks.len();
    let entry_id = sync_queue.push(StagedOp::AddTracks {
        playlist_id: target_id.to_string(),
        playlist_title: target_title.to_string(),
        tracks,
        allow_duplicates: false,
    });
    *toast = Some(ToastState::staged(
        format!("Staged Copy (+{count}) into '{target_title}' in Sync."),
        Some(entry_id),
    ));
}

fn stage_move_tracks(
    sync_queue: &mut SyncQueue,
    source_id: &str,
    source_title: &str,
    target_id: &str,
    target_title: &str,
    tracks: Vec<Track>,
    toast: &mut Option<ToastState>,
) {
    let count = tracks.len();
    sync_queue.push(StagedOp::AddTracks {
        playlist_id: target_id.to_string(),
        playlist_title: target_title.to_string(),
        tracks: tracks.clone(),
        allow_duplicates: false,
    });
    let entry_id = sync_queue.push(StagedOp::RemoveTracks {
        playlist_id: source_id.to_string(),
        playlist_title: source_title.to_string(),
        tracks,
    });
    *toast = Some(ToastState::staged(
        format!("Staged Move ({count} track(s) {source_title} -> {target_title}) in Sync."),
        Some(entry_id),
    ));
}

/// Immediately stages Copy (`Ctrl`) or Move (`Shift`) on drop, or opens `DropPrompt` popup.
#[allow(clippy::too_many_arguments)]
pub fn handle_cross_playlist_drop(
    ctx: &egui::Context,
    prompt: DropPrompt,
    drop_prompt: &mut Option<DropPrompt>,
    playlist_cache: &HashMap<String, Playlist>,
    library: &AsyncState<Vec<PlaylistSummary>>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
) {
    let (target_editable, src_can_remove) = check_drop_permissions(
        &prompt.source_playlist_id,
        &prompt.target_playlist_id,
        playlist_cache,
        library,
    );
    let (ctrl, shift) = ctx.input(|i| (i.modifiers.ctrl || i.modifiers.mac_cmd, i.modifiers.shift));
    if ctrl && target_editable {
        stage_copy_tracks(
            sync_queue,
            &prompt.target_playlist_id,
            &prompt.target_playlist_title,
            prompt.tracks,
            toast,
        );
        return;
    }
    if shift && target_editable && src_can_remove {
        stage_move_tracks(
            sync_queue,
            &prompt.source_playlist_id,
            &prompt.source_playlist_title,
            &prompt.target_playlist_id,
            &prompt.target_playlist_title,
            prompt.tracks,
            toast,
        );
        return;
    }
    *drop_prompt = Some(prompt);
}

pub fn show_drop_prompt(
    ctx: &egui::Context,
    drop_prompt: &mut Option<DropPrompt>,
    playlist_cache: &HashMap<String, Playlist>,
    library: &AsyncState<Vec<PlaylistSummary>>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
) {
    let Some(prompt) = drop_prompt.clone() else {
        return;
    };
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *drop_prompt = None;
        return;
    }
    let (target_editable, src_can_remove) = check_drop_permissions(
        &prompt.source_playlist_id,
        &prompt.target_playlist_id,
        playlist_cache,
        library,
    );

    let area_resp = egui::Area::new(egui::Id::new("dnd_copy_move_popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(prompt.pos + egui::vec2(8.0, 8.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.strong(format!(
                    "Drop {} track(s) -> {}",
                    prompt.tracks.len(),
                    prompt.target_playlist_title
                ));
                ui.small("Tip: hold Ctrl=Copy or Shift=Move on drop");
                ui.separator();
                if ui
                    .add_enabled(
                        target_editable,
                        egui::Button::new("Copy Here (Keep in Source)"),
                    )
                    .clicked()
                {
                    stage_copy_tracks(
                        sync_queue,
                        &prompt.target_playlist_id,
                        &prompt.target_playlist_title,
                        prompt.tracks.clone(),
                        toast,
                    );
                    *drop_prompt = None;
                }
                if ui
                    .add_enabled(
                        target_editable && src_can_remove,
                        egui::Button::new("Move Here (Copy + Remove Source)"),
                    )
                    .clicked()
                {
                    stage_move_tracks(
                        sync_queue,
                        &prompt.source_playlist_id,
                        &prompt.source_playlist_title,
                        &prompt.target_playlist_id,
                        &prompt.target_playlist_title,
                        prompt.tracks.clone(),
                        toast,
                    );
                    *drop_prompt = None;
                }
                if ui.button("Cancel (Esc)").clicked() {
                    *drop_prompt = None;
                }
            });
        })
        .response;

    if ctx.input(|i| i.pointer.any_pressed())
        && let Some(pos) = ctx.pointer_interact_pos()
        && !area_resp.rect.contains(pos)
    {
        *drop_prompt = None;
    }
}

pub fn show_confirmation_modal(
    ctx: &egui::Context,
    pending_modal: &mut Option<PendingMutation>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    let Some(pending) = pending_modal.clone() else {
        return;
    };
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *pending_modal = None;
        return;
    }
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("modal_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.allocate_response(screen.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(screen, 0.0, egui::Color32::from_black_alpha(120));
        });

    let confirm_pressed = ctx.input(|i| {
        i.key_pressed(egui::Key::Enter)
            && !i.modifiers.ctrl
            && !i.modifiers.mac_cmd
            && !i.modifiers.alt
    });
    let mut execute = confirm_pressed;
    egui::Window::new(&pending.title)
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(&pending.summary);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Confirm & Execute (Enter)").clicked() {
                    execute = true;
                }
                if ui.button("Cancel (Esc)").clicked() {
                    *pending_modal = None;
                }
            });
        });
    if execute {
        let action = match &pending.action {
            ConfirmedMutation::ExecuteSync { entries } => ConfirmedMutation::ExecuteSync {
                entries: entries.clone(),
            },
            ConfirmedMutation::RestoreSnapshot { snapshot_id } => {
                ConfirmedMutation::RestoreSnapshot {
                    snapshot_id: snapshot_id.clone(),
                }
            }
        };
        let _ = cmd_tx.send(WorkerCmd::ExecuteMutation(action));
        *pending_modal = None;
    }
}
