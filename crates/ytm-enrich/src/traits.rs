use async_trait::async_trait;
use ytm_core::Track;

use crate::error::EnrichError;
use crate::models::EnrichedTrackMetadata;

/// Async metadata enrichment contract (SQLite cache, MusicBrainz, Last.fm, Deezer, ListenBrainz).
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    /// Lookup tags, release year, and genre metadata for `track`.
    async fn enrich_track(&self, track: &Track) -> Result<EnrichedTrackMetadata, EnrichError>;
}
