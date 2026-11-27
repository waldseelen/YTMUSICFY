use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ytm_core::Track;

/// Origin of enriched metadata tags and release attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EnrichmentSource {
    Cache,
    MusicBrainz,
    LastFm,
    Deezer,
    ListenBrainz,
    Unknown,
}

impl EnrichmentSource {
    /// Stable lowercase identifier persisted to SQLite.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::MusicBrainz => "musicbrainz",
            Self::LastFm => "lastfm",
            Self::Deezer => "deezer",
            Self::ListenBrainz => "listenbrainz",
            Self::Unknown => "unknown",
        }
    }
}

/// Resolved external metadata for a track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnrichedTrackMetadata {
    pub cache_key: String,
    pub raw_tags: Vec<String>,
    pub primary_genre: String,
    pub release_year: Option<u16>,
    pub bpm: Option<f32>,
    pub source: EnrichmentSource,
}

/// Single parent-genre rule mapping multiple subgenre patterns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenreRule {
    pub parent: String,
    pub patterns: Vec<String>,
}

/// Deserialized `genres.toml` taxonomy rule set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenreTaxonomyConfig {
    pub default_genre: String,
    pub min_bucket_tracks: usize,
    #[serde(default)]
    pub rules: Vec<GenreRule>,
}

/// Target genre partition produced by the `split-genre` planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenreBucket {
    pub genre: String,
    pub target_playlist_title: String,
    pub existing_playlist_id: Option<String>,
    pub tracks_to_add: Vec<Track>,
    pub already_present_count: usize,
}

/// Diagnostic genre assignment record captured for each enriched track in a split plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackGenreInfo {
    pub primary_genre: String,
    pub raw_tags: Vec<String>,
    pub source: EnrichmentSource,
    pub release_year: Option<u16>,
}

/// Complete dry-run or execution plan for `ytm split-genre`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenreSplitPlan {
    pub source_playlist_id: String,
    pub source_playlist_title: String,
    pub buckets: Vec<GenreBucket>,
    pub skipped_small_buckets: usize,
    #[serde(default)]
    pub skipped_buckets: Vec<GenreBucket>,
    #[serde(default)]
    pub track_details: HashMap<String, TrackGenreInfo>,
}

/// Runtime settings controlling external metadata enrichment providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrichmentConfig {
    pub cache_ttl_days: u64,
    pub musicbrainz_user_agent: String,
    pub musicbrainz_interval_ms: u64,
    pub enable_deezer_fallback: bool,
    pub enable_listenbrainz_fallback: bool,
    pub lastfm_api_key: Option<String>,
    #[serde(default)]
    pub artist_cache_first: bool,
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            cache_ttl_days: 30,
            musicbrainz_user_agent:
                "ytm-architect/0.1.0 (https://github.com/ytm-architect/ytm-architect)".to_string(),
            musicbrainz_interval_ms: 1100,
            enable_deezer_fallback: true,
            enable_listenbrainz_fallback: false,
            lastfm_api_key: None,
            artist_cache_first: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genres_example_toml_deserializes_cleanly() {
        let raw = include_str!("../../../config/genres.example.toml");
        let cfg: GenreTaxonomyConfig = toml::from_str(raw).expect("valid genres.example.toml");
        assert_eq!(cfg.default_genre, "Other");
        assert_eq!(cfg.min_bucket_tracks, 5);
        assert!(cfg.rules.iter().any(|r| r.parent == "Rock"));
        assert!(cfg.rules.iter().any(|r| r.parent == "Electronic"));
    }

    #[test]
    fn enrichment_config_defaults_artist_cache_first_to_false() {
        let cfg = EnrichmentConfig::default();
        assert!(!cfg.artist_cache_first);

        let legacy_json = serde_json::json!({
            "cache_ttl_days": 30,
            "musicbrainz_user_agent": "ua",
            "musicbrainz_interval_ms": 1100,
            "enable_deezer_fallback": true,
            "enable_listenbrainz_fallback": false,
            "lastfm_api_key": null
        });
        let decoded: EnrichmentConfig =
            serde_json::from_value(legacy_json).expect("deserialize without artist_cache_first");
        assert!(!decoded.artist_cache_first);
    }
}
