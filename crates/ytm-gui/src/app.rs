use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use eframe::egui;
use ytm_core::{
    BrowsePage, FetchManifest, LyricsPayload, Playlist, PlaylistSummary, SearchFilter, SearchPage,
    SessionStatus, Track,
};
use ytm_enrich::GenreSplitPlan;
use ytm_ops::{DedupePlan, DiffPlan, HealthReport, SortPlan};
use ytm_store::{AppPaths, ExportFormat, Snapshot, SnapshotMetadata};

use crate::artwork::ArtworkCache;
use crate::browser_launch::SessionRefreshState;
use crate::command_palette::FocusPane;
use crate::draft_cache::DraftCache;
use crate::greetings::GreetingScheduler;
use crate::i18n::{ResolvedLang, resolve_lang};
use crate::lightbox::LightboxState;
use crate::media_controls::OsMediaBridge;
use crate::mini_player::MiniPlayerState;
use crate::navigation::{NavHistory, ToastState, show_status_bar};
use crate::play_queue::PlayQueue;
use crate::playback_ops::{apply_player_bar_outcome, poll_os_media_bridge, poll_track_end_advance};
use crate::player::PlayerController;
use crate::playlist_meta_modal::PlaylistMetaModal;
use crate::right_drawer::RightDrawerTab;
use crate::search_controller::SearchController;
use crate::sidebar;
use crate::state::{
    AsyncState, BrowserKind, DedupeForm, DraftRevision, DragDropMode, DropPrompt, GenreSplitForm,
    GuiAppConfig, GuiTab, HealthForm, InspectSortState, JobId, MutationPolicy, PendingMutation,
    RepertoireTreeState, SetOpsForm, SidebarSearchMode, SortForm, SplitPaneState, WorkerCmd,
    WorkerEvent,
};
use crate::status_bus::StatusBus;
use crate::sync_queue::SyncQueue;
use crate::tab_router::show_central_panel;
use crate::theme::ThemeState;
use crate::top_bar::{TopBarCtx, show_top_bar};
use crate::tray::TrayController;
use crate::views;
use crate::worker::spawn_worker;

#[path = "app_init.rs"]
mod init;

