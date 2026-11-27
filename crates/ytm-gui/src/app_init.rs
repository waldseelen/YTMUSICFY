use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use eframe::egui;

use crate::app::YtmGuiApp;
use crate::artwork::ArtworkCache;
use crate::browser_launch::SessionRefreshState;
use crate::command_palette::FocusPane;
use crate::draft_cache::DraftCache;
use crate::greetings::GreetingScheduler;
use crate::lightbox::LightboxState;
use crate::media_controls::OsMediaBridge;
use crate::navigation::NavHistory;
use crate::play_queue::PlayQueue;
use crate::player::PlayerController;
use crate::right_drawer::RightDrawerTab;
use crate::search_controller::SearchController;
use crate::state::{
    AsyncState, BrowserKind, DedupeForm, DraftRevision, DragDropMode, GenreSplitForm, GuiAppConfig,
    GuiTab, HealthForm, InspectSortState, MutationPolicy, RepertoireTreeState, SetOpsForm,
    SidebarSearchMode, SortForm, SplitPaneState, WorkerCmd, WorkerEvent,
};
use crate::status_bus::StatusBus;
use crate::sync_queue::SyncQueue;
use crate::theme::ThemeState;

impl YtmGuiApp {
    pub(crate) fn init_with_channels(
        cmd_tx: Sender<WorkerCmd>,
        evt_rx: Receiver<WorkerEvent>,
        default_export: String,
        egui_ctx: Option<egui::Context>,
    ) -> Self {
        Self {
            cmd_tx,
            evt_rx,
            active_tab: GuiTab::Inspector,
            sidebar_filter: String::new(),
            sidebar_search_mode: SidebarSearchMode::default(),
            drag_mode: DragDropMode::default(),
            repertoire_tree: RepertoireTreeState::default(),
            new_playlist_title: String::new(),
            selected_playlist_id: None,
            session: AsyncState::Loading("Verifying InnerTube session...".to_string()),
            library: AsyncState::Loading("Loading library playlists...".to_string()),
            playlist_cache: HashMap::new(),
            playlist_manifests: HashMap::new(),
            draft_cache: DraftCache::default(),
            loading_playlists: HashSet::new(),
            playlist_errors: HashMap::new(),
            sync_queue: SyncQueue::default(),
            last_persisted_rev: DraftRevision::default(),
            mutation_policy: MutationPolicy::default(),
            gui_config: GuiAppConfig::default(),
            theme_state: ThemeState::default(),
            status_bus: StatusBus::new(),
            greeting_scheduler: GreetingScheduler::default(),
            start_instant: Instant::now(),
            search_controller: SearchController::default(),
            search_bar_rect: egui::Rect::NOTHING,
            session_refresh: SessionRefreshState::Idle,
            offline_mode: false,
            pending_copy_url: None,
            right_drawer: RightDrawerTab::Closed,
            lightbox: LightboxState::Closed,
            sync_drawer_open: false,
            catalog_query: String::new(),
            catalog_filter: ytm_core::SearchFilter::Songs,
            catalog_results: AsyncState::Idle,
            lyrics_payload: AsyncState::Idle,
            lyrics_video_id: None,
            palette_open: false,
            palette_query: String::new(),
            active_job_id: None,
            split_state: SplitPaneState::default(),
            inspect_sort: InspectSortState::default(),
            inspect_selected: HashSet::new(),
            inspect_search: String::new(),
            ytm_query: String::new(),
            search_results: AsyncState::Idle,
            dedupe_form: DedupeForm::default(),
            dedupe_plan: AsyncState::Idle,
            sort_form: SortForm::default(),
            sort_plan: AsyncState::Idle,
            set_form: SetOpsForm::default(),
            set_plan: AsyncState::Idle,
            health_form: HealthForm::default(),
            health_report: AsyncState::Idle,
            genre_form: GenreSplitForm::default(),
            genre_plan: AsyncState::Idle,
            snapshots: AsyncState::Idle,
            snapshot_detail: AsyncState::Idle,
            export_dir: default_export,
            export_fmt: ytm_store::ExportFormat::Json,
            auth_raw_input: String::new(),
            auth_browser: BrowserKind::Brave,
            audio_quality: crate::auth_store::AudioQuality::default(),
            cookie_bridge_active: false,
            pending_modal: None,
            drop_prompt: None,
            batch_progress: None,
            toast: None,
            nav_history: NavHistory::new(GuiTab::Inspector, None),
            ui_scale: 1.0,
            base_pixels_per_point: None,
            focus_search_once: false,
            inspect_last_clicked: None,
            inspect_focused_row: None,
            inspect_jump_input: String::new(),
            sidebar_focused_idx: None,
            play_queue: PlayQueue::default(),
            player: PlayerController::new(egui_ctx.clone()),
            media_bridge: OsMediaBridge::new(),
            focus_pane: FocusPane::default(),
            muted: false,
            prev_volume: 100.0,
            last_prefetched_video_id: None,
            last_ended_counter: 0,
            browse_target: "FEmusic_home".to_string(),
            browse_state: AsyncState::Idle,
            lyrics_browse_id: None,
            lyrics_state: AsyncState::Idle,
            artwork: ArtworkCache::new(egui_ctx),
            meta_modal: None,
        }
    }
}
