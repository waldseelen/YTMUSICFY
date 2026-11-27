use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::time::Duration;

use chrono::Utc;
use eframe::egui;
use ytm_core::{
    AlbumRef, BROWSE_LIKED_ALBUMS, BrowseCardKind, BrowsePage, InnerTubeClientTrait, Playlist,
    PlaylistPrivacy, Track,
};
use ytm_store::{CachedPlaylistRecord, SqliteStore};

use crate::worker::BackendState;
use crate::worker_exec::{
    cache_fetched_playlist, emit_progress, load_playlist_with_manifest, sync_library_cache,
};
use crate::worker_msgs::WorkerEvent;

/// Convert a fetched album `BrowsePage` into an indexed read-only `Playlist` entry.
pub(crate) fn album_page_to_indexed_playlist(album_id: &str, page: &BrowsePage) -> Playlist {
    let mut tracks: Vec<Track> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let all_raw = page
        .tracks
        .iter()
        .chain(page.sections.iter().flat_map(|s| s.tracks.iter()));
    for raw in all_raw {
        if !raw.video_id.is_empty() && !seen_ids.insert(raw.video_id.clone()) {
            continue;
        }
        let mut t = raw.clone();
        t.position = tracks.len();
        if t.album.is_none() && !page.title.trim().is_empty() {
            t.album = Some(AlbumRef {
                name: page.title.clone(),
                browse_id: Some(album_id.to_string()),
            });
        }
        if t.artists.is_empty() && !page.artists.is_empty() {
            t.artists = page.artists.clone();
        }
        tracks.push(t);
    }

    let artist_line = page
        .artists
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let title = if artist_line.is_empty() {
        page.title.clone()
    } else {
        format!("{} — {}", page.title, artist_line)
    };

    Playlist {
        id: album_id.to_string(),
        title,
        description: page.subtitle.clone(),
        privacy: PlaylistPrivacy::Private,
        owned: false,
        track_count: tracks.len(),
        tracks,
    }
}

