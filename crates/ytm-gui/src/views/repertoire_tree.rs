use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, Track};

use crate::icons::{Icon, icon_button, icon_text_button};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TreeRow {
    BranchHeader { group_idx: usize },
    TrackRow { group_idx: usize, track_idx: usize },
}

fn flatten_visible_rows(matches: &[PlaylistMatches], collapsed: &HashSet<String>) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    for (group_idx, group) in matches.iter().enumerate() {
        rows.push(TreeRow::BranchHeader { group_idx });
        if !collapsed.contains(&group.playlist.id) {
            rows.extend((0..group.tracks.len()).map(|track_idx| TreeRow::TrackRow {
                group_idx,
                track_idx,
            }));
        }
    }
    rows
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

    if let Some(p) = egui::DragAndDrop::payload::<DragTrackPayload>(ui.ctx()) {
        let lbl = if p.tracks.len() > 1 {
            format!("{} tracks", p.tracks.len())
        } else {
            p.tracks
                .first()
                .map_or_else(|| "Track".into(), |f| f.title.clone())
        };
        paint_drag_ghost(ui.ctx(), &lbl);
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
        if cached_cnt < playlists.len() && ui.button("Index Missing Playlists").clicked() {
            let _ = cmd_tx.send(WorkerCmd::IndexAllPlaylists { missing_only: true });
        }
        if ui.small_button("Expand All").clicked() {
            tree_state.collapsed_playlists.clear();
        }
        if ui.small_button("Collapse All").clicked() {
            tree_state
                .collapsed_playlists
                .extend(matches.iter().map(|m| m.playlist.id.clone()));
        }
        ui.separator();
        ui.label("Drag:");
        for mode in DragDropMode::ALL {
            ui.selectable_value(drag_mode, mode, mode.label());
        }
        ui.separator();
        if icon_text_button(ui, Icon::Close, "Exit Track Search", 13.0, true).clicked() {
            *search_mode = SidebarSearchMode::Playlists;
        }
    });
    ui.separator();

    let is_editable = |id: &str| {
        playlists
            .iter()
            .any(|p| p.id == id && p.owned && !p.id.eq_ignore_ascii_case("LM"))
    };
    if !tree_state
        .target_playlist_id
        .as_deref()
        .is_some_and(is_editable)
    {
        tree_state.target_playlist_id = selected_playlist_id
            .as_deref()
            .filter(|id| is_editable(id))
            .or_else(|| {
                playlists
                    .iter()
                    .find(|p| is_editable(&p.id))
                    .map(|p| p.id.as_str())
            })
            .map(str::to_string);
    }

    show_tree_selection_bar(ui, &matches, playlists, tree_state, sync_queue, toast);

    let pointer_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
    let avail = ui.available_size();
    let total_w = (avail.x - 8.0).max(200.0);
    let left_w = (total_w * tree_state.split_ratio).max(100.0);
    let right_w = (total_w - left_w).max(100.0);
    let pane_h = avail.y.max(180.0);
    let mut selected_counts = Vec::with_capacity(matches.len());
    for g in &matches {
        let mut cnt = 0usize;
        for tr in &g.tracks {
            if tree_state
                .selected_keys
                .contains(&selection_key(&g.playlist.id, tr))
            {
                cnt += 1;
            }
        }
        selected_counts.push(cnt);
    }
    let visible_rows = flatten_visible_rows(&matches, &tree_state.collapsed_playlists);

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(left_w, pane_h), egui::Layout::top_down(egui::Align::LEFT), |ui| {
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
                    .auto_shrink([false, false])
                    .show_rows(ui, 22.0, visible_rows.len(), |ui, row_range| {
                        for row in &visible_rows[row_range] {
                            match *row {
                                TreeRow::BranchHeader { group_idx } => {
                                    let group = &matches[group_idx];
                                    let pl = &group.playlist;
                                    let collapsed = tree_state.collapsed_playlists.contains(&pl.id);
                                    let arrow = if collapsed { ">" } else { "v" };
                                    let header_resp = ui
                                        .horizontal(|ui| {
                                            let lbl = format!("{arrow} {} ({} / {})", pl.title, group.tracks.len(), pl.tracks.len());
                                            if ui.selectable_label(false, egui::RichText::new(lbl).strong()).clicked() {
                                                if collapsed { tree_state.collapsed_playlists.remove(&pl.id); }
                                                else { tree_state.collapsed_playlists.insert(pl.id.clone()); }
                                            }
                                            if ui.small_button("Open ↗").clicked() {
                                                *selected_playlist_id = Some(pl.id.clone());
                                                *search_mode = SidebarSearchMode::Playlists;
                                                *active_tab = GuiTab::Inspector;
                                            }
                                        })
                                        .response;
                                    let can_drop = pl.owned && !pl.id.eq_ignore_ascii_case("LM");
                                    if can_drop && let Some(p) = header_resp.dnd_hover_payload::<DragTrackPayload>() && p.source_playlist_id != pl.id {
                                        ui.painter().rect_stroke(header_resp.rect, 3.0, egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 175, 240)), egui::StrokeKind::Inside);
                                    }
                                    if can_drop && let Some(p) = header_resp.dnd_release_payload::<DragTrackPayload>() && p.source_playlist_id != pl.id {
                                        let prompt = build_drop_prompt(pointer_pos, &p, &pl.id, playlists);
                                        handle_cross_playlist_drop(ui.ctx(), prompt, *drag_mode, drop_prompt, playlist_cache, library, sync_queue, toast);
                                    }
                                }
                                TreeRow::TrackRow { group_idx, track_idx } => {
                                    if let Some(act) = render_track_row(ui, &matches[group_idx], track_idx, selected_counts[group_idx], tree_state) {
                                        actions.push(act);
                                    }
                                }
                            }
                        }
                    });
            });
        });

        let (sep_rect, sep_resp) = ui.allocate_exact_size(egui::vec2(8.0, pane_h), egui::Sense::click_and_drag());
        if sep_resp.hovered() || sep_resp.dragged() { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal); }
        if sep_resp.dragged() {
            tree_state.split_ratio = (tree_state.split_ratio + ui.input(|i| i.pointer.delta().x) / total_w).clamp(0.30, 0.72);
        }
        ui.painter().vline(sep_rect.center().x, sep_rect.y_range(), egui::Stroke::new(1.5_f32, ui.visuals().widgets.noninteractive.bg_stroke.color));

        ui.allocate_ui_with_layout(egui::vec2(right_w, pane_h), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.set_max_width(right_w);
            actions.extend(render_target_pane(
                ui, playlists, library, loading_playlists, playlist_cache, sync_queue,
                tree_state, *drag_mode, drop_prompt, toast, pointer_pos, cmd_tx,
            ));
        });
    });

    actions
}

