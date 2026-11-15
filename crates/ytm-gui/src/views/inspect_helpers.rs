use std::collections::{HashMap, HashSet};

use eframe::egui;
use ytm_core::{Playlist, Track};
use ytm_ops::compute_minimal_moves;

use crate::navigation::ToastState;
use crate::playback_ops::PlayAction;
use crate::state::{AsyncState, DragTrackPayload, InspectColumn, InspectSortState};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::inspect_sort::sort_column_label;
pub use crate::views::inspect_sort::{sort_header, sort_tracks_slice};

pub fn show_selection_bar(
    ui: &mut egui::Ui,
    pl: &Playlist,
    editable: bool,
    sync_queue: &mut SyncQueue,
    selected_positions: &mut HashSet<usize>,
    toast: &mut Option<ToastState>,
) {
    if selected_positions.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Selected: {}", selected_positions.len()));
        if ui
            .add_enabled(editable, egui::Button::new("Stage Remove Selected"))
            .clicked()
        {
            let picked: Vec<Track> = pl
                .tracks
                .iter()
                .filter(|t| selected_positions.contains(&t.position))
                .cloned()
                .collect();
            let cnt = picked.len();
            let eid = sync_queue.push(StagedOp::RemoveTracks {
                playlist_id: pl.id.clone(),
                playlist_title: pl.title.clone(),
                tracks: picked,
            });
            selected_positions.clear();
            *toast = Some(ToastState::staged(
                format!("Staged -{cnt} track removal(s) in Sync."),
                Some(eid),
            ));
        }
        if ui.button("Clear Selection").clicked() {
            selected_positions.clear();
        }
    });
}

pub fn show_sort_stage_bar(
    ui: &mut egui::Ui,
    pl: &Playlist,
    editable: bool,
    playlist_cache: &HashMap<String, Playlist>,
    sync_queue: &mut SyncQueue,
    sort_state: &mut InspectSortState,
    toast: &mut Option<ToastState>,
) {
    if sort_state.column == InspectColumn::Position && sort_state.asc {
        return;
    }
    let mut sorted_refs: Vec<&Track> = pl.tracks.iter().collect();
    sort_tracks_slice(&mut sorted_refs, *sort_state);
    let current_sids: Vec<String> = pl
        .tracks
        .iter()
        .filter_map(|t| t.set_video_id.clone())
        .collect();
    let target_sids: Vec<String> = sorted_refs
        .iter()
        .filter_map(|t| t.set_video_id.clone())
        .collect();

    let dir_label = if sort_state.asc { "A-Z" } else { "Z-A" };
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(30, 44, 38))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(66, 168, 112)))
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(130, 220, 165),
                    format!(
                        "Order Preview: {} ({dir_label})",
                        sort_column_label(sort_state.column)
                    ),
                );
                if !editable {
                    ui.weak("(Read-only playlist — preview only)");
                } else if current_sids.len() != pl.tracks.len() {
                    ui.add_enabled(false, egui::Button::new("Save Order to YTM"))
                        .on_disabled_hover_text(
                            "Some tracks lack setVideoId. Force Refresh from YTM first.",
                        );
                } else if current_sids != target_sids {
                    let (_, moves) = compute_minimal_moves(&current_sids, &target_sids);
                    if !moves.is_empty() {
                        let btn = egui::Button::new(
                            egui::RichText::new(format!(
                                "Save Order to YTM ({} moves · tracks unchanged)",
                                moves.len()
                            ))
                            .strong()
                            .color(egui::Color32::from_rgb(16, 20, 18)),
                        )
                        .fill(egui::Color32::from_rgb(90, 200, 130));
                        if ui
                            .add(btn)
                            .on_hover_text(
                                "Stages minimal LIS moves into Sync. Track list stays identical; only YouTube Music's permanent order is rewritten on Sync.",
                            )
                            .clicked()
                        {
                            match sync_queue.stage_reorder(
                                &pl.id,
                                &pl.title,
                                target_sids,
                                playlist_cache,
                            ) {
                                Ok(n) => {
                                    *sort_state = InspectSortState::default();
                                    *toast = Some(ToastState::staged(
                                        format!("Staged {n} LIS move(s) — click Sync Now (Ctrl+Enter) to save order."),
                                        sync_queue.last_entry_id(),
                                    ));
                                }
                                Err(e) => *toast = Some(ToastState::info(false, e)),
                            }
                        }
                    }
                } else {
                    ui.weak("(Matches current YTM order)");
                }
                if ui
                    .button("Reset to YTM Order (#)")
                    .on_hover_text("Discard preview sort and show default playlist position order")
                    .clicked()
                {
                    *sort_state = InspectSortState::default();
                }
            });
        });
}

