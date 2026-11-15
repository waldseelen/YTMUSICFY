use serde_json::{Value, json};
use ytm_core::{
    AuthCredentials, ClientConfig, CoreError, InnerTubeClient, InnerTubeClientTrait, MockTransport,
    MoveAction, MusicVideoType, PlaylistPrivacy, TrackAvailability,
};

fn test_creds(page_id: Option<&str>) -> AuthCredentials {
    AuthCredentials {
        cookie: "__Secure-3PAPISID=test_sapisid_secret; SID=sid123".to_string(),
        user_agent: "TestAgent/1.0".to_string(),
        x_goog_authuser: Some("1".to_string()),
        x_goog_page_id: page_id.map(str::to_string),
        x_goog_visitor_id: Some("VISITOR_123".to_string()),
    }
}

fn fast_config() -> ClientConfig {
    ClientConfig {
        mutation_delay_ms: 0,
        ..ClientConfig::default()
    }
}

fn fixture(name: &str) -> Value {
    let raw = match name {
        "account_menu_authenticated" => {
            include_str!("../../../tests/fixtures/account_menu_authenticated.json")
        }
        "account_menu_logged_out" => {
            include_str!("../../../tests/fixtures/account_menu_logged_out.json")
        }
        "library_playlists" => include_str!("../../../tests/fixtures/library_playlists.json"),
        "playlist_browse_page1" => {
            include_str!("../../../tests/fixtures/playlist_browse_page1.json")
        }
        "playlist_browse_continuation" => {
            include_str!("../../../tests/fixtures/playlist_browse_continuation.json")
        }
        "edit_playlist_response" => {
            include_str!("../../../tests/fixtures/edit_playlist_response.json")
        }
        "search_results_atv" => include_str!("../../../tests/fixtures/search_results_atv.json"),
        other => panic!("unknown fixture: {other}"),
    };
    serde_json::from_str(raw).expect("valid JSON fixture")
}

#[tokio::test]
async fn verify_session_passes_authenticated_and_rejects_logged_out_200_ok() {
    let transport = MockTransport::with_responses(vec![
        Ok(fixture("account_menu_authenticated")),
        Ok(fixture("account_menu_logged_out")),
    ]);
    let client = InnerTubeClient::with_clock(
        transport,
        test_creds(Some("BRAND_99")),
        fast_config(),
        || 1_700_000_000,
    );

    let status = client.verify_session().await.expect("authenticated");
    assert!(status.authenticated);
    assert_eq!(status.account_name.as_deref(), Some("Demo Listener"));
    assert_eq!(status.channel_handle.as_deref(), Some("@demolistener"));
    assert_eq!(status.auth_user, "1");
    assert_eq!(status.brand_page_id.as_deref(), Some("BRAND_99"));

    let recorded = client.transport().recorded_requests();
    assert_eq!(recorded[0].endpoint, "account/account_menu");
    assert_eq!(
        recorded[0].body["context"]["user"]["onBehalfOfUser"],
        "BRAND_99"
    );
    assert!(recorded[0].body["context"]["client"].get("user").is_none());
    assert!(
        recorded[0]
            .headers
            .iter()
            .any(|(k, v)| k == "X-Goog-PageId" && v == "BRAND_99")
    );

    let err = client.verify_session().await.unwrap_err();
    assert!(matches!(err, CoreError::AuthExpired));
}

#[tokio::test]
async fn get_library_playlists_skips_new_playlist_and_maps_vllm() {
    let transport = MockTransport::with_responses(vec![Ok(fixture("library_playlists"))]);
    let client = InnerTubeClient::new(transport, test_creds(None), fast_config());

    let lists = client.get_library_playlists().await.expect("playlists");
    assert_eq!(lists.len(), 2);
    assert_eq!(lists[0].id, "LM");
    assert!(lists[0].is_liked_music);
    assert_eq!(lists[1].id, "PLTestPlaylist01");
    assert_eq!(lists[1].title, "Synth Classics");
    assert_eq!(lists[1].track_count, Some(4));
    assert!(lists[1].owned);
}

