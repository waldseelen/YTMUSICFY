use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ytm_core::{FetchManifest, MusicVideoType, Playlist};

use crate::error::StoreError;

/// Supported playlist export serialization formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Json,
    Csv,
    M3u8,
}

impl FromStr for ExportFormat {
    type Err = StoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "csv" => Ok(Self::Csv),
            "m3u8" => Ok(Self::M3u8),
            other => Err(StoreError::InvalidExportFormat(other.to_string())),
        }
    }
}

/// Summary metadata for a persisted playlist snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub id: String,
    pub playlist_id: String,
    pub playlist_title: String,
    pub reason: String,
    pub created_at: DateTime<Utc>,
    pub item_count: usize,
}

/// Full atomic snapshot capturing a playlist before any remote mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub metadata: SnapshotMetadata,
    pub playlist: Playlist,
}

/// Row model for `track_cache` SQLite table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedTrack {
    pub video_id: String,
    pub title: String,
    pub artists_json: String,
    pub album: Option<String>,
    pub duration_sec: Option<u32>,
    pub release_year: Option<u16>,
    pub music_video_type: MusicVideoType,
    pub updated_at: i64,
}

/// Row model for `artist_genres` SQLite table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedArtistGenre {
    pub artist_name_norm: String,
    pub raw_tags: Vec<String>,
    pub primary_genre: String,
    pub source: String,
    pub updated_at: i64,
}

/// Row model for `track_enrichment` SQLite table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedTrackEnrichment {
    pub cache_key: String,
    pub video_id: Option<String>,
    pub tags: Vec<String>,
    pub primary_genre: String,
    pub release_year: Option<u16>,
    pub bpm: Option<f32>,
    pub source: String,
    pub updated_at: i64,
}

/// Cached playlist payload paired with optional fetch telemetry and cache timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedPlaylistRecord {
    pub playlist: Playlist,
    pub manifest: Option<FetchManifest>,
    pub fetched_at: i64,
}

/// Persisted `SyncQueue` entry stored in the `staged_ops` SQLite table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedStagedOp {
    pub entry_id: u64,
    pub seq: usize,
    pub selected: bool,
    pub op_json: String,
    pub created_at: i64,
}

/// Persisted playback queue/session state stored in the `playback_session` SQLite table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedPlaybackSession {
    pub session_json: String,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ytm_core::PlaylistPrivacy;

    #[test]
    fn export_format_parses_case_insensitively() {
        assert_eq!(ExportFormat::from_str("JSON").unwrap(), ExportFormat::Json);
        assert_eq!(ExportFormat::from_str("csv").unwrap(), ExportFormat::Csv);
        assert_eq!(ExportFormat::from_str("M3U8").unwrap(), ExportFormat::M3u8);
        assert!(ExportFormat::from_str("yaml").is_err());
    }

    #[test]
    fn snapshot_serializes_and_roundtrips() {
        let snap = Snapshot {
            metadata: SnapshotMetadata {
                id: "snap_001".to_string(),
                playlist_id: "PL123".to_string(),
                playlist_title: "Test".to_string(),
                reason: "pre-dedupe".to_string(),
                created_at: DateTime::from_timestamp(1_700_000_000, 0).expect("valid ts"),
                item_count: 0,
            },
            playlist: Playlist {
                id: "PL123".to_string(),
                title: "Test".to_string(),
                description: None,
                privacy: PlaylistPrivacy::Private,
                owned: true,
                track_count: 0,
                tracks: Vec::new(),
            },
        };
        let encoded = serde_json::to_string(&snap).expect("encode");
        let decoded: Snapshot = serde_json::from_str(&encoded).expect("decode");
        assert_eq!(decoded, snap);
    }

    #[test]
    fn persisted_v2_models_roundtrip_json() {
        let op = PersistedStagedOp {
            entry_id: 42,
            seq: 1,
            selected: true,
            op_json: r#"{"DeletePlaylist":{"playlist_id":"PL1"}}"#.to_string(),
            created_at: 1_700_000_000,
        };
        let op_back: PersistedStagedOp =
            serde_json::from_str(&serde_json::to_string(&op).unwrap()).unwrap();
        assert_eq!(op_back, op);

        let session = PersistedPlaybackSession {
            session_json: r#"{"current_index":2}"#.to_string(),
            updated_at: 1_700_000_123,
        };
        let session_back: PersistedPlaybackSession =
            serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
        assert_eq!(session_back, session);
    }
}
