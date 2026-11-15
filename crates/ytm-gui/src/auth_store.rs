use std::io::Write;
use std::path::{Path, PathBuf};
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

/// Persist credentials into OS Secret Service (`secret-tool`) when available, plus `0600` `auth.json`.
pub fn save_credentials_secure(paths: &AppPaths, creds: &AuthCredentials) -> Result<()> {
    paths.ensure_dirs()?;
    let json_bytes = serde_json::to_vec_pretty(creds)?;
    write_secret_file(&paths.auth_file(), &json_bytes)?;

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

/// Load credentials from OS Secret Service first, falling back to `0600` `auth.json`.
pub fn load_credentials_secure(paths: &AppPaths) -> Result<AuthCredentials> {
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

/// Write a `0600` Netscape cookie jar for `mpv`/`yt-dlp` from `AuthCredentials`.
pub fn write_mpv_cookie_jar(paths: &AppPaths, creds: &AuthCredentials) -> Result<PathBuf> {
    paths.ensure_dirs()?;
    let jar_path = mpv_cookie_jar_path(paths);
    let content = cookie_header_to_netscape(&creds.cookie);
    write_secret_file(&jar_path, content.as_bytes())?;
    Ok(jar_path)
}

#[must_use]
pub fn mpv_cookie_jar_path(paths: &AppPaths) -> PathBuf {
    paths.data_dir().join("ytm-cookies.txt")
}

pub fn remove_mpv_cookie_jar_at(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[must_use]
pub fn active_cookie_jar_path(paths: &AppPaths) -> Option<PathBuf> {
    let path = mpv_cookie_jar_path(paths);
    path.exists().then_some(path)
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
}
