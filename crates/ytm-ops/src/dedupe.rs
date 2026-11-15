use std::collections::{HashMap, HashSet};

use strsim::jaro_winkler;
use ytm_core::{MusicVideoType, Playlist, Track};

use crate::error::OpsError;
use crate::models::{DedupeConfig, DedupePlan, DuplicateCluster, KeeperStrategy};
use crate::normalize::{NormalizedTrack, normalize_track};

/// Deterministic track metadata & stream quality heuristic score.
#[must_use]
pub fn keeper_score(track: &Track) -> i32 {
    let type_score = match track.music_video_type {
        MusicVideoType::AudioTrack => 40,
        MusicVideoType::OfficialSourceMusic => 30,
        MusicVideoType::OfficialMusicVideo => 15,
        MusicVideoType::Unknown => 0,
        MusicVideoType::UserGenerated => -20,
    };
    let availability_score = if track.availability.is_available() {
        50
    } else {
        -100
    };
    let normalized = normalize_track(track);
    let clean_score = if normalized.is_clean_title { 10 } else { 0 };
    let album_score = if track.album.is_some() { 5 } else { 0 };
    let artist_score = if track.artists.is_empty() { 0 } else { 3 };
    let year_score = if track.release_year.is_some() { 2 } else { 0 };

    type_score + availability_score + clean_score + album_score + artist_score + year_score
}

/// Checks whether two tracks pass the duration tolerance guard.
pub(crate) fn within_duration_tolerance(
    a: &Track,
    b: &Track,
    tolerance_sec: u32,
    ignore_duration: bool,
) -> bool {
    if ignore_duration {
        return true;
    }
    match (a.duration_sec, b.duration_sec) {
        (Some(da), Some(db)) => da.abs_diff(db) <= tolerance_sec,
        _ => true,
    }
}

/// Computes fuzzy similarity between two normalized tracks, returning `None` if incompatible.
pub(crate) fn fuzzy_track_similarity(
    a: &NormalizedTrack,
    b: &NormalizedTrack,
    threshold: f64,
) -> Option<f64> {
    if a.title_norm.is_empty() || b.title_norm.is_empty() {
        return None;
    }

    let title_sim = jaro_winkler(&a.title_norm, &b.title_norm);
    match (a.artist_norm.is_empty(), b.artist_norm.is_empty()) {
        (false, false) => {
            let artist_sim = jaro_winkler(&a.artist_norm, &b.artist_norm);
            let combined = 0.4 * artist_sim + 0.6 * title_sim;
            let component_floor = threshold.min(0.82);
            if artist_sim >= component_floor
                && title_sim >= component_floor
                && combined >= threshold
            {
                Some(combined)
            } else {
                None
            }
        }
        (true, true) => {
            if a.title_norm == b.title_norm {
                Some(title_sim)
            } else {
                None
            }
        }
        _ => {
            if a.title_norm == b.title_norm && threshold <= 0.90 {
                Some(0.90)
            } else {
                None
            }
        }
    }
}

fn choose_keeper_pos(indices: &[usize], tracks: &[Track], strategy: KeeperStrategy) -> usize {
    let mut best_pos = 0usize;
    for (pos, &idx) in indices.iter().enumerate().skip(1) {
        let candidate = &tracks[idx];
        let incumbent = &tracks[indices[best_pos]];
        let replace = match strategy {
            KeeperStrategy::Quality => {
                let c_score = keeper_score(candidate);
                let i_score = keeper_score(incumbent);
                c_score > i_score || (c_score == i_score && candidate.position < incumbent.position)
            }
            KeeperStrategy::Oldest => {
                let c_avail = candidate.availability.is_available();
                let i_avail = incumbent.availability.is_available();
                (c_avail && !i_avail)
                    || (c_avail == i_avail && candidate.position < incumbent.position)
            }
            KeeperStrategy::Newest => {
                let c_avail = candidate.availability.is_available();
                let i_avail = incumbent.availability.is_available();
                (c_avail && !i_avail)
                    || (c_avail == i_avail && candidate.position > incumbent.position)
            }
        };
        if replace {
            best_pos = pos;
        }
    }
    best_pos
}

