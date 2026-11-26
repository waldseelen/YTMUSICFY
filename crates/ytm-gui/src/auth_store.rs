use std::ffi::OsStr;
#[cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ytm_core::AuthCredentials;
use ytm_store::{AppPaths, write_secret_file};

use crate::state::GuiAppConfig;

/// Audio stream quality preset forwarded to `mpv` / `yt-dlp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioQuality {
    #[default]
    AutoPremium,
    BalancedOpus,
    DataSaver,
}

impl AudioQuality {
    pub const ALL: [Self; 3] = [Self::AutoPremium, Self::BalancedOpus, Self::DataSaver];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AutoPremium => "Auto (Premium 256k AAC / Opus)",
            Self::BalancedOpus => "Balanced (160k Opus)",
            Self::DataSaver => "Data Saver (<= 96 kbps)",
        }
    }

    #[must_use]
    pub const fn ytdl_format(self) -> &'static str {
        match self {
            Self::AutoPremium => "bestaudio[abr>=192]/bestaudio/best",
            Self::BalancedOpus => "bestaudio[acodec=opus]/bestaudio/best",
            Self::DataSaver => "bestaudio[abr<=96]/worstaudio/bestaudio",
        }
    }
}

/// Persist credentials into OS Secret Service (`secret-tool`) on Unix when available, plus `0600` `auth.json`.
pub fn save_credentials_secure(paths: &AppPaths, creds: &AuthCredentials) -> Result<()> {
    paths.ensure_dirs()?;
    let json_bytes = serde_json::to_vec_pretty(creds)?;
    write_secret_file(&paths.auth_file(), &json_bytes)?;

    #[cfg(unix)]
    if let Ok(mut child) = Command::new("secret-tool")
        .args([
            "store",
            "--label=ytm-architect InnerTube Auth",
            "service",
            "ytm-architect",
            "account",
            "default",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(&json_bytes);
        }
        let _ = child.wait();
    }
    Ok(())
}

