use std::collections::{HashMap, HashSet};

use ytm_core::Playlist;
use ytm_ops::compute_minimal_moves;

use crate::state::DraftRevision;

#[path = "sync_draft.rs"]
mod sync_draft;
#[path = "sync_ops.rs"]
mod sync_ops;

pub use sync_ops::{StagedEntry, StagedOp};

/// Central local staging queue backing `Sync` preview & working-copy overlays.
#[derive(Debug, Clone, Default)]
pub struct SyncQueue {
    entries: Vec<StagedEntry>,
    redo_stack: Vec<StagedEntry>,
    next_id: u64,
    revision: DraftRevision,
}

impl SyncQueue {
    #[must_use]
    pub fn revision(&self) -> DraftRevision {
        self.revision
    }

    #[must_use]
    pub fn entries(&self) -> &[StagedEntry] {
        &self.entries
    }

    #[must_use]
    pub fn selected_entries(&self) -> Vec<StagedEntry> {
        self.entries
            .iter()
            .filter(|e| e.selected)
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn push(&mut self, op: StagedOp) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.entries.push(StagedEntry {
            id,
            op,
            selected: true,
        });
        self.redo_stack.clear();
        self.revision.bump();
        id
    }

    /// Undoes the most recent staged entry and pushes it onto the redo stack.
    pub fn undo_last(&mut self) -> Option<StagedEntry> {
        let popped = self.entries.pop()?;
        self.redo_stack.push(popped.clone());
        self.revision.bump();
        Some(popped)
    }

