use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, Track};

use crate::navigation::ToastState;
use crate::state::{AsyncState, DiffFilter, DraftRevision, DropPrompt, SplitPaneState, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::split_pane::{SplitPaneParams, render_split_pane, track_key};

type SplitDiffKey = (String, String, DraftRevision, bool, usize, usize);

#[derive(Clone)]
struct SplitDiffComputed {
    keys_a: Arc<HashSet<String>>,
    keys_b: Arc<HashSet<String>>,
    only_a_tracks: Arc<Vec<Track>>,
    only_b_tracks: Arc<Vec<Track>>,
    both_count: usize,
}

fn compute_split_diff(pl_a: &Playlist, pl_b: &Playlist, fuzzy: bool) -> SplitDiffComputed {
    let a_track_keys: Vec<String> = pl_a.tracks.iter().map(|t| track_key(t, fuzzy)).collect();
    let b_track_keys: Vec<String> = pl_b.tracks.iter().map(|t| track_key(t, fuzzy)).collect();

    let keys_a: HashSet<String> = a_track_keys.iter().cloned().collect();
    let keys_b: HashSet<String> = b_track_keys.iter().cloned().collect();

    let mut only_a_tracks = Vec::new();
    let mut both_count = 0usize;
    for (t, key) in pl_a.tracks.iter().zip(&a_track_keys) {
        if keys_b.contains(key) {
            both_count += 1;
        } else {
            only_a_tracks.push(t.clone());
        }
    }

    let only_b_tracks: Vec<Track> = pl_b
        .tracks
        .iter()
        .zip(&b_track_keys)
        .filter(|(_, key)| !keys_a.contains(*key))
        .map(|(t, _)| t.clone())
        .collect();

    SplitDiffComputed {
        keys_a: Arc::new(keys_a),
        keys_b: Arc::new(keys_b),
        only_a_tracks: Arc::new(only_a_tracks),
        only_b_tracks: Arc::new(only_b_tracks),
        both_count,
    }
}

fn cached_split_diff(
    ctx: &egui::Context,
    pl_a: &Playlist,
    pl_b: &Playlist,
    revision: DraftRevision,
    fuzzy: bool,
) -> SplitDiffComputed {
    let key: SplitDiffKey = (
        pl_a.id.clone(),
        pl_b.id.clone(),
        revision,
        fuzzy,
        pl_a.tracks.len(),
        pl_b.tracks.len(),
    );
    let memo_id = egui::Id::new("split_diff_memo");
    ctx.data_mut(|data| {
        if let Some((cached_key, computed)) =
            data.get_temp::<(SplitDiffKey, SplitDiffComputed)>(memo_id)
            && cached_key == key
        {
            return computed;
        }
        let computed = compute_split_diff(pl_a, pl_b, fuzzy);
        data.insert_temp(memo_id, (key, computed.clone()));
        computed
    })
}

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

    let diff = cached_split_diff(ui.ctx(), pl_a, &pl_b, sync_queue.revision(), split.fuzzy);

    ui.horizontal_wrapped(|ui| {
        ui.colored_label(
            egui::Color32::from_rgb(90, 200, 120),
            format!("+ Only A: {}", diff.only_a_tracks.len()),
        );
        ui.colored_label(
            egui::Color32::from_rgb(235, 165, 70),
            format!("+ Only B: {}", diff.only_b_tracks.len()),
        );
        ui.label(format!("= Both: {}", diff.both_count));
        ui.separator();

        let can_add_b =
            pl_b.owned && !pl_b.id.eq_ignore_ascii_case("LM") && !diff.only_a_tracks.is_empty();
        if ui
            .add_enabled(
                can_add_b,
                egui::Button::new(format!(
                    "Stage Missing A -> B (+{})",
                    diff.only_a_tracks.len()
                )),
            )
            .clicked()
        {
            let cnt = diff.only_a_tracks.len();
            let eid = sync_queue.push(StagedOp::AddTracks {
                playlist_id: pl_b.id.clone(),
                playlist_title: pl_b.title.clone(),
                tracks: diff.only_a_tracks.as_ref().clone(),
                allow_duplicates: false,
            });
            *toast = Some(ToastState::staged(
                format!("Staged +{cnt} tracks into '{}'.", pl_b.title),
                Some(eid),
            ));
        }

        let can_add_a =
            pl_a.owned && !pl_a.id.eq_ignore_ascii_case("LM") && !diff.only_b_tracks.is_empty();
        if ui
            .add_enabled(
                can_add_a,
                egui::Button::new(format!(
                    "Stage Missing B -> A (+{})",
                    diff.only_b_tracks.len()
                )),
            )
            .clicked()
        {
            let cnt = diff.only_b_tracks.len();
            let eid = sync_queue.push(StagedOp::AddTracks {
                playlist_id: pl_a.id.clone(),
                playlist_title: pl_a.title.clone(),
                tracks: diff.only_b_tracks.as_ref().clone(),
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
                other_keys: &diff.keys_b,
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
                other_keys: &diff.keys_a,
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

#[cfg(test)]
mod tests {
    use ytm_core::{ArtistRef, MusicVideoType, PlaylistPrivacy, TrackAvailability};

    use super::*;

    fn mk_track(vid: &str, artist: &str, title: &str) -> Track {
        Track {
            video_id: vid.to_string(),
            set_video_id: Some(format!("S_{vid}")),
            position: 0,
            title: title.to_string(),
            artists: vec![ArtistRef {
                name: artist.to_string(),
                browse_id: None,
            }],
            album: None,
            duration_sec: Some(200),
            release_year: None,
            music_video_type: MusicVideoType::AudioTrack,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        }
    }

    #[test]
    fn split_diff_memo_reuses_arc_sets_and_invalidates_on_fuzzy_or_revision() {
        let ctx = egui::Context::default();
        let pl_a = Playlist {
            id: "PLA".to_string(),
            title: "A".to_string(),
            description: None,
            privacy: PlaylistPrivacy::Private,
            owned: true,
            track_count: 2,
            tracks: vec![
                mk_track("V1", "Daft Punk", "Around the World"),
                mk_track("V2", "Muse", "Hysteria"),
            ],
        };
        let pl_b = Playlist {
            id: "PLB".to_string(),
            title: "B".to_string(),
            description: None,
            privacy: PlaylistPrivacy::Private,
            owned: true,
            track_count: 1,
            tracks: vec![mk_track("V3", "Daft Punk", "Around the World (Live)")],
        };

        let first = cached_split_diff(&ctx, &pl_a, &pl_b, DraftRevision(1), false);
        let second = cached_split_diff(&ctx, &pl_a, &pl_b, DraftRevision(1), false);
        assert!(Arc::ptr_eq(&first.keys_a, &second.keys_a));
        assert!(Arc::ptr_eq(&first.only_a_tracks, &second.only_a_tracks));
        assert_eq!(first.both_count, 0);

        let fuzzy = cached_split_diff(&ctx, &pl_a, &pl_b, DraftRevision(1), true);
        assert!(!Arc::ptr_eq(&first.keys_a, &fuzzy.keys_a));
        assert_eq!(fuzzy.both_count, 1);
        assert_eq!(fuzzy.only_a_tracks.len(), 1);
        assert_eq!(fuzzy.only_b_tracks.len(), 0);
    }
}