pub fn show_quick_search_results(
    ui: &mut egui::Ui,
    pl: &Playlist,
    editable: bool,
    search_results: &AsyncState<Vec<Track>>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
) -> Vec<PlayAction> {
    let mut actions = Vec::new();
    let Some(found) = search_results.data() else {
        return actions;
    };
    if found.is_empty() {
        return actions;
    }
    ui.group(|ui| {
        ui.strong("Quick Search Results (▶ Play, drag ⋮⋮ to playlist, or + Stage Add):");
        for (idx, t) in found.iter().take(5).enumerate() {
            ui.horizontal(|ui| {
                let drag = ui
                    .dnd_drag_source(
                        egui::Id::new(("quick_search_drag", idx, &t.video_id)),
                        DragTrackPayload {
                            source_playlist_id: "__SEARCH__".to_string(),
                            tracks: vec![t.clone()],
                        },
                        |ui| ui.small("⋮⋮"),
                    )
                    .response;
                if drag.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
                if ui.small_button("▶").on_hover_text("Play now").clicked() {
                    actions.push(PlayAction::PlayFromList {
                        tracks: vec![t.clone()],
                        start_idx: 0,
                    });
                }
                ui.label(format!(
                    "{} - {} ({})",
                    t.primary_artist_name().unwrap_or("-"),
                    t.title,
                    t.duration_sec
                        .map_or_else(|| "-".to_string(), |d| format!("{d}s"))
                ));
                if ui
                    .add_enabled(editable, egui::Button::new("+ Stage Add"))
                    .clicked()
                {
                    let eid = sync_queue.push(StagedOp::AddTracks {
                        playlist_id: pl.id.clone(),
                        playlist_title: pl.title.clone(),
                        tracks: vec![t.clone()],
                        allow_duplicates: false,
                    });
                    *toast = Some(ToastState::staged(
                        format!("Staged '+' '{}' in Sync.", t.title),
                        Some(eid),
                    ));
                }
            });
        }
    });
    actions
}

pub fn compute_drop_reorder(
    pl: &Playlist,
    dragged_sids: &[String],
    target_pos: usize,
) -> Option<Vec<String>> {
    let all_sids: Vec<String> = pl
        .tracks
        .iter()
        .filter_map(|t| t.set_video_id.clone())
        .collect();
    if all_sids.len() != pl.tracks.len() {
        return None;
    }
    let dragged_set: HashSet<&str> = dragged_sids.iter().map(String::as_str).collect();
    let remaining: Vec<String> = all_sids
        .iter()
        .filter(|sid| !dragged_set.contains(sid.as_str()))
        .cloned()
        .collect();
    let insert_at = if target_pos >= all_sids.len() {
        remaining.len()
    } else {
        let target_sid = all_sids.get(target_pos)?;
        if dragged_set.contains(target_sid.as_str()) {
            return None;
        }
        remaining
            .iter()
            .position(|sid| sid == target_sid)
            .unwrap_or(remaining.len())
    };
    let mut out = Vec::with_capacity(all_sids.len());
    out.extend_from_slice(&remaining[..insert_at]);
    out.extend(
        all_sids
            .iter()
            .filter(|sid| dragged_set.contains(sid.as_str()))
            .cloned(),
    );
    out.extend_from_slice(&remaining[insert_at..]);
    Some(out)
}

pub fn format_duration(total_secs: u64) -> String {
    let hrs = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hrs > 0 {
        format!("{hrs:02}:{mins:02}:{secs:02}")
    } else {
        format!("{mins:02}:{secs:02}")
    }
}

#[cfg(test)]
mod tests {
    use ytm_core::{MusicVideoType, PlaylistPrivacy, TrackAvailability};

    use super::*;

    fn mk_track(vid: &str, sid: &str, pos: usize) -> Track {
        Track {
            video_id: vid.to_string(),
            set_video_id: Some(sid.to_string()),
            position: pos,
            title: vid.to_string(),
            artists: Vec::new(),
            album: None,
            duration_sec: Some(120),
            release_year: None,
            music_video_type: MusicVideoType::AudioTrack,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        }
    }

    #[test]
    fn compute_drop_reorder_moves_to_middle_and_end() {
        let pl = Playlist {
            id: "PL1".to_string(),
            title: "Mix".to_string(),
            description: None,
            privacy: PlaylistPrivacy::Private,
            owned: true,
            track_count: 3,
            tracks: vec![
                mk_track("V1", "S1", 0),
                mk_track("V2", "S2", 1),
                mk_track("V3", "S3", 2),
            ],
        };
        let to_end = compute_drop_reorder(&pl, &["S1".to_string()], 3).expect("move to end");
        assert_eq!(to_end, vec!["S2", "S3", "S1"]);
        let to_top = compute_drop_reorder(&pl, &["S3".to_string()], 0).expect("move to top");
        assert_eq!(to_top, vec!["S3", "S1", "S2"]);
    }
}
