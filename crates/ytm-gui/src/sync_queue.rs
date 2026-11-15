use std::collections::{HashMap, HashSet};

use ytm_core::{MoveAction, Playlist, PlaylistPrivacy, Track};
use ytm_ops::compute_minimal_moves;

/// A single pending remote mutation queued locally until the user clicks `Sync Now`.
#[derive(Debug, Clone)]
pub enum StagedOp {
    Reorder {
        playlist_id: String,
        playlist_title: String,
        target_set_video_ids: Vec<String>,
        moves: Vec<MoveAction>,
    },
    AddTracks {
        playlist_id: String,
        playlist_title: String,
        tracks: Vec<Track>,
        allow_duplicates: bool,
    },
    RemoveTracks {
        playlist_id: String,
        playlist_title: String,
        tracks: Vec<Track>,
    },
    CreatePlaylist {
        title: String,
        description: String,
        privacy: PlaylistPrivacy,
        tracks: Vec<Track>,
    },
    UpgradeTracks {
        playlist_id: String,
        playlist_title: String,
        upgrades: Vec<(Track, Track)>,
    },
}

impl StagedOp {
    #[must_use]
    pub fn target_playlist_id(&self) -> Option<&str> {
        match self {
            Self::Reorder { playlist_id, .. }
            | Self::AddTracks { playlist_id, .. }
            | Self::RemoveTracks { playlist_id, .. }
            | Self::UpgradeTracks { playlist_id, .. } => Some(playlist_id.as_str()),
            Self::CreatePlaylist { .. } => None,
        }
    }

    #[must_use]
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Reorder { .. } => "Reorder (LIS)",
            Self::AddTracks { .. } => "Add Tracks",
            Self::RemoveTracks { .. } => "Remove Tracks",
            Self::CreatePlaylist { .. } => "Create Playlist",
            Self::UpgradeTracks { .. } => "ATV Upgrade",
        }
    }

    #[must_use]
    pub fn target_label(&self) -> &str {
        match self {
            Self::Reorder { playlist_title, .. }
            | Self::AddTracks { playlist_title, .. }
            | Self::RemoveTracks { playlist_title, .. }
            | Self::UpgradeTracks { playlist_title, .. } => playlist_title.as_str(),
            Self::CreatePlaylist { title, .. } => title.as_str(),
        }
    }

    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::Reorder {
                playlist_title,
                moves,
                ..
            } => format!("Reorder '{playlist_title}' with {} LIS moves", moves.len()),
            Self::AddTracks {
                playlist_title,
                tracks,
                ..
            } => format!("Add {} tracks to '{playlist_title}'", tracks.len()),
            Self::RemoveTracks {
                playlist_title,
                tracks,
                ..
            } => format!("Remove {} tracks from '{playlist_title}'", tracks.len()),
            Self::CreatePlaylist { title, tracks, .. } => {
                format!("Create playlist '{title}' with {} tracks", tracks.len())
            }
            Self::UpgradeTracks {
                playlist_title,
                upgrades,
                ..
            } => format!(
                "Upgrade {} tracks to Official ATV in '{playlist_title}'",
                upgrades.len()
            ),
        }
    }
}

/// Sequenced entry in the local Sync staging queue.
#[derive(Debug, Clone)]
pub struct StagedEntry {
    pub id: u64,
    pub op: StagedOp,
}

/// Central local staging queue backing `Sync` preview & working-copy overlays.
#[derive(Debug, Clone, Default)]
pub struct SyncQueue {
    entries: Vec<StagedEntry>,
    next_id: u64,
}

impl SyncQueue {
    #[must_use]
    pub fn entries(&self) -> &[StagedEntry] {
        &self.entries
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
        self.entries.push(StagedEntry { id, op });
        id
    }

    pub fn remove_entry(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    #[must_use]
    pub fn last_entry_id(&self) -> Option<u64> {
        self.entries.last().map(|e| e.id)
    }

    /// Moves a staged entry one slot up (`delta < 0`) or down (`delta > 0`).
    pub fn move_entry(&mut self, id: u64, delta: isize) -> bool {
        let Some(idx) = self.entries.iter().position(|e| e.id == id) else {
            return false;
        };
        let Some(target) = idx.checked_add_signed(delta) else {
            return false;
        };
        if target >= self.entries.len() {
            return false;
        }
        self.entries.swap(idx, target);
        true
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[must_use]
    pub fn affected_playlist_ids(&self) -> HashSet<String> {
        self.entries
            .iter()
            .filter_map(|e| e.op.target_playlist_id().map(str::to_string))
            .collect()
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
                .any(|e| e.op.target_playlist_id() == Some(playlist_id))
        });

        let saved_prev = if let Some(idx) = replace_idx
            && !has_later_structural_edit
        {
            Some(self.entries.remove(idx))
        } else {
            None
        };

        let Some(pre_reorder) = self.working_playlist(playlist_id, base_cache) else {
            if let Some(prev) = saved_prev {
                self.entries.push(prev);
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
            if let Some(prev) = saved_prev {
                self.entries.push(prev);
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
        let mut pl = base_cache.get(playlist_id)?.clone();
        for entry in &self.entries {
            apply_op_to_draft(&mut pl, &entry.op);
        }
        Some(pl)
    }
}

fn apply_op_to_draft(pl: &mut Playlist, op: &StagedOp) {
    match op {
        StagedOp::AddTracks {
            playlist_id,
            tracks,
            allow_duplicates,
            ..
        } if playlist_id == &pl.id => {
            let mut existing: HashSet<String> = if *allow_duplicates {
                HashSet::new()
            } else {
                pl.tracks.iter().map(|t| t.video_id.clone()).collect()
            };
            for t in tracks {
                if *allow_duplicates || existing.insert(t.video_id.clone()) {
                    let mut cloned = t.clone();
                    cloned.set_video_id = None;
                    pl.tracks.push(cloned);
                }
            }
            reindex_playlist(pl);
        }
        StagedOp::RemoveTracks {
            playlist_id,
            tracks,
            ..
        } if playlist_id == &pl.id => {
            for target in tracks {
                if let Some(pos) = pl.tracks.iter().position(|cur| {
                    if let (Some(a), Some(b)) = (&target.set_video_id, &cur.set_video_id) {
                        a == b
                    } else {
                        cur.video_id == target.video_id
                    }
                }) {
                    pl.tracks.remove(pos);
                }
            }
            reindex_playlist(pl);
        }
        StagedOp::UpgradeTracks {
            playlist_id,
            upgrades,
            ..
        } if playlist_id == &pl.id => {
            for (old_t, atv) in upgrades {
                if let Some(pos) = pl.tracks.iter().position(|cur| {
                    if let (Some(a), Some(b)) = (&old_t.set_video_id, &cur.set_video_id) {
                        a == b
                    } else {
                        cur.video_id == old_t.video_id
                    }
                }) {
                    let mut replacement = atv.clone();
                    replacement.set_video_id = None;
                    pl.tracks[pos] = replacement;
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
        _ => {}
    }
}

fn reindex_playlist(pl: &mut Playlist) {
    for (idx, track) in pl.tracks.iter_mut().enumerate() {
        track.position = idx;
    }
    pl.track_count = pl.tracks.len();
}

#[cfg(test)]
#[path = "sync_queue_tests.rs"]
mod tests;
