use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{FetchCompleteness, FetchManifest, Playlist};
use ytm_store::ExportFormat;

use crate::draft_cache::DraftCache;
use crate::icons::{Icon, icon_text_button, paint_icon};
use crate::interaction::search_box_with_clear;
use crate::navigation::ToastState;
use crate::playback_ops::PlayAction;
use crate::playlist_meta_modal::PlaylistMetaModal;
use crate::state::{DragTrackPayload, SplitPaneState, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::async_banner;
use crate::views::components::show_fetch_badge;
use crate::views::inspect_helpers::format_duration;
use crate::views::inspect_rows::RowRenderOutcome;

pub(crate) fn show_load_banner(
    ui: &mut egui::Ui,
    is_loading: bool,
    load_error: Option<&str>,
    selected_playlist_id: &Option<String>,
    loading_playlists: &mut HashSet<String>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    let loading_msg = is_loading.then_some("Loading playlist...");
    async_banner(ui, loading_msg, load_error);
    if load_error.is_some()
        && let Some(pid) = selected_playlist_id.as_ref()
        && ui.button("Retry Load").clicked()
        && loading_playlists.insert(pid.clone())
    {
        let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(pid.clone()));
    }
}

pub(crate) fn handle_end_drop_zone(
    ui: &mut egui::Ui,
    pl: &Playlist,
    outcome: &mut RowRenderOutcome,
) {
    if !egui::DragAndDrop::has_any_payload(ui.ctx()) {
        return;
    }
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

pub(crate) struct InspectToolbarParams<'a> {
    pub pl: &'a Playlist,
    pub manifest: Option<&'a FetchManifest>,
    pub editable: bool,
    pub loading_playlists: &'a mut HashSet<String>,
    pub playlist_cache: &'a mut HashMap<String, Playlist>,
    pub draft_cache: &'a mut DraftCache,
    pub split: &'a mut SplitPaneState,
    pub meta_modal: &'a mut Option<PlaylistMetaModal>,
    pub search: &'a mut String,
    pub ytm_query: &'a mut String,
    pub jump_input: &'a mut String,
    pub focused_row: &'a mut Option<usize>,
    pub export_dir: &'a mut String,
    pub export_fmt: &'a mut ExportFormat,
    pub focus_search: bool,
    pub cmd_tx: &'a Sender<WorkerCmd>,
}

