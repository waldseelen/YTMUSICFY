use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};
use ytm_core::{MoveAction, Playlist, Track};

use crate::sort::compute_minimal_moves;

/// Pure diff plan describing removals, additions, and target ordering to restore a playlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreDiffPlan {
    pub to_remove: Vec<(String, String)>,
    pub removed_tracks: Vec<Track>,
    pub to_add: Vec<String>,
    pub added_tracks: Vec<Track>,
    pub allow_duplicates: bool,
    pub target_video_order: Vec<String>,
}

/// Computes the pure membership diff needed to restore `live` toward `snapshot`.
#[must_use]
pub fn plan_restore_diff(live: &Playlist, snapshot: &Playlist) -> RestoreDiffPlan {
    let mut desired_counts: HashMap<&str, usize> = HashMap::new();
    let mut target_video_order = Vec::with_capacity(snapshot.tracks.len());
    for track in &snapshot.tracks {
        if !track.video_id.is_empty() {
            *desired_counts.entry(track.video_id.as_str()).or_insert(0) += 1;
            target_video_order.push(track.video_id.clone());
        }
    }

    let mut kept_counts: HashMap<String, usize> = HashMap::new();
    let mut to_remove = Vec::new();
    let mut removed_tracks = Vec::new();
    for track in &live.tracks {
        let want = desired_counts
            .get(track.video_id.as_str())
            .copied()
            .unwrap_or(0);
        let seen = kept_counts.entry(track.video_id.clone()).or_insert(0);
        if *seen < want {
            *seen += 1;
        } else if let Some(set_video_id) = &track.set_video_id {
            to_remove.push((track.video_id.clone(), set_video_id.clone()));
            removed_tracks.push(track.clone());
        }
    }

    let mut remaining_kept = kept_counts;
    let mut to_add = Vec::new();
    let mut added_tracks = Vec::new();
    for track in &snapshot.tracks {
        if track.video_id.is_empty() {
            continue;
        }
        let consume = remaining_kept.get_mut(&track.video_id).map(|count| {
            if *count > 0 {
                *count -= 1;
                true
            } else {
                false
            }
        });
        if consume != Some(true) {
            to_add.push(track.video_id.clone());
            added_tracks.push(track.clone());
        }
    }

    let allow_duplicates = desired_counts.values().any(|&cnt| cnt > 1);

    RestoreDiffPlan {
        to_remove,
        removed_tracks,
        to_add,
        added_tracks,
        allow_duplicates,
        target_video_order,
    }
}

