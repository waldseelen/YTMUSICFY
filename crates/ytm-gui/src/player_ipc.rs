use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use serde_json::Value;

use crate::player::{PlayerSnapshot, PlayerState};

const TIME_POS_REPAINT_MIN_INTERVAL: Duration = Duration::from_millis(200);
const TIME_POS_BACKGROUND_REPAINT_INTERVAL: Duration = Duration::from_millis(1500);

#[cfg(unix)]
pub(crate) type IpcStream = std::os::unix::net::UnixStream;
#[cfg(windows)]
pub(crate) type IpcStream = std::fs::File;

/// Structured classification of `mpv` / `yt-dlp` stream playback failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackErrorKind {
    CookieExpired,
    GeoBlocked,
    AgeRestricted,
    MissingYtdlp,
    Network,
    Other(String),
}

impl PlaybackErrorKind {
    #[must_use]
    pub fn user_message(&self) -> String {
        match self {
            Self::CookieExpired => "Stream blocked: YouTube cookies expired — refresh Auth.".into(),
            Self::GeoBlocked => "Stream unavailable in your region (geo-blocked).".into(),
            Self::AgeRestricted => {
                "Age-restricted track requires signed-in browser cookies.".into()
            }
            Self::MissingYtdlp => "yt-dlp not found in PATH — install yt-dlp to play audio.".into(),
            Self::Network => "Network error while resolving or buffering audio stream.".into(),
            Self::Other(msg) => msg.clone(),
        }
    }
}

#[must_use]
pub fn default_ipc_endpoint(pid: u32) -> String {
    #[cfg(unix)]
    {
        std::env::temp_dir()
            .join(format!("ytm-architect-mpv-{pid}.sock"))
            .to_string_lossy()
            .into_owned()
    }
    #[cfg(windows)]
    {
        format!(r"\\.\pipe\ytm-architect-mpv-{pid}")
    }
}

pub(crate) fn cleanup_ipc_endpoint(endpoint: &str) {
    #[cfg(unix)]
    let _ = std::fs::remove_file(endpoint);
    #[cfg(not(unix))]
    let _ = endpoint;
}

#[must_use]
pub fn resolve_mpv_binary() -> &'static str {
    #[cfg(unix)]
    if Path::new("/usr/bin/mpv").exists() {
        return "/usr/bin/mpv";
    }
    #[cfg(windows)]
    {
        return "mpv.exe";
    }
    #[cfg(not(windows))]
    "mpv"
}

pub(crate) fn connect_with_backoff(
    endpoint: &str,
    timeout: Duration,
) -> std::io::Result<IpcStream> {
    let deadline = Instant::now() + timeout;
    loop {
        #[cfg(unix)]
        let attempt = std::os::unix::net::UnixStream::connect(endpoint);
        #[cfg(windows)]
        let attempt = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(endpoint);
        match attempt {
            Ok(s) => return Ok(s),
            Err(err) if Instant::now() >= deadline => return Err(err),
            Err(_) => std::thread::sleep(Duration::from_millis(35)),
        }
    }
}

pub(crate) fn write_json_line(stream: &mut IpcStream, value: &Value) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(value).unwrap_or_default();
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    stream.flush()
}

pub(crate) fn spawn_ipc_reader(
    read_stream: IpcStream,
    snapshot: Arc<Mutex<PlayerSnapshot>>,
    ctx: Option<egui::Context>,
    log_path: PathBuf,
) {
    std::thread::spawn(move || {
        let mut last_time_pos: Option<Instant> = None;
        for raw in BufReader::new(read_stream).lines().map_while(Result::ok) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw)
                && apply_ipc_event_throttled(
                    &val,
                    &snapshot,
                    &mut last_time_pos,
                    Instant::now(),
                    Some(&log_path),
                )
                && let Some(c) = &ctx
            {
                c.request_repaint();
            }
        }
    });
}

