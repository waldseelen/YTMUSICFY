use serde_json::Value;

use crate::browse_models::{
    BrowseCard, BrowseCardKind, BrowsePage, BrowsePageKind, BrowseSection, WatchNextResult,
};
use crate::error::CoreError;
use crate::models::{AlbumRef, ArtistRef, MusicVideoType, Track, TrackAvailability};
use crate::parser::track::{extract_thumbnail_url, parse_tracks_from_items};
use crate::parser::tree::{
    check_innertube_error, extract_runs_text, find_all_by_key, find_first_by_key,
    is_explicitly_logged_out, parse_duration_seconds,
};

/// Parse InnerTube `browse` response for Home (`FEmusic_home`), Artist, or Album pages.
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
    let subtitle = header
        .and_then(|h| h.get("subtitle").or_else(|| h.get("description")))
        .map(extract_runs_text)
        .filter(|s| !s.is_empty());
    let thumbnail_url = header.and_then(extract_thumbnail_url);

    let mut sections = Vec::new();
    let mut carousels = Vec::new();
    find_all_by_key(response, "musicCarouselShelfRenderer", &mut carousels);
    for shelf in carousels {
        if let Some(sec) = parse_shelf(shelf, "contents", &thumbnail_url) {
            sections.push(sec);
        }
    }

    let mut shelves = Vec::new();
    find_all_by_key(response, "musicShelfRenderer", &mut shelves);
    for shelf in shelves {
        if let Some(sec) = parse_shelf(shelf, "contents", &thumbnail_url) {
            sections.push(sec);
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
        thumbnail_url,
        sections,
        tracks,
    })
}

fn parse_shelf(
    shelf: &Value,
    contents_key: &str,
    fallback_thumb: &Option<String>,
) -> Option<BrowseSection> {
    let items = shelf.get(contents_key).and_then(Value::as_array)?;
    let title = shelf
        .pointer("/header/musicCarouselShelfBasicHeaderRenderer/title")
        .or_else(|| shelf.get("title"))
        .map(extract_runs_text)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Items".to_string());

    let mut tracks = parse_tracks_from_items(items, 0);
    if let Some(thumb) = fallback_thumb {
        for t in &mut tracks {
            if t.thumbnail_url.is_none() {
                t.thumbnail_url = Some(thumb.clone());
            }
        }
    }

    let mut cards = Vec::new();
    for item in items {
        if let Some(two_row) = item.get("musicTwoRowItemRenderer")
            && let Some(card) = parse_two_row_card(two_row)
        {
            cards.push(card);
        }
    }

    if tracks.is_empty() && cards.is_empty() {
        return None;
    }
    Some(BrowseSection {
        title,
        cards,
        tracks,
    })
}

fn parse_two_row_card(renderer: &Value) -> Option<BrowseCard> {
    let title = renderer.get("title").map(extract_runs_text)?;
    if title.is_empty() {
        return None;
    }
    let subtitle = renderer
        .get("subtitle")
        .map(extract_runs_text)
        .unwrap_or_default();
    let thumbnail_url = extract_thumbnail_url(renderer);
    let nav = renderer
        .get("navigationEndpoint")
        .or_else(|| renderer.pointer("/title/navigationEndpoint"))
        .or_else(|| renderer.pointer("/title/runs/0/navigationEndpoint"));
    let video_id = nav
        .and_then(|n| n.pointer("/watchEndpoint/videoId"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let browse_id = nav
        .and_then(|n| n.pointer("/browseEndpoint/browseId"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let kind = if video_id.is_some() {
        BrowseCardKind::Track
    } else if let Some(bid) = &browse_id {
        if bid.starts_with("UC") || bid.starts_with("MPLA") {
            BrowseCardKind::Artist
        } else if bid.starts_with("MPREb_") {
            BrowseCardKind::Album
        } else {
            BrowseCardKind::Playlist
        }
    } else {
        return None;
    };

    Some(BrowseCard {
        kind,
        browse_id,
        video_id,
        title,
        subtitle,
        thumbnail_url,
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
    let mut tracks = Vec::new();
    for (idx, item) in panel_items.into_iter().enumerate() {
        if let Some(t) = parse_panel_video(item, idx) {
            tracks.push(t);
        }
    }

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
