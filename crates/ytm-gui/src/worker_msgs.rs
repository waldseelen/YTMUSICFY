use std::path::PathBuf;

use ytm_core::{
    ArtistHarvest, ArtistHarvestOptions, BrowsePage, FetchManifest, LyricsPayload, Playlist,
    PlaylistSummary, SearchFilter, SearchPage, SessionStatus, Track, WatchNextResult,
};
use ytm_enrich::GenreSplitPlan;
use ytm_ops::{
    DedupeConfig, DedupePlan, DiffPlan, HealthReport, KeeperStrategy, MatchMode, SetAlgebraOp,
    SortPlan, SortSpec,
};
use ytm_store::{ExportFormat, Snapshot, SnapshotMetadata};

use crate::auth_store::AudioQuality;
use crate::browser_launch::SessionRefreshState;
use crate::state::{ConfirmedMutation, DispatchEntryOutcome, GuiAppConfig, JobId, MutationPolicy};
use crate::sync_queue::StagedEntry;
use crate::worker_auth::BrowserKind;

/// Commands dispatched from UI thread to Tokio 3-lane background worker.
#[derive(Debug, Clone)]
pub enum WorkerCmd {
    Bootstrap,
    RefreshLibrary,
    ImportAuth(String),
    ImportBrowserAuth(BrowserKind),
    RefreshSessionGuided {
        browser: BrowserKind,
        external_browser: Option<String>,
        video_id: Option<String>,
    },
    ImportClipboardAuth,
    LoadPlaylist(String),
    SearchTracks(String),
    FetchSearchSuggestions {
        req_id: u64,
        query: String,
    },
    SearchInstantLive {
        req_id: u64,
        query: String,
    },
    SearchCatalog {
        query: String,
        filter: SearchFilter,
        continuation: Option<String>,
    },
    BackupPlaylist {
        target: String,
        format: ExportFormat,
        out_dir: PathBuf,
    },
    PlanDedupe {
        playlist_id: String,
        config: DedupeConfig,
    },
    PlanDedupeDraft {
        playlist: Playlist,
        config: DedupeConfig,
    },
    PlanSort {
        playlist_id: String,
        spec: SortSpec,
    },
    PlanSortDraft {
        playlist: Playlist,
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
    SaveStagedOps(Vec<StagedEntry>),
    SavePlaybackSession(String),
    UpdateGuiConfig(GuiAppConfig),
    SetLikeStatus {
        video_id: String,
        liked: bool,
    },
    LoadBrowse {
        browse_id: String,
        params: Option<String>,
    },
    HarvestArtist {
        artist_id: String,
        options: ArtistHarvestOptions,
    },
    LoadBrowseContinuation {
        browse_id: String,
        token: String,
    },
    LoadWatchNext {
        video_id: String,
        playlist_id: Option<String>,
        seed_radio: bool,
    },
    LoadLyrics {
        browse_id: String,
    },
    LoadLyricsPayload {
        video_id: String,
        browse_id: String,
    },
    SetAudioQuality(AudioQuality),
    RecordPlayHistory {
        video_id: String,
        title: String,
        artists_json: String,
        album: Option<String>,
        duration_sec: Option<u32>,
    },
    IndexAllPlaylists {
        missing_only: bool,
    },
    CancelJob(JobId),
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
    SearchSuggestionsLoaded {
        req_id: u64,
        result: Result<Vec<String>, String>,
    },
    InstantLiveLoaded {
        req_id: u64,
        result: Result<SearchPage, String>,
    },
    SessionRefreshProgress(SessionRefreshState),
    LibraryCacheHydrated(Vec<PlaylistSummary>),
    CatalogSearchResults {
        query: String,
        filter: SearchFilter,
        append: bool,
        result: Result<SearchPage, String>,
    },
    JobStarted {
        job_id: JobId,
        label: String,
    },
    JobEnded(JobId),
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
    SyncEntryFinished {
        entry_id: u64,
        outcome: DispatchEntryOutcome,
    },
    LikeStatusResult {
        video_id: String,
        liked: bool,
        result: Result<(), String>,
    },
    BrowseLoaded {
        browse_id: String,
        result: Result<BrowsePage, String>,
    },
    ArtistHarvested(Result<ArtistHarvest, String>),
    BrowseCacheHydrated(BrowsePage),
    BrowseContinuationLoaded {
        browse_id: String,
        result: Result<BrowsePage, String>,
    },
    WatchNextLoaded {
        seed_radio: bool,
        result: Result<WatchNextResult, String>,
    },
    LyricsLoaded(Result<Option<String>, String>),
    LyricsPayloadLoaded {
        video_id: String,
        result: Result<Option<LyricsPayload>, String>,
    },
    AudioConfigUpdated {
        cookie_jar: Option<PathBuf>,
        quality: AudioQuality,
        preferred_browser: Option<BrowserKind>,
    },
    PlaylistCacheHydrated(Vec<Playlist>),
    PlaylistManifestLoaded {
        playlist_id: String,
        manifest: FetchManifest,
    },
    StagedOpsLoaded(Vec<StagedEntry>),
    MutationPolicyLoaded(MutationPolicy),
    PlaybackSessionLoaded(String),
    GuiConfigLoaded(GuiAppConfig),
    SystemThemeChanged(bool),
}

// Keep unused-import-free compilation cleanly if KeeperStrategy isn't directly named here.
pub type _UnusedCheck = Option<KeeperStrategy>;