pub struct YtmGuiApp {
    pub(crate) cmd_tx: Sender<WorkerCmd>,
    evt_rx: Receiver<WorkerEvent>,
    pub(crate) active_tab: GuiTab,
    pub(crate) sidebar_filter: String,
    pub(crate) sidebar_search_mode: SidebarSearchMode,
    pub(crate) drag_mode: DragDropMode,
    pub(crate) repertoire_tree: RepertoireTreeState,
    pub(crate) new_playlist_title: String,
    pub(crate) selected_playlist_id: Option<String>,
    pub(crate) session: AsyncState<SessionStatus>,
    pub(crate) library: AsyncState<Vec<PlaylistSummary>>,
    pub(crate) playlist_cache: HashMap<String, Playlist>,
    pub(crate) playlist_manifests: HashMap<String, FetchManifest>,
    pub(crate) draft_cache: DraftCache,
    pub(crate) loading_playlists: HashSet<String>,
    pub(crate) playlist_errors: HashMap<String, String>,
    pub(crate) batch_progress: Option<(usize, usize, String)>,
    pub(crate) active_job_id: Option<JobId>,
    pub(crate) sync_queue: SyncQueue,
    pub(crate) last_persisted_rev: DraftRevision,
    pub(crate) mutation_policy: MutationPolicy,
    pub(crate) gui_config: GuiAppConfig,
    pub(crate) theme_state: ThemeState,
    pub(crate) status_bus: StatusBus,
    pub(crate) greeting_scheduler: GreetingScheduler,
    pub(crate) start_instant: Instant,
    pub(crate) search_controller: SearchController,
    pub(crate) search_bar_rect: egui::Rect,
    pub(crate) session_refresh: SessionRefreshState,
    pub(crate) offline_mode: bool,
    pub(crate) pending_copy_url: Option<String>,
    pub(crate) right_drawer: RightDrawerTab,
    pub(crate) lightbox: LightboxState,
    pub(crate) sync_drawer_open: bool,
    pub(crate) catalog_query: String,
    pub(crate) catalog_filter: SearchFilter,
    pub(crate) catalog_results: AsyncState<SearchPage>,
    pub(crate) lyrics_payload: AsyncState<Option<LyricsPayload>>,
    pub(crate) lyrics_video_id: Option<String>,
    pub(crate) palette_open: bool,
    pub(crate) palette_query: String,
    pub(crate) split_state: SplitPaneState,
    pub(crate) inspect_sort: InspectSortState,
    pub(crate) inspect_selected: HashSet<usize>,
    pub(crate) inspect_search: String,
    pub(crate) ytm_query: String,
    pub(crate) search_results: AsyncState<Vec<Track>>,
    pub(crate) dedupe_form: DedupeForm,
    pub(crate) dedupe_plan: AsyncState<DedupePlan>,
    pub(crate) sort_form: SortForm,
    pub(crate) sort_plan: AsyncState<SortPlan>,
    pub(crate) set_form: SetOpsForm,
    pub(crate) set_plan: AsyncState<DiffPlan>,
    pub(crate) health_form: HealthForm,
    pub(crate) health_report: AsyncState<HealthReport>,
    pub(crate) genre_form: GenreSplitForm,
    pub(crate) genre_plan: AsyncState<(Playlist, GenreSplitPlan)>,
    pub(crate) snapshots: AsyncState<Vec<SnapshotMetadata>>,
    pub(crate) snapshot_detail: AsyncState<Snapshot>,
    pub(crate) export_dir: String,
    pub(crate) export_fmt: ExportFormat,
    pub(crate) auth_raw_input: String,
    pub(crate) auth_browser: BrowserKind,
    pub(crate) audio_quality: crate::auth_store::AudioQuality,
    pub(crate) cookie_bridge_active: bool,
    pub(crate) pending_modal: Option<PendingMutation>,
    pub(crate) drop_prompt: Option<DropPrompt>,
    pub(crate) toast: Option<ToastState>,
    pub(crate) nav_history: NavHistory,
    pub(crate) ui_scale: f32,
    pub(crate) base_pixels_per_point: Option<f32>,
    pub(crate) focus_search_once: bool,
    pub(crate) inspect_last_clicked: Option<usize>,
    pub(crate) inspect_focused_row: Option<usize>,
    pub(crate) inspect_jump_input: String,
    pub(crate) sidebar_focused_idx: Option<usize>,
    pub(crate) play_queue: PlayQueue,
    pub(crate) player: PlayerController,
    pub(crate) audio_effects: crate::audio_fx::AudioEffects,
    pub(crate) media_bridge: OsMediaBridge,
    pub(crate) focus_pane: FocusPane,
    pub(crate) muted: bool,
    pub(crate) prev_volume: f64,
    pub(crate) last_prefetched_video_id: Option<String>,
    pub(crate) last_ended_counter: u64,
    pub(crate) browse_target: String,
    pub(crate) browse_state: AsyncState<BrowsePage>,
    pub(crate) browse_from_cache: bool,
    pub(crate) lyrics_browse_id: Option<String>,
    pub(crate) lyrics_state: AsyncState<Option<String>>,
    pub(crate) artwork: ArtworkCache,
    pub(crate) meta_modal: Option<PlaylistMetaModal>,
    pub(crate) artist_harvest_modal: Option<crate::views::artist_harvest::ArtistHarvestModal>,
    pub(crate) tray_ctrl: TrayController,
    pub(crate) mini_player: MiniPlayerState,
    pub(crate) main_window_visible: bool,
    pub(crate) explicit_quit: bool,
    pub(crate) egui_ctx: Option<egui::Context>,
}

impl YtmGuiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (cmd_tx, evt_rx) = spawn_worker(cc.egui_ctx.clone());
        let default_export = AppPaths::from_project_dirs()
            .map(|p| p.data_dir().join("exports").display().to_string())
            .unwrap_or_else(|_| "./backups".to_string());
        #[cfg(feature = "screenshot-harness")]
        let skip_bootstrap = std::env::var_os("DALGA_PREVIEW_SCENE").is_some();
        #[cfg(not(feature = "screenshot-harness"))]
        let skip_bootstrap = false;
        if !skip_bootstrap {
            let _ = cmd_tx.send(WorkerCmd::Bootstrap);
        }
        let mut app =
            Self::init_with_channels(cmd_tx, evt_rx, default_export, Some(cc.egui_ctx.clone()));
        app.media_bridge = OsMediaBridge::from_creation_context(cc);
        app.tray_ctrl = TrayController::new(app.gui_config.tray_enabled, Some(cc.egui_ctx.clone()));
        app
    }

    #[must_use]
    pub(crate) fn lang(&self) -> ResolvedLang {
        resolve_lang(self.gui_config.ui_lang)
    }

    #[must_use]
    pub(crate) fn now_ms(&self) -> u64 {
        self.start_instant.elapsed().as_millis() as u64
    }

    pub(crate) fn mark_staged_persisted(&mut self) {
        self.last_persisted_rev = self.sync_queue.revision();
    }

    pub(crate) fn drain_events(&mut self) {
        while let Ok(evt) = self.evt_rx.try_recv() {
            self.apply_event(evt);
        }
    }

    pub(crate) fn apply_event(&mut self, evt: WorkerEvent) {
        crate::app_events::apply_worker_event(self, evt);
    }
}

