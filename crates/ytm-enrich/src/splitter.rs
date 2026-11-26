use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use tracing::{debug, warn};
use ytm_core::Track;
use ytm_store::{CachedArtistGenre, CachedTrackEnrichment, EnrichmentCacheStore};

use crate::error::EnrichError;
pub use crate::genre_split::plan_genre_split;
use crate::models::{EnrichedTrackMetadata, EnrichmentConfig, EnrichmentSource};
pub use crate::normalize::build_cache_key;
use crate::normalize::normalized_track_parts;
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

#[async_trait]
impl<C, H> MetadataProvider for EnrichmentService<C, H>
where
    C: EnrichmentCacheStore + 'static,
    H: EnrichmentHttp + 'static,
{
    async fn enrich_track(&self, track: &Track) -> Result<EnrichedTrackMetadata, EnrichError> {
        let parts = normalized_track_parts(track);
        let cache_key = parts.cache_key;
        let artist_norm = parts.artist_norm;
        let artist_display = parts.artist_display;
        let title_display = parts.title_display;
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

        if self.config.artist_cache_first
            && let Some(ref artist_row) = artist_cached
            && !artist_row.raw_tags.is_empty()
        {
            let payload = ProviderPayload {
                raw_tags: artist_row.raw_tags.clone(),
                ..Default::default()
            };
            return self
                .finish_and_cache(
                    track,
                    (&cache_key, ""),
                    payload,
                    EnrichmentSource::Cache,
                    true,
                    now_ts,
                )
                .await;
        }

        let (mut payload, mut source, mb_degraded) = match self
            .query_musicbrainz(&title_display, &artist_display)
            .await
        {
            Ok((p, src)) => (p, src, false),
            Err(err) => {
                warn!(
                    artist = artist_display,
                    title = title_display,
                    error = %err,
                    "musicbrainz lookup failed; falling back to cache/secondary providers"
                );
                (
                    ProviderPayload::default(),
                    EnrichmentSource::MusicBrainz,
                    true,
                )
            }
        };

        if payload.raw_tags.is_empty()
            && let Some(ref artist_row) = artist_cached
            && !artist_row.raw_tags.is_empty()
        {
            payload.raw_tags = artist_row.raw_tags.clone();
            source = EnrichmentSource::Cache;
        }

        self.apply_secondary_fallbacks(&artist_display, &title_display, &mut payload, &mut source)
            .await;

        let save_track = !mb_degraded || !payload.raw_tags.is_empty();
        self.finish_and_cache(
            track,
            (&cache_key, &artist_norm),
            payload,
            source,
            save_track,
            now_ts,
        )
        .await
    }
}

