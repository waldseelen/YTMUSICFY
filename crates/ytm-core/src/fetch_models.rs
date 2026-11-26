use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{AddedPlaylistItemResult, EditPlaylistResult, Playlist};

/// Completeness classification for a paginated playlist fetch, driving tiered mutation locks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FetchCompleteness {
    #[default]
    Complete,
    PartialHidden {
        declared: usize,
        fetched: usize,
        hidden_estimate: usize,
    },
    Truncated {
        pages_fetched: usize,
        max_pages: usize,
    },
    Degraded {
        dropped_renderers: Vec<String>,
    },
}

impl FetchCompleteness {
    /// Returns `true` when mutations must be hard-locked (`Truncated` or `Degraded`).
    #[must_use]
    pub fn is_mutation_locked(&self) -> bool {
        matches!(self, Self::Truncated { .. } | Self::Degraded { .. })
    }
}

/// Telemetry manifest describing completeness and item counts of a playlist fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchManifest {
    pub declared_track_count: Option<usize>,
    pub fetched_tracks: usize,
    pub available_tracks: usize,
    pub unavailable_tracks: usize,
    pub missing_set_video_id: usize,
    pub pages_fetched: usize,
    pub completeness: FetchCompleteness,
    pub fetched_at: DateTime<Utc>,
}

impl FetchManifest {
    /// Build a [`FetchManifest`] from a hydrated [`Playlist`] and pagination telemetry.
    #[must_use]
    pub fn build(
        playlist: &Playlist,
        declared_track_count: Option<usize>,
        pages_fetched: usize,
        truncated: bool,
        max_pages: usize,
        dropped_renderers: Vec<String>,
        fetched_at: DateTime<Utc>,
    ) -> Self {
        let fetched_tracks = playlist.tracks.len();
        let available_tracks = playlist
            .tracks
            .iter()
            .filter(|t| t.availability.is_available())
            .count();
        let unavailable_tracks = fetched_tracks.saturating_sub(available_tracks);
        let missing_set_video_id = playlist
            .tracks
            .iter()
            .filter(|t| {
                t.set_video_id
                    .as_deref()
                    .is_none_or(|s| s.trim().is_empty())
            })
            .count();

        let completeness = if truncated {
            FetchCompleteness::Truncated {
                pages_fetched,
                max_pages,
            }
        } else if !dropped_renderers.is_empty() {
            FetchCompleteness::Degraded { dropped_renderers }
        } else if let Some(declared) = declared_track_count
            && fetched_tracks < declared
        {
            FetchCompleteness::PartialHidden {
                declared,
                fetched: fetched_tracks,
                hidden_estimate: declared - fetched_tracks,
            }
        } else {
            FetchCompleteness::Complete
        };

        Self {
            declared_track_count,
            fetched_tracks,
            available_tracks,
            unavailable_tracks,
            missing_set_video_id,
            pages_fetched,
            completeness,
            fetched_at,
        }
    }

    /// Synthesize a baseline manifest for legacy/mock `get_playlist` callers.
    #[must_use]
    pub fn from_playlist_complete(playlist: &Playlist) -> Self {
        Self::build(
            playlist,
            Some(playlist.track_count),
            1,
            false,
            1,
            Vec::new(),
            Utc::now(),
        )
    }
}

/// Result of fetching a playlist together with its [`FetchManifest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistFetchResult {
    pub playlist: Playlist,
    pub manifest: FetchManifest,
}

/// Per-chunk outcome recorded while executing a chunked `browse/edit_playlist` batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkOutcome {
    Succeeded {
        chunk_index: usize,
        action_count: usize,
        result: EditPlaylistResult,
    },
    Failed {
        chunk_index: usize,
        action_count: usize,
        error: String,
    },
}

/// Complete report across all attempted chunks in a `browse/edit_playlist` batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationBatchReport {
    pub total_chunks: usize,
    pub outcomes: Vec<ChunkOutcome>,
}

impl MutationBatchReport {
    /// Number of chunks that succeeded (`STATUS_SUCCEEDED`).
    #[must_use]
    pub fn succeeded_chunks(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| matches!(o, ChunkOutcome::Succeeded { .. }))
            .count()
    }

    /// Number of chunks that failed.
    #[must_use]
    pub fn failed_chunks(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| matches!(o, ChunkOutcome::Failed { .. }))
            .count()
    }

    /// Returns `true` when every planned chunk ran and succeeded.
    #[must_use]
    pub fn is_complete_success(&self) -> bool {
        self.outcomes.len() == self.total_chunks && self.failed_chunks() == 0
    }

    /// Collect all `added_items` across succeeded chunks in execution order.
    #[must_use]
    pub fn added_items(&self) -> Vec<AddedPlaylistItemResult> {
        self.outcomes
            .iter()
            .filter_map(|o| match o {
                ChunkOutcome::Succeeded { result, .. } => Some(result.added_items.iter().cloned()),
                ChunkOutcome::Failed { .. } => None,
            })
            .flatten()
            .collect()
    }

    /// First chunk failure message, if any chunk failed.
    #[must_use]
    pub fn first_error(&self) -> Option<&str> {
        self.outcomes.iter().find_map(|o| match o {
            ChunkOutcome::Failed { error, .. } => Some(error.as_str()),
            ChunkOutcome::Succeeded { .. } => None,
        })
    }

    /// Wrap a single legacy `EditPlaylistResult` into a [`MutationBatchReport`].
    #[must_use]
    pub fn from_single_result(action_count: usize, result: EditPlaylistResult) -> Self {
        if action_count == 0 {
            return Self {
                total_chunks: 0,
                outcomes: Vec::new(),
            };
        }
        Self {
            total_chunks: 1,
            outcomes: vec![ChunkOutcome::Succeeded {
                chunk_index: 0,
                action_count,
                result,
            }],
        }
    }

    /// Wrap legacy chunk results into a [`MutationBatchReport`].
    #[must_use]
    pub fn from_chunk_results(total_actions: usize, results: Vec<EditPlaylistResult>) -> Self {
        let total_chunks = results.len();
        let outcomes = results
            .into_iter()
            .enumerate()
            .map(|(chunk_index, result)| ChunkOutcome::Succeeded {
                chunk_index,
                action_count: if total_chunks == 1 { total_actions } else { 0 },
                result,
            })
            .collect();
        Self {
            total_chunks,
            outcomes,
        }
    }
}

#[cfg(test)]
#[path = "fetch_models_tests.rs"]
mod tests;
