mod common;

use std::collections::HashMap;

use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ytm_cli::{ActiveTab, Commands, DedupeArgs, HealthArgs, SortArgs, TuiAction, TuiApp};
use ytm_core::{
    FetchCompleteness, FetchManifest, InnerTubeClientTrait, MusicVideoType, Playlist,
    PlaylistPrivacy, TrackAvailability,
};
use ytm_enrich::MockEnrichmentHttp;

use common::{CliHarness, StatefulMockClient, track};

#[tokio::test]
async fn e2e_health_upgrade_and_headless_tui_tabs() {
    let mut del = track(
        "V_DEL",
        "SH_1",
        0,
        "Gone",
        "Deleted Track",
        180,
        MusicVideoType::OfficialMusicVideo,
    );
    del.availability = TrackAvailability::Deleted;
    let ugc = track(
        "V_UGC",
        "SH_2",
        1,
        "Uploader",
        "Muse - Hysteria (Official Video)",
        228,
        MusicVideoType::UserGenerated,
    );
    let pl_health = Playlist {
        id: "PL_HEALTH".to_string(),
        title: "HealthMix".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 2,
        tracks: vec![del, ugc],
    };
    let mut catalog = HashMap::new();
    catalog.insert(
        "hysteria".to_string(),
        vec![track(
            "V_ATV_HYST",
            "S_NEW",
            0,
            "Muse",
            "Hysteria",
            227,
            MusicVideoType::AudioTrack,
        )],
    );

    let client = StatefulMockClient::new(vec![pl_health], catalog);
    let harness = CliHarness::new(MockEnrichmentHttp::with_responses(Vec::new()));

    let health_out = harness
        .run_cmd(
            &client,
            &Commands::Health(HealthArgs {
                target: "PL_HEALTH".to_string(),
                find_official: true,
                prune_unavailable: true,
                upgrade: true,
                execute: true,
                force_incomplete: false,
            }),
            None,
        )
        .await
        .expect("health upgrade + prune execute");

    assert!(health_out.contains("Manifest: declared=2 fetched=2 unavailable=1"));
    assert!(
        health_out.contains("Batch[health-add]: chunks_succeeded=1/1 failed=0 confirmed_added=1")
    );
    assert!(health_out.contains("Batch[health-remove]: chunks_succeeded=1/1"));

    let final_pl = client.get_playlist("PL_HEALTH").await.expect("pl");
    assert_eq!(final_pl.tracks.len(), 1);
    assert_eq!(final_pl.tracks[0].video_id, "V_ATV_HYST");

    // Verify `Commands::Tui` and headless 5-tab Ratatui TestBackend rendering
    let mut captured_tab = ActiveTab::Playlists;
    harness
        .run_cmd_with_tui(&client, &Commands::Tui, None, |mut app: TuiApp| {
            for expected in ActiveTab::ALL {
                assert_eq!(app.active_tab, expected);
                let buf = app.render_to_test_buffer(100, 30);
                let rendered: String = buf.content().iter().map(|c| c.symbol()).collect();
                assert!(rendered.contains("ytm-architect"));
                assert!(rendered.contains(expected.title()));
                app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
            }
            assert_eq!(
                app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
                TuiAction::Quit
            );
            captured_tab = app.active_tab;
            Ok(())
        })
        .await
        .expect("tui command");
    assert_eq!(captured_tab, ActiveTab::Playlists);
}

