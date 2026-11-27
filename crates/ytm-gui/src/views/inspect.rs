use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::{FetchManifest, Playlist, PlaylistAccess, PlaylistSummary, Track};
use ytm_store::ExportFormat;

use crate::artwork::ArtworkCache;
use crate::draft_cache::DraftCache;
use crate::navigation::ToastState;
use crate::playback_ops::PlayAction;
use crate::playlist_meta_modal::PlaylistMetaModal;
use crate::state::{
    AsyncState, DropPrompt, InspectColumn, InspectSortState, SplitPaneState, WorkerCmd,
};
use crate::sync_queue::SyncQueue;
use crate::views::inspect_actions::{apply_row_outcome, handle_table_keyboard};
use crate::views::inspect_helpers::{
    show_quick_search_results, show_selection_bar, show_sort_stage_bar, sort_header,
    sort_tracks_slice,
};
use crate::views::inspect_rows::render_inspect_rows;
use crate::views::inspect_toolbar::{
    InspectToolbarParams, handle_end_drop_zone, show_filter_and_export_bar, show_load_banner,
    show_playlist_header,
};
use crate::views::split_diff;

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    lang: crate::i18n::ResolvedLang,
    playlist: Option<&Playlist>,
    manifest: Option<&FetchManifest>,
    selected_playlist_id: &mut Option<String>,
    is_loading: bool,
    load_error: Option<&str>,
    playlists: &[PlaylistSummary],
    library: &AsyncState<Vec<PlaylistSummary>>,
    loading_playlists: &mut HashSet<String>,
    playlist_cache: &mut HashMap<String, Playlist>,
    draft_cache: &mut DraftCache,
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
    artwork: &mut ArtworkCache,
    toast: &mut Option<ToastState>,
    focus_search: bool,
    cmd_tx: &Sender<WorkerCmd>,
) -> Vec<PlayAction> {
    let mut play_actions = Vec::new();
    show_load_banner(
        ui,
        is_loading,
        load_error,
        selected_playlist_id,
        loading_playlists,
        cmd_tx,
    );

    let Some(pl) = playlist.cloned() else {
        ui.label("Select a playlist from the left sidebar to inspect tracks.");
        return play_actions;
    };
    let manifest_locked = manifest.is_some_and(|m| m.completeness.is_mutation_locked());
    let editable = pl.access() == PlaylistAccess::EditableOwned && !manifest_locked;

    let mut toolbar_params = InspectToolbarParams {
        pl: &pl,
        manifest,
        editable,
        loading_playlists,
        playlist_cache,
        draft_cache,
        split,
        meta_modal,
        search,
        ytm_query,
        jump_input,
        focused_row,
        export_dir,
        export_fmt,
        focus_search,
        cmd_tx,
    };
    play_actions.extend(show_playlist_header(
        ui,
        &mut toolbar_params,
        sync_queue,
        toast,
    ));

    if toolbar_params.split.enabled {
        ui.separator();
        split_diff::show(
            ui,
            Some(&pl),
            selected_playlist_id,
            playlists,
            library,
            toolbar_params.loading_playlists,
            toolbar_params.playlist_cache,
            toolbar_params.split,
            sync_queue,
            drop_prompt,
            toast,
            focus_search,
            cmd_tx,
        );
        return play_actions;
    }

    let mut scroll_target_row = show_filter_and_export_bar(ui, &mut toolbar_params);
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
                sort_header(&mut header, "Duration", InspectColumn::Duration, sort_state);
                sort_header(&mut header, "Type", InspectColumn::Type, sort_state);
                sort_header(&mut header, "Status", InspectColumn::Status, sort_state);
            })
            .body(|body| {
                row_out = Some(render_inspect_rows(
                    body,
                    lang,
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
        if can_drag_reorder {
            handle_end_drop_zone(ui, &pl, &mut outcome);
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
