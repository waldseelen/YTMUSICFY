use std::sync::{Arc, mpsc};

use ytm_core::{MusicVideoType, Playlist, PlaylistPrivacy, Track, TrackAvailability};
use ytm_store::{CachedPlaylistRecord, SqliteStore};

use super::*;
use crate::state::WorkerEvent;
use crate::worker::BackendState;

struct TestDir(std::path::PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ytm_gui_{label}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("create test dir");
        Self(dir)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn handle_bootstrap_emits_session_err_and_hydrates_records_without_auto_refresh() {
    let temp = TestDir::new("worker_auth");
    let paths = AppPaths::from_root(&temp.0);
    paths.ensure_dirs().expect("ensure dirs");
    let store = Arc::new(SqliteStore::open_in_memory(None).expect("open memory store"));
    let pl = Playlist {
        id: "PL_BOOT".to_string(),
        title: "Boot Mix".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 1,
        tracks: vec![Track {
            video_id: "vid1".to_string(),
            set_video_id: Some("set1".to_string()),
            position: 0,
            title: "Song".to_string(),
            artists: Vec::new(),
            album: None,
            duration_sec: Some(180),
            release_year: None,
            music_video_type: MusicVideoType::AudioTrack,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        }],
    };
    store
        .upsert_cached_playlist_record(&CachedPlaylistRecord {
            playlist: pl,
            manifest: None,
            fetched_at: 1_700_000_000,
        })
        .expect("upsert cached record");

    let cfg = GuiAppConfig {
        preferred_browser: Some(BrowserKind::Brave),
        ..Default::default()
    };
    assert!(!cfg.auto_refresh_browser_cookies);

    let mut state = BackendState {
        paths,
        config: cfg,
        store,
        client: None,
        mutation_lock: Arc::new(tokio::sync::Mutex::new(())),
        active_jobs: std::collections::HashMap::new(),
        next_job_id: 0,
    };
    let (tx, rx) = mpsc::channel();
    handle_bootstrap(&mut state, &tx).await;
    drop(tx);

    let events: Vec<WorkerEvent> = rx.into_iter().collect();
    assert!(events.iter().any(|e| matches!(
        e,
        WorkerEvent::PlaylistCacheHydrated(pls) if pls.len() == 1 && pls[0].id == "PL_BOOT"
    )));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WorkerEvent::SessionLoaded(Err(_)))),
        "handle_bootstrap must always emit SessionLoaded(Err) when unverified"
    );
}

#[test]
fn browser_candidates_include_linux_keyring_and_basictext_fallbacks() {
    let candidates = BrowserKind::Brave.ytdlp_candidates();
    if cfg!(target_os = "linux") {
        assert_eq!(
            candidates.first().map(String::as_str),
            Some("brave+basictext")
        );
        assert!(candidates.contains(&"brave+gnomekeyring".to_string()));
    }
    assert!(candidates.contains(&"brave".to_string()));
    assert!(!allow_gnome_keyring(
        Some("Hyprland"),
        Some("hyprland"),
        None
    ));
    assert!(allow_gnome_keyring(Some("GNOME"), None, None));
    assert!(allow_gnome_keyring(Some("Hyprland"), None, Some("1")));
}
