use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use ytm_core::{MoveAction, Playlist, Track};

use crate::error::OpsError;
use crate::models::{SortDirection, SortField, SortPlan, SortSpec};
use crate::normalize::normalize_track;

/// Computes the strictly minimal sequence of `MoveAction` edits via `O(N log N)` LIS.
///
/// Elements belonging to the Longest Increasing Subsequence stay anchored; every non-LIS
/// item emits one `ACTION_MOVE_VIDEO_BEFORE` emitted back-to-front so `target` ordering holds.
#[must_use]
pub fn compute_minimal_moves(current: &[String], target: &[String]) -> (usize, Vec<MoveAction>) {
    if current.len() != target.len() || current.is_empty() {
        return (0, Vec::new());
    }

    let target_pos: HashMap<&str, usize> = target
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    let mut ranks = Vec::with_capacity(current.len());
    for id in current {
        let Some(&pos) = target_pos.get(id.as_str()) else {
            return (0, Vec::new());
        };
        ranks.push(pos);
    }

    // Standard Patience-sorting O(N log N) LIS reconstruction over `ranks`.
    let n = ranks.len();
    let mut tail_vals: Vec<usize> = Vec::with_capacity(n);
    let mut tail_indices: Vec<usize> = Vec::with_capacity(n);
    let mut prev = vec![usize::MAX; n];

    for (i, &val) in ranks.iter().enumerate() {
        let pos = tail_vals.partition_point(|&x| x < val);
        if pos > 0 {
            prev[i] = tail_indices[pos - 1];
        }
        if pos == tail_vals.len() {
            tail_vals.push(val);
            tail_indices.push(i);
        } else {
            tail_vals[pos] = val;
            tail_indices[pos] = i;
        }
    }

    let mut anchored_set: HashSet<&str> = HashSet::with_capacity(tail_indices.len());
    let mut cur = *tail_indices.last().unwrap_or(&usize::MAX);
    while cur != usize::MAX {
        anchored_set.insert(current[cur].as_str());
        cur = prev[cur];
    }

    let anchored_count = anchored_set.len();
    let mut moves = Vec::with_capacity(n - anchored_count);

    // Emit moves in reverse target order so each moved item targets an already-settled suffix.
    for i in (0..target.len()).rev() {
        let id = &target[i];
        if !anchored_set.contains(id.as_str()) {
            let successor = target.get(i + 1).cloned();
            moves.push(MoveAction {
                set_video_id: id.clone(),
                moved_set_video_successor: successor,
            });
        }
    }

    (anchored_count, moves)
}

/// Applies `MoveAction` operations in memory (mirroring InnerTube `ACTION_MOVE_VIDEO_BEFORE`).
#[must_use]
pub fn apply_move_actions(items: &[String], moves: &[MoveAction]) -> Vec<String> {
    let mut working = items.to_vec();
    for action in moves {
        let Some(from_idx) = working.iter().position(|id| id == &action.set_video_id) else {
            continue;
        };
        let moved = working.remove(from_idx);
        match &action.moved_set_video_successor {
            Some(succ) => {
                if let Some(to_idx) = working.iter().position(|id| id == succ) {
                    working.insert(to_idx, moved);
                } else {
                    working.push(moved);
                }
            }
            None => working.push(moved),
        }
    }
    working
}

fn compare_tracks_by_field(
    a: &Track,
    b: &Track,
    field: SortField,
    genre_map: Option<&HashMap<String, String>>,
) -> Ordering {
    let na = normalize_track(a);
    let nb = normalize_track(b);

    match field {
        SortField::Artist => na
            .artist_norm
            .cmp(&nb.artist_norm)
            .then_with(|| na.title_norm.cmp(&nb.title_norm)),
        SortField::Album => a
            .album
            .as_ref()
            .map(|al| al.name.to_lowercase())
            .unwrap_or_default()
            .cmp(
                &b.album
                    .as_ref()
                    .map(|al| al.name.to_lowercase())
                    .unwrap_or_default(),
            ),
        SortField::Title => na.title_norm.cmp(&nb.title_norm),
        SortField::Duration => a.duration_sec.cmp(&b.duration_sec),
        SortField::Year => a.release_year.cmp(&b.release_year),
        SortField::Genre => {
            let ga = genre_map
                .and_then(|m| m.get(&a.video_id))
                .map(String::as_str)
                .unwrap_or("zzz");
            let gb = genre_map
                .and_then(|m| m.get(&b.video_id))
                .map(String::as_str)
                .unwrap_or("zzz");
            ga.cmp(gb)
        }
        SortField::Type => a
            .music_video_type
            .short_label()
            .cmp(b.music_video_type.short_label()),
        SortField::Shuffle => Ordering::Equal,
    }
}

