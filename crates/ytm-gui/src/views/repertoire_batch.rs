use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, Track};

use crate::navigation::ToastState;
use crate::state::RepertoireTreeState;
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::repertoire_target::PlaylistMatches;
use crate::views::repertoire_tree::selection_key;

/// Action bar for bulk Copy / Move of multi-selected tracks in the Repertoire File-Tree.
pub(crate) fn show_tree_selection_bar(
    ui: &mut egui::Ui,
    matches: &[PlaylistMatches],
    playlists: &[PlaylistSummary],
    tree_state: &mut RepertoireTreeState,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
) {
    let mut picked: Vec<(&Playlist, Vec<Track>)> = Vec::new();
    let mut total = 0usize;
    for g in matches {
        let trs: Vec<Track> = g
            .tracks
            .iter()
            .filter(|t| {
                tree_state
                    .selected_keys
                    .contains(&selection_key(&g.playlist.id, t))
            })
            .cloned()
            .collect();
        if !trs.is_empty() {
            total += trs.len();
            picked.push((&g.playlist, trs));
        }
    }
    if total == 0 {
        return;
    }
    let target_info = tree_state.target_playlist_id.as_deref().and_then(|tid| {
        playlists
            .iter()
            .find(|p| p.id == tid && p.owned && !p.id.eq_ignore_ascii_case("LM"))
            .map(|p| (p.id.clone(), p.title.clone()))
    });

    ui.horizontal_wrapped(|ui| {
        ui.strong(format!("Selected: {total}"));
        if let Some((tid, ttitle)) = target_info {
            let to_add: Vec<Track> = picked
                .iter()
                .filter(|(pl, _)| pl.id != tid)
                .flat_map(|(_, trs)| trs.clone())
                .collect();
            if ui
                .add_enabled(
                    !to_add.is_empty(),
                    egui::Button::new(format!("Copy -> {ttitle}")),
                )
                .clicked()
            {
                let cnt = to_add.len();
                let eid = sync_queue.push(StagedOp::AddTracks {
                    playlist_id: tid.clone(),
                    playlist_title: ttitle.clone(),
                    tracks: to_add,
                    allow_duplicates: false,
                });
                tree_state.selected_keys.clear();
                *toast = Some(ToastState::staged(
                    format!("Staged Copy (+{cnt}) into '{ttitle}'."),
                    Some(eid),
                ));
            }
            let can_move = picked
                .iter()
                .any(|(pl, _)| pl.id != tid && pl.owned && !pl.id.eq_ignore_ascii_case("LM"));
            if ui
                .add_enabled(can_move, egui::Button::new(format!("Move -> {ttitle}")))
                .clicked()
            {
                let mut cnt = 0usize;
                let mut eid = None;
                for (pl, trs) in &picked {
                    if pl.id != tid && pl.owned && !pl.id.eq_ignore_ascii_case("LM") {
                        cnt += trs.len();
                        eid = Some(sync_queue.push(StagedOp::MoveTracks {
                            source_playlist_id: pl.id.clone(),
                            source_playlist_title: pl.title.clone(),
                            target_playlist_id: tid.clone(),
                            target_playlist_title: ttitle.clone(),
                            tracks: trs.clone(),
                        }));
                    }
                }
                tree_state.selected_keys.clear();
                *toast = Some(ToastState::staged(
                    format!("Staged Move ({cnt} track(s)) -> '{ttitle}'."),
                    eid,
                ));
            }
        }
        if ui.button("Clear Selection").clicked() {
            tree_state.selected_keys.clear();
        }
    });
}
