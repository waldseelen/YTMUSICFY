use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use anyhow::{Context, Result};
use eframe::egui;
use ytm_core::{AuthCredentials, InnerTubeClient, InnerTubeClientTrait, ReqwestTransport};
use ytm_enrich::{EnrichmentService, MetadataProvider, ReqwestEnrichmentHttp};
use ytm_ops::{SortField, plan_dedupe, plan_set_op, plan_sort};
use ytm_store::{AppPaths, SnapshotStore, SqliteStore};

use crate::state::{GuiAppConfig, WorkerCmd, WorkerEvent};
use crate::worker_exec::{
    backup_inner, emit_progress, import_auth_inner, load_playlist, run_confirmed_mutation,
};
use crate::worker_plans::{build_matcher, plan_genre_split_inner, scan_health_inner};

pub fn spawn_worker(ctx: egui::Context) -> (Sender<WorkerCmd>, Receiver<WorkerEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCmd>();
    let (evt_tx, evt_rx) = mpsc::channel::<WorkerEvent>();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(worker_loop(ctx, cmd_rx, evt_tx));
    });

    (cmd_tx, evt_rx)
}

pub(crate) struct BackendState {
    pub(crate) paths: AppPaths,
    pub(crate) config: GuiAppConfig,
    pub(crate) store: Arc<SqliteStore>,
    pub(crate) client: Option<InnerTubeClient<ReqwestTransport>>,
}

fn load_config(paths: &AppPaths) -> GuiAppConfig {
    let path = paths.config_file();
    let mut cfg = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str::<GuiAppConfig>(&s).ok())
            .unwrap_or_default()
    } else {
        GuiAppConfig::default()
    };
    if cfg.enrichment.lastfm_api_key.is_none()
        && let Ok(val) = std::env::var("YTM_LASTFM_API_KEY")
        && !val.trim().is_empty()
    {
        cfg.enrichment.lastfm_api_key = Some(val.trim().to_string());
    }
    cfg
}

fn load_saved_client(
    paths: &AppPaths,
    cfg: &GuiAppConfig,
) -> Result<InnerTubeClient<ReqwestTransport>> {
    let bytes = fs::read(paths.auth_file()).context("reading saved auth.json")?;
    let creds: AuthCredentials = serde_json::from_slice(&bytes).context("decoding auth.json")?;
    let transport = ReqwestTransport::new(cfg.innertube.max_retries)?;
    Ok(InnerTubeClient::new(
        transport,
        creds,
        cfg.innertube.clone(),
    ))
}

