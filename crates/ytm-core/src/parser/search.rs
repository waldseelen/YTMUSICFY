use serde_json::Value;

use crate::browse_models::{BrowseCard, BrowseCardKind, LyricsPayload, SearchPage, TimedLyricLine};
use crate::error::CoreError;
use crate::parser::browse::{parse_lyrics_response, parse_two_row_card};
use crate::parser::playlist::extract_continuation_token;
use crate::parser::track::{extract_thumbnail_url, parse_tracks_from_items};
use crate::parser::tree::{
    check_innertube_error, extract_runs_text, find_all_by_key, find_first_by_key,
    is_explicitly_logged_out,
};

fn parse_ms(val: Option<&Value>) -> Option<u64> {
    let v = val?;
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
}

/// Parse InnerTube lyrics response into [`LyricsPayload::Synced`] (`timedLyricsData`) or [`LyricsPayload::Plain`].
#[must_use]
pub fn parse_lyrics_payload(response: &Value) -> Option<LyricsPayload> {
    if let Some(entries) = find_first_by_key(response, "timedLyricsData").and_then(Value::as_array)
        && !entries.is_empty()
    {
        let mut timed = Vec::with_capacity(entries.len());
        let mut plain = Vec::with_capacity(entries.len());
        let mut all_synced = true;

        for entry in entries {
            let text = entry
                .get("lyricLine")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            plain.push(text.clone());

            let cue = entry.get("cueRange");
            if let (Some(start_ms), Some(end_ms)) = (
                parse_ms(cue.and_then(|c| c.get("startTimeMilliseconds"))),
                parse_ms(cue.and_then(|c| c.get("endTimeMilliseconds"))),
            ) {
                timed.push(TimedLyricLine {
                    start_ms,
                    end_ms,
                    text,
                });
            } else {
                all_synced = false;
            }
        }

        if all_synced && !timed.is_empty() {
            return Some(LyricsPayload::Synced(timed));
        }
        let joined = plain.join("\n");
        if !joined.trim().is_empty() {
            return Some(LyricsPayload::Plain(joined));
        }
    }

    parse_lyrics_response(response).map(LyricsPayload::Plain)
}

