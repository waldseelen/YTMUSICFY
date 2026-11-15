use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::{Playlist, PlaylistSummary, Track};
use ytm_ops::normalize_track;

use crate::state::{DiffFilter, DragTrackPayload, DropPrompt, SplitPaneState, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    playlist_a: Option<&Playlist>,
    playlists: &[PlaylistSummary],
    loading_playlists: &mut HashSet<String>,
    playlist_cache: &HashMap<String, Playlist>,
    split: &mut SplitPaneState,
    sync_queue: &mut SyncQueue,
    drop_prompt: &mut Option<DropPrompt>,
    toast: &mut Option<(bool, String)>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    let Some(pl_a) = playlist_a else {
        ui.label("Select Playlist A in the left sidebar first.");
        return;
    };

    ui.horizontal(|ui| {
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
                        && !playlist_cache.contains_key(&p.id)
                        && loading_playlists.insert(p.id.clone())
                    {
                        let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(p.id.clone()));
                    }
                }
            });
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

    ui.horizontal(|ui| {
        ui.colored_label(
            egui::Color32::from_rgb(90, 200, 120),
            format!("Only in A: {}", only_a_tracks.len()),
        );
        ui.colored_label(
            egui::Color32::from_rgb(235, 165, 70),
            format!("Only in B: {}", only_b_tracks.len()),
        );
        ui.label(format!("In Both: {both_count}"));
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
            sync_queue.push(StagedOp::AddTracks {
                playlist_id: pl_b.id.clone(),
                playlist_title: pl_b.title.clone(),
                tracks: only_a_tracks.clone(),
                allow_duplicates: false,
            });
            *toast = Some((true, format!("Staged +{cnt} tracks into '{}'.", pl_b.title)));
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
            sync_queue.push(StagedOp::AddTracks {
                playlist_id: pl_a.id.clone(),
                playlist_title: pl_a.title.clone(),
                tracks: only_b_tracks.clone(),
                allow_duplicates: false,
            });
            *toast = Some((true, format!("Staged +{cnt} tracks into '{}'.", pl_a.title)));
        }
    });
    ui.separator();

    let pointer_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
    let half_w = ((ui.available_width() - 12.0) * 0.5).max(180.0);
    let avail_h = ui.available_height().max(200.0);

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(half_w, avail_h),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                let pane_resp = ui
                    .group(|ui| {
                        ui.strong(format!("A: {} ({})", pl_a.title, pl_a.tracks.len()));
                        diff_table(ui, "split_a", pl_a, &keys_b, split, false);
                    })
                    .response;
                if let Some(payload) = pane_resp.dnd_release_payload::<DragTrackPayload>()
                    && payload.source_playlist_id != pl_a.id
                {
                    *drop_prompt = Some(DropPrompt {
                        pos: pointer_pos,
                        source_playlist_id: payload.source_playlist_id.clone(),
                        source_playlist_title: pl_b.title.clone(),
                        target_playlist_id: pl_a.id.clone(),
                        target_playlist_title: pl_a.title.clone(),
                        tracks: payload.tracks.clone(),
                    });
                }
            },
        );

        ui.allocate_ui_with_layout(
            egui::vec2(half_w, avail_h),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                let pane_resp = ui
                    .group(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(format!("B: {} ({})", pl_b.title, pl_b.tracks.len()));
                            ui.add(
                                egui::TextEdit::singleline(&mut split.search_b)
                                    .hint_text("Filter B...")
                                    .desired_width(130.0),
                            );
                        });
                        diff_table(ui, "split_b", &pl_b, &keys_a, split, true);
                    })
                    .response;
                if let Some(payload) = pane_resp.dnd_release_payload::<DragTrackPayload>()
                    && payload.source_playlist_id != pl_b.id
                {
                    *drop_prompt = Some(DropPrompt {
                        pos: pointer_pos,
                        source_playlist_id: payload.source_playlist_id.clone(),
                        source_playlist_title: pl_a.title.clone(),
                        target_playlist_id: pl_b.id.clone(),
                        target_playlist_title: pl_b.title.clone(),
                        tracks: payload.tracks.clone(),
                    });
                }
            },
        );
    });
}

fn diff_table(
    ui: &mut egui::Ui,
    salt: &str,
    pl: &Playlist,
    other_keys: &HashSet<String>,
    split: &SplitPaneState,
    is_b: bool,
) {
    let needle = if is_b {
        split.search_b.trim().to_ascii_lowercase()
    } else {
        String::new()
    };

    let rows: Vec<(&Track, bool)> = pl
        .tracks
        .iter()
        .filter_map(|t| {
            let in_other = other_keys.contains(&track_key(t, split.fuzzy));
            let keep = match split.filter {
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
        .column(Column::exact(24.0))
        .column(Column::exact(48.0))
        .column(Column::initial(150.0).clip(true))
        .column(Column::remainder().clip(true))
        .header(20.0, |mut h| {
            for col in ["", "Diff", "Title", "Artist"] {
                h.col(|ui| {
                    ui.strong(col);
                });
            }
        })
        .body(|body| {
            body.rows(20.0, rows.len(), |mut row| {
                let idx = row.index();
                let (t, in_other) = rows[idx];
                row.col(|ui| {
                    let payload = DragTrackPayload {
                        source_playlist_id: pl.id.clone(),
                        tracks: vec![t.clone()],
                    };
                    ui.dnd_drag_source(
                        egui::Id::new((salt, &pl.id, idx, &t.video_id)),
                        payload,
                        |ui| ui.small("⋮⋮"),
                    );
                });
                row.col(|ui| {
                    let (tag, color) = if in_other {
                        ("BOTH", egui::Color32::GRAY)
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

fn track_key(track: &Track, fuzzy: bool) -> String {
    if !fuzzy && !track.video_id.is_empty() {
        return track.video_id.clone();
    }
    let norm = normalize_track(track);
    format!("{}::{}", norm.artist_norm, norm.title_norm)
}
