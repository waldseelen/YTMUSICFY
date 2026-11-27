use std::collections::HashMap;
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, Track};

use crate::navigation::ToastState;
use crate::state::{
    AsyncState, ConfirmedMutation, DragDropMode, DropPrompt, PendingMutation, WorkerCmd,
};
use crate::sync_queue::{StagedOp, SyncQueue};

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
    let entry_id = sync_queue.push(StagedOp::MoveTracks {
        source_playlist_id: source_id.to_string(),
        source_playlist_title: source_title.to_string(),
        target_playlist_id: target_id.to_string(),
        target_playlist_title: target_title.to_string(),
        tracks,
    });
    *toast = Some(ToastState::staged(
        format!("Staged Move ({count} track(s) {source_title} -> {target_title}) in Sync."),
        Some(entry_id),
    ));
}

/// Immediately stages Copy (`Ctrl`) or Move (`Shift`) or applies `drag_mode`, else opens `DropPrompt`.
#[allow(clippy::too_many_arguments)]
pub fn handle_cross_playlist_drop(
    ctx: &egui::Context,
    prompt: DropPrompt,
    drag_mode: DragDropMode,
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
    let want_copy = ctrl || (!shift && drag_mode == DragDropMode::Copy);
    let want_move = shift || (!ctrl && drag_mode == DragDropMode::Move);

    if want_copy && target_editable {
        stage_copy_tracks(
            sync_queue,
            &prompt.target_playlist_id,
            &prompt.target_playlist_title,
            prompt.tracks,
            toast,
        );
        return;
    }
    if want_move && target_editable && src_can_remove {
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
    let typed_id = egui::Id::new("delete_confirm_title");
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ctx.memory_mut(|m| m.data.remove::<String>(typed_id));
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

    let required_title = match &pending.action {
        ConfirmedMutation::DeletePlaylist { playlist_title, .. } => Some(playlist_title.as_str()),
        _ => None,
    };
    let mut typed = ctx.memory_mut(|m| m.data.get_temp::<String>(typed_id).unwrap_or_default());
    let mut execute = false;
    let mut cancel = false;
    egui::Window::new(&pending.title)
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(&pending.summary);
            if let Some(title) = required_title {
                ui.add_space(6.0);
                ui.label(format!("Type '{title}' to confirm permanent deletion:"));
                ui.add(egui::TextEdit::singleline(&mut typed).desired_width(280.0));
            }
            ui.add_space(10.0);
            let can_confirm = required_title.is_none_or(|t| typed.trim() == t.trim());
            ui.horizontal(|ui| {
                let btn_label = if required_title.is_some() {
                    "Confirm & Delete"
                } else {
                    "Confirm & Execute"
                };
                if ui
                    .add_enabled(can_confirm, egui::Button::new(btn_label))
                    .clicked()
                {
                    execute = true;
                }
                if ui.button("Cancel (Esc)").clicked() {
                    cancel = true;
                }
            });
        });
    if execute {
        ctx.memory_mut(|m| m.data.remove::<String>(typed_id));
        let _ = cmd_tx.send(WorkerCmd::ExecuteMutation(pending.action));
        *pending_modal = None;
    } else if cancel {
        ctx.memory_mut(|m| m.data.remove::<String>(typed_id));
        *pending_modal = None;
    } else if required_title.is_some() {
        ctx.memory_mut(|m| m.data.insert_temp(typed_id, typed));
    }
}

#[cfg(test)]
#[path = "overlays_tests.rs"]
mod tests;