#[tokio::test]
async fn e2e_manifest_summary_and_incomplete_execute_guard() {
    let pl = Playlist {
        id: "PL_INC".to_string(),
        title: "Incomplete".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 2,
        tracks: vec![
            track(
                "V_B",
                "S_1",
                0,
                "Artist B",
                "Song",
                200,
                MusicVideoType::AudioTrack,
            ),
            track(
                "V_A",
                "S_2",
                1,
                "Artist A",
                "Song",
                200,
                MusicVideoType::AudioTrack,
            ),
        ],
    };
    let client = StatefulMockClient::new(vec![pl], HashMap::new());
    client.manifests.lock().expect("lock manifests").insert(
        "PL_INC".to_string(),
        FetchManifest {
            declared_track_count: Some(10),
            fetched_tracks: 2,
            available_tracks: 2,
            unavailable_tracks: 0,
            missing_set_video_id: 0,
            pages_fetched: 1,
            completeness: FetchCompleteness::Degraded {
                dropped_renderers: vec!["mysteryRenderer".to_string()],
            },
            fetched_at: Utc::now(),
        },
    );
    let harness = CliHarness::new(MockEnrichmentHttp::with_responses(Vec::new()));

    let inspect_out = harness
        .run_cmd(
            &client,
            &Commands::Inspect {
                playlist: "PL_INC".to_string(),
                json: false,
            },
            None,
        )
        .await
        .expect("inspect");
    assert!(inspect_out.contains(
        "Manifest: declared=10 fetched=2 unavailable=0 dropped=1 status=Degraded(mysteryRenderer)"
    ));

    let blocked = harness
        .run_cmd(
            &client,
            &Commands::Dedupe(DedupeArgs {
                playlist: "PL_INC".to_string(),
                threshold: 0.90,
                duration_tol: 15,
                ignore_duration: false,
                exact_only: false,
                keep: ytm_cli::CliKeepStrategy::Quality,
                execute: true,
                force_incomplete: false,
            }),
            None,
        )
        .await;
    assert!(
        blocked.is_err(),
        "Degraded fetch must block --execute without --force-incomplete"
    );
    assert_eq!(client.mutations(), 0);

    let sort_out = harness
        .run_cmd(
            &client,
            &Commands::Sort(SortArgs {
                playlist: "PL_INC".to_string(),
                by: "artist:asc".to_string(),
                seed: None,
                execute: true,
                force_incomplete: true,
            }),
            None,
        )
        .await
        .expect("force-incomplete overrides guard");
    assert!(sort_out.contains("Manifest: declared=10 fetched=2"));
    assert_eq!(client.mutations(), 1);
}

#[tokio::test]
async fn e2e_health_upgrade_skips_removal_when_add_unconfirmed() {
    let ugc = track(
        "V_UGC",
        "SH_2",
        0,
        "Uploader",
        "Muse - Hysteria (Official Video)",
        228,
        MusicVideoType::UserGenerated,
    );
    let pl_health = Playlist {
        id: "PL_H_GUARD".to_string(),
        title: "GuardMix".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 1,
        tracks: vec![ugc],
    };
    let mut catalog = HashMap::new();
    catalog.insert(
        "hysteria".to_string(),
        vec![track(
            "V_ATV_HYST",
            "S_NEW",
            0,
            "Muse",
            "Hysteria",
            227,
            MusicVideoType::AudioTrack,
        )],
    );

    let client = StatefulMockClient::new(vec![pl_health], catalog);
    client
        .skip_add_confirm
        .lock()
        .expect("lock skip_add_confirm")
        .insert("V_ATV_HYST".to_string());
    let harness = CliHarness::new(MockEnrichmentHttp::with_responses(Vec::new()));

    let out = harness
        .run_cmd(
            &client,
            &Commands::Health(HealthArgs {
                target: "PL_H_GUARD".to_string(),
                find_official: true,
                prune_unavailable: false,
                upgrade: true,
                execute: true,
                force_incomplete: false,
            }),
            None,
        )
        .await
        .expect("health execute");

    assert!(out.contains("Batch[health-add]: chunks_succeeded=1/1 failed=0 confirmed_added=0"));
    let pl = client.get_playlist("PL_H_GUARD").await.expect("pl");
    assert_eq!(pl.tracks.len(), 1);
    assert_eq!(
        pl.tracks[0].video_id, "V_UGC",
        "unconfirmed ATV add must not delete UGC source"
    );
}
