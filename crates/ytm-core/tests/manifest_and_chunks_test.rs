use serde_json::{Value, json};
use ytm_core::{
    AuthCredentials, ChunkOutcome, ClientConfig, CoreError, FetchCompleteness, InnerTubeClient,
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

fn load_fixture(raw: &str) -> Value {
    serde_json::from_str(raw).expect("valid JSON fixture")
}

#[tokio::test]
async fn get_library_playlists_paginates_and_guards_duplicate_tokens() {
    let page1 = load_fixture(include_str!(
        "../../../tests/fixtures/library_playlists_page1.json"
    ));
    let page2 = load_fixture(include_str!(
        "../../../tests/fixtures/library_playlists.json"
    ));
    let transport = MockTransport::with_responses(vec![Ok(page1), Ok(page2)]);
    let client = InnerTubeClient::new(transport, test_creds(), fast_config());

    let lists = client
        .get_library_playlists()
        .await
        .expect("paginated library");
    assert_eq!(lists.len(), 3);
    assert_eq!(lists[0].id, "PLPageOne01");
    assert_eq!(lists[1].id, "LM");
    assert_eq!(lists[2].id, "PLTestPlaylist01");

    // Duplicate token loop guard terminates cleanly without looping.
    let dup_page = load_fixture(include_str!(
        "../../../tests/fixtures/library_playlists_page1.json"
    ));
    let dup_transport = MockTransport::with_responses(vec![Ok(dup_page.clone()), Ok(dup_page)]);
    let dup_client = InnerTubeClient::new(dup_transport, test_creds(), fast_config());
    let dup_lists = dup_client
        .get_library_playlists()
        .await
        .expect("duplicate loop exits");
    assert_eq!(dup_lists.len(), 2);
}

#[tokio::test]
async fn manifest_detects_partial_hidden_truncated_and_degraded_states() {
    let page1 = load_fixture(include_str!(
        "../../../tests/fixtures/playlist_trailing_message_page1.json"
    ));
    let empty_intermediate = json!({
        "onResponseReceivedActions": [{
            "reloadContinuationItemsCommand": {
                "continuationItems": [{
                    "continuationItemRenderer": {
                        "continuationEndpoint": {
                            "continuationCommand": { "token": "CONT_FINAL_TRACKS" }
                        }
                    }
                }]
            }
        }]
    });
    let final_page = json!({
        "onResponseReceivedActions": [{
            "appendContinuationItemsAction": {
                "continuationItems": [
                    {
                        "musicResponsiveListItemRenderer": {
                            "playlistItemData": { "videoId": "vid_h2", "playlistSetVideoId": "SET_H2" },
                            "overlay": { "musicItemThumbnailOverlayRenderer": { "content": { "musicPlayButtonRenderer": { "playNavigationEndpoint": { "watchEndpoint": { "videoId": "vid_h2" } } } } } },
                            "flexColumns": [{ "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Track Two" }] } } }]
                        }
                    },
                    {
                        "musicResponsiveListItemRenderer": {
                            "playlistItemData": { "videoId": "vid_h3", "playlistSetVideoId": "SET_H3" },
                            "overlay": { "musicItemThumbnailOverlayRenderer": { "content": { "musicPlayButtonRenderer": { "playNavigationEndpoint": { "watchEndpoint": { "videoId": "vid_h3" } } } } } },
                            "flexColumns": [{ "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Track Three" }] } } }]
                        }
                    }
                ]
            }
        }]
    });

    let client = InnerTubeClient::new(
        MockTransport::with_responses(vec![
            Ok(page1.clone()),
            Ok(empty_intermediate),
            Ok(final_page),
        ]),
        test_creds(),
        fast_config(),
    );
    let res = client
        .get_playlist_with_manifest("PLHiddenTest")
        .await
        .expect("partial hidden");
    assert_eq!(res.playlist.tracks.len(), 3);
    assert_eq!(res.manifest.pages_fetched, 3);
    assert_eq!(
        res.manifest.completeness,
        FetchCompleteness::PartialHidden {
            declared: 4,
            fetched: 3,
            hidden_estimate: 1,
        }
    );
    assert!(!res.manifest.completeness.is_mutation_locked());

    let trunc_client = InnerTubeClient::new(
        MockTransport::with_responses(vec![Ok(page1)]),
        test_creds(),
        ClientConfig {
            max_pagination_pages: 1,
            ..fast_config()
        },
    );
    let trunc = trunc_client
        .get_playlist_with_manifest("PLHiddenTest")
        .await
        .expect("truncated");
    assert!(matches!(
        trunc.manifest.completeness,
        FetchCompleteness::Truncated {
            pages_fetched: 1,
            max_pages: 1,
        }
    ));
    assert!(trunc.manifest.completeness.is_mutation_locked());

    let degraded_page = json!({
        "contents": {
            "twoColumnBrowseResultsRenderer": {
                "secondaryContents": {
                    "sectionListRenderer": {
                        "contents": [{
                            "musicPlaylistShelfRenderer": {
                                "playlistId": "PLDegraded",
                                "contents": [{ "unexpectedRenderer": {} }]
                            }
                        }]
                    }
                }
            }
        }
    });
    let deg_client = InnerTubeClient::new(
        MockTransport::with_responses(vec![Ok(degraded_page)]),
        test_creds(),
        fast_config(),
    );
    let deg = deg_client
        .get_playlist_with_manifest("PLDegraded")
        .await
        .expect("degraded");
    assert_eq!(
        deg.manifest.completeness,
        FetchCompleteness::Degraded {
            dropped_renderers: vec!["unexpectedRenderer".to_string()],
        }
    );
    assert!(deg.manifest.completeness.is_mutation_locked());
}

