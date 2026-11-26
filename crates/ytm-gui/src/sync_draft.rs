use std::collections::{HashMap, HashSet};

use ytm_core::{MusicVideoType, Playlist, Track, TrackAvailability};

use crate::sync_queue::{StagedEntry, StagedOp};

/// Projects a playlist's local working draft by replaying staged operations over the cached base.
pub(super) fn project_working_playlist(
    entries: &[StagedEntry],
    playlist_id: &str,
    base_cache: &HashMap<String, Playlist>,
) -> Option<Playlist> {
    let mut pl = base_cache.get(playlist_id)?.clone();
    for entry in entries {
        if matches!(
            &entry.op,
            StagedOp::DeletePlaylist { playlist_id: pid, .. } if pid == playlist_id
        ) {
            return None;
        }
        apply_op_to_draft(&mut pl, &entry.op);
    }
    Some(pl)
}

fn apply_op_to_draft(pl: &mut Playlist, op: &StagedOp) {
    match op {
        StagedOp::AddTracks {
            playlist_id,
            tracks,
            allow_duplicates,
            ..
        } if playlist_id == &pl.id => add_tracks_to_playlist(pl, tracks, *allow_duplicates),
        StagedOp::RemoveTracks {
            playlist_id,
            tracks,
            ..
        } if playlist_id == &pl.id => remove_tracks_from_playlist(pl, tracks),
        StagedOp::MoveTracks {
            source_playlist_id,
            target_playlist_id,
            tracks,
            ..
        } => {
            if source_playlist_id == &pl.id {
                remove_tracks_from_playlist(pl, tracks);
            }
            if target_playlist_id == &pl.id {
                add_tracks_to_playlist(pl, tracks, false);
            }
        }
        StagedOp::UpdatePlaylistMeta {
            playlist_id,
            title,
            description,
            privacy,
            ..
        } if playlist_id == &pl.id => {
            if let Some(new_title) = title {
                pl.title = new_title.clone();
            }
            if let Some(new_desc) = description {
                pl.description = Some(new_desc.clone());
            }
            if let Some(new_privacy) = privacy {
                pl.privacy = *new_privacy;
            }
        }
        StagedOp::UpgradeTracks {
            playlist_id,
            upgrades,
            preserve_position,
            ..
        } if playlist_id == &pl.id => {
            for (old_t, atv) in upgrades {
                if let Some(pos) = find_track_pos(&pl.tracks, old_t) {
                    let mut replacement = atv.clone();
                    replacement.set_video_id = None;
                    if *preserve_position {
                        pl.tracks[pos] = replacement;
                    } else {
                        pl.tracks.remove(pos);
                        pl.tracks.push(replacement);
                    }
                }
            }
            reindex_playlist(pl);
        }
        StagedOp::Reorder {
            playlist_id,
            target_set_video_ids,
            ..
        } if playlist_id == &pl.id => {
            let mut by_sid: HashMap<String, Track> = HashMap::new();
            let mut trailing = Vec::new();
            for t in pl.tracks.drain(..) {
                if let Some(sid) = &t.set_video_id {
                    by_sid.insert(sid.clone(), t);
                } else {
                    trailing.push(t);
                }
            }
            for sid in target_set_video_ids {
                if let Some(t) = by_sid.remove(sid) {
                    pl.tracks.push(t);
                }
            }
            let mut leftover: Vec<Track> = by_sid.into_values().collect();
            leftover.sort_by_key(|t| t.position);
            pl.tracks.extend(leftover);
            pl.tracks.extend(trailing);
            reindex_playlist(pl);
        }
        StagedOp::SetLikeStatus {
            video_id,
            title,
            liked,
        } if pl.id.eq_ignore_ascii_case("LM") || pl.id.eq_ignore_ascii_case("VLLM") => {
            if !*liked {
                pl.tracks.retain(|t| t.video_id != *video_id);
            } else if !pl.tracks.iter().any(|t| t.video_id == *video_id) {
                pl.tracks.push(Track {
                    video_id: video_id.clone(),
                    set_video_id: None,
                    position: pl.tracks.len(),
                    title: title.clone(),
                    artists: Vec::new(),
                    album: None,
                    duration_sec: None,
                    release_year: None,
                    music_video_type: MusicVideoType::Unknown,
                    availability: TrackAvailability::Available,
                    thumbnail_url: None,
                });
            }
            reindex_playlist(pl);
        }
        // `CreatePlaylist` creates a new remote playlist not in `base_cache`; `RestoreSnapshot`
        // requires snapshot track loading from `ytm-store` and cannot be projected synchronously.
        _ => {}
    }
}

fn find_track_pos(haystack: &[Track], target: &Track) -> Option<usize> {
    haystack.iter().position(|cur| {
        if let (Some(a), Some(b)) = (&target.set_video_id, &cur.set_video_id) {
            a == b
        } else {
            cur.video_id == target.video_id
        }
    })
}

fn add_tracks_to_playlist(pl: &mut Playlist, tracks: &[Track], allow_duplicates: bool) {
    let mut existing: HashSet<String> = if allow_duplicates {
        HashSet::new()
    } else {
        pl.tracks.iter().map(|t| t.video_id.clone()).collect()
    };
    for t in tracks {
        if allow_duplicates || existing.insert(t.video_id.clone()) {
            let mut cloned = t.clone();
            cloned.set_video_id = None;
            pl.tracks.push(cloned);
        }
    }
    reindex_playlist(pl);
}

fn remove_tracks_from_playlist(pl: &mut Playlist, tracks: &[Track]) {
    for target in tracks {
        if let Some(pos) = find_track_pos(&pl.tracks, target) {
            pl.tracks.remove(pos);
        }
    }
    reindex_playlist(pl);
}

fn reindex_playlist(pl: &mut Playlist) {
    for (idx, track) in pl.tracks.iter_mut().enumerate() {
        track.position = idx;
    }
    pl.track_count = pl.tracks.len();
}
