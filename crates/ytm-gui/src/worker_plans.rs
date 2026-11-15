use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use anyhow::Result;
use eframe::egui;
use ytm_core::{
    InnerTubeClient, InnerTubeClientTrait, MusicVideoType, Playlist, ReqwestTransport, Track,
};
use ytm_enrich::{
    EnrichmentService, GenreMatcher, GenreSplitPlan, ReqwestEnrichmentHttp, plan_genre_split,
};
use ytm_ops::{HealthReport, normalize_track, scan_playlist_health};
use ytm_store::AppPaths;

use crate::state::WorkerEvent;
use crate::worker::BackendState;
use crate::worker_exec::{emit_progress, load_playlist};

pub fn build_matcher(custom: Option<&Path>, paths: &AppPaths) -> Result<GenreMatcher> {
    if let Some(p) = custom {
        return Ok(GenreMatcher::from_toml_path(p)?);
    }
    let default_path = paths.genres_file();
    if default_path.exists() {
        return Ok(GenreMatcher::from_toml_path(&default_path)?);
    }
    Ok(GenreMatcher::built_in_default())
}

pub async fn scan_health_inner(
    client: &InnerTubeClient<ReqwestTransport>,
    playlist_id: &str,
    find_official: bool,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) -> Result<HealthReport> {
    let pl = load_playlist(client, playlist_id).await?;
    let mut search_map: HashMap<String, Vec<Track>> = HashMap::new();
    if find_official {
        let candidates: Vec<&Track> = pl
            .tracks
            .iter()
            .filter(|t| {
                matches!(
                    t.music_video_type,
                    MusicVideoType::UserGenerated | MusicVideoType::OfficialMusicVideo
                )
            })
            .collect();
        let total = candidates.len().max(1);
        for (idx, t) in candidates.into_iter().enumerate() {
            if idx % 3 == 0 {
                emit_progress(
                    tx,
                    ctx,
                    idx,
                    total,
                    format!("ATV lookup {}/{total}: {}", idx + 1, t.title),
                );
            }
            let n = normalize_track(t);
            let q = format!("{} {}", n.artist_norm, n.title_norm);
            search_map.insert(t.video_id.clone(), client.search_tracks(q.trim(), 5).await?);
        }
    }
    Ok(scan_playlist_health(&pl, &search_map, 0.88))
}

pub(crate) async fn plan_genre_split_inner(
    state: &BackendState,
    playlist_id: &str,
    min_tracks: usize,
    custom_genres_path: Option<&Path>,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) -> Result<(Playlist, GenreSplitPlan)> {
    let Some(client) = &state.client else {
        anyhow::bail!("not authenticated");
    };
    let pl = load_playlist(client, playlist_id).await?;
    let existing = client.get_library_playlists().await?;
    let prefix = format!("{} - ", pl.title).to_ascii_lowercase();
    let mut existing_tracks = HashMap::new();
    for s in &existing {
        if s.owned && s.title.to_ascii_lowercase().starts_with(&prefix) {
            let hydrated = load_playlist(client, &s.id).await?;
            existing_tracks.insert(s.id.clone(), hydrated.tracks);
        }
    }
    let matcher = build_matcher(custom_genres_path, &state.paths)?;
    let svc = EnrichmentService::new(
        Arc::clone(&state.store),
        ReqwestEnrichmentHttp::new()?,
        state.config.enrichment.clone(),
        matcher,
    );
    emit_progress(
        tx,
        ctx,
        0,
        pl.tracks.len().max(1),
        format!("Enriching {} tracks...", pl.tracks.len()),
    );
    let plan = plan_genre_split(&pl, &existing, &existing_tracks, &svc, min_tracks).await?;
    Ok((pl, plan))
}
