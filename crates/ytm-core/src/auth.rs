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

fn parse_header_text_auth(trimmed: &str) -> Result<AuthCredentials, CoreError> {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut has_header_lines = false;

    for line in trimmed.lines() {
        let line = line.trim();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_sapisidhash_matches_known_sha1_vector() {
        // sha1("1700000000 test_sapisid https://music.youtube.com")
        let token = compute_sapisidhash("test_sapisid", YTM_ORIGIN, 1_700_000_000);
        assert_eq!(
            token,
            "SAPISIDHASH 1700000000_34f6546f99ec18cec46e50e842f2dab45b9d2d7d"
        );
    }

    #[test]
    fn extract_sapisid_prefers_secure_3papisid_and_strips_quotes() {
        let cookie = r#"SAPISID="old_val"; __Secure-3PAPISID="new_secure_val"; PREF=tz=UTC"#;
        assert_eq!(extract_sapisid(cookie).unwrap(), "new_secure_val");

        let only_sapisid = r#"PREF=tz=UTC; SAPISID="fallback_val""#;
        assert_eq!(extract_sapisid(only_sapisid).unwrap(), "fallback_val");

        assert!(matches!(
            extract_sapisid("PREF=tz=UTC; SID=abc"),
            Err(CoreError::MissingCookie)
        ));
    }

    #[test]
    fn parse_auth_input_supports_browser_json_and_raw_headers() {
        let browser_json = r#"{
            "Cookie": "__Secure-3PAPISID=secret123; SID=abc",
            "User-Agent": "CustomUA/1.0",
            "X-Goog-AuthUser": "2",
            "X-Goog-PageId": "10987654321",
            "X-Goog-Visitor-Id": "CgtVisitor"
        }"#;
        let creds = parse_auth_input(browser_json).expect("parse browser.json");
        assert_eq!(creds.x_goog_authuser.as_deref(), Some("2"));
        assert_eq!(creds.x_goog_page_id.as_deref(), Some("10987654321"));
        assert_eq!(creds.x_goog_visitor_id.as_deref(), Some("CgtVisitor"));

        let raw_headers =
            "POST /youtubei/v1/browse HTTP/2\nCookie: SAPISID=raw_tok\nX-Goog-AuthUser: 1\n";
        let creds2 = parse_auth_input(raw_headers).expect("parse raw headers");
        assert_eq!(creds2.x_goog_authuser.as_deref(), Some("1"));
        let built = build_auth_headers(&creds2, 1_700_000_000).expect("build headers");
        assert!(built.iter().any(|(k, _)| k == "Authorization"));
        assert!(
            built
                .iter()
                .any(|(k, v)| k == "Cookie" && v.contains("SOCS=CAI"))
        );
    }
}
