mod common;

use std::collections::HashMap;

use serde_json::json;
use ytm_cli::{
    AuthCommands, CliExportFormat, CliKeepStrategy, CliSetOp, Commands, DedupeArgs, SetArgs,
    SnapshotCommands, SortArgs, SplitGenreArgs,
};
use ytm_core::{InnerTubeClientTrait, MusicVideoType, Playlist, PlaylistPrivacy};
use ytm_enrich::MockEnrichmentHttp;
use ytm_store::SnapshotStore;

use common::{CliHarness, StatefulMockClient, track};

#[tokio::test]
async fn e2e_cli_dry_run_mutate_snapshot_restore_and_idempotent_genre_split() {
    let mb_rock = json!({ "recordings": [{ "first-release-date": "2010", "tags": [{ "name": "rock", "count": 5 }] }] });
    let mb_elec = json!({ "recordings": [{ "first-release-date": "2013", "tags": [{ "name": "house", "count": 5 }] }] });
    let http = MockEnrichmentHttp::with_responses(vec![
        Ok((200, mb_rock.clone())),
        Ok((200, mb_elec)),
        Ok((200, mb_rock)),
    ]);
    let harness = CliHarness::new(http);

    let pl_main = Playlist {
        id: "PL_MAIN".to_string(),
        title: "Architect Mix".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 4,
        tracks: vec![
            track(
                "V_ZEB",
                "S_1",
                0,
                "Zebrahead",
                "Anthem",
                210,
                MusicVideoType::AudioTrack,
            ),
            track(
                "V_UGC",
                "S_2",
                1,
                "FanChan",
                "Daft Punk - Get Lucky (Lyrics)",
                248,
                MusicVideoType::UserGenerated,
            ),
            track(
                "V_ATV",
                "S_3",
                2,
                "Daft Punk",
                "Get Lucky",
                249,
                MusicVideoType::AudioTrack,
            ),
            track(
                "V_ABBA",
                "S_4",
                3,
                "ABBA",
                "Waterloo",
                168,
                MusicVideoType::AudioTrack,
            ),
        ],
    };
    let pl_other = Playlist {
        id: "PL_OTHER".to_string(),
        title: "Other Mix".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 1,
        tracks: vec![track(
            "V_UNIQUE",
            "SO_1",
            0,
            "Muse",
            "Starlight",
            240,
            MusicVideoType::AudioTrack,
        )],
    };
    let client = StatefulMockClient::new(vec![pl_main, pl_other], HashMap::new());

    // 1. auth import writes auth.json securely
    let auth_raw =
        json!({ "cookie": "SAPISID=abc; __Secure-3PAPISID=xyz", "user-agent": "TestUA" })
            .to_string();
    harness
        .run_cmd(
            &client,
            &Commands::Auth {
                action: AuthCommands::Import { file: None },
            },
            Some(auth_raw),
        )
        .await
        .expect("auth import");
    assert!(harness.paths.auth_file().exists());
    assert_eq!(client.mutations(), 0);

    // 2. dedupe dry-run emits ZERO mutations and ZERO snapshots
    let dry_out = harness
        .run_cmd(
            &client,
            &Commands::Dedupe(DedupeArgs {
                playlist: "PL_MAIN".to_string(),
                threshold: 0.90,
                duration_tol: 15,
                ignore_duration: false,
                exact_only: false,
                keep: CliKeepStrategy::Quality,
                execute: false,
                force_incomplete: false,
            }),
            None,
        )
        .await
        .expect("dedupe dry-run");
    assert_eq!(client.mutations(), 0);
    assert!(
        harness
            .store
            .list_snapshots(None)
            .await
            .expect("snaps")
            .is_empty()
    );
    assert!(dry_out.contains("[DRY-RUN]"));
    assert!(dry_out.contains("Manifest: declared=4 fetched=4"));

    // 3. dedupe --execute creates pre-dedupe snapshot & keeps ATV
    harness
        .run_cmd(
            &client,
            &Commands::Dedupe(DedupeArgs {
                playlist: "PL_MAIN".to_string(),
                threshold: 0.90,
                duration_tol: 15,
                ignore_duration: false,
                exact_only: false,
                keep: CliKeepStrategy::Quality,
                execute: true,
                force_incomplete: false,
            }),
            None,
        )
        .await
        .expect("dedupe execute");
    let snaps = harness
        .store
        .list_snapshots(Some("PL_MAIN"))
        .await
        .expect("snaps");
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].reason, "pre-dedupe");
    let pre_dedupe_id = snaps[0].id.clone();
    let after_dedupe = client.get_playlist("PL_MAIN").await.expect("pl");
    assert_eq!(after_dedupe.tracks.len(), 3);
    assert!(after_dedupe.tracks.iter().all(|t| t.video_id != "V_UGC"));

    // 4. sort --by artist:asc --execute sorts ABBA -> Daft Punk -> Zebrahead via LIS moves
    harness
        .run_cmd(
            &client,
            &Commands::Sort(SortArgs {
                playlist: "PL_MAIN".to_string(),
                by: "artist:asc".to_string(),
                seed: None,
                execute: true,
                force_incomplete: false,
            }),
            None,
        )
        .await
        .expect("sort execute");
    let after_sort = client.get_playlist("PL_MAIN").await.expect("sorted pl");
    let ids_sorted: Vec<&str> = after_sort
        .tracks
        .iter()
        .map(|t| t.video_id.as_str())
        .collect();
    assert_eq!(ids_sorted, vec!["V_ABBA", "V_ATV", "V_ZEB"]);

    // 5. snapshot restore --execute restores exact original 4 tracks & original order
    harness
        .run_cmd(
            &client,
            &Commands::Snapshots {
                action: SnapshotCommands::Restore {
                    snapshot_id: pre_dedupe_id,
                    execute: true,
                },
            },
            None,
        )
        .await
        .expect("restore execute");
    let restored = client.get_playlist("PL_MAIN").await.expect("restored pl");
    let ids_restored: Vec<&str> = restored
        .tracks
        .iter()
        .map(|t| t.video_id.as_str())
        .collect();
    assert_eq!(ids_restored, vec!["V_ZEB", "V_UGC", "V_ATV", "V_ABBA"]);

    // 6. set diff --into PL_MAIN --execute inserts only V_UNIQUE
    harness
        .run_cmd(
            &client,
            &Commands::Set(SetArgs {
                op: CliSetOp::Diff,
                playlist_a: "PL_OTHER".to_string(),
                playlist_b: "PL_MAIN".to_string(),
                fuzzy: true,
                into: Some("PL_MAIN".to_string()),
                execute: true,
                force_incomplete: false,
            }),
            None,
        )
        .await
        .expect("set diff execute");
    assert_eq!(
        client
            .get_playlist("PL_MAIN")
            .await
            .expect("pl")
            .tracks
            .len(),
        5
    );

    // 7. backup --format csv --out creates exported CSV file
    let export_dir = harness._dir.path().join("exports");
    harness
        .run_cmd(
            &client,
            &Commands::Backup {
                target: "PL_MAIN".to_string(),
                format: CliExportFormat::Csv,
                out: Some(export_dir.clone()),
            },
            None,
        )
        .await
        .expect("backup csv");
    assert!(export_dir.join("PL_MAIN.csv").exists());

    // 8. split-genre --min-tracks 2 --execute twice (second run is idempotent!)
    let pl_genre = Playlist {
        id: "PL_GENRE".to_string(),
        title: "GenreMix".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 3,
        tracks: vec![
            track(
                "V_ZEB",
                "SG_1",
                0,
                "Zebrahead",
                "Anthem",
                210,
                MusicVideoType::AudioTrack,
            ),
            track(
                "V_ATV",
                "SG_2",
                1,
                "Daft Punk",
                "Get Lucky",
                249,
                MusicVideoType::AudioTrack,
            ),
            track(
                "V_ABBA",
                "SG_3",
                2,
                "ABBA",
                "Waterloo",
                168,
                MusicVideoType::AudioTrack,
            ),
        ],
    };
    let genre_client = StatefulMockClient::new(vec![pl_genre], HashMap::new());
    for pass in 0..2 {
        harness
            .run_cmd(
                &genre_client,
                &Commands::SplitGenre(SplitGenreArgs {
                    playlist: "PL_GENRE".to_string(),
                    config: None,
                    min_tracks: 2,
                    execute: true,
                    force_incomplete: false,
                }),
                None,
            )
            .await
            .expect("split genre");
        assert_eq!(
            genre_client.mutations(),
            1,
            "pass {pass} should stabilize at 1 mutation"
        );
    }
}
