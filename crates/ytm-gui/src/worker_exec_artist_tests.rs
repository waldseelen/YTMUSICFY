use std::sync::mpsc;

use eframe::egui;
use ytm_core::FetchCompleteness;
use ytm_store::SqliteStore;

use super::mock::{MockClient, mk_playlist, mk_track};
use super::*;
use crate::state::DispatchEntryOutcome;

fn harvest_track(vid: &str, title: &str, kind: ytm_core::MusicVideoType) -> ytm_core::Track {
    ytm_core::Track {
        video_id: vid.to_string(),
        set_video_id: None,
        position: 0,
        title: title.to_string(),
        artists: vec![ytm_core::ArtistRef {
            name: "Nora Solis".to_string(),
            browse_id: None,
        }],
        album: None,
        duration_sec: Some(200),
        release_year: Some(2024),
        music_video_type: kind,
        availability: ytm_core::TrackAvailability::Available,
        thumbnail_url: None,
    }
}

fn harvest_entry(
    track: ytm_core::Track,
    source: ytm_core::ArtistSectionKind,
) -> ytm_core::ArtistHarvestTrack {
    ytm_core::ArtistHarvestTrack {
        track,
        source,
        release_title: "Coastal Echoes".to_string(),
    }
}

#[tokio::test]
async fn artist_harvest_sync_snapshots_before_post_and_reads_back() {
    use crate::views::artist_harvest::{ArtistHarvestTarget, stage_artist_harvest};
    use ytm_core::ArtistHarvest;
    use ytm_ops::ArtistHarvestConfig;

    let store = SqliteStore::open_in_memory(None).expect("mem store");
    let client = MockClient::default();
    client.insert(
        mk_playlist("PL_ART", vec![mk_track("EXISTING", Some("S1"), 0)]),
        FetchCompleteness::Complete,
    );
    *client.log_fetches.lock().unwrap() = true;

    let harvest = ArtistHarvest {
        artist_id: "UC_NORA".to_string(),
        artist_name: "Nora Solis".to_string(),
        releases: Vec::new(),
        pages_fetched: 3,
        tracks: vec![
            harvest_entry(
                harvest_track(
                    "UGC1",
                    "Midnight Harbor",
                    ytm_core::MusicVideoType::UserGenerated,
                ),
                ytm_core::ArtistSectionKind::Songs,
            ),
            harvest_entry(
                harvest_track(
                    "ATV1",
                    "Midnight Harbor",
                    ytm_core::MusicVideoType::AudioTrack,
                ),
                ytm_core::ArtistSectionKind::Albums,
            ),
            harvest_entry(
                harvest_track(
                    "EXISTING",
                    "Paper Tides",
                    ytm_core::MusicVideoType::AudioTrack,
                ),
                ytm_core::ArtistSectionKind::Albums,
            ),
        ],
    };

    let mut queue = crate::sync_queue::SyncQueue::default();
    let staged = stage_artist_harvest(
        &mut queue,
        &harvest,
        &[mk_track("EXISTING", Some("S1"), 0)],
        &ArtistHarvestConfig::default(),
        &ArtistHarvestTarget::ExistingPlaylist {
            playlist_id: "PL_ART".to_string(),
            playlist_title: "Playlist PL_ART".to_string(),
        },
    )
    .expect("stage harvest");
    assert_eq!(staged, 1);
    let entries = queue.selected_entries();
    match &entries[0].op {
        StagedOp::AddTracks {
            tracks,
            allow_duplicates,
            ..
        } => {
            assert!(!allow_duplicates);
            assert_eq!(tracks.len(), 1);
            assert_eq!(tracks[0].video_id, "ATV1");
        }
        other => panic!("expected AddTracks, got {other:?}"),
    }

    let (tx, rx) = mpsc::channel();
    let ctx = egui::Context::default();
    let res = run_confirmed_mutation(
        &client,
        &store,
        ConfirmedMutation::ExecuteSync { entries },
        &tx,
        &ctx,
    )
    .await;
    assert!(res.is_ok());
    assert_eq!(
        client.calls.lock().unwrap().clone(),
        vec!["fetch:PL_ART", "add:PL_ART:1", "fetch:PL_ART"]
    );
    let snapshots = store.list_snapshots(None).await.expect("snapshots");
    assert_eq!(snapshots.len(), 1);
    assert!(rx.try_iter().any(|e| matches!(
        e,
        WorkerEvent::SyncEntryFinished {
            outcome: DispatchEntryOutcome::Succeeded {
                readback_verified: true,
                ..
            },
            ..
        }
    )));
}

