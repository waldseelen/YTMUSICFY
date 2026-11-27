use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, Track};

use crate::icons::{Icon, icon_button};
use crate::navigation::ToastState;
use crate::overlays::handle_cross_playlist_drop;
use crate::playback_ops::PlayAction;
use crate::state::{
    AsyncState, DragDropMode, DragTrackPayload, DropPrompt, RepertoireGroupMode,
    RepertoireMatchScope, RepertoireTreeState, WorkerCmd,
};
use crate::sync_queue::SyncQueue;

#[derive(Debug, Clone)]
pub struct PlaylistMatches {
    pub playlist: Playlist,
    pub tracks: Vec<Track>,
}

/// Returns `true` when `id` is an indexed saved album browse ID (`MPRE...`).
#[must_use]
pub fn is_indexed_album_id(id: &str) -> bool {
    id.starts_with("MPRE")
}

/// Collects tracks matching `query` across all locally cached/indexed playlists and albums.
#[must_use]
pub fn collect_repertoire_matches(
    query: &str,
    playlists: &[PlaylistSummary],
    playlist_cache: &HashMap<String, Playlist>,
    sync_queue: &SyncQueue,
) -> Vec<PlaylistMatches> {
    collect_repertoire_matches_scoped(
        query,
        RepertoireMatchScope::All,
        RepertoireGroupMode::BySource,
        playlists,
        playlist_cache,
        sync_queue,
    )
}

/// Collects tracks matching `query` using the chosen field scope and grouping mode.
#[must_use]
pub fn collect_repertoire_matches_scoped(
    query: &str,
    scope: RepertoireMatchScope,
    group_mode: RepertoireGroupMode,
    playlists: &[PlaylistSummary],
    playlist_cache: &HashMap<String, Playlist>,
    sync_queue: &SyncQueue,
) -> Vec<PlaylistMatches> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut seen_ids = HashSet::new();
    let mut out = Vec::new();

    for sum in playlists {
        if !seen_ids.insert(sum.id.clone()) {
            continue;
        }
        if let Some(pl) = sync_queue.working_playlist(&sum.id, playlist_cache) {
            let tracks = filter_matching_tracks(&pl, &needle, scope);
            if !tracks.is_empty() {
                out.push(PlaylistMatches {
                    playlist: pl,
                    tracks,
                });
            }
        }
    }
    for id in playlist_cache.keys() {
        if seen_ids.insert(id.clone())
            && let Some(pl) = sync_queue.working_playlist(id, playlist_cache)
        {
            let tracks = filter_matching_tracks(&pl, &needle, scope);
            if !tracks.is_empty() {
                out.push(PlaylistMatches {
                    playlist: pl,
                    tracks,
                });
            }
        }
    }
    if group_mode == RepertoireGroupMode::ByAlbum {
        return group_matches_by_album(out);
    }
    out
}

fn group_matches_by_album(groups: Vec<PlaylistMatches>) -> Vec<PlaylistMatches> {
    let mut by_album: HashMap<String, (String, Vec<Track>, HashSet<String>)> = HashMap::new();
    for g in groups {
        for tr in g.tracks {
            let album_name = tr
                .album
                .as_ref()
                .map(|a| a.name.trim())
                .filter(|n| !n.is_empty())
                .unwrap_or("Single / Unknown Album")
                .to_string();
            let key = album_name.to_ascii_lowercase();
            let entry = by_album
                .entry(key)
                .or_insert_with(|| (album_name, Vec::new(), HashSet::new()));
            let vid = if tr.video_id.is_empty() {
                tr.title.clone()
            } else {
                tr.video_id.clone()
            };
            if entry.2.insert(vid) {
                entry.1.push(tr);
            }
        }
    }
    let mut out: Vec<PlaylistMatches> = by_album
        .into_iter()
        .map(|(key, (album_title, tracks, _))| PlaylistMatches {
            playlist: Playlist {
                id: format!("MPRE_GROUP::{key}"),
                title: album_title,
                description: None,
                privacy: ytm_core::PlaylistPrivacy::Private,
                owned: false,
                track_count: tracks.len(),
                tracks: tracks.clone(),
            },
            tracks,
        })
        .collect();
    out.sort_by(|a, b| {
        b.tracks
            .len()
            .cmp(&a.tracks.len())
            .then_with(|| a.playlist.title.cmp(&b.playlist.title))
    });
    out
}

