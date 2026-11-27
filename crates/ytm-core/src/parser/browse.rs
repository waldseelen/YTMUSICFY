use serde_json::Value;

use crate::browse_models::{BrowsePage, BrowsePageKind, BrowseSection, WatchNextResult};
use crate::error::CoreError;
use crate::models::{AlbumRef, ArtistRef, MusicVideoType, Track, TrackAvailability};
pub(crate) use crate::parser::browse_cards::parse_two_row_card;
use crate::parser::browse_cards::{parse_card_shelf, parse_shelf};
use crate::parser::browse_header::{combine_subtitle, extract_header_artists};
use crate::parser::playlist::extract_continuation_token;
use crate::parser::track::{extract_thumbnail_url, parse_tracks_from_items};
use crate::parser::tree::{
    check_innertube_error, extract_runs_text, find_all_by_key, find_first_by_key,
    is_explicitly_logged_out, parse_duration_seconds,
};

fn extract_shelf_with_cont(
    shelf: &Value,
    array_key: &str,
    fallback_thumb: &Option<String>,
    sections: &mut Vec<BrowseSection>,
    continuation: &mut Option<String>,
) {
    let items = shelf
        .get(array_key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if continuation.is_none() {
        *continuation = extract_continuation_token(shelf, items);
    }
    if let Some(sec) = parse_shelf(shelf, array_key, fallback_thumb) {
        sections.push(sec);
    }
}

fn visit_browse_sections(
    node: &Value,
    fallback_thumb: &Option<String>,
    sections: &mut Vec<BrowseSection>,
    continuation: &mut Option<String>,
) {
    match node {
        Value::Object(map) => {
            if let Some(cols) = map
                .get("twoColumnBrowseResultsRenderer")
                .or_else(|| map.get("singleColumnBrowseResultsRenderer"))
            {
                if let Some(tabs) = cols.get("tabs") {
                    visit_browse_sections(tabs, fallback_thumb, sections, continuation);
                }
                if let Some(secondary) = cols.get("secondaryContents") {
                    visit_browse_sections(secondary, fallback_thumb, sections, continuation);
                }
                return;
            }
            if let Some(card_shelf) = map.get("musicCardShelfRenderer") {
                if let Some(sec) = parse_card_shelf(card_shelf, fallback_thumb) {
                    sections.push(sec);
                }
                return;
            }
            if let Some(carousel) = map.get("musicCarouselShelfRenderer") {
                if let Some(sec) = parse_shelf(carousel, "contents", fallback_thumb) {
                    sections.push(sec);
                }
                return;
            }
            if let Some(shelf) = map
                .get("musicShelfRenderer")
                .or_else(|| map.get("musicPlaylistShelfRenderer"))
                .or_else(|| map.get("musicPlaylistShelfContinuation"))
            {
                extract_shelf_with_cont(shelf, "contents", fallback_thumb, sections, continuation);
                return;
            }
            if let Some(grid) = map
                .get("gridRenderer")
                .or_else(|| map.get("gridContinuation"))
            {
                extract_shelf_with_cont(grid, "items", fallback_thumb, sections, continuation);
                return;
            }
            for child in map.values() {
                visit_browse_sections(child, fallback_thumb, sections, continuation);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                visit_browse_sections(item, fallback_thumb, sections, continuation);
            }
        }
        _ => {}
    }
}

fn collect_browse_sections(
    root: &Value,
    fallback_thumb: &Option<String>,
) -> (Vec<BrowseSection>, Option<String>) {
    let mut sections = Vec::new();
    let mut continuation = None;
    visit_browse_sections(root, fallback_thumb, &mut sections, &mut continuation);

    if continuation.is_none() {
        let sec_items = find_first_by_key(root, "sectionListRenderer")
            .and_then(|s| s.get("contents"))
            .and_then(Value::as_array)
            .or_else(|| {
                find_first_by_key(root, "sectionListContinuation")
                    .and_then(|s| s.get("contents"))
                    .and_then(Value::as_array)
            })
            .or_else(|| {
                find_first_by_key(root, "appendContinuationItemsAction")
                    .and_then(|a| a.get("continuationItems"))
                    .and_then(Value::as_array)
            })
            .or_else(|| {
                find_first_by_key(root, "musicPlaylistShelfContinuation")
                    .and_then(|c| c.get("contents"))
                    .and_then(Value::as_array)
            })
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        continuation = extract_continuation_token(root, sec_items);
    }

    (sections, continuation)
}

/// Parse InnerTube `browse` response for Home, Explore, History, Liked Albums, Artist, or Album pages.
pub fn parse_browse_page(response: &Value, browse_id: &str) -> Result<BrowsePage, CoreError> {
    check_innertube_error(response)?;
    if is_explicitly_logged_out(response) {
        return Err(CoreError::AuthExpired);
    }

    let kind = BrowsePageKind::from_browse_id(browse_id);
    let header = find_first_by_key(response, "musicResponsiveHeaderRenderer")
        .or_else(|| find_first_by_key(response, "musicImmersiveHeaderRenderer"))
        .or_else(|| find_first_by_key(response, "musicVisualHeaderRenderer"))
        .or_else(|| find_first_by_key(response, "musicDetailHeaderRenderer"));

    let title = header
        .and_then(|h| h.get("title"))
        .map(extract_runs_text)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| match kind {
            BrowsePageKind::Home => "Home".to_string(),
            _ => browse_id.to_string(),
        });
    let subtitle = combine_subtitle(header, response);
    let artists = extract_header_artists(header);
    let thumbnail_url = header.and_then(extract_thumbnail_url);

    let (mut sections, continuation) = collect_browse_sections(response, &thumbnail_url);
    if kind == BrowsePageKind::Album {
        for sec in &mut sections {
            for track in &mut sec.tracks {
                if track.artists.is_empty() && !artists.is_empty() {
                    track.artists = artists.clone();
                }
                if track.album.is_none() && !title.is_empty() {
                    track.album = Some(AlbumRef {
                        name: title.clone(),
                        browse_id: Some(browse_id.to_string()),
                    });
                }
            }
        }
    }

    let tracks: Vec<Track> = sections
        .iter()
        .flat_map(|s| s.tracks.iter().cloned())
        .collect();

    Ok(BrowsePage {
        browse_id: browse_id.to_string(),
        kind,
        title,
        subtitle,
        artists,
        thumbnail_url,
        sections,
        tracks,
        continuation,
    })
}

