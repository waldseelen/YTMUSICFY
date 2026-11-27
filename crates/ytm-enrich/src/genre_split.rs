use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

use ytm_core::{Playlist, PlaylistSummary, Track};

use crate::error::EnrichError;
use crate::models::{GenreBucket, GenreSplitPlan, TrackGenreInfo};
use crate::traits::MetadataProvider;

/// Computes an idempotent `GenreSplitPlan` partitioning tracks into `"{Playlist} - {Genre}"` buckets.
pub async fn plan_genre_split<P: MetadataProvider>(
    playlist: &Playlist,
    existing_playlists: &[PlaylistSummary],
    existing_tracks_by_playlist: &HashMap<String, Vec<Track>>,
    provider: &P,
    min_bucket_tracks: usize,
) -> Result<GenreSplitPlan, EnrichError> {
    plan_genre_split_with_progress(
        playlist,
        existing_playlists,
        existing_tracks_by_playlist,
        provider,
        min_bucket_tracks,
        |_, _, _| ControlFlow::Continue(()),
    )
    .await
}

/// Computes an idempotent `GenreSplitPlan` while reporting per-track progress and cancellation.
pub async fn plan_genre_split_with_progress<P, F>(
    playlist: &Playlist,
    existing_playlists: &[PlaylistSummary],
    existing_tracks_by_playlist: &HashMap<String, Vec<Track>>,
    provider: &P,
    min_bucket_tracks: usize,
    mut on_progress: F,
) -> Result<GenreSplitPlan, EnrichError>
where
    P: MetadataProvider,
    F: FnMut(usize, usize, &str) -> ControlFlow<()>,
{
    let mut grouped: HashMap<String, Vec<Track>> = HashMap::new();
    let mut track_details: HashMap<String, TrackGenreInfo> = HashMap::new();
    let valid_total = playlist
        .tracks
        .iter()
        .filter(|t| !t.video_id.is_empty())
        .count();
    let mut processed = 0usize;

    for track in &playlist.tracks {
        if track.video_id.is_empty() {
            continue;
        }
        if matches!(
            on_progress(processed, valid_total, &track.title),
            ControlFlow::Break(())
        ) {
            return Err(EnrichError::Cancelled);
        }
        let enriched = provider.enrich_track(track).await?;
        track_details.insert(
            track.video_id.clone(),
            TrackGenreInfo {
                primary_genre: enriched.primary_genre.clone(),
                raw_tags: enriched.raw_tags,
                source: enriched.source,
                release_year: enriched.release_year,
            },
        );
        grouped
            .entry(enriched.primary_genre)
            .or_default()
            .push(track.clone());
        processed += 1;
    }

    let mut buckets = Vec::new();
    let mut skipped_buckets = Vec::new();

    for (genre, tracks) in grouped {
        let bucket = build_genre_bucket(
            &playlist.title,
            genre,
            tracks,
            existing_playlists,
            existing_tracks_by_playlist,
        );
        let bucket_total = bucket.tracks_to_add.len() + bucket.already_present_count;
        if bucket_total < min_bucket_tracks {
            skipped_buckets.push(bucket);
        } else {
            buckets.push(bucket);
        }
    }

    sort_buckets(&mut buckets);
    sort_buckets(&mut skipped_buckets);
    let skipped_small_buckets = skipped_buckets.len();

    Ok(GenreSplitPlan {
        source_playlist_id: playlist.id.clone(),
        source_playlist_title: playlist.title.clone(),
        buckets,
        skipped_small_buckets,
        skipped_buckets,
        track_details,
    })
}

fn build_genre_bucket(
    source_title: &str,
    genre: String,
    tracks: Vec<Track>,
    existing_playlists: &[PlaylistSummary],
    existing_tracks_by_playlist: &HashMap<String, Vec<Track>>,
) -> GenreBucket {
    let target_playlist_title = format!("{source_title} - {genre}");
    let existing_playlist_id = existing_playlists
        .iter()
        .find(|p| p.owned && p.title.eq_ignore_ascii_case(&target_playlist_title))
        .map(|p| p.id.clone());

    let existing_ids: HashSet<&str> = existing_playlist_id
        .as_ref()
        .and_then(|id| existing_tracks_by_playlist.get(id))
        .map(|existing| {
            existing
                .iter()
                .filter(|t| !t.video_id.is_empty())
                .map(|t| t.video_id.as_str())
                .collect()
        })
        .unwrap_or_default();

    let mut tracks_to_add = Vec::new();
    let mut already_present_count = 0usize;
    let mut seen_in_source = HashSet::new();

    for track in tracks {
        if !seen_in_source.insert(track.video_id.clone()) {
            continue;
        }
        if existing_ids.contains(track.video_id.as_str()) {
            already_present_count += 1;
        } else {
            tracks_to_add.push(track);
        }
    }

    GenreBucket {
        genre,
        target_playlist_title,
        existing_playlist_id,
        tracks_to_add,
        already_present_count,
    }
}

fn sort_buckets(buckets: &mut [GenreBucket]) {
    buckets.sort_by(|a, b| {
        let total_a = a.tracks_to_add.len() + a.already_present_count;
        let total_b = b.tracks_to_add.len() + b.already_present_count;
        total_b.cmp(&total_a).then_with(|| a.genre.cmp(&b.genre))
    });
}
