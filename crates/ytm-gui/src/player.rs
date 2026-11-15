use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use serde_json::{Value, json};
use ytm_core::Track;

use crate::player_ipc::apply_ipc_event;

/// Explicit playback finite-state machine (no parallel boolean flags).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PlayerState {
    #[default]
    Stopped,
    Loading {
        video_id: String,
        title: String,
    },
    Playing {
        video_id: String,
    },
    Paused {
        video_id: String,
    },
    Error(String),
}

impl PlayerState {
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Loading { .. } | Self::Playing { .. } | Self::Paused { .. }
        )
    }

    #[must_use]
    pub fn video_id(&self) -> Option<&str> {
        match self {
            Self::Loading { video_id, .. }
            | Self::Playing { video_id }
            | Self::Paused { video_id } => Some(video_id),
            Self::Stopped | Self::Error(_) => None,
        }
    }
}

/// Snapshot of live playback progress synchronized from the `mpv` IPC listener.
#[derive(Debug, Clone)]
pub struct PlayerSnapshot {
    pub state: PlayerState,
    pub position_sec: f64,
    pub duration_sec: f64,
    pub volume: f64,
    pub ended_counter: u64,
}

impl Default for PlayerSnapshot {
    fn default() -> Self {
        Self {
            state: PlayerState::Stopped,
            position_sec: 0.0,
            duration_sec: 0.0,
            volume: 85.0,
            ended_counter: 0,
        }
    }
}

/// Lightweight local `mpv` JSON Unix-socket IPC playback controller.
pub struct PlayerController {
    ctx: Option<egui::Context>,
    snapshot: Arc<Mutex<PlayerSnapshot>>,
    cmd_tx: Option<Sender<Value>>,
    child: Option<Child>,
    socket_path: PathBuf,
    shutting_down: Arc<AtomicBool>,
}

impl PlayerController {
    #[must_use]
    pub fn new(ctx: Option<egui::Context>) -> Self {
        let pid = std::process::id();
        let socket_path = std::env::temp_dir().join(format!("ytm-architect-mpv-{pid}.sock"));
        Self {
            ctx,
            snapshot: Arc::new(Mutex::new(PlayerSnapshot::default())),
            cmd_tx: None,
            child: None,
            socket_path,
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> PlayerSnapshot {
        self.snapshot.lock().expect("lock player snapshot").clone()
    }

    /// Start playing `track`, lazily spawning `mpv` IPC daemon on first invocation.
    pub fn play_track(&mut self, track: &Track) {
        if !self.ensure_mpv_running() {
            return;
        }
        let url = format!("https://www.youtube.com/watch?v={}", track.video_id);
        let fallback_dur = f64::from(track.duration_sec.unwrap_or(0));
        {
            let mut snap = self.snapshot.lock().expect("lock player snapshot");
            snap.state = PlayerState::Loading {
                video_id: track.video_id.clone(),
                title: track.title.clone(),
            };
            snap.position_sec = 0.0;
            snap.duration_sec = fallback_dur;
        }
        self.send_cmd(json!({ "command": ["set_property", "pause", false] }));
        self.send_cmd(json!({ "command": ["loadfile", url, "replace"] }));
    }

    pub fn toggle_pause(&self) {
        let paused = matches!(
            self.snapshot.lock().expect("lock").state,
            PlayerState::Paused { .. }
        );
        self.send_cmd(json!({ "command": ["set_property", "pause", !paused] }));
    }

    pub fn seek_absolute(&self, sec: f64) {
        let clamped = sec.max(0.0);
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.position_sec = clamped;
        }
        self.send_cmd(json!({ "command": ["seek", clamped, "absolute"] }));
    }

    pub fn set_volume(&self, vol: f64) {
        let clamped = vol.clamp(0.0, 120.0);
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.volume = clamped;
        }
        self.send_cmd(json!({ "command": ["set_property", "volume", clamped] }));
    }

