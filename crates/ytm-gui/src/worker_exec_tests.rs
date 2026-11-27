use std::sync::{Arc, mpsc};

use eframe::egui;
use ytm_core::{FetchCompleteness, PlaylistPrivacy};
use ytm_store::SqliteStore;

use super::*;
use crate::state::DispatchEntryOutcome;

use super::mock::{MockClient, mk_playlist, mk_track};

#[tokio::test]
async fn partial_sync_failure_preserves_remaining_entries_in_sqlite() {
    let store = Arc::new(SqliteStore::open_in_memory(None).expect("mem store"));
    let client = MockClient::default();
    *client.fail_add_on.lock().unwrap() = Some("FailOn4".to_string());

    let entries = vec![1u64, 2, 3, 4, 5]
        .into_iter()
        .map(|id| StagedEntry {
            id,
            op: StagedOp::CreatePlaylist {
                title: if id == 4 {
                    "FailOn4".to_string()
                } else {
                    format!("List{id}")
                },
                description: String::new(),
                privacy: PlaylistPrivacy::Private,
                tracks: Vec::new(),
            },
            selected: true,
        })
        .collect::<Vec<_>>();
    persist_staged_entries(&store, &entries).expect("seed staged ops");

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
    assert!(res.is_err());

    let finished: Vec<(u64, DispatchEntryOutcome)> = rx
        .try_iter()
        .filter_map(|e| match e {
            WorkerEvent::SyncEntryFinished { entry_id, outcome } => Some((entry_id, outcome)),
            _ => None,
        })
        .collect();
    assert_eq!(finished.len(), 4);
    assert!(matches!(
        finished[0].1,
        DispatchEntryOutcome::Succeeded { .. }
    ));
    assert!(matches!(
        finished[2].1,
        DispatchEntryOutcome::Succeeded { .. }
    ));
    assert!(matches!(finished[3].1, DispatchEntryOutcome::Failed { .. }));

    let remaining = load_persisted_staged_entries(&store).expect("reload staged ops");
    let rem_ids: Vec<u64> = remaining.iter().map(|e| e.id).collect();
    assert_eq!(rem_ids, vec![4, 5]);
}

#[tokio::test]
async fn delete_playlist_aborts_before_post_when_snapshot_fails_and_reorder_aborts_on_drift() {
    let blocker_dir = tempfile::TempDir::new().expect("create temp dir");
    let bad_snap = blocker_dir.path().join("snap_blocker_file");
    std::fs::write(&bad_snap, b"blocker").expect("write blocker");
    let store = SqliteStore::open_in_memory(Some(bad_snap)).expect("open store");
    let client = MockClient::default();
    client.insert(
        mk_playlist("PL1", vec![mk_track("V1", Some("S1"), 0)]),
        FetchCompleteness::Complete,
    );

    let (tx, _rx) = mpsc::channel();
    let ctx = egui::Context::default();
    let del_res = run_confirmed_mutation(
        &client,
        &store,
        ConfirmedMutation::DeletePlaylist {
            playlist_id: "PL1".to_string(),
            playlist_title: "PL1".to_string(),
        },
        &tx,
        &ctx,
    )
    .await;
    assert!(del_res.is_err());
    assert!(
        !client
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.starts_with("delete:"))
    );
    drop(blocker_dir);

    let mem = SqliteStore::open_in_memory(None).expect("mem store");
    client.insert(
        mk_playlist(
            "PL1",
            vec![
                mk_track("V1", Some("S1"), 0),
                mk_track("V2", Some("S_CHANGED"), 1),
            ],
        ),
        FetchCompleteness::Complete,
    );
    let drift_entry = StagedEntry {
        id: 1,
        op: StagedOp::Reorder {
            playlist_id: "PL1".to_string(),
            playlist_title: "PL1".to_string(),
            target_set_video_ids: vec!["S2".to_string(), "S1".to_string()],
            moves: Vec::new(),
        },
        selected: true,
    };
    let (tx2, rx2) = mpsc::channel();
    let drift_res = run_confirmed_mutation(
        &client,
        &mem,
        ConfirmedMutation::ExecuteSync {
            entries: vec![drift_entry],
        },
        &tx2,
        &ctx,
    )
    .await;
    assert!(drift_res.is_err());
    assert!(
        !client
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.starts_with("move:"))
    );
    assert!(rx2.try_iter().any(|e| matches!(
        e,
        WorkerEvent::SyncEntryFinished {
            outcome: DispatchEntryOutcome::AbortedDrift { .. },
            ..
        }
    )));

    client.insert(
        mk_playlist("PL_TRUNC", vec![mk_track("V1", Some("S1"), 0)]),
        FetchCompleteness::Truncated {
            pages_fetched: 500,
            max_pages: 500,
        },
    );
    let trunc_res = run_confirmed_mutation(
        &client,
        &mem,
        ConfirmedMutation::ExecuteSync {
            entries: vec![StagedEntry {
                id: 9,
                op: StagedOp::AddTracks {
                    playlist_id: "PL_TRUNC".to_string(),
                    playlist_title: "Trunc".to_string(),
                    tracks: vec![mk_track("V9", None, 0)],
                    allow_duplicates: false,
                },
                selected: true,
            }],
        },
        &tx2,
        &ctx,
    )
    .await;
    assert!(trunc_res.is_err());
    assert!(
        !client
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.starts_with("add:PL_TRUNC"))
    );
}