#[tokio::test]
async fn mutation_batch_report_preserves_chunks_and_chunked_create_recovers_409() {
    let chunk0_ok = load_fixture(include_str!(
        "../../../tests/fixtures/edit_playlist_response.json"
    ));
    let chunk1_fail = json!({
        "status": "STATUS_FAILED",
        "actions": [{
            "addToToastAction": {
                "item": { "notificationTextRenderer": { "successResponseText": { "runs": [{ "text": "Chunk 2 failed" }] } } }
            }
        }]
    });
    let report_client = InnerTubeClient::new(
        MockTransport::with_responses(vec![Ok(chunk0_ok.clone()), Ok(chunk1_fail)]),
        test_creds(),
        ClientConfig {
            mutation_chunk_size: 2,
            ..fast_config()
        },
    );
    let vids = vec!["v1".into(), "v2".into(), "v3".into(), "v4".into()];
    let report = report_client
        .add_playlist_items_report("PLTest", &vids, true)
        .await
        .expect("report returned on partial failure");
    assert_eq!(report.total_chunks, 2);
    assert_eq!(report.succeeded_chunks(), 1);
    assert_eq!(report.failed_chunks(), 1);
    assert_eq!(report.added_items().len(), 1);
    assert!(
        report
            .first_error()
            .is_some_and(|msg| msg.contains("Chunk 2 failed"))
    );
    assert!(matches!(
        report.outcomes[0],
        ChunkOutcome::Succeeded { chunk_index: 0, .. }
    ));
    assert!(matches!(
        report.outcomes[1],
        ChunkOutcome::Failed { chunk_index: 1, .. }
    ));

    let create_client = InnerTubeClient::new(
        MockTransport::with_responses(vec![
            Ok(json!({ "playlistId": "PLNew99" })),
            Err(CoreError::HttpError(
                "HTTP 409 on browse/edit_playlist: Conflict".into(),
            )),
            Ok(chunk0_ok.clone()),
            Ok(chunk0_ok),
        ]),
        test_creds(),
        ClientConfig {
            mutation_chunk_size: 2,
            max_retries: 2,
            ..fast_config()
        },
    );
    let five_vids = vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()];
    let created_id = create_client
        .create_playlist("Big Mix", "desc", PlaylistPrivacy::Private, &five_vids)
        .await
        .expect("chunked create with 409 recovery");
    assert_eq!(created_id, "PLNew99");
    let reqs = create_client.transport().recorded_requests();
    assert_eq!(reqs.len(), 4);
    assert_eq!(reqs[0].endpoint, "playlist/create");
    assert_eq!(reqs[0].body["videoIds"].as_array().unwrap().len(), 2);
    assert_eq!(reqs[1].endpoint, "browse/edit_playlist");
    assert_eq!(reqs[2].endpoint, "browse/edit_playlist");
    assert_eq!(reqs[3].endpoint, "browse/edit_playlist");
}
