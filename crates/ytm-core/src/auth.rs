use std::collections::HashMap;

use chrono::Utc;
use serde_json::Value;
use sha1::{Digest, Sha1};

use crate::error::CoreError;
use crate::models::AuthCredentials;

pub const YTM_ORIGIN: &str = "https://music.youtube.com";
const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0";

/// Returns current UNIX epoch timestamp in seconds.
#[must_use]
pub fn current_unix_timestamp() -> u64 {
    u64::try_from(Utc::now().timestamp()).unwrap_or(0)
}

/// Parse credentials from native `AuthCredentials` JSON, `ytmusicapi` `browser.json`,
/// raw browser DevTools HTTP headers, or a bare `Cookie:` string.
pub fn parse_auth_input(raw: &str) -> Result<AuthCredentials, CoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidAuthInput("empty auth input".to_string()));
    }

    let creds = if trimmed.starts_with('{') {
        parse_json_auth(trimmed)?
    } else {
        parse_header_text_auth(trimmed)?
    };

    // Validate immediately so broken cookies fail at import time rather than silently later.
    let _ = extract_sapisid(&creds.cookie)?;
    Ok(creds)
}

fn parse_json_auth(trimmed: &str) -> Result<AuthCredentials, CoreError> {
    if let Ok(mut native) = serde_json::from_str::<AuthCredentials>(trimmed) {
        if !native.cookie.trim().is_empty() {
            if native.user_agent.trim().is_empty() {
                native.user_agent = DEFAULT_USER_AGENT.to_string();
            }
            if native.x_goog_authuser.is_none() {
                native.x_goog_authuser = Some("0".to_string());
            }
            return Ok(native);
        }
    }

    let val: Value = serde_json::from_str(trimmed)
        .map_err(|e| CoreError::InvalidAuthInput(format!("invalid JSON: {e}")))?;
    let obj = val
        .as_object()
        .ok_or_else(|| CoreError::InvalidAuthInput("JSON auth must be an object".to_string()))?;

    let mut map = HashMap::with_capacity(obj.len());
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            map.insert(k.to_ascii_lowercase(), s.trim().to_string());
        }
    }
    credentials_from_header_map(map)
}

fn is_header_key_line(line: &str) -> bool {
    !line.is_empty()
        && !line.contains(':')
        && !line.contains('=')
        && !line.contains(' ')
        && line
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn parse_header_text_auth(trimmed: &str) -> Result<AuthCredentials, CoreError> {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut has_header_lines = false;
    let lines: Vec<&str> = trimmed.lines().map(str::trim).collect();
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        // Skip HTTP request start line like `POST /youtubei/v1/... HTTP/2`.
        if (line.starts_with("GET ") || line.starts_with("POST ")) && line.contains("HTTP/") {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().to_string();
            if !key.is_empty() && !val.is_empty() {
                has_header_lines = true;
                map.insert(key, val);
            }
        } else if is_header_key_line(line) {
            // Support Chrome DevTools alternating `header-name\nheader-value` copy format.
            if let Some(val) = lines.get(i).copied().filter(|v| !v.is_empty()) {
                has_header_lines = true;
                map.insert(line.to_ascii_lowercase(), val.to_string());
                i += 1;
            }
        } else if !map.contains_key("cookie")
            && (line.contains("SAPISID=") || line.contains("__Secure-3PAPISID="))
        {
            has_header_lines = true;
            map.insert("cookie".to_string(), line.to_string());
        }
    }

    if !has_header_lines || (!map.contains_key("cookie") && trimmed.contains("SAPISID=")) {
        map.insert("cookie".to_string(), trimmed.to_string());
    }

    credentials_from_header_map(map)
}

fn credentials_from_header_map(
    mut map: HashMap<String, String>,
) -> Result<AuthCredentials, CoreError> {
    let cookie = map
        .remove("cookie")
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| CoreError::InvalidAuthInput("missing Cookie header".to_string()))?;

    let user_agent = map
        .remove("user-agent")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string());

    let x_goog_authuser = map
        .remove("x-goog-authuser")
        .filter(|s| !s.trim().is_empty())
        .or_else(|| Some("0".to_string()));

    let x_goog_page_id = map.remove("x-goog-pageid").filter(|s| !s.trim().is_empty());

    let x_goog_visitor_id = map
        .remove("x-goog-visitor-id")
        .filter(|s| !s.trim().is_empty());

    Ok(AuthCredentials {
        cookie,
        user_agent,
        x_goog_authuser,
        x_goog_page_id,
        x_goog_visitor_id,
    })
}

