use std::collections::{HashMap, VecDeque};

use ytm_core::{InnerTubeClientTrait, MoveAction, MutationBatchReport, Playlist, Track};

use crate::state::DispatchEntryOutcome;

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
