use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::Sender;
use std::time::Duration;

use ytm_core::{
    ChunkOutcome, InnerTubeClientTrait, MoveAction, MutationBatchReport, Playlist, Track,
};

use crate::state::{DispatchEntryOutcome, WorkerEvent};

pub(super) async fn execute_move_tracks<C: InnerTubeClientTrait>(
    client: &C,
    source_id: &str,
    target_id: &str,
    add_ids: &[String],
    remove_pairs: &[(String, String)],
) -> Result<(usize, usize), (usize, usize, String)> {
    let mut chunks = 0usize;
    let mut confirmed = 0usize;
    if !add_ids.is_empty() {
        let add_report = client
            .add_playlist_items_report(target_id, add_ids, false)
            .await
            .map_err(|e| (0, 2, e.to_string()))?;
        if !add_report.is_complete_success() {
            return Err((
                add_report.succeeded_chunks(),
                add_report.total_chunks.max(1),
                add_report
                    .first_error()
                    .unwrap_or("Add phase failed")
                    .to_string(),
            ));
        }
        let added = add_report.added_items();
        if added.is_empty() {
            return Err((
                add_report.succeeded_chunks(),
                add_report.total_chunks.max(1),
                "Target playlist add returned zero confirmed items; aborting source removal."
                    .to_string(),
            ));
        }
        chunks += add_report.succeeded_chunks();
        confirmed += added.len();
    }
    if !remove_pairs.is_empty() {
        let rem_report = client
            .remove_playlist_items_report(source_id, remove_pairs)
            .await
            .map_err(|e| (chunks, chunks + 1, e.to_string()))?;
        if !rem_report.is_complete_success() {
            return Err((
                chunks + rem_report.succeeded_chunks(),
                chunks + rem_report.total_chunks.max(1),
                rem_report
                    .first_error()
                    .unwrap_or("Remove phase failed")
                    .to_string(),
            ));
        }
        chunks += rem_report.succeeded_chunks();
    }
    Ok((chunks, confirmed))
}

pub(super) async fn execute_upgrade_tracks<C: InnerTubeClientTrait>(
    client: &C,
    playlist_id: &str,
    upgrades: &[(Track, Track)],
    preserve_position: bool,
    live: &Playlist,
) -> Result<(usize, usize), (usize, usize, String)> {
    if upgrades.is_empty() {
        return Ok((0, 0));
    }
    let add_ids: Vec<String> = upgrades
        .iter()
        .map(|(_, atv)| atv.video_id.clone())
        .collect();
    let add_report = client
        .add_playlist_items_report(playlist_id, &add_ids, false)
        .await
        .map_err(|e| (0, 2, e.to_string()))?;
    if !add_report.is_complete_success() {
        return Err((
            add_report.succeeded_chunks(),
            add_report.total_chunks.max(1),
            add_report
                .first_error()
                .unwrap_or("Upgrade add failed")
                .to_string(),
        ));
    }

    let mut added_by_vid: HashMap<String, VecDeque<String>> = HashMap::new();
    for item in add_report.added_items() {
        added_by_vid
            .entry(item.video_id)
            .or_default()
            .push_back(item.set_video_id);
    }

    let mut moves = Vec::new();
    let mut remove_pairs = Vec::new();
    for (old_t, atv) in upgrades {
        let Some(new_sid) = added_by_vid
            .get_mut(&atv.video_id)
            .and_then(VecDeque::pop_front)
        else {
            continue;
        };
        if let Some((old_sid, successor)) = find_live_track_and_successor(live, old_t) {
            if preserve_position {
                moves.push(MoveAction {
                    set_video_id: new_sid,
                    moved_set_video_successor: successor,
                });
            }
            remove_pairs.push((old_t.video_id.clone(), old_sid));
        }
    }

    let mut chunks = add_report.succeeded_chunks();
    if preserve_position && !moves.is_empty() {
        let move_report = client
            .move_playlist_items_report(playlist_id, &moves)
            .await
            .map_err(|e| (chunks, chunks + 1, e.to_string()))?;
        if !move_report.is_complete_success() {
            return Err((
                chunks + move_report.succeeded_chunks(),
                chunks + move_report.total_chunks.max(1),
                move_report
                    .first_error()
                    .unwrap_or("Upgrade position move failed")
                    .to_string(),
            ));
        }
        chunks += move_report.succeeded_chunks();
    }

    if !remove_pairs.is_empty() {
        let rem_report = client
            .remove_playlist_items_report(playlist_id, &remove_pairs)
            .await
            .map_err(|e| (chunks, chunks + 1, e.to_string()))?;
        if !rem_report.is_complete_success() {
            return Err((
                chunks + rem_report.succeeded_chunks(),
                chunks + rem_report.total_chunks.max(1),
                rem_report
                    .first_error()
                    .unwrap_or("Upgrade remove failed")
                    .to_string(),
            ));
        }
        chunks += rem_report.succeeded_chunks();
    }

    Ok((chunks, remove_pairs.len()))
}

