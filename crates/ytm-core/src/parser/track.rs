use serde_json::Value;

use crate::models::{AlbumRef, ArtistRef, MusicVideoType, Track, TrackAvailability};
use crate::parser::tree::{extract_runs_text, find_first_by_key, parse_duration_seconds};

/// Parse `musicResponsiveListItemRenderer` entries into canonical `Track` structs.
#[must_use]
pub fn parse_tracks_from_items(items: &[Value], start_pos: usize) -> Vec<Track> {
    let mut tracks = Vec::new();
    for entry in items {
        let Some(renderer) = entry.get("musicResponsiveListItemRenderer") else {
            continue;
        };
        if let Some(track) = parse_single_track(renderer, start_pos + tracks.len()) {
            tracks.push(track);
        }
    }
    tracks
}

fn parse_single_track(renderer: &Value, position: usize) -> Option<Track> {
    let video_id = extract_video_id(renderer).unwrap_or_default();
    let set_video_id = extract_set_video_id(renderer);

    let music_video_type = find_first_by_key(renderer, "watchEndpointMusicConfig")
        .and_then(|cfg| cfg.get("musicVideoType"))
        .and_then(Value::as_str)
        .map(MusicVideoType::from_innertube_str)
        .unwrap_or(MusicVideoType::Unknown);

    let (title, artists, album, mut duration_sec) = parse_columns(renderer);
    if duration_sec.is_none() {
        duration_sec = renderer
            .pointer("/fixedColumns/0/musicResponsiveListItemFixedColumnRenderer/text")
            .map(extract_runs_text)
            .and_then(|s| parse_duration_seconds(&s));
    }

    let availability = classify_availability(renderer, &title);

    if title.is_empty() && video_id.is_empty() {
        return None;
    }

    Some(Track {
        video_id,
        set_video_id,
        position,
        title,
        artists,
        album,
        duration_sec,
        release_year: None,
        music_video_type,
        availability,
    })
}

fn extract_video_id(renderer: &Value) -> Option<String> {
    renderer
        .pointer("/playlistItemData/videoId")
        .and_then(Value::as_str)
        .or_else(|| {
            find_first_by_key(renderer, "watchEndpoint")
                .and_then(|w| w.get("videoId"))
                .and_then(Value::as_str)
        })
        .or_else(|| find_first_by_key(renderer, "removedVideoId").and_then(Value::as_str))
        .map(str::to_string)
}

fn extract_set_video_id(renderer: &Value) -> Option<String> {
    // Check `playlistItemData` first: greyed-out unavailable tracks omit `menu` yet preserve this ID.
    renderer
        .pointer("/playlistItemData/playlistSetVideoId")
        .and_then(Value::as_str)
        .or_else(|| {
            find_first_by_key(renderer, "watchEndpoint")
                .and_then(|w| w.get("playlistSetVideoId"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            find_first_by_key(renderer, "playlistEditEndpoint")
                .and_then(|pe| find_first_by_key(pe, "setVideoId"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

fn parse_columns(renderer: &Value) -> (String, Vec<ArtistRef>, Option<AlbumRef>, Option<u32>) {
    let mut title = String::new();
    let mut artists = Vec::new();
    let mut album: Option<AlbumRef> = None;
    let mut duration_sec: Option<u32> = None;
    let mut fallback_texts: Vec<String> = Vec::new();

    let cols = renderer
        .get("flexColumns")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    for col in cols {
        let Some(text_obj) = col.pointer("/musicResponsiveListItemFlexColumnRenderer/text") else {
            continue;
        };
        fallback_texts.push(extract_runs_text(text_obj));

        let Some(runs) = text_obj.get("runs").and_then(Value::as_array) else {
            continue;
        };
        for run in runs {
            let run_text = run.get("text").and_then(Value::as_str).unwrap_or("").trim();
            if run_text.is_empty() || run_text == "•" || run_text == "&" || run_text == "," {
                continue;
            }
            if let Some(nav) = run.get("navigationEndpoint") {
                if nav.get("watchEndpoint").is_some() {
                    if title.is_empty() {
                        title = run_text.to_string();
                    }
                    continue;
                }
                let page_type = nav
                    .pointer("/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let browse_id = nav
                    .pointer("/browseEndpoint/browseId")
                    .and_then(Value::as_str)
                    .map(str::to_string);

                match page_type {
                    "MUSIC_PAGE_TYPE_NON_MUSIC_AUDIO_TRACK_PAGE" if title.is_empty() => {
                        title = run_text.to_string();
                    }
                    "MUSIC_PAGE_TYPE_ALBUM" | "MUSIC_PAGE_TYPE_AUDIOBOOK" if album.is_none() => {
                        album = Some(AlbumRef {
                            name: run_text.to_string(),
                            browse_id,
                        });
                    }
                    "MUSIC_PAGE_TYPE_ARTIST"
                    | "MUSIC_PAGE_TYPE_UNKNOWN"
                    | "MUSIC_PAGE_TYPE_USER_CHANNEL" => {
                        artists.push(ArtistRef {
                            name: run_text.to_string(),
                            browse_id,
                        });
                    }
                    _ => {}
                }
            } else if duration_sec.is_none() {
                if let Some(secs) = parse_duration_seconds(run_text) {
                    duration_sec = Some(secs);
                }
            }
        }
    }

    if title.is_empty() {
        if let Some(first) = fallback_texts.first() {
            title = first.clone();
        }
    }
    if artists.is_empty() {
        if let Some(second) = fallback_texts.get(1) {
            if !second.is_empty() && parse_duration_seconds(second).is_none() {
                artists.push(ArtistRef {
                    name: second.clone(),
                    browse_id: None,
                });
            }
        }
    }

    (title, artists, album, duration_sec)
}

fn classify_availability(renderer: &Value, title: &str) -> TrackAvailability {
    let lower = title.to_ascii_lowercase();
    if lower == "[deleted video]" || lower == "song deleted" {
        return TrackAvailability::Deleted;
    }
    if lower == "[private video]" {
        return TrackAvailability::Private;
    }
    let policy = renderer
        .get("musicItemRendererDisplayPolicy")
        .and_then(Value::as_str)
        .unwrap_or("");
    if policy == "MUSIC_ITEM_RENDERER_DISPLAY_POLICY_GREY_OUT" {
        return TrackAvailability::Unplayable("greyed out in InnerTube".to_string());
    }
    TrackAvailability::Available
}
