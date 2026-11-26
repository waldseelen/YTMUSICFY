use eframe::egui;
use serde::{Deserialize, Serialize};
use ytm_core::{ClientConfig, SessionStatus, Track};
use ytm_enrich::EnrichmentConfig;

use crate::auth_store::AudioQuality;
use crate::sync_queue::StagedEntry;

#[path = "state_forms.rs"]
mod forms;
pub use forms::*;

/// Governance mode controlling whether `SetLikeStatus` stages in `SyncQueue` or executes immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MutationPolicy {
    #[default]
    StrictLock,
    QuickLike,
}

/// Explicit authentication finite-state machine replacing parallel boolean session flags.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SessionState {
    #[default]
    Unauthenticated,
    Verifying,
    Authenticated(SessionStatus),
    Expired {
        reason: String,
        last_known: Option<SessionStatus>,
    },
}

/// Monotonic counter incremented on every `SyncQueue` edit to key memoized drafts.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct DraftRevision(pub u64);

impl DraftRevision {
    /// Increments the draft revision in place and returns the new revision.
    pub fn bump(&mut self) -> Self {
        self.0 += 1;
        *self
    }
}

/// Deterministic fingerprint locking tool previews to their source playlist, draft, and form state.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlanFingerprint {
    pub playlist_id: String,
    pub draft_revision: DraftRevision,
    pub form_hash: u64,
}

/// Identifier for a cancellable background worker job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct JobId(pub u64);

/// Per-entry verification result emitted during `SyncQueue` dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchEntryOutcome {
    Succeeded {
        entry_id: u64,
        chunks_sent: usize,
        confirmed_items: usize,
        readback_verified: bool,
    },
    AbortedDrift {
        entry_id: u64,
        reason: String,
    },
    Failed {
        entry_id: u64,
        completed_chunks: usize,
        total_chunks: usize,
        error: String,
    },
}

/// Explicit state machine preventing invalid parallel boolean flags.
#[derive(Debug, Clone, Default)]
pub enum AsyncState<T> {
    #[default]
    Idle,
    Loading(String),
    Ready(T),
    Failed(String),
}

impl<T> AsyncState<T> {
    #[must_use]
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading(_))
    }

    #[must_use]
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Ready(val) => Some(val),
            _ => None,
        }
    }
}

/// Top-level workspace tabs in `ytm-gui`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiTab {
    Browse,
    Search,
    QueueLyrics,
    Inspector,
    Dedupe,
    Sort,
    SetOps,
    Health,
    GenreSplit,
    Snapshots,
    Sync,
    Auth,
    Settings,
}

impl GuiTab {
    pub const ALL: [Self; 13] = [
        Self::Browse,
        Self::Search,
        Self::QueueLyrics,
        Self::Inspector,
        Self::Dedupe,
        Self::Sort,
        Self::SetOps,
        Self::Health,
        Self::GenreSplit,
        Self::Snapshots,
        Self::Sync,
        Self::Auth,
        Self::Settings,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Browse => "Home & Explore",
            Self::Search => "Search",
            Self::QueueLyrics => "Queue & Lyrics",
            Self::Inspector => "Inspector",
            Self::Dedupe => "Dedupe",
            Self::Sort => "Sort (LIS)",
            Self::SetOps => "Set Algebra",
            Self::Health => "Health",
            Self::GenreSplit => "Genre Split",
            Self::Snapshots => "Snapshots",
            Self::Sync => "Sync",
            Self::Auth => "Auth",
            Self::Settings => "Settings",
        }
    }
}

/// Sortable columns inside the Inspector track table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectColumn {
    Position,
    Title,
    Artist,
    Album,
    Duration,
    Type,
    Status,
}

/// View-only table sort state in Inspector (plus optional one-click LIS staging).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectSortState {
    pub column: InspectColumn,
    pub asc: bool,
}

impl Default for InspectSortState {
    fn default() -> Self {
        Self {
            column: InspectColumn::Position,
            asc: true,
        }
    }
}

/// Diff category filter inside Split View.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffFilter {
    #[default]
    All,
    OnlyA,
    OnlyB,
    InBoth,
}

/// Side-by-side Split Pane & live Diff configuration.
#[derive(Debug, Clone)]
pub struct SplitPaneState {
    pub enabled: bool,
    pub playlist_b_id: Option<String>,
    pub search_a: String,
    pub search_b: String,
    pub fuzzy: bool,
    pub filter: DiffFilter,
    pub split_ratio: f32,
    pub selected_a: std::collections::HashSet<usize>,
    pub selected_b: std::collections::HashSet<usize>,
}

impl Default for SplitPaneState {
    fn default() -> Self {
        Self {
            enabled: false,
            playlist_b_id: None,
            search_a: String::new(),
            search_b: String::new(),
            fuzzy: false,
            filter: DiffFilter::All,
            split_ratio: 0.5,
            selected_a: std::collections::HashSet::new(),
            selected_b: std::collections::HashSet::new(),
        }
    }
}

/// Payload carried when dragging single or multiple tracks across UI targets.
#[derive(Debug, Clone)]
pub struct DragTrackPayload {
    pub source_playlist_id: String,
    pub tracks: Vec<Track>,
}

/// Kubuntu/Dolphin-style popup menu shown when dropping tracks onto another playlist.
#[derive(Debug, Clone)]
pub struct DropPrompt {
    pub pos: egui::Pos2,
    pub source_playlist_id: String,
    pub source_playlist_title: String,
    pub target_playlist_id: String,
    pub target_playlist_title: String,
    pub tracks: Vec<Track>,
}

/// Persisted TOML application settings loaded from `config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiAppConfig {
    pub innertube: ClientConfig,
    pub enrichment: EnrichmentConfig,
    pub preferred_browser: Option<BrowserKind>,
    pub audio_quality: AudioQuality,
    #[serde(default)]
    pub mutation_policy: MutationPolicy,
    #[serde(default)]
    pub auto_refresh_browser_cookies: bool,
}

/// Confirmed immediate actions gated behind modal confirmation.
#[derive(Debug, Clone)]
pub enum ConfirmedMutation {
    RestoreSnapshot {
        snapshot_id: String,
    },
    ExecuteSync {
        entries: Vec<StagedEntry>,
    },
    DeletePlaylist {
        playlist_id: String,
        playlist_title: String,
    },
}

/// Confirmation dialog payload displayed before any remote InnerTube mutation.
#[derive(Debug, Clone)]
pub struct PendingMutation {
    pub title: String,
    pub summary: String,
    pub action: ConfirmedMutation,
}

pub use crate::worker_auth::BrowserKind;
pub use crate::worker_msgs::{WorkerCmd, WorkerEvent};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_app_config_defaults_missing_governance_fields() {
        let cfg: GuiAppConfig = toml::from_str("").expect("deserialize empty config");
        assert_eq!(cfg.mutation_policy, MutationPolicy::StrictLock);
        assert!(!cfg.auto_refresh_browser_cookies);
    }

    #[test]
    fn draft_revision_bump_and_session_state_default() {
        let mut rev = DraftRevision::default();
        assert_eq!(rev, DraftRevision(0));
        assert_eq!(rev.bump(), DraftRevision(1));
        assert_eq!(rev.bump(), DraftRevision(2));
        assert_eq!(SessionState::default(), SessionState::Unauthenticated);
    }
}
