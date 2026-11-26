use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::auth::{current_unix_timestamp, refresh_sapisidhash_in_headers};
use crate::error::CoreError;
use crate::traits::HttpTransport;

pub const YTM_BASE_API: &str = "https://music.youtube.com/youtubei/v1/";
pub const YTM_PARAMS_BASE: &str = "?alt=json&prettyPrint=false";
const YTM_HOME_URL: &str = "https://music.youtube.com/";
const WEB_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// Retry safety classification for outbound InnerTube HTTP calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RetryPolicy {
    /// Safe to retry on transient transport errors, timeouts, `429`, and `503`.
    #[default]
    IdempotentRead,
    /// Non-idempotent mutation: retry only explicit pre-execution rejections (`429`),
    /// never blind-retry ambiguous timeouts, transport errors, or `503`.
    MutationSafe,
}

fn env_innertube_key() -> Option<String> {
    if let Ok(key) = std::env::var("YTM_INNERTUBE_KEY") {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(key) = option_env!("YTM_INNERTUBE_KEY") {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn extract_innertube_api_key(html: &str) -> Option<String> {
    let marker = "\"INNERTUBE_API_KEY\":\"";
    let start = html.find(marker)? + marker.len();
    let rest = &html[start..];
    let end = rest.find('"')?;
    let key = rest[..end].trim();
    (!key.is_empty()).then(|| key.to_string())
}

/// Apply `±25%` jitter to `base_ms` using sub-second clock entropy and retry attempt.
#[must_use]
pub fn jittered_delay_ms(base_ms: u64, attempt: usize) -> u64 {
    if base_ms == 0 {
        return 0;
    }
    let nanos = u64::from(Utc::now().timestamp_subsec_nanos());
    let mixed = nanos
        .wrapping_add((attempt as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let factor_milli = 750 + (mixed % 501);
    base_ms.saturating_mul(factor_milli) / 1000
}

/// Production `reqwest` (`rustls-tls`) transport with HTTP/2 header headroom and backoff.
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
    base_url: String,
    key_discovery_url: Option<String>,
    max_retries: usize,
    api_key: Arc<RwLock<Option<String>>>,
}

impl ReqwestTransport {
    /// Construct a `rustls-tls` HTTP client configured for YouTube Music.
    pub fn new(max_retries: usize) -> Result<Self, CoreError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            // Workspace `reqwest` omits the `http2` feature; enforce HTTP/1.1 fallback (MEMORY.md Trap 10)
            // so response headers exceeding HTTP/2's 16 KiB header list limit never fail.
            .http1_only()
            .build()
            .map_err(|e| CoreError::HttpError(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            base_url: YTM_BASE_API.to_string(),
            key_discovery_url: Some(YTM_HOME_URL.to_string()),
            max_retries,
            api_key: Arc::new(RwLock::new(None)),
        })
    }

    #[cfg(test)]
    pub(crate) fn with_test_urls(
        base_url: String,
        key_discovery_url: Option<String>,
        max_retries: usize,
    ) -> Result<Self, CoreError> {
        let mut transport = Self::new(max_retries)?;
        transport.base_url = base_url;
        transport.key_discovery_url = key_discovery_url;
        Ok(transport)
    }

    async fn resolve_params(&self) -> String {
        if let Some(key) = env_innertube_key() {
            return format!("{YTM_PARAMS_BASE}&key={key}");
        }
        let Some(discovery_url) = &self.key_discovery_url else {
            return YTM_PARAMS_BASE.to_string();
        };
        if let Some(key) = self.api_key.read().await.as_deref() {
            return format!("{YTM_PARAMS_BASE}&key={key}");
        }

        let fetched = match self
            .client
            .get(discovery_url)
            .header("User-Agent", WEB_UA)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => resp
                .text()
                .await
                .ok()
                .and_then(|html| extract_innertube_api_key(&html)),
            _ => None,
        };

        // Never cache `None`: transient offline errors must allow recovery on subsequent requests.
        if let Some(key) = fetched {
            *self.api_key.write().await = Some(key.clone());
            format!("{YTM_PARAMS_BASE}&key={key}")
        } else {
            YTM_PARAMS_BASE.to_string()
        }
    }

    fn build_header_map(headers: &[(String, String)]) -> Result<HeaderMap, CoreError> {
        let mut map = HeaderMap::new();
        for (k, v) in headers {
            let name = HeaderName::from_bytes(k.as_bytes()).map_err(|e| {
                CoreError::InvalidAuthInput(format!("invalid header name '{k}': {e}"))
            })?;
            let val = HeaderValue::from_str(v).map_err(|e| {
                CoreError::InvalidAuthInput(format!("invalid header value for '{k}': {e}"))
            })?;
            map.insert(name, val);
        }
        Ok(map)
    }

    fn retry_delay_ms(attempt: usize, retry_after_header: Option<&HeaderValue>) -> u64 {
        if let Some(secs) = retry_after_header
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
        {
            return secs.saturating_mul(1000).max(500);
        }
        let exp = 2u64.saturating_pow(u32::try_from(attempt).unwrap_or(4));
        let base = 500u64.saturating_mul(exp).min(16_000);
        jittered_delay_ms(base, attempt)
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn post_json(
        &self,
        endpoint: &str,
        body: &Value,
        headers: &[(String, String)],
    ) -> Result<Value, CoreError> {
        self.post_json_with_policy(endpoint, body, headers, RetryPolicy::IdempotentRead)
            .await
    }

    async fn post_json_with_policy(
        &self,
        endpoint: &str,
        body: &Value,
        headers: &[(String, String)],
        policy: RetryPolicy,
    ) -> Result<Value, CoreError> {
        let clean_ep = endpoint.trim_start_matches('/');
        let params = self.resolve_params().await;
        let url = format!("{}{clean_ep}{params}", self.base_url);
        let mut req_headers = headers.to_vec();

        let mut attempt = 0usize;
        loop {
            if attempt > 0 {
                let _ = refresh_sapisidhash_in_headers(&mut req_headers, current_unix_timestamp());
            }
            let header_map = Self::build_header_map(&req_headers)?;
            let send_res = self
                .client
                .post(&url)
                .headers(header_map)
                .json(body)
                .send()
                .await;

            let resp = match send_res {
                Ok(r) => r,
                Err(e) => {
                    if policy == RetryPolicy::IdempotentRead && attempt < self.max_retries {
                        let delay = Self::retry_delay_ms(attempt, None);
                        tracing::warn!(error = %e, delay_ms = delay, "idempotent read retry");
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(CoreError::HttpError(e.to_string()));
                }
            };

            let status = resp.status();
            let status_code = status.as_u16();
            if status_code == 429 || status_code == 503 {
                let retry_allowed = status_code == 429 || policy == RetryPolicy::IdempotentRead;
                let delay = Self::retry_delay_ms(attempt, resp.headers().get("retry-after"));
                if !retry_allowed || attempt >= self.max_retries {
                    return Err(CoreError::RateLimited {
                        status: status_code,
                        retry_after_ms: Some(delay),
                    });
                }
                tracing::warn!(status = status_code, delay_ms = delay, "InnerTube backoff");
                tokio::time::sleep(Duration::from_millis(delay)).await;
                attempt += 1;
                continue;
            }

            if !status.is_success() {
                let body_txt = resp.text().await.unwrap_or_default();
                return Err(CoreError::HttpError(format!(
                    "HTTP {status_code} on {clean_ep}: {body_txt}"
                )));
            }

            let json: Value = resp.json().await.map_err(|e| CoreError::ParseError {
                context: clean_ep.to_string(),
                detail: e.to_string(),
            })?;
            return Ok(json);
        }
    }

    async fn get_text(&self, url: &str, headers: &[(String, String)]) -> Result<String, CoreError> {
        let header_map = Self::build_header_map(headers)?;
        let resp = self
            .client
            .get(url)
            .headers(header_map)
            .send()
            .await
            .map_err(|e| CoreError::HttpError(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(CoreError::HttpError(format!(
                "GET {url} returned HTTP {}",
                status.as_u16()
            )));
        }

        resp.text()
            .await
            .map_err(|e| CoreError::HttpError(e.to_string()))
    }
}

/// Recorded outbound request captured by [`MockTransport`].
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub endpoint: String,
    pub body: Value,
    pub headers: Vec<(String, String)>,
}

/// Deterministic queue-backed mock transport for unit and integration tests.
#[derive(Debug, Clone, Default)]
pub struct MockTransport {
    responses: Arc<Mutex<VecDeque<Result<Value, CoreError>>>>,
    recorded: Arc<Mutex<Vec<RecordedRequest>>>,
}

impl MockTransport {
    #[must_use]
    pub fn with_responses(responses: Vec<Result<Value, CoreError>>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
            recorded: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Push an extra mock response onto the FIFO queue.
    pub fn push_ok(&self, val: Value) {
        self.responses.lock().expect("mock lock").push_back(Ok(val));
    }

    /// Return all recorded POST requests in invocation order.
    #[must_use]
    pub fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.recorded.lock().expect("mock lock").clone()
    }
}

#[async_trait]
impl HttpTransport for MockTransport {
    async fn post_json(
        &self,
        endpoint: &str,
        body: &Value,
        headers: &[(String, String)],
    ) -> Result<Value, CoreError> {
        self.recorded
            .lock()
            .expect("mock lock")
            .push(RecordedRequest {
                endpoint: endpoint.to_string(),
                body: body.clone(),
                headers: headers.to_vec(),
            });

        self.responses
            .lock()
            .expect("mock lock")
            .pop_front()
            .unwrap_or_else(|| {
                Err(CoreError::HttpError(format!(
                    "MockTransport queue exhausted on '{endpoint}'"
                )))
            })
    }

    async fn get_text(
        &self,
        url: &str,
        _headers: &[(String, String)],
    ) -> Result<String, CoreError> {
        Err(CoreError::HttpError(format!(
            "MockTransport does not support GET '{url}'"
        )))
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
