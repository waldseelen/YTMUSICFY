use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, SessionStatus};
use ytm_enrich::GenreSplitPlan;
use ytm_ops::{DedupePlan, DiffPlan, HealthReport, SortPlan};
use ytm_store::{AppPaths, ExportFormat, Snapshot, SnapshotMetadata};

use crate::state::{
    AsyncState, DedupeForm, GenreSplitForm, GuiTab, HealthForm, PendingMutation, SetOpsForm,
    SortForm, WorkerCmd, WorkerEvent,
};
use crate::views;
use crate::worker::spawn_worker;

pub struct YtmGuiApp {
    cmd_tx: Sender<WorkerCmd>,
    evt_rx: Receiver<WorkerEvent>,
    active_tab: GuiTab,
    sidebar_filter: String,
    selected_playlist_id: Option<String>,
    session: AsyncState<SessionStatus>,
    library: AsyncState<Vec<PlaylistSummary>>,
    active_playlist: AsyncState<Playlist>,
    dedupe_form: DedupeForm,
    dedupe_plan: AsyncState<DedupePlan>,
    sort_form: SortForm,
    sort_plan: AsyncState<SortPlan>,
    set_form: SetOpsForm,
    set_plan: AsyncState<DiffPlan>,
    health_form: HealthForm,
    health_report: AsyncState<HealthReport>,
    genre_form: GenreSplitForm,
    genre_plan: AsyncState<(Playlist, GenreSplitPlan)>,
    snapshots: AsyncState<Vec<SnapshotMetadata>>,
    snapshot_detail: AsyncState<Snapshot>,
    inspect_search: String,
    export_dir: String,
    export_fmt: ExportFormat,
    auth_raw_input: String,
    pending_modal: Option<PendingMutation>,
    toast: Option<(bool, String)>,
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
            selected_playlist_id: None,
            session: AsyncState::Loading("Verifying InnerTube session...".to_string()),
            library: AsyncState::Loading("Loading library playlists...".to_string()),
            active_playlist: AsyncState::Idle,
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
            inspect_search: String::new(),
            export_dir: default_export,
            export_fmt: ExportFormat::Json,
            auth_raw_input: String::new(),
            pending_modal: None,
            toast: None,
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
                self.session = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::LibraryLoaded(res) => {
                self.library = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::PlaylistLoaded(res) => {
                self.active_playlist = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::DedupePlanned(res) => {
                self.dedupe_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::SortPlanned(res) => {
                self.sort_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::SetOpPlanned(res) => {
                self.set_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::HealthScanned(res) => {
                self.health_report = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::GenreSplitPlanned(res) => {
                self.genre_plan = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::SnapshotsLoaded(res) => {
                self.snapshots = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::SnapshotDetailLoaded(res) => {
                self.snapshot_detail = res.map_or_else(AsyncState::Failed, AsyncState::Ready);
            }
            WorkerEvent::StatusToast { ok, message } => {
                self.toast = Some((ok, message));
            }
            WorkerEvent::MutationFinished {
                ok,
                message,
                reload_playlist_id,
            } => {
                self.toast = Some((ok, message));
                if let Some(pid) = reload_playlist_id {
                    self.active_playlist =
                        AsyncState::Loading("Refreshing playlist...".to_string());
                    let _ = self.cmd_tx.send(WorkerCmd::LoadPlaylist(pid));
                }
            }
        }
    }

    fn show_top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("ytm-architect");
            ui.separator();
            match &self.session {
                AsyncState::Ready(s) => {
                    ui.colored_label(
                        egui::Color32::from_rgb(90, 200, 120),
                        format!(
                            "{} ({})",
                            s.account_name.as_deref().unwrap_or("Signed in"),
                            s.channel_handle.as_deref().unwrap_or("-")
                        ),
                    );
                }
                AsyncState::Loading(m) => {
                    ui.spinner();
                    ui.label(m);
                }
                AsyncState::Failed(_) => {
                    ui.colored_label(egui::Color32::from_rgb(235, 160, 60), "Not authenticated");
                }
                AsyncState::Idle => {}
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Backup All").clicked() {
                    let _ = self.cmd_tx.send(WorkerCmd::BackupPlaylist {
                        target: "all".to_string(),
                        format: self.export_fmt,
                        out_dir: PathBuf::from(self.export_dir.trim()),
                    });
                }
                if ui.button("Refresh").clicked() {
                    self.library = AsyncState::Loading("Refreshing library...".to_string());
                    let _ = self.cmd_tx.send(WorkerCmd::RefreshLibrary);
                }
            });
        });
    }

    fn show_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.strong("Library Playlists");
        ui.text_edit_singleline(&mut self.sidebar_filter);
        ui.separator();

        if let AsyncState::Loading(m) = &self.library {
            ui.spinner();
            ui.label(m);
            return;
        }
        if let AsyncState::Failed(err) = &self.library {
            ui.colored_label(egui::Color32::from_rgb(235, 95, 95), err);
            return;
        }
        let Some(playlists) = self.library.data() else {
            return;
        };

        let needle = self.sidebar_filter.trim().to_ascii_lowercase();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for pl in playlists {
                if !needle.is_empty() && !pl.title.to_ascii_lowercase().contains(&needle) {
                    continue;
                }
                let selected = self.selected_playlist_id.as_deref() == Some(pl.id.as_str());
                let count = pl
                    .track_count
                    .map_or_else(|| "?".to_string(), |c| c.to_string());
                let label = format!("{} ({count})", pl.title);
                if ui.selectable_label(selected, label).clicked() {
                    self.selected_playlist_id = Some(pl.id.clone());
                    self.active_playlist = AsyncState::Loading("Loading playlist...".to_string());
                    let _ = self.cmd_tx.send(WorkerCmd::LoadPlaylist(pl.id.clone()));
                }
            }
        });
    }

    fn show_confirmation_modal(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_modal.clone() else {
            return;
        };
        egui::Window::new(&pending.title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(&pending.summary);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Confirm & Execute").clicked() {
                        let _ = self.cmd_tx.send(WorkerCmd::ExecuteMutation(pending.action));
                        self.pending_modal = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_modal = None;
                    }
                });
            });
    }
}

