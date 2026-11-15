use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::{Playlist, PlaylistSummary, Track, TrackAvailability};
use ytm_store::ExportFormat;

use crate::state::{
    AsyncState, DragTrackPayload, DropPrompt, InspectColumn, InspectSortState, SplitPaneState,
    WorkerCmd,
};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::inspect_helpers::{
    compute_drop_reorder, format_duration, show_quick_search_results, show_selection_bar,
    show_sort_stage_bar, sort_header, sort_tracks_slice,
};
use crate::views::{async_banner, split_diff};

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    playlist: Option<&Playlist>,
    is_loading: bool,
    load_error: Option<&str>,
    playlists: &[PlaylistSummary],
    loading_playlists: &mut HashSet<String>,
    playlist_cache: &mut HashMap<String, Playlist>,
    sync_queue: &mut SyncQueue,
    sort_state: &mut InspectSortState,
    selected_positions: &mut HashSet<usize>,
    search: &mut String,
    ytm_query: &mut String,
    search_results: &AsyncState<Vec<Track>>,
    export_dir: &mut String,
    export_fmt: &mut ExportFormat,
    split: &mut SplitPaneState,
    drop_prompt: &mut Option<DropPrompt>,
    toast: &mut Option<(bool, String)>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    let loading_msg = is_loading.then_some("Loading playlist...");
    async_banner(ui, loading_msg, load_error);

    let Some(pl) = playlist else {
        ui.label("Select a playlist from the left sidebar to inspect tracks.");
        return;
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
    });

    if split.enabled {
        ui.separator();
        split_diff::show(
            ui,
            Some(pl),
            playlists,
            loading_playlists,
            playlist_cache,
            split,
            sync_queue,
            drop_prompt,
            toast,
            cmd_tx,
        );
        return;
    }

    ui.horizontal_wrapped(|ui| {
        ui.label("Filter:");
        ui.add(egui::TextEdit::singleline(search).desired_width(170.0));
        ui.separator();
        ui.label("Quick Add:");
        ui.add(
            egui::TextEdit::singleline(ytm_query)
                .hint_text("Search YouTube Music...")
                .desired_width(170.0),
        );
        if ui
            .add_enabled(!ytm_query.trim().is_empty(), egui::Button::new("Search"))
            .clicked()
        {
            let _ = cmd_tx.send(WorkerCmd::SearchTracks(ytm_query.trim().to_string()));
        }
        ui.separator();
        egui::ComboBox::from_id_salt("export_fmt_combo")
            .selected_text(format!("{export_fmt:?}"))
            .show_ui(ui, |ui| {
                ui.selectable_value(export_fmt, ExportFormat::Json, "JSON");
                ui.selectable_value(export_fmt, ExportFormat::Csv, "CSV");
                ui.selectable_value(export_fmt, ExportFormat::M3u8, "M3U8");
            });
        ui.add(egui::TextEdit::singleline(export_dir).desired_width(140.0));
        if ui.button("Backup & Export").clicked() {
            let _ = cmd_tx.send(WorkerCmd::BackupPlaylist {
                target: pl.id.clone(),
                format: *export_fmt,
                out_dir: PathBuf::from(export_dir.trim()),
            });
        }
    });

    show_quick_search_results(ui, pl, editable, search_results, sync_queue, toast);

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

    show_selection_bar(ui, pl, editable, sync_queue, selected_positions, toast);
    show_sort_stage_bar(
        ui,
        pl,
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
    let mut intra_reorder: Option<(Vec<String>, usize)> = None;
    let mut ctx_remove: Option<Track> = None;

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
            body.rows(21.0, visible.len(), |mut row| {
                let t = visible[row.index()];
                let mut checked = selected_positions.contains(&t.position);
                row.col(|ui| {
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
                        tracks: drag_tracks,
                    };
                    let drag_resp = ui
                        .dnd_drag_source(
                            egui::Id::new(("insp_row", &pl.id, t.position)),
                            payload,
                            |ui| {
                                ui.small("⋮⋮");
                            },
                        )
                        .response;
                    if can_drag_reorder {
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
                                intra_reorder = Some((sids, t.position));
                            }
                        }
                    }
                });
                row.col(|ui| {
                    if ui.checkbox(&mut checked, "").changed() {
                        if checked {
                            selected_positions.insert(t.position);
                        } else {
                            selected_positions.remove(&t.position);
                        }
                    }
                });
                row.col(|ui| {
                    ui.label((t.position + 1).to_string());
                });
                row.col(|ui| {
                    let lbl = ui.add(egui::Label::new(&t.title).sense(egui::Sense::click()));
                    lbl.context_menu(|ui| {
                        if ui.button("Copy YTM Watch URL").clicked() {
                            ui.ctx().copy_text(format!(
                                "https://music.youtube.com/watch?v={}",
                                t.video_id
                            ));
                            *toast = Some((true, "Copied track URL to clipboard.".to_string()));
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(editable, egui::Button::new("Remove Track (Stage)"))
                            .clicked()
                        {
                            ctx_remove = Some(t.clone());
                            ui.close_menu();
                        }
                    });
                });
                row.col(|ui| {
                    ui.label(t.primary_artist_name().unwrap_or("-"));
                });
                row.col(|ui| {
                    ui.label(t.album.as_ref().map_or("-", |a| a.name.as_str()));
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
                    let color = if t.availability.is_available() {
                        egui::Color32::from_rgb(90, 200, 120)
                    } else {
                        egui::Color32::from_rgb(235, 100, 100)
                    };
                    let text = match &t.availability {
                        TrackAvailability::Available => "OK",
                        TrackAvailability::Deleted => "Deleted",
                        TrackAvailability::Private => "Private",
                        TrackAvailability::RegionBlocked => "RegionBlocked",
                        TrackAvailability::Unplayable(r) => r.as_str(),
                    };
                    ui.colored_label(color, text);
                });
            });
        });

    if let Some((dragged_sids, target_pos)) = intra_reorder {
        if let Some(target_order) = compute_drop_reorder(pl, &dragged_sids, target_pos) {
            match sync_queue.stage_reorder(&pl.id, &pl.title, target_order, playlist_cache) {
                Ok(n) => {
                    selected_positions.clear();
                    *toast = Some((
                        true,
                        if n == 0 {
                            "Order returned to baseline; no Sync moves needed.".to_string()
                        } else {
                            format!("Staged row drag ({n} LIS moves) in Sync.")
                        },
                    ));
                }
                Err(e) => *toast = Some((false, e)),
            }
        }
    }
    if let Some(track) = ctx_remove {
        sync_queue.push(StagedOp::RemoveTracks {
            playlist_id: pl.id.clone(),
            playlist_title: pl.title.clone(),
            tracks: vec![track.clone()],
        });
        *toast = Some((true, format!("Staged '-' '{}' in Sync.", track.title)));
    }
}
