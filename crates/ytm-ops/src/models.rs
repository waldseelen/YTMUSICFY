use std::str::FromStr;

use serde::{Deserialize, Serialize};
use ytm_core::{MoveAction, Track};

use crate::error::OpsError;

/// Tie-breaker / keeper preference when selecting which duplicate copy survives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum KeeperStrategy {
    #[default]
    Quality,
    Oldest,
    Newest,
}

impl FromStr for KeeperStrategy {
    type Err = OpsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "quality" | "highest-quality" => Ok(Self::Quality),
            "oldest" => Ok(Self::Oldest),
            "newest" => Ok(Self::Newest),
            other => Err(OpsError::InvalidKeeperStrategy(other.to_string())),
        }
    }
}

/// Configuration parameters controlling exact and fuzzy playlist deduplication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DedupeConfig {
    pub fuzzy_threshold: f64,
    pub duration_tolerance_sec: u32,
    pub ignore_duration: bool,
    pub keep: KeeperStrategy,
    pub exact_only: bool,
}

impl Default for DedupeConfig {
    fn default() -> Self {
        Self {
            fuzzy_threshold: 0.90,
            duration_tolerance_sec: 15,
            ignore_duration: false,
            keep: KeeperStrategy::Quality,
            exact_only: false,
        }
    }
}

/// Group of duplicate tracks collapsing down to one surviving keeper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateCluster {
    pub keeper: Track,
    pub removals: Vec<Track>,
    pub reason: String,
    pub similarity: f64,
}

/// Dry-run or executable deduplication plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DedupePlan {
    pub playlist_id: String,
    pub playlist_title: String,
    pub clusters: Vec<DuplicateCluster>,
    pub total_removals: usize,
    pub skipped_without_set_id: usize,
}

/// Supported sort keys for playlist ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortField {
    Artist,
    Album,
    Title,
    Duration,
    Year,
    Genre,
    Type,
    Shuffle,
}

impl FromStr for SortField {
    type Err = OpsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "artist" => Ok(Self::Artist),
            "album" => Ok(Self::Album),
            "title" => Ok(Self::Title),
            "duration" => Ok(Self::Duration),
            "year" => Ok(Self::Year),
            "genre" => Ok(Self::Genre),
            "type" => Ok(Self::Type),
            "shuffle" => Ok(Self::Shuffle),
            other => Err(OpsError::InvalidSortSpec(other.to_string())),
        }
    }
}

/// Sort direction per key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

impl FromStr for SortDirection {
    type Err = OpsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "asc" => Ok(Self::Asc),
            "desc" => Ok(Self::Desc),
            other => Err(OpsError::InvalidSortSpec(other.to_string())),
        }
    }
}

/// Single `(field, direction)` component inside a multi-key `SortSpec`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SortKey {
    pub field: SortField,
    pub direction: SortDirection,
}

/// Composable multi-key sort specification (e.g., `"artist:asc,year:asc,title:asc"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortSpec {
    pub keys: Vec<SortKey>,
    pub artist_spread: bool,
    pub seed: Option<u64>,
}

impl FromStr for SortSpec {
    type Err = OpsError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(OpsError::InvalidSortSpec(raw.to_string()));
        }

        let mut keys = Vec::new();
        let mut artist_spread = false;

        for token in trimmed.split(',') {
            let token = token.trim();
            if token.is_empty() {
                return Err(OpsError::InvalidSortSpec(raw.to_string()));
            }
            let (field_raw, dir_raw) = match token.split_once(':') {
                Some((f, d)) => (f, Some(d)),
                None => (token, None),
            };
            let field = SortField::from_str(field_raw)?;
            if field == SortField::Shuffle {
                artist_spread = true;
            }
            let direction = match dir_raw {
                Some(d) => SortDirection::from_str(d)?,
                None => SortDirection::Asc,
            };
            keys.push(SortKey { field, direction });
        }

        Ok(Self {
            keys,
            artist_spread,
            seed: None,
        })
    }
}

/// Result produced by the LIS minimal-move sorting planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortPlan {
    pub playlist_id: String,
    pub playlist_title: String,
    pub target_order: Vec<Track>,
    pub anchored_count: usize,
    pub moves: Vec<MoveAction>,
}

/// Supported playlist set operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SetAlgebraOp {
    Diff,
    Intersect,
    Merge,
}

impl FromStr for SetAlgebraOp {
    type Err = OpsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "diff" => Ok(Self::Diff),
            "intersect" => Ok(Self::Intersect),
            "merge" => Ok(Self::Merge),
            other => Err(OpsError::InvalidSetOp(other.to_string())),
        }
    }
}

/// Matching mode used across set algebra operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MatchMode {
    #[default]
    ExactVideoId,
    FuzzyNormalized,
}

/// Computed set algebra preview and optional target playlist insertion list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffPlan {
    pub op: SetAlgebraOp,
    pub match_mode: MatchMode,
    pub source_a_id: String,
    pub source_b_id: String,
    pub result_tracks: Vec<Track>,
    pub target_playlist_id: Option<String>,
    pub add_video_ids: Vec<String>,
}

/// Category of playlist item health defect.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HealthIssueKind {
    Deleted,
    Private,
    RegionBlocked,
    Unplayable(String),
    MissingSetVideoId,
}

/// Individual unhealthy track detected in a playlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthIssue {
    pub track: Track,
    pub kind: HealthIssueKind,
}

/// Candidate replacement upgrading a `UGC` or `OMV` item to an official `ATV` audio track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpgradeCandidate {
    pub source_track: Track,
    pub official_atv: Track,
    pub similarity: f64,
}

/// Full health and official-audio upgrade report for a playlist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    pub playlist_id: String,
    pub playlist_title: String,
    pub total_tracks: usize,
    pub issues: Vec<HealthIssue>,
    pub upgrade_candidates: Vec<UpgradeCandidate>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_spec_parses_multi_key_directions() {
        let spec = SortSpec::from_str("artist:asc,year:desc,title").expect("valid spec");
        assert_eq!(spec.keys.len(), 3);
        assert_eq!(spec.keys[0].field, SortField::Artist);
        assert_eq!(spec.keys[0].direction, SortDirection::Asc);
        assert_eq!(spec.keys[1].field, SortField::Year);
        assert_eq!(spec.keys[1].direction, SortDirection::Desc);
        assert_eq!(spec.keys[2].field, SortField::Title);
        assert_eq!(spec.keys[2].direction, SortDirection::Asc);
        assert!(!spec.artist_spread);
    }

    #[test]
    fn sort_spec_rejects_empty_or_invalid_keys() {
        assert!(SortSpec::from_str("").is_err());
        assert!(SortSpec::from_str("artist:sideways").is_err());
        assert!(SortSpec::from_str("bitrate:asc").is_err());
    }

    #[test]
    fn dedupe_config_defaults_match_spec() {
        let cfg = DedupeConfig::default();
        assert!((cfg.fuzzy_threshold - 0.90).abs() < f64::EPSILON);
        assert_eq!(cfg.duration_tolerance_sec, 15);
        assert!(!cfg.ignore_duration);
        assert_eq!(cfg.keep, KeeperStrategy::Quality);
    }
}
