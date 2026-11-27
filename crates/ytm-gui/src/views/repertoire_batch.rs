use std::collections::HashSet;

use eframe::egui;
use ytm_core::{Playlist, PlaylistPrivacy, PlaylistSummary, Track};

use crate::navigation::ToastState;
use crate::state::{ConfirmedMutation, PendingMutation, RepertoireTreeState};
use crate::sync_queue::{StagedEntry, StagedOp, SyncQueue};
use crate::views::repertoire_target::PlaylistMatches;
use crate::views::repertoire_tree::selection_key;

/// Collect unique tracks across selected rows, preserving first-seen order.
#[must_use]
pub(crate) fn unique_selected_tracks(picked: &[(&Playlist, Vec<Track>)]) -> Vec<Track> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for (_, trs) in picked {
        for tr in trs {
            let key = if tr.video_id.is_empty() {
                format!("title::{}", tr.title)
            } else {
                tr.video_id.clone()
            };
            if seen.insert(key) {
                unique.push(tr.clone());
            }
        }
    }
    unique
}

/// Parameters bundled for `show_tree_selection_bar`.
pub(crate) struct TreeSelectionCtx<'a> {
    pub query: &'a str,
    pub matches: &'a [PlaylistMatches],
    pub playlists: &'a [PlaylistSummary],
    pub tree_state: &'a mut RepertoireTreeState,
    pub sync_queue: &'a mut SyncQueue,
    pub pending_modal: &'a mut Option<PendingMutation>,
    pub toast: &'a mut Option<ToastState>,
}

