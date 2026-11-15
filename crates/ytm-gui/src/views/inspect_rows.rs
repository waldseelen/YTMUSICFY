use std::collections::HashSet;

use eframe::egui;
use egui_extras::TableBody;
use ytm_core::{Playlist, PlaylistSummary, Track, TrackAvailability};

use crate::interaction::{apply_range_select, auto_scroll_on_drag, paint_drag_ghost};
use crate::navigation::ToastState;
use crate::state::DragTrackPayload;
use crate::sync_queue::SyncQueue;

/// Per-frame output collected from rendering track rows.
pub struct RowRenderOutcome {
    pub intra_reorder: Option<(Vec<String>, usize)>,
    pub batch_copy_move: Option<(String, String, Vec<Track>, bool)>,
    pub remove_single: Option<Track>,
    pub filter_artist: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn render_inspect_rows(
    mut body: TableBody<'_>,
    pl: &Playlist,
    visible: &[&Track],
    editable: bool,
    can_drag_reorder: bool,
    playlists: &[PlaylistSummary],
    selected_positions: &mut HashSet<usize>,
    last_clicked: &mut Option<usize>,
    focused_row: &mut Option<usize>,
    scroll_target_row: Option<usize>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
) -> RowRenderOutcome {
    let mut out = RowRenderOutcome {
        intra_reorder: None,
        batch_copy_move: None,
        remove_single: None,
        filter_artist: None,
    };
    let visible_positions: Vec<usize> = visible.iter().map(|t| t.position).collect();
    let shift_held = body.ui_mut().input(|i| i.modifiers.shift);
    let ctrl_held = body
        .ui_mut()
        .input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
    let table_clip = body.ui_mut().clip_rect();
    auto_scroll_on_drag(body.ui_mut(), table_clip);

    if let Some(payload) = egui::DragAndDrop::payload::<DragTrackPayload>(body.ui_mut().ctx()) {
        let label = if payload.tracks.len() > 1 {
            format!("♪ {} tracks", payload.tracks.len())
        } else if let Some(first) = payload.tracks.first() {
            format!("♪ {}", first.title)
        } else {
            "♪ Track".to_string()
        };
        paint_drag_ghost(body.ui_mut().ctx(), &label);
    }

    body.rows(22.0, visible.len(), |mut row| {
        let idx = row.index();
        let t = visible[idx];
        let mut checked = selected_positions.contains(&t.position);
        let is_focused = *focused_row == Some(idx);
        row.set_selected(checked || is_focused);

        let drag_tracks = if checked && selected_positions.len() > 1 {
            pl.tracks
                .iter()
                .filter(|item| selected_positions.contains(&item.position))
                .cloned()
                .collect()
        } else {
            vec![t.clone()]
        };
        let payload = DragTrackPayload {
            source_playlist_id: pl.id.clone(),
            tracks: drag_tracks.clone(),
        };

        row.col(|ui| {
            let drag_resp = ui
                .dnd_drag_source(
                    egui::Id::new(("insp_row", &pl.id, t.position)),
                    payload.clone(),
                    |ui| {
                        if can_drag_reorder {
                            ui.small("⋮⋮");
                        } else {
                            ui.colored_label(egui::Color32::DARK_GRAY, "⋮⋮");
                        }
                    },
                )
                .response;
            if drag_resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if drag_resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if !can_drag_reorder {
                drag_resp.clone().on_hover_text(
                    "Intra-playlist reorder requires an editable playlist sorted by # Asc with empty filter (you can still drag onto left playlists).",
                );
            } else {
                if drag_resp.dnd_hover_payload::<DragTrackPayload>().is_some() {
                    ui.painter().hline(
                        drag_resp.rect.x_range(),
                        drag_resp.rect.top(),
                        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 175, 240)),
                    );
                }
                if let Some(p) = drag_resp.dnd_release_payload::<DragTrackPayload>()
                    && p.source_playlist_id == pl.id
                {
                    let sids: Vec<String> = p
                        .tracks
                        .iter()
                        .filter_map(|tr| tr.set_video_id.clone())
                        .collect();
                    if !sids.is_empty() {
                        out.intra_reorder = Some((sids, t.position));
                    }
                }
            }
        });

        row.col(|ui| {
            if ui.checkbox(&mut checked, "").changed() {
                handle_row_select(
                    idx,
                    t.position,
                    checked,
                    shift_held,
                    &visible_positions,
                    selected_positions,
                    last_clicked,
                    focused_row,
                );
            }
        });

        row.col(|ui| {
            let pos_resp = ui.add(
                egui::Label::new((t.position + 1).to_string()).sense(egui::Sense::click()),
            );
            if scroll_target_row == Some(idx) || is_focused {
                pos_resp.scroll_to_me(Some(egui::Align::Center));
            }
        });

        row.col(|ui| {
            let title_resp = ui.dnd_drag_source(
                egui::Id::new(("insp_title_drag", &pl.id, t.position)),
                payload.clone(),
                |ui| ui.label(&t.title),
            );
            let r = title_resp.response.interact(egui::Sense::click());
            if r.clicked() {
                let next = if ctrl_held { !checked } else { true };
                if !ctrl_held && !shift_held {
                    selected_positions.clear();
                }
                handle_row_select(
                    idx,
                    t.position,
                    next,
                    shift_held,
                    &visible_positions,
                    selected_positions,
                    last_clicked,
                    focused_row,
                );
            }
            attach_row_context_menu(
                &r,
                pl,
                t,
                &drag_tracks,
                editable,
                playlists,
                &mut out,
                sync_queue,
                toast,
            );
        });

        row.col(|ui| {
            let r = ui.add(
                egui::Label::new(t.primary_artist_name().unwrap_or("-"))
                    .sense(egui::Sense::click()),
            );
            attach_row_context_menu(
                &r,
                pl,
                t,
                &drag_tracks,
                editable,
                playlists,
                &mut out,
                sync_queue,
                toast,
            );
        });

        row.col(|ui| {
            let r = ui.add(
                egui::Label::new(t.album.as_ref().map_or("-", |a| a.name.as_str()))
                    .sense(egui::Sense::click()),
            );
            attach_row_context_menu(
                &r,
                pl,
                t,
                &drag_tracks,
                editable,
                playlists,
                &mut out,
                sync_queue,
                toast,
            );
        });

        row.col(|ui| {
            ui.label(
                t.duration_sec
                    .map_or_else(|| "-".to_string(), |d| d.to_string()),
            );
        });
        row.col(|ui| {
            ui.label(t.music_video_type.short_label());
        });
        row.col(|ui| {
            let (icon, color, text) = match &t.availability {
                TrackAvailability::Available => {
                    ("✓", egui::Color32::from_rgb(90, 200, 120), "OK")
                }
                TrackAvailability::Deleted => {
                    ("✕", egui::Color32::from_rgb(235, 100, 100), "Deleted")
                }
                TrackAvailability::Private => {
                    ("⚠", egui::Color32::from_rgb(235, 100, 100), "Private")
                }
                TrackAvailability::RegionBlocked => (
                    "⚠",
                    egui::Color32::from_rgb(235, 100, 100),
                    "RegionBlocked",
                ),
                TrackAvailability::Unplayable(r) => {
                    ("✕", egui::Color32::from_rgb(235, 100, 100), r.as_str())
                }
            };
            ui.colored_label(color, format!("{icon} {text}"));
        });
    });

    out
}

