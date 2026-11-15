use std::collections::HashSet;

use ytm_core::{Playlist, Track};

use crate::dedupe::{fuzzy_track_similarity, within_duration_tolerance};
use crate::error::OpsError;
use crate::models::{DiffPlan, MatchMode, SetAlgebraOp};
use crate::normalize::{NormalizedTrack, normalize_track};

fn track_matches_any_fuzzy(
    needle: &Track,
    needle_norm: &NormalizedTrack,
    haystack: &[(Track, NormalizedTrack)],
    threshold: f64,
) -> bool {
    haystack.iter().any(|(other, other_norm)| {
        within_duration_tolerance(needle, other, 15, false)
            && fuzzy_track_similarity(needle_norm, other_norm, threshold).is_some()
    })
}

/// Computes `Diff` (`A \ B`), `Intersect` (`A ∩ B`), or `Merge` (`A ∪ B`) across two playlists.
pub fn plan_set_op(
    op: SetAlgebraOp,
    match_mode: MatchMode,
    playlist_a: &Playlist,
    playlist_b: &Playlist,
    target_playlist_id: Option<String>,
    fuzzy_threshold: f64,
) -> Result<DiffPlan, OpsError> {
    if fuzzy_threshold.is_nan() || !(0.0..=1.0).contains(&fuzzy_threshold) {
        return Err(OpsError::InvalidThreshold(fuzzy_threshold));
    }

    let mut result_tracks = Vec::new();
    let mut seen_video_ids: HashSet<String> = HashSet::new();

    match match_mode {
        MatchMode::ExactVideoId => {
            let b_ids: HashSet<&str> = playlist_b
                .tracks
                .iter()
                .filter(|t| !t.video_id.is_empty())
                .map(|t| t.video_id.as_str())
                .collect();

            match op {
                SetAlgebraOp::Diff => {
                    for t in &playlist_a.tracks {
                        if !t.video_id.is_empty()
                            && !b_ids.contains(t.video_id.as_str())
                            && seen_video_ids.insert(t.video_id.clone())
                        {
                            result_tracks.push(t.clone());
                        }
                    }
                }
                SetAlgebraOp::Intersect => {
                    for t in &playlist_a.tracks {
                        if !t.video_id.is_empty()
                            && b_ids.contains(t.video_id.as_str())
                            && seen_video_ids.insert(t.video_id.clone())
                        {
                            result_tracks.push(t.clone());
                        }
                    }
                }
                SetAlgebraOp::Merge => {
                    for t in playlist_a.tracks.iter().chain(playlist_b.tracks.iter()) {
                        if !t.video_id.is_empty() && seen_video_ids.insert(t.video_id.clone()) {
                            result_tracks.push(t.clone());
                        }
                    }
                }
            }
        }
        MatchMode::FuzzyNormalized => {
            let b_norm: Vec<(Track, NormalizedTrack)> = playlist_b
                .tracks
                .iter()
                .cloned()
                .map(|t| {
                    let norm = normalize_track(&t);
                    (t, norm)
                })
                .collect();
            let mut seen_norm: Vec<(Track, NormalizedTrack)> = Vec::new();

            match op {
                SetAlgebraOp::Diff => {
                    for t in &playlist_a.tracks {
                        let n = normalize_track(t);
                        if !track_matches_any_fuzzy(t, &n, &b_norm, fuzzy_threshold)
                            && !track_matches_any_fuzzy(t, &n, &seen_norm, fuzzy_threshold)
                        {
                            seen_norm.push((t.clone(), n));
                            result_tracks.push(t.clone());
                        }
                    }
                }
                SetAlgebraOp::Intersect => {
                    for t in &playlist_a.tracks {
                        let n = normalize_track(t);
                        if track_matches_any_fuzzy(t, &n, &b_norm, fuzzy_threshold)
                            && !track_matches_any_fuzzy(t, &n, &seen_norm, fuzzy_threshold)
                        {
                            seen_norm.push((t.clone(), n));
                            result_tracks.push(t.clone());
                        }
                    }
                }
                SetAlgebraOp::Merge => {
                    for t in playlist_a.tracks.iter().chain(playlist_b.tracks.iter()) {
                        let n = normalize_track(t);
                        if !track_matches_any_fuzzy(t, &n, &seen_norm, fuzzy_threshold) {
                            seen_norm.push((t.clone(), n));
                            result_tracks.push(t.clone());
                        }
                    }
                }
            }
        }
    }

    let add_video_ids = result_tracks
        .iter()
        .filter(|t| !t.video_id.is_empty())
        .map(|t| t.video_id.clone())
        .collect();

    Ok(DiffPlan {
        op,
        match_mode,
        source_a_id: playlist_a.id.clone(),
        source_b_id: playlist_b.id.clone(),
        result_tracks,
        target_playlist_id,
        add_video_ids,
    })
}
