use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ytm_core::{
    InnerTubeClient, InnerTubeClientTrait, ReqwestTransport, SessionStatus,
    netscape_cookies_to_header, parse_auth_input,
};
use ytm_store::AppPaths;

use crate::auth_store::{save_credentials_secure, write_mpv_cookie_jar};
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
    let tmp_path = paths
        .data_dir()
        .join(format!("ytm-browser-cookies-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&tmp_path);

    let output = tokio::process::Command::new("yt-dlp")
        .args([
            "--cookies-from-browser",
            browser.ytdlp_arg(),
            "--cookies",
            &tmp_path.to_string_lossy(),
            "--skip-download",
            "--no-playlist",
            "--quiet",
            "https://music.youtube.com/",
        ])
        .output()
        .await
        .context("running yt-dlp (install yt-dlp or paste headers manually)")?;

    let cookie_txt = std::fs::read_to_string(&tmp_path);
    let _ = std::fs::remove_file(&tmp_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "yt-dlp failed reading {} cookies: {}",
            browser.label(),
            stderr.trim()
        );
    }
    let raw_netscape = cookie_txt.context("reading exported browser cookies")?;
    let cookie_header = netscape_cookies_to_header(&raw_netscape)
        .context("missing signed-in YouTube Music cookies in browser profile")?;
    import_auth_inner(paths, cfg, &cookie_header).await
}

pub(crate) async fn handle_bootstrap(
    state: &mut crate::worker::BackendState,
    tx: &std::sync::mpsc::Sender<crate::state::WorkerEvent>,
) {
    use ytm_core::InnerTubeClientTrait;
    use ytm_store::SnapshotStore;

    if let Ok(snaps) = state.store.list_snapshots(None).await {
        let _ = tx.send(crate::state::WorkerEvent::SnapshotsLoaded(Ok(snaps)));
    }
    let mut verified = false;
    if let Some(client) = &state.client {
        let sess = client.verify_session().await.map_err(|e| e.to_string());
        verified = sess.as_ref().is_ok_and(|s| s.authenticated);
        if verified {
            let _ = tx.send(crate::state::WorkerEvent::SessionLoaded(sess));
            let libs = client
                .get_library_playlists()
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(crate::state::WorkerEvent::LibraryLoaded(libs));
        } else if state.config.preferred_browser.is_none() {
            let _ = tx.send(crate::state::WorkerEvent::SessionLoaded(sess));
        }
    }
    if !verified {
        if let Some(browser) = state.config.preferred_browser
            && let Ok((client, status)) =
                import_browser_auth_inner(&state.paths, &state.config, browser).await
        {
            let libs = client
                .get_library_playlists()
                .await
                .map_err(|e| e.to_string());
            state.client = Some(client);
            let _ = tx.send(crate::state::WorkerEvent::AudioConfigUpdated {
                cookie_jar: crate::auth_store::active_cookie_jar_path(&state.paths),
                quality: state.config.audio_quality,
                preferred_browser: state.config.preferred_browser,
            });
            let _ = tx.send(crate::state::WorkerEvent::SessionLoaded(Ok(status)));
            let _ = tx.send(crate::state::WorkerEvent::LibraryLoaded(libs));
        } else if state.client.is_none() {
            let _ = tx.send(crate::state::WorkerEvent::SessionLoaded(Err(
                "No saved auth.json. Paste browser headers in Auth tab.".to_string(),
            )));
        }
    }
}
