use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;
use serde_json::json;
use ytm_core::Track;

use crate::auth_store::AudioQuality;
pub use crate::player_ipc::PlaybackErrorKind;

#[path = "player_actor.rs"]
mod actor;

#[cfg(test)]
pub(crate) use actor::should_defer_configure;
use actor::{ActorCmd, run_mpv_actor};

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
    pub codec_name: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub last_error_kind: Option<PlaybackErrorKind>,
    pub pending_configure: Option<AudioQuality>,
    pub window_active: bool,
}

impl Default for PlayerSnapshot {
    fn default() -> Self {
        Self {
            state: PlayerState::Stopped,
            position_sec: 0.0,
            duration_sec: 0.0,
            volume: 85.0,
            ended_counter: 0,
            codec_name: None,
            bitrate_kbps: None,
            last_error_kind: None,
            pending_configure: None,
            window_active: true,
        }
    }
}

impl PlayerSnapshot {
    fn reset_media(&mut self) {
        self.position_sec = 0.0;
        self.codec_name = None;
        self.bitrate_kbps = None;
    }
}

/// Non-blocking local `mpv` JSON IPC playback controller (Unix socket / Windows Named Pipe).
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
        let dur = f64::from(track.duration_sec.unwrap_or(0));
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.state = PlayerState::Loading {
                video_id: track.video_id.clone(),
                title: track.title.clone(),
            };
            snap.duration_sec = dur;
            snap.last_error_kind = None;
            snap.reset_media();
        }
        let _ = self.cmd_tx.send(ActorCmd::Play {
            video_id: track.video_id.clone(),
        });
    }

    /// Pre-append the upcoming `video_id` into `mpv` for gapless transition.
    pub fn prefetch_next(&self, video_id: &str) {
        if !video_id.trim().is_empty() {
            let _ = self.cmd_tx.send(ActorCmd::PrefetchNext {
                video_id: video_id.to_string(),
            });
        }
    }

    pub fn toggle_pause(&self) {
        let paused = matches!(
            self.snapshot.lock().expect("lock").state,
            PlayerState::Paused { .. }
        );
        let _ = self.cmd_tx.send(ActorCmd::Ipc(
            json!({ "command": ["set_property", "pause", !paused] }),
        ));
    }

    pub fn seek_absolute(&self, sec: f64) {
        let clamped = sec.max(0.0);
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.position_sec = clamped;
        }
        let _ = self.cmd_tx.send(ActorCmd::Ipc(
            json!({ "command": ["seek", clamped, "absolute"] }),
        ));
    }

    pub fn set_volume(&self, vol: f64) {
        let clamped = vol.clamp(0.0, 120.0);
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.volume = clamped;
        }
        let _ = self.cmd_tx.send(ActorCmd::Ipc(
            json!({ "command": ["set_property", "volume", clamped] }),
        ));
    }

    pub fn configure(&self, cookie_jar: Option<PathBuf>, quality: AudioQuality) {
        let _ = self.cmd_tx.send(ActorCmd::Configure {
            cookie_jar,
            quality,
        });
    }

    /// Immediately apply a deferred `Configure` by restarting `mpv` and resuming at saved position.
    pub fn apply_pending_configure(&self) {
        let _ = self.cmd_tx.send(ActorCmd::ApplyPendingConfigure);
    }

    /// Set the mpv audio filter chain (EQ, scaletempo, etc.).
    pub fn set_audio_filters(&self, filters: String) {
        let _ = self.cmd_tx.send(ActorCmd::SetAudioFilters(filters));
    }

    /// Set playback speed (0.75x to 1.5x, pitch-corrected).
    pub fn set_playback_speed(&self, speed: f64) {
        let _ = self.cmd_tx.send(ActorCmd::SetPlaybackSpeed(speed));
    }

    /// Start sleep timer with given duration.
    pub fn set_sleep_timer(&self, duration: Duration) {
        let _ = self.cmd_tx.send(ActorCmd::SetSleepTimer(duration));
    }

    /// Cancel sleep timer.
    pub fn cancel_sleep_timer(&self) {
        let _ = self.cmd_tx.send(ActorCmd::CancelSleepTimer);
    }

    pub fn stop(&self) {
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.state = PlayerState::Stopped;
            snap.reset_media();
        }
        let _ = self.cmd_tx.send(ActorCmd::Stop);
    }

    /// Track whether the window is focused/visible so background `time-pos` repaints throttle down.
    pub fn set_window_active(&self, active: bool) {
        if let Ok(mut snap) = self.snapshot.lock() {
            snap.window_active = active;
        }
    }
}

impl Drop for PlayerController {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(ActorCmd::Shutdown);
    }
}