/// Extract `__Secure-3PAPISID` (preferred) or `SAPISID` from a Cookie header, stripping quotes.
pub fn extract_sapisid(cookie: &str) -> Result<String, CoreError> {
    let mut fallback_sapisid: Option<String> = None;

    for pair in cookie.split(';') {
        let Some((name, raw_val)) = pair.trim().split_once('=') else {
            continue;
        };
        let key = name.trim();
        let val = raw_val.trim().trim_matches('"').trim();
        if val.is_empty() {
            continue;
        }
        if key == "__Secure-3PAPISID" {
            return Ok(val.to_string());
        }
        if key == "SAPISID" && fallback_sapisid.is_none() {
            fallback_sapisid = Some(val.to_string());
        }
    }

    fallback_sapisid.ok_or(CoreError::MissingCookie)
}

/// Compute `SAPISIDHASH <ts>_<sha1_hex>` using injectable UNIX timestamp.
#[must_use]
pub fn compute_sapisidhash(sapisid: &str, origin: &str, timestamp_secs: u64) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("{timestamp_secs} {sapisid} {origin}").as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("SAPISIDHASH {timestamp_secs}_{digest}")
}

/// Build full InnerTube authentication headers from persisted credentials.
pub fn build_auth_headers(
    creds: &AuthCredentials,
    timestamp_secs: u64,
) -> Result<Vec<(String, String)>, CoreError> {
    let sapisid = extract_sapisid(&creds.cookie)?;
    let authorization = compute_sapisidhash(&sapisid, YTM_ORIGIN, timestamp_secs);

    let has_socs = creds
        .cookie
        .split(';')
        .any(|part| part.trim().starts_with("SOCS="));
    let cookie_header = if has_socs {
        creds.cookie.clone()
    } else {
        format!("{}; SOCS=CAI", creds.cookie.trim_end_matches(';').trim())
    };

    let ua = if creds.user_agent.trim().is_empty() {
        DEFAULT_USER_AGENT.to_string()
    } else {
        creds.user_agent.clone()
    };

    let mut headers = vec![
        ("Authorization".to_string(), authorization),
        ("Cookie".to_string(), cookie_header),
        ("User-Agent".to_string(), ua),
        ("Origin".to_string(), YTM_ORIGIN.to_string()),
        ("X-Origin".to_string(), YTM_ORIGIN.to_string()),
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Accept".to_string(), "*/*".to_string()),
        (
            "X-Goog-AuthUser".to_string(),
            creds
                .x_goog_authuser
                .clone()
                .unwrap_or_else(|| "0".to_string()),
        ),
    ];

    if let Some(page_id) = &creds.x_goog_page_id {
        if !page_id.trim().is_empty() {
            headers.push(("X-Goog-PageId".to_string(), page_id.clone()));
        }
    }

    if let Some(visitor_id) = &creds.x_goog_visitor_id {
        if !visitor_id.trim().is_empty() {
            headers.push(("X-Goog-Visitor-Id".to_string(), visitor_id.clone()));
        }
    }

    Ok(headers)
}

/// Refresh the `Authorization` (`SAPISIDHASH`) header in-place for retry attempts.
pub fn refresh_sapisidhash_in_headers(
    headers: &mut [(String, String)],
    timestamp_secs: u64,
) -> Result<(), CoreError> {
    let cookie = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("cookie"))
        .map(|(_, v)| v.as_str())
        .ok_or(CoreError::MissingCookie)?;
    let sapisid = extract_sapisid(cookie)?;
    let fresh = compute_sapisidhash(&sapisid, YTM_ORIGIN, timestamp_secs);
    if let Some((_, val)) = headers
        .iter_mut()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
    {
        *val = fresh;
    }
    Ok(())
}

/// Convert Netscape `cookies.txt` (including `#HttpOnly_` lines from `yt-dlp`) into a `Cookie:` header.
pub fn netscape_cookies_to_header(netscape_txt: &str) -> Result<String, CoreError> {
    let mut cookies: Vec<(String, String)> = Vec::new();
    for raw_line in netscape_txt.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line = if let Some(rest) = trimmed.strip_prefix("#HttpOnly_") {
            rest
        } else if trimmed.starts_with('#') {
            continue;
        } else {
            trimmed
        };
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 7 {
            continue;
        }
        let domain = parts[0].trim().trim_start_matches('.').to_ascii_lowercase();
        if domain != "youtube.com" && !domain.ends_with(".youtube.com") {
            continue;
        }
        let name = parts[5].trim();
        let val = parts[6].trim();
        if !name.is_empty() && !val.is_empty() {
            if let Some(existing) = cookies.iter_mut().find(|(k, _)| k == name) {
                existing.1 = val.to_string();
            } else {
                cookies.push((name.to_string(), val.to_string()));
            }
        }
    }

    if cookies.is_empty() {
        return Err(CoreError::MissingCookie);
    }
    let header = cookies
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ");
    extract_sapisid(&header)?;
    Ok(format!("Cookie: {header}"))
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
