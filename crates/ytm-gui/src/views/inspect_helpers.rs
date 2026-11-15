use std::collections::{HashMap, HashSet};

use eframe::egui;
use ytm_core::{Playlist, Track};
use ytm_ops::compute_minimal_moves;

use crate::state::{AsyncState, InspectColumn, InspectSortState};
use crate::sync_queue::{StagedOp, SyncQueue};

pub fn show_selection_bar(
    ui: &mut egui::Ui,
    pl: &Playlist,
    editable: bool,
    sync_queue: &mut SyncQueue,
    selected_positions: &mut HashSet<usize>,
    toast: &mut Option<(bool, String)>,
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
            sync_queue.push(StagedOp::RemoveTracks {
                playlist_id: pl.id.clone(),
                playlist_title: pl.title.clone(),
                tracks: picked,
            });
            selected_positions.clear();
            *toast = Some((true, format!("Staged -{cnt} track removal(s) in Sync.")));
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
    toast: &mut Option<(bool, String)>,
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

    ui.horizontal_wrapped(|ui| {
        if current_sids.len() == pl.tracks.len() && current_sids != target_sids {
            let (_, moves) = compute_minimal_moves(&current_sids, &target_sids);
            if editable
                && !moves.is_empty()
                && ui
                    .button(format!(
                        "Stage Sorted Order to Sync ({} LIS moves)",
                        moves.len()
                    ))
                    .clicked()
            {
                match sync_queue.stage_reorder(&pl.id, &pl.title, target_sids, playlist_cache) {
                    Ok(n) => {
                        *sort_state = InspectSortState::default();
                        *toast = Some((true, format!("Staged {n} LIS move(s) in Sync.")));
                    }
                    Err(e) => *toast = Some((false, e)),
                }
            }
        }
        if ui.button("Reset # Order").clicked() {
            *sort_state = InspectSortState::default();
        }
    });
}

pub fn show_quick_search_results(
    ui: &mut egui::Ui,
    pl: &Playlist,
    editable: bool,
    search_results: &AsyncState<Vec<Track>>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<(bool, String)>,
) {
    let Some(found) = search_results.data() else {
        return;
    };
    if found.is_empty() {
        return;
    }
    ui.group(|ui| {
        ui.strong("Quick Search Results:");
        for t in found.iter().take(5) {
            ui.horizontal(|ui| {
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
                    sync_queue.push(StagedOp::AddTracks {
                        playlist_id: pl.id.clone(),
                        playlist_title: pl.title.clone(),
                        tracks: vec![t.clone()],
                        allow_duplicates: false,
                    });
                    *toast = Some((true, format!("Staged '+' '{}' in Sync.", t.title)));
                }
            });
        }
    });
}

pub fn sort_header(
    header: &mut egui_extras::TableRow<'_, '_>,
    title: &str,
    col: InspectColumn,
    sort_state: &mut InspectSortState,
) {
    header.col(|ui| {
        let active = sort_state.column == col;
        let arrow = if active {
            if sort_state.asc { " ▲" } else { " ▼" }
        } else {
            ""
        };
        if ui
            .selectable_label(active, format!("{title}{arrow}"))
            .clicked()
        {
            if active {
                sort_state.asc = !sort_state.asc;
            } else {
                sort_state.column = col;
                sort_state.asc = true;
            }
        }
    });
}

pub fn sort_tracks_slice(tracks: &mut [&Track], sort: InspectSortState) {
    tracks.sort_by(|a, b| {
        let ord = match sort.column {
            InspectColumn::Position => a.position.cmp(&b.position),
            InspectColumn::Title => a
                .title
                .to_ascii_lowercase()
                .cmp(&b.title.to_ascii_lowercase()),
            InspectColumn::Artist => a
                .primary_artist_name()
                .unwrap_or("")
                .to_ascii_lowercase()
                .cmp(&b.primary_artist_name().unwrap_or("").to_ascii_lowercase()),
            InspectColumn::Album => a
                .album
                .as_ref()
                .map_or("", |x| x.name.as_str())
                .to_ascii_lowercase()
                .cmp(
                    &b.album
                        .as_ref()
                        .map_or("", |x| x.name.as_str())
                        .to_ascii_lowercase(),
                ),
            InspectColumn::Duration => a.duration_sec.cmp(&b.duration_sec),
            InspectColumn::Type => a
                .music_video_type
                .short_label()
                .cmp(b.music_video_type.short_label()),
            InspectColumn::Status => a
                .availability
                .is_available()
                .cmp(&b.availability.is_available()),
        };
        let ord = ord.then_with(|| a.position.cmp(&b.position));
        if sort.asc { ord } else { ord.reverse() }
    });
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
    let target_sid = all_sids.get(target_pos).cloned()?;
    if dragged_set.contains(target_sid.as_str()) {
        return None;
    }
    let remaining: Vec<String> = all_sids
        .iter()
        .filter(|sid| !dragged_set.contains(sid.as_str()))
        .cloned()
        .collect();
    let insert_at = remaining
        .iter()
        .position(|sid| sid == &target_sid)
        .unwrap_or(remaining.len());
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