fn apply_artist_spread(tracks: &mut Vec<Track>) {
    if tracks.len() < 3 {
        return;
    }
    let pool = std::mem::take(tracks);
    let mut items: Vec<(Track, String)> = pool
        .into_iter()
        .map(|t| {
            let artist = normalize_track(&t).artist_norm;
            (t, artist)
        })
        .collect();

    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, artist) in &items {
        *counts.entry(artist.clone()).or_insert(0) += 1;
    }

    let mut result = Vec::with_capacity(items.len());
    let mut prev_artist = String::new();

    while !items.is_empty() {
        // Pick an artist different from `prev_artist` with the highest remaining frequency
        // (preserving shuffled order on ties) so dominant artists never clump at the tail.
        let best_idx = items
            .iter()
            .enumerate()
            .filter(|(_, (_, a))| prev_artist.is_empty() || a.is_empty() || *a != prev_artist)
            .max_by(|(idx_a, (_, artist_a)), (idx_b, (_, artist_b))| {
                let ca = counts.get(artist_a).copied().unwrap_or(0);
                let cb = counts.get(artist_b).copied().unwrap_or(0);
                ca.cmp(&cb).then_with(|| idx_b.cmp(idx_a))
            })
            .map(|(idx, _)| idx)
            .unwrap_or(0);

        let (chosen, artist) = items.remove(best_idx);
        if let Some(c) = counts.get_mut(&artist) {
            *c = c.saturating_sub(1);
        }
        prev_artist = artist;
        result.push(chosen);
    }

    *tracks = result;
}

/// Computes a non-mutating minimal-move sort plan for `playlist` under `spec`.
pub fn plan_sort(
    playlist: &Playlist,
    spec: &SortSpec,
    genre_map: Option<&HashMap<String, String>>,
) -> Result<SortPlan, OpsError> {
    if spec.keys.is_empty() {
        return Err(OpsError::InvalidSortSpec("empty sort spec".to_string()));
    }

    let mut target_order = playlist.tracks.clone();
    let is_shuffle = spec.keys.iter().any(|k| k.field == SortField::Shuffle);

    if is_shuffle {
        let mut rng = StdRng::seed_from_u64(spec.seed.unwrap_or(0));
        target_order.shuffle(&mut rng);
        if spec.artist_spread {
            apply_artist_spread(&mut target_order);
        }
    } else {
        target_order.sort_by(|a, b| {
            for key in &spec.keys {
                let ord = compare_tracks_by_field(a, b, key.field, genre_map);
                let directed = match key.direction {
                    SortDirection::Asc => ord,
                    SortDirection::Desc => ord.reverse(),
                };
                if directed != Ordering::Equal {
                    return directed;
                }
            }
            a.position.cmp(&b.position)
        });
        if spec.artist_spread {
            apply_artist_spread(&mut target_order);
        }
    }

    let current_ids: Vec<String> = playlist
        .tracks
        .iter()
        .map(|t| {
            t.set_video_id
                .clone()
                .ok_or_else(|| OpsError::MissingSetVideoId {
                    title: t.title.clone(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let target_ids: Vec<String> = target_order
        .iter()
        .filter_map(|t| t.set_video_id.clone())
        .collect();

    let (anchored_count, moves) = compute_minimal_moves(&current_ids, &target_ids);

    Ok(SortPlan {
        playlist_id: playlist.id.clone(),
        playlist_title: playlist.title.clone(),
        target_order,
        anchored_count,
        moves,
    })
}
