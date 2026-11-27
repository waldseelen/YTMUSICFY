use async_trait::async_trait;
use ytm_core::Playlist;

use crate::error::StoreError;
use crate::models::{
    CachedArtistGenre, CachedTrack, CachedTrackEnrichment, Snapshot, SnapshotMetadata,
};
use crate::play_history::PlayHistoryEntry;

/// Atomic playlist snapshot persistence contract (JSON disk file + SQLite index).
#[async_trait]
pub trait SnapshotStore: Send + Sync {
    /// Persist an atomic snapshot before any remote mutation executes.
    async fn save_snapshot(
        &self,
        playlist: &Playlist,
        reason: &str,
    ) -> Result<SnapshotMetadata, StoreError>;

    /// List snapshots, optionally filtered by `playlist_id`, newest first.
    async fn list_snapshots(
        &self,
        playlist_id: Option<&str>,
    ) -> Result<Vec<SnapshotMetadata>, StoreError>;

    /// Load a complete snapshot payload by ID.
    async fn load_snapshot(&self, snapshot_id: &str) -> Result<Snapshot, StoreError>;
}

/// Local SQLite enrichment and track metadata cache contract.
#[async_trait]
pub trait EnrichmentCacheStore: Send + Sync {
    async fn get_cached_track(&self, video_id: &str) -> Result<Option<CachedTrack>, StoreError>;

    async fn upsert_cached_track(&self, track: &CachedTrack) -> Result<(), StoreError>;

    async fn get_artist_genre(
        &self,
        artist_name_norm: &str,
    ) -> Result<Option<CachedArtistGenre>, StoreError>;

    async fn upsert_artist_genre(&self, entry: &CachedArtistGenre) -> Result<(), StoreError>;

    async fn get_track_enrichment(
        &self,
        cache_key: &str,
    ) -> Result<Option<CachedTrackEnrichment>, StoreError>;

    async fn upsert_track_enrichment(
        &self,
        entry: &CachedTrackEnrichment,
    ) -> Result<(), StoreError>;

    /// Insert a track into the play history.
    async fn insert_play_history(
        &self,
        video_id: &str,
        title: &str,
        artists_json: &str,
        album: Option<&str>,
        duration_sec: Option<u32>,
    ) -> Result<(), StoreError>;

    /// Get recent play history entries, most recent first.
    async fn get_play_history(&self, limit: usize) -> Result<Vec<PlayHistoryEntry>, StoreError>;
}
