use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::{Playlist, PlaylistSummary, Track};
use ytm_store::ExportFormat;

use crate::artwork::ArtworkCache;
use crate::interaction::search_box_with_clear;
use crate::navigation::ToastState;
use crate::playback_ops::PlayAction;
use crate::playlist_meta_modal::PlaylistMetaModal;
use crate::state::{
    AsyncState, ConfirmedMutation, DragTrackPayload, DropPrompt, InspectColumn, InspectSortState,
    PendingMutation, SplitPaneState, WorkerCmd,
};
use crate::sync_queue::SyncQueue;
use crate::views::inspect_actions::{apply_row_outcome, handle_table_keyboard};
use crate::views::inspect_helpers::{
    format_duration, show_quick_search_results, show_selection_bar, show_sort_stage_bar,
    sort_header, sort_tracks_slice,
};
use crate::views::inspect_rows::render_inspect_rows;
use crate::views::{async_banner, split_diff};

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    playlist: Option<&Playlist>,
    selected_playlist_id: &mut Option<String>,
    is_loading: bool,
    load_error: Option<&str>,
    playlists: &[PlaylistSummary],
    library: &AsyncState<Vec<PlaylistSummary>>,
    loading_playlists: &mut HashSet<String>,
    playlist_cache: &mut HashMap<String, Playlist>,
    sync_queue: &mut SyncQueue,
    sort_state: &mut InspectSortState,
    selected_positions: &mut HashSet<usize>,
    last_clicked: &mut Option<usize>,
    focused_row: &mut Option<usize>,
    jump_input: &mut String,
    search: &mut String,
    ytm_query: &mut String,
    search_results: &AsyncState<Vec<Track>>,
    export_dir: &mut String,
    export_fmt: &mut ExportFormat,
    split: &mut SplitPaneState,
    drop_prompt: &mut Option<DropPrompt>,
    meta_modal: &mut Option<PlaylistMetaModal>,
    pending_modal: &mut Option<PendingMutation>,
    artwork: &mut ArtworkCache,
    toast: &mut Option<ToastState>,
    focus_search: bool,
    cmd_tx: &Sender<WorkerCmd>,
) -> Vec<PlayAction> {
    let mut play_actions = Vec::new();
    let loading_msg = is_loading.then_some("Loading playlist...");
    async_banner(ui, loading_msg, load_error);
    if load_error.is_some()
        && let Some(pid) = selected_playlist_id.as_ref()
        && ui.button("Retry Load").clicked()
        && loading_playlists.insert(pid.clone())
    {
        let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(pid.clone()));
    }

    let Some(pl) = playlist.cloned() else {
        ui.label("Select a playlist from the left sidebar to inspect tracks.");
        return play_actions;
    };

    let total_secs: u64 = pl
        .tracks
        .iter()
        .filter_map(|t| t.duration_sec.map(u64::from))
        .sum();
    let editable = pl.owned && !pl.id.eq_ignore_ascii_case("LM");

    ui.horizontal_wrapped(|ui| {
        ui.heading(&pl.title);
        ui.label(format!(
            "ID: {} | Tracks: {} | Runtime: {} | {}",
            pl.id,
            pl.tracks.len(),
            format_duration(total_secs),
            if editable {
                "Editable"
            } else {
                "Read-only / Copy source"
            }
        ));
        if ui.button("Refresh").clicked() {
            playlist_cache.remove(&pl.id);
            if loading_playlists.insert(pl.id.clone()) {
                let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(pl.id.clone()));
            }
        }
        let split_btn = if split.enabled {
            "Close Split Diff"
        } else {
            "Split Diff View"
        };
        if ui.selectable_label(split.enabled, split_btn).clicked() {
            split.enabled = !split.enabled;
        }
        if !pl.tracks.is_empty()
            && ui
                .button("▶ Play Playlist")
                .on_hover_text("Play all tracks in order")
                .clicked()
        {
            play_actions.push(PlayAction::PlayFromList {
                tracks: pl.tracks.clone(),
                start_idx: 0,
            });
        }
        if editable {
            if ui.button("✎ Edit Details").clicked() {
                *meta_modal = Some(PlaylistMetaModal::new(
                    pl.id.clone(),
                    pl.title.clone(),
                    pl.description.clone().unwrap_or_default(),
                    pl.privacy,
                ));
            }
            if ui.button("🗑 Delete").clicked() {
                *pending_modal = Some(PendingMutation {
                    title: format!("Delete Playlist '{}'", pl.title),
                    summary: format!(
                        "Permanently delete '{}' ({}) on YouTube Music? A pre-delete snapshot will be saved.",
                        pl.title, pl.id
                    ),
                    action: ConfirmedMutation::DeletePlaylist {
                        playlist_id: pl.id.clone(),
                        playlist_title: pl.title.clone(),
                    },
                });
            }
        }
    });

    if split.enabled {
        ui.separator();
        split_diff::show(
            ui,
            Some(&pl),
            selected_playlist_id,
            playlists,
            library,
            loading_playlists,
            playlist_cache,
            split,
            sync_queue,
            drop_prompt,
            toast,
            focus_search,
            cmd_tx,
        );
        return play_actions;
    }

    let mut scroll_target_row: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        ui.label("Filter:");
        search_box_with_clear(
            ui,
            "inspect_filter_box",
            search,
            "Title, artist, album...",
            160.0,
            focus_search,
        );
        ui.separator();
        ui.label("Quick Add:");
        let qa = search_box_with_clear(
            ui,
            "quick_add_box",
            ytm_query,
            "Search YouTube Music...",
            155.0,
            false,
        );
        let q_trim = ytm_query.trim().to_string();
        if !q_trim.is_empty() && (qa.submitted || ui.button("Search").clicked()) {
            let _ = cmd_tx.send(WorkerCmd::SearchTracks(q_trim));
        } else if q_trim.is_empty() {
            ui.add_enabled(false, egui::Button::new("Search"));
        }
        ui.separator();
        ui.label("# Go:");
        let jump_resp = ui.add(egui::TextEdit::singleline(jump_input).desired_width(42.0));
        if jump_resp.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && let Ok(n) = jump_input.trim().parse::<usize>()
            && n >= 1
        {
            scroll_target_row = Some((n - 1).min(pl.tracks.len().saturating_sub(1)));
            *focused_row = scroll_target_row;
        }
        ui.separator();
        egui::ComboBox::from_id_salt("export_fmt_combo")
            .selected_text(format!("{export_fmt:?}"))
            .show_ui(ui, |ui| {
                ui.selectable_value(export_fmt, ExportFormat::Json, "JSON");
                ui.selectable_value(export_fmt, ExportFormat::Csv, "CSV");
                ui.selectable_value(export_fmt, ExportFormat::M3u8, "M3U8");
            });
        ui.add(egui::TextEdit::singleline(export_dir).desired_width(120.0));
        if ui.button("Backup & Export").clicked() {
            let _ = cmd_tx.send(WorkerCmd::BackupPlaylist {
                target: pl.id.clone(),
                format: *export_fmt,
                out_dir: PathBuf::from(export_dir.trim()),
            });
        }
    });

    play_actions.extend(show_quick_search_results(
        ui,
        &pl,
        editable,
        search_results,
        sync_queue,
        toast,
    ));

    let needle = search.trim().to_ascii_lowercase();
    let mut visible: Vec<&Track> = pl
        .tracks
        .iter()
        .filter(|t| {
            needle.is_empty()
                || t.title.to_ascii_lowercase().contains(&needle)
                || t.primary_artist_name()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&needle)
                || t.album
                    .as_ref()
                    .is_some_and(|a| a.name.to_ascii_lowercase().contains(&needle))
                || t.video_id.to_ascii_lowercase().contains(&needle)
        })
        .collect();

    sort_tracks_slice(&mut visible, *sort_state);
    if let Some(act) = handle_table_keyboard(
        ui.ctx(),
        &pl,
        &visible,
        editable,
        selected_positions,
        last_clicked,
        focused_row,
        &mut scroll_target_row,
        sync_queue,
        toast,
    ) {
        play_actions.push(act);
    }

    show_selection_bar(ui, &pl, editable, sync_queue, selected_positions, toast);
    show_sort_stage_bar(
        ui,
        &pl,
        editable,
        playlist_cache,
        sync_queue,
        sort_state,
        toast,
    );
    ui.separator();

    let can_drag_reorder = editable
        && sort_state.column == InspectColumn::Position
        && sort_state.asc
        && needle.is_empty();
    let mut row_out = None;
    let table_h = (ui.available_height() - if can_drag_reorder { 24.0 } else { 0.0 }).max(120.0);

    ui.allocate_ui(egui::vec2(ui.available_width(), table_h), |ui| {
        TableBuilder::new(ui)
            .id_salt("inspect_tbl")
            .striped(true)
            .resizable(true)
            .column(Column::exact(24.0))
            .column(Column::exact(26.0))
            .column(Column::exact(38.0))
            .column(Column::initial(220.0).clip(true))
            .column(Column::initial(155.0).clip(true))
            .column(Column::initial(145.0).clip(true))
            .column(Column::exact(58.0))
            .column(Column::exact(48.0))
            .column(Column::remainder().clip(true))
            .header(22.0, |mut header| {
                header.col(|ui| {
                    ui.strong("");
                });
                header.col(|ui| {
                    let all_selected = !visible.is_empty()
                        && visible
                            .iter()
                            .all(|t| selected_positions.contains(&t.position));
                    let mut chk = all_selected;
                    if ui.checkbox(&mut chk, "").changed() {
                        if chk {
                            for t in &visible {
                                selected_positions.insert(t.position);
                            }
                        } else {
                            selected_positions.clear();
                        }
                    }
                });
                sort_header(&mut header, "#", InspectColumn::Position, sort_state);
                sort_header(&mut header, "Title", InspectColumn::Title, sort_state);
                sort_header(&mut header, "Artist", InspectColumn::Artist, sort_state);
                sort_header(&mut header, "Album", InspectColumn::Album, sort_state);
                sort_header(&mut header, "Dur(s)", InspectColumn::Duration, sort_state);
                sort_header(&mut header, "Type", InspectColumn::Type, sort_state);
                sort_header(&mut header, "Status", InspectColumn::Status, sort_state);
            })
            .body(|body| {
                row_out = Some(render_inspect_rows(
                    body,
                    &pl,
                    &visible,
                    editable,
                    can_drag_reorder,
                    playlists,
                    selected_positions,
                    last_clicked,
                    focused_row,
                    scroll_target_row,
                    sync_queue,
                    artwork,
                    toast,
                ));
            });
    });

    if let Some(mut outcome) = row_out {
        if can_drag_reorder && egui::DragAndDrop::has_any_payload(ui.ctx()) {
            let end_resp =
                ui.allocate_response(egui::vec2(ui.available_width(), 20.0), egui::Sense::hover());
            let hovered = end_resp.dnd_hover_payload::<DragTrackPayload>().is_some();
            ui.painter().text(
                end_resp.rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drop here to move to end of playlist",
                egui::FontId::proportional(11.0),
                if hovered {
                    egui::Color32::from_rgb(90, 175, 240)
                } else {
                    egui::Color32::GRAY
                },
            );
            if let Some(p) = end_resp.dnd_release_payload::<DragTrackPayload>()
                && p.source_playlist_id == pl.id
            {
                let sids: Vec<String> = p
                    .tracks
                    .iter()
                    .filter_map(|tr| tr.set_video_id.clone())
                    .collect();
                if !sids.is_empty() {
                    outcome.intra_reorder = Some((sids, pl.tracks.len()));
                }
            }
        }
        play_actions.extend(apply_row_outcome(
            &pl,
            outcome,
            playlist_cache,
            sync_queue,
            selected_positions,
            search,
            toast,
        ));
    }
    play_actions
}