fn filter_matching_tracks(pl: &Playlist, needle: &str, scope: RepertoireMatchScope) -> Vec<Track> {
    let is_album_source = is_indexed_album_id(&pl.id);
    let source_title_matches = is_album_source && pl.title.to_ascii_lowercase().contains(needle);
    pl.tracks
        .iter()
        .filter(|t| {
            let match_title = t.title.to_ascii_lowercase().contains(needle);
            let match_artist = t
                .primary_artist_name()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .contains(needle);
            let match_album = t
                .album
                .as_ref()
                .is_some_and(|a| a.name.to_ascii_lowercase().contains(needle))
                || source_title_matches;
            match scope {
                RepertoireMatchScope::All => {
                    match_title
                        || match_artist
                        || match_album
                        || t.video_id.to_ascii_lowercase().contains(needle)
                }
                RepertoireMatchScope::Title => match_title,
                RepertoireMatchScope::Artist => match_artist,
                RepertoireMatchScope::Album => match_album,
            }
        })
        .cloned()
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_target_pane(
    ui: &mut egui::Ui,
    playlists: &[PlaylistSummary],
    library: &AsyncState<Vec<PlaylistSummary>>,
    loading_playlists: &mut HashSet<String>,
    playlist_cache: &HashMap<String, Playlist>,
    sync_queue: &mut SyncQueue,
    tree_state: &mut RepertoireTreeState,
    drag_mode: DragDropMode,
    drop_prompt: &mut Option<DropPrompt>,
    toast: &mut Option<ToastState>,
    pointer_pos: egui::Pos2,
    cmd_tx: &Sender<WorkerCmd>,
) -> Vec<PlayAction> {
    let mut actions = Vec::new();
    let target_label = tree_state
        .target_playlist_id
        .as_deref()
        .and_then(|id| playlists.iter().find(|p| p.id == id))
        .map_or_else(
            || "Select Target Playlist...".to_string(),
            |p| p.title.clone(),
        );

    let pane_resp = ui
        .group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Target Workspace:");
                egui::ComboBox::from_id_salt("rep_tree_target_combo")
                    .selected_text(target_label)
                    .show_ui(ui, |ui| {
                        for p in playlists
                            .iter()
                            .filter(|p| p.owned && !p.id.eq_ignore_ascii_case("LM"))
                        {
                            if ui
                                .selectable_value(
                                    &mut tree_state.target_playlist_id,
                                    Some(p.id.clone()),
                                    &p.title,
                                )
                                .clicked()
                                && !playlist_cache.contains_key(&p.id)
                                && loading_playlists.insert(p.id.clone())
                            {
                                let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(p.id.clone()));
                            }
                        }
                    });
            });
            ui.separator();

            let Some(tid) = tree_state.target_playlist_id.clone() else {
                ui.weak("Pick a target playlist above to drop tracks here.");
                return;
            };
            let Some(target_pl) = sync_queue.working_playlist(&tid, playlist_cache) else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Loading target playlist...");
                });
                return;
            };

            ui.small(format!(
                "Drop tracks from left File-Tree into '{}' ({} tracks) — or drag tracks onto a left branch:",
                target_pl.title,
                target_pl.tracks.len()
            ));
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("rep_tree_right_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (idx, t) in target_pl.tracks.iter().enumerate() {
                        let payload = DragTrackPayload {
                            source_playlist_id: target_pl.id.clone(),
                            tracks: vec![t.clone()],
                        };
                        ui.horizontal(|ui| {
                            let drag_resp = ui
                                .dnd_drag_source(
                                    egui::Id::new(("tree_target_drag", &target_pl.id, idx)),
                                    payload,
                                    |ui| ui.small("::"),
                                )
                                .response;
                            if drag_resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            if icon_button(ui, Icon::Play, 12.0, true, "Play now").clicked() {
                                actions.push(PlayAction::PlayFromList {
                                    tracks: target_pl.tracks.clone(),
                                    start_idx: idx,
                                });
                            }
                            ui.add(
                                egui::Label::new(format!(
                                    "{}. {} — {}",
                                    idx + 1,
                                    t.primary_artist_name().unwrap_or("-"),
                                    t.title
                                ))
                                .truncate(),
                            );
                        });
                    }
                });
        })
        .response;

    if let Some(tid) = tree_state.target_playlist_id.as_ref()
        && playlists
            .iter()
            .any(|p| &p.id == tid && p.owned && !p.id.eq_ignore_ascii_case("LM"))
    {
        let hovered = pane_resp
            .dnd_hover_payload::<DragTrackPayload>()
            .is_some_and(|p| p.source_playlist_id != *tid);
        if hovered {
            ui.painter().rect_stroke(
                pane_resp.rect,
                4.0,
                egui::Stroke::new(2.5_f32, egui::Color32::from_rgb(70, 195, 130)),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(payload) = pane_resp.dnd_release_payload::<DragTrackPayload>()
            && payload.source_playlist_id != *tid
        {
            let prompt = build_drop_prompt(pointer_pos, &payload, tid, playlists);
            handle_cross_playlist_drop(
                ui.ctx(),
                prompt,
                drag_mode,
                drop_prompt,
                playlist_cache,
                library,
                sync_queue,
                toast,
            );
        }
    }

    actions
}

pub(crate) fn build_drop_prompt(
    pos: egui::Pos2,
    payload: &DragTrackPayload,
    target_id: &str,
    playlists: &[PlaylistSummary],
) -> DropPrompt {
    let src_title = playlists
        .iter()
        .find(|p| p.id == payload.source_playlist_id)
        .map_or_else(|| payload.source_playlist_id.clone(), |p| p.title.clone());
    let tgt_title = playlists
        .iter()
        .find(|p| p.id == target_id)
        .map_or_else(|| target_id.to_string(), |p| p.title.clone());
    DropPrompt {
        pos,
        source_playlist_id: payload.source_playlist_id.clone(),
        source_playlist_title: src_title,
        target_playlist_id: target_id.to_string(),
        target_playlist_title: tgt_title,
        tracks: payload.tracks.clone(),
    }
}

#[cfg(test)]
#[path = "repertoire_target_tests.rs"]
mod tests;