pub(crate) fn show_playlist_header(
    ui: &mut egui::Ui,
    p: &mut InspectToolbarParams<'_>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
) -> Vec<PlayAction> {
    let mut play_actions = Vec::new();
    let total_secs: u64 =
        p.pl.tracks
            .iter()
            .filter_map(|t| t.duration_sec.map(u64::from))
            .sum();

    ui.horizontal_wrapped(|ui| {
        ui.heading(&p.pl.title);
        ui.label(format!(
            "ID: {} | Tracks: {} | Runtime: {} | {}",
            p.pl.id,
            p.pl.tracks.len(),
            format_duration(total_secs),
            if p.editable {
                "Editable"
            } else {
                "Read-only / Locked"
            }
        ));
        show_fetch_badge(ui, p.manifest);
        if ui
            .button("Refresh")
            .on_hover_text("Reload this playlist fresh from YouTube Music")
            .clicked()
        {
            p.playlist_cache.remove(&p.pl.id);
            p.draft_cache.invalidate_playlist(&p.pl.id);
            if p.loading_playlists.insert(p.pl.id.clone()) {
                let _ = p.cmd_tx.send(WorkerCmd::LoadPlaylist(p.pl.id.clone()));
            }
        }
        let split_btn = if p.split.enabled {
            "Close Split Diff"
        } else {
            "Split Diff View"
        };
        if ui
            .selectable_label(p.split.enabled, split_btn)
            .on_hover_text(
                "Compare this playlist side-by-side against another list and drag tracks across",
            )
            .clicked()
        {
            p.split.enabled = !p.split.enabled;
        }
        if !p.pl.tracks.is_empty()
            && icon_text_button(ui, Icon::Play, "Play Playlist", 13.0, true)
                .on_hover_text("Play all tracks in order")
                .clicked()
        {
            play_actions.push(PlayAction::PlayFromList {
                tracks: p.pl.tracks.clone(),
                start_idx: 0,
            });
        }
        if p.editable {
            if icon_text_button(ui, Icon::Edit, "Edit Details", 13.0, true)
                .on_hover_text("Rename playlist, update description, or change privacy")
                .clicked()
            {
                *p.meta_modal = Some(PlaylistMetaModal::new(
                    p.pl.id.clone(),
                    p.pl.title.clone(),
                    p.pl.description.clone().unwrap_or_default(),
                    p.pl.privacy,
                ));
            }
            if icon_text_button(ui, Icon::Trash, "Stage Delete", 13.0, true)
                .on_hover_text(
                    "Stage playlist deletion into Sync (requires snapshot & typed confirmation)",
                )
                .clicked()
            {
                let eid = sync_queue.push(StagedOp::DeletePlaylist {
                    playlist_id: p.pl.id.clone(),
                    playlist_title: p.pl.title.clone(),
                });
                *toast = Some(ToastState::staged(
                    format!("Staged deletion of '{}' in Sync.", p.pl.title),
                    Some(eid),
                ));
            }
        }
    });

    if let Some(m) = p.manifest {
        match &m.completeness {
            FetchCompleteness::Truncated {
                pages_fetched,
                max_pages,
            } => {
                let err_col = egui::Color32::from_rgb(235, 95, 95);
                ui.horizontal(|ui| {
                    paint_icon(ui, Icon::Warn, 14.0, err_col);
                    ui.colored_label(
                        err_col,
                        format!(
                            "Fetch truncated at {pages_fetched}/{max_pages} pages — mutations are locked until cleanly refreshed."
                        ),
                    );
                });
            }
            FetchCompleteness::Degraded { dropped_renderers } => {
                let err_col = egui::Color32::from_rgb(235, 95, 95);
                ui.horizontal(|ui| {
                    paint_icon(ui, Icon::Warn, 14.0, err_col);
                    ui.colored_label(
                        err_col,
                        format!(
                            "{} unrecognized renderer(s) dropped during parse — mutations are locked until refreshed.",
                            dropped_renderers.len()
                        ),
                    );
                });
            }
            FetchCompleteness::PartialHidden {
                declared,
                fetched,
                hidden_estimate,
            } => {
                let warn_col = egui::Color32::from_rgb(235, 180, 70);
                ui.horizontal(|ui| {
                    paint_icon(ui, Icon::Warn, 14.0, warn_col);
                    ui.colored_label(
                        warn_col,
                        format!(
                            "PartialHidden: fetched {fetched}/{declared} tracks (~{hidden_estimate} hidden/unavailable on YouTube)."
                        ),
                    );
                });
            }
            FetchCompleteness::Complete => {}
        }
    }

    play_actions
}

pub(crate) fn show_filter_and_export_bar(
    ui: &mut egui::Ui,
    p: &mut InspectToolbarParams<'_>,
) -> Option<usize> {
    let mut scroll_target_row: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        ui.label("Filter:");
        search_box_with_clear(
            ui,
            "inspect_filter_box",
            p.search,
            "Title, artist, album...",
            160.0,
            p.focus_search,
        );
        ui.separator();
        ui.label("Quick Add:");
        let qa = search_box_with_clear(
            ui,
            "quick_add_box",
            p.ytm_query,
            "Search YouTube Music...",
            155.0,
            false,
        );
        let q_trim = p.ytm_query.trim().to_string();
        if !q_trim.is_empty() && (qa.submitted || ui.button("Search").clicked()) {
            let _ = p.cmd_tx.send(WorkerCmd::SearchTracks(q_trim));
        } else if q_trim.is_empty() {
            ui.add_enabled(false, egui::Button::new("Search"));
        }
        ui.separator();
        ui.label("# Go:");
        let jump_resp = ui.add(egui::TextEdit::singleline(p.jump_input).desired_width(42.0));
        if jump_resp.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && let Ok(n) = p.jump_input.trim().parse::<usize>()
            && n >= 1
        {
            scroll_target_row = Some((n - 1).min(p.pl.tracks.len().saturating_sub(1)));
            *p.focused_row = scroll_target_row;
        }
        ui.separator();
        egui::ComboBox::from_id_salt("export_fmt_combo")
            .selected_text(format!("{:?}", p.export_fmt))
            .show_ui(ui, |ui| {
                ui.selectable_value(p.export_fmt, ExportFormat::Json, "JSON");
                ui.selectable_value(p.export_fmt, ExportFormat::Csv, "CSV");
                ui.selectable_value(p.export_fmt, ExportFormat::M3u8, "M3U8");
            });
        ui.add(egui::TextEdit::singleline(p.export_dir).desired_width(120.0));
        if ui.button("Backup & Export").clicked() {
            let _ = p.cmd_tx.send(WorkerCmd::BackupPlaylist {
                target: p.pl.id.clone(),
                format: *p.export_fmt,
                out_dir: PathBuf::from(p.export_dir.trim()),
            });
        }
    });
    scroll_target_row
}
