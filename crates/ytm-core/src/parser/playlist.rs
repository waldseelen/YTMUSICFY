use serde_json::Value;

use crate::error::CoreError;
use crate::models::{
    AddedPlaylistItemResult, EditPlaylistResult, Playlist, PlaylistPrivacy, PlaylistSummary,
    SessionStatus, Track,
};
use crate::parser::track::parse_tracks_from_items;
use crate::parser::tree::{
    check_innertube_error, extract_runs_text, find_all_by_key, find_first_by_key,
    is_explicitly_logged_out, normalize_playlist_id,
};

/// Parse `account/account_menu` response for explicit signed-in canary verification.
pub fn parse_session_status(
    response: &Value,
    auth_user: &str,
    brand_page_id: Option<&str>,
) -> Result<SessionStatus, CoreError> {
    check_innertube_error(response)?;
    if is_explicitly_logged_out(response) {
        return Err(CoreError::AuthExpired);
    }

    let header =
        find_first_by_key(response, "activeAccountHeaderRenderer").ok_or(CoreError::AuthExpired)?;
    let account_name = header
        .get("accountName")
        .map(extract_runs_text)
        .filter(|s| !s.is_empty());
    let channel_handle = header
        .get("channelHandle")
        .map(extract_runs_text)
        .filter(|s| !s.is_empty());

    Ok(SessionStatus {
        authenticated: true,
        account_name,
        channel_handle,
        auth_user: auth_user.to_string(),
        brand_page_id: brand_page_id.map(str::to_string),
    })
}

/// Parse `FEmusic_liked_playlists` response into `PlaylistSummary` entries.
#[must_use]
pub fn parse_library_playlists(response: &Value) -> Vec<PlaylistSummary> {
    let mut renderers = Vec::new();
    find_all_by_key(response, "musicTwoRowItemRenderer", &mut renderers);

    let mut summaries = Vec::new();
    for item in renderers {
        let raw_browse_id = item
            .pointer("/navigationEndpoint/browseEndpoint/browseId")
            .and_then(Value::as_str)
            .or_else(|| {
                find_first_by_key(item, "browseEndpoint")
                    .and_then(|be| be.get("browseId"))
                    .and_then(Value::as_str)
            });

        let Some(browse_id) = raw_browse_id else {
            continue;
        };

        let id = normalize_playlist_id(browse_id);
        let is_liked_music = id == "LM";
        let title = item
            .get("title")
            .map(extract_runs_text)
            .unwrap_or_else(|| id.clone());
        let owned = is_liked_music || find_first_by_key(item, "playlistEditorEndpoint").is_some();

        let track_count = item
            .get("subtitle")
            .and_then(|s| s.get("runs"))
            .and_then(Value::as_array)
            .and_then(|runs| {
                runs.iter().rev().find_map(|r| {
                    let txt = r.get("text").and_then(Value::as_str)?;
                    if looks_like_track_count(txt) {
                        parse_leading_usize(txt)
                    } else {
                        None
                    }
                })
            });

        summaries.push(PlaylistSummary {
            id,
            title,
            track_count,
            owned,
            is_liked_music,
        });
    }
    summaries
}

fn parse_leading_usize(text: &str) -> Option<usize> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<usize>().ok()
    }
}

