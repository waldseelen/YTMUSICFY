use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
use tokio::sync::{Mutex as AsyncMutex, mpsc as tokio_mpsc};
use ytm_core::{InnerTubeClient, InnerTubeClientTrait, ReqwestTransport};
use ytm_ops::{plan_dedupe, plan_sort};
use ytm_store::{AppPaths, EnrichmentCacheStore, SqliteStore};

#[path = "worker_jobs.rs"]
mod worker_jobs;

use crate::auth_store::{active_cookie_jar_path, save_gui_config};
use crate::state::{GuiAppConfig, JobId, WorkerCmd, WorkerEvent};
use crate::worker_auth::{
    handle_bootstrap, import_auth_inner, import_browser_auth_inner, import_clipboard_auth_inner,
};
use crate::worker_exec::{
    cache_fetched_playlist, hydrate_gui_startup_state, load_config, load_playlist_with_manifest,
    load_saved_client, persist_staged_entries, sync_library_cache,
};
use crate::worker_index::spawn_index_all_job;
use crate::worker_media::{handle_interactive_cmd, spawn_backup_job};

pub fn spawn_worker(ctx: egui::Context) -> (Sender<WorkerCmd>, Receiver<WorkerEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCmd>();
    let (evt_tx, evt_rx) = mpsc::channel::<WorkerEvent>();
    let (async_tx, async_rx) = tokio_mpsc::unbounded_channel::<WorkerCmd>();

    // Non-blocking bridge so Tokio worker threads never block on std::sync::mpsc::recv().
    std::thread::spawn(move || {
        while let Ok(cmd) = cmd_rx.recv() {
            if async_tx.send(cmd).is_err() {
                break;
            }
        }
    });

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(worker_loop(ctx, async_rx, evt_tx));
    });

    (cmd_tx, evt_rx)
}

pub(crate) struct BackendState {
    pub(crate) paths: AppPaths,
    pub(crate) config: GuiAppConfig,
    pub(crate) store: Arc<SqliteStore>,
    pub(crate) client: Option<Arc<InnerTubeClient<ReqwestTransport>>>,
    pub(crate) mutation_lock: Arc<AsyncMutex<()>>,
    pub(crate) active_jobs: HashMap<JobId, Arc<AtomicBool>>,
    pub(crate) next_job_id: u64,
}

impl BackendState {
    pub(crate) fn allocate_job(&mut self) -> (JobId, Arc<AtomicBool>) {
        self.next_job_id = self.next_job_id.saturating_add(1);
        let id = JobId(self.next_job_id);
        let cancel = Arc::new(AtomicBool::new(false));
        self.active_jobs.insert(id, Arc::clone(&cancel));
        (id, cancel)
    }
}

fn emit_audio_config(state: &BackendState, tx: &Sender<WorkerEvent>) {
    let _ = tx.send(WorkerEvent::AudioConfigUpdated {
        cookie_jar: active_cookie_jar_path(&state.paths),
        quality: state.config.audio_quality,
        preferred_browser: state.config.preferred_browser,
    });
}

async fn finish_auth_import(
    state: &mut BackendState,
    res: anyhow::Result<(InnerTubeClient<ReqwestTransport>, ytm_core::SessionStatus)>,
    preferred_browser: Option<crate::worker_auth::BrowserKind>,
    ok_msg: String,
    err_prefix: &str,
    tx: &Sender<WorkerEvent>,
) {
    match res {
        Ok((client, status)) => {
            let client = Arc::new(client);
            state.client = Some(Arc::clone(&client));
            if let Some(browser) = preferred_browser {
                state.config.preferred_browser = Some(browser);
                let _ = save_gui_config(&state.paths, &state.config);
            }
            emit_audio_config(state, tx);
            let _ = tx.send(WorkerEvent::SessionLoaded(Ok(status)));
            let _ = tx.send(WorkerEvent::StatusToast {
                ok: true,
                message: ok_msg,
            });
            let libs = client
                .get_library_playlists()
                .await
                .map_err(|e| e.to_string());
            if let Ok(list) = &libs {
                sync_library_cache(&state.store, list);
            }
            let _ = tx.send(WorkerEvent::LibraryLoaded(libs));
        }
        Err(e) => {
            let _ = tx.send(WorkerEvent::StatusToast {
                ok: false,
                message: format!("{err_prefix}: {e}"),
            });
        }
    }
}

