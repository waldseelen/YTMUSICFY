use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, SessionStatus, Track};
use ytm_enrich::GenreSplitPlan;
use ytm_ops::{DedupePlan, DiffPlan, HealthReport, SortPlan};
use ytm_store::{AppPaths, ExportFormat, Snapshot, SnapshotMetadata};

use crate::navigation::{NavHistory, ToastState, apply_global_input, show_status_bar};
use crate::overlays::{show_confirmation_modal, show_drop_prompt, show_top_bar};
use crate::sidebar;
use crate::state::{
    AsyncState, BrowserKind, DedupeForm, DropPrompt, GenreSplitForm, GuiTab, HealthForm,
    InspectSortState, PendingMutation, SetOpsForm, SortForm, SplitPaneState, WorkerCmd,
    WorkerEvent,
};
use crate::sync_queue::SyncQueue;
use crate::tab_router::show_central_panel;
use crate::worker::spawn_worker;

pub struct YtmGuiApp {
    pub(crate) cmd_tx: Sender<WorkerCmd>,
    evt_rx: Receiver<WorkerEvent>,
    pub(crate) active_tab: GuiTab,
    sidebar_filter: String,
    new_playlist_title: String,
    pub(crate) selected_playlist_id: Option<String>,
    pub(crate) session: AsyncState<SessionStatus>,
    pub(crate) library: AsyncState<Vec<PlaylistSummary>>,
    pub(crate) playlist_cache: HashMap<String, Playlist>,
    pub(crate) loading_playlists: HashSet<String>,
    pub(crate) playlist_errors: HashMap<String, String>,
    pub(crate) sync_queue: SyncQueue,
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
    pub(crate) pending_modal: Option<PendingMutation>,
    pub(crate) drop_prompt: Option<DropPrompt>,
    batch_progress: Option<(usize, usize, String)>,
    pub(crate) toast: Option<ToastState>,
    pub(crate) nav_history: NavHistory,
    pub(crate) ui_scale: f32,
    base_pixels_per_point: Option<f32>,
    pub(crate) focus_search_once: bool,
    pub(crate) inspect_last_clicked: Option<usize>,
    pub(crate) inspect_focused_row: Option<usize>,
    pub(crate) inspect_jump_input: String,
    pub(crate) sidebar_focused_idx: Option<usize>,
}