/// Parse an InnerTube `browse` continuation response into an incremental [`BrowsePage`].
pub fn parse_browse_continuation(response: &Value) -> Result<BrowsePage, CoreError> {
    check_innertube_error(response)?;
    if is_explicitly_logged_out(response) {
        return Err(CoreError::AuthExpired);
    }

    let (mut sections, continuation) = collect_browse_sections(response, &None);
    if sections.is_empty()
        && let Some(cont_items) = find_first_by_key(response, "appendContinuationItemsAction")
            .and_then(|a| a.get("continuationItems"))
            .and_then(Value::as_array)
    {
        let mut cards = Vec::new();
        let mut track_items = Vec::new();
        for item in cont_items {
            if let Some(two_row) = item.get("musicTwoRowItemRenderer")
                && let Some(card) = parse_two_row_card(two_row)
            {
                cards.push(card);
                continue;
            }
            if let Some(mrlir) = item.get("musicResponsiveListItemRenderer")
                && let Some(card) = crate::parser::browse_cards::parse_list_item_card(mrlir)
            {
                cards.push(card);
                continue;
            }
            track_items.push(item.clone());
        }
        let tracks = parse_tracks_from_items(&track_items, 0);
        if !tracks.is_empty() || !cards.is_empty() {
            sections.push(BrowseSection {
                title: "More".to_string(),
                cards,
                tracks,
                browse_id: None,
                browse_params: None,
            });
        }
    }

    let tracks: Vec<Track> = sections
        .iter()
        .flat_map(|s| s.tracks.iter().cloned())
        .collect();

    Ok(BrowsePage {
        browse_id: String::new(),
        kind: BrowsePageKind::Unknown,
        title: String::new(),
        subtitle: None,
        artists: Vec::new(),
        thumbnail_url: None,
        sections,
        tracks,
        continuation,
    })
}

