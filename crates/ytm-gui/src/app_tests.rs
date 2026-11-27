use std::sync::mpsc;

use super::*;
use crate::state::DispatchEntryOutcome;
use crate::sync_queue::StagedOp;

#[test]
fn worker_events_transition_app_states_cleanly() {
    let (cmd_tx, _cmd_rx) = mpsc::channel();
    let (evt_tx, evt_rx) = mpsc::channel();
    let mut app = YtmGuiApp::init_with_channels(cmd_tx, evt_rx, "./backups".to_string(), None);

    assert!(app.session.is_loading());
    evt_tx
        .send(WorkerEvent::SessionLoaded(Ok(SessionStatus {
            authenticated: true,
            account_name: Some("Tester".to_string()),
            channel_handle: Some("@test".to_string()),
            auth_user: "0".to_string(),
            brand_page_id: None,
        })))
        .expect("send event");
    app.drain_events();
    assert_eq!(
        app.session.data().and_then(|s| s.account_name.as_deref()),
        Some("Tester")
    );

    evt_tx
        .send(WorkerEvent::MutationFinished {
            ok: true,
            message: "done".to_string(),
            reload_ids: vec!["PL1".to_string()],
        })
        .expect("send mutation finished");
    app.drain_events();
    assert!(app.loading_playlists.contains("PL1"));
    assert_eq!(
        app.status_bus.history().back().map(|m| m.text.as_str()),
        Some("done")
    );
}

#[test]
fn sync_entry_finished_prunes_selectively_and_like_error_rolls_back() {
    let (cmd_tx, _cmd_rx) = mpsc::channel();
    let (evt_tx, evt_rx) = mpsc::channel();
    let mut app = YtmGuiApp::init_with_channels(cmd_tx, evt_rx, "./backups".to_string(), None);

    let id1 = app.sync_queue.push(StagedOp::DeletePlaylist {
        playlist_id: "P1".to_string(),
        playlist_title: "One".to_string(),
    });
    let id2 = app.sync_queue.push(StagedOp::DeletePlaylist {
        playlist_id: "P2".to_string(),
        playlist_title: "Two".to_string(),
    });

    evt_tx
        .send(WorkerEvent::SyncEntryFinished {
            entry_id: id1,
            outcome: DispatchEntryOutcome::Succeeded {
                entry_id: id1,
                chunks_sent: 1,
                confirmed_items: 1,
                readback_verified: true,
            },
        })
        .unwrap();
    evt_tx
        .send(WorkerEvent::MutationFinished {
            ok: false,
            message: "entry 2 failed".to_string(),
            reload_ids: Vec::new(),
        })
        .unwrap();
    app.drain_events();

    assert_eq!(app.sync_queue.len(), 1);
    assert_eq!(app.sync_queue.entries()[0].id, id2);

    app.play_queue.set_liked("VID_ERR", true);
    evt_tx
        .send(WorkerEvent::LikeStatusResult {
            video_id: "VID_ERR".to_string(),
            liked: true,
            result: Err("network error".to_string()),
        })
        .unwrap();
    app.drain_events();
    assert!(!app.play_queue.is_liked("VID_ERR"));
}

#[test]
fn failed_playlist_reload_invalidates_draft_cache_when_base_cache_is_missing() {
    let (cmd_tx, _cmd_rx) = mpsc::channel();
    let (evt_tx, evt_rx) = mpsc::channel();
    let mut app = YtmGuiApp::init_with_channels(cmd_tx, evt_rx, "./backups".to_string(), None);

    let pl = Playlist {
        id: "PL_REF".to_string(),
        title: "Refresh Test".to_string(),
        description: None,
        privacy: ytm_core::PlaylistPrivacy::Private,
        owned: true,
        track_count: 0,
        tracks: Vec::new(),
    };
    app.playlist_cache.insert("PL_REF".to_string(), pl.clone());
    app.playlist_cache.insert("PL_KEEP".to_string(), pl);
    let rev = app.sync_queue.revision();
    assert!(
        app.draft_cache
            .get_or_compute("PL_REF", &app.sync_queue, &app.playlist_cache)
            .is_some()
    );
    assert!(
        app.draft_cache
            .get_or_compute("PL_KEEP", &app.sync_queue, &app.playlist_cache)
            .is_some()
    );
    assert!(app.draft_cache.get("PL_REF", rev).is_some());

    // Simulate "Refresh" removing PL_REF from playlist_cache and reload failing.
    app.playlist_cache.remove("PL_REF");
    evt_tx
        .send(WorkerEvent::PlaylistLoaded {
            playlist_id: "PL_REF".to_string(),
            result: Err("offline".to_string()),
        })
        .unwrap();
    evt_tx
        .send(WorkerEvent::PlaylistLoaded {
            playlist_id: "PL_KEEP".to_string(),
            result: Err("transient".to_string()),
        })
        .unwrap();
    app.drain_events();

    assert!(app.draft_cache.get("PL_REF", rev).is_none());
    assert!(app.draft_cache.get("PL_KEEP", rev).is_some());
    assert_eq!(
        app.playlist_errors.get("PL_REF").map(String::as_str),
        Some("offline")
    );
}

#[test]
fn offline_cache_hydrates_library_and_survives_network_failure() {
    let (cmd_tx, _cmd_rx) = mpsc::channel();
    let (evt_tx, evt_rx) = mpsc::channel();
    let mut app = YtmGuiApp::init_with_channels(cmd_tx, evt_rx, "./backups".to_string(), None);

    evt_tx
        .send(WorkerEvent::LibraryCacheHydrated(vec![
            ytm_core::PlaylistSummary {
                id: "PL_CACHE".to_string(),
                title: "Cached Mix".to_string(),
                track_count: Some(12),
                is_liked_music: false,
                owned: true,
            },
        ]))
        .unwrap();
    evt_tx
        .send(WorkerEvent::LibraryLoaded(Err(
            "network unreachable".to_string()
        )))
        .unwrap();
    evt_tx
        .send(WorkerEvent::SessionLoaded(Err("offline".to_string())))
        .unwrap();
    app.drain_events();

    assert!(app.offline_mode);
    let cached = app.library.data().expect("cached library preserved");
    assert_eq!(cached.len(), 1);
    assert_eq!(cached[0].id, "PL_CACHE");
    // Does not hijack active workspace when cached library exists.
    assert_eq!(app.active_tab, GuiTab::Inspector);
}