impl YtmGuiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (cmd_tx, evt_rx) = spawn_worker(cc.egui_ctx.clone());
        let default_export = AppPaths::from_project_dirs()
            .map(|p| p.data_dir().join("exports").display().to_string())
            .unwrap_or_else(|_| "./backups".to_string());
        let _ = cmd_tx.send(WorkerCmd::Bootstrap);
        Self::init_with_channels(cmd_tx, evt_rx, default_export)
    }

    pub(crate) fn init_with_channels(
        cmd_tx: Sender<WorkerCmd>,
        evt_rx: Receiver<WorkerEvent>,
        default_export: String,
    ) -> Self {
        Self {
            cmd_tx,
            evt_rx,
            active_tab: GuiTab::Inspector,
            sidebar_filter: String::new(),
            new_playlist_title: String::new(),
            selected_playlist_id: None,
            session: AsyncState::Loading("Verifying InnerTube session...".to_string()),
            library: AsyncState::Loading("Loading library playlists...".to_string()),
            playlist_cache: HashMap::new(),
            loading_playlists: HashSet::new(),
            playlist_errors: HashMap::new(),
            sync_queue: SyncQueue::default(),
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
            export_fmt: ExportFormat::Json,
            auth_raw_input: String::new(),
            auth_browser: BrowserKind::Brave,
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
        }
    }

    pub(crate) fn drain_events(&mut self) {
        while let Ok(evt) = self.evt_rx.try_recv() {
            self.apply_event(evt);
        }
    }

    pub(crate) fn apply_event(&mut self, evt: WorkerEvent) {
        match evt {
            WorkerEvent::SessionLoaded(res) => {
                let failed = res.is_err();
                self.session = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
                if failed {
                    self.active_tab = GuiTab::Auth;
                }
            }
            WorkerEvent::LibraryLoaded(res) => {
                if let Ok(mut list) = res {
                    for s in &mut list {
                        if s.track_count.is_none()
                            && let Some(cached) = self.playlist_cache.get(&s.id)
                        {
                            s.track_count = Some(cached.tracks.len());
                        }
                    }
                    self.library = AsyncState::Ready(list);
                } else if let Err(e) = res {
                    self.library = AsyncState::Failed(e);
                }
            }
            WorkerEvent::PlaylistLoaded {
                playlist_id,
                result,
            } => {
                self.loading_playlists.remove(&playlist_id);
                match result {
                    Ok(pl) => {
                        self.playlist_errors.remove(&playlist_id);
                        if let AsyncState::Ready(list) = &mut self.library
                            && let Some(sum) = list.iter_mut().find(|s| s.id == pl.id)
                        {
                            sum.track_count = Some(pl.tracks.len());
                        }
                        self.playlist_cache.insert(pl.id.clone(), pl);
                    }
                    Err(e) => {
                        self.playlist_errors.insert(playlist_id, e);
                    }
                }
            }
            WorkerEvent::SearchResults(res) => {
                self.search_results = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::Progress {
                done,
                total,
                message,
            } => {
                self.batch_progress = if done >= total {
                    None
                } else {
                    Some((done, total, message))
                };
            }
            WorkerEvent::DedupePlanned(res) => {
                self.dedupe_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::SortPlanned(res) => {
                self.batch_progress = None;
                self.sort_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::SetOpPlanned(res) => {
                self.set_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::HealthScanned(res) => {
                self.batch_progress = None;
                self.health_report = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::GenreSplitPlanned(res) => {
                self.batch_progress = None;
                self.genre_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::SnapshotsLoaded(res) => {
                self.snapshots = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::SnapshotDetailLoaded(res) => {
                self.snapshot_detail = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::StatusToast { ok, message } => {
                self.batch_progress = None;
                self.toast = Some(ToastState::info(ok, message));
            }
            WorkerEvent::MutationFinished {
                ok,
                message,
                reload_ids,
            } => {
                self.batch_progress = None;
                self.toast = Some(ToastState::info(ok, message));
                if ok {
                    self.sync_queue.clear();
                    for id in reload_ids {
                        if id == "__LIBRARY__" {
                            let _ = self.cmd_tx.send(WorkerCmd::RefreshLibrary);
                        } else {
                            self.playlist_cache.remove(&id);
                            if self.loading_playlists.insert(id.clone()) {
                                let _ = self.cmd_tx.send(WorkerCmd::LoadPlaylist(id));
                            }
                        }
                    }
                }
            }
        }
    }
}

impl eframe::App for YtmGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        let base_ppp = *self
            .base_pixels_per_point
            .get_or_insert_with(|| ctx.pixels_per_point());
        let navigated_via_history = apply_global_input(
            ctx,
            base_ppp,
            &mut self.ui_scale,
            &mut self.active_tab,
            &mut self.selected_playlist_id,
            &mut self.focus_search_once,
            &self.sync_queue,
            &mut self.pending_modal,
            &mut self.nav_history,
        );

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            show_top_bar(
                ui,
                &self.session,
                &mut self.library,
                &mut self.active_tab,
                &mut self.selected_playlist_id,
                &mut self.nav_history,
                &mut self.ui_scale,
                self.export_fmt,
                &self.export_dir,
                &self.cmd_tx,
            );
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            show_status_bar(
                ui,
                self.batch_progress.as_ref(),
                &mut self.toast,
                &mut self.sync_queue,
                &mut self.active_tab,
            );
        });

        let prev_selected = self.selected_playlist_id.clone();
        let focus_sidebar = self.focus_search_once
            && (self.active_tab != GuiTab::Inspector || self.selected_playlist_id.is_none());
        egui::SidePanel::left("library_sidebar")
            .default_width(250.0)
            .show(ctx, |ui| {
                sidebar::show(
                    ui,
                    &self.library,
                    &mut self.sidebar_filter,
                    &mut self.new_playlist_title,
                    &mut self.selected_playlist_id,
                    &mut self.loading_playlists,
                    &mut self.playlist_cache,
                    &mut self.sync_queue,
                    &mut self.split_state,
                    &mut self.set_form,
                    &mut self.active_tab,
                    &mut self.drop_prompt,
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
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            show_central_panel(ui, self);
        });
        self.focus_search_once = false;
        if !navigated_via_history {
            self.nav_history
                .record(self.active_tab, self.selected_playlist_id.clone());
        }

        show_drop_prompt(
            ctx,
            &mut self.drop_prompt,
            &self.playlist_cache,
            &self.library,
            &mut self.sync_queue,
            &mut self.toast,
        );
        show_confirmation_modal(ctx, &mut self.pending_modal, &self.cmd_tx);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn worker_events_transition_app_states_cleanly() {
        let (cmd_tx, _cmd_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        let mut app = YtmGuiApp::init_with_channels(cmd_tx, evt_rx, "./backups".to_string());

        assert!(app.session.is_loading());
        evt_tx
            .send(WorkerEvent::SessionLoaded(Ok(SessionStatus {
                authenticated: true,
                account_name: Some("Tester".to_string()),
                channel_handle: Some("@test".to_string()),
                auth_user: "0".to_string(),
                brand_page_id: None,
            })))
            .expect("send event");
        app.drain_events();
        assert_eq!(
            app.session.data().and_then(|s| s.account_name.as_deref()),
            Some("Tester")
        );

        evt_tx
            .send(WorkerEvent::MutationFinished {
                ok: true,
                message: "done".to_string(),
                reload_ids: vec!["PL1".to_string()],
            })
            .expect("send mutation finished");
        app.drain_events();
        assert!(app.loading_playlists.contains("PL1"));
        assert_eq!(app.toast.as_ref().map(|t| t.message.as_str()), Some("done"));
    }
}
