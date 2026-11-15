use std::path::PathBuf;

use eframe::egui;
use serde::{Deserialize, Serialize};
use ytm_core::{ClientConfig, Playlist, PlaylistSummary, SessionStatus, Track};
use ytm_enrich::{EnrichmentConfig, GenreSplitPlan};
use ytm_ops::{
    DedupeConfig, DedupePlan, DiffPlan, HealthReport, KeeperStrategy, MatchMode, SetAlgebraOp,
    SortDirection, SortField, SortKey, SortPlan, SortSpec,
};
use ytm_store::{ExportFormat, Snapshot, SnapshotMetadata};

use crate::sync_queue::StagedEntry;

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
    Inspector,
    Dedupe,
    Sort,
    SetOps,
    Health,
    GenreSplit,
    Snapshots,
    Sync,
    Auth,
}

impl GuiTab {
    pub const ALL: [Self; 9] = [
        Self::Inspector,
        Self::Dedupe,
        Self::Sort,
        Self::SetOps,
        Self::Health,
        Self::GenreSplit,
        Self::Snapshots,
        Self::Sync,
        Self::Auth,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Inspector => "Inspector",
            Self::Dedupe => "Dedupe",
            Self::Sort => "Sort (LIS)",
            Self::SetOps => "Set Algebra",
            Self::Health => "Health",
            Self::GenreSplit => "Genre Split",
            Self::Snapshots => "Snapshots",
            Self::Sync => "Sync",
            Self::Auth => "Auth",
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
}

/// Confirmed immediate actions gated behind modal confirmation.
#[derive(Debug, Clone)]
pub enum ConfirmedMutation {
    RestoreSnapshot { snapshot_id: String },
    ExecuteSync { entries: Vec<StagedEntry> },
}

/// Confirmation dialog payload displayed before any remote InnerTube mutation.
#[derive(Debug, Clone)]
pub struct PendingMutation {
    pub title: String,
    pub summary: String,
    pub action: ConfirmedMutation,
}

/// Form parameters for Dedupe view.
#[derive(Debug, Clone)]
pub struct DedupeForm {
    pub threshold: f64,
    pub duration_tol: u32,
    pub ignore_duration: bool,
    pub exact_only: bool,
    pub keep: KeeperStrategy,
}

impl Default for DedupeForm {
    fn default() -> Self {
        let d = DedupeConfig::default();
        Self {
            threshold: d.fuzzy_threshold,
            duration_tol: d.duration_tolerance_sec,
            ignore_duration: d.ignore_duration,
            exact_only: d.exact_only,
            keep: d.keep,
        }
    }
}

/// Form parameters for LIS minimal-move Sort view.
#[derive(Debug, Clone)]
pub struct SortForm {
    pub keys: Vec<SortKey>,
    pub artist_spread: bool,
    pub use_seed: bool,
    pub seed: u64,
}

impl Default for SortForm {
    fn default() -> Self {
        Self {
            keys: vec![
                SortKey {
                    field: SortField::Artist,
                    direction: SortDirection::Asc,
                },
                SortKey {
                    field: SortField::Title,
                    direction: SortDirection::Asc,
                },
            ],
            artist_spread: false,
            use_seed: true,
            seed: 42,
        }
    }
}

impl SortForm {
    #[must_use]
    pub fn to_spec(&self) -> SortSpec {
        let has_shuffle = self.keys.iter().any(|k| k.field == SortField::Shuffle);
        SortSpec {
            keys: self.keys.clone(),
            artist_spread: self.artist_spread || has_shuffle,
            seed: if has_shuffle && self.use_seed {
                Some(self.seed)
            } else {
                None
            },
        }
    }
}

/// Form parameters for Set Algebra view.
#[derive(Debug, Clone)]
pub struct SetOpsForm {
    pub op: SetAlgebraOp,
    pub playlist_a: String,
    pub playlist_b: String,
    pub fuzzy: bool,
    pub target_playlist: String,
}

impl Default for SetOpsForm {
    fn default() -> Self {
        Self {
            op: SetAlgebraOp::Diff,
            playlist_a: "LM".to_string(),
            playlist_b: String::new(),
            fuzzy: true,
            target_playlist: String::new(),
        }
    }
}

/// Form parameters for Health Scanner view.
#[derive(Debug, Clone)]
pub struct HealthForm {
    pub find_official: bool,
    pub prune_unavailable: bool,
    pub upgrade: bool,
}

impl Default for HealthForm {
    fn default() -> Self {
        Self {
            find_official: true,
            prune_unavailable: true,
            upgrade: true,
        }
    }
}

/// Form parameters for Genre Split view.
#[derive(Debug, Clone)]
pub struct GenreSplitForm {
    pub min_tracks: usize,
    pub custom_genres_toml: String,
}

impl Default for GenreSplitForm {
    fn default() -> Self {
        Self {
            min_tracks: 5,
            custom_genres_toml: String::new(),
        }
    }
}

pub use crate::worker_auth::BrowserKind;

/// Commands dispatched from UI thread to Tokio background worker.
#[derive(Debug, Clone)]
pub enum WorkerCmd {
    Bootstrap,
    RefreshLibrary,
    ImportAuth(String),
    ImportBrowserAuth(BrowserKind),
    LoadPlaylist(String),
    SearchTracks(String),
    BackupPlaylist {
        target: String,
        format: ExportFormat,
        out_dir: PathBuf,
    },
    PlanDedupe {
        playlist_id: String,
        config: DedupeConfig,
    },
    PlanSort {
        playlist_id: String,
        spec: SortSpec,
    },
    PlanSetOp {
        op: SetAlgebraOp,
        mode: MatchMode,
        playlist_a: String,
        playlist_b: String,
        target: Option<String>,
    },
    ScanHealth {
        playlist_id: String,
        find_official: bool,
    },
    PlanGenreSplit {
        playlist_id: String,
        min_tracks: usize,
        custom_genres_path: Option<PathBuf>,
    },
    RefreshSnapshots,
    LoadSnapshot(String),
    ExecuteMutation(ConfirmedMutation),
}

/// Events emitted from Tokio background worker back to UI thread.
#[derive(Debug, Clone)]
pub enum WorkerEvent {
    SessionLoaded(Result<SessionStatus, String>),
    LibraryLoaded(Result<Vec<PlaylistSummary>, String>),
    PlaylistLoaded {
        playlist_id: String,
        result: Result<Playlist, String>,
    },
    SearchResults(Result<Vec<Track>, String>),
    Progress {
        done: usize,
        total: usize,
        message: String,
    },
    DedupePlanned(Result<DedupePlan, String>),
    SortPlanned(Result<SortPlan, String>),
    SetOpPlanned(Result<DiffPlan, String>),
    HealthScanned(Result<HealthReport, String>),
    GenreSplitPlanned(Result<(Playlist, GenreSplitPlan), String>),
    SnapshotsLoaded(Result<Vec<SnapshotMetadata>, String>),
    SnapshotDetailLoaded(Result<Snapshot, String>),
    StatusToast {
        ok: bool,
        message: String,
    },
    MutationFinished {
        ok: bool,
        message: String,
        reload_ids: Vec<String>,
    },
}