impl eframe::App for YtmGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        poll_track_end_advance(self);

        let current_art_url = self
            .play_queue
            .current_track()
            .and_then(|t| t.effective_thumbnail_url());
        let current_art = current_art_url
            .as_deref()
            .and_then(|url| self.artwork.texture_for(ctx, url).cloned());
        let navigated_via_history =
            crate::app_frame::sync_theme_and_input(ctx, self, current_art_url.as_deref());
        let lang = self.lang();
        poll_os_media_bridge(self);

        let now_ms = self.now_ms();
        let palette = self.theme_state.palette;
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            self.search_bar_rect = show_top_bar(
                ui,
                TopBarCtx {
                    lang,
                    palette: &palette,
                    session: &self.session,
                    auth_browser: self.auth_browser,
                    active_tab: &mut self.active_tab,
                    selected_playlist_id: &mut self.selected_playlist_id,
                    nav_history: &mut self.nav_history,
                    ui_scale: &mut self.ui_scale,
                    gui_config: &mut self.gui_config,
                    search_controller: &mut self.search_controller,
                    catalog_query: &mut self.catalog_query,
                    catalog_filter: self.catalog_filter,
                    focus_search_once: self.focus_search_once,
                    status_bus: &mut self.status_bus,
                    sync_queue: &mut self.sync_queue,
                    sync_drawer_open: &mut self.sync_drawer_open,
                    mini_player_open: &mut self.mini_player.open,
                    start_instant: self.start_instant,
                    now_ms,
                    cmd_tx: &self.cmd_tx,
                },
            );
        });

        crate::app_frame::show_session_offline_banner(ctx, self, &palette, lang);

        if self.batch_progress.is_some() {
            egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
                show_status_bar(
                    ui,
                    self.batch_progress.as_ref(),
                    self.active_job_id,
                    &self.cmd_tx,
                );
            });
        }

        self.focus_pane = if self.pending_modal.is_some()
            || self.meta_modal.is_some()
            || self.artist_harvest_modal.is_some()
            || self.palette_open
            || self.lightbox.is_open()
        {
            FocusPane::Modal
        } else if self.sync_drawer_open || self.right_drawer.is_open() {
            FocusPane::Drawer
        } else {
            FocusPane::MainTable
        };
        let playlists_for_bar = self.library.data().map(Vec::as_slice).unwrap_or(&[]);
        let mut bar_out = None;
        egui::TopBottomPanel::bottom("player_bar").show(ctx, |ui| {
            bar_out = Some(views::player_bar::show(
                ui,
                views::player_bar::PlayerBarCtx {
                    lang,
                    queue: &self.play_queue,
                    player: &self.player,
                    playlists: playlists_for_bar,
                    favourite_playlist_id: self.gui_config.favourite_playlist_id.as_deref(),
                    muted: self.muted,
                    right_drawer: self.right_drawer,
                    art_tex: current_art.as_ref(),
                    current_effects: &mut self.audio_effects,
                    sync_queue: &mut self.sync_queue,
                    toast: &mut self.toast,
                },
            ));
        });
        if let Some(out) = bar_out {
            apply_player_bar_outcome(out, self);
        }

        let prev_selected = self.selected_playlist_id.clone();
        let focus_sidebar = self.focus_search_once
            && (self.active_tab != GuiTab::Inspector || self.selected_playlist_id.is_none());
        egui::SidePanel::left("library_sidebar_v2")
            .default_width(250.0)
            .min_width(220.0)
            .max_width(360.0)
            .show(ctx, |ui| {
                sidebar::show(
                    ui,
                    lang,
                    &self.library,
                    &mut self.sidebar_filter,
                    &mut self.sidebar_search_mode,
                    self.drag_mode,
                    &mut self.new_playlist_title,
                    &mut self.selected_playlist_id,
                    &mut self.loading_playlists,
                    &mut self.playlist_cache,
                    &mut self.draft_cache,
                    &mut self.sync_queue,
                    &mut self.split_state,
                    &mut self.set_form,
                    &mut self.active_tab,
                    &mut self.sync_drawer_open,
                    &mut self.drop_prompt,
                    &mut self.meta_modal,
                    &mut self.pending_modal,
                    &mut self.toast,
                    focus_sidebar,
                    &mut self.sidebar_focused_idx,
                    &self.cmd_tx,
                );
            });
        if prev_selected != self.selected_playlist_id {
            self.inspect_selected.clear();
            self.inspect_last_clicked = None;
            self.inspect_focused_row = None;
            self.inspect_sort = InspectSortState::default();
            self.dedupe_plan = AsyncState::Idle;
            self.sort_plan = AsyncState::Idle;
            self.health_report = AsyncState::Idle;
            self.genre_plan = AsyncState::Idle;
        }

        if self.sync_drawer_open {
            egui::SidePanel::right("sync_staging_drawer")
                .default_width(360.0)
                .min_width(280.0)
                .max_width(520.0)
                .show(ctx, |ui| {
                    views::sync::show(
                        ui,
                        &mut self.sync_queue,
                        &mut self.pending_modal,
                        &mut self.toast,
                    );
                });
        } else {
            crate::right_drawer::show_right_drawer(ctx, self, lang);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            show_central_panel(ui, self);
        });
        crate::app_events::finish_frame(self, ctx, navigated_via_history);
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