/// Load credentials from OS Secret Service on Unix first, falling back to `0600`/user-scoped `auth.json`.
pub fn load_credentials_secure(paths: &AppPaths) -> Result<AuthCredentials> {
    #[cfg(unix)]
    if let Ok(out) = Command::new("secret-tool")
        .args(["lookup", "service", "ytm-architect", "account", "default"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        && out.status.success()
        && let Ok(creds) = serde_json::from_slice::<AuthCredentials>(&out.stdout)
    {
        return Ok(creds);
    }

    let bytes = std::fs::read(paths.auth_file()).context("reading saved auth.json")?;
    serde_json::from_slice(&bytes).context("decoding auth.json")
}

fn resolve_runtime_cookie_dir(xdg_runtime: Option<&OsStr>, fallback_temp: &Path) -> PathBuf {
    if let Some(dir) = xdg_runtime
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join("ytm-architect");
    }
    fallback_temp.join("ytm-architect")
}

/// Resolve runtime cookie directory (`$XDG_RUNTIME_DIR/ytm-architect` on Linux, temp dir fallback).
#[must_use]
pub fn runtime_cookie_dir() -> PathBuf {
    resolve_runtime_cookie_dir(
        std::env::var_os("XDG_RUNTIME_DIR").as_deref(),
        &std::env::temp_dir(),
    )
}

/// Ensure the runtime cookie directory exists with restrictive permissions (`0700` on Unix).
pub fn ensure_runtime_cookie_dir() -> Result<PathBuf> {
    let dir = runtime_cookie_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating runtime cookie dir '{}'", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    Ok(dir)
}

/// Write a `0600` Netscape cookie jar for `mpv`/`yt-dlp` from `AuthCredentials` in runtime storage.
pub fn write_mpv_cookie_jar(paths: &AppPaths, creds: &AuthCredentials) -> Result<PathBuf> {
    let _ = ensure_runtime_cookie_dir()?;
    let jar_path = mpv_cookie_jar_path(paths);
    let content = cookie_header_to_netscape(&creds.cookie);
    write_secret_file(&jar_path, content.as_bytes())?;
    Ok(jar_path)
}

#[must_use]
pub fn mpv_cookie_jar_path(_paths: &AppPaths) -> PathBuf {
    runtime_cookie_dir().join("ytm-cookies.txt")
}

pub fn remove_mpv_cookie_jar_at(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[must_use]
pub fn active_cookie_jar_path(paths: &AppPaths) -> Option<PathBuf> {
    let path = mpv_cookie_jar_path(paths);
    path.exists().then_some(path)
}

/// Sweep stale temporary browser cookie files and legacy persistent cookie jars.
pub fn sweep_stale_cookie_files(paths: &AppPaths) {
    sweep_browser_temps_in(&runtime_cookie_dir());
    sweep_browser_temps_in(paths.data_dir());

    let legacy_jar = paths.data_dir().join("ytm-cookies.txt");
    if legacy_jar != mpv_cookie_jar_path(paths) {
        let _ = std::fs::remove_file(legacy_jar);
    }
}

fn sweep_browser_temps_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("ytm-browser-cookies-") && name.ends_with(".txt") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub fn save_gui_config(paths: &AppPaths, cfg: &GuiAppConfig) -> Result<()> {
    paths.ensure_dirs()?;
    let body = toml::to_string_pretty(cfg).context("serializing config.toml")?;
    std::fs::write(paths.config_file(), body).context("writing config.toml")
}

/// Format raw `"k1=v1; k2=v2"` Cookie header into Netscape cookie file rows for `.youtube.com`.
#[must_use]
pub fn cookie_header_to_netscape(cookie_header: &str) -> String {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for pair in cookie_header.split(';') {
        let trimmed = pair.trim();
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = name.trim();
        let val = value.trim();
        if key.is_empty() || val.is_empty() {
            continue;
        }
        out.push_str(&format!(
            ".youtube.com\tTRUE\t/\tTRUE\t2147483647\t{key}\t{val}\n"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_header_to_netscape_formats_valid_youtube_rows() {
        let raw = "__Secure-3PAPISID=abc=123; SID=xyz; empty=";
        let txt = cookie_header_to_netscape(raw);
        assert!(txt.starts_with("# Netscape HTTP Cookie File\n"));
        assert!(
            txt.contains(".youtube.com\tTRUE\t/\tTRUE\t2147483647\t__Secure-3PAPISID\tabc=123\n")
        );
        assert!(txt.contains(".youtube.com\tTRUE\t/\tTRUE\t2147483647\tSID\txyz\n"));
        assert!(!txt.contains("empty="));
    }

    #[test]
    fn runtime_cookie_dir_prefers_xdg_runtime_dir_and_falls_back_to_temp() {
        let fallback = Path::new("/tmp/fallback");
        assert_eq!(
            resolve_runtime_cookie_dir(Some(OsStr::new("/run/user/1000")), fallback),
            PathBuf::from("/run/user/1000/ytm-architect")
        );
        assert_eq!(
            resolve_runtime_cookie_dir(Some(OsStr::new("")), fallback),
            PathBuf::from("/tmp/fallback/ytm-architect")
        );
        assert_eq!(
            resolve_runtime_cookie_dir(None, fallback),
            PathBuf::from("/tmp/fallback/ytm-architect")
        );
    }

    struct TestDir(PathBuf);

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

    #[test]
    fn write_mpv_cookie_jar_sets_0600_and_sweep_cleans_stale_temp_files() {
        let temp = TestDir::new("auth_store");
        let paths = AppPaths::from_root(&temp.0);
        paths.ensure_dirs().expect("ensure dirs");

        let creds = AuthCredentials {
            cookie: "SAPISID=abc; SID=xyz".to_string(),
            user_agent: "Mozilla/5.0".to_string(),
            x_goog_authuser: Some("0".to_string()),
            x_goog_page_id: None,
            x_goog_visitor_id: None,
        };
        let jar = write_mpv_cookie_jar(&paths, &creds).expect("write cookie jar");
        assert!(jar.starts_with(runtime_cookie_dir()));
        assert!(jar.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&jar).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let stale_runtime = runtime_cookie_dir().join("ytm-browser-cookies-999999.txt");
        std::fs::write(&stale_runtime, "stale").expect("write stale runtime");
        let stale_data = paths.data_dir().join("ytm-browser-cookies-888888.txt");
        std::fs::write(&stale_data, "stale").expect("write stale data");
        let legacy_jar = paths.data_dir().join("ytm-cookies.txt");
        std::fs::write(&legacy_jar, "legacy").expect("write legacy jar");

        sweep_stale_cookie_files(&paths);
        assert!(!stale_runtime.exists());
        assert!(!stale_data.exists());
        assert!(!legacy_jar.exists());

        remove_mpv_cookie_jar_at(&jar);
    }
}
