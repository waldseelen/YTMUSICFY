use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ytm_core::{
    InnerTubeClient, InnerTubeClientTrait, ReqwestTransport, SessionStatus,
    netscape_cookies_to_header, parse_auth_input,
};
use ytm_store::AppPaths;

use crate::auth_store::{
    ensure_runtime_cookie_dir, mpv_cookie_jar_path, remove_mpv_cookie_jar_at,
    save_credentials_secure, sweep_stale_cookie_files, write_mpv_cookie_jar,
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
            // Try `basictext` (v10) and `yt-dlp` auto-detection before explicit keyring backends
            // so tiling/Wayland desktops don't pop up GNOME "Choose password for new keyring" dialogs.
            vec![
                format!("{base}+basictext"),
                base.to_string(),
                format!("{base}+kwallet6"),
                format!("{base}+kwallet5"),
                format!("{base}+gnomekeyring"),
            ]
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

fn allow_gnome_keyring(desktop: Option<&str>, session: Option<&str>, opt_in: Option<&str>) -> bool {
    if opt_in.is_some_and(|v| v.trim() == "1" || v.trim().eq_ignore_ascii_case("true")) {
        return true;
    }
    [desktop, session].into_iter().flatten().any(|v| {
        let lower = v.to_ascii_lowercase();
        ["gnome", "ubuntu", "cinnamon", "mate", "xfce", "budgie"]
            .iter()
            .any(|de| lower.contains(de))
    })
}

pub async fn import_browser_auth_inner(
    paths: &AppPaths,
    cfg: &GuiAppConfig,
    browser: BrowserKind,
) -> Result<(InnerTubeClient<ReqwestTransport>, SessionStatus)> {
    paths.ensure_dirs()?;
    let runtime_dir = ensure_runtime_cookie_dir()?;
    let gnome_ok = allow_gnome_keyring(
        std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
        std::env::var("DESKTOP_SESSION").ok().as_deref(),
        std::env::var("YTM_USE_SECRET_SERVICE").ok().as_deref(),
    );

    let mut last_err = String::new();
    for spec in browser.ytdlp_candidates() {
        if spec.ends_with("+gnomekeyring") && !gnome_ok {
            continue;
        }
        let temp_dir = tempfile::Builder::new()
            .prefix("ytm-cookie-")
            .tempdir_in(&runtime_dir)
            .context("creating private runtime directory for browser cookie extraction")?;
        // Pass a non-existent path inside the 0700 temp dir so yt-dlp does not attempt
        // to parse a 0-byte file as a Netscape jar before writing extracted cookies.
        let tmp_path = temp_dir.path().join("cookies.txt");

        let output = tokio::process::Command::new("yt-dlp")
            .args([
                "--ignore-config",
                "--no-warnings",
                "--cookies-from-browser",
                &spec,
                "--cookies",
                &tmp_path.to_string_lossy(),
                "--skip-download",
                "--quiet",
                "about:blank",
            ])
            .output()
            .await
            .context("running yt-dlp (install yt-dlp or paste headers manually)")?;

        let cookie_txt = std::fs::read_to_string(&tmp_path);
        drop(temp_dir);

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

/// Reads clipboard text asynchronously off the UI thread (`wl-paste`/`xclip`/`powershell`)
/// to prevent Wayland `egui` clipboard freezes when pasting large DevTools headers.
pub async fn import_clipboard_auth_inner(
    paths: &AppPaths,
    cfg: &GuiAppConfig,
) -> Result<(InnerTubeClient<ReqwestTransport>, SessionStatus)> {
    let raw = read_system_clipboard_async().await?;
    if raw.trim().is_empty() {
        anyhow::bail!("Clipboard is empty.");
    }
    import_auth_inner(paths, cfg, &raw).await
}

async fn read_system_clipboard_async() -> Result<String> {
    #[cfg(windows)]
    let candidates: &[(&str, &[&str])] =
        &[("powershell", &["-NoProfile", "-Command", "Get-Clipboard"])];
    #[cfg(not(windows))]
    let candidates: &[(&str, &[&str])] = &[
        ("wl-paste", &["--no-newline"]),
        ("xclip", &["-o", "-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--output"]),
    ];

    for (bin, args) in candidates {
        let fut = tokio::process::Command::new(bin).args(*args).output();
        if let Ok(Ok(out)) = tokio::time::timeout(std::time::Duration::from_secs(4), fut).await
            && out.status.success()
        {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
    }
    anyhow::bail!(
        "Failed reading system clipboard (ensure wl-paste/xclip is installed or use Import from File...)."
    )
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

    if let Ok(Some(summaries)) = state.store.load_library_summaries()
        && !summaries.is_empty()
    {
        let _ = tx.send(crate::state::WorkerEvent::LibraryCacheHydrated(summaries));
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
                if let Ok(list) = &libs {
                    crate::worker_exec::sync_library_cache(&state.store, list);
                }
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
                if let Ok(list) = &libs {
                    crate::worker_exec::sync_library_cache(&state.store, list);
                }
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
#[path = "worker_auth_tests.rs"]
mod tests;
