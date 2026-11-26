use serde_json::json;

use super::{extract_continuation_token, parse_library_playlists, parse_playlist_page_detailed};

#[test]
fn parse_library_playlists_handles_localized_counts_and_grid_continuation() {
    let resp = json!({
        "contents": {
            "gridRenderer": {
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
                        "continuationItemRenderer": {
                            "continuationEndpoint": {
                                "continuationCommand": { "token": "LIB_NEXT_2" }
                            }
                        }
                    }
                ]
            }
        }
    });

    let (parsed, next) = parse_library_playlists(&resp).expect("library playlists");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].id, "LM");
    assert_eq!(parsed[1].track_count, Some(1245));
    assert_eq!(next.as_deref(), Some("LIB_NEXT_2"));
}

#[test]
fn extract_continuation_token_ignores_trailing_message_renderer() {
    let items = vec![
        json!({ "continuationItemRenderer": { "continuationEndpoint": { "continuationCommand": { "token": "PAGE_2" } } } }),
        json!({ "messageRenderer": { "text": { "runs": [{ "text": "Unavailable videos are hidden" }] } } }),
    ];
    assert_eq!(
        extract_continuation_token(&json!({}), &items).as_deref(),
        Some("PAGE_2")
    );
}

#[test]
fn parse_playlist_page_detailed_extracts_header_declared_count() {
    let raw = include_str!("../../../../tests/fixtures/playlist_browse_page1.json");
    let val: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
    let parsed = parse_playlist_page_detailed(&val, "PLTestPlaylist01").expect("parse");
    assert_eq!(parsed.declared_track_count, Some(3));
    assert_eq!(parsed.next_token.as_deref(), Some("CONT_TOKEN_PAGE_2"));
    assert!(parsed.dropped_renderers.is_empty());
}
