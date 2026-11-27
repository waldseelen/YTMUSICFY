use serde_json::Value;
use ytm_core::{
    AuthCredentials, BROWSE_HOME, BrowseCardKind, BrowsePageKind, ClientConfig, InnerTubeClient,
    InnerTubeClientTrait, MockTransport,
};

fn test_creds() -> AuthCredentials {
    AuthCredentials {
        cookie: "__Secure-3PAPISID=test_sapisid_secret; SID=sid123".to_string(),
        user_agent: "TestAgent/1.0".to_string(),
        x_goog_authuser: Some("0".to_string()),
        x_goog_page_id: None,
        x_goog_visitor_id: None,
    }
}

#[tokio::test]
async fn home_browse_fixture_parses_listen_again_mixes_new_releases_and_quick_picks() {
    let raw: Value = serde_json::from_str(include_str!("../../../tests/fixtures/home_browse.json"))
        .expect("valid home_browse fixture");
    let client = InnerTubeClient::new(
        MockTransport::with_responses(vec![Ok(raw)]),
        test_creds(),
        ClientConfig {
            mutation_delay_ms: 0,
            ..ClientConfig::default()
        },
    );

    let page = client
        .get_browse_page(BROWSE_HOME)
        .await
        .expect("parse home browse page");

    assert_eq!(page.kind, BrowsePageKind::Home);
    assert_eq!(page.browse_id, BROWSE_HOME);
    assert_eq!(page.continuation.as_deref(), Some("HOME_CONT_1"));
    assert_eq!(page.sections.len(), 4);

    assert_eq!(page.sections[0].title, "Listen again");
    assert_eq!(page.sections[0].cards.len(), 2);
    assert_eq!(page.sections[0].cards[0].kind, BrowseCardKind::Track);
    assert_eq!(
        page.sections[0].cards[0].video_id.as_deref(),
        Some("vid_listen_1")
    );
    assert_eq!(page.sections[0].cards[1].kind, BrowseCardKind::Album);
    assert_eq!(
        page.sections[0].cards[1].browse_id.as_deref(),
        Some("MPREb_COASTAL")
    );

    assert_eq!(page.sections[1].title, "Mixed for you");
    assert_eq!(page.sections[1].cards.len(), 1);
    assert_eq!(page.sections[1].cards[0].kind, BrowseCardKind::Playlist);
    assert_eq!(
        page.sections[1].cards[0].browse_id.as_deref(),
        Some("VLRDTMAK5uy_supermix")
    );

    assert_eq!(page.sections[2].title, "New releases");
    assert_eq!(page.sections[2].cards.len(), 1);
    assert_eq!(page.sections[2].cards[0].kind, BrowseCardKind::Album);
    assert_eq!(
        page.sections[2].cards[0].browse_id.as_deref(),
        Some("MPREb_ANALOG")
    );

    assert_eq!(page.sections[3].title, "Quick picks");
    assert_eq!(page.sections[3].tracks.len(), 1);
    assert_eq!(page.sections[3].tracks[0].video_id, "vid_qp_1");
    assert_eq!(page.sections[3].tracks[0].title, "Glass Currents");
}
