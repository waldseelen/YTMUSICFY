use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistPrivacy, PlaylistSummary};

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
    toast: &mut Option<(bool, String)>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.strong("Library Playlists");
    ui.add(
        egui::TextEdit::singleline(sidebar_filter)
            .hint_text("Filter playlists...")
            .desired_width(f32::INFINITY),
    );
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(new_playlist_title)
                .hint_text("New playlist title...")
                .desired_width(145.0),
        );
        let title = new_playlist_title.trim();
        if ui
            .add_enabled(!title.is_empty(), egui::Button::new("+ Stage"))
            .clicked()
        {
            sync_queue.push(StagedOp::CreatePlaylist {
                title: title.to_string(),
                description: "Created by ytm-gui".to_string(),
                privacy: PlaylistPrivacy::Private,
                tracks: Vec::new(),
            });
            *toast = Some((true, format!("Staged new playlist '{title}' in Sync.")));
            new_playlist_title.clear();
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
        return;
    }
    let Some(playlists) = library.data() else {
        return;
    };

    let dirty_ids = sync_queue.affected_playlist_ids();
    let needle = sidebar_filter.trim().to_ascii_lowercase();
    let pointer_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();

    egui::ScrollArea::vertical().show(ui, |ui| {
        for pl in playlists {
            if !needle.is_empty() && !pl.title.to_ascii_lowercase().contains(&needle) {
                continue;
            }
            let selected = selected_playlist_id.as_deref() == Some(pl.id.as_str());
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
            let label = format!("{}{} ({count}){loading_mark}", dirty_mark, pl.title);

            let resp = ui.selectable_label(selected, label);
            if resp.dnd_hover_payload::<DragTrackPayload>().is_some() {
                ui.painter().rect_stroke(
                    resp.rect.expand(1.0),
                    2.0,
                    egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(90, 175, 240)),
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
                *drop_prompt = Some(DropPrompt {
                    pos: pointer_pos,
                    source_playlist_id: payload.source_playlist_id.clone(),
                    source_playlist_title: src_title,
                    target_playlist_id: pl.id.clone(),
                    target_playlist_title: pl.title.clone(),
                    tracks: payload.tracks.clone(),
                });
            }

            if resp.clicked() {
                *selected_playlist_id = Some(pl.id.clone());
                if !playlist_cache.contains_key(&pl.id) && loading_playlists.insert(pl.id.clone()) {
                    let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(pl.id.clone()));
                }
            }

            resp.context_menu(|ui| {
                if ui.button("Open in Main Pane").clicked() {
                    *selected_playlist_id = Some(pl.id.clone());
                    *active_tab = GuiTab::Inspector;
                    if !playlist_cache.contains_key(&pl.id)
                        && loading_playlists.insert(pl.id.clone())
                    {
                        let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(pl.id.clone()));
                    }
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
    });
}