fn parse_list_item_card(renderer: &Value) -> Option<BrowseCard> {
    let nav = renderer
        .get("navigationEndpoint")
        .or_else(|| {
            renderer
                .pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint")
        })?;
    let browse_id = nav
        .pointer("/browseEndpoint/browseId")
        .and_then(Value::as_str)
        .map(str::to_string)?;

    let cols = renderer.get("flexColumns").and_then(Value::as_array)?;
    let title = cols
        .first()
        .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text"))
        .map(extract_runs_text)
        .filter(|s| !s.is_empty())?;
    let subtitle = cols
        .get(1)
        .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text"))
        .map(extract_runs_text)
        .unwrap_or_default();

    let page_type = nav
        .pointer("/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
        .and_then(Value::as_str)
        .unwrap_or("");
    let kind = match page_type {
        "MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL" => BrowseCardKind::Artist,
        "MUSIC_PAGE_TYPE_ALBUM" | "MUSIC_PAGE_TYPE_AUDIOBOOK" => BrowseCardKind::Album,
        _ if browse_id.starts_with("UC") || browse_id.starts_with("MPLA") => BrowseCardKind::Artist,
        _ if browse_id.starts_with("MPREb_") => BrowseCardKind::Album,
        _ => BrowseCardKind::Playlist,
    };

    Some(BrowseCard {
        kind,
        browse_id: Some(browse_id),
        video_id: None,
        title,
        subtitle,
        thumbnail_url: extract_thumbnail_url(renderer),
    })
}

fn collect_search_items(
    items: &[Value],
    tracks: &mut Vec<crate::models::Track>,
    cards: &mut Vec<BrowseCard>,
) {
    for item in items {
        if let Some(two_row) = item.get("musicTwoRowItemRenderer") {
            if let Some(card) = parse_two_row_card(two_row) {
                cards.push(card);
            }
            continue;
        }
        if let Some(renderer) = item.get("musicResponsiveListItemRenderer") {
            let has_watch = find_first_by_key(renderer, "watchEndpoint").is_some()
                || find_first_by_key(renderer, "playNavigationEndpoint").is_some();
            if !has_watch && let Some(card) = parse_list_item_card(renderer) {
                cards.push(card);
                continue;
            }
            let parsed = parse_tracks_from_items(std::slice::from_ref(item), tracks.len());
            tracks.extend(parsed);
        }
    }
}

/// Parse InnerTube `POST search` (initial or continuation) into [`SearchPage`].
pub fn parse_search_page(response: &Value) -> Result<SearchPage, CoreError> {
    check_innertube_error(response)?;
    if is_explicitly_logged_out(response) {
        return Err(CoreError::AuthExpired);
    }

    let mut tracks = Vec::new();
    let mut cards = Vec::new();
    let mut continuation = None;

    if let Some(card_shelf) = find_first_by_key(response, "musicCardShelfRenderer") {
        if let Some(title) = card_shelf.get("title").map(extract_runs_text)
            && !title.is_empty()
        {
            let nav = card_shelf.pointer("/title/runs/0/navigationEndpoint");
            let video_id = nav
                .and_then(|n| n.pointer("/watchEndpoint/videoId"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let browse_id = nav
                .and_then(|n| n.pointer("/browseEndpoint/browseId"))
                .and_then(Value::as_str)
                .map(str::to_string);
            if video_id.is_some() || browse_id.is_some() {
                let kind = if video_id.is_some() {
                    BrowseCardKind::Track
                } else if browse_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with("UC") || id.starts_with("MPLA"))
                {
                    BrowseCardKind::Artist
                } else if browse_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with("MPREb_"))
                {
                    BrowseCardKind::Album
                } else {
                    BrowseCardKind::Playlist
                };
                cards.push(BrowseCard {
                    kind,
                    browse_id,
                    video_id,
                    title,
                    subtitle: card_shelf
                        .get("subtitle")
                        .map(extract_runs_text)
                        .unwrap_or_default(),
                    thumbnail_url: extract_thumbnail_url(card_shelf),
                });
            }
        }
        if let Some(contents) = card_shelf.get("contents").and_then(Value::as_array) {
            collect_search_items(contents, &mut tracks, &mut cards);
        }
    }

    let mut shelves = Vec::new();
    find_all_by_key(response, "musicShelfRenderer", &mut shelves);
    for shelf in shelves {
        let items = shelf
            .get("contents")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        collect_search_items(items, &mut tracks, &mut cards);
        if continuation.is_none() {
            continuation = extract_continuation_token(shelf, items);
        }
    }

    let cont_items = find_first_by_key(response, "appendContinuationItemsAction")
        .and_then(|a| a.get("continuationItems"))
        .and_then(Value::as_array)
        .or_else(|| {
            find_first_by_key(response, "reloadContinuationItemsCommand")
                .and_then(|r| r.get("continuationItems"))
                .and_then(Value::as_array)
        })
        .or_else(|| {
            find_first_by_key(response, "musicShelfContinuation")
                .and_then(|c| c.get("contents"))
                .and_then(Value::as_array)
        })
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    if !cont_items.is_empty() {
        collect_search_items(cont_items, &mut tracks, &mut cards);
        if continuation.is_none() {
            continuation = extract_continuation_token(response, cont_items);
        }
    } else if continuation.is_none() {
        continuation = extract_continuation_token(response, &[]);
    }

    Ok(SearchPage {
        tracks,
        cards,
        continuation,
    })
}

/// Parse InnerTube `music/get_search_suggestions` response into deduplicated suggestion queries.
pub fn parse_search_suggestions(response: &Value) -> Result<Vec<String>, CoreError> {
    check_innertube_error(response)?;

    let mut nodes = Vec::new();
    find_all_by_key(response, "historySuggestionRenderer", &mut nodes);
    find_all_by_key(response, "searchSuggestionRenderer", &mut nodes);

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for node in nodes {
        let query = node
            .get("navigationEndpoint")
            .and_then(|n| n.get("searchEndpoint"))
            .and_then(|s| s.get("query"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                node.get("suggestion")
                    .map(extract_runs_text)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            });
        if let Some(q) = query
            && seen.insert(q.to_ascii_lowercase())
        {
            out.push(q);
        }
    }
    Ok(out)
}
