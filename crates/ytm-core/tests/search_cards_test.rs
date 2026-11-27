use serde_json::json;
use ytm_core::{
    AuthCredentials, BrowseCardKind, ClientConfig, InnerTubeClient, InnerTubeClientTrait,
    MockTransport, SearchFilter,
};

fn creds() -> AuthCredentials {
    AuthCredentials {
        cookie: "__Secure-3PAPISID=secret; SID=sid".to_string(),
        user_agent: "TestAgent/1.0".to_string(),
        x_goog_authuser: Some("0".to_string()),
        x_goog_page_id: None,
        x_goog_visitor_id: None,
    }
}

fn fast_cfg() -> ClientConfig {
    ClientConfig {
        mutation_delay_ms: 0,
        ..ClientConfig::default()
    }
}

#[tokio::test]
async fn search_keeps_albums_eps_and_community_playlists_with_play_overlay() {
    let search_resp = json!({
        "contents": {
            "tabbedSearchResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [
                                    {
                                        "musicCardShelfRenderer": {
                                            "title": { "runs": [{ "text": "Neşet Ertaş" }] },
                                            "subtitle": { "runs": [{ "text": "Sanatçı • 8,54 Mn dinleyici" }] },
                                            "onTap": { "browseEndpoint": { "browseId": "UCxuSb9oqYBpO4nDSRHPknzw" } }
                                        }
                                    },
                                    {
                                        "musicShelfRenderer": {
                                            "contents": [
                                                {
                                                    "musicResponsiveListItemRenderer": {
                                                        "navigationEndpoint": {
                                                            "browseEndpoint": {
                                                                "browseId": "MPREb_EFES",
                                                                "browseEndpointContextSupportedConfigs": {
                                                                    "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_ALBUM" }
                                                                }
                                                            }
                                                        },
                                                        "overlay": {
                                                            "musicItemThumbnailOverlayRenderer": {
                                                                "content": {
                                                                    "musicPlayButtonRenderer": {
                                                                        "playNavigationEndpoint": {
                                                                            "watchPlaylistEndpoint": { "playlistId": "OLAK5uy_efes" }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        },
                                                        "flexColumns": [
                                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Efes Plak Yılları" }] } } },
                                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Albüm • Neşet Ertaş • 2025" }] } } }
                                                        ]
                                                    }
                                                },
                                                {
                                                    "musicResponsiveListItemRenderer": {
                                                        "navigationEndpoint": {
                                                            "browseEndpoint": {
                                                                "browseId": "VLPL_COMMUNITY_1",
                                                                "browseEndpointContextSupportedConfigs": {
                                                                    "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_PLAYLIST" }
                                                                }
                                                            }
                                                        },
                                                        "overlay": {
                                                            "musicItemThumbnailOverlayRenderer": {
                                                                "content": {
                                                                    "musicPlayButtonRenderer": {
                                                                        "playNavigationEndpoint": {
                                                                            "watchPlaylistEndpoint": { "playlistId": "PL_COMMUNITY_1" }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        },
                                                        "flexColumns": [
                                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Bozkırın Efsaneleri" }] } } },
                                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Dinleyici Topluluğu • 45 şarkı" }] } } }
                                                        ]
                                                    }
                                                }
                                            ]
                                        }
                                    }
                                ]
                            }
                        }
                    }
                }]
            }
        }
    });

    let client = InnerTubeClient::new(
        MockTransport::with_responses(vec![Ok(search_resp)]),
        creds(),
        fast_cfg(),
    );
    let page = client
        .search_catalog("Neşet Ertaş", SearchFilter::CommunityPlaylists, None)
        .await
        .expect("search cards");

    assert!(page.tracks.is_empty());
    assert_eq!(page.cards.len(), 3);
    assert_eq!(page.cards[0].kind, BrowseCardKind::Artist);
    assert_eq!(
        page.cards[0].browse_id.as_deref(),
        Some("UCxuSb9oqYBpO4nDSRHPknzw")
    );
    assert_eq!(page.cards[1].kind, BrowseCardKind::Album);
    assert_eq!(page.cards[1].browse_id.as_deref(), Some("MPREb_EFES"));
    assert_eq!(page.cards[1].subtitle, "Albüm • Neşet Ertaş • 2025");
    assert_eq!(page.cards[2].kind, BrowseCardKind::Playlist);
    assert_eq!(page.cards[2].browse_id.as_deref(), Some("VLPL_COMMUNITY_1"));
    assert_eq!(page.cards[2].title, "Bozkırın Efsaneleri");
}
