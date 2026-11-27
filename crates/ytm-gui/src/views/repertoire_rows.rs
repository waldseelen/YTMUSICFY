use eframe::egui;

use crate::icons::{Icon, icon_button};
use crate::playback_ops::PlayAction;
use crate::state::{DragTrackPayload, RepertoireTreeState};
use crate::views::inspect_helpers::format_duration;
use crate::views::repertoire_target::PlaylistMatches;
use crate::views::repertoire_tree::selection_key;

/// Render a single matched track row in the left Repertoire File-Tree.
pub(crate) fn render_track_row(
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
                let album_part = t
                    .album
                    .as_ref()
                    .map(|a| format!(" · {}", a.name))
                    .unwrap_or_default();
                ui.add(
                    egui::Label::new(format!(
                        "::  {} — {}{} ({dur})",
                        t.primary_artist_name().unwrap_or("-"),
                        t.title,
                        album_part
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
