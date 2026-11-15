use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;
use serde_json::{Value, json};
use ytm_core::Track;

use crate::auth_store::{AudioQuality, remove_mpv_cookie_jar_at};
use crate::player_ipc::{apply_ipc_event, connect_with_backoff, write_json_line};

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

enum ActorCmd {
    Configure {
        cookie_jar: Option<PathBuf>,
        quality: AudioQuality,
    },
    Play {
        video_id: String,
    },
    SetPause(bool),
    SeekAbsolute(f64),
    SetVolume(f64),
    Stop,
    Shutdown,
}

/// Non-blocking local `mpv` JSON Unix-socket IPC playback controller.
pub struct PlayerController {
    snapshot: Arc<Mutex<PlayerSnapshot>>,
    cmd_tx: Sender<ActorCmd>,
}

impl PlayerController {
    #[must_use]
    pub fn new(ctx: Option<egui::Context>) -> Self {
        let snapshot = Arc::new(Mutex::new(PlayerSnapshot::default()));
        let (cmd_tx, cmd_rx) = mpsc::channel::<ActorCmd>();
        let actor_snap = Arc::clone(&snapshot);
        thread::spawn(move || run_mpv_actor(cmd_rx, actor_snap, ctx));
        Self { snapshot, cmd_tx }
    }

    #[must_use]
    pub fn snapshot(&self) -> PlayerSnapshot {
        self.snapshot.lock().expect("lock player snapshot").clone()
    }

    /// Queue track playback asynchronously (never blocks the UI thread).
    pub fn play_track(&self, track: &Track) {
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
        let _ = self.cmd_tx.send(ActorCmd::Play {
            video_id: track.video_id.clone(),
        });
    }

    pub fn toggle_pause(&self) {
        let paused = matches!(
            self.snapshot.lock().expect("lock").state,
            PlayerState::Paused { .. }
        );
        let _ = self.cmd_tx.send(ActorCmd::SetPause(!paused));
    }

    pub fn seek_absolute(&self, sec: f64) {
        let clamped = sec.max(0.0);
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.position_sec = clamped;
        }
        let _ = self.cmd_tx.send(ActorCmd::SeekAbsolute(clamped));
    }

    pub fn set_volume(&self, vol: f64) {
        let clamped = vol.clamp(0.0, 120.0);
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.volume = clamped;
        }
        let _ = self.cmd_tx.send(ActorCmd::SetVolume(clamped));
    }

    pub fn configure(&self, cookie_jar: Option<PathBuf>, quality: AudioQuality) {
        let _ = self.cmd_tx.send(ActorCmd::Configure {
            cookie_jar,
            quality,
        });
    }

    pub fn stop(&self) {
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.state = PlayerState::Stopped;
            snap.position_sec = 0.0;
        }
        let _ = self.cmd_tx.send(ActorCmd::Stop);
    }
}

impl Drop for PlayerController {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(ActorCmd::Shutdown);
    }
}

struct MpvProcess {
    child: Child,
    write_stream: UnixStream,
    socket_path: PathBuf,
}

impl MpvProcess {
    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn send(&mut self, value: &Value) -> bool {
        write_json_line(&mut self.write_stream, value).is_ok()
    }