async fn worker_loop(
    egui_ctx: egui::Context,
    mut cmd_rx: tokio_mpsc::UnboundedReceiver<WorkerCmd>,
    evt_tx: Sender<WorkerEvent>,
) {
    let paths = match AppPaths::from_project_dirs() {
        Ok(p) => p,
        Err(e) => {
            let _ = evt_tx.send(WorkerEvent::StatusToast {
                ok: false,
                message: e.to_string(),
            });
            return;
        }
    };
    let _ = paths.ensure_dirs();
    crate::theme_detect::spawn_theme_watcher(evt_tx.clone(), egui_ctx.clone());
    let config = load_config(&paths);
    let store = match SqliteStore::open(&paths.db_file(), Some(paths.snapshots_dir())) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            let _ = evt_tx.send(WorkerEvent::StatusToast {
                ok: false,
                message: e.to_string(),
            });
            return;
        }
    };
    let client = load_saved_client(&paths, &config).ok().map(Arc::new);
    let mut state = BackendState {
        paths,
        config,
        store,
        client,
        mutation_lock: Arc::new(AsyncMutex::new(())),
        active_jobs: HashMap::new(),
        next_job_id: 0,
    };

    while let Some(cmd) = cmd_rx.recv().await {
        handle_cmd(&mut state, cmd, &evt_tx, &egui_ctx).await;
        egui_ctx.request_repaint();
    }
}