fn render_track_row(
    ui: &mut egui::Ui,
    group: &PlaylistMatches,
    track_idx: usize,
    selected_count: usize,
    tree_state: &mut RepertoireTreeState,
) -> Option<PlayAction> {
    let pl = &group.playlist;
    let t = &group.tracks[track_idx];
    let key = selection_key(&pl.id, t);
    let mut checked = tree_state.selected_keys.contains(&key);
    let drag_id = egui::Id::new(("tree_drag", &pl.id, track_idx, &t.video_id));

    let tracks = if !ui.ctx().is_being_dragged(drag_id) {
        Vec::new()
    } else if checked && selected_count > 1 {
        group
            .tracks
            .iter()
            .filter(|tr| {
                tree_state
                    .selected_keys
                    .contains(&selection_key(&pl.id, tr))
            })
            .cloned()
            .collect()
    } else {
        vec![t.clone()]
    };
    let payload = DragTrackPayload {
        source_playlist_id: if tracks.is_empty() {
            String::new()
        } else {
            pl.id.clone()
        },
        tracks,
    };

    let mut action = None;
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        if ui.checkbox(&mut checked, "").changed() {
            if checked {
                tree_state.selected_keys.insert(key);
            } else {
                tree_state.selected_keys.remove(&key);
            }
        }
        let drag_resp = ui
            .dnd_drag_source(drag_id, payload, |ui| {
                let dur = t
                    .duration_sec
                    .map_or_else(|| "-".into(), |d| format_duration(u64::from(d)));
                ui.add(
                    egui::Label::new(format!(
                        "::  {} — {} ({dur})",
                        t.primary_artist_name().unwrap_or("-"),
                        t.title
                    ))
                    .truncate(),
                );
            })
            .response;
        if drag_resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if drag_resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if icon_button(ui, Icon::Play, 12.0, true, "Play track").clicked() {
            action = Some(PlayAction::PlayFromList {
                tracks: group.tracks.clone(),
                start_idx: track_idx,
            });
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use ytm_core::{MusicVideoType, PlaylistPrivacy, TrackAvailability};

    use super::*;

    fn mk_match(id: &str, count: usize) -> PlaylistMatches {
        let tracks: Vec<Track> = (0..count)
            .map(|i| Track {
                video_id: format!("{id}_V{i}"),
                set_video_id: Some(format!("{id}_S{i}")),
                position: i,
                title: format!("T{i}"),
                artists: Vec::new(),
                album: None,
                duration_sec: Some(180),
                release_year: None,
                music_video_type: MusicVideoType::AudioTrack,
                availability: TrackAvailability::Available,
                thumbnail_url: None,
            })
            .collect();
        PlaylistMatches {
            playlist: Playlist {
                id: id.into(),
                title: id.into(),
                description: None,
                privacy: PlaylistPrivacy::Private,
                owned: true,
                track_count: count,
                tracks: tracks.clone(),
            },
            tracks,
        }
    }

    #[test]
    fn flatten_visible_rows_respects_collapsed_branches() {
        let matches = vec![mk_match("PL1", 2), mk_match("PL2", 3)];
        let rows = flatten_visible_rows(&matches, &HashSet::from(["PL1".to_string()]));
        let bh = |group_idx| TreeRow::BranchHeader { group_idx };
        let tr = |group_idx, track_idx| TreeRow::TrackRow {
            group_idx,
            track_idx,
        };
        assert_eq!(rows, vec![bh(0), bh(1), tr(1, 0), tr(1, 1), tr(1, 2)]);
    }
}
