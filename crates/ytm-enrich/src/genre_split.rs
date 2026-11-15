use std::collections::{HashMap, HashSet};

use ytm_core::{Playlist, PlaylistSummary, Track};

use crate::error::EnrichError;
use crate::models::{GenreBucket, GenreSplitPlan};
use crate::traits::MetadataProvider;

/// Computes an idempotent `GenreSplitPlan` partitioning tracks into `"{Playlist} - {Genre}"` buckets.
pub async fn plan_genre_split<P: MetadataProvider>(
    playlist: &Playlist,
    existing_playlists: &[PlaylistSummary],
    existing_tracks_by_playlist: &HashMap<String, Vec<Track>>,
    provider: &P,
    min_bucket_tracks: usize,
) -> Result<GenreSplitPlan, EnrichError> {
    let mut grouped: HashMap<String, Vec<Track>> = HashMap::new();

    for track in &playlist.tracks {
        if track.video_id.is_empty() {
            continue;
        }
        let enriched = provider.enrich_track(track).await?;
        grouped
            .entry(enriched.primary_genre)
            .or_default()
            .push(track.clone());
    }

    let mut buckets = Vec::new();
    let mut skipped_small_buckets = 0usize;

    for (genre, tracks) in grouped {
        if tracks.len() < min_bucket_tracks {
            skipped_small_buckets += 1;
            continue;
        }

        let target_playlist_title = format!("{} - {}", playlist.title, genre);
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

        buckets.push(GenreBucket {
            genre,
            target_playlist_title,
            existing_playlist_id,
            tracks_to_add,
            already_present_count,
        });
    }

    buckets.sort_by(|a, b| {
        let total_a = a.tracks_to_add.len() + a.already_present_count;
        let total_b = b.tracks_to_add.len() + b.already_present_count;
        total_b.cmp(&total_a).then_with(|| a.genre.cmp(&b.genre))
    });

    Ok(GenreSplitPlan {
        source_playlist_id: playlist.id.clone(),
        source_playlist_title: playlist.title.clone(),
        buckets,
        skipped_small_buckets,
    })
}
