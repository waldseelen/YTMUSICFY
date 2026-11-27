use serde_json::json;
use ytm_core::{
    AuthCredentials, BrowsePageKind, ClientConfig, InnerTubeClient, InnerTubeClientTrait,
    MockTransport,
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
async fn artist_page_keeps_top_songs_before_albums_and_parses_see_all_params() {
    let artist_resp = json!({
        "header": {
            "musicImmersiveHeaderRenderer": {
                "title": { "runs": [{ "text": "Neset Ertas" }] },
                "description": { "runs": [{ "text": "Bozkırın Tezenesi" }] }
            }
        },
        "contents": {
            "singleColumnBrowseResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [
                                    {
                                        "musicShelfRenderer": {
                                            "title": { "runs": [{ "text": "En çok dinlenen şarkılar" }] },
                                            "bottomEndpoint": {
                                                "browseEndpoint": {
                                                    "browseId": "VLOLAK5uy_top_songs",
                                                    "params": "ggTopParams"
                                                }
                                            },
                                            "contents": [
                                                {
                                                    "musicResponsiveListItemRenderer": {
                                                        "playlistItemData": { "videoId": "vid_yolcu" },
                                                        "flexColumns": [
                                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Yolcu" }] } } },
                                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Neşet Ertaş • 36 Mn dinlenme" }] } } }
                                                        ]
                                                    }
                                                }
                                            ]
                                        }
                                    },
                                    {
                                        "musicCarouselShelfRenderer": {
                                            "header": {
                                                "musicCarouselShelfBasicHeaderRenderer": {
                                                    "title": { "runs": [{ "text": "Albümler" }] },
                                                    "moreContentButton": {
                                                        "buttonRenderer": {
                                                            "navigationEndpoint": {
                                                                "browseEndpoint": {
                                                                    "browseId": "MPADUCxuSb9oqYBpO4nDSRHPknzw",
                                                                    "params": "ggAlbumsParams"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            },
                                            "contents": [
                                                {
                                                    "musicTwoRowItemRenderer": {
                                                        "title": { "runs": [{ "text": "Efes Plak Yılları" }] },
                                                        "subtitle": { "runs": [{ "text": "2025" }] },
                                                        "navigationEndpoint": {
                                                            "browseEndpoint": {
                                                                "browseId": "MPREb_EFES",
                                                                "browseEndpointContextSupportedConfigs": {
                                                                    "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_ALBUM" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            ]
                                        }
                                    },
                                    {
                                        "musicCarouselShelfRenderer": {
                                            "header": {
                                                "musicCarouselShelfBasicHeaderRenderer": {
                                                    "title": { "runs": [{ "text": "Single'lar ve EP'ler" }] }
                                                }
                                            },
                                            "contents": [
                                                {
                                                    "musicTwoRowItemRenderer": {
                                                        "title": { "runs": [{ "text": "Kalktı Kısmetimiz" }] },
                                                        "subtitle": { "runs": [{ "text": "EP • 2026" }] },
                                                        "navigationEndpoint": { "browseEndpoint": { "browseId": "MPREb_KALKTI" } }
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
    let see_all_resp = json!({
        "contents": {
            "singleColumnBrowseResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "gridRenderer": {
                                        "items": [{
                                            "musicTwoRowItemRenderer": {
                                                "title": { "runs": [{ "text": "Garip Bülbül" }] },
                                                "subtitle": { "runs": [{ "text": "2022" }] },
                                                "navigationEndpoint": { "browseEndpoint": { "browseId": "MPREb_GARIP" } }
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

    let client = InnerTubeClient::new(
        MockTransport::with_responses(vec![Ok(artist_resp), Ok(see_all_resp)]),
        creds(),
        fast_cfg(),
    );

    let artist = client
        .get_browse_page("UCxuSb9oqYBpO4nDSRHPknzw")
        .await
        .expect("artist page");
    assert_eq!(artist.kind, BrowsePageKind::Artist);
    assert_eq!(artist.sections.len(), 3);
    assert_eq!(artist.sections[0].title, "En çok dinlenen şarkılar");
    assert_eq!(artist.sections[0].tracks.len(), 1);
    assert_eq!(artist.sections[0].tracks[0].title, "Yolcu");
    assert_eq!(
        artist.sections[0].browse_id.as_deref(),
        Some("VLOLAK5uy_top_songs")
    );
    assert_eq!(
        artist.sections[0].browse_params.as_deref(),
        Some("ggTopParams")
    );
    assert_eq!(artist.sections[1].title, "Albümler");
    assert_eq!(
        artist.sections[1].browse_id.as_deref(),
        Some("MPADUCxuSb9oqYBpO4nDSRHPknzw")
    );
    assert_eq!(
        artist.sections[1].browse_params.as_deref(),
        Some("ggAlbumsParams")
    );
    assert_eq!(artist.sections[2].title, "Single'lar ve EP'ler");

    let all_albums = client
        .get_browse_page_with_params("MPADUCxuSb9oqYBpO4nDSRHPknzw", Some("ggAlbumsParams"))
        .await
        .expect("artist albums see all");
    assert_eq!(all_albums.sections[0].cards[0].title, "Garip Bülbül");
    let reqs = client.transport().recorded_requests();
    assert_eq!(reqs[1].body["params"], "ggAlbumsParams");
}

#[tokio::test]
async fn album_page_parses_header_artist_link_and_filters_turkish_play_counts() {
    let album_resp = json!({
        "contents": {
            "twoColumnBrowseResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicResponsiveHeaderRenderer": {
                                        "title": { "runs": [{ "text": "Efes Plak Yılları" }] },
                                        "subtitle": { "runs": [{ "text": "Albüm • 2025" }] },
                                        "secondSubtitle": { "runs": [{ "text": "7 şarkı • 34 dakika" }] },
                                        "straplineTextOne": {
                                            "runs": [{
                                                "text": "Neşet Ertaş",
                                                "navigationEndpoint": {
                                                    "browseEndpoint": { "browseId": "UCxuSb9oqYBpO4nDSRHPknzw" }
                                                }
                                            }]
                                        }
                                    }
                                }]
                            }
                        }
                    }
                }],
                "secondaryContents": {
                    "sectionListRenderer": {
                        "contents": [{
                            "musicShelfRenderer": {
                                "contents": [{
                                    "musicResponsiveListItemRenderer": {
                                        "playlistItemData": { "videoId": "vid_seher" },
                                        "flexColumns": [
                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Seher Vakti" }] } } },
                                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "136 B dinlenme" }] } } }
                                        ],
                                        "fixedColumns": [
                                            { "musicResponsiveListItemFixedColumnRenderer": { "text": { "runs": [{ "text": "6:13" }] } } }
                                        ]
                                    }
                                }]
                            }
                        }]
                    }
                }
            }
        }
    });

    let client = InnerTubeClient::new(
        MockTransport::with_responses(vec![Ok(album_resp)]),
        creds(),
        fast_cfg(),
    );
    let album = client.get_browse_page("MPREb_EFES").await.expect("album");
    assert_eq!(album.kind, BrowsePageKind::Album);
    assert_eq!(album.title, "Efes Plak Yılları");
    assert_eq!(
        album.subtitle.as_deref(),
        Some("Albüm • 2025 • 7 şarkı • 34 dakika")
    );
    assert_eq!(album.artists.len(), 1);
    assert_eq!(album.artists[0].name, "Neşet Ertaş");
    assert_eq!(
        album.artists[0].browse_id.as_deref(),
        Some("UCxuSb9oqYBpO4nDSRHPknzw")
    );
    assert_eq!(album.tracks.len(), 1);
    assert_eq!(album.tracks[0].title, "Seher Vakti");
    assert_eq!(album.tracks[0].artists[0].name, "Neşet Ertaş");
    assert_eq!(
        album.tracks[0].album.as_ref().map(|a| a.name.as_str()),
        Some("Efes Plak Yılları")
    );
    assert_eq!(album.tracks[0].duration_sec, Some(373));
}

#[tokio::test]
async fn community_playlist_normalizes_pl_to_vl_and_parses_playlist_shelf_and_continuation() {
    let playlist_resp = json!({
        "contents": {
            "twoColumnBrowseResultsRenderer": {
                "tabs": [{
                    "tabRenderer": {
                        "content": {
                            "sectionListRenderer": {
                                "contents": [{
                                    "musicResponsiveHeaderRenderer": {
                                        "title": { "runs": [{ "text": "Anadolu Seçkisi" }] },
                                        "subtitle": { "runs": [{ "text": "Oynatma listesi • 2024" }] },
                                        "secondSubtitle": { "runs": [{ "text": "42 şarkı • 2 sa. 15 dk." }] },
                                        "facepile": {
                                            "avatarStackViewModel": {
                                                "text": { "content": "Berkay Demir" },
                                                "rendererContext": {
                                                    "commandContext": {
                                                        "onTap": {
                                                            "innertubeCommand": {
                                                                "browseEndpoint": { "browseId": "UC_CREATOR_1" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }]
                            }
                        }
                    }
                }],
                "secondaryContents": {
                    "sectionListRenderer": {
                        "contents": [{
                            "musicPlaylistShelfRenderer": {
                                "contents": [
                                    {
                                        "musicResponsiveListItemRenderer": {
                                            "playlistItemData": { "videoId": "vid_pl_1" },
                                            "flexColumns": [
                                                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Gönül Dağı" }] } } },
                                                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Neşet Ertaş" }] } } }
                                            ]
                                        }
                                    },
                                    {
                                        "continuationItemRenderer": {
                                            "continuationEndpoint": {
                                                "continuationCommand": { "token": "PL_CONT_1" }
                                            }
                                        }
                                    }
                                ]
                            }
                        }]
                    }
                }
            }
        }
    });
    let playlist_cont = json!({
        "continuationContents": {
            "musicPlaylistShelfContinuation": {
                "contents": [{
                    "musicResponsiveListItemRenderer": {
                        "playlistItemData": { "videoId": "vid_pl_2" },
                        "flexColumns": [
                            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Yalan Dünya" }] } } }
                        ]
                    }
                }]
            }
        }
    });

    let client = InnerTubeClient::new(
        MockTransport::with_responses(vec![Ok(playlist_resp), Ok(playlist_cont)]),
        creds(),
        fast_cfg(),
    );

    let page = client
        .get_browse_page("PL_OTHER_USER_LIST")
        .await
        .expect("community playlist");
    assert_eq!(page.kind, BrowsePageKind::Playlist);
    assert_eq!(page.browse_id, "VLPL_OTHER_USER_LIST");
    assert_eq!(page.artists[0].name, "Berkay Demir");
    assert_eq!(page.artists[0].browse_id.as_deref(), Some("UC_CREATOR_1"));
    assert_eq!(page.tracks.len(), 1);
    assert_eq!(page.tracks[0].title, "Gönül Dağı");
    assert_eq!(page.continuation.as_deref(), Some("PL_CONT_1"));

    let cont = client
        .get_browse_continuation("PL_CONT_1")
        .await
        .expect("playlist continuation");
    assert_eq!(cont.tracks.len(), 1);
    assert_eq!(cont.tracks[0].title, "Yalan Dünya");

    let reqs = client.transport().recorded_requests();
    assert_eq!(reqs[0].body["browseId"], "VLPL_OTHER_USER_LIST");
}