#[tokio::test]
async fn move_tracks_aborts_source_removal_if_add_returns_empty_and_upgrade_preserves_pos() {
    let store = SqliteStore::open_in_memory(None).expect("mem store");
    let client = MockClient::default();
    client.insert(
        mk_playlist("SRC", vec![mk_track("V1", Some("S1"), 0)]),
        FetchCompleteness::Complete,
    );
    client.insert(mk_playlist("DST", Vec::new()), FetchCompleteness::Complete);
    *client.empty_add_items.lock().unwrap() = true;

    let (tx, _rx) = mpsc::channel();
    let ctx = egui::Context::default();
    let move_res = run_confirmed_mutation(
        &client,
        &store,
        ConfirmedMutation::ExecuteSync {
            entries: vec![StagedEntry {
                id: 5,
                op: StagedOp::MoveTracks {
                    source_playlist_id: "SRC".to_string(),
                    source_playlist_title: "Src".to_string(),
                    target_playlist_id: "DST".to_string(),
                    target_playlist_title: "Dst".to_string(),
                    tracks: vec![mk_track("V1", Some("S1"), 0)],
                },
                selected: true,
            }],
        },
        &tx,
        &ctx,
    )
    .await;
    assert!(move_res.is_err());
    assert!(
        !client
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.starts_with("remove:"))
    );

    *client.empty_add_items.lock().unwrap() = false;
    client.calls.lock().unwrap().clear();
    client.insert(
        mk_playlist(
            "PL1",
            vec![
                mk_track("UGC1", Some("S_OLD"), 0),
                mk_track("V2", Some("S2"), 1),
            ],
        ),
        FetchCompleteness::Complete,
    );
    let res = run_confirmed_mutation(
        &client,
        &store,
        ConfirmedMutation::ExecuteSync {
            entries: vec![StagedEntry {
                id: 7,
                op: StagedOp::UpgradeTracks {
                    playlist_id: "PL1".to_string(),
                    playlist_title: "PL1".to_string(),
                    upgrades: vec![(
                        mk_track("UGC1", Some("S_OLD"), 0),
                        mk_track("ATV1", None, 0),
                    )],
                    preserve_position: true,
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
    assert_eq!(calls, vec!["add:PL1:1", "move:PL1:1", "remove:PL1:1"]);
}

#[test]
fn collect_add_ids_skips_existing_duplicates_and_unavailable_tracks() {
    use ytm_core::TrackAvailability;
    let live = mk_playlist("PL1", vec![mk_track("V_LIVE", None, 0)]);
    let mut dead = mk_track("V_DEAD", None, 3);
    dead.availability = TrackAvailability::Deleted;
    let input = vec![
        mk_track("V_LIVE", None, 0),
        mk_track("V_OK_1", None, 1),
        mk_track("V_OK_1", None, 2),
        dead,
        mk_track("", None, 4),
        mk_track("V_OK_2", None, 5),
    ];
    assert_eq!(
        worker_sync::ops::collect_add_ids(&input, &live, false),
        vec!["V_OK_1".to_string(), "V_OK_2".to_string()]
    );
    assert_eq!(
        worker_sync::ops::collect_add_ids(&input, &live, true),
        vec![
            "V_LIVE".to_string(),
            "V_OK_1".to_string(),
            "V_OK_1".to_string(),
            "V_OK_2".to_string()
        ]
    );
}
