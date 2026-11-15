use serde_json::Value;

use crate::error::CoreError;

/// Recursively find the first JSON value matching `key`.
pub fn find_first_by_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            if let Some(found) = map.get(key) {
                return Some(found);
            }
            for child in map.values() {
                if let Some(found) = find_first_by_key(child, key) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => {
            for child in arr {
                if let Some(found) = find_first_by_key(child, key) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

/// Recursively collect every JSON value stored under `key`.
pub fn find_all_by_key<'a>(value: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            if let Some(found) = map.get(key) {
                out.push(found);
            }
            for child in map.values() {
                find_all_by_key(child, key, out);
            }
        }
        Value::Array(arr) => {
            for child in arr {
                find_all_by_key(child, key, out);
            }
        }
        _ => {}
    }
}

/// Concatenate InnerTube `runs[].text`, `simpleText`, or string content cleanly.
#[must_use]
pub fn extract_runs_text(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    if let Some(runs) = value.get("runs").and_then(Value::as_array) {
        return runs
            .iter()
            .filter_map(|r| r.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("");
    }
    if let Some(simple) = value.get("simpleText").and_then(Value::as_str) {
        return simple.to_string();
    }
    if let Some(content) = value.get("content").and_then(Value::as_str) {
        return content.to_string();
    }
    String::new()
}

/// Parse `"MM:SS"` or `"HH:MM:SS"` into seconds.
#[must_use]
pub fn parse_duration_seconds(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.contains(':') {
        return None;
    }
    let parts: Vec<u32> = trimmed
        .split(':')
        .map(|seg| seg.trim().parse::<u32>().ok())
        .collect::<Option<Vec<_>>>()?;

    match parts.as_slice() {
        [m, s] if *s < 60 => Some(m * 60 + s),
        [h, m, s] if *m < 60 && *s < 60 => Some(h * 3600 + m * 60 + s),
        _ => None,
    }
}

/// Strip leading `"VL"` browse prefix while turning `"VLLM"` into `"LM"`.
#[must_use]
pub fn normalize_playlist_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("VLLM") || trimmed.eq_ignore_ascii_case("LM") {
        "LM".to_string()
    } else if let Some(stripped) = trimmed.strip_prefix("VL") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Format playlist ID into InnerTube `browseId` (`"LM"` -> `"VLLM"`, `"PL..."` -> `"VLPL..."`).
#[must_use]
pub fn to_browse_playlist_id(playlist_id: &str) -> String {
    let trimmed = playlist_id.trim();
    if trimmed.eq_ignore_ascii_case("LM") {
        "VLLM".to_string()
    } else if trimmed.starts_with("VL") {
        trimmed.to_string()
    } else {
        format!("VL{trimmed}")
    }
}

/// Check `responseContext.serviceTrackingParams` for explicit logged-out flags (`logged_in == "0"`).
#[must_use]
pub fn is_explicitly_logged_out(response: &Value) -> bool {
    let Some(services) = response
        .pointer("/responseContext/serviceTrackingParams")
        .and_then(Value::as_array)
    else {
        return false;
    };

    for svc in services {
        let name = svc.get("service").and_then(Value::as_str).unwrap_or("");
        let Some(params) = svc.get("params").and_then(Value::as_array) else {
            continue;
        };
        for param in params {
            let key = param.get("key").and_then(Value::as_str).unwrap_or("");
            let val = param.get("value").and_then(Value::as_str).unwrap_or("");
            if (name == "GFEEDBACK" && key == "logged_in" && val == "0")
                || (name == "CSI" && key == "yt_li" && val == "0")
            {
                return true;
            }
        }
    }
    false
}

/// Check response for InnerTube top-level error object or gated-feature popup.
pub fn check_innertube_error(response: &Value) -> Result<(), CoreError> {
    if let Some(err) = response.get("error") {
        let code = err
            .get("code")
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| "UNKNOWN".to_string());
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("InnerTube error")
            .to_string();
        return Err(CoreError::ApiError { code, message });
    }
    if let Some(tag) = find_first_by_key(response, "identifier")
        .and_then(|v| v.get("tag"))
        .and_then(Value::as_str)
    {
        if tag == "PAfeature_enablement" {
            return Err(CoreError::ApiError {
                code: "GATED_FEATURE".to_string(),
                message: "account is gated from playlist creation".to_string(),
            });
        }
    }
    Ok(())
}
