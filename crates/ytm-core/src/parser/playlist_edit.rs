use serde_json::Value;

use crate::error::CoreError;
use crate::models::{AddedPlaylistItemResult, EditPlaylistResult, Track};
use crate::parser::track::parse_tracks_from_items;
use crate::parser::tree::{
    check_innertube_error, extract_runs_text, find_all_by_key, is_explicitly_logged_out,
};

fn extract_toast_message(response: &Value) -> Option<String> {
    let mut toasts = Vec::new();
    find_all_by_key(response, "addToToastAction", &mut toasts);
    for toast in toasts {
        let candidates = [
            toast.pointer("/item/notificationTextRenderer/successResponseText"),
            toast.pointer("/item/notificationTextRenderer/text"),
            toast.pointer("/item/notificationActionRenderer/responseText"),
        ];
        for cand in candidates.into_iter().flatten() {
            let txt = extract_runs_text(cand);
            if !txt.trim().is_empty() {
                return Some(txt.trim().to_string());
            }
        }
    }
    None
}

/// Parse `browse/edit_playlist` response payload requiring `STATUS_SUCCEEDED`.
pub fn parse_edit_playlist_response(response: &Value) -> Result<EditPlaylistResult, CoreError> {
    check_innertube_error(response)?;
    if is_explicitly_logged_out(response) {
        return Err(CoreError::AuthExpired);
    }

    let status = response
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("STATUS_UNKNOWN")
        .to_string();

    if status != "STATUS_SUCCEEDED" {
        let message = extract_toast_message(response)
            .unwrap_or_else(|| format!("browse/edit_playlist returned {status}"));
        return Err(CoreError::ApiError {
            code: status,
            message,
        });
    }

    let mut added_items = Vec::new();
    if let Some(results) = response
        .get("playlistEditResults")
        .and_then(Value::as_array)
    {
        for entry in results {
            if let Some(data) = entry.get("playlistEditVideoAddedResultData")
                && let (Some(vid), Some(sid)) = (
                    data.get("videoId").and_then(Value::as_str),
                    data.get("setVideoId").and_then(Value::as_str),
                )
            {
                added_items.push(AddedPlaylistItemResult {
                    video_id: vid.to_string(),
                    set_video_id: sid.to_string(),
                });
            }
        }
    }

    Ok(EditPlaylistResult {
        status,
        added_items,
    })
}

/// Parse `playlist/delete` response payload (allows empty `{}` success responses).
pub fn parse_delete_playlist_response(response: &Value) -> Result<(), CoreError> {
    check_innertube_error(response)?;
    if is_explicitly_logged_out(response) {
        return Err(CoreError::AuthExpired);
    }
    if let Some(status) = response.get("status").and_then(Value::as_str)
        && status != "STATUS_SUCCEEDED"
    {
        let message = extract_toast_message(response)
            .unwrap_or_else(|| format!("playlist/delete returned {status}"));
        return Err(CoreError::ApiError {
            code: status.to_string(),
            message,
        });
    }
    Ok(())
}

/// Parse `POST search` song shelf into `Vec<Track>`.
#[must_use]
pub fn parse_search_tracks(response: &Value, limit: usize) -> Vec<Track> {
    let mut shelves = Vec::new();
    find_all_by_key(response, "musicShelfRenderer", &mut shelves);

    let mut tracks = Vec::new();
    for shelf in shelves {
        let Some(contents) = shelf.get("contents").and_then(Value::as_array) else {
            continue;
        };
        let parsed = parse_tracks_from_items(contents, tracks.len());
        tracks.extend(parsed);
        if tracks.len() >= limit {
            tracks.truncate(limit);
            break;
        }
    }
    tracks
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn edit_playlist_rejects_non_succeeded_and_extracts_toast_error() {
        let toast_err = json!({
            "status": "STATUS_FAILED",
            "actions": [{
                "addToToastAction": {
                    "item": {
                        "notificationTextRenderer": {
                            "successResponseText": {
                                "runs": [{ "text": "Playlist limit reached" }]
                            }
                        }
                    }
                }
            }]
        });
        let err = parse_edit_playlist_response(&toast_err).unwrap_err();
        match err {
            CoreError::ApiError { code, message } => {
                assert_eq!(code, "STATUS_FAILED");
                assert_eq!(message, "Playlist limit reached");
            }
            other => panic!("expected ApiError, got {other:?}"),
        }

        let missing_status = json!({});
        assert!(matches!(
            parse_edit_playlist_response(&missing_status),
            Err(CoreError::ApiError { code, .. }) if code == "STATUS_UNKNOWN"
        ));

        assert!(parse_delete_playlist_response(&json!({})).is_ok());
    }
}
