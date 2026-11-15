use std::collections::{HashSet, VecDeque};
use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

use crate::error::EnrichError;

/// Mockable async HTTP transport returning `(status_code, json_body)` for enrichment APIs.
#[async_trait]
pub trait EnrichmentHttp: Send + Sync {
    async fn get_json(
        &self,
        provider: &str,
        url: &str,
        user_agent: Option<&str>,
    ) -> Result<(u16, Value), EnrichError>;
}

/// Real `reqwest`-backed HTTP client for MusicBrainz, Last.fm, Deezer, and ListenBrainz.
#[derive(Debug, Clone)]
pub struct ReqwestEnrichmentHttp {
    client: Client,
}

impl ReqwestEnrichmentHttp {
    /// Builds a rustls HTTP client with a 15-second timeout.
    pub fn new() -> Result<Self, EnrichError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| EnrichError::Http {
                provider: "reqwest".to_string(),
                detail: e.to_string(),
            })?;
        Ok(Self { client })
    }
}

#[async_trait]
impl EnrichmentHttp for ReqwestEnrichmentHttp {
    async fn get_json(
        &self,
        provider: &str,
        url: &str,
        user_agent: Option<&str>,
    ) -> Result<(u16, Value), EnrichError> {
        let mut req = self.client.get(url).header("Accept", "application/json");
        if let Some(ua) = user_agent {
            req = req.header("User-Agent", ua);
        }
        let resp = req.send().await.map_err(|e| EnrichError::Http {
            provider: provider.to_string(),
            detail: e.to_string(),
        })?;
        let status = resp.status().as_u16();
        let body_text = resp.text().await.map_err(|e| EnrichError::Http {
            provider: provider.to_string(),
            detail: e.to_string(),
        })?;
        let json: Value = match serde_json::from_str(&body_text) {
            Ok(val) => val,
            Err(_) => Value::String(body_text),
        };
        Ok((status, json))
    }
}

/// Deterministic scripted HTTP double capturing requested URLs for offline tests.
#[derive(Debug, Default)]
pub struct MockEnrichmentHttp {
    responses: Mutex<VecDeque<Result<(u16, Value), EnrichError>>>,
    recorded_urls: Mutex<Vec<String>>,
}