fn find_live_track_and_successor(
    live: &Playlist,
    target: &Track,
) -> Option<(String, Option<String>)> {
    let idx = live.tracks.iter().position(|cur| {
        if let (Some(a), Some(b)) = (&target.set_video_id, &cur.set_video_id) {
            a == b
        } else {
            cur.video_id == target.video_id
        }
    })?;
    let sid = live.tracks[idx].set_video_id.clone()?;
    let successor = live
        .tracks
        .get(idx + 1)
        .and_then(|next| next.set_video_id.clone());
    Some((sid, successor))
}

pub(super) fn resolve_remove_pairs(tracks: &[Track], live: &Playlist) -> Vec<(String, String)> {
    tracks
        .iter()
        .filter_map(|t| {
            let sid = t.set_video_id.clone().or_else(|| {
                live.tracks
                    .iter()
                    .find(|cur| cur.video_id == t.video_id)
                    .and_then(|cur| cur.set_video_id.clone())
            })?;
            Some((t.video_id.clone(), sid))
        })
        .collect()
}

pub(super) fn same_multiset(a: &[String], b: &[String]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut sa: Vec<&str> = a.iter().map(String::as_str).collect();
    let mut sb: Vec<&str> = b.iter().map(String::as_str).collect();
    sa.sort_unstable();
    sb.sort_unstable();
    sa == sb
}

pub(super) fn report_result(
    res: std::result::Result<MutationBatchReport, ytm_core::CoreError>,
) -> Result<(usize, usize), (usize, usize, String)> {
    let report = res.map_err(|e| (0, 1, e.to_string()))?;
    if report.is_complete_success() {
        Ok((report.succeeded_chunks(), report.added_items().len()))
    } else {
        Err((
            report.succeeded_chunks(),
            report.total_chunks.max(1),
            report
                .first_error()
                .unwrap_or("Mutation batch failed")
                .to_string(),
        ))
    }
}

/// Collect playable video IDs for `AddTracks`, omitting duplicates already in `live`
/// or repeated earlier in `tracks` when `allow_duplicates` is false.
pub(in crate::worker_exec) fn collect_add_ids(
    tracks: &[Track],
    live: &Playlist,
    allow_duplicates: bool,
) -> Vec<String> {
    let mut existing: HashSet<&str> = if allow_duplicates {
        HashSet::new()
    } else {
        live.tracks.iter().map(|t| t.video_id.as_str()).collect()
    };
    let mut ids = Vec::with_capacity(tracks.len());
    for t in tracks {
        if t.video_id.is_empty() || !t.availability.is_available() {
            continue;
        }
        if !allow_duplicates && !existing.insert(t.video_id.as_str()) {
            continue;
        }
        ids.push(t.video_id.clone());
    }
    ids
}

