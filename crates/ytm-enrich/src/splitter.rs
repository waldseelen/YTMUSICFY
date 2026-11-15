use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tracing::debug;
use ytm_core::{Playlist, PlaylistSummary, Track};
use ytm_store::{CachedArtistGenre, CachedTrackEnrichment, EnrichmentCacheStore};

use crate::error::EnrichError;
use crate::models::{
    EnrichedTrackMetadata, EnrichmentConfig, EnrichmentSource, GenreBucket, GenreSplitPlan,
};
use crate::providers::{
    EnrichmentHttp, ProviderPayload, build_musicbrainz_url, parse_deezer_album_response,
    parse_lastfm_response, parse_listenbrainz_response, parse_musicbrainz_recording_response,
    percent_encode,
};
use crate::rate_limit::PacedLimiter;
use crate::taxonomy::GenreMatcher;
use crate::traits::MetadataProvider;

/// Metadata enrichment orchestrator checking SQLite TTL caches before querying remote providers.
pub struct EnrichmentService<C, H> {
    cache: Arc<C>,
    http: H,
    config: EnrichmentConfig,
    matcher: GenreMatcher,
    mb_limiter: PacedLimiter,
}

impl<C, H> EnrichmentService<C, H> {
    /// Constructs a new enrichment service wrapping a persistent cache store and HTTP transport.
    #[must_use]
    pub fn new(cache: Arc<C>, http: H, config: EnrichmentConfig, matcher: GenreMatcher) -> Self {
        let mb_limiter = PacedLimiter::new(Duration::from_millis(config.musicbrainz_interval_ms));
        Self {
            cache,
            http,
            config,
            matcher,
            mb_limiter,
        }
    }

    #[must_use]
    pub fn matcher(&self) -> &GenreMatcher {
        &self.matcher
    }
}

