use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, Track};

use crate::interaction::{paint_drag_ghost, search_box_with_clear};
use crate::navigation::ToastState;
use crate::overlays::handle_cross_playlist_drop;
use crate::playback_ops::PlayAction;
use crate::state::{
    AsyncState, DragDropMode, DragTrackPayload, DropPrompt, GuiTab, RepertoireTreeState,
    SidebarSearchMode, WorkerCmd,
};
use crate::sync_queue::SyncQueue;
use crate::views::inspect_helpers::format_duration;
use crate::views::repertoire_batch::show_tree_selection_bar;
pub use crate::views::repertoire_target::{PlaylistMatches, collect_repertoire_matches};
use crate::views::repertoire_target::{build_drop_prompt, render_target_pane};

pub(crate) fn selection_key(playlist_id: &str, track: &Track) -> String {
    format!("{playlist_id}::{}::{}", track.position, track.video_id)
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    query: &mut String,
    search_mode: &mut SidebarSearchMode,
    playlists: &[PlaylistSummary],
    library: &AsyncState<Vec<PlaylistSummary>>,
    loading_playlists: &mut HashSet<String>,
    playlist_cache: &HashMap<String, Playlist>,
    sync_queue: &mut SyncQueue,
    tree_state: &mut RepertoireTreeState,
    drag_mode: &mut DragDropMode,
    selected_playlist_id: &mut Option<String>,
    active_tab: &mut GuiTab,
    drop_prompt: &mut Option<DropPrompt>,
    toast: &mut Option<ToastState>,
    cmd_tx: &Sender<WorkerCmd>,
) -> Vec<PlayAction> {
    let mut actions = Vec::new();
    let matches = collect_repertoire_matches(query, playlists, playlist_cache, sync_queue);
    let total_hits: usize = matches.iter().map(|m| m.tracks.len()).sum();
    let cached_cnt = playlists
        .iter()
        .filter(|p| playlist_cache.contains_key(&p.id))
        .count();

    if let Some(payload) = egui::DragAndDrop::payload::<DragTrackPayload>(ui.ctx()) {
        let label = if payload.tracks.len() > 1 {
            format!("♪ {} tracks", payload.tracks.len())
        } else if let Some(first) = payload.tracks.first() {
            format!("♪ {}", first.title)
        } else {
            "♪ Track".to_string()
        };
        paint_drag_ghost(ui.ctx(), &label);
    }

    ui.horizontal_wrapped(|ui| {
        ui.heading("Repertoire Search:");
        search_box_with_clear(
            ui,
            "rep_tree_main_search",
            query,
            "Search artist, track, album...",
            200.0,
            false,
        );
        ui.label(format!(
            "{total_hits} track(s) in {} playlist(s) | Indexed: {cached_cnt}/{}",
            matches.len(),
            playlists.len()
        ));
        if cached_cnt < playlists.len()
            && ui
                .button("Index Missing Playlists")
                .on_hover_text(
                    "Fetches uncached playlists into local SQLite at a safe pace (350ms/playlist) so future searches are instant.",
                )
                .clicked()
        {
            let _ = cmd_tx.send(WorkerCmd::IndexAllPlaylists { missing_only: true });
        }
        if ui
            .small_button("Expand All")
            .on_hover_text("Expand all matching playlist branches")
            .clicked()
        {
            tree_state.collapsed_playlists.clear();
        }
        if ui
            .small_button("Collapse All")
            .on_hover_text("Collapse all matching playlist branches")
            .clicked()
        {
            for m in &matches {
                tree_state.collapsed_playlists.insert(m.playlist.id.clone());
            }
        }
        ui.separator();
        ui.label("Drag:");
        for mode in DragDropMode::ALL {
            let tip = match mode {
                DragDropMode::Ask => "Show Copy/Move popup on drop",
                DragDropMode::Move => "Stage Move (Copy + Remove Source) immediately on drop",
                DragDropMode::Copy => "Stage Copy immediately on drop",
            };
            ui.selectable_value(drag_mode, mode, mode.label())
                .on_hover_text(tip);
        }
        ui.separator();
        if ui
            .button("✕ Exit Track Search")
            .on_hover_text("Switch left search bar back to Playlist filter mode")
            .clicked()
        {
            *search_mode = SidebarSearchMode::Playlists;
        }
    });
    ui.separator();

    let target_is_editable = tree_state.target_playlist_id.as_deref().is_some_and(|tid| {
        playlists
            .iter()
            .any(|p| p.id == tid && p.owned && !p.id.eq_ignore_ascii_case("LM"))
    });
    if !target_is_editable {
        tree_state.target_playlist_id = selected_playlist_id
            .clone()
            .filter(|sid| {
                playlists
                    .iter()
                    .any(|p| p.id == *sid && p.owned && !p.id.eq_ignore_ascii_case("LM"))
            })
            .or_else(|| {
                playlists
                    .iter()
                    .find(|p| p.owned && !p.id.eq_ignore_ascii_case("LM"))
                    .map(|p| p.id.clone())
            });
    }

    show_tree_selection_bar(ui, &matches, playlists, tree_state, sync_queue, toast);

    let pointer_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
    let avail = ui.available_size();
    let splitter_w = 8.0_f32;
    let total_w = (avail.x - splitter_w).max(200.0);
    let left_w = (total_w * tree_state.split_ratio).max(100.0);
    let right_w = (total_w - left_w).max(100.0);
    let pane_h = avail.y.max(180.0);

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(left_w, pane_h),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_max_width(left_w);
                ui.group(|ui| {
                    ui.strong("Matching Playlists (File-Tree)");
                    ui.separator();
                    if matches.is_empty() {
                        ui.weak("No cached tracks match. Click 'Index Missing Playlists' above to scan uncached lists.");
                        return;
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("rep_tree_left_scroll")
                        .show(ui, |ui| {
                            for group in &matches {
                                actions.extend(render_tree_branch(
                                    ui,
                                    group,
                                    tree_state,
                                    *drag_mode,
                                    playlists,
                                    playlist_cache,
                                    library,
                                    sync_queue,
                                    drop_prompt,
                                    toast,
                                    selected_playlist_id,
                                    active_tab,
                                    search_mode,
                                    pointer_pos,
                                ));
                            }
                        });
                });
            },
        );

        let (sep_rect, sep_resp) = ui.allocate_exact_size(
            egui::vec2(splitter_w, pane_h),
            egui::Sense::click_and_drag(),
        );
        if sep_resp.hovered() || sep_resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if sep_resp.dragged() {
            let delta = ui.input(|i| i.pointer.delta().x);
            let total = (avail.x - splitter_w).max(1.0);
            tree_state.split_ratio = (tree_state.split_ratio + delta / total).clamp(0.30, 0.72);
        }
        ui.painter().vline(
            sep_rect.center().x,
            sep_rect.y_range(),
            egui::Stroke::new(1.5_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );

        ui.allocate_ui_with_layout(
            egui::vec2(right_w, pane_h),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_max_width(right_w);
                actions.extend(render_target_pane(
                    ui,
                    playlists,
                    library,
                    loading_playlists,
                    playlist_cache,
                    sync_queue,
                    tree_state,
                    *drag_mode,
                    drop_prompt,
                    toast,
                    pointer_pos,
                    cmd_tx,
                ));
            },
        );
    });

    actions
}

