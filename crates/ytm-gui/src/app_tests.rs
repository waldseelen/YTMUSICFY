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
    // Opens on Home & Explore even when offline/unauthenticated.
    assert_eq!(app.active_tab, GuiTab::Browse);
}

#[test]
fn mini_player_actions_sync_with_player_controller_and_share_search_controller() {
    let (cmd_tx, _cmd_rx) = mpsc::channel();
    let (_evt_tx, evt_rx) = mpsc::channel();
    let mut app = YtmGuiApp::init_with_channels(cmd_tx, evt_rx, "./backups".to_string(), None);

    let t1 = Track {
        video_id: "M1".into(),
        set_video_id: None,
        position: 0,
        title: "One".into(),
        artists: Vec::new(),
        album: None,
        duration_sec: Some(120),
        release_year: None,
        music_video_type: ytm_core::MusicVideoType::AudioTrack,
        availability: ytm_core::TrackAvailability::Available,
        thumbnail_url: None,
    };
    let mut t2 = t1.clone();
    t2.video_id = "M2".into();
    t2.title = "Two".into();

    crate::playback_ops::dispatch_play_actions(
        vec![crate::playback_ops::PlayAction::PlayFromList {
            tracks: vec![t1, t2],
            start_idx: 0,
        }],
        &mut app,
    );
    assert_eq!(app.player.snapshot().state.video_id(), Some("M1"));

    let out = crate::views::player_bar::PlayerBarOutcome {
        next: true,
        toggle_shuffle: true,
        cycle_repeat: true,
        ..Default::default()
    };
    crate::playback_ops::apply_player_bar_outcome(out, &mut app);
    assert!(app.play_queue.is_shuffle());
    assert_eq!(
        app.play_queue.repeat_mode(),
        crate::play_queue::RepeatMode::All
    );
    assert_eq!(app.player.snapshot().state.video_id(), Some("M2"));

    let seek_out = crate::views::player_bar::PlayerBarOutcome {
        seek_to: Some(42.0),
        ..Default::default()
    };
    crate::playback_ops::apply_player_bar_outcome(seek_out, &mut app);
    assert!((app.player.snapshot().position_sec - 42.0).abs() < 0.01);

    // Mini Player and top bar share the exact same SearchController instance.
    let ptr_before: *const SearchController = &app.search_controller;
    app.search_controller.on_query_change("daft", 10);
    let ptr_after: *const SearchController = &app.search_controller;
    assert_eq!(ptr_before, ptr_after);
    assert_eq!(app.search_controller.query, "daft");
}

#[test]
fn home_browse_cache_hydrates_first_and_survives_offline_failure() {
    let (cmd_tx, _cmd_rx) = mpsc::channel();
    let (evt_tx, evt_rx) = mpsc::channel();
    let mut app = YtmGuiApp::init_with_channels(cmd_tx, evt_rx, "./backups".to_string(), None);

    let cached_home = BrowsePage {
        browse_id: ytm_core::BROWSE_HOME.to_string(),
        kind: ytm_core::BrowsePageKind::Home,
        title: "Home".to_string(),
        subtitle: None,
        artists: Vec::new(),
        thumbnail_url: None,
        sections: vec![ytm_core::BrowseSection {
            title: "Listen again".to_string(),
            cards: Vec::new(),
            tracks: Vec::new(),
            browse_id: None,
            browse_params: None,
        }],
        tracks: Vec::new(),
        continuation: None,
    };

    evt_tx
        .send(WorkerEvent::BrowseCacheHydrated(cached_home))
        .unwrap();
    evt_tx
        .send(WorkerEvent::BrowseLoaded {
            browse_id: ytm_core::BROWSE_HOME.to_string(),
            result: Err("offline".to_string()),
        })
        .unwrap();
    app.drain_events();

    assert!(app.offline_mode);
    assert!(app.browse_from_cache);
    let page = app.browse_state.data().expect("cached home kept");
    assert_eq!(page.sections[0].title, "Listen again");
}
