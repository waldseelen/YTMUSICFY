use serde_json::Value;

use crate::browse_models::{BrowseCard, BrowseCardKind, BrowseSection};
use crate::parser::track::{extract_thumbnail_url, parse_tracks_from_items};
use crate::parser::tree::{extract_runs_text, find_first_by_key, to_browse_playlist_id};

pub(crate) fn classify_browse_card_kind(
    browse_id: &str,
    page_type: Option<&str>,
) -> BrowseCardKind {
    match page_type.unwrap_or("") {
        "MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL" => BrowseCardKind::Artist,
        "MUSIC_PAGE_TYPE_ALBUM" | "MUSIC_PAGE_TYPE_AUDIOBOOK" => BrowseCardKind::Album,
        "MUSIC_PAGE_TYPE_PLAYLIST" => BrowseCardKind::Playlist,
        _ if browse_id.starts_with("UC")
            || browse_id.starts_with("MPLA")
            || browse_id.starts_with("MPAD") =>
        {
            BrowseCardKind::Artist
        }
        _ if browse_id.starts_with("MPRE") => BrowseCardKind::Album,
        _ => BrowseCardKind::Playlist,
    }
}

pub(crate) fn browse_endpoint_page_type(endpoint: &Value) -> Option<&str> {
    endpoint
        .pointer("/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
        .and_then(Value::as_str)
}

fn is_browse_entity_id(browse_id: &str, page_type: Option<&str>) -> bool {
    matches!(
        page_type,
        Some(
            "MUSIC_PAGE_TYPE_ARTIST"
                | "MUSIC_PAGE_TYPE_USER_CHANNEL"
                | "MUSIC_PAGE_TYPE_ALBUM"
                | "MUSIC_PAGE_TYPE_AUDIOBOOK"
                | "MUSIC_PAGE_TYPE_PLAYLIST"
                | "MUSIC_PAGE_TYPE_PODCAST_SHOW_DETAIL_PAGE"
        )
    ) || browse_id.starts_with("UC")
        || browse_id.starts_with("MPLA")
        || browse_id.starts_with("MPAD")
        || browse_id.starts_with("MPRE")
        || browse_id.starts_with("VL")
        || browse_id.starts_with("PL")
        || browse_id.starts_with("OLAK")
        || browse_id.starts_with("RD")
        || browse_id.starts_with("MPSP")
}

fn extract_section_endpoint(shelf: &Value) -> (Option<String>, Option<String>) {
    let ep = shelf
        .pointer("/bottomEndpoint/browseEndpoint")
        .or_else(|| {
            shelf
                .get("header")
                .and_then(|h| find_first_by_key(h, "moreContentButton"))
                .and_then(|b| find_first_by_key(b, "browseEndpoint"))
        })
        .or_else(|| {
            shelf
                .get("header")
                .and_then(|h| find_first_by_key(h, "browseEndpoint"))
        })
        .or_else(|| {
            shelf
                .get("title")
                .and_then(|t| find_first_by_key(t, "browseEndpoint"))
        });

    let browse_id = ep
        .and_then(|e| e.get("browseId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let browse_params = ep
        .and_then(|e| e.get("params"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    (browse_id, browse_params)
}

/// Parse an Album, EP, Artist, or Playlist `musicResponsiveListItemRenderer` into a [`BrowseCard`].
pub(crate) fn parse_list_item_card(renderer: &Value) -> Option<BrowseCard> {
    let has_item_video = renderer
        .pointer("/playlistItemData/videoId")
        .and_then(Value::as_str)
        .is_some_and(|v| !v.trim().is_empty());
    if has_item_video {
        return None;
    }

    let first_col = renderer
        .get("flexColumns")
        .and_then(Value::as_array)
        .and_then(|cols| cols.first());
    if let Some(col0) = first_col
        && find_first_by_key(col0, "watchEndpoint").is_some()
    {
        return None;
    }

    let browse_ep = renderer
        .get("navigationEndpoint")
        .and_then(|n| find_first_by_key(n, "browseEndpoint"))
        .or_else(|| first_col.and_then(|c| find_first_by_key(c, "browseEndpoint")))?;
    let browse_id = browse_ep
        .get("browseId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let page_type = browse_endpoint_page_type(browse_ep);
    if !is_browse_entity_id(&browse_id, page_type) {
        return None;
    }

    let title = first_col
        .and_then(|c| find_first_by_key(c, "text"))
        .map(extract_runs_text)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| browse_id.clone());

    let subtitle = renderer
        .get("flexColumns")
        .and_then(Value::as_array)
        .map(|cols| {
            cols.iter()
                .skip(1)
                .filter_map(|c| find_first_by_key(c, "text"))
                .map(extract_runs_text)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" • ")
        })
        .unwrap_or_default();

    Some(BrowseCard {
        kind: classify_browse_card_kind(&browse_id, page_type),
        browse_id: Some(browse_id),
        video_id: None,
        title,
        subtitle,
        thumbnail_url: extract_thumbnail_url(renderer),
    })
}

pub(crate) fn parse_shelf(
    shelf: &Value,
    contents_key: &str,
    fallback_thumb: &Option<String>,
) -> Option<BrowseSection> {
    let items = shelf.get(contents_key).and_then(Value::as_array)?;
    let header_title = find_first_by_key(shelf, "musicCarouselShelfBasicHeaderRenderer")
        .and_then(|h| h.get("title"));
    let raw_title = header_title
        .or_else(|| shelf.get("title"))
        .map(extract_runs_text)
        .unwrap_or_default();

    let mut cards = Vec::new();
    let mut track_items = Vec::new();
    for item in items {
        if let Some(two_row) = item
            .get("musicTwoRowItemRenderer")
            .or_else(|| item.get("musicGridItemRenderer"))
        {
            if let Some(card) = parse_two_row_card(two_row) {
                cards.push(card);
            }
            continue;
        }
        if let Some(mrlir) = item.get("musicResponsiveListItemRenderer")
            && let Some(card) = parse_list_item_card(mrlir)
        {
            cards.push(card);
            continue;
        }
        track_items.push(item.clone());
    }

    let mut tracks = parse_tracks_from_items(&track_items, 0);
    if let Some(thumb) = fallback_thumb {
        for t in &mut tracks {
            if t.thumbnail_url.is_none() {
                t.thumbnail_url = Some(thumb.clone());
            }
        }
    }

    if tracks.is_empty() && cards.is_empty() {
        return None;
    }

    let title = if !raw_title.is_empty() {
        raw_title
    } else if !tracks.is_empty() {
        "Tracks".to_string()
    } else {
        "Items".to_string()
    };
    let (browse_id, browse_params) = extract_section_endpoint(shelf);

    Some(BrowseSection {
        title,
        cards,
        tracks,
        browse_id,
        browse_params,
    })
}

pub(crate) fn parse_card_shelf(
    shelf: &Value,
    fallback_thumb: &Option<String>,
) -> Option<BrowseSection> {
    let header_title = find_first_by_key(shelf, "musicCardShelfHeaderBasicRenderer")
        .and_then(|h| h.get("title"))
        .map(extract_runs_text)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Featured".to_string());

    let mut cards = Vec::new();
    if let Some(card) = parse_two_row_card(shelf) {
        cards.push(card);
    }

    let items = shelf
        .get("contents")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut track_items = Vec::new();
    for item in items {
        if let Some(mrlir) = item.get("musicResponsiveListItemRenderer")
            && let Some(card) = parse_list_item_card(mrlir)
        {
            cards.push(card);
            continue;
        }
        track_items.push(item.clone());
    }
    let mut tracks = parse_tracks_from_items(&track_items, 0);
    if let Some(thumb) = fallback_thumb {
        for t in &mut tracks {
            if t.thumbnail_url.is_none() {
                t.thumbnail_url = Some(thumb.clone());
            }
        }
    }

    if tracks.is_empty() && cards.is_empty() {
        return None;
    }
    let (browse_id, browse_params) = extract_section_endpoint(shelf);
    Some(BrowseSection {
        title: header_title,
        cards,
        tracks,
        browse_id,
        browse_params,
    })
}

pub(crate) fn parse_two_row_card(renderer: &Value) -> Option<BrowseCard> {
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
        .or_else(|| renderer.get("onTap"))
        .or_else(|| {
            renderer
                .get("title")
                .and_then(|t| find_first_by_key(t, "navigationEndpoint"))
        })
        .or_else(|| find_first_by_key(renderer, "playNavigationEndpoint"));

    let watch_ep = nav.and_then(|n| find_first_by_key(n, "watchEndpoint"));
    let browse_ep = nav.and_then(|n| find_first_by_key(n, "browseEndpoint"));
    let watch_playlist_ep = nav
        .and_then(|n| find_first_by_key(n, "watchPlaylistEndpoint"))
        .or_else(|| find_first_by_key(renderer, "watchPlaylistEndpoint"));

    let video_id = watch_ep
        .and_then(|w| w.get("videoId"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let raw_browse_id = browse_ep
        .and_then(|b| b.get("browseId"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            if video_id.is_none() {
                watch_playlist_ep
                    .and_then(|w| w.get("playlistId"))
                    .and_then(Value::as_str)
                    .map(to_browse_playlist_id)
            } else {
                None
            }
        });

    let page_type = browse_ep
        .and_then(|b| find_first_by_key(b, "pageType"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let kind = if video_id.is_some() {
        BrowseCardKind::Track
    } else if let Some(bid) = &raw_browse_id {
        if page_type == "MUSIC_PAGE_TYPE_ARTIST" || bid.starts_with("UC") || bid.starts_with("MPLA")
        {
            BrowseCardKind::Artist
        } else if page_type == "MUSIC_PAGE_TYPE_ALBUM" || bid.starts_with("MPREb_") {
            BrowseCardKind::Album
        } else {
            BrowseCardKind::Playlist
        }
    } else {
        return None;
    };

    Some(BrowseCard {
        kind,
        browse_id: raw_browse_id,
        video_id,
        title,
        subtitle,
        thumbnail_url,
    })
}
