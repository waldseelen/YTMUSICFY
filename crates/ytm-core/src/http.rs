use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use crate::error::CoreError;
use crate::traits::HttpTransport;

pub const YTM_BASE_API: &str = "https://music.youtube.com/youtubei/v1/";
pub const YTM_PARAMS_BROWSER: &str =
    "?alt=json&prettyPrint=false&key=REDACTED_INNERTUBE_KEY";

/// Production `reqwest` (`rustls-tls`) transport with HTTP/2 header headroom and backoff.
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
    base_url: String,
    max_retries: usize,
}

impl ReqwestTransport {
    /// Construct a `rustls-tls` HTTP client configured for YouTube Music.
    pub fn new(max_retries: usize) -> Result<Self, CoreError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| CoreError::HttpError(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            base_url: YTM_BASE_API.to_string(),
            max_retries,
        })
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
        500u64.saturating_mul(exp).min(16_000)
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
        let clean_ep = endpoint.trim_start_matches('/');
        let url = format!("{}{clean_ep}{YTM_PARAMS_BROWSER}", self.base_url);
        let header_map = Self::build_header_map(headers)?;

        let mut attempt = 0usize;
        loop {
            let resp = self
                .client
                .post(&url)
                .headers(header_map.clone())
                .json(body)
                .send()
                .await
                .map_err(|e| CoreError::HttpError(e.to_string()))?;

            let status = resp.status();
            let status_code = status.as_u16();
            if status_code == 429 || status_code == 503 {
                let delay = Self::retry_delay_ms(attempt, resp.headers().get("retry-after"));
                if attempt >= self.max_retries {
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