/// Parse InnerTube `next` response into radio queue tracks and optional `MPLYt_...` lyrics browse ID.
pub fn parse_watch_next(response: &Value) -> Result<WatchNextResult, CoreError> {
    check_innertube_error(response)?;
    if is_explicitly_logged_out(response) {
        return Err(CoreError::AuthExpired);
    }

    let mut panel_items = Vec::new();
    find_all_by_key(response, "playlistPanelVideoRenderer", &mut panel_items);
    let tracks: Vec<Track> = panel_items
        .into_iter()
        .enumerate()
        .filter_map(|(idx, item)| parse_panel_video(item, idx))
        .collect();

    let mut tabs = Vec::new();
    find_all_by_key(response, "tabRenderer", &mut tabs);
    let lyrics_browse_id = tabs.into_iter().find_map(|tab| {
        let bid = tab.pointer("/endpoint/browseEndpoint/browseId")?.as_str()?;
        let page_type = tab
            .pointer("/endpoint/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
            .and_then(Value::as_str)
            .unwrap_or("");
        if bid.starts_with("MPLYt_") || page_type == "MUSIC_PAGE_TYPE_TRACK_LYRICS" {
            Some(bid.to_string())
        } else {
            None
        }
    });

    Ok(WatchNextResult {
        tracks,
        lyrics_browse_id,
    })
}

fn parse_panel_video(item: &Value, position: usize) -> Option<Track> {
    let video_id = item.get("videoId")?.as_str()?.to_string();
    let title = item.get("title").map(extract_runs_text)?;
    let duration_sec = item
        .get("lengthText")
        .map(extract_runs_text)
        .and_then(|s| parse_duration_seconds(&s));
    let thumbnail_url = extract_thumbnail_url(item);

    let mut artists = Vec::new();
    let mut album = None;
    if let Some(runs) = item
        .pointer("/longBylineText/runs")
        .and_then(Value::as_array)
    {
        for run in runs {
            let txt = run.get("text").and_then(Value::as_str).unwrap_or("").trim();
            if txt.is_empty() || txt == "•" || txt == "&" || txt == "," {
                continue;
            }
            if let Some(nav) = run.get("navigationEndpoint") {
                let page_type = nav
                    .pointer("/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let bid = nav
                    .pointer("/browseEndpoint/browseId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if page_type == "MUSIC_PAGE_TYPE_ALBUM" && album.is_none() {
                    album = Some(AlbumRef {
                        name: txt.to_string(),
                        browse_id: bid,
                    });
                } else if page_type == "MUSIC_PAGE_TYPE_ARTIST" {
                    artists.push(ArtistRef {
                        name: txt.to_string(),
                        browse_id: bid,
                    });
                }
            }
        }
    }
    if artists.is_empty()
        && let Some(short) = item.get("shortBylineText").map(extract_runs_text)
        && !short.is_empty()
    {
        artists.push(ArtistRef {
            name: short,
            browse_id: None,
        });
    }

    Some(Track {
        video_id,
        set_video_id: None,
        position,
        title,
        artists,
        album,
        duration_sec,
        release_year: None,
        music_video_type: MusicVideoType::AudioTrack,
        availability: TrackAvailability::Available,
        thumbnail_url,
    })
}

/// Extract plain-text song lyrics from an InnerTube `browse` lyrics response.
#[must_use]
pub fn parse_lyrics_response(response: &Value) -> Option<String> {
    if let Some(desc) = find_first_by_key(response, "musicDescriptionShelfRenderer") {
        let text = desc
            .get("description")
            .map(extract_runs_text)
            .unwrap_or_default();
        if !text.trim().is_empty() {
            return Some(text);
        }
    }
    let mut lines = Vec::new();
    find_all_by_key(response, "lyricsLine", &mut lines);
    if !lines.is_empty() {
        let joined: Vec<&str> = lines.into_iter().filter_map(Value::as_str).collect();
        if !joined.is_empty() {
            return Some(joined.join("\n"));
        }
    }
    None
}