    /// Redoes the most recently undone staged entry.
    pub fn redo_last(&mut self) -> Option<StagedEntry> {
        let restored = self.redo_stack.pop()?;
        self.entries.push(restored.clone());
        self.revision.bump();
        Some(restored)
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn remove_entry(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        if self.entries.len() != before {
            self.revision.bump();
            true
        } else {
            false
        }
    }

    /// Removes all entries whose IDs are in `ids` (preserving remaining order) and returns count removed.
    pub fn remove_entries(&mut self, ids: &[u64]) -> usize {
        if ids.is_empty() {
            return 0;
        }
        let id_set: HashSet<u64> = ids.iter().copied().collect();
        let before = self.entries.len();
        self.entries.retain(|e| !id_set.contains(&e.id));
        let removed = before - self.entries.len();
        if removed > 0 {
            self.revision.bump();
        }
        removed
    }

    /// Toggles selection state of entry `id` and bumps `DraftRevision` if found.
    pub fn toggle_selected(&mut self, id: u64) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) else {
            return false;
        };
        entry.selected = !entry.selected;
        self.revision.bump();
        true
    }

    /// Replaces queue contents from persisted entries, advancing `next_id` beyond all loaded IDs.
    pub fn load_entries(&mut self, entries: Vec<StagedEntry>) {
        self.next_id = entries.iter().map(|e| e.id).max().unwrap_or(0);
        self.entries = entries;
        self.redo_stack.clear();
        self.revision.bump();
    }

    #[must_use]
    pub fn last_entry_id(&self) -> Option<u64> {
        self.entries.last().map(|e| e.id)
    }

    /// Moves a staged entry up (`delta < 0`) or down (`delta > 0`), guarding `Reorder` dependencies.
    pub fn move_entry(&mut self, id: u64, delta: isize) -> Result<(), String> {
        let Some(idx) = self.entries.iter().position(|e| e.id == id) else {
            return Err("Staged entry not found.".to_string());
        };
        if delta == 0 {
            return Ok(());
        }
        let Some(target) = idx.checked_add_signed(delta) else {
            return Err("Cannot move entry past start of queue.".to_string());
        };
        if target >= self.entries.len() {
            return Err("Cannot move entry past end of queue.".to_string());
        }
        if let Some(pid) = reorder_crossing_playlist(&self.entries, idx, target) {
            return Err(format!(
                "Cannot move Reorder across structural edit on '{pid}'; re-stage Sort instead."
            ));
        }
        self.entries.swap(idx, target);
        self.revision.bump();
        Ok(())
    }

    pub fn clear(&mut self) {
        if !self.entries.is_empty() {
            self.entries.clear();
            self.redo_stack.clear();
            self.revision.bump();
        }
    }

    #[must_use]
    pub fn affected_playlist_ids(&self) -> HashSet<String> {
        let mut out = HashSet::new();
        for entry in &self.entries {
            match &entry.op {
                StagedOp::MoveTracks {
                    source_playlist_id,
                    target_playlist_id,
                    ..
                } => {
                    out.insert(source_playlist_id.clone());
                    out.insert(target_playlist_id.clone());
                }
                other => {
                    if let Some(pid) = other.target_playlist_id() {
                        out.insert(pid.to_string());
                    }
                }
            }
        }
        out
    }

    /// Stages or coalesces a `Reorder` onto `playlist_id` relative to the pre-reorder working draft.
    pub fn stage_reorder(
        &mut self,
        playlist_id: &str,
        playlist_title: &str,
        target_set_video_ids: Vec<String>,
        base_cache: &HashMap<String, Playlist>,
    ) -> Result<usize, String> {
        let replace_idx = self.entries.iter().rposition(|e| {
            matches!(
                &e.op,
                StagedOp::Reorder { playlist_id: pid, .. } if pid == playlist_id
            )
        });
        let has_later_structural_edit = replace_idx.is_some_and(|idx| {
            self.entries[idx + 1..]
                .iter()
                .any(|e| e.op.is_structural_for(playlist_id))
        });

        let saved_prev = if let Some(idx) = replace_idx
            && !has_later_structural_edit
        {
            Some((idx, self.entries.remove(idx)))
        } else {
            None
        };

        let Some(pre_reorder) = self.working_playlist(playlist_id, base_cache) else {
            if let Some((idx, prev)) = saved_prev {
                self.entries.insert(idx, prev);
            }
            return Err("Playlist is not loaded in local cache yet.".to_string());
        };

        let baseline_ids: Vec<String> = pre_reorder
            .tracks
            .iter()
            .filter_map(|t| t.set_video_id.clone())
            .collect();
        if baseline_ids.len() != pre_reorder.tracks.len()
            || baseline_ids.len() != target_set_video_ids.len()
        {
            if let Some((idx, prev)) = saved_prev {
                self.entries.insert(idx, prev);
            }
            return Err(
                "Sync newly added tracks (or load missing setVideoIds) before reordering."
                    .to_string(),
            );
        }

        let (_, moves) = compute_minimal_moves(&baseline_ids, &target_set_video_ids);
        let move_count = moves.len();
        if !moves.is_empty() {
            self.push(StagedOp::Reorder {
                playlist_id: playlist_id.to_string(),
                playlist_title: playlist_title.to_string(),
                target_set_video_ids,
                moves,
            });
        } else if saved_prev.is_some() {
            self.revision.bump();
        }
        Ok(move_count)
    }

    /// Projects a playlist's local working state by replaying queued ops over the cached base.
    #[must_use]
    pub fn working_playlist(
        &self,
        playlist_id: &str,
        base_cache: &HashMap<String, Playlist>,
    ) -> Option<Playlist> {
        sync_draft::project_working_playlist(&self.entries, playlist_id, base_cache)
    }
}

fn reorder_crossing_playlist(entries: &[StagedEntry], idx: usize, target: usize) -> Option<&str> {
    let lo = idx.min(target);
    let hi = idx.max(target);
    for endpoint in [idx, target] {
        if let StagedOp::Reorder { playlist_id, .. } = &entries[endpoint].op
            && (lo..=hi)
                .filter(|&i| i != endpoint)
                .any(|i| entries[i].op.is_structural_for(playlist_id))
        {
            return Some(playlist_id.as_str());
        }
        for i in (lo..=hi).filter(|&i| i != endpoint) {
            if let StagedOp::Reorder { playlist_id, .. } = &entries[i].op
                && entries[endpoint].op.is_structural_for(playlist_id)
            {
                return Some(playlist_id.as_str());
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "sync_queue_tests.rs"]
mod tests;