impl eframe::App for YtmGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            self.show_top_bar(ui);
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Safety: Dry-run default + pre-mutation SQLite snapshots");
                if let Some((ok, msg)) = &self.toast {
                    ui.separator();
                    let color = if *ok {
                        egui::Color32::from_rgb(90, 200, 120)
                    } else {
                        egui::Color32::from_rgb(235, 95, 95)
                    };
                    ui.colored_label(color, msg);
                }
            });
        });

        egui::SidePanel::left("library_sidebar")
            .default_width(240.0)
            .show(ctx, |ui| self.show_sidebar(ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for tab in GuiTab::ALL {
                    ui.selectable_value(&mut self.active_tab, tab, tab.label());
                }
            });
            ui.separator();

            let sel_id = self.selected_playlist_id.as_deref();
            match self.active_tab {
                GuiTab::Inspector => views::inspect::show(
                    ui,
                    &mut self.active_playlist,
                    &mut self.inspect_search,
                    &mut self.export_dir,
                    &mut self.export_fmt,
                    &self.cmd_tx,
                ),
                GuiTab::Dedupe => views::dedupe::show(
                    ui,
                    sel_id,
                    &mut self.dedupe_form,
                    &mut self.dedupe_plan,
                    &mut self.pending_modal,
                    &self.cmd_tx,
                ),
                GuiTab::Sort => views::sort::show(
                    ui,
                    sel_id,
                    &mut self.sort_form,
                    &mut self.sort_plan,
                    &mut self.pending_modal,
                    &self.cmd_tx,
                ),
                GuiTab::SetOps => {
                    let playlists = self.library.data().map(Vec::as_slice).unwrap_or(&[]);
                    views::set_ops::show(
                        ui,
                        playlists,
                        &mut self.set_form,
                        &mut self.set_plan,
                        &mut self.pending_modal,
                        &self.cmd_tx,
                    );
                }
                GuiTab::Health => views::health::show(
                    ui,
                    self.active_playlist.data(),
                    sel_id,
                    &mut self.health_form,
                    &mut self.health_report,
                    &mut self.pending_modal,
                    &self.cmd_tx,
                ),
                GuiTab::GenreSplit => views::genres_snapshots::show_genre_split(
                    ui,
                    sel_id,
                    &mut self.genre_form,
                    &mut self.genre_plan,
                    &mut self.pending_modal,
                    &self.cmd_tx,
                ),
                GuiTab::Snapshots => views::genres_snapshots::show_snapshots(
                    ui,
                    &self.snapshots,
                    &mut self.snapshot_detail,
                    &mut self.pending_modal,
                    &self.cmd_tx,
                ),
                GuiTab::Auth => {
                    views::auth::show(ui, &self.session, &mut self.auth_raw_input, &self.cmd_tx)
                }
            }
        });

        self.show_confirmation_modal(ctx);
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
                reload_playlist_id: Some("PL1".to_string()),
            })
            .expect("send mutation finished");
        app.drain_events();
        assert!(app.active_playlist.is_loading());
        assert_eq!(app.toast.as_ref().map(|(_, m)| m.as_str()), Some("done"));
    }
}