/// Action bar for bulk Select All, Create New Playlist, Copy, and Move in Repertoire Search.
pub(crate) fn show_tree_selection_bar(ui: &mut egui::Ui, ctx: TreeSelectionCtx<'_>) {
    let TreeSelectionCtx {
        query,
        matches,
        playlists,
        tree_state,
        sync_queue,
        pending_modal,
        toast,
    } = ctx;
    let mut picked: Vec<(&Playlist, Vec<Track>)> = Vec::new();
    let mut total_selected = 0usize;
    let mut total_hits = 0usize;
    for g in matches {
        total_hits += g.tracks.len();
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
            total_selected += trs.len();
            picked.push((&g.playlist, trs));
        }
    }
    if total_hits == 0 {
        return;
    }

    let unique_selected = unique_selected_tracks(&picked);
    ui.horizontal_wrapped(|ui| {
        if ui.button(format!("Select All ({total_hits})")).clicked() {
            for g in matches {
                for t in &g.tracks {
                    tree_state
                        .selected_keys
                        .insert(selection_key(&g.playlist.id, t));
                }
            }
        }
        if total_selected > 0 && ui.button("Clear Selection").clicked() {
            tree_state.selected_keys.clear();
        }
        if total_selected > 0 {
            ui.strong(format!(
                "Selected: {} ({} unique)",
                total_selected,
                unique_selected.len()
            ));
        }
    });

    if total_selected == 0 {
        return;
    }

    ui.horizontal_wrapped(|ui| {
        ui.strong("New Playlist:");
        ui.add(
            egui::TextEdit::singleline(&mut tree_state.new_playlist_title)
                .desired_width(190.0)
                .hint_text("Playlist name (e.g. Cyberpunk)..."),
        );
        let q_trim = query.trim();
        if !q_trim.is_empty()
            && tree_state.new_playlist_title.trim().is_empty()
            && ui.small_button(format!("Use \"{q_trim}\"")).clicked()
        {
            tree_state.new_playlist_title = q_trim.to_string();
        }
        egui::ComboBox::from_id_salt("rep_new_pl_privacy")
            .selected_text(format!("{:?}", tree_state.new_playlist_privacy))
            .width(82.0)
            .show_ui(ui, |ui| {
                for p in [
                    PlaylistPrivacy::Private,
                    PlaylistPrivacy::Unlisted,
                    PlaylistPrivacy::Public,
                ] {
                    ui.selectable_value(&mut tree_state.new_playlist_privacy, p, format!("{p:?}"));
                }
            });

        let clean_title = tree_state.new_playlist_title.trim().to_string();
        let can_create = !clean_title.is_empty() && !unique_selected.is_empty();
        if ui
            .add_enabled(
                can_create,
                egui::Button::new(format!(
                    "Create & Add {} Track(s) Now...",
                    unique_selected.len()
                )),
            )
            .clicked()
        {
            let cnt = unique_selected.len();
            let op = StagedOp::CreatePlaylist {
                title: clean_title.clone(),
                description: format!("Created from Dalga library search ({q_trim})"),
                privacy: tree_state.new_playlist_privacy,
                tracks: unique_selected.clone(),
            };
            *pending_modal = Some(PendingMutation {
                title: format!("Create Playlist '{clean_title}'"),
                summary: format!(
                    "Create new {:?} playlist '{clean_title}' on YouTube Music and add {cnt} unique track(s) directly?",
                    tree_state.new_playlist_privacy
                ),
                action: ConfirmedMutation::ExecuteSync {
                    entries: vec![StagedEntry {
                        id: 9_100_000_000,
                        op,
                        selected: true,
                    }],
                },
            });
            tree_state.selected_keys.clear();
            tree_state.new_playlist_title.clear();
        }
        if ui
            .add_enabled(can_create, egui::Button::new("+ Stage New"))
            .clicked()
        {
            let cnt = unique_selected.len();
            let eid = sync_queue.push(StagedOp::CreatePlaylist {
                title: clean_title.clone(),
                description: format!("Created from Dalga library search ({q_trim})"),
                privacy: tree_state.new_playlist_privacy,
                tracks: unique_selected.clone(),
            });
            tree_state.selected_keys.clear();
            tree_state.new_playlist_title.clear();
            *toast = Some(ToastState::staged(
                format!("Staged new playlist '{clean_title}' (+{cnt}) in Sync."),
                Some(eid),
            ));
        }
    });

    let target_info = tree_state.target_playlist_id.as_deref().and_then(|tid| {
        playlists
            .iter()
            .find(|p| p.id == tid && p.owned && !p.id.eq_ignore_ascii_case("LM"))
            .map(|p| (p.id.clone(), p.title.clone()))
    });

    if let Some((tid, ttitle)) = target_info {
        ui.horizontal_wrapped(|ui| {
            let to_add: Vec<Track> = picked
                .iter()
                .filter(|(pl, _)| pl.id != tid)
                .flat_map(|(_, trs)| trs.clone())
                .collect();
            let unique_add = unique_selected_tracks(
                &picked
                    .iter()
                    .filter(|(pl, _)| pl.id != tid)
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            if ui
                .add_enabled(
                    !to_add.is_empty(),
                    egui::Button::new(format!("Stage Copy -> {ttitle}")),
                )
                .clicked()
            {
                let cnt = unique_add.len();
                let eid = sync_queue.push(StagedOp::AddTracks {
                    playlist_id: tid.clone(),
                    playlist_title: ttitle.clone(),
                    tracks: unique_add.clone(),
                    allow_duplicates: false,
                });
                tree_state.selected_keys.clear();
                *toast = Some(ToastState::staged(
                    format!("Staged Copy (+{cnt}) into '{ttitle}'."),
                    Some(eid),
                ));
            }
            if ui
                .add_enabled(
                    !unique_add.is_empty(),
                    egui::Button::new(format!("Copy -> {ttitle} Now...")),
                )
                .clicked()
            {
                let cnt = unique_add.len();
                let op = StagedOp::AddTracks {
                    playlist_id: tid.clone(),
                    playlist_title: ttitle.clone(),
                    tracks: unique_add,
                    allow_duplicates: false,
                };
                *pending_modal = Some(PendingMutation {
                    title: format!("Add Tracks to '{ttitle}'"),
                    summary: format!(
                        "Save pre-sync snapshot and add {cnt} track(s) directly to '{ttitle}'?"
                    ),
                    action: ConfirmedMutation::ExecuteSync {
                        entries: vec![StagedEntry {
                            id: 9_100_000_001,
                            op,
                            selected: true,
                        }],
                    },
                });
                tree_state.selected_keys.clear();
            }
            let can_move = picked
                .iter()
                .any(|(pl, _)| pl.id != tid && pl.owned && !pl.id.eq_ignore_ascii_case("LM"));
            if ui
                .add_enabled(
                    can_move,
                    egui::Button::new(format!("Stage Move -> {ttitle}")),
                )
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
        });
    }
}

#[cfg(test)]
mod tests {
    use ytm_core::{MusicVideoType, TrackAvailability};

    use super::*;

    fn mk_track(id: &str, title: &str) -> Track {
        Track {
            video_id: id.into(),
            set_video_id: None,
            position: 0,
            title: title.into(),
            artists: Vec::new(),
            album: None,
            duration_sec: Some(180),
            release_year: None,
            music_video_type: MusicVideoType::AudioTrack,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        }
    }

    fn mk_pl(id: &str) -> Playlist {
        Playlist {
            id: id.into(),
            title: id.into(),
            description: None,
            privacy: PlaylistPrivacy::Private,
            owned: true,
            track_count: 1,
            tracks: Vec::new(),
        }
    }

    #[test]
    fn unique_selected_tracks_deduplicates_across_playlists_preserving_order() {
        let p1 = mk_pl("PL1");
        let p2 = mk_pl("PL2");
        let t1 = mk_track("V1", "First");
        let t2 = mk_track("V2", "Second");
        let t1_dup = mk_track("V1", "First");
        let picked = vec![(&p1, vec![t1, t2]), (&p2, vec![t1_dup])];

        let out = unique_selected_tracks(&picked);
        let ids: Vec<&str> = out.iter().map(|t| t.video_id.as_str()).collect();
        assert_eq!(ids, vec!["V1", "V2"]);
    }
}