impl MockEnrichmentHttp {
    /// Initializes a scripted response queue.
    #[must_use]
    pub fn with_responses(responses: Vec<Result<(u16, Value), EnrichError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            recorded_urls: Mutex::new(Vec::new()),
        }
    }

    /// Returns all requested URLs in chronological order.
    #[must_use]
    pub fn recorded_urls(&self) -> Vec<String> {
        self.recorded_urls
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl EnrichmentHttp for MockEnrichmentHttp {
    async fn get_json(
        &self,
        provider: &str,
        url: &str,
        _user_agent: Option<&str>,
    ) -> Result<(u16, Value), EnrichError> {
        if let Ok(mut urls) = self.recorded_urls.lock() {
            urls.push(url.to_string());
        }
        let mut queue = self.responses.lock().map_err(|_| EnrichError::Http {
            provider: provider.to_string(),
            detail: "mock queue mutex poisoned".to_string(),
        })?;
        queue.pop_front().ok_or_else(|| EnrichError::Http {
            provider: provider.to_string(),
            detail: format!("no scripted mock response remaining for {url}"),
        })?
    }
}

/// Extracted metadata payload returned by a remote provider.
#[derive(Debug, Clone, Default)]
pub struct ProviderPayload {
    pub raw_tags: Vec<String>,
    pub release_year: Option<u16>,
    pub bpm: Option<f32>,
}

pub(crate) fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for b in input.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(*b as char);
        } else if *b == b' ' {
            out.push_str("%20");
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

pub(crate) fn build_musicbrainz_url(title: &str, artist: &str) -> String {
    let lucene = if artist.trim().is_empty() {
        format!("recording:\"{}\"", title.replace('"', " "))
    } else {
        format!(
            "recording:\"{}\" AND artist:\"{}\"",
            title.replace('"', " "),
            artist.replace('"', " ")
        )
    };
    format!(
        "https://musicbrainz.org/ws/2/recording/?query={}&fmt=json&limit=3",
        percent_encode(&lucene)
    )
}

pub(crate) fn parse_musicbrainz_recording_response(body: &Value) -> ProviderPayload {
    let mut tags_scored: Vec<(String, i64)> = Vec::new();
    let mut release_year: Option<u16> = None;

    if let Some(recordings) = body.get("recordings").and_then(Value::as_array) {
        for rec in recordings.iter().take(2) {
            if release_year.is_none()
                && let Some(date) = rec.get("first-release-date").and_then(Value::as_str)
            {
                release_year = parse_year_prefix(date);
            }
            collect_counted_tags(rec.get("tags"), &mut tags_scored);
            if let Some(credits) = rec.get("artist-credit").and_then(Value::as_array) {
                for credit in credits {
                    if let Some(artist) = credit.get("artist") {
                        collect_counted_tags(artist.get("tags"), &mut tags_scored);
                    }
                }
            }
            if !tags_scored.is_empty() {
                break;
            }
        }
    }

    tags_scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ProviderPayload {
        raw_tags: dedup_preserve_order(tags_scored.into_iter().map(|(t, _)| t)),
        release_year,
        bpm: None,
    }
}

pub(crate) fn parse_lastfm_response(body: &Value) -> ProviderPayload {
    let mut raw_tags = Vec::new();
    if let Some(tags) = body
        .get("toptags")
        .and_then(|v| v.get("tag"))
        .and_then(Value::as_array)
    {
        for tag in tags {
            if let Some(name) = tag.get("name").and_then(Value::as_str) {
                raw_tags.push(name.trim().to_string());
            }
        }
    }
    ProviderPayload {
        raw_tags: dedup_preserve_order(raw_tags.into_iter()),
        release_year: None,
        bpm: None,
    }
}

pub(crate) fn parse_deezer_album_response(body: &Value) -> ProviderPayload {
    let mut raw_tags = Vec::new();
    if let Some(genres) = body
        .get("genres")
        .and_then(|v| v.get("data"))
        .and_then(Value::as_array)
    {
        for g in genres {
            if let Some(name) = g.get("name").and_then(Value::as_str) {
                raw_tags.push(name.trim().to_string());
            }
        }
    }
    let release_year = body
        .get("release_date")
        .and_then(Value::as_str)
        .and_then(parse_year_prefix);
    let bpm = body
        .get("bpm")
        .and_then(Value::as_f64)
        .map(|v| v as f32)
        .filter(|&v| v > 0.0);

    ProviderPayload {
        raw_tags: dedup_preserve_order(raw_tags.into_iter()),
        release_year,
        bpm,
    }
}

pub(crate) fn parse_listenbrainz_response(body: &Value) -> ProviderPayload {
    let mut raw_tags = Vec::new();
    if let Some(arr) = body.as_array() {
        for entry in arr {
            if let Some(tags) = entry
                .get("tag")
                .and_then(|v| v.get("artist"))
                .and_then(Value::as_array)
            {
                for item in tags {
                    if let Some(tag) = item.get("tag").and_then(Value::as_str) {
                        raw_tags.push(tag.trim().to_string());
                    }
                }
            }
        }
    }
    ProviderPayload {
        raw_tags: dedup_preserve_order(raw_tags.into_iter()),
        release_year: None,
        bpm: None,
    }
}

fn collect_counted_tags(val: Option<&Value>, out: &mut Vec<(String, i64)>) {
    let Some(arr) = val.and_then(Value::as_array) else {
        return;
    };
    for item in arr {
        if let Some(name) = item.get("name").and_then(Value::as_str) {
            let count = item.get("count").and_then(Value::as_i64).unwrap_or(1);
            out.push((name.trim().to_string(), count));
        }
    }
}

fn parse_year_prefix(date_str: &str) -> Option<u16> {
    date_str.trim().get(0..4)?.parse::<u16>().ok()
}

fn dedup_preserve_order(iter: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in iter {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(trimmed.to_string());
        }
    }
    out
}
