use serde_json::Value;

use crate::error::CoreError;
use crate::models::{Playlist, PlaylistPrivacy, PlaylistSummary, SessionStatus, Track};
use crate::parser::track::parse_tracks_with_diagnostics;
use crate::parser::tree::{
    check_innertube_error, extract_runs_text, find_all_by_key, find_first_by_key,
    is_explicitly_logged_out, normalize_playlist_id,
};

/// Detailed output of parsing the first page of `get_playlist`.
#[derive(Debug, Clone)]
pub struct ParsedPlaylistPage {
    pub playlist: Playlist,
    pub next_token: Option<String>,
    pub declared_track_count: Option<usize>,
    pub dropped_renderers: Vec<String>,
}

/// Detailed output of parsing a playlist continuation response.
#[derive(Debug, Clone)]
pub struct ParsedContinuationPage {
    pub tracks: Vec<Track>,
    pub next_token: Option<String>,
    pub dropped_renderers: Vec<String>,
}

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

/// Parse `FEmusic_liked_playlists` (or its continuation) into summaries + optional continuation token.
pub fn parse_library_playlists(
    response: &Value,
) -> Result<(Vec<PlaylistSummary>, Option<String>), CoreError> {
    check_innertube_error(response)?;
    if is_explicitly_logged_out(response) {
        return Err(CoreError::AuthExpired);
    }

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
                    looks_like_track_count(txt).then(|| parse_leading_usize(txt))?
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

    let grid = find_first_by_key(response, "gridRenderer")
        .or_else(|| find_first_by_key(response, "gridContinuation"));
    let cont_items = find_continuation_items(response)
        .or_else(|| {
            grid.and_then(|g| g.get("items"))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
        })
        .unwrap_or(&[]);
    let next_token = extract_continuation_token(grid.unwrap_or(response), cont_items);

    Ok((summaries, next_token))
}

fn parse_leading_usize(text: &str) -> Option<usize> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<usize>().ok()
    }
}

fn extract_declared_track_count(header: Option<&Value>, shelf: Option<&Value>) -> Option<usize> {
    if let Some(sub) = header.and_then(|h| h.get("secondSubtitle")) {
        if let Some(runs) = sub.get("runs").and_then(Value::as_array) {
            for run in runs {
                if let Some(txt) = run.get("text").and_then(Value::as_str)
                    && looks_like_track_count(txt)
                    && let Some(count) = parse_leading_usize(txt)
                {
                    return Some(count);
                }
            }
        }
        let flat = extract_runs_text(sub);
        for seg in flat.split(['•', '·']) {
            if looks_like_track_count(seg)
                && let Some(count) = parse_leading_usize(seg)
            {
                return Some(count);
            }
        }
    }

    [header, shelf]
        .into_iter()
        .flatten()
        .filter_map(|v| v.get("collapsedItemCount"))
        .find_map(|v| {
            v.as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .or_else(|| v.as_str().and_then(parse_leading_usize))
        })
}

/// Parse first page of `get_playlist` (`POST browse`) with header track count and diagnostics.
pub fn parse_playlist_page_detailed(
    response: &Value,
    requested_id: &str,
) -> Result<ParsedPlaylistPage, CoreError> {
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
        (
            find_first_by_key(response, "musicResponsiveHeaderRenderer"),
            PlaylistPrivacy::Public,
            None,
        )
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

    let declared_track_count = extract_declared_track_count(header, shelf);
    let (tracks, dropped_renderers) = parse_tracks_with_diagnostics(contents, 0);
    let next_token = shelf.and_then(|s| extract_continuation_token(s, contents));

    Ok(ParsedPlaylistPage {
        playlist: Playlist {
            id,
            title,
            description,
            privacy,
            owned,
            track_count: tracks.len(),
            tracks,
        },
        next_token,
        declared_track_count,
        dropped_renderers,
    })
}

/// Parse first page of `get_playlist` (`POST browse`), returning `(Playlist, Option<continuation_token>)`.
pub fn parse_playlist_page(
    response: &Value,
    requested_id: &str,
) -> Result<(Playlist, Option<String>), CoreError> {
    let parsed = parse_playlist_page_detailed(response, requested_id)?;
    Ok((parsed.playlist, parsed.next_token))
}

