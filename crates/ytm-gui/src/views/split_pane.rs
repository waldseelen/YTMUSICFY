use std::collections::{HashMap, HashSet};

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::{Playlist, PlaylistSummary, Track};
use ytm_ops::normalize_track;

use crate::interaction::search_box_with_clear;
use crate::navigation::ToastState;
use crate::overlays::handle_cross_playlist_drop;
use crate::state::{AsyncState, DiffFilter, DragDropMode, DragTrackPayload, DropPrompt};
use crate::sync_queue::SyncQueue;
use crate::views::inspect_helpers::format_duration;

pub struct SplitPaneParams<'a> {
    pub salt: &'a str,
    pub tag: &'a str,
    pub pl: &'a Playlist,
    pub other_pl_title: &'a str,
    pub other_keys: &'a HashSet<String>,
    pub width: f32,
    pub height: f32,
    pub filter: DiffFilter,
    pub fuzzy: bool,
    pub is_b: bool,
    pub focus_search: bool,
    pub pointer_pos: egui::Pos2,
}

#[allow(clippy::too_many_arguments)]
pub fn render_split_pane(
    ui: &mut egui::Ui,
    p: SplitPaneParams<'_>,
    search: &mut String,
    selected: &mut HashSet<usize>,
    drop_prompt: &mut Option<DropPrompt>,
    playlist_cache: &HashMap<String, Playlist>,
    library: &AsyncState<Vec<PlaylistSummary>>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
) {
    let runtime_sec: u64 =
        p.pl.tracks
            .iter()
            .filter_map(|t| t.duration_sec.map(u64::from))
            .sum();
    ui.allocate_ui_with_layout(
        egui::vec2(p.width, p.height),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            let pane_resp = ui
                .group(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(format!(
                            "{}: {} ({}, {})",
                            p.tag,
                            p.pl.title,
                            p.pl.tracks.len(),
                            format_duration(runtime_sec)
                        ));
                        search_box_with_clear(
                            ui,
                            &format!("{}_search", p.salt),
                            search,
                            &format!("Filter {}...", p.tag),
                            115.0,
                            p.focus_search,
                        );
                    });
                    diff_table(
                        ui,
                        p.salt,
                        p.pl,
                        p.other_keys,
                        search,
                        selected,
                        p.filter,
                        p.fuzzy,
                        p.is_b,
                    );
                })
                .response;

            if let Some(payload) = pane_resp.dnd_hover_payload::<DragTrackPayload>()
                && payload.source_playlist_id != p.pl.id
            {
                ui.painter().rect_stroke(
                    pane_resp.rect,
                    3.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 175, 240)),
                    egui::StrokeKind::Inside,
                );
            }
            if let Some(payload) = pane_resp.dnd_release_payload::<DragTrackPayload>()
                && payload.source_playlist_id != p.pl.id
            {
                let prompt = DropPrompt {
                    pos: p.pointer_pos,
                    source_playlist_id: payload.source_playlist_id.clone(),
                    source_playlist_title: p.other_pl_title.to_string(),
                    target_playlist_id: p.pl.id.clone(),
                    target_playlist_title: p.pl.title.clone(),
                    tracks: payload.tracks.clone(),
                };
                handle_cross_playlist_drop(
                    ui.ctx(),
                    prompt,
                    DragDropMode::Ask,
                    drop_prompt,
                    playlist_cache,
                    library,
                    sync_queue,
                    toast,
                );
            }
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn diff_table(
    ui: &mut egui::Ui,
    salt: &str,
    pl: &Playlist,
    other_keys: &HashSet<String>,
    search: &str,
    selected: &mut HashSet<usize>,
    filter: DiffFilter,
    fuzzy: bool,
    is_b: bool,
) {
    let needle = search.trim().to_ascii_lowercase();
    let rows: Vec<(&Track, bool)> = pl
        .tracks
        .iter()
        .filter_map(|t| {
            let in_other = other_keys.contains(&track_key(t, fuzzy));
            let keep = match filter {
                DiffFilter::All => true,
                DiffFilter::OnlyA => !is_b && !in_other,
                DiffFilter::OnlyB => is_b && !in_other,
                DiffFilter::InBoth => in_other,
            };
            if !keep {
                return None;
            }
            if !needle.is_empty()
                && !t.title.to_ascii_lowercase().contains(&needle)
                && !t
                    .primary_artist_name()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&needle)
            {
                return None;
            }
            Some((t, in_other))
        })
        .collect();

    TableBuilder::new(ui)
        .id_salt(salt)
        .striped(true)
        .resizable(true)
        .column(Column::exact(22.0))
        .column(Column::exact(22.0))
        .column(Column::exact(48.0))
        .column(Column::initial(140.0).clip(true))
        .column(Column::remainder().clip(true))
        .header(20.0, |mut h| {
            for col in ["", "Sel", "Diff", "Title", "Artist"] {
                h.col(|ui| {
                    ui.strong(col);
                });
            }
        })
        .body(|body| {
            body.rows(20.0, rows.len(), |mut row| {
                let idx = row.index();
                let (t, in_other) = rows[idx];
                let mut chk = selected.contains(&t.position);
                row.col(|ui| {
                    let drag_tracks = if chk && selected.len() > 1 {
                        pl.tracks
                            .iter()
                            .filter(|item| selected.contains(&item.position))
                            .cloned()
                            .collect()
                    } else {
                        vec![t.clone()]
                    };
                    let drag = ui
                        .dnd_drag_source(
                            egui::Id::new((salt, &pl.id, idx, &t.video_id)),
                            DragTrackPayload {
                                source_playlist_id: pl.id.clone(),
                                tracks: drag_tracks,
                            },
                            |ui| ui.small("⋮⋮"),
                        )
                        .response;
                    if drag.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }
                });
                row.col(|ui| {
                    if ui.checkbox(&mut chk, "").changed() {
                        if chk {
                            selected.insert(t.position);
                        } else {
                            selected.remove(&t.position);
                        }
                    }
                });
                row.col(|ui| {
                    let (tag, color) = if in_other {
                        ("=B", egui::Color32::GRAY)
                    } else if is_b {
                        ("+B", egui::Color32::from_rgb(235, 165, 70))
                    } else {
                        ("+A", egui::Color32::from_rgb(90, 200, 120))
                    };
                    ui.colored_label(color, tag);
                });
                row.col(|ui| {
                    ui.label(&t.title);
                });
                row.col(|ui| {
                    ui.label(t.primary_artist_name().unwrap_or("-"));
                });
            });
        });
}

pub fn track_key(track: &Track, fuzzy: bool) -> String {
    if !fuzzy && !track.video_id.is_empty() {
        return track.video_id.clone();
    }
    let norm = normalize_track(track);
    format!("{}::{}", norm.artist_norm, norm.title_norm)
}
