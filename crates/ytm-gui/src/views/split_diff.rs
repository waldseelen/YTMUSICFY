use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, Track};

use crate::navigation::ToastState;
use crate::state::{AsyncState, DiffFilter, DropPrompt, SplitPaneState, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::split_pane::{SplitPaneParams, render_split_pane, track_key};

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    playlist_a: Option<&Playlist>,
    selected_playlist_id: &mut Option<String>,
    playlists: &[PlaylistSummary],
    library: &AsyncState<Vec<PlaylistSummary>>,
    loading_playlists: &mut HashSet<String>,
    playlist_cache: &HashMap<String, Playlist>,
    split: &mut SplitPaneState,
    sync_queue: &mut SyncQueue,
    drop_prompt: &mut Option<DropPrompt>,
    toast: &mut Option<ToastState>,
    focus_search: bool,
    cmd_tx: &Sender<WorkerCmd>,
) {
    let Some(pl_a) = playlist_a else {
        ui.label("Select Playlist A in the left sidebar first.");
        return;
    };

    ui.horizontal_wrapped(|ui| {
        ui.strong("Split Diff View");
        ui.separator();
        ui.label("Playlist B:");
        let selected_label = split
            .playlist_b_id
            .as_deref()
            .and_then(|id| playlists.iter().find(|p| p.id == id))
            .map_or_else(|| "Choose Playlist B...".to_string(), |p| p.title.clone());
        egui::ComboBox::from_id_salt("split_b_combo")
            .selected_text(selected_label)
            .show_ui(ui, |ui| {
                for p in playlists {
                    if p.id == pl_a.id {
                        continue;
                    }
                    if ui
                        .selectable_value(&mut split.playlist_b_id, Some(p.id.clone()), &p.title)
                        .clicked()
                    {
                        split.selected_b.clear();
                        if !playlist_cache.contains_key(&p.id)
                            && loading_playlists.insert(p.id.clone())
                        {
                            let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(p.id.clone()));
                        }
                    }
                }
            });
        if let Some(b_id) = split.playlist_b_id.clone()
            && ui
                .button("⇄ Swap A/B")
                .on_hover_text("Swap Left and Right playlists")
                .clicked()
        {
            let prev_a = pl_a.id.clone();
            *selected_playlist_id = Some(b_id);
            split.playlist_b_id = Some(prev_a);
            std::mem::swap(&mut split.search_a, &mut split.search_b);
            std::mem::swap(&mut split.selected_a, &mut split.selected_b);
            return;
        }
        ui.checkbox(&mut split.fuzzy, "Fuzzy match");
        ui.separator();
        ui.selectable_value(&mut split.filter, DiffFilter::All, "All");
        ui.selectable_value(&mut split.filter, DiffFilter::OnlyA, "Only A");
        ui.selectable_value(&mut split.filter, DiffFilter::OnlyB, "Only B");
        ui.selectable_value(&mut split.filter, DiffFilter::InBoth, "In Both");
    });

    let Some(b_id) = split.playlist_b_id.clone() else {
        ui.label("Pick Playlist B above (or right-click a playlist in the sidebar -> Open in Split Pane B).");
        return;
    };
    let Some(pl_b) = sync_queue.working_playlist(&b_id, playlist_cache) else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Loading Playlist B...");
        });
        return;
    };

    let keys_a: HashSet<String> = pl_a
        .tracks
        .iter()
        .map(|t| track_key(t, split.fuzzy))
        .collect();
    let keys_b: HashSet<String> = pl_b
        .tracks
        .iter()
        .map(|t| track_key(t, split.fuzzy))
        .collect();

    let only_a_tracks: Vec<Track> = pl_a
        .tracks
        .iter()
        .filter(|t| !keys_b.contains(&track_key(t, split.fuzzy)))
        .cloned()
        .collect();
    let only_b_tracks: Vec<Track> = pl_b
        .tracks
        .iter()
        .filter(|t| !keys_a.contains(&track_key(t, split.fuzzy)))
        .cloned()
        .collect();
    let both_count = pl_a
        .tracks
        .iter()
        .filter(|t| keys_b.contains(&track_key(t, split.fuzzy)))
        .count();

    ui.horizontal_wrapped(|ui| {
        ui.colored_label(
            egui::Color32::from_rgb(90, 200, 120),
            format!("+ Only A: {}", only_a_tracks.len()),
        );
        ui.colored_label(
            egui::Color32::from_rgb(235, 165, 70),
            format!("+ Only B: {}", only_b_tracks.len()),
        );
        ui.label(format!("= Both: {both_count}"));
        ui.separator();

        let can_add_b =
            pl_b.owned && !pl_b.id.eq_ignore_ascii_case("LM") && !only_a_tracks.is_empty();
        if ui
            .add_enabled(
                can_add_b,
                egui::Button::new(format!("Stage Missing A -> B (+{})", only_a_tracks.len())),
            )
            .clicked()
        {
            let cnt = only_a_tracks.len();
            let eid = sync_queue.push(StagedOp::AddTracks {
                playlist_id: pl_b.id.clone(),
                playlist_title: pl_b.title.clone(),
                tracks: only_a_tracks.clone(),
                allow_duplicates: false,
            });
            *toast = Some(ToastState::staged(
                format!("Staged +{cnt} tracks into '{}'.", pl_b.title),
                Some(eid),
            ));
        }

        let can_add_a =
            pl_a.owned && !pl_a.id.eq_ignore_ascii_case("LM") && !only_b_tracks.is_empty();
        if ui
            .add_enabled(
                can_add_a,
                egui::Button::new(format!("Stage Missing B -> A (+{})", only_b_tracks.len())),
            )
            .clicked()
        {
            let cnt = only_b_tracks.len();
            let eid = sync_queue.push(StagedOp::AddTracks {
                playlist_id: pl_a.id.clone(),
                playlist_title: pl_a.title.clone(),
                tracks: only_b_tracks.clone(),
                allow_duplicates: false,
            });
            *toast = Some(ToastState::staged(
                format!("Staged +{cnt} tracks into '{}'.", pl_a.title),
                Some(eid),
            ));
        }
    });
    ui.separator();

    let pointer_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
    let total_w = (ui.available_width() - 18.0).max(360.0);
    let ratio = split.split_ratio.clamp(0.25, 0.75);
    let left_w = (total_w * ratio).max(160.0);
    let right_w = (total_w - left_w).max(160.0);
    let avail_h = ui.available_height().max(200.0);

    ui.horizontal_top(|ui| {
        render_split_pane(
            ui,
            SplitPaneParams {
                salt: "split_a",
                tag: "A",
                pl: pl_a,
                other_pl_title: &pl_b.title,
                other_keys: &keys_b,
                width: left_w,
                height: avail_h,
                filter: split.filter,
                fuzzy: split.fuzzy,
                is_b: false,
                focus_search,
                pointer_pos,
            },
            &mut split.search_a,
            &mut split.selected_a,
            drop_prompt,
            playlist_cache,
            library,
            sync_queue,
            toast,
        );

        let (sep_rect, sep_resp) =
            ui.allocate_exact_size(egui::vec2(8.0, avail_h), egui::Sense::drag());
        if sep_resp.hovered() || sep_resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        let sep_color = if sep_resp.dragged() {
            egui::Color32::from_rgb(90, 175, 240)
        } else {
            egui::Color32::from_gray(90)
        };
        ui.painter().vline(
            sep_rect.center().x,
            sep_rect.y_range(),
            egui::Stroke::new(2.0_f32, sep_color),
        );
        if sep_resp.dragged() {
            split.split_ratio =
                (split.split_ratio + sep_resp.drag_delta().x / total_w).clamp(0.25, 0.75);
        }

        render_split_pane(
            ui,
            SplitPaneParams {
                salt: "split_b",
                tag: "B",
                pl: &pl_b,
                other_pl_title: &pl_a.title,
                other_keys: &keys_a,
                width: right_w,
                height: avail_h,
                filter: split.filter,
                fuzzy: split.fuzzy,
                is_b: true,
                focus_search: false,
                pointer_pos,
            },
            &mut split.search_b,
            &mut split.selected_b,
            drop_prompt,
            playlist_cache,
            library,
            sync_queue,
            toast,
        );
    });
}
