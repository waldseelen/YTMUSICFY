use serde_json::{Value, json};
use ytm_core::{
    ArtistHarvestOptions, ArtistSectionKind, AuthCredentials, ClientConfig, InnerTubeClient,
    MockTransport, harvest_artist,
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

fn album_page(title: &str, video_ids: &[&str], continuation: Option<&str>) -> Value {
    let mut contents: Vec<Value> = video_ids
        .iter()
        .map(|vid| {
            json!({
                "musicResponsiveListItemRenderer": {
                    "playlistItemData": { "videoId": vid },
                    "flexColumns": [
                        { "musicResponsiveListItemFlexColumnRenderer": {
                            "text": { "runs": [{ "text": format!("Track {vid}") }] }
                        } }
                    ]
                }
            })
        })
        .collect();
    if let Some(token) = continuation {
        contents.push(json!({
            "continuationItemRenderer": {
                "continuationEndpoint": { "continuationCommand": { "token": token } }
            }
        }));
    }
    json!({
        "header": {
            "musicResponsiveHeaderRenderer": {
                "title": { "runs": [{ "text": title }] }
            }
        },
        "contents": {
            "twoColumnBrowseResultsRenderer": {
                "secondaryContents": {
                    "sectionListRenderer": {
                        "contents": [{ "musicPlaylistShelfRenderer": { "contents": contents } }]
                    }
                }
            }
        }
    })
}

fn album_continuation(title: &str, video_ids: &[&str]) -> Value {
    json!({
        "continuationContents": {
            "musicPlaylistShelfContinuation": {
                "contents": video_ids
                    .iter()
                    .map(|vid| {
                        json!({
                            "musicResponsiveListItemRenderer": {
                                "playlistItemData": { "videoId": vid },
                                "flexColumns": [
                                    { "musicResponsiveListItemFlexColumnRenderer": {
                                        "text": { "runs": [{ "text": format!("Track {vid}") }] }
                                    } }
                                ]
                            }
                        })
                    })
                    .collect::<Vec<_>>()
            }
        },
        "title": title
    })
}

#[tokio::test]
async fn artist_harvest_walks_shelves_see_all_and_continuations() {
    let artist = serde_json::from_str(include_str!("../../../tests/fixtures/artist_browse.json"))
        .expect("artist fixture");
    let artist_cont = serde_json::from_str(include_str!(
        "../../../tests/fixtures/artist_browse_continuation.json"
    ))
    .expect("artist continuation fixture");
    let albums_see_all = serde_json::from_str(include_str!(
        "../../../tests/fixtures/artist_albums_see_all.json"
    ))
    .expect("albums see-all fixture");
    let albums_see_all_cont = serde_json::from_str(include_str!(
        "../../../tests/fixtures/artist_albums_see_all_continuation.json"
    ))
    .expect("albums see-all continuation fixture");

    let client = InnerTubeClient::new(
        MockTransport::with_responses(vec![
            Ok(artist),
            Ok(artist_cont),
            Ok(albums_see_all),
            Ok(albums_see_all_cont),
            Ok(album_page(
                "Coastal Echoes",
                &["atv_coastal_1"],
                Some("COASTAL_CONT"),
            )),
            Ok(album_continuation("Coastal Echoes", &["atv_coastal_2"])),
            Ok(album_page("Paper Tides", &["atv_paper_1"], None)),
            Ok(album_page("Signal Fires", &["atv_signal_1"], None)),
            Ok(album_page("Harbor Lights", &["atv_harbor_1"], None)),
            Ok(album_page("Night Drive Vol. 2", &["atv_night_1"], None)),
            Ok(album_page("Late Night Compilation", &["atv_late_1"], None)),
        ]),
        creds(),
        fast_cfg(),
    );

    let harvest = harvest_artist(&client, "UC_NORA", &ArtistHarvestOptions::default())
        .await
        .expect("harvest");

    assert_eq!(harvest.artist_id, "UC_NORA");
    assert_eq!(harvest.artist_name, "Nora Solis");
    assert_eq!(harvest.pages_fetched, 11);

    let releases: Vec<(&str, ArtistSectionKind)> = harvest
        .releases
        .iter()
        .map(|r| (r.browse_id.as_str(), r.kind))
        .collect();
    assert_eq!(
        releases,
        vec![
            ("MPREb_COASTAL", ArtistSectionKind::Albums),
            ("MPREb_PAPER_TIDES", ArtistSectionKind::Albums),
            ("MPREb_SIGNAL_FIRES", ArtistSectionKind::Albums),
            ("MPREb_HARBOR_EP", ArtistSectionKind::Singles),
            ("MPREb_NIGHT_2", ArtistSectionKind::AppearsOn),
            ("MPREb_LATE_NIGHT", ArtistSectionKind::AppearsOn),
        ]
    );

    let vids: Vec<&str> = harvest
        .tracks
        .iter()
        .map(|t| t.track.video_id.as_str())
        .collect();
    assert_eq!(
        vids,
        vec![
            "vid_top_1",
            "atv_coastal_1",
            "atv_coastal_2",
            "atv_paper_1",
            "atv_signal_1",
            "atv_harbor_1",
            "atv_night_1",
            "atv_late_1",
        ]
    );

    let reqs = client.transport().recorded_requests();
    assert_eq!(reqs[2].body["browseId"], "MPAD_NORA_ALBUMS");
    assert_eq!(reqs[2].body["params"], "ggAlbumsParams");
    assert_eq!(reqs[3].body["continuation"], "ALBUMS_CONT");
    assert_eq!(reqs[4].body["browseId"], "MPREb_COASTAL");
}

#[tokio::test]
async fn artist_harvest_respects_disabled_shelves() {
    let artist = serde_json::from_str(include_str!("../../../tests/fixtures/artist_browse.json"))
        .expect("artist fixture");
    let artist_cont = serde_json::from_str(include_str!(
        "../../../tests/fixtures/artist_browse_continuation.json"
    ))
    .expect("artist continuation fixture");
    let albums_see_all = serde_json::from_str(include_str!(
        "../../../tests/fixtures/artist_albums_see_all.json"
    ))
    .expect("albums see-all fixture");
    let albums_see_all_cont = serde_json::from_str(include_str!(
        "../../../tests/fixtures/artist_albums_see_all_continuation.json"
    ))
    .expect("albums see-all continuation fixture");
    let client = InnerTubeClient::new(
        MockTransport::with_responses(vec![
            Ok(artist),
            Ok(artist_cont),
            Ok(albums_see_all),
            Ok(albums_see_all_cont),
            Ok(album_page("Coastal Echoes", &["atv_coastal_1"], None)),
            Ok(album_page("Paper Tides", &["atv_paper_1"], None)),
            Ok(album_page("Signal Fires", &["atv_signal_1"], None)),
        ]),
        creds(),
        fast_cfg(),
    );

    let opts = ArtistHarvestOptions {
        include_top_songs: false,
        include_singles: false,
        include_appears_on: false,
        ..ArtistHarvestOptions::default()
    };
    let harvest = harvest_artist(&client, "UC_NORA", &opts)
        .await
        .expect("harvest");
    assert_eq!(harvest.releases.len(), 3);
    assert_eq!(harvest.releases[0].browse_id, "MPREb_COASTAL");
    let vids: Vec<&str> = harvest
        .tracks
        .iter()
        .map(|t| t.track.video_id.as_str())
        .collect();
    assert_eq!(vids, vec!["atv_coastal_1", "atv_paper_1", "atv_signal_1"]);
}
