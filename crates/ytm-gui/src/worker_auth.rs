use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ytm_core::{
    InnerTubeClient, InnerTubeClientTrait, ReqwestTransport, SessionStatus,
    netscape_cookies_to_header, parse_auth_input,
};
use ytm_store::{AppPaths, write_secret_file};

use crate::auth_store::{
    browser_cookie_temp_path, ensure_runtime_cookie_dir, mpv_cookie_jar_path,
    remove_mpv_cookie_jar_at, save_credentials_secure, sweep_stale_cookie_files,
    write_mpv_cookie_jar,
};
use crate::state::GuiAppConfig;

/// Supported local browsers for one-click cookie extraction via `yt-dlp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BrowserKind {
    #[default]
    Brave,
    Chrome,
    Chromium,
    Firefox,
}

impl BrowserKind {
    pub const ALL: [Self; 4] = [Self::Brave, Self::Chrome, Self::Chromium, Self::Firefox];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Brave => "Brave",
            Self::Chrome => "Chrome",
            Self::Chromium => "Chromium",
            Self::Firefox => "Firefox",
        }
    }

    #[must_use]
    pub const fn ytdlp_arg(self) -> &'static str {
        match self {
            Self::Brave => "brave",
            Self::Chrome => "chrome",
            Self::Chromium => "chromium",
            Self::Firefox => "firefox",
        }
    }

    #[must_use]
    pub fn ytdlp_candidates(self) -> Vec<String> {
        let base = self.ytdlp_arg();
        if cfg!(target_os = "linux") && !matches!(self, Self::Firefox) {
            vec![format!("{base}+gnomekeyring"), base.to_string()]
        } else {
            vec![base.to_string()]
        }
    }
}

pub async fn import_auth_inner(
    paths: &AppPaths,
    cfg: &GuiAppConfig,
    raw: &str,
) -> Result<(InnerTubeClient<ReqwestTransport>, SessionStatus)> {
    let creds = parse_auth_input(raw)?;
    let transport = ReqwestTransport::new(cfg.innertube.max_retries)?;
    let client = InnerTubeClient::new(transport, creds.clone(), cfg.innertube.clone());
    let status = client.verify_session().await?;
    if !status.authenticated {
        anyhow::bail!("session canary returned unauthenticated");
    }
    paths.ensure_dirs()?;
    save_credentials_secure(paths, &creds)?;
    let _ = write_mpv_cookie_jar(paths, &creds);
    Ok((client, status))
}