async fn worker_loop(
    egui_ctx: egui::Context,
    cmd_rx: Receiver<WorkerCmd>,
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
    let client = load_saved_client(&paths, &config).ok();
    let mut state = BackendState {
        paths,
        config,
        store,
        client,
    };

    while let Ok(cmd) = cmd_rx.recv() {
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
            if let Ok(snaps) = state.store.list_snapshots(None).await {
                let _ = tx.send(WorkerEvent::SnapshotsLoaded(Ok(snaps)));
            }
            if let Some(client) = &state.client {
                let sess = client.verify_session().await.map_err(|e| e.to_string());
                let is_ok = sess.as_ref().is_ok_and(|s| s.authenticated);
                let _ = tx.send(WorkerEvent::SessionLoaded(sess));
                if is_ok {
                    let libs = client
                        .get_library_playlists()
                        .await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(WorkerEvent::LibraryLoaded(libs));
                }
            } else {
                let _ = tx.send(WorkerEvent::SessionLoaded(Err(
                    "No saved auth.json. Paste browser headers in Auth tab.".to_string(),
                )));
            }
        }
        WorkerCmd::RefreshLibrary => {
            let Some(client) = &state.client else {
                return;
            };
            let libs = client
                .get_library_playlists()
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::LibraryLoaded(libs));
        }
        WorkerCmd::ImportAuth(raw) => {
            match import_auth_inner(&state.paths, &state.config, &raw).await {
                Ok((client, status)) => {
                    state.client = Some(client);
                    let _ = tx.send(WorkerEvent::SessionLoaded(Ok(status)));
                    let _ = tx.send(WorkerEvent::StatusToast {
                        ok: true,
                        message: "Credentials verified and saved.".to_string(),
                    });
                    if let Some(c) = &state.client {
                        let libs = c.get_library_playlists().await.map_err(|e| e.to_string());
                        let _ = tx.send(WorkerEvent::LibraryLoaded(libs));
                    }
                }
                Err(e) => {
                    let _ = tx.send(WorkerEvent::StatusToast {
                        ok: false,
                        message: format!("Auth import failed: {e}"),
                    });
                }
            }
        }
        WorkerCmd::LoadPlaylist(id) => {
            let Some(client) = &state.client else {
                return;
            };
            let result = load_playlist(client, &id).await.map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::PlaylistLoaded {
                playlist_id: id,
                result,
            });
        }
        WorkerCmd::SearchTracks(query) => {
            let Some(client) = &state.client else {
                return;
            };
            let res = client
                .search_tracks(query.trim(), 8)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::SearchResults(res));
        }
        WorkerCmd::BackupPlaylist {
            target,
            format,
            out_dir,
        } => {
            let Some(client) = &state.client else {
                return;
            };
            let msg = backup_inner(client, &state.store, &target, format, &out_dir, tx, ctx).await;
            let _ = tx.send(match msg {
                Ok(m) => WorkerEvent::StatusToast {
                    ok: true,
                    message: m,
                },
                Err(e) => WorkerEvent::StatusToast {
                    ok: false,
                    message: e.to_string(),
                },
            });
            if let Ok(snaps) = state.store.list_snapshots(None).await {
                let _ = tx.send(WorkerEvent::SnapshotsLoaded(Ok(snaps)));
            }
        }
        WorkerCmd::PlanDedupe {
            playlist_id,
            config,
        } => {
            let Some(client) = &state.client else {
                return;
            };
            let res = async {
                let pl = load_playlist(client, &playlist_id).await?;
                Ok::<_, anyhow::Error>(plan_dedupe(&pl, &config)?)
            }
            .await
            .map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::DedupePlanned(res));
        }
        WorkerCmd::PlanSort { playlist_id, spec } => {
            let Some(client) = &state.client else {
                return;
            };
            let res = async {
                let pl = load_playlist(client, &playlist_id).await?;
                let genre_map = if spec.keys.iter().any(|k| k.field == SortField::Genre) {
                    let matcher = build_matcher(None, &state.paths)?;
                    let svc = EnrichmentService::new(
                        Arc::clone(&state.store),
                        ReqwestEnrichmentHttp::new()?,
                        state.config.enrichment.clone(),
                        matcher,
                    );
                    let mut map = HashMap::new();
                    let total = pl.tracks.len().max(1);
                    for (idx, t) in pl.tracks.iter().enumerate() {
                        if idx % 5 == 0 {
                            emit_progress(
                                tx,
                                ctx,
                                idx,
                                total,
                                format!("Enriching {}/{total}...", idx + 1),
                            );
                        }
                        let meta = svc.enrich_track(t).await?;
                        map.insert(t.video_id.clone(), meta.primary_genre);
                    }
                    Some(map)
                } else {
                    None
                };
                Ok::<_, anyhow::Error>(plan_sort(&pl, &spec, genre_map.as_ref())?)
            }
            .await
            .map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::SortPlanned(res));
        }
        WorkerCmd::PlanSetOp {
            op,
            mode,
            playlist_a,
            playlist_b,
            target,
        } => {
            let Some(client) = &state.client else {
                return;
            };
            let res = async {
                let a = load_playlist(client, &playlist_a).await?;
                let b = load_playlist(client, &playlist_b).await?;
                Ok::<_, anyhow::Error>(plan_set_op(op, mode, &a, &b, target, 0.90)?)
            }
            .await
            .map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::SetOpPlanned(res));
        }
        WorkerCmd::ScanHealth {
            playlist_id,
            find_official,
        } => {
            let Some(client) = &state.client else {
                return;
            };
            let res = scan_health_inner(client, &playlist_id, find_official, tx, ctx)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::HealthScanned(res));
        }
        WorkerCmd::PlanGenreSplit {
            playlist_id,
            min_tracks,
            custom_genres_path,
        } => {
            let res = plan_genre_split_inner(
                state,
                &playlist_id,
                min_tracks,
                custom_genres_path.as_deref(),
                tx,
                ctx,
            )
            .await
            .map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::GenreSplitPlanned(res));
        }
        WorkerCmd::RefreshSnapshots => {
            let res = state
                .store
                .list_snapshots(None)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::SnapshotsLoaded(res));
        }
        WorkerCmd::LoadSnapshot(id) => {
            let res = state
                .store
                .load_snapshot(&id)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::SnapshotDetailLoaded(res));
        }
        WorkerCmd::ExecuteMutation(mutation) => {
            let Some(client) = &state.client else {
                return;
            };
            let event = match run_confirmed_mutation(client, &state.store, mutation, tx, ctx).await
            {
                Ok((message, reload_ids)) => WorkerEvent::MutationFinished {
                    ok: true,
                    message,
                    reload_ids,
                },
                Err(e) => WorkerEvent::MutationFinished {
                    ok: false,
                    message: format!("Sync error: {e}"),
                    reload_ids: Vec::new(),
                },
            };
            let _ = tx.send(event);
            if let Ok(snaps) = state.store.list_snapshots(None).await {
                let _ = tx.send(WorkerEvent::SnapshotsLoaded(Ok(snaps)));
            }
        }
    }
}