/// Add tracks to a playlist; if a large chunk hits `STATUS_FAILED`, retry once and bisect
/// the remaining tail so one unaddable/restricted video cannot abort the entire copy.
pub(super) async fn add_tracks_resilient<C: InnerTubeClientTrait>(
    client: &C,
    playlist_id: &str,
    ids: &[String],
    allow_duplicates: bool,
    tx: &Sender<WorkerEvent>,
) -> Result<(usize, usize), (usize, usize, String)> {
    if ids.is_empty() {
        return Ok((0, 0));
    }
    let report = client
        .add_playlist_items_report(playlist_id, ids, allow_duplicates)
        .await
        .map_err(|e| (0, 1, e.to_string()))?;
    if report.is_complete_success() {
        return Ok((report.succeeded_chunks(), report.added_items().len()));
    }

    let first_err = report
        .first_error()
        .unwrap_or("browse/edit_playlist returned STATUS_FAILED")
        .to_string();
    let done_tracks: usize = report
        .outcomes
        .iter()
        .filter_map(|o| match o {
            ChunkOutcome::Succeeded { action_count, .. } => Some(*action_count),
            ChunkOutcome::Failed { .. } => None,
        })
        .sum();
    let mut total_chunks = report.total_chunks.max(1);
    let mut ok_chunks = report.succeeded_chunks();
    let mut ok_items = report.added_items().len();
    let tail = &ids[done_tracks.min(ids.len())..];

    // If `ids` fits in 1 chunk and there's only 1 item, don't recurse further.
    if tail.is_empty() || (tail.len() == 1 && ok_items == 0) {
        return Err((ok_chunks, total_chunks, first_err));
    }

    // Brief cooldown before bisecting the failed tail (handles post-100 rate bursts too).
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let mut queue: VecDeque<&[String]> = tail.chunks(10).collect();
    let mut rejected: Vec<String> = Vec::new();
    while let Some(slice) = queue.pop_front() {
        total_chunks += 1;
        match client
            .add_playlist_items_report(playlist_id, slice, allow_duplicates)
            .await
        {
            Ok(sub) if sub.is_complete_success() => {
                ok_chunks += sub.succeeded_chunks().max(1);
                ok_items += sub.added_items().len().max(slice.len());
            }
            _ if slice.len() > 1 => {
                let mid = slice.len() / 2;
                let (left, right) = slice.split_at(mid);
                queue.push_front(right);
                queue.push_front(left);
                tokio::time::sleep(Duration::from_millis(350)).await;
            }
            _ => {
                rejected.push(slice[0].clone());
            }
        }
    }

    if ok_items == 0 {
        return Err((ok_chunks, total_chunks, first_err));
    }
    if !rejected.is_empty() {
        let preview = rejected
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let _ = tx.send(WorkerEvent::StatusToast {
            ok: false,
            message: format!(
                "Added {ok_items} track(s); skipped {} unaddable video(s): {preview}",
                rejected.len()
            ),
        });
    }
    Ok((ok_chunks, ok_items))
}

pub(super) async fn execute_set_like_status_batch<C: InnerTubeClientTrait>(
    client: &C,
    video_ids: &[String],
    liked: bool,
    touched_ids: &mut Vec<String>,
) -> Result<(usize, usize), (usize, usize, String)> {
    let mut completed = 0;
    for vid in video_ids {
        if let Err(e) = client.set_like_status(vid, liked).await {
            return Err((completed, video_ids.len(), e.to_string()));
        }
        completed += 1;
    }
    if !touched_ids.iter().any(|id| id == "LM") {
        touched_ids.push("LM".to_string());
    }
    Ok((completed, video_ids.len()))
}

pub(super) fn failed_outcome(
    entry_id: u64,
    completed_chunks: usize,
    total_chunks: usize,
    error: String,
) -> DispatchEntryOutcome {
    DispatchEntryOutcome::Failed {
        entry_id,
        completed_chunks,
        total_chunks,
        error,
    }
}
