use serde_json::json;
use ytm_core::{
    AuthCredentials, BrowseCardKind, BrowsePageKind, ClientConfig, CoreError, InnerTubeClient,
    InnerTubeClientTrait, MockTransport, PlaylistPrivacy,
};

fn test_creds() -> AuthCredentials {
    AuthCredentials {
        cookie: "__Secure-3PAPISID=test_sapisid_secret; SID=sid123".to_string(),
        user_agent: "TestAgent/1.0".to_string(),
        x_goog_authuser: Some("1".to_string()),
        x_goog_page_id: None,
        x_goog_visitor_id: Some("VISITOR_123".to_string()),
    }
}

fn fast_config() -> ClientConfig {
    ClientConfig {
        mutation_delay_ms: 0,
        ..ClientConfig::default()
    }
}

#[tokio::test]
async fn update_playlist_metadata_and_like_status_send_expected_payloads() {
    let transport = MockTransport::with_responses(vec![
        Ok(json!({ "status": "STATUS_SUCCEEDED" })),
        Ok(json!({})),
        Ok(json!({})),
    ]);
    let client = InnerTubeClient::new(transport, test_creds(), fast_config());

    let bad = client
        .update_playlist_metadata("PL1", Some("Bad<Title>"), None, None)
        .await;
    assert!(matches!(bad, Err(CoreError::InvalidInput(_))));

    client
        .update_playlist_metadata(
            "VLPL1",
            Some("Renamed"),
            Some("New desc"),
            Some(PlaylistPrivacy::Unlisted),
        )
        .await
        .expect("update meta");
    client.set_like_status("vid_1", true).await.expect("like");
    client
        .set_like_status("vid_1", false)
        .await
        .expect("unlike");

    let reqs = client.transport().recorded_requests();
    assert_eq!(reqs.len(), 3);
    assert_eq!(reqs[0].endpoint, "browse/edit_playlist");
    assert_eq!(reqs[0].body["playlistId"], "PL1");
    assert_eq!(
        reqs[0].body["actions"][0]["action"],
        "ACTION_SET_PLAYLIST_NAME"
    );
    assert_eq!(
        reqs[0].body["actions"][1]["action"],
        "ACTION_SET_PLAYLIST_DESCRIPTION"
    );
    assert_eq!(
        reqs[0].body["actions"][2]["action"],
        "ACTION_SET_PLAYLIST_PRIVACY"
    );
    assert_eq!(reqs[1].endpoint, "like/like");
    assert_eq!(reqs[1].body["target"]["videoId"], "vid_1");
    assert_eq!(reqs[2].endpoint, "like/removelike");
    assert_eq!(reqs[2].body["target"]["videoId"], "vid_1");
}

#[tokio::test]
async fn browse_watch_next_and_lyrics_parse_correctly() {
    let home_json = json!({
        "contents": {
            "singleColumnBrowseResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicCarouselShelfRenderer": {
                                        "header": {
                                            "musicCarouselShelfBasicHeaderRenderer": {
                                                "title": { "runs": [{ "text": "Quick picks" }] }
                                            }
                                        },
                                        "contents": [{
                                            "musicTwoRowItemRenderer": {
                                                "title": {
                                                    "runs": [{ "text": "Dark Side" }],
                                                    "navigationEndpoint": {
                                                        "browseEndpoint": {
                                                            "browseId": "MPREb_123",
                                                            "browseEndpointContextSupportedConfigs": {
                                                                "browseEndpointContextMusicConfig": {
                                                                    "pageType": "MUSIC_PAGE_TYPE_ALBUM"
                                                                }
                                                            }
                                                        }
                                                    }
                                                },
                                                "subtitle": { "runs": [{ "text": "Pink Floyd" }] },
                                                "thumbnailRenderer": {
                                                    "musicThumbnailRenderer": {
                                                        "thumbnail": { "thumbnails": [{ "url": "https://img/a.jpg", "width": 120 }] }
                                                    }
                                                }
                                            }
                                        }]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });
    let next_json = json!({
        "contents": {
            "singleColumnMusicWatchNextResultsRenderer": {
                "tabbedRenderer": {
                    "watchNextTabbedResultsRenderer": {
                        "tabs": [
                            {
                                "tabRenderer": {
                                    "content": {
                                        "musicQueueRenderer": {
                                            "content": {
                                                "playlistPanelRenderer": {
                                                    "contents": [{
                                                        "playlistPanelVideoRenderer": {
                                                            "videoId": "vid_radio_1",
                                                            "title": { "runs": [{ "text": "Echoes" }] },
                                                            "lengthText": { "runs": [{ "text": "3:45" }] },
                                                            "thumbnail": { "thumbnails": [{ "url": "https://img/r.jpg" }] },
                                                            "shortBylineText": { "runs": [{ "text": "Pink Floyd" }] }
                                                        }
                                                    }]
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            {
                                "tabRenderer": {
                                    "endpoint": {
                                        "browseEndpoint": { "browseId": "MPLYt_abc" }
                                    }
                                }
                            }
                        ]
                    }
                }
            }
        }
    });
    let lyrics_json = json!({
        "contents": {
            "sectionListRenderer": {
                "contents": [{
                    "musicDescriptionShelfRenderer": {
                        "description": { "runs": [{ "text": "Line 1\nLine 2" }] }
                    }
                }]
            }
        }
    });

    let transport =
        MockTransport::with_responses(vec![Ok(home_json), Ok(next_json), Ok(lyrics_json)]);
    let client = InnerTubeClient::new(transport, test_creds(), fast_config());

    let page = client
        .get_browse_page("FEmusic_home")
        .await
        .expect("browse home");
    assert_eq!(page.kind, BrowsePageKind::Home);
    assert_eq!(page.sections.len(), 1);
    assert_eq!(page.sections[0].cards[0].kind, BrowseCardKind::Album);
    assert_eq!(
        page.sections[0].cards[0].browse_id.as_deref(),
        Some("MPREb_123")
    );
    assert_eq!(
        page.sections[0].cards[0].thumbnail_url.as_deref(),
        Some("https://img/a.jpg")
    );

    let next = client
        .get_watch_next("vid_seed", Some("VLPL1"))
        .await
        .expect("watch next");
    assert_eq!(next.lyrics_browse_id.as_deref(), Some("MPLYt_abc"));
    assert_eq!(next.tracks.len(), 1);
    assert_eq!(next.tracks[0].video_id, "vid_radio_1");
    assert_eq!(next.tracks[0].duration_sec, Some(225));
    assert_eq!(
        next.tracks[0].effective_thumbnail_url().as_deref(),
        Some("https://img/r.jpg")
    );

    let lyrics = client.get_lyrics("MPLYt_abc").await.expect("lyrics");
    assert_eq!(lyrics.as_deref(), Some("Line 1\nLine 2"));
}

#[tokio::test]
async fn get_search_suggestions_parses_fixture_and_sends_input() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/search_suggestions.json"
    ))
    .expect("parse fixture");
    let transport = MockTransport::with_responses(vec![Ok(fixture)]);
    let client = InnerTubeClient::new(transport, test_creds(), fast_config());

    let suggestions = client
        .get_search_suggestions("daft")
        .await
        .expect("suggestions");
    assert_eq!(
        suggestions,
        vec![
            "daft punk",
            "daft punk around the world",
            "daft punk discovery",
        ]
    );
    let reqs = client.transport().recorded_requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].endpoint, "music/get_search_suggestions");
    assert_eq!(reqs[0].body["input"], "daft");
}