async fn handle_cmd(
    state: &mut BackendState,
    cmd: WorkerCmd,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) {
    match cmd {
        WorkerCmd::Bootstrap => {
            emit_audio_config(state, tx);
            let _ = tx.send(WorkerEvent::GuiConfigLoaded(state.config.clone()));
            if let Ok(Some(sess)) = state.store.load_playback_session() {
                let _ = tx.send(WorkerEvent::PlaybackSessionLoaded(sess.session_json));
            }
            if let Ok(Some(home_page)) = state.store.load_cached_browse_page(ytm_core::BROWSE_HOME)
            {
                let _ = tx.send(WorkerEvent::BrowseCacheHydrated(home_page));
            }
            hydrate_gui_startup_state(&state.store, state.config.mutation_policy, tx);
            handle_bootstrap(state, tx).await;
        }
        WorkerCmd::CancelJob(job_id) => {
            if let Some(flag) = state.active_jobs.remove(&job_id) {
                flag.store(true, Ordering::Relaxed);
                let _ = tx.send(WorkerEvent::StatusToast {
                    ok: true,
                    message: format!("Cancelling job #{}...", job_id.0),
                });
            }
        }
        WorkerCmd::SetAudioQuality(quality) => {
            state.config.audio_quality = quality;
            let _ = save_gui_config(&state.paths, &state.config);
            emit_audio_config(state, tx);
        }
        WorkerCmd::UpdateGuiConfig(new_cfg) => {
            state.config = new_cfg.clone();
            let _ = save_gui_config(&state.paths, &state.config);
            emit_audio_config(state, tx);
            let _ = tx.send(WorkerEvent::MutationPolicyLoaded(
                state.config.mutation_policy,
            ));
            let _ = tx.send(WorkerEvent::GuiConfigLoaded(new_cfg));
            let _ = tx.send(WorkerEvent::StatusToast {
                ok: true,
                message: "Settings saved to config.toml.".to_string(),
            });
        }
        WorkerCmd::SaveStagedOps(entries) => {
            let _ = persist_staged_entries(&state.store, &entries);
        }
        WorkerCmd::SavePlaybackSession(json) => {
            let sess = ytm_store::PersistedPlaybackSession {
                session_json: json,
                updated_at: chrono::Utc::now().timestamp(),
            };
            let _ = state.store.save_playback_session(&sess);
        }
        WorkerCmd::ImportAuth(raw) => {
            let res = import_auth_inner(&state.paths, &state.config, &raw).await;
            finish_auth_import(
                state,
                res,
                None,
                "Credentials verified and saved.".to_string(),
                "Auth import failed",
                tx,
            )
            .await;
        }
        WorkerCmd::ImportClipboardAuth => {
            let _ = tx.send(WorkerEvent::StatusToast {
                ok: true,
                message: "Reading system clipboard & verifying session...".to_string(),
            });
            let res = import_clipboard_auth_inner(&state.paths, &state.config).await;
            finish_auth_import(
                state,
                res,
                None,
                "Clipboard session verified and saved.".to_string(),
                "Clipboard auth failed",
                tx,
            )
            .await;
        }
        WorkerCmd::ImportBrowserAuth(browser) => {
            let _ = tx.send(WorkerEvent::StatusToast {
                ok: true,
                message: format!("Importing cookies from {}...", browser.label()),
            });
            let res = import_browser_auth_inner(&state.paths, &state.config, browser).await;
            finish_auth_import(
                state,
                res,
                Some(browser),
                format!("Imported session from {} and saved.", browser.label()),
                "Browser import failed",
                tx,
            )
            .await;
        }
        // --- Lane 1: Interactive parallel spawns ---
        WorkerCmd::RefreshLibrary => {
            let Some(client) = state.client.as_ref().map(Arc::clone) else {
                return;
            };
            let store = Arc::clone(&state.store);
            let tx = tx.clone();
            let ctx = ctx.clone();
            tokio::spawn(async move {
                let libs = client
                    .get_library_playlists()
                    .await
                    .map_err(|e| e.to_string());
                if let Ok(list) = &libs {
                    sync_library_cache(&store, list);
                }
                let _ = tx.send(WorkerEvent::LibraryLoaded(libs));
                ctx.request_repaint();
            });
        }
        WorkerCmd::LoadPlaylist(id) => {
            let Some(client) = state.client.as_ref().map(Arc::clone) else {
                return;
            };
            let store = Arc::clone(&state.store);
            let tx = tx.clone();
            let ctx = ctx.clone();
            tokio::spawn(async move {
                let result = match load_playlist_with_manifest(client.as_ref(), &id).await {
                    Ok(fetched) => {
                        cache_fetched_playlist(&store, &fetched, &tx);
                        Ok(fetched.playlist)
                    }
                    Err(e) => Err(e.to_string()),
                };
                let _ = tx.send(WorkerEvent::PlaylistLoaded {
                    playlist_id: id,
                    result,
                });
                ctx.request_repaint();
            });
        }
        WorkerCmd::HarvestArtist { artist_id, options } => {
            let Some(client) = state.client.as_ref().map(Arc::clone) else {
                let _ = tx.send(WorkerEvent::StatusToast {
                    ok: false,
                    message: "Sign in before harvesting an artist.".to_string(),
                });
                return;
            };
            let tx = tx.clone();
            let ctx = ctx.clone();
            tokio::spawn(async move {
                let res = ytm_core::harvest_artist(client.as_ref(), &artist_id, &options)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::ArtistHarvested(res));
                ctx.request_repaint();
            });
        }
        WorkerCmd::PlanDedupeDraft { playlist, config } => {
            let res = plan_dedupe(&playlist, &config).map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::DedupePlanned(res));
        }
        WorkerCmd::PlanSortDraft { playlist, spec } => {
            let res = plan_sort(&playlist, &spec, None).map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::SortPlanned(res));
        }
        WorkerCmd::RecordPlayHistory {
            video_id,
            title,
            artists_json,
            album,
            duration_sec,
        } => {
            let store = Arc::clone(&state.store);
            tokio::spawn(async move {
                let _ = store
                    .insert_play_history(
                        &video_id,
                        &title,
                        &artists_json,
                        album.as_deref(),
                        duration_sec,
                    )
                    .await;
            });
        }
        // --- Lane 2: Cancellable Background Jobs ---
        WorkerCmd::IndexAllPlaylists { missing_only } => {
            spawn_index_all_job(state, missing_only, tx, ctx);
        }
        WorkerCmd::BackupPlaylist {
            target,
            format,
            out_dir,
        } => {
            spawn_backup_job(state, target, format, out_dir, tx, ctx);
        }
        // --- Lane 3: Serialized Remote Mutations ---
        WorkerCmd::ExecuteMutation(mutation) => {
            worker_jobs::spawn_execute_mutation(state, mutation, tx, ctx);
        }
        other => {
            if let Some(rem) = worker_jobs::handle_background_job(state, other, tx, ctx).await {
                handle_interactive_cmd(state, rem, tx, ctx);
            }
        }
    }
}
