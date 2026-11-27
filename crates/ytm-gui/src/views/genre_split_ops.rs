use std::collections::HashSet;

use ytm_core::{Playlist, PlaylistPrivacy, Track};
use ytm_enrich::{GenreBucket, GenreSplitPlan};

use crate::state::GenreSplitForm;
use crate::sync_queue::StagedOp;

/// Pure planner converting inspected/edited `GenreSplitPlan` into ordered `StagedOp` actions.
#[must_use]
pub fn build_genre_split_ops(
    source_pl: &Playlist,
    plan: &GenreSplitPlan,
    form: &GenreSplitForm,
) -> (Vec<StagedOp>, usize, usize) {
    let mut ops = Vec::new();
    let mut moved_tracks: Vec<Track> = Vec::new();
    let mut seen_moved: HashSet<String> = HashSet::new();
    let mut active_buckets = 0usize;

    let mut candidate_buckets: Vec<&GenreBucket> = plan.buckets.iter().collect();
    if form.include_skipped {
        candidate_buckets.extend(plan.skipped_buckets.iter());
    }

    for bucket in candidate_buckets {
        if form.excluded_buckets.contains(&bucket.genre) {
            continue;
        }
        let filtered_tracks: Vec<Track> = bucket
            .tracks_to_add
            .iter()
            .filter(|t| !form.excluded_video_ids.contains(&t.video_id))
            .cloned()
            .collect();
        if filtered_tracks.is_empty() {
            continue;
        }
        active_buckets += 1;
        for tr in &filtered_tracks {
            if seen_moved.insert(tr.video_id.clone()) {
                moved_tracks.push(tr.clone());
            }
        }
        if let Some(existing_id) = &bucket.existing_playlist_id {
            ops.push(StagedOp::AddTracks {
                playlist_id: existing_id.clone(),
                playlist_title: bucket.target_playlist_title.clone(),
                tracks: filtered_tracks,
                allow_duplicates: false,
            });
        } else {
            ops.push(StagedOp::CreatePlaylist {
                title: bucket.target_playlist_title.clone(),
                description: format!("Split by genre from '{}'", source_pl.title),
                privacy: PlaylistPrivacy::Private,
                tracks: filtered_tracks,
            });
        }
    }

    let can_remove_source =
        form.move_mode && source_pl.owned && !source_pl.id.eq_ignore_ascii_case("LM");
    if can_remove_source && !moved_tracks.is_empty() {
        ops.push(StagedOp::RemoveTracks {
            playlist_id: source_pl.id.clone(),
            playlist_title: source_pl.title.clone(),
            tracks: moved_tracks.clone(),
        });
    }

    (ops, active_buckets, moved_tracks.len())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use ytm_core::{MusicVideoType, TrackAvailability};

    use super::*;

    fn mk_track(id: &str, title: &str) -> Track {
        Track {
            video_id: id.into(),
            set_video_id: Some(format!("S_{id}")),
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

    #[test]
    fn build_genre_split_ops_supports_copy_and_move_and_exclusions() {
        let src = Playlist {
            id: "PL_SRC".into(),
            title: "Mix".into(),
            description: None,
            privacy: PlaylistPrivacy::Private,
            owned: true,
            track_count: 3,
            tracks: vec![
                mk_track("V1", "A"),
                mk_track("V2", "B"),
                mk_track("V3", "C"),
            ],
        };
        let plan = GenreSplitPlan {
            source_playlist_id: "PL_SRC".into(),
            source_playlist_title: "Mix".into(),
            buckets: vec![
                GenreBucket {
                    genre: "Electronic".into(),
                    target_playlist_title: "Mix - Electronic".into(),
                    existing_playlist_id: None,
                    tracks_to_add: vec![mk_track("V1", "A"), mk_track("V2", "B")],
                    already_present_count: 0,
                },
                GenreBucket {
                    genre: "Rock".into(),
                    target_playlist_title: "Mix - Rock".into(),
                    existing_playlist_id: Some("PL_R".into()),
                    tracks_to_add: vec![mk_track("V3", "C")],
                    already_present_count: 0,
                },
            ],
            skipped_small_buckets: 0,
            skipped_buckets: Vec::new(),
            track_details: HashMap::new(),
        };

        let mut form = GenreSplitForm {
            move_mode: true,
            ..Default::default()
        };
        form.excluded_buckets.insert("Rock".into());
        form.excluded_video_ids.insert("V2".into());

        let (ops, buckets, tracks) = build_genre_split_ops(&src, &plan, &form);
        assert_eq!(buckets, 1);
        assert_eq!(tracks, 1);
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], StagedOp::CreatePlaylist { .. }));
        assert!(matches!(ops[1], StagedOp::RemoveTracks { .. }));
    }
}
