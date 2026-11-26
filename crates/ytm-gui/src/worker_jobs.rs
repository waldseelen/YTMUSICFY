use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_enrich::{EnrichmentService, MetadataProvider, ReqwestEnrichmentHttp};
use ytm_ops::{SortField, plan_dedupe, plan_set_op, plan_sort};

use crate::state::{WorkerCmd, WorkerEvent};
use crate::worker::BackendState;
use crate::worker_exec::{emit_progress, load_playlist};
use crate::worker_plans::{build_matcher, plan_genre_split_inner, scan_health_inner};

/// Dispatches Lane 2 cancellable background planning jobs (`Dedupe`, `Sort`, `SetOp`, `Health`, `GenreSplit`).
pub(crate) async fn handle_background_job(
    state: &mut BackendState,
    cmd: WorkerCmd,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) -> Option<WorkerCmd> {
    match cmd {
        WorkerCmd::PlanDedupe {
            playlist_id,
            config,
        } => {
            let client = state.client.as_ref().map(Arc::clone)?;
            let tx = tx.clone();
            let ctx = ctx.clone();
            tokio::spawn(async move {
                let res = async {
                    let pl = load_playlist(client.as_ref(), &playlist_id).await?;
                    Ok::<_, anyhow::Error>(plan_dedupe(&pl, &config)?)
                }
                .await
                .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::DedupePlanned(res));
                ctx.request_repaint();
            });
            None
        }
        WorkerCmd::PlanSort { playlist_id, spec } => {
            let client = state.client.as_ref().map(Arc::clone)?;
            let (job_id, cancel) = state.allocate_job();
            let store = Arc::clone(&state.store);
            let paths = state.paths.clone();
            let enrich_cfg = state.config.enrichment.clone();
            let tx = tx.clone();
            let ctx = ctx.clone();
            let _ = tx.send(WorkerEvent::JobStarted {
                job_id,
                label: "Sort LIS".to_string(),
            });
            tokio::spawn(async move {
                let res = async {
                    let pl = load_playlist(client.as_ref(), &playlist_id).await?;
                    let genre_map = if spec.keys.iter().any(|k| k.field == SortField::Genre) {
                        let matcher = build_matcher(None, &paths)?;
                        let svc = EnrichmentService::new(
                            store,
                            ReqwestEnrichmentHttp::new()?,
                            enrich_cfg,
                            matcher,
                        );
                        let mut map = HashMap::new();
                        let total = pl.tracks.len().max(1);
                        for (idx, t) in pl.tracks.iter().enumerate() {
                            if cancel.load(Ordering::Relaxed) {
                                anyhow::bail!("Sort enrichment cancelled by user.");
                            }
                            if idx % 5 == 0 {
                                emit_progress(
                                    &tx,
                                    &ctx,
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
                let _ = tx.send(WorkerEvent::JobEnded(job_id));
                ctx.request_repaint();
            });
            None
        }
        WorkerCmd::PlanSetOp {
            op,
            mode,
            playlist_a,
            playlist_b,
            target,
        } => {
            let client = state.client.as_ref().map(Arc::clone)?;
            let tx = tx.clone();
            let ctx = ctx.clone();
            tokio::spawn(async move {
                let res = async {
                    let a = load_playlist(client.as_ref(), &playlist_a).await?;
                    let b = load_playlist(client.as_ref(), &playlist_b).await?;
                    Ok::<_, anyhow::Error>(plan_set_op(op, mode, &a, &b, target, 0.90)?)
                }
                .await
                .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::SetOpPlanned(res));
                ctx.request_repaint();
            });
            None
        }
        WorkerCmd::ScanHealth {
            playlist_id,
            find_official,
        } => {
            let client = state.client.as_ref().map(Arc::clone)?;
            let (job_id, _cancel) = state.allocate_job();
            let tx = tx.clone();
            let ctx = ctx.clone();
            let _ = tx.send(WorkerEvent::JobStarted {
                job_id,
                label: "Health Scan".to_string(),
            });
            tokio::spawn(async move {
                let res =
                    scan_health_inner(client.as_ref(), &playlist_id, find_official, &tx, &ctx)
                        .await
                        .map_err(|e| e.to_string());
                let _ = tx.send(WorkerEvent::HealthScanned(res));
                let _ = tx.send(WorkerEvent::JobEnded(job_id));
                ctx.request_repaint();
            });
            None
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
            None
        }
        other => Some(other),
    }
}