async fn collect_liked_album_cards<C: InnerTubeClientTrait>(
    client: &C,
    cancel: &std::sync::atomic::AtomicBool,
) -> Vec<(String, String)> {
    let Ok(first) = client.get_browse_page(BROWSE_LIKED_ALBUMS).await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen_albums = HashSet::new();
    let mut seen_tokens = HashSet::new();

    let mut push_page = |page: &BrowsePage| {
        for card in page.sections.iter().flat_map(|s| s.cards.iter()) {
            if card.kind == BrowseCardKind::Album
                && let Some(bid) = card.browse_id.as_deref()
                && bid.starts_with("MPRE")
                && seen_albums.insert(bid.to_string())
            {
                out.push((bid.to_string(), card.title.clone()));
            }
        }
    };

    push_page(&first);
    let mut next_tok = first.continuation;
    let mut pages = 1usize;
    while let Some(tok) = next_tok {
        if cancel.load(Ordering::Relaxed) || pages >= 20 || !seen_tokens.insert(tok.clone()) {
            break;
        }
        let Ok(page) = client.get_browse_continuation(&tok).await else {
            break;
        };
        push_page(&page);
        next_tok = page.continuation;
        pages += 1;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    out
}

/// Spawn a cancellable background repertoire indexing job (playlists + saved albums).
pub(crate) fn spawn_index_all_job(
    state: &mut BackendState,
    missing_only: bool,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) {
    let Some(client) = state.client.as_ref().map(Arc::clone) else {
        let _ = tx.send(WorkerEvent::StatusToast {
            ok: false,
            message: "Not authenticated: sign in under Settings & Auth first.".to_string(),
        });
        return;
    };
    let (job_id, cancel) = state.allocate_job();
    let store = Arc::clone(&state.store);
    let tx = tx.clone();
    let ctx = ctx.clone();
    let _ = tx.send(WorkerEvent::JobStarted {
        job_id,
        label: "Index Library & Albums".to_string(),
    });

    tokio::spawn(async move {
        run_index_all(client.as_ref(), &store, missing_only, &cancel, &tx, &ctx).await;
        let _ = tx.send(WorkerEvent::JobEnded(job_id));
        ctx.request_repaint();
    });
}

async fn run_index_all<C: InnerTubeClientTrait>(
    client: &C,
    store: &SqliteStore,
    missing_only: bool,
    cancel: &std::sync::atomic::AtomicBool,
    tx: &Sender<WorkerEvent>,
    ctx: &egui::Context,
) {
    let cached_ids: HashSet<String> = if missing_only {
        store
            .load_all_cached_playlists()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.id)
            .collect()
    } else {
        HashSet::new()
    };

    let libs = match client.get_library_playlists().await {
        Ok(l) => l,
        Err(e) => {
            let _ = tx.send(WorkerEvent::StatusToast {
                ok: false,
                message: format!("Repertoire index failed: {e}"),
            });
            return;
        }
    };
    sync_library_cache(store, &libs);
    let _ = tx.send(WorkerEvent::LibraryLoaded(Ok(libs.clone())));
    let album_cards = collect_liked_album_cards(client, cancel).await;
    if !cancel.load(Ordering::Relaxed) {
        let live_album_ids: Vec<String> = album_cards.iter().map(|(id, _)| id.clone()).collect();
        let _ = store.prune_missing_cached_albums(&live_album_ids);
    }

    let pl_targets: Vec<_> = libs
        .into_iter()
        .filter(|p| !missing_only || !cached_ids.contains(&p.id))
        .collect();
    let album_targets: Vec<_> = album_cards
        .into_iter()
        .filter(|(id, _)| !missing_only || !cached_ids.contains(id))
        .collect();

    let total = pl_targets.len() + album_targets.len();
    if total == 0 {
        let _ = tx.send(WorkerEvent::StatusToast {
            ok: true,
            message: "All library playlists and saved albums are already indexed locally."
                .to_string(),
        });
        return;
    }

    let mut indexed = 0usize;
    let mut step = 0usize;

    for sum in &pl_targets {
        if cancel.load(Ordering::Relaxed) {
            emit_cancelled(indexed, total, tx, ctx);
            return;
        }
        emit_progress(
            tx,
            ctx,
            step,
            total,
            format!("Indexing playlist {}/{total}: {}", step + 1, sum.title),
        );
        let result = match load_playlist_with_manifest(client, &sum.id).await {
            Ok(fetched) => {
                cache_fetched_playlist(store, &fetched, tx);
                indexed += 1;
                Ok(fetched.playlist)
            }
            Err(e) => Err(e.to_string()),
        };
        let _ = tx.send(WorkerEvent::PlaylistLoaded {
            playlist_id: sum.id.clone(),
            result,
        });
        step += 1;
        if step < total {
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
    }

    for (album_id, album_title) in &album_targets {
        if cancel.load(Ordering::Relaxed) {
            emit_cancelled(indexed, total, tx, ctx);
            return;
        }
        emit_progress(
            tx,
            ctx,
            step,
            total,
            format!("Indexing album {}/{total}: {album_title}", step + 1),
        );
        if let Ok(page) = client.get_browse_page(album_id).await {
            let pl = album_page_to_indexed_playlist(album_id, &page);
            let _ = store.upsert_cached_playlist_record(&CachedPlaylistRecord {
                playlist: pl.clone(),
                manifest: None,
                fetched_at: Utc::now().timestamp(),
            });
            let _ = tx.send(WorkerEvent::PlaylistLoaded {
                playlist_id: album_id.clone(),
                result: Ok(pl),
            });
            indexed += 1;
        }
        step += 1;
        if step < total {
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
    }

    emit_progress(tx, ctx, total, total, "Indexing complete".to_string());
    let _ = tx.send(WorkerEvent::StatusToast {
        ok: true,
        message: format!(
            "Indexed {indexed}/{total} item(s) (playlists + saved albums) locally for instant search."
        ),
    });
}

fn emit_cancelled(indexed: usize, total: usize, tx: &Sender<WorkerEvent>, ctx: &egui::Context) {
    emit_progress(tx, ctx, total, total, "Cancelled".to_string());
    let _ = tx.send(WorkerEvent::StatusToast {
        ok: true,
        message: format!("Indexing cancelled after {indexed}/{total} item(s)."),
    });
}

#[cfg(test)]
mod tests {
    use ytm_core::{ArtistRef, BrowsePageKind, TrackAvailability};

    use super::*;

    #[test]
    fn album_page_to_indexed_playlist_backfills_album_and_artists() {
        let track = Track {
            video_id: "V1".to_string(),
            set_video_id: None,
            position: 0,
            title: "Night City".to_string(),
            artists: Vec::new(),
            album: None,
            duration_sec: Some(200),
            release_year: Some(2020),
            music_video_type: ytm_core::MusicVideoType::AudioTrack,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        };
        let page = BrowsePage {
            browse_id: "MPREb_123".to_string(),
            kind: BrowsePageKind::Album,
            title: "Cyberpunk 2077 OST".to_string(),
            subtitle: Some("Album • 2020".to_string()),
            thumbnail_url: None,
            artists: vec![ArtistRef {
                name: "Marcin Przybyłowicz".to_string(),
                browse_id: None,
            }],
            sections: Vec::new(),
            tracks: vec![track],
            continuation: None,
        };
        let pl = album_page_to_indexed_playlist("MPREb_123", &page);
        assert_eq!(pl.id, "MPREb_123");
        assert!(!pl.owned);
        assert_eq!(pl.tracks.len(), 1);
        assert_eq!(
            pl.tracks[0].album.as_ref().map(|a| a.name.as_str()),
            Some("Cyberpunk 2077 OST")
        );
        assert_eq!(
            pl.tracks[0].primary_artist_name(),
            Some("Marcin Przybyłowicz")
        );
    }
}