/// Classify the root cause from the tail of `ytm-architect-mpv.log`.
#[must_use]
pub fn classify_mpv_log_error(log_tail: &str, fallback: &str) -> PlaybackErrorKind {
    let lower = log_tail.to_ascii_lowercase();
    let table = [
        ("ytdl_hook] failed to run", PlaybackErrorKind::MissingYtdlp),
        ("yt-dlp: not found", PlaybackErrorKind::MissingYtdlp),
        ("yt-dlp not found", PlaybackErrorKind::MissingYtdlp),
        (
            "sign in to confirm you're not a bot",
            PlaybackErrorKind::CookieExpired,
        ),
        (
            "cookies are no longer valid",
            PlaybackErrorKind::CookieExpired,
        ),
        ("http error 403", PlaybackErrorKind::CookieExpired),
        ("login required", PlaybackErrorKind::CookieExpired),
        (
            "sign in to confirm your age",
            PlaybackErrorKind::AgeRestricted,
        ),
        ("age-restricted", PlaybackErrorKind::AgeRestricted),
        (
            "not made this video available in your country",
            PlaybackErrorKind::GeoBlocked,
        ),
        ("geo-restricted", PlaybackErrorKind::GeoBlocked),
        ("blocked it in your country", PlaybackErrorKind::GeoBlocked),
        ("timed out", PlaybackErrorKind::Network),
        ("could not resolve host", PlaybackErrorKind::Network),
        ("connection reset", PlaybackErrorKind::Network),
        ("network is unreachable", PlaybackErrorKind::Network),
    ];
    for (needle, kind) in table {
        if lower.contains(needle) {
            return kind;
        }
    }
    let detail = log_tail
        .lines()
        .rev()
        .find(|l| l.to_ascii_lowercase().contains("error"))
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or(fallback);
    PlaybackErrorKind::Other(detail.to_string())
}

pub(crate) fn read_mpv_log_tail(log_path: &Path) -> String {
    let Ok(bytes) = std::fs::read(log_path) else {
        return String::new();
    };
    let start = bytes.len().saturating_sub(4096);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

/// Apply an `mpv` JSON IPC event line while throttling `time-pos` repaints to `>= 200ms`.
pub(crate) fn apply_ipc_event_throttled(
    event: &Value,
    snapshot: &Arc<Mutex<PlayerSnapshot>>,
    last_time_pos_repaint: &mut Option<Instant>,
    now: Instant,
    log_path: Option<&Path>,
) -> bool {
    let kind = event.get("event").and_then(Value::as_str).unwrap_or("");
    let mut snap = snapshot.lock().expect("lock player snapshot");
    match kind {
        "property-change" => apply_property_change(event, &mut snap, last_time_pos_repaint, now),
        "playback-restart" => {
            if let PlayerState::Loading { video_id, .. } = &snap.state {
                snap.state = PlayerState::Playing {
                    video_id: video_id.clone(),
                };
                snap.last_error_kind = None;
                return true;
            }
            false
        }
        "end-file" => match event.get("reason").and_then(Value::as_str).unwrap_or("") {
            "eof" => {
                snap.ended_counter = snap.ended_counter.saturating_add(1);
                snap.position_sec = 0.0;
                true
            }
            "error" => {
                let err_txt = event
                    .get("file_error")
                    .and_then(Value::as_str)
                    .unwrap_or("Stream playback failed");
                let tail = log_path.map(read_mpv_log_tail).unwrap_or_default();
                let err_kind = classify_mpv_log_error(&tail, err_txt);
                snap.state = PlayerState::Error(err_kind.user_message());
                snap.last_error_kind = Some(err_kind);
                true
            }
            _ => false,
        },
        _ => false,
    }
}

fn apply_property_change(
    event: &Value,
    snap: &mut PlayerSnapshot,
    last_time_pos_repaint: &mut Option<Instant>,
    now: Instant,
) -> bool {
    let name = event.get("name").and_then(Value::as_str).unwrap_or("");
    let Some(data) = event.get("data") else {
        return false;
    };
    match name {
        "time-pos" => {
            if let Some(pos) = data.as_f64() {
                snap.position_sec = pos.max(0.0);
                let interval = if snap.window_active {
                    TIME_POS_REPAINT_MIN_INTERVAL
                } else {
                    TIME_POS_BACKGROUND_REPAINT_INTERVAL
                };
                if last_time_pos_repaint
                    .is_none_or(|prev| now.saturating_duration_since(prev) >= interval)
                {
                    *last_time_pos_repaint = Some(now);
                    return true;
                }
            }
        }
        "duration" => {
            if let Some(dur) = data.as_f64()
                && dur > 0.0
            {
                snap.duration_sec = dur;
                return true;
            }
        }
        "pause" => {
            if let Some(paused) = data.as_bool()
                && let Some(vid) = snap.state.video_id().map(str::to_string)
            {
                snap.state = if paused {
                    PlayerState::Paused { video_id: vid }
                } else {
                    PlayerState::Playing { video_id: vid }
                };
                return true;
            }
        }
        "audio-codec-name" => {
            let next = data
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            if next != snap.codec_name {
                snap.codec_name = next;
                return true;
            }
        }
        "audio-bitrate" => {
            if let Some(bps) = data.as_f64()
                && bps > 0.0
                && let Ok(kbps) = format!("{:.0}", bps / 1000.0).parse::<u32>()
                && Some(kbps) != snap.bitrate_kbps
            {
                snap.bitrate_kbps = Some(kbps);
                return true;
            }
        }
        _ => {}
    }
    false
}

#[cfg(test)]
#[path = "player_ipc_tests.rs"]
mod tests;