impl<C, H> EnrichmentService<C, H>
where
    C: EnrichmentCacheStore + 'static,
    H: EnrichmentHttp + 'static,
{
    async fn finish_and_cache(
        &self,
        track: &Track,
        (cache_key, artist_norm): (&str, &str),
        payload: ProviderPayload,
        source: EnrichmentSource,
        save_track: bool,
        now_ts: i64,
    ) -> Result<EnrichedTrackMetadata, EnrichError> {
        let primary_genre = self.matcher.classify(&payload.raw_tags);
        let release_year = payload.release_year.or(track.release_year);

        if save_track {
            let track_row = CachedTrackEnrichment {
                cache_key: cache_key.to_string(),
                video_id: Some(track.video_id.clone()).filter(|id| !id.is_empty()),
                tags: payload.raw_tags.clone(),
                primary_genre: primary_genre.clone(),
                release_year,
                bpm: payload.bpm,
                source: source.as_str().to_string(),
                updated_at: now_ts,
            };
            self.cache.upsert_track_enrichment(&track_row).await?;
        }

        if !artist_norm.is_empty() && !payload.raw_tags.is_empty() {
            let artist_row = CachedArtistGenre {
                artist_name_norm: artist_norm.to_string(),
                raw_tags: payload.raw_tags.clone(),
                primary_genre: primary_genre.clone(),
                source: source.as_str().to_string(),
                updated_at: now_ts,
            };
            self.cache.upsert_artist_genre(&artist_row).await?;
        }

        Ok(EnrichedTrackMetadata {
            cache_key: cache_key.to_string(),
            raw_tags: payload.raw_tags,
            primary_genre,
            release_year,
            bpm: payload.bpm,
            source,
        })
    }

    async fn try_tags(
        &self,
        provider: &str,
        url: &str,
        parse: fn(&Value) -> ProviderPayload,
    ) -> Option<Vec<String>> {
        match self.fetch_optional_json(provider, url).await {
            Ok(Some(body)) => Some(parse(&body).raw_tags).filter(|t| !t.is_empty()),
            Ok(None) => None,
            Err(err) => {
                warn!(error = %err, "{provider} fallback failed");
                None
            }
        }
    }

    async fn apply_secondary_fallbacks(
        &self,
        artist: &str,
        title: &str,
        payload: &mut ProviderPayload,
        source: &mut EnrichmentSource,
    ) {
        if payload.raw_tags.is_empty()
            && let Some(api_key) = &self.config.lastfm_api_key
            && !artist.is_empty()
        {
            let url = format!(
                "https://ws.audioscrobbler.com/2.0/?method=track.getTopTags&artist={}&track={}&api_key={}&format=json",
                percent_encode(artist),
                percent_encode(title),
                percent_encode(api_key),
            );
            if let Some(tags) = self.try_tags("lastfm", &url, parse_lastfm_response).await {
                payload.raw_tags = tags;
                *source = EnrichmentSource::LastFm;
            }
        }

        if payload.raw_tags.is_empty() && self.config.enable_deezer_fallback {
            match self.query_deezer(artist, title).await {
                Ok(dz) if !dz.raw_tags.is_empty() => {
                    payload.raw_tags = dz.raw_tags;
                    payload.release_year = payload.release_year.or(dz.release_year);
                    payload.bpm = payload.bpm.or(dz.bpm);
                    *source = EnrichmentSource::Deezer;
                }
                Ok(_) => {}
                Err(err) => warn!(error = %err, "deezer fallback failed"),
            }
        }

        if payload.raw_tags.is_empty() && self.config.enable_listenbrainz_fallback {
            let url = format!(
                "https://api.listenbrainz.org/1/metadata/recording/?recording_name={}&artist_name={}",
                percent_encode(title),
                percent_encode(artist),
            );
            if let Some(tags) = self
                .try_tags("listenbrainz", &url, parse_listenbrainz_response)
                .await
            {
                payload.raw_tags = tags;
                *source = EnrichmentSource::ListenBrainz;
            }
        }
    }

    async fn query_musicbrainz(
        &self,
        title: &str,
        artist: &str,
    ) -> Result<(ProviderPayload, EnrichmentSource), EnrichError> {
        const MAX_ATTEMPTS: usize = 3;
        let url = build_musicbrainz_url(title, artist);
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 && self.config.musicbrainz_interval_ms > 0 {
                let backoff = self
                    .config
                    .musicbrainz_interval_ms
                    .saturating_mul((attempt as u64) + 1);
                tokio::time::sleep(Duration::from_millis(backoff)).await;
            }
            self.mb_limiter.acquire().await;
            let (status, body) = self
                .http
                .get_json(
                    "musicbrainz",
                    &url,
                    Some(&self.config.musicbrainz_user_agent),
                )
                .await?;
            if status == 429 || status == 503 {
                if attempt + 1 < MAX_ATTEMPTS {
                    continue;
                }
                return Err(EnrichError::RateLimited("musicbrainz".to_string()));
            }
            if !(200..300).contains(&status) {
                return Err(EnrichError::Http {
                    provider: "musicbrainz".to_string(),
                    detail: format!("unexpected HTTP status {status}"),
                });
            }
            return Ok((
                parse_musicbrainz_recording_response(&body),
                EnrichmentSource::MusicBrainz,
            ));
        }
        Err(EnrichError::RateLimited("musicbrainz".to_string()))
    }

    async fn fetch_optional_json(
        &self,
        provider: &str,
        url: &str,
    ) -> Result<Option<Value>, EnrichError> {
        let (status, body) = self.http.get_json(provider, url, None).await?;
        if status == 429 {
            return Err(EnrichError::RateLimited(provider.to_string()));
        }
        Ok((200..300).contains(&status).then_some(body))
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
        let Some(body) = self.fetch_optional_json("deezer", &search_url).await? else {
            return Ok(ProviderPayload::default());
        };
        if let Some(album_id) = body
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("album"))
            .and_then(|al| al.get("id"))
            .and_then(Value::as_u64)
            && let Some(al_body) = self
                .fetch_optional_json(
                    "deezer",
                    &format!("https://api.deezer.com/album/{album_id}"),
                )
                .await?
        {
            return Ok(parse_deezer_album_response(&al_body));
        }
        Ok(ProviderPayload::default())
    }
}