fn normalize_key_component(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn build_cache_key(track: &Track) -> (String, String, String) {
    let artist = track.primary_artist_name().unwrap_or_default().trim();
    let title = track.title.trim();
    let artist_norm = normalize_key_component(artist);
    let title_norm = normalize_key_component(title);
    let key = format!("{artist_norm}::{title_norm}");
    (key, artist_norm, title_norm)
}

#[async_trait]
impl<C, H> MetadataProvider for EnrichmentService<C, H>
where
    C: EnrichmentCacheStore + 'static,
    H: EnrichmentHttp + 'static,
{
    async fn enrich_track(&self, track: &Track) -> Result<EnrichedTrackMetadata, EnrichError> {
        let (cache_key, artist_norm, _title_norm) = build_cache_key(track);
        let now_ts = Utc::now().timestamp();
        let ttl_secs =
            i64::try_from(self.config.cache_ttl_days.saturating_mul(86_400)).unwrap_or(i64::MAX);

        if let Some(cached) = self.cache.get_track_enrichment(&cache_key).await?
            && now_ts.saturating_sub(cached.updated_at) <= ttl_secs
        {
            let primary_genre = if cached.tags.is_empty() {
                cached.primary_genre
            } else {
                self.matcher.classify(&cached.tags)
            };
            debug!(cache_key, primary_genre, "enrichment track cache hit");
            return Ok(EnrichedTrackMetadata {
                cache_key,
                raw_tags: cached.tags,
                primary_genre,
                release_year: cached.release_year.or(track.release_year),
                bpm: cached.bpm,
                source: EnrichmentSource::Cache,
            });
        }

        let artist_cached = if artist_norm.is_empty() {
            None
        } else {
            self.cache
                .get_artist_genre(&artist_norm)
                .await?
                .filter(|row| now_ts.saturating_sub(row.updated_at) <= ttl_secs)
        };

        let artist_display = track.primary_artist_name().unwrap_or_default().trim();
        let title_display = track.title.trim();

        let (mut payload, mut source) = self
            .query_musicbrainz(title_display, artist_display)
            .await?;

        if payload.raw_tags.is_empty()
            && let Some(ref artist_row) = artist_cached
            && !artist_row.raw_tags.is_empty()
        {
            payload.raw_tags = artist_row.raw_tags.clone();
            source = EnrichmentSource::Cache;
        }

        if payload.raw_tags.is_empty()
            && let Some(api_key) = &self.config.lastfm_api_key
            && !artist_display.is_empty()
        {
            let lfm = self
                .query_lastfm(artist_display, title_display, api_key)
                .await?;
            if !lfm.raw_tags.is_empty() {
                payload.raw_tags = lfm.raw_tags;
                source = EnrichmentSource::LastFm;
            }
        }

        if payload.raw_tags.is_empty() && self.config.enable_deezer_fallback {
            let dz = self.query_deezer(artist_display, title_display).await?;
            if !dz.raw_tags.is_empty() {
                payload.raw_tags = dz.raw_tags;
                if payload.release_year.is_none() {
                    payload.release_year = dz.release_year;
                }
                if payload.bpm.is_none() {
                    payload.bpm = dz.bpm;
                }
                source = EnrichmentSource::Deezer;
            }
        }

        if payload.raw_tags.is_empty() && self.config.enable_listenbrainz_fallback {
            let lb = self
                .query_listenbrainz(artist_display, title_display)
                .await?;
            if !lb.raw_tags.is_empty() {
                payload.raw_tags = lb.raw_tags;
                source = EnrichmentSource::ListenBrainz;
            }
        }

        let primary_genre = self.matcher.classify(&payload.raw_tags);
        let release_year = payload.release_year.or(track.release_year);

        let track_row = CachedTrackEnrichment {
            cache_key: cache_key.clone(),
            video_id: Some(track.video_id.clone()).filter(|id| !id.is_empty()),
            tags: payload.raw_tags.clone(),
            primary_genre: primary_genre.clone(),
            release_year,
            bpm: payload.bpm,
            source: source.as_str().to_string(),
            updated_at: now_ts,
        };
        self.cache.upsert_track_enrichment(&track_row).await?;

        if !artist_norm.is_empty() && !payload.raw_tags.is_empty() {
            let artist_row = CachedArtistGenre {
                artist_name_norm: artist_norm,
                raw_tags: payload.raw_tags.clone(),
                primary_genre: primary_genre.clone(),
                source: source.as_str().to_string(),
                updated_at: now_ts,
            };
            self.cache.upsert_artist_genre(&artist_row).await?;
        }

        Ok(EnrichedTrackMetadata {
            cache_key,
            raw_tags: payload.raw_tags,
            primary_genre,
            release_year,
            bpm: payload.bpm,
            source,
        })
    }
}

impl<C, H> EnrichmentService<C, H>
where
    H: EnrichmentHttp,
{
    async fn query_musicbrainz(
        &self,
        title: &str,
        artist: &str,
    ) -> Result<(ProviderPayload, EnrichmentSource), EnrichError> {
        self.mb_limiter.acquire().await;
        let url = build_musicbrainz_url(title, artist);
        let (status, body) = self
            .http
            .get_json(
                "musicbrainz",
                &url,
                Some(&self.config.musicbrainz_user_agent),
            )
            .await?;
        if status == 429 || status == 503 {
            return Err(EnrichError::RateLimited("musicbrainz".to_string()));
        }
        if !(200..300).contains(&status) {
            return Err(EnrichError::Http {
                provider: "musicbrainz".to_string(),
                detail: format!("unexpected HTTP status {status}"),
            });
        }
        let payload = parse_musicbrainz_recording_response(&body);
        Ok((payload, EnrichmentSource::MusicBrainz))
    }

    async fn query_lastfm(
        &self,
        artist: &str,
        title: &str,
        api_key: &str,
    ) -> Result<ProviderPayload, EnrichError> {
        let url = format!(
            "https://ws.audioscrobbler.com/2.0/?method=track.getTopTags&artist={}&track={}&api_key={}&format=json",
            percent_encode(artist),
            percent_encode(title),
            percent_encode(api_key),
        );
        let (status, body) = self.http.get_json("lastfm", &url, None).await?;
        if status == 429 {
            return Err(EnrichError::RateLimited("lastfm".to_string()));
        }
        if !(200..300).contains(&status) {
            return Ok(ProviderPayload::default());
        }
        Ok(parse_lastfm_response(&body))
    }

    async fn query_deezer(
        &self,
        artist: &str,
        title: &str,
    ) -> Result<ProviderPayload, EnrichError> {
        let query = if artist.is_empty() {
            title.to_string()
        } else {
            format!("{artist} {title}")
        };
        let search_url = format!("https://api.deezer.com/search?q={}", percent_encode(&query));
        let (status, body) = self.http.get_json("deezer", &search_url, None).await?;
        if status == 429 {
            return Err(EnrichError::RateLimited("deezer".to_string()));
        }
        if !(200..300).contains(&status) {
            return Ok(ProviderPayload::default());
        }

        if let Some(album_id) = body
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("album"))
            .and_then(|al| al.get("id"))
            .and_then(serde_json::Value::as_u64)
        {
            let album_url = format!("https://api.deezer.com/album/{album_id}");
            let (al_status, al_body) = self.http.get_json("deezer", &album_url, None).await?;
            if (200..300).contains(&al_status) {
                return Ok(parse_deezer_album_response(&al_body));
            }
        }
        Ok(ProviderPayload::default())
    }

    async fn query_listenbrainz(
        &self,
        artist: &str,
        title: &str,
    ) -> Result<ProviderPayload, EnrichError> {
        let url = format!(
            "https://api.listenbrainz.org/1/metadata/recording/?recording_name={}&artist_name={}",
            percent_encode(title),
            percent_encode(artist),
        );
        let (status, body) = self.http.get_json("listenbrainz", &url, None).await?;
        if status == 429 {
            return Err(EnrichError::RateLimited("listenbrainz".to_string()));
        }
        if !(200..300).contains(&status) {
            return Ok(ProviderPayload::default());
        }
        Ok(parse_listenbrainz_response(&body))
    }
}

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