fn build_cluster(
    indices: &[usize],
    tracks: &[Track],
    strategy: KeeperStrategy,
    reason: String,
    similarity: f64,
    skipped_without_set_id: &mut usize,
) -> (usize, Option<DuplicateCluster>) {
    let keeper_pos = choose_keeper_pos(indices, tracks, strategy);
    let keeper_idx = indices[keeper_pos];
    let keeper = tracks[keeper_idx].clone();

    let mut removals = Vec::new();
    for (pos, &idx) in indices.iter().enumerate() {
        if pos == keeper_pos {
            continue;
        }
        let candidate = &tracks[idx];
        if candidate.set_video_id.is_some() {
            removals.push(candidate.clone());
        } else {
            *skipped_without_set_id += 1;
        }
    }

    let cluster = if removals.is_empty() {
        None
    } else {
        Some(DuplicateCluster {
            keeper,
            removals,
            reason,
            similarity,
        })
    };
    (keeper_idx, cluster)
}

/// Computes a non-mutating deduplication plan (`Pass 1` exact `video_id`, `Pass 2` fuzzy metadata).
pub fn plan_dedupe(playlist: &Playlist, config: &DedupeConfig) -> Result<DedupePlan, OpsError> {
    if config.fuzzy_threshold.is_nan() || !(0.0..=1.0).contains(&config.fuzzy_threshold) {
        return Err(OpsError::InvalidThreshold(config.fuzzy_threshold));
    }

    let tracks = &playlist.tracks;
    let mut removed_or_grouped: HashSet<usize> = HashSet::new();
    let mut clusters = Vec::new();
    let mut skipped_without_set_id = 0usize;

    // Pass 1: exact video_id duplicates while keeping surviving keeper eligible for Pass 2.
    let mut by_video_id: HashMap<&str, Vec<usize>> = HashMap::new();
    for (idx, track) in tracks.iter().enumerate() {
        if !track.video_id.is_empty() {
            by_video_id.entry(&track.video_id).or_default().push(idx);
        }
    }

    let mut exact_groups: Vec<Vec<usize>> = by_video_id
        .into_values()
        .filter(|group| group.len() > 1)
        .collect();
    exact_groups.sort_by_key(|group| group[0]);

    for group in exact_groups {
        let (keeper_idx, cluster) = build_cluster(
            &group,
            tracks,
            config.keep,
            "exact video_id match".to_string(),
            1.0,
            &mut skipped_without_set_id,
        );
        for &idx in &group {
            if idx != keeper_idx {
                removed_or_grouped.insert(idx);
            }
        }
        if let Some(c) = cluster {
            clusters.push(c);
        }
    }

    // Pass 2: fuzzy normalized title/artist matching across remaining tracks.
    if !config.exact_only {
        let surviving: Vec<usize> = (0..tracks.len())
            .filter(|idx| !removed_or_grouped.contains(idx))
            .collect();
        let normalized: Vec<NormalizedTrack> = surviving
            .iter()
            .map(|&idx| normalize_track(&tracks[idx]))
            .collect();
        let mut consumed = vec![false; surviving.len()];

        for i in 0..surviving.len() {
            if consumed[i] {
                continue;
            }
            let anchor_idx = surviving[i];
            let mut group = vec![anchor_idx];
            let mut min_similarity = 1.0_f64;

            for j in (i + 1)..surviving.len() {
                if consumed[j] {
                    continue;
                }
                let cand_idx = surviving[j];
                if !within_duration_tolerance(
                    &tracks[anchor_idx],
                    &tracks[cand_idx],
                    config.duration_tolerance_sec,
                    config.ignore_duration,
                ) {
                    continue;
                }
                if let Some(sim) =
                    fuzzy_track_similarity(&normalized[i], &normalized[j], config.fuzzy_threshold)
                {
                    consumed[j] = true;
                    group.push(cand_idx);
                    if sim < min_similarity {
                        min_similarity = sim;
                    }
                }
            }

            if group.len() > 1 {
                consumed[i] = true;
                let (keeper_idx, cluster) = build_cluster(
                    &group,
                    tracks,
                    config.keep,
                    format!(
                        "fuzzy normalized match ({:.2} >= {:.2})",
                        min_similarity, config.fuzzy_threshold
                    ),
                    min_similarity,
                    &mut skipped_without_set_id,
                );
                for &idx in &group {
                    if idx != keeper_idx {
                        removed_or_grouped.insert(idx);
                    }
                }
                if let Some(c) = cluster {
                    clusters.push(c);
                }
            }
        }
    }

    clusters.sort_by_key(|c| c.keeper.position);
    let total_removals = clusters.iter().map(|c| c.removals.len()).sum();

    Ok(DedupePlan {
        playlist_id: playlist.id.clone(),
        playlist_title: playlist.title.clone(),
        clusters,
        total_removals,
        skipped_without_set_id,
    })
}
