use std::collections::{HashMap, HashSet};

use eframe::egui;
use ytm_core::{Playlist, Track};

use crate::navigation::ToastState;
use crate::playback_ops::PlayAction;
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::inspect_helpers::compute_drop_reorder;
use crate::views::inspect_rows::RowRenderOutcome;

#[allow(clippy::too_many_arguments)]
pub fn handle_table_keyboard(
    ctx: &egui::Context,
    pl: &Playlist,
    visible: &[&Track],
    editable: bool,
    selected_positions: &mut HashSet<usize>,
    last_clicked: &mut Option<usize>,
    focused_row: &mut Option<usize>,
    scroll_target_row: &mut Option<usize>,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
) -> Option<PlayAction> {
    if ctx.wants_keyboard_input() || visible.is_empty() {
        return None;
    }
    let (ctrl_a, esc, del, up, down, home, end, space, enter) = ctx.input(|i| {
        (
            (i.modifiers.ctrl || i.modifiers.mac_cmd) && i.key_pressed(egui::Key::A),
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Home),
            i.key_pressed(egui::Key::End),
            i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::Enter),
        )
    });
    if ctrl_a {
        for t in visible {
            selected_positions.insert(t.position);
        }
    } else if esc {
        selected_positions.clear();
        *focused_row = None;
    } else if del && editable && !selected_positions.is_empty() {
        let picked: Vec<Track> = pl
            .tracks
            .iter()
            .filter(|t| selected_positions.contains(&t.position))
            .cloned()
            .collect();
        let cnt = picked.len();
        let eid = sync_queue.push(StagedOp::RemoveTracks {
            playlist_id: pl.id.clone(),
            playlist_title: pl.title.clone(),
            tracks: picked,
        });
        selected_positions.clear();
        *toast = Some(ToastState::staged(
            format!("Staged -{cnt} track removal(s) via Delete key."),
            Some(eid),
        ));
    } else if down {
        let next = focused_row.map_or(0, |r| (r + 1).min(visible.len() - 1));
        *focused_row = Some(next);
        *scroll_target_row = Some(next);
    } else if up {
        let prev = focused_row.map_or(0, |r| r.saturating_sub(1));
        *focused_row = Some(prev);
        *scroll_target_row = Some(prev);
    } else if home {
        *focused_row = Some(0);
        *scroll_target_row = Some(0);
    } else if end {
        let last = visible.len() - 1;
        *focused_row = Some(last);
        *scroll_target_row = Some(last);
    } else if space
        && let Some(idx) = *focused_row
        && let Some(t) = visible.get(idx)
    {
        if !selected_positions.remove(&t.position) {
            selected_positions.insert(t.position);
        }
        *last_clicked = Some(t.position);
    } else if enter && let Some(idx) = *focused_row {
        return Some(PlayAction::PlayFromList {
            tracks: visible.iter().copied().cloned().collect(),
            start_idx: idx,
        });
    }
    None
}

pub fn apply_row_outcome(
    pl: &Playlist,
    outcome: RowRenderOutcome,
    playlist_cache: &HashMap<String, Playlist>,
    sync_queue: &mut SyncQueue,
    selected_positions: &mut HashSet<usize>,
    search: &mut String,
    toast: &mut Option<ToastState>,
) -> Vec<PlayAction> {
    if let Some((dragged_sids, target_pos)) = outcome.intra_reorder
        && let Some(target_order) = compute_drop_reorder(pl, &dragged_sids, target_pos)
    {
        match sync_queue.stage_reorder(&pl.id, &pl.title, target_order, playlist_cache) {
            Ok(n) => {
                selected_positions.clear();
                *toast = Some(if n == 0 {
                    ToastState::info(true, "Order returned to baseline; no Sync moves needed.")
                } else {
                    ToastState::staged(
                        format!("Staged row drag ({n} LIS moves) in Sync."),
                        sync_queue.last_entry_id(),
                    )
                });
            }
            Err(e) => *toast = Some(ToastState::info(false, e)),
        }
    }
    if let Some((dest_id, dest_title, tracks, remove_source)) = outcome.batch_copy_move {
        let cnt = tracks.len();
        let eid = sync_queue.push(StagedOp::AddTracks {
            playlist_id: dest_id,
            playlist_title: dest_title.clone(),
            tracks: tracks.clone(),
            allow_duplicates: false,
        });
        let last_eid = if remove_source {
            sync_queue.push(StagedOp::RemoveTracks {
                playlist_id: pl.id.clone(),
                playlist_title: pl.title.clone(),
                tracks,
            })
        } else {
            eid
        };
        selected_positions.clear();
        *toast = Some(ToastState::staged(
            format!(
                "Staged {} ({cnt} track(s) -> '{dest_title}') in Sync.",
                if remove_source { "Move" } else { "Copy" }
            ),
            Some(last_eid),
        ));
    }
    if let Some(track) = outcome.remove_single {
        let eid = sync_queue.push(StagedOp::RemoveTracks {
            playlist_id: pl.id.clone(),
            playlist_title: pl.title.clone(),
            tracks: vec![track.clone()],
        });
        *toast = Some(ToastState::staged(
            format!("Staged '-' '{}' in Sync.", track.title),
            Some(eid),
        ));
    }
    if let Some(artist) = outcome.filter_artist {
        *search = artist;
    }
    outcome.play_actions
}