#[allow(clippy::too_many_arguments)]
fn render_tree_branch(
    ui: &mut egui::Ui,
    group: &PlaylistMatches,
    tree_state: &mut RepertoireTreeState,
    drag_mode: DragDropMode,
    playlists: &[PlaylistSummary],
    playlist_cache: &HashMap<String, Playlist>,
    library: &AsyncState<Vec<PlaylistSummary>>,
    sync_queue: &mut SyncQueue,
    drop_prompt: &mut Option<DropPrompt>,
    toast: &mut Option<ToastState>,
    selected_playlist_id: &mut Option<String>,
    active_tab: &mut GuiTab,
    search_mode: &mut SidebarSearchMode,
    pointer_pos: egui::Pos2,
) -> Vec<PlayAction> {
    let mut actions = Vec::new();
    let pl = &group.playlist;
    let collapsed = tree_state.collapsed_playlists.contains(&pl.id);
    let arrow = if collapsed { ">" } else { "v" };

    let header_resp = ui
        .horizontal(|ui| {
            let toggle = ui.selectable_label(
                false,
                egui::RichText::new(format!(
                    "{arrow} {} ({} / {})",
                    pl.title,
                    group.tracks.len(),
                    pl.tracks.len()
                ))
                .strong(),
            );
            if toggle.clicked() {
                if collapsed {
                    tree_state.collapsed_playlists.remove(&pl.id);
                } else {
                    tree_state.collapsed_playlists.insert(pl.id.clone());
                }
            }
            if ui
                .small_button("Open ↗")
                .on_hover_text("Open this playlist in Inspector")
                .clicked()
            {
                *selected_playlist_id = Some(pl.id.clone());
                *search_mode = SidebarSearchMode::Playlists;
                *active_tab = GuiTab::Inspector;
            }
        })
        .response;

    let can_drop_here = pl.owned && !pl.id.eq_ignore_ascii_case("LM");
    if can_drop_here
        && let Some(payload) = header_resp.dnd_hover_payload::<DragTrackPayload>()
        && payload.source_playlist_id != pl.id
    {
        ui.painter().rect_stroke(
            header_resp.rect,
            3.0,
            egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 175, 240)),
            egui::StrokeKind::Inside,
        );
    }
    if can_drop_here
        && let Some(payload) = header_resp.dnd_release_payload::<DragTrackPayload>()
        && payload.source_playlist_id != pl.id
    {
        let prompt = build_drop_prompt(pointer_pos, &payload, &pl.id, playlists);
        handle_cross_playlist_drop(
            ui.ctx(),
            prompt,
            drag_mode,
            drop_prompt,
            playlist_cache,
            library,
            sync_queue,
            toast,
        );
    }

    if collapsed {
        return actions;
    }

    for (idx, t) in group.tracks.iter().enumerate() {
        let key = selection_key(&pl.id, t);
        let mut checked = tree_state.selected_keys.contains(&key);
        let selected_same_pl: Vec<Track> = group
            .tracks
            .iter()
            .filter(|tr| {
                tree_state
                    .selected_keys
                    .contains(&selection_key(&pl.id, tr))
            })
            .cloned()
            .collect();
        let drag_tracks = if checked && selected_same_pl.len() > 1 {
            selected_same_pl
        } else {
            vec![t.clone()]
        };
        let payload = DragTrackPayload {
            source_playlist_id: pl.id.clone(),
            tracks: drag_tracks,
        };

        ui.horizontal(|ui| {
            ui.add_space(18.0);
            if ui.checkbox(&mut checked, "").changed() {
                if checked {
                    tree_state.selected_keys.insert(key.clone());
                } else {
                    tree_state.selected_keys.remove(&key);
                }
            }
            let drag_resp = ui
                .dnd_drag_source(
                    egui::Id::new(("tree_drag", &pl.id, idx, &t.video_id)),
                    payload,
                    |ui| {
                        ui.add(
                            egui::Label::new(format!(
                                "::  {} — {} ({})",
                                t.primary_artist_name().unwrap_or("-"),
                                t.title,
                                t.duration_sec.map_or_else(
                                    || "-".to_string(),
                                    |d| format_duration(u64::from(d))
                                )
                            ))
                            .truncate(),
                        );
                    },
                )
                .response;
            if drag_resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if drag_resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if ui.small_button("▶").on_hover_text("Play now").clicked() {
                actions.push(PlayAction::PlayFromList {
                    tracks: group.tracks.clone(),
                    start_idx: idx,
                });
            }
        });
    }
    actions
}