fn find_continuation_items(response: &Value) -> Option<&[Value]> {
    find_first_by_key(response, "appendContinuationItemsAction")
        .and_then(|a| a.get("continuationItems"))
        .and_then(Value::as_array)
        .or_else(|| {
            find_first_by_key(response, "reloadContinuationItemsCommand")
                .and_then(|r| r.get("continuationItems"))
                .and_then(Value::as_array)
        })
        .or_else(|| {
            find_first_by_key(response, "musicPlaylistShelfContinuation")
                .and_then(|c| c.get("contents"))
                .and_then(Value::as_array)
        })
        .map(Vec::as_slice)
}

/// Parse subsequent continuation response with error/auth checks and dropped-renderer diagnostics.
pub fn parse_continuation_page_detailed(
    response: &Value,
    start_pos: usize,
) -> Result<ParsedContinuationPage, CoreError> {
    check_innertube_error(response)?;
    if is_explicitly_logged_out(response) {
        return Err(CoreError::AuthExpired);
    }
    let items = find_continuation_items(response).unwrap_or(&[]);
    let (tracks, dropped_renderers) = parse_tracks_with_diagnostics(items, start_pos);
    let next_token = extract_continuation_token(response, items);
    Ok(ParsedContinuationPage {
        tracks,
        next_token,
        dropped_renderers,
    })
}

/// Parse subsequent continuation response (`POST browse` with `{"continuation": token}`).
#[must_use]
pub fn parse_continuation_page(response: &Value, start_pos: usize) -> (Vec<Track>, Option<String>) {
    let items = find_continuation_items(response).unwrap_or(&[]);
    let (tracks, _) = parse_tracks_with_diagnostics(items, start_pos);
    let next_token = extract_continuation_token(response, items);
    (tracks, next_token)
}

fn token_from_continuation_endpoint(endpoint: &Value) -> Option<String> {
    if let Some(tok) = endpoint
        .pointer("/continuationCommand/token")
        .and_then(Value::as_str)
    {
        return Some(tok.to_string());
    }
    let cmds = endpoint
        .pointer("/commandExecutorCommand/commands")
        .and_then(Value::as_array)?;
    for cmd in cmds {
        let req = cmd
            .pointer("/continuationCommand/request")
            .and_then(Value::as_str);
        if req.is_none() || req == Some("CONTINUATION_REQUEST_TYPE_BROWSE") {
            if let Some(tok) = cmd
                .pointer("/continuationCommand/token")
                .and_then(Value::as_str)
            {
                return Some(tok.to_string());
            }
        }
    }
    None
}

/// Extract next continuation token across all `items` (`continuationItemRenderer` even before trailing `messageRenderer`)
/// or legacy `continuations` / `reloadContinuationItemsCommand`.
#[must_use]
pub fn extract_continuation_token(container: &Value, items: &[Value]) -> Option<String> {
    for item in items.iter().rev() {
        if let Some(endpoint) = item.pointer("/continuationItemRenderer/continuationEndpoint")
            && let Some(tok) = token_from_continuation_endpoint(endpoint)
        {
            return Some(tok);
        }
    }

    if let Some(reload_items) = find_first_by_key(container, "reloadContinuationItemsCommand")
        .and_then(|r| r.get("continuationItems"))
        .and_then(Value::as_array)
    {
        for item in reload_items.iter().rev() {
            if let Some(endpoint) = item.pointer("/continuationItemRenderer/continuationEndpoint")
                && let Some(tok) = token_from_continuation_endpoint(endpoint)
            {
                return Some(tok);
            }
        }
    }

    find_first_by_key(container, "nextContinuationData")
        .or_else(|| find_first_by_key(container, "nextGridContinuationData"))
        .and_then(|d| d.get("continuation"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn looks_like_track_count(raw: &str) -> bool {
    let lower = raw.to_lowercase();
    ["track", "song", "parça", "şarkı", "müzik", "video"]
        .iter()
        .any(|kw| lower.contains(kw))
}

#[cfg(test)]
#[path = "playlist_tests.rs"]
mod tests;