    pub fn stop(&self) {
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.state = PlayerState::Stopped;
            snap.position_sec = 0.0;
        }
        self.send_cmd(json!({ "command": ["stop"] }));
    }

    fn send_cmd(&self, payload: Value) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(payload);
        }
    }

    fn ensure_mpv_running(&mut self) -> bool {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(None) if self.cmd_tx.is_some() => return true,
                _ => {
                    self.child = None;
                    self.cmd_tx = None;
                }
            }
        }
        let _ = std::fs::remove_file(&self.socket_path);
        let initial_vol = self.snapshot.lock().expect("lock").volume;
        let mpv_bin = if PathBuf::from("/usr/bin/mpv").exists() {
            "/usr/bin/mpv"
        } else {
            "mpv"
        };
        let spawned = Command::new(mpv_bin)
            .arg("--no-video")
            .arg("--idle=yes")
            .arg("--really-quiet")
            .arg("--ytdl=yes")
            .arg("--ytdl-format=bestaudio/best")
            .arg("--audio-display=no")
            .arg(format!("--volume={initial_vol:.0}"))
            .arg(format!("--input-ipc-server={}", self.socket_path.display()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        let child = match spawned {
            Ok(c) => c,
            Err(err) => {
                let mut snap = self.snapshot.lock().expect("lock");
                snap.state = PlayerState::Error(format!(
                    "Failed to launch mpv ({mpv_bin}): {err}. Ensure mpv & yt-dlp are installed."
                ));
                return false;
            }
        };
        self.child = Some(child);

        let stream = match connect_with_backoff(&self.socket_path, Duration::from_secs(3)) {
            Ok(s) => s,
            Err(err) => {
                let mut snap = self.snapshot.lock().expect("lock");
                snap.state = PlayerState::Error(format!("mpv IPC connect failed: {err}"));
                return false;
            }
        };
        let Ok(read_stream) = stream.try_clone() else {
            return false;
        };

        let (tx, rx) = mpsc::channel::<Value>();
        self.cmd_tx = Some(tx);
        self.spawn_io_threads(stream, read_stream, rx);
        true
    }

    fn spawn_io_threads(
        &self,
        mut write_stream: UnixStream,
        read_stream: UnixStream,
        rx: Receiver<Value>,
    ) {
        let shutting_down_w = Arc::clone(&self.shutting_down);
        thread::spawn(move || {
            for init in [
                json!({ "command": ["observe_property", 1, "time-pos"] }),
                json!({ "command": ["observe_property", 2, "duration"] }),
                json!({ "command": ["observe_property", 3, "pause"] }),
                json!({ "command": ["observe_property", 4, "core-idle"] }),
            ] {
                if write_json_line(&mut write_stream, &init).is_err() {
                    return;
                }
            }
            while !shutting_down_w.load(Ordering::Relaxed) {
                let Ok(cmd) = rx.recv() else {
                    break;
                };
                if write_json_line(&mut write_stream, &cmd).is_err() {
                    break;
                }
            }
        });

        let snap = Arc::clone(&self.snapshot);
        let ctx = self.ctx.clone();
        let shutting_down_r = Arc::clone(&self.shutting_down);
        thread::spawn(move || {
            let reader = BufReader::new(read_stream);
            for line in reader.lines() {
                if shutting_down_r.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(raw) = line else {
                    break;
                };
                if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                    let changed = apply_ipc_event(&val, &snap);
                    if changed && let Some(c) = &ctx {
                        c.request_repaint();
                    }
                }
            }
        });
    }
}

impl Drop for PlayerController {
    fn drop(&mut self) {
        self.shutting_down.store(true, Ordering::Relaxed);
        self.send_cmd(json!({ "command": ["quit"] }));
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn connect_with_backoff(path: &PathBuf, timeout: Duration) -> std::io::Result<UnixStream> {
    let deadline = Instant::now() + timeout;
    loop {
        match UnixStream::connect(path) {
            Ok(s) => return Ok(s),
            Err(err) => {
                if Instant::now() >= deadline {
                    return Err(err);
                }
                thread::sleep(Duration::from_millis(35));
            }
        }
    }
}

fn write_json_line(stream: &mut UnixStream, value: &Value) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(value).unwrap_or_default();
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    stream.flush()
}
