use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistPrivacy, PlaylistSummary};

use crate::interaction::{auto_scroll_on_drag, search_box_with_clear};
use crate::navigation::ToastState;
use crate::overlays::handle_cross_playlist_drop;
use crate::state::{
    AsyncState, DragTrackPayload, DropPrompt, GuiTab, SetOpsForm, SplitPaneState, WorkerCmd,
};
use crate::sync_queue::{StagedOp, SyncQueue};

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    library: &AsyncState<Vec<PlaylistSummary>>,
    sidebar_filter: &mut String,
    new_playlist_title: &mut String,
    selected_playlist_id: &mut Option<String>,
    loading_playlists: &mut HashSet<String>,
    playlist_cache: &mut HashMap<String, Playlist>,
    sync_queue: &mut SyncQueue,
    split_state: &mut SplitPaneState,
    set_form: &mut SetOpsForm,
    active_tab: &mut GuiTab,
    drop_prompt: &mut Option<DropPrompt>,
    toast: &mut Option<ToastState>,
    focus_filter: bool,
    focused_idx: &mut Option<usize>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.strong("Library Playlists");
    search_box_with_clear(
        ui,
        "sidebar_filter_box",
        sidebar_filter,
        "Filter playlists...",
        ui.available_width() - 28.0,
        focus_filter,
    );
    ui.horizontal(|ui| {
        let new_resp = ui.add(
            egui::TextEdit::singleline(new_playlist_title)
                .hint_text("New playlist title...")
                .desired_width(145.0),
        );
        let enter_create = new_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let title = new_playlist_title.trim().to_string();
        if !title.is_empty()
            && (enter_create || ui.add_enabled(true, egui::Button::new("+ Stage")).clicked())
        {
            let eid = sync_queue.push(StagedOp::CreatePlaylist {
                title: title.clone(),
                description: "Created by ytm-gui".to_string(),
                privacy: PlaylistPrivacy::Private,
                tracks: Vec::new(),
            });
            *toast = Some(ToastState::staged(
                format!("Staged new playlist '{title}' in Sync."),
                Some(eid),
            ));
            new_playlist_title.clear();
        } else if title.is_empty() {
            ui.add_enabled(false, egui::Button::new("+ Stage"));
        }
    });
    ui.separator();

    if let AsyncState::Loading(m) = library {
        ui.spinner();
        ui.label(m);
        return;
    }
    if let AsyncState::Failed(err) = library {
        ui.colored_label(egui::Color32::from_rgb(235, 95, 95), err);
        if ui.button("Retry Library").clicked() {
            let _ = cmd_tx.send(WorkerCmd::RefreshLibrary);
        }
        return;
    }
    let Some(playlists) = library.data() else {
        return;
    };

    let dirty_ids = sync_queue.affected_playlist_ids();
    let needle = sidebar_filter.trim().to_ascii_lowercase();
    let filtered: Vec<&PlaylistSummary> = playlists
        .iter()
        .filter(|pl| needle.is_empty() || pl.title.to_ascii_lowercase().contains(&needle))
        .collect();

    let sidebar_active = ui.rect_contains_pointer(ui.max_rect())
        || *active_tab != GuiTab::Inspector
        || selected_playlist_id.is_none();
    if sidebar_active && !ui.ctx().wants_keyboard_input() && !filtered.is_empty() {
        let (up, down, enter) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Enter),
            )
        });
        if down {
            *focused_idx = Some(focused_idx.map_or(0, |idx| (idx + 1).min(filtered.len() - 1)));
        } else if up {
            *focused_idx = Some(focused_idx.map_or(0, |idx| idx.saturating_sub(1)));
        }
        if enter
            && let Some(idx) = *focused_idx
            && let Some(pl) = filtered.get(idx)
        {
            select_playlist(
                pl.id.clone(),
                selected_playlist_id,
                playlist_cache,
                loading_playlists,
                cmd_tx,
            );
        }
    }

    let pointer_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
    let scroll_out = egui::ScrollArea::vertical().show(ui, |ui| {
        for (vis_idx, pl) in filtered.iter().enumerate() {
            let selected = selected_playlist_id.as_deref() == Some(pl.id.as_str());
            let kb_focused = *focused_idx == Some(vis_idx);
            let draft_count = sync_queue
                .working_playlist(&pl.id, playlist_cache)
                .map(|p| p.tracks.len())
                .or(pl.track_count);
            let count = draft_count.map_or_else(|| "?".to_string(), |c| c.to_string());
            let dirty_mark = if dirty_ids.contains(&pl.id) { "*" } else { "" };
            let loading_mark = if loading_playlists.contains(&pl.id) {
                " ..."
            } else {
                ""
            };
            let label = format!("{dirty_mark}{} ({count}){loading_mark}", pl.title);

            let resp = ui.selectable_label(selected || kb_focused, label);
            if kb_focused {
                resp.scroll_to_me(Some(egui::Align::Center));
            }
            if resp.dnd_hover_payload::<DragTrackPayload>().is_some() {
                ui.painter().rect_stroke(
                    resp.rect.expand(1.0),
                    2.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 175, 240)),
                    egui::StrokeKind::Inside,
                );
            }
            if let Some(payload) = resp.dnd_release_payload::<DragTrackPayload>()
                && payload.source_playlist_id != pl.id
            {
                let src_title = playlists
                    .iter()
                    .find(|p| p.id == payload.source_playlist_id)
                    .map_or_else(|| payload.source_playlist_id.clone(), |p| p.title.clone());
                let prompt = DropPrompt {
                    pos: pointer_pos,
                    source_playlist_id: payload.source_playlist_id.clone(),
                    source_playlist_title: src_title,
                    target_playlist_id: pl.id.clone(),
                    target_playlist_title: pl.title.clone(),
                    tracks: payload.tracks.clone(),
                };
                handle_cross_playlist_drop(
                    ui.ctx(),
                    prompt,
                    drop_prompt,
                    playlist_cache,
                    library,
                    sync_queue,
                    toast,
                );
            }

            if resp.clicked() {
                *focused_idx = Some(vis_idx);
                select_playlist(
                    pl.id.clone(),
                    selected_playlist_id,
                    playlist_cache,
                    loading_playlists,
                    cmd_tx,
                );
            }
            if resp.double_clicked() {
                *active_tab = GuiTab::Inspector;
            }

            resp.context_menu(|ui| {
                if ui.button("Open in Main Pane").clicked() {
                    *active_tab = GuiTab::Inspector;
                    select_playlist(
                        pl.id.clone(),
                        selected_playlist_id,
                        playlist_cache,
                        loading_playlists,
                        cmd_tx,
                    );
                    ui.close_menu();
                }
                if ui.button("Open in Split Pane B (Diff)").clicked() {
                    split_state.enabled = true;
                    split_state.playlist_b_id = Some(pl.id.clone());
                    *active_tab = GuiTab::Inspector;
                    if !playlist_cache.contains_key(&pl.id)
                        && loading_playlists.insert(pl.id.clone())
                    {
                        let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(pl.id.clone()));
                    }
                    ui.close_menu();
                }
                if ui.button("Set as Diff A").clicked() {
                    set_form.playlist_a = pl.id.clone();
                    ui.close_menu();
                }
                if ui.button("Set as Diff B").clicked() {
                    set_form.playlist_b = pl.id.clone();
                    ui.close_menu();
                }
                if ui.button("Force Refresh from YTM").clicked() {
                    playlist_cache.remove(&pl.id);
                    if loading_playlists.insert(pl.id.clone()) {
                        let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(pl.id.clone()));
                    }
                    ui.close_menu();
                }
            });
        }
        auto_scroll_on_drag(ui, ui.clip_rect());
    });
    let _ = scroll_out;
}

fn select_playlist(
    id: String,
    selected_playlist_id: &mut Option<String>,
    playlist_cache: &HashMap<String, Playlist>,
    loading_playlists: &mut HashSet<String>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    *selected_playlist_id = Some(id.clone());
    if !playlist_cache.contains_key(&id) && loading_playlists.insert(id.clone()) {
        let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(id));
    }
}