pub async fn import_browser_auth_inner(
    paths: &AppPaths,
    cfg: &GuiAppConfig,
    browser: BrowserKind,
) -> Result<(InnerTubeClient<ReqwestTransport>, SessionStatus)> {
    paths.ensure_dirs()?;
    let _ = ensure_runtime_cookie_dir()?;
    let tmp_path = browser_cookie_temp_path();

    let mut last_err = String::new();
    for spec in browser.ytdlp_candidates() {
        write_secret_file(&tmp_path, b"")?;
        let output = tokio::process::Command::new("yt-dlp")
            .args([
                "--ignore-config",
                "--cookies-from-browser",
                &spec,
                "--cookies",
                &tmp_path.to_string_lossy(),
                "--skip-download",
                "--no-playlist",
                "--quiet",
                "https://www.youtube.com/watch?v=BaW_jenozKc",
            ])
            .output()
            .await
            .context("running yt-dlp (install yt-dlp or paste headers manually)");

        let output = match output {
            Ok(out) => out,
            Err(err) => {
                let _ = std::fs::remove_file(&tmp_path);
                return Err(err);
            }
        };

        let cookie_txt = std::fs::read_to_string(&tmp_path);
        let _ = std::fs::remove_file(&tmp_path);

        if let Ok(raw_netscape) = cookie_txt
            && let Ok(cookie_header) = netscape_cookies_to_header(&raw_netscape)
        {
            return import_auth_inner(paths, cfg, &cookie_header).await;
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stderr.is_empty() {
            last_err = stderr;
        }
        if last_err.contains("secretstorage") {
            anyhow::bail!(
                "yt-dlp needs Python `secretstorage` to decrypt {} keyring cookies. Run: sudo pacman -S python-secretstorage",
                browser.label()
            );
        }
    }

    anyhow::bail!(
        "yt-dlp failed reading {} cookies: {}",
        browser.label(),
        if last_err.is_empty() {
            "no signed-in YouTube Music cookies found".to_string()
        } else {
            last_err
        }
    );
}

pub(crate) async fn handle_bootstrap(
    state: &mut crate::worker::BackendState,
    tx: &std::sync::mpsc::Sender<crate::state::WorkerEvent>,
) {
    use ytm_core::InnerTubeClientTrait;
    use ytm_store::SnapshotStore;

    sweep_stale_cookie_files(&state.paths);
    if state.client.is_none() {
        remove_mpv_cookie_jar_at(&mpv_cookie_jar_path(&state.paths));
    }

    if let Ok(records) = state.store.load_all_cached_playlist_records()
        && !records.is_empty()
    {
        let mut cached = Vec::with_capacity(records.len());
        for rec in records {
            if let Some(manifest) = rec.manifest {
                let _ = tx.send(crate::state::WorkerEvent::PlaylistManifestLoaded {
                    playlist_id: rec.playlist.id.clone(),
                    manifest,
                });
            }
            cached.push(rec.playlist);
        }
        let _ = tx.send(crate::state::WorkerEvent::PlaylistCacheHydrated(cached));
    }
    if let Ok(snaps) = state.store.list_snapshots(None).await {
        let _ = tx.send(crate::state::WorkerEvent::SnapshotsLoaded(Ok(snaps)));
    }

    let mut verify_err: Option<String> = None;
    if let Some(client) = &state.client {
        match client.verify_session().await {
            Ok(status) if status.authenticated => {
                let _ = tx.send(crate::state::WorkerEvent::SessionLoaded(Ok(status)));
                let libs = client
                    .get_library_playlists()
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(crate::state::WorkerEvent::LibraryLoaded(libs));
                return;
            }
            Ok(_) => {
                verify_err = Some("Saved session is unauthenticated or expired.".to_string());
            }
            Err(err) => {
                verify_err = Some(err.to_string());
            }
        }
    }

    if state.config.auto_refresh_browser_cookies
        && let Some(browser) = state.config.preferred_browser
    {
        match import_browser_auth_inner(&state.paths, &state.config, browser).await {
            Ok((client, status)) => {
                let libs = client
                    .get_library_playlists()
                    .await
                    .map_err(|e| e.to_string());
                state.client = Some(std::sync::Arc::new(client));
                let _ = tx.send(crate::state::WorkerEvent::AudioConfigUpdated {
                    cookie_jar: crate::auth_store::active_cookie_jar_path(&state.paths),
                    quality: state.config.audio_quality,
                    preferred_browser: state.config.preferred_browser,
                });
                let _ = tx.send(crate::state::WorkerEvent::SessionLoaded(Ok(status)));
                let _ = tx.send(crate::state::WorkerEvent::LibraryLoaded(libs));
                return;
            }
            Err(err) => {
                let msg = verify_err.map_or_else(
                    || err.to_string(),
                    |prev| format!("{prev}; browser refresh failed: {err}"),
                );
                let _ = tx.send(crate::state::WorkerEvent::SessionLoaded(Err(msg)));
                return;
            }
        }
    }

    let msg = verify_err
        .unwrap_or_else(|| "No saved auth.json. Paste browser headers in Auth tab.".to_string());
    let _ = tx.send(crate::state::WorkerEvent::SessionLoaded(Err(msg)));
}

#[cfg(test)]
mod tests {
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
}