#[tokio::test]
async fn artist_harvest_aborts_before_post_when_snapshot_fails() {
    use crate::views::artist_harvest::{ArtistHarvestTarget, stage_artist_harvest};
    use ytm_core::ArtistHarvest;
    use ytm_ops::ArtistHarvestConfig;

    let blocker_dir = tempfile::TempDir::new().expect("create temp dir");
    let bad_snap = blocker_dir.path().join("snap_blocker_file");
    std::fs::write(&bad_snap, b"blocker").expect("write blocker");
    let store = SqliteStore::open_in_memory(Some(bad_snap)).expect("open store");
    let client = MockClient::default();
    client.insert(
        mk_playlist("PL_ART", vec![mk_track("EXISTING", Some("S1"), 0)]),
        FetchCompleteness::Complete,
    );

    let harvest = ArtistHarvest {
        artist_id: "UC_NORA".to_string(),
        artist_name: "Nora Solis".to_string(),
        releases: Vec::new(),
        pages_fetched: 1,
        tracks: vec![harvest_entry(
            harvest_track(
                "ATV1",
                "Midnight Harbor",
                ytm_core::MusicVideoType::AudioTrack,
            ),
            ytm_core::ArtistSectionKind::Albums,
        )],
    };
    let mut queue = crate::sync_queue::SyncQueue::default();
    stage_artist_harvest(
        &mut queue,
        &harvest,
        &[],
        &ArtistHarvestConfig::default(),
        &ArtistHarvestTarget::ExistingPlaylist {
            playlist_id: "PL_ART".to_string(),
            playlist_title: "Playlist PL_ART".to_string(),
        },
    )
    .expect("stage harvest");

    let (tx, _rx) = mpsc::channel();
    let ctx = egui::Context::default();
    let res = run_confirmed_mutation(
        &client,
        &store,
        ConfirmedMutation::ExecuteSync {
            entries: queue.selected_entries(),
        },
        &tx,
        &ctx,
    )
    .await;
    assert!(res.is_err());
    assert!(
        !client
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.starts_with("add:"))
    );
}

#[tokio::test]
async fn quick_add_stages_addtracks_with_allow_duplicates_false() {
    let store = SqliteStore::open_in_memory(None).expect("mem store");
    let client = MockClient::default();
    client.insert(
        mk_playlist("PL_QUICK", vec![mk_track("EXISTING", Some("S1"), 0)]),
        FetchCompleteness::Complete,
    );
    *client.log_fetches.lock().unwrap() = true;

    let (tx, rx) = mpsc::channel();
    let ctx = egui::Context::default();
    let res = run_confirmed_mutation(
        &client,
        &store,
        ConfirmedMutation::ExecuteSync {
            entries: vec![StagedEntry {
                id: 1,
                op: StagedOp::AddTracks {
                    playlist_id: "PL_QUICK".to_string(),
                    playlist_title: "Quick Add Target".to_string(),
                    tracks: vec![mk_track("NEW_TRACK", None, 0)],
                    allow_duplicates: false,
                },
                selected: true,
            }],
        },
        &tx,
        &ctx,
    )
    .await;
    assert!(res.is_ok());
    let calls = client.calls.lock().unwrap().clone();
    assert!(calls.iter().any(|c| c.starts_with("add:PL_QUICK")));
    let snapshots = store.list_snapshots(None).await.expect("snapshots");
    assert_eq!(snapshots.len(), 1);
    assert!(rx.try_iter().any(|e| matches!(
        e,
        WorkerEvent::SyncEntryFinished {
            outcome: DispatchEntryOutcome::Succeeded {
                readback_verified: true,
                ..
            },
            ..
        }
    )));
}