/// Computes the minimal `MoveAction` edits to reorder `updated` to match `snapshot`.
#[must_use]
pub fn compute_restore_moves(updated: &Playlist, snapshot: &Playlist) -> Vec<MoveAction> {
    let mut set_queues: HashMap<&str, VecDeque<String>> = HashMap::new();
    let mut current_set_ids = Vec::new();
    for track in &updated.tracks {
        if let Some(sid) = &track.set_video_id {
            current_set_ids.push(sid.clone());
            set_queues
                .entry(track.video_id.as_str())
                .or_default()
                .push_back(sid.clone());
        }
    }

    let mut target_set_ids = Vec::new();
    for track in &snapshot.tracks {
        if let Some(sid) = set_queues
            .get_mut(track.video_id.as_str())
            .and_then(VecDeque::pop_front)
        {
            target_set_ids.push(sid);
        }
    }

    if current_set_ids.len() == target_set_ids.len() && !target_set_ids.is_empty() {
        let (_, moves) = compute_minimal_moves(&current_set_ids, &target_set_ids);
        moves
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use ytm_core::{MusicVideoType, PlaylistPrivacy, TrackAvailability};

    use super::*;
    use crate::sort::apply_move_actions;

    fn track(video_id: &str, set_video_id: Option<&str>, pos: usize) -> Track {
        Track {
            video_id: video_id.to_string(),
            set_video_id: set_video_id.map(str::to_string),
            position: pos,
            title: format!("Track {video_id}"),
            artists: Vec::new(),
            album: None,
            duration_sec: Some(180),
            release_year: None,
            music_video_type: MusicVideoType::AudioTrack,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        }
    }

    fn playlist(tracks: Vec<Track>) -> Playlist {
        Playlist {
            id: "PL_TEST".to_string(),
            title: "Test".to_string(),
            description: None,
            privacy: PlaylistPrivacy::Private,
            owned: true,
            track_count: tracks.len(),
            tracks,
        }
    }

    #[test]
    fn identical_snapshot_is_noop() {
        let snap = playlist(vec![track("V1", Some("S1"), 0), track("V2", Some("S2"), 1)]);
        let live = playlist(vec![track("V1", Some("L1"), 0), track("V2", Some("L2"), 1)]);
        let plan = plan_restore_diff(&live, &snap);
        assert!(plan.to_remove.is_empty());
        assert!(plan.removed_tracks.is_empty());
        assert!(plan.to_add.is_empty());
        assert!(plan.added_tracks.is_empty());
        assert!(!plan.allow_duplicates);
        assert_eq!(plan.target_video_order, vec!["V1", "V2"]);
        assert!(compute_restore_moves(&live, &snap).is_empty());
    }

    #[test]
    fn duplicate_counts_are_reconciled_accurately() {
        // Snapshot wants 2x V1 and 1x V3; live has 1x V1, 2x V2, 1x V4(no set_video_id).
        let snap = playlist(vec![
            track("V1", Some("S1"), 0),
            track("V3", Some("S2"), 1),
            track("V1", Some("S3"), 2),
        ]);
        let live = playlist(vec![
            track("V1", Some("L1"), 0),
            track("V2", Some("L2"), 1),
            track("V2", Some("L3"), 2),
            track("V4", None, 3),
        ]);
        let plan = plan_restore_diff(&live, &snap);
        assert_eq!(
            plan.to_remove,
            vec![
                ("V2".to_string(), "L2".to_string()),
                ("V2".to_string(), "L3".to_string()),
            ]
        );
        assert_eq!(plan.removed_tracks.len(), 2);
        assert_eq!(plan.to_add, vec!["V3", "V1"]);
        assert_eq!(plan.added_tracks.len(), 2);
        assert!(plan.allow_duplicates);
        assert_eq!(plan.target_video_order, vec!["V1", "V3", "V1"]);
    }

    #[test]
    fn missing_set_video_id_is_never_queued_for_removal() {
        let snap = playlist(vec![track("V1", Some("S1"), 0)]);
        let live = playlist(vec![
            track("V1", Some("L1"), 0),
            track("EXTRA_NO_SID", None, 1),
            track("EXTRA_WITH_SID", Some("L2"), 2),
        ]);
        let plan = plan_restore_diff(&live, &snap);
        assert_eq!(
            plan.to_remove,
            vec![("EXTRA_WITH_SID".to_string(), "L2".to_string())]
        );
        assert_eq!(plan.removed_tracks.len(), 1);
        assert_eq!(plan.removed_tracks[0].video_id, "EXTRA_WITH_SID");
    }

    #[test]
    fn compute_restore_moves_reconstructs_duplicate_and_reordered_sequence() {
        let snap = playlist(vec![
            track("V1", Some("S1"), 0),
            track("V2", Some("S2"), 1),
            track("V1", Some("S3"), 2),
            track("V3", Some("S4"), 3),
        ]);
        let updated = playlist(vec![
            track("V3", Some("U1"), 0),
            track("V1", Some("U2"), 1),
            track("V1", Some("U3"), 2),
            track("V2", Some("U4"), 3),
        ]);
        let moves = compute_restore_moves(&updated, &snap);
        let current_ids: Vec<String> = updated
            .tracks
            .iter()
            .filter_map(|t| t.set_video_id.clone())
            .collect();
        let reordered = apply_move_actions(&current_ids, &moves);
        assert_eq!(reordered, vec!["U2", "U4", "U3", "U1"]);
    }

    #[test]
    fn compute_restore_moves_returns_empty_on_length_mismatch() {
        let snap = playlist(vec![track("V1", Some("S1"), 0), track("V2", Some("S2"), 1)]);
        let updated = playlist(vec![track("V1", Some("U1"), 0)]);
        assert!(compute_restore_moves(&updated, &snap).is_empty());
    }
}