/// Parse first page of `get_playlist` (`POST browse`), returning `(Playlist, Option<continuation_token>)`.
pub fn parse_playlist_page(
    response: &Value,
    requested_id: &str,
) -> Result<(Playlist, Option<String>), CoreError> {
    check_innertube_error(response)?;
    if is_explicitly_logged_out(response) {
        return Err(CoreError::AuthExpired);
    }

    let editable = find_first_by_key(response, "musicEditablePlaylistDetailHeaderRenderer");
    let owned = editable.is_some();

    let (header, privacy, header_id) = if let Some(ed) = editable {
        let privacy = match ed
            .pointer("/editHeader/musicPlaylistEditHeaderRenderer/privacy")
            .and_then(Value::as_str)
        {
            Some("PUBLIC") => PlaylistPrivacy::Public,
            Some("UNLISTED") => PlaylistPrivacy::Unlisted,
            _ => PlaylistPrivacy::Private,
        };
        let pid = ed
            .get("playlistId")
            .and_then(Value::as_str)
            .map(normalize_playlist_id);
        let resp_hdr = ed
            .pointer("/header/musicResponsiveHeaderRenderer")
            .or_else(|| find_first_by_key(ed, "musicResponsiveHeaderRenderer"));
        (resp_hdr, privacy, pid)
    } else {
        let resp_hdr = find_first_by_key(response, "musicResponsiveHeaderRenderer");
        (resp_hdr, PlaylistPrivacy::Public, None)
    };

    let shelf = find_first_by_key(response, "musicPlaylistShelfRenderer");
    let shelf_id = shelf
        .and_then(|s| s.get("playlistId").or_else(|| s.get("targetId")))
        .and_then(Value::as_str)
        .map(normalize_playlist_id);

    let id = header_id
        .or(shelf_id)
        .unwrap_or_else(|| normalize_playlist_id(requested_id));

    let title = header
        .and_then(|h| h.get("title"))
        .map(extract_runs_text)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| id.clone());

    let description = header
        .and_then(|h| find_first_by_key(h, "musicDescriptionShelfRenderer"))
        .and_then(|d| d.get("description"))
        .map(extract_runs_text)
        .filter(|s| !s.is_empty());

    let contents = shelf
        .and_then(|s| s.get("contents"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let tracks = parse_tracks_from_items(contents, 0);
    let next_token = shelf.and_then(|s| extract_continuation_token(s, contents));

    Ok((
        Playlist {
            id,
            title,
            description,
            privacy,
            owned,
            track_count: tracks.len(),
            tracks,
        },
        next_token,
    ))
}

/// Parse subsequent continuation response (`POST browse` with `{"continuation": token}`).
#[must_use]
pub fn parse_continuation_page(response: &Value, start_pos: usize) -> (Vec<Track>, Option<String>) {
    let items = find_first_by_key(response, "appendContinuationItemsAction")
        .and_then(|a| a.get("continuationItems"))
        .and_then(Value::as_array)
        .or_else(|| {
            find_first_by_key(response, "musicPlaylistShelfContinuation")
                .and_then(|c| c.get("contents"))
                .and_then(Value::as_array)
        })
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let tracks = parse_tracks_from_items(items, start_pos);
    let next_token = extract_continuation_token(response, items);
    (tracks, next_token)
}

/// Extract next continuation token from 2025 `continuationItemRenderer` or legacy `continuations`.
#[must_use]
pub fn extract_continuation_token(container: &Value, items: &[Value]) -> Option<String> {
    if let Some(last) = items.last() {
        if let Some(endpoint) = last.pointer("/continuationItemRenderer/continuationEndpoint") {
            if let Some(tok) = endpoint
                .pointer("/continuationCommand/token")
                .and_then(Value::as_str)
            {
                return Some(tok.to_string());
            }
            if let Some(cmds) = endpoint
                .pointer("/commandExecutorCommand/commands")
                .and_then(Value::as_array)
            {
                for cmd in cmds {
                    let req = cmd
                        .pointer("/continuationCommand/request")
                        .and_then(Value::as_str);
                    if req == Some("CONTINUATION_REQUEST_TYPE_BROWSE") {
                        if let Some(tok) = cmd
                            .pointer("/continuationCommand/token")
                            .and_then(Value::as_str)
                        {
                            return Some(tok.to_string());
                        }
                    }
                }
            }
        }
    }

    find_first_by_key(container, "nextContinuationData")
        .or_else(|| find_first_by_key(container, "nextGridContinuationData"))
        .and_then(|d| d.get("continuation"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Parse `browse/edit_playlist` response payload.
pub fn parse_edit_playlist_response(response: &Value) -> Result<EditPlaylistResult, CoreError> {
    check_innertube_error(response)?;
    let status = response
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("STATUS_UNKNOWN")
        .to_string();

    if status == "STATUS_FAILED" {
        return Err(CoreError::ApiError {
            code: status,
            message: "browse/edit_playlist returned STATUS_FAILED".to_string(),
        });
    }

    let mut added_items = Vec::new();
    if let Some(results) = response
        .get("playlistEditResults")
        .and_then(Value::as_array)
    {
        for entry in results {
            if let Some(data) = entry.get("playlistEditVideoAddedResultData") {
                if let (Some(vid), Some(sid)) = (
                    data.get("videoId").and_then(Value::as_str),
                    data.get("setVideoId").and_then(Value::as_str),
                ) {
                    added_items.push(AddedPlaylistItemResult {
                        video_id: vid.to_string(),
                        set_video_id: sid.to_string(),
                    });
                }
            }
        }
    }

    Ok(EditPlaylistResult {
        status,
        added_items,
    })
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

fn looks_like_track_count(raw: &str) -> bool {
    let lower = raw.to_lowercase();
    ["track", "song", "parça", "şarkı", "müzik", "video"]
        .iter()
        .any(|kw| lower.contains(kw))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_library_playlists;

    #[test]
    fn parse_library_playlists_handles_localized_and_formatted_counts() {
        let resp = json!({
            "contents": {
                "items": [
                    {
                        "musicTwoRowItemRenderer": {
                            "title": { "runs": [{ "text": "Liked Music" }] },
                            "subtitle": { "runs": [{ "text": "Auto playlist" }] },
                            "navigationEndpoint": { "browseEndpoint": { "browseId": "VLLM" } }
                        }
                    },
                    {
                        "musicTwoRowItemRenderer": {
                            "title": { "runs": [{ "text": "Turkish Rock" }] },
                            "subtitle": { "runs": [{ "text": "Playlist" }, { "text": " • " }, { "text": "1,245 şarkı" }] },
                            "navigationEndpoint": { "browseEndpoint": { "browseId": "VLPL1" } }
                        }
                    },
                    {
                        "musicTwoRowItemRenderer": {
                            "title": { "runs": [{ "text": "Chill" }] },
                            "subtitle": { "runs": [{ "text": "108 songs" }] },
                            "navigationEndpoint": { "browseEndpoint": { "browseId": "VLPL2" } }
                        }
                    }
                ]
            }
        });

        let parsed = parse_library_playlists(&resp);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].id, "LM");
        assert_eq!(parsed[0].track_count, None);
        assert_eq!(parsed[1].track_count, Some(1245));
        assert_eq!(parsed[2].track_count, Some(108));
    }
}