    fn shutdown(&mut self) {
        let _ = self.send(&json!({ "command": ["quit"] }));
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn run_mpv_actor(
    cmd_rx: Receiver<ActorCmd>,
    snapshot: Arc<Mutex<PlayerSnapshot>>,
    ctx: Option<egui::Context>,
) {
    let pid = std::process::id();
    let socket_path = std::env::temp_dir().join(format!("ytm-architect-mpv-{pid}.sock"));
    let log_path = std::env::temp_dir().join("ytm-architect-mpv.log");
    let mut proc: Option<MpvProcess> = None;
    let mut cookie_jar: Option<PathBuf> = None;
    let mut quality = AudioQuality::default();

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            ActorCmd::Shutdown => break,
            ActorCmd::Configure {
                cookie_jar: next_jar,
                quality: next_quality,
            } => {
                if next_jar != cookie_jar || next_quality != quality {
                    cookie_jar = next_jar;
                    quality = next_quality;
                    if let Some(mut old) = proc.take() {
                        old.shutdown();
                    }
                }
            }
            ActorCmd::Play { video_id } => {
                if !proc.as_mut().is_some_and(MpvProcess::is_alive) {
                    if let Some(mut old) = proc.take() {
                        old.shutdown();
                    }
                    let vol = snapshot.lock().expect("lock").volume;
                    match spawn_mpv(
                        &socket_path,
                        &log_path,
                        vol,
                        cookie_jar.as_deref(),
                        quality,
                        &snapshot,
                        &ctx,
                    ) {
                        Ok(p) => proc = Some(p),
                        Err(err) => {
                            let mut snap = snapshot.lock().expect("lock");
                            snap.state = PlayerState::Error(err);
                            if let Some(c) = &ctx {
                                c.request_repaint();
                            }
                            continue;
                        }
                    }
                }
                if let Some(p) = proc.as_mut() {
                    let url = format!("https://www.youtube.com/watch?v={video_id}");
                    let ok = p.send(&json!({ "command": ["set_property", "pause", false] }))
                        && p.send(&json!({ "command": ["loadfile", url, "replace"] }));
                    if !ok {
                        p.shutdown();
                        proc = None;
                    }
                }
            }
            ActorCmd::SetPause(paused) => {
                if let Some(p) = proc.as_mut() {
                    let _ = p.send(&json!({ "command": ["set_property", "pause", paused] }));
                }
            }
            ActorCmd::SeekAbsolute(sec) => {
                if let Some(p) = proc.as_mut() {
                    let _ = p.send(&json!({ "command": ["seek", sec, "absolute"] }));
                }
            }
            ActorCmd::SetVolume(vol) => {
                if let Some(p) = proc.as_mut() {
                    let _ = p.send(&json!({ "command": ["set_property", "volume", vol] }));
                }
            }
            ActorCmd::Stop => {
                if let Some(p) = proc.as_mut() {
                    let _ = p.send(&json!({ "command": ["stop"] }));
                }
            }
        }
    }

    if let Some(mut p) = proc {
        p.shutdown();
    }
    if let Some(jar) = cookie_jar.as_deref() {
        remove_mpv_cookie_jar_at(jar);
    }
}

fn spawn_mpv(
    socket_path: &Path,
    log_path: &Path,
    initial_vol: f64,
    cookie_jar: Option<&Path>,
    quality: AudioQuality,
    snapshot: &Arc<Mutex<PlayerSnapshot>>,
    ctx: &Option<egui::Context>,
) -> Result<MpvProcess, String> {
    let _ = std::fs::remove_file(socket_path);
    let mpv_bin = if PathBuf::from("/usr/bin/mpv").exists() {
        "/usr/bin/mpv"
    } else {
        "mpv"
    };
    let mut cmd = Command::new(mpv_bin);
    cmd.arg("--no-video")
        .arg("--idle=yes")
        .arg("--really-quiet")
        .arg("--ytdl=yes")
        .arg(format!("--ytdl-format={}", quality.ytdl_format()));
    if let Some(jar) = cookie_jar {
        cmd.arg(
            "--ytdl-raw-options-append=extractor-args=youtube:player_client=web_music,tv,android",
        )
        .arg(format!(
            "--ytdl-raw-options-append=cookies={}",
            jar.display()
        ));
    } else {
        cmd.arg("--ytdl-raw-options-append=extractor-args=youtube:player_client=android");
    }
    let mut child = cmd
        .arg("--audio-display=no")
        .arg(format!("--volume={initial_vol:.0}"))
        .arg(format!("--log-file={}", log_path.display()))
        .arg(format!("--input-ipc-server={}", socket_path.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to launch mpv ({mpv_bin}): {e}"))?;

    let mut write_stream = match connect_with_backoff(socket_path, Duration::from_secs(4)) {
        Ok(s) => s,
        Err(err) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("mpv IPC connect failed: {err}"));
        }
    };
    let read_stream = write_stream
        .try_clone()
        .map_err(|e| format!("mpv IPC stream clone failed: {e}"))?;

    for init in [
        json!({ "command": ["observe_property", 1, "time-pos"] }),
        json!({ "command": ["observe_property", 2, "duration"] }),
        json!({ "command": ["observe_property", 3, "pause"] }),
        json!({ "command": ["observe_property", 4, "core-idle"] }),
    ] {
        let _ = write_json_line(&mut write_stream, &init);
    }

    let snap_r = Arc::clone(snapshot);
    let ctx_r = ctx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(read_stream);
        for line in reader.lines() {
            let Ok(raw) = line else {
                break;
            };
            if let Ok(val) = serde_json::from_str::<Value>(&raw)
                && apply_ipc_event(&val, &snap_r)
                && let Some(c) = &ctx_r
            {
                c.request_repaint();
            }
        }
    });

    Ok(MpvProcess {
        child,
        write_stream,
        socket_path: socket_path.to_path_buf(),
    })
}