#[tokio::test]
async fn get_playlist_paginates_continuations_and_preserves_grey_out_set_video_id() {
    let transport = MockTransport::with_responses(vec![
        Ok(fixture("playlist_browse_page1")),
        Ok(fixture("playlist_browse_continuation")),
    ]);
    let client = InnerTubeClient::new(transport, test_creds(None), fast_config());

    let playlist = client
        .get_playlist("PLTestPlaylist01")
        .await
        .expect("get_playlist");
    assert_eq!(playlist.id, "PLTestPlaylist01");
    assert_eq!(playlist.title, "Synth Classics");
    assert_eq!(playlist.privacy, PlaylistPrivacy::Private);
    assert!(playlist.owned);
    assert_eq!(playlist.tracks.len(), 3);

    assert_eq!(playlist.tracks[0].video_id, "vid_atv_001");
    assert_eq!(
        playlist.tracks[0].set_video_id.as_deref(),
        Some("SET_ID_001")
    );
    assert_eq!(
        playlist.tracks[0].music_video_type,
        MusicVideoType::AudioTrack
    );
    assert_eq!(playlist.tracks[0].duration_sec, Some(413));
    assert_eq!(
        playlist.tracks[0].availability,
        TrackAvailability::Available
    );

    assert_eq!(playlist.tracks[1].video_id, "vid_dead_002");
    assert_eq!(
        playlist.tracks[1].set_video_id.as_deref(),
        Some("SET_ID_002")
    );
    assert_eq!(playlist.tracks[1].availability, TrackAvailability::Deleted);

    assert_eq!(playlist.tracks[2].video_id, "vid_ugc_003");
    assert_eq!(
        playlist.tracks[2].set_video_id.as_deref(),
        Some("SET_ID_003")
    );
    assert_eq!(
        playlist.tracks[2].music_video_type,
        MusicVideoType::UserGenerated
    );

    let reqs = client.transport().recorded_requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].body["browseId"], "VLPLTestPlaylist01");
    assert_eq!(reqs[1].body["continuation"], "CONT_TOKEN_PAGE_2");
}

#[tokio::test]
async fn mutations_chunk_at_50_and_validate_set_video_ids() {
    let transport = MockTransport::with_responses(vec![
        Ok(fixture("edit_playlist_response")),
        Ok(fixture("edit_playlist_response")),
        Ok(fixture("edit_playlist_response")),
        Ok(fixture("edit_playlist_response")),
    ]);
    let client = InnerTubeClient::new(transport, test_creds(None), fast_config());

    let ids: Vec<String> = (0..55).map(|i| format!("vid_{i}")).collect();
    let res = client
        .add_playlist_items("VLPLTestPlaylist01", &ids, true)
        .await
        .expect("chunked add");
    assert_eq!(res.status, "STATUS_SUCCEEDED");

    let reqs = client.transport().recorded_requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].body["playlistId"], "PLTestPlaylist01");
    assert_eq!(reqs[0].body["actions"].as_array().unwrap().len(), 50);
    assert_eq!(reqs[1].body["actions"].as_array().unwrap().len(), 5);
    assert_eq!(
        reqs[0].body["actions"][0]["dedupeOption"],
        "DEDUPE_OPTION_SKIP"
    );

    let missing_err = client
        .remove_playlist_items("PLTestPlaylist01", &[("vid_x".to_string(), "".to_string())])
        .await
        .unwrap_err();
    assert!(matches!(missing_err, CoreError::MissingSetVideoId { .. }));

    client
        .move_playlist_items(
            "PLTestPlaylist01",
            &[
                MoveAction {
                    set_video_id: "SET_A".to_string(),
                    moved_set_video_successor: Some("SET_B".to_string()),
                },
                MoveAction {
                    set_video_id: "SET_C".to_string(),
                    moved_set_video_successor: None,
                },
            ],
        )
        .await
        .expect("move items");

    let all_reqs = client.transport().recorded_requests();
    let move_actions = all_reqs.last().unwrap().body["actions"].as_array().unwrap();
    assert_eq!(move_actions[0]["movedSetVideoIdSuccessor"], "SET_B");
    assert!(move_actions[1].get("movedSetVideoIdSuccessor").is_none());
}

#[tokio::test]
async fn search_tracks_and_nested_command_executor_continuation_work() {
    let nested_page = json!({
        "contents": {
            "twoColumnBrowseResultsRenderer": {
                "secondaryContents": {
                    "sectionListRenderer": {
                        "contents": [{
                            "musicPlaylistShelfRenderer": {
                                "playlistId": "PLNested",
                                "contents": [{
                                    "continuationItemRenderer": {
                                        "continuationEndpoint": {
                                            "commandExecutorCommand": {
                                                "commands": [{
                                                    "continuationCommand": {
                                                        "request": "CONTINUATION_REQUEST_TYPE_BROWSE",
                                                        "token": "NESTED_TOK"
                                                    }
                                                }]
                                            }
                                        }
                                    }
                                }]
                            }
                        }]
                    }
                }
            }
        }
    });
    let (_, tok) =
        ytm_core::parser::parse_playlist_page(&nested_page, "PLNested").expect("parse nested");
    assert_eq!(tok.as_deref(), Some("NESTED_TOK"));

    let transport = MockTransport::with_responses(vec![Ok(fixture("search_results_atv"))]);
    let client = InnerTubeClient::new(transport, test_creds(None), fast_config());
    let hits = client
        .search_tracks("Pink Floyd Time", 5)
        .await
        .expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].video_id, "vid_atv_001");
    assert_eq!(hits[0].music_video_type, MusicVideoType::AudioTrack);
    assert_eq!(hits[0].duration_sec, Some(413));
}