#[allow(clippy::too_many_arguments)]
fn handle_row_select(
    idx: usize,
    pos: usize,
    want_selected: bool,
    shift_held: bool,
    visible_positions: &[usize],
    selected: &mut HashSet<usize>,
    last_clicked: &mut Option<usize>,
    focused_row: &mut Option<usize>,
) {
    *focused_row = Some(idx);
    if shift_held && let Some(anchor) = *last_clicked {
        apply_range_select(visible_positions, anchor, pos, selected);
    } else if want_selected {
        selected.insert(pos);
        *last_clicked = Some(pos);
    } else {
        selected.remove(&pos);
        *last_clicked = Some(pos);
    }
}

#[allow(clippy::too_many_arguments)]
fn attach_row_context_menu(
    resp: &egui::Response,
    pl: &Playlist,
    track: &Track,
    selection_tracks: &[Track],
    editable: bool,
    playlists: &[PlaylistSummary],
    out: &mut RowRenderOutcome,
    _sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
) {
    resp.context_menu(|ui| {
        let url = format!("https://music.youtube.com/watch?v={}", track.video_id);
        if ui.button("Copy YTM Watch URL").clicked() {
            ui.ctx().copy_text(url.clone());
            *toast = Some(ToastState::info(true, "Copied track URL to clipboard."));
            ui.close_menu();
        }
        if ui.button("Open in Browser").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            ui.close_menu();
        }
        if let Some(artist) = track.primary_artist_name()
            && ui.button(format!("Filter by Artist: {artist}")).clicked()
        {
            out.filter_artist = Some(artist.to_string());
            ui.close_menu();
        }
        ui.separator();
        let batch_cnt = selection_tracks.len();
        ui.menu_button(format!("Copy ({batch_cnt}) to Playlist..."), |ui| {
            for dest in playlists {
                if !dest.owned || dest.id.eq_ignore_ascii_case("LM") || dest.id == pl.id {
                    continue;
                }
                if ui.button(&dest.title).clicked() {
                    out.batch_copy_move = Some((
                        dest.id.clone(),
                        dest.title.clone(),
                        selection_tracks.to_vec(),
                        false,
                    ));
                    ui.close_menu();
                }
            }
        });
        ui.add_enabled_ui(editable, |ui| {
            ui.menu_button(format!("Move ({batch_cnt}) to Playlist..."), |ui| {
                for dest in playlists {
                    if !dest.owned || dest.id.eq_ignore_ascii_case("LM") || dest.id == pl.id {
                        continue;
                    }
                    if ui.button(&dest.title).clicked() {
                        out.batch_copy_move = Some((
                            dest.id.clone(),
                            dest.title.clone(),
                            selection_tracks.to_vec(),
                            true,
                        ));
                        ui.close_menu();
                    }
                }
            });
        });
        if ui
            .add_enabled(editable, egui::Button::new("Remove Track (Stage)"))
            .clicked()
        {
            out.remove_single = Some(track.clone());
            ui.close_menu();
        }
    });
}
