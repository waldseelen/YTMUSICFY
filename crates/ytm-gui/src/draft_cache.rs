use std::collections::HashMap;
use std::sync::Arc;

use ytm_core::Playlist;

use crate::state::DraftRevision;
use crate::sync_queue::SyncQueue;

#[derive(Debug, Clone)]
struct CachedDraft {
    revision: DraftRevision,
    playlist: Arc<Playlist>,
}

/// Frame-memoized cache of projected working-copy playlists keyed by `(playlist_id, DraftRevision)`.
#[derive(Debug, Clone, Default)]
pub struct DraftCache {
    entries: HashMap<String, CachedDraft>,
}

impl DraftCache {
    /// Returns an already-cached draft if its stored `DraftRevision` matches `revision`.
    #[must_use]
    pub fn get(&self, playlist_id: &str, revision: DraftRevision) -> Option<Arc<Playlist>> {
        self.entries
            .get(playlist_id)
            .filter(|entry| entry.revision == revision)
            .map(|entry| Arc::clone(&entry.playlist))
    }

    /// Returns a memoized working draft for `playlist_id`, recomputing only on revision mismatch.
    pub fn get_or_compute(
        &mut self,
        playlist_id: &str,
        sync_queue: &SyncQueue,
        base_cache: &HashMap<String, Playlist>,
    ) -> Option<Arc<Playlist>> {
        let rev = sync_queue.revision();
        if let Some(hit) = self.get(playlist_id, rev) {
            return Some(hit);
        }
        let Some(projected) = sync_queue.working_playlist(playlist_id, base_cache) else {
            self.entries.remove(playlist_id);
            return None;
        };
        let arc = Arc::new(projected);
        self.entries.insert(
            playlist_id.to_string(),
            CachedDraft {
                revision: rev,
                playlist: Arc::clone(&arc),
            },
        );
        Some(arc)
    }

    /// Returns the memoized draft track count for `playlist_id` without cloning `Playlist`.
    pub fn track_count(
        &mut self,
        playlist_id: &str,
        sync_queue: &SyncQueue,
        base_cache: &HashMap<String, Playlist>,
    ) -> Option<usize> {
        self.get_or_compute(playlist_id, sync_queue, base_cache)
            .map(|pl| pl.tracks.len())
    }

    /// Invalidates a single playlist entry when its underlying `playlist_cache` payload refreshes.
    pub fn invalidate_playlist(&mut self, playlist_id: &str) {
        self.entries.remove(playlist_id);
    }

    /// Clears all memoized drafts (e.g., on full library/cache hydration).
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use ytm_core::{PlaylistPrivacy, Track};

    use super::*;
    use crate::sync_queue::StagedOp;

    fn mk_playlist(id: &str, title: &str) -> Playlist {
        Playlist {
            id: id.to_string(),
            title: title.to_string(),
            description: None,
            privacy: PlaylistPrivacy::Private,
            owned: true,
            track_count: 0,
            tracks: Vec::<Track>::new(),
        }
    }

    #[test]
    fn draft_cache_returns_identical_arc_and_invalidates_on_revision_bump() {
        let mut base = HashMap::new();
        base.insert("PL1".to_string(), mk_playlist("PL1", "Initial"));

        let mut queue = SyncQueue::default();
        let mut cache = DraftCache::default();

        let first = cache
            .get_or_compute("PL1", &queue, &base)
            .expect("first draft");
        let second = cache
            .get("PL1", queue.revision())
            .expect("direct revision hit");
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(cache.track_count("PL1", &queue, &base), Some(0));

        let old_rev = queue.revision();
        queue.push(StagedOp::UpdatePlaylistMeta {
            playlist_id: "PL1".to_string(),
            playlist_title: "Initial".to_string(),
            title: Some("Renamed".to_string()),
            description: None,
            privacy: None,
        });
        assert_ne!(queue.revision(), old_rev);
        assert!(cache.get("PL1", queue.revision()).is_none());

        let updated = cache
            .get_or_compute("PL1", &queue, &base)
            .expect("recomputed draft");
        assert!(!Arc::ptr_eq(&first, &updated));
        assert_eq!(updated.title, "Renamed");
        assert!(cache.get("PL1", old_rev).is_none());

        cache.invalidate_playlist("PL1");
        assert!(cache.get("PL1", queue.revision()).is_none());
    }
}
