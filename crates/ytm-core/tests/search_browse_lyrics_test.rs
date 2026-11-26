use serde_json::json;
use ytm_core::{
    AuthCredentials, BROWSE_EXPLORE, BROWSE_HISTORY, BROWSE_HOME, BROWSE_LIKED_ALBUMS,
    BROWSE_LIKED_PLAYLISTS, BrowseCardKind, ClientConfig, InnerTubeClient, InnerTubeClientTrait,
    LyricsPayload, MockTransport, SearchFilter, TimedLyricLine,
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
async fn filtered_search_browse_continuations_and_timed_lyrics_work() {
    assert_eq!(SearchFilter::Top.innertube_params(), None);
    assert!(SearchFilter::Songs.innertube_params().is_some());
    assert!(SearchFilter::Videos.innertube_params().is_some());
    assert!(SearchFilter::Albums.innertube_params().is_some());
    assert!(SearchFilter::Artists.innertube_params().is_some());
    assert!(
        SearchFilter::CommunityPlaylists
            .innertube_params()
            .is_some()
    );
    assert_eq!(BROWSE_HOME, "FEmusic_home");
    assert_eq!(BROWSE_EXPLORE, "FEmusic_explore");
    assert_eq!(BROWSE_HISTORY, "FEmusic_history");
    assert_eq!(BROWSE_LIKED_ALBUMS, "FEmusic_liked_albums");
    assert_eq!(BROWSE_LIKED_PLAYLISTS, "FEmusic_liked_playlists");

    let search_albums = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicShelfRenderer": {
                                        "contents": [
                                            {
                                                "musicResponsiveListItemRenderer": {
                                                    "flexColumns": [
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": {
                                                                    "runs": [{
                                                                        "text": "Animals",
                                                                        "navigationEndpoint": {
                                                                            "browseEndpoint": {
                                                                                "browseId": "MPREb_ANIMALS",
                                                                                "browseEndpointContextSupportedConfigs": {
                                                                                    "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_ALBUM" }
                                                                                }
                                                                            }
                                                                        }
                                                                    }]
                                                                }
                                                            }
                                                        },
                                                        { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Pink Floyd" }] } } }
                                                    ]
                                                }
                                            },
                                            {
                                                "continuationItemRenderer": {
                                                    "continuationEndpoint": { "continuationCommand": { "token": "SEARCH_CONT_2" } }
                                                }
                                            }
                                        ]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });
    let search_cont = json!({
        "onResponseReceivedActions": [{
            "appendContinuationItemsAction": {
                "continuationItems": [{
                    "musicResponsiveListItemRenderer": {
                        "playlistItemData": { "videoId": "vid_song_99" },
                        "overlay": { "musicItemThumbnailOverlayRenderer": { "content": { "musicPlayButtonRenderer": { "playNavigationEndpoint": { "watchEndpoint": { "videoId": "vid_song_99" } } } } } },
                        "flexColumns": [{ "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Pigs" }] } } }]
                    }
                }]
            }
        }]
    });
    let liked_albums = json!({
        "contents": {
            "singleColumnBrowseResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "gridRenderer": {
                                        "items": [
                                            {
                                                "musicTwoRowItemRenderer": {
                                                    "title": { "runs": [{ "text": "Meddle" }] },
                                                    "subtitle": { "runs": [{ "text": "Pink Floyd" }] },
                                                    "navigationEndpoint": { "browseEndpoint": { "browseId": "MPREb_MEDDLE" } }
                                                }
                                            },
                                            {
                                                "continuationItemRenderer": {
                                                    "continuationEndpoint": { "continuationCommand": { "token": "BROWSE_CONT_2" } }
                                                }
                                            }
                                        ]
                                    }
                                }]
                            }
                        }
                    }
                }]
            }
        }
    });
    let browse_cont = json!({
        "onResponseReceivedActions": [{
            "appendContinuationItemsAction": {
                "continuationItems": [{
                    "musicTwoRowItemRenderer": {
                        "title": { "runs": [{ "text": "Atom Heart Mother" }] },
                        "subtitle": { "runs": [{ "text": "Pink Floyd" }] },
                        "navigationEndpoint": { "browseEndpoint": { "browseId": "MPREb_AHM" } }
                    }
                }]
            }
        }]
    });
    let timed_lyrics = json!({
        "contents": {
            "elementRenderer": {
                "newElement": {
                    "type": {
                        "componentType": {
                            "model": {
                                "timedLyricsModel": {
                                    "lyricsData": {
                                        "timedLyricsData": [
                                            {
                                                "lyricLine": "First line",
                                                "cueRange": { "startTimeMilliseconds": "1000", "endTimeMilliseconds": "2500" }
                                            },
                                            {
                                                "lyricLine": "Second line",
                                                "cueRange": { "startTimeMilliseconds": 2500, "endTimeMilliseconds": 4000 }
                                            }
                                        ]
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    let plain_lyrics = json!({
        "contents": { "sectionListRenderer": { "contents": [{ "musicDescriptionShelfRenderer": { "description": { "runs": [{ "text": "Plain verse" }] } } }] } }
    });

    let transport = MockTransport::with_responses(vec![
        Ok(search_albums),
        Ok(search_cont),
        Ok(liked_albums.clone()),
        Ok(liked_albums.clone()),
        Ok(liked_albums),
        Ok(browse_cont),
        Ok(timed_lyrics),
        Ok(plain_lyrics),
    ]);
    let client = InnerTubeClient::new(transport, test_creds(), fast_config());

    let albums = client
        .search_catalog("Pink Floyd", SearchFilter::Albums, None)
        .await
        .expect("search albums");
    assert_eq!(albums.cards.len(), 1);
    assert_eq!(albums.cards[0].kind, BrowseCardKind::Album);
    assert_eq!(albums.continuation.as_deref(), Some("SEARCH_CONT_2"));

    let more_search = client
        .search_catalog("Pink Floyd", SearchFilter::Albums, Some("SEARCH_CONT_2"))
        .await
        .expect("search continuation");
    assert_eq!(more_search.tracks.len(), 1);
    assert_eq!(more_search.tracks[0].video_id, "vid_song_99");

    let explore = client
        .get_browse_page(BROWSE_EXPLORE)
        .await
        .expect("explore");
    assert_eq!(explore.continuation.as_deref(), Some("BROWSE_CONT_2"));
    let history = client
        .get_browse_page(BROWSE_HISTORY)
        .await
        .expect("history");
    assert_eq!(history.browse_id, BROWSE_HISTORY);
    let albums_page = client
        .get_browse_page(BROWSE_LIKED_ALBUMS)
        .await
        .expect("liked albums");
    assert_eq!(albums_page.sections[0].cards[0].title, "Meddle");

    let cont_page = client
        .get_browse_continuation("BROWSE_CONT_2")
        .await
        .expect("browse continuation");
    assert_eq!(cont_page.sections[0].cards[0].title, "Atom Heart Mother");

    let synced = client
        .get_lyrics_payload("MPLYt_synced")
        .await
        .expect("synced lyrics")
        .expect("some payload");
    assert_eq!(
        synced,
        LyricsPayload::Synced(vec![
            TimedLyricLine {
                start_ms: 1000,
                end_ms: 2500,
                text: "First line".into()
            },
            TimedLyricLine {
                start_ms: 2500,
                end_ms: 4000,
                text: "Second line".into()
            },
        ])
    );

    let plain = client
        .get_lyrics_payload("MPLYt_plain")
        .await
        .expect("plain lyrics")
        .expect("some payload");
    assert_eq!(plain, LyricsPayload::Plain("Plain verse".into()));

    let reqs = client.transport().recorded_requests();
    assert_eq!(reqs[0].endpoint, "search");
    assert_eq!(
        reqs[0].body["params"],
        SearchFilter::Albums.innertube_params().unwrap()
    );
    assert_eq!(reqs[1].body["continuation"], "SEARCH_CONT_2");
    assert_eq!(reqs[5].body["continuation"], "BROWSE_CONT_2");
}
