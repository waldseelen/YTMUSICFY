use std::path::PathBuf;

use ytm_core::{
    BrowsePage, Playlist, PlaylistPrivacy, PlaylistSummary, SessionStatus, Track, WatchNextResult,
};
use ytm_enrich::GenreSplitPlan;
use ytm_ops::{
    DedupeConfig, DedupePlan, DiffPlan, HealthReport, KeeperStrategy, MatchMode, SetAlgebraOp,
    SortPlan, SortSpec,
};
use ytm_store::{ExportFormat, Snapshot, SnapshotMetadata};

use crate::state::ConfirmedMutation;
use crate::worker_auth::BrowserKind;

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
    UpdatePlaylistMeta {
        playlist_id: String,
        title: Option<String>,
        description: Option<String>,
        privacy: Option<PlaylistPrivacy>,
    },
    SetLikeStatus {
        video_id: String,
        liked: bool,
    },
    LoadBrowse {
        browse_id: String,
    },
    LoadWatchNext {
        video_id: String,
        playlist_id: Option<String>,
        seed_radio: bool,
    },
    LoadLyrics {
        browse_id: String,
    },
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
    BrowseLoaded {
        browse_id: String,
        result: Result<BrowsePage, String>,
    },
    WatchNextLoaded {
        seed_radio: bool,
        result: Result<WatchNextResult, String>,
    },
    LyricsLoaded(Result<Option<String>, String>),
}

// Keep unused-import-free compilation cleanly if KeeperStrategy isn't directly named here.
pub type _UnusedCheck = Option<KeeperStrategy>;
