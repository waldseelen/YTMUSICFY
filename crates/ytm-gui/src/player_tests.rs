use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

use serde_json::{Value, json};

use super::{ActorCmd, MpvRuntime, run_mpv_actor_with};
use crate::auth_store::AudioQuality;
use crate::player::{PlaybackErrorKind, PlayerSnapshot, PlayerState};

#[derive(Debug, Clone, PartialEq)]
enum RuntimeOp {
    Spawn(Option<PathBuf>, AudioQuality),
    Send(Value),
    Shutdown,
}

struct FakeMpvRuntime {
    alive: bool,
    fail_sends: bool,
    snapshot: Arc<Mutex<PlayerSnapshot>>,
    ops: Vec<RuntimeOp>,
    pending_on_send: Vec<Option<AudioQuality>>,
}

impl FakeMpvRuntime {
    fn new_alive(snapshot: Arc<Mutex<PlayerSnapshot>>, quality: AudioQuality) -> Self {
        Self {
            alive: true,
            fail_sends: false,
            snapshot,
            ops: vec![RuntimeOp::Spawn(None, quality)],
            pending_on_send: Vec::new(),
        }
    }

    fn new_stopped_failing_sends(snapshot: Arc<Mutex<PlayerSnapshot>>) -> Self {
        Self {
            alive: false,
            fail_sends: true,
            snapshot,
            ops: Vec::new(),
            pending_on_send: Vec::new(),
        }
    }
}

impl MpvRuntime for FakeMpvRuntime {
    fn is_alive(&mut self) -> bool {
        self.alive
    }

    fn spawn(
        &mut self,
        _vol: f64,
        cookie_jar: Option<&Path>,
        quality: AudioQuality,
    ) -> Result<(), PlaybackErrorKind> {
        self.alive = true;
        self.ops
            .push(RuntimeOp::Spawn(cookie_jar.map(Path::to_path_buf), quality));
        Ok(())
    }

    fn send(&mut self, value: &Value) -> bool {
        if self.alive && !self.fail_sends {
            let pending = self.snapshot.lock().unwrap().pending_configure;
            self.pending_on_send.push(pending);
            self.ops.push(RuntimeOp::Send(value.clone()));
            true
        } else {
            false
        }
    }

    fn shutdown(&mut self) {
        if self.alive {
            self.alive = false;
            self.ops.push(RuntimeOp::Shutdown);
        }
    }
}

#[test]
fn deferred_configure_applies_on_next_track_without_interrupting_playback() {
    let snapshot = Arc::new(Mutex::new(PlayerSnapshot {
        state: PlayerState::Playing {
            video_id: "V1".into(),
        },
        position_sec: 42.5,
        ..PlayerSnapshot::default()
    }));
    let mut runtime = FakeMpvRuntime::new_alive(Arc::clone(&snapshot), AudioQuality::AutoPremium);

    let (tx, rx) = mpsc::channel();
    tx.send(ActorCmd::Configure {
        cookie_jar: None,
        quality: AudioQuality::DataSaver,
    })
    .unwrap();
    tx.send(ActorCmd::Ipc(json!({ "marker": "while_deferred" })))
        .unwrap();
    tx.send(ActorCmd::Play {
        video_id: "V2".into(),
    })
    .unwrap();
    tx.send(ActorCmd::Shutdown).unwrap();
    run_mpv_actor_with(rx, Arc::clone(&snapshot), None, &mut runtime);

    assert_eq!(runtime.pending_on_send[0], Some(AudioQuality::DataSaver));
    assert_eq!(runtime.pending_on_send[1], None);
    assert_eq!(
        runtime.ops,
        vec![
            RuntimeOp::Spawn(None, AudioQuality::AutoPremium),
            RuntimeOp::Send(json!({ "marker": "while_deferred" })),
            RuntimeOp::Shutdown,
            RuntimeOp::Spawn(None, AudioQuality::DataSaver),
            RuntimeOp::Send(json!({ "command": ["set_property", "start", "none"] })),
            RuntimeOp::Send(json!({ "command": ["set_property", "pause", false] })),
            RuntimeOp::Send(json!({
                "command": ["loadfile", "https://www.youtube.com/watch?v=V2", "replace"]
            })),
            RuntimeOp::Shutdown,
        ]
    );
    let snap = snapshot.lock().unwrap();
    assert_eq!(snap.pending_configure, None);
    assert!((snap.position_sec - 42.5).abs() < f64::EPSILON);
}

#[test]
fn apply_now_restarts_mpv_immediately_and_resumes_at_saved_position() {
    let snapshot = Arc::new(Mutex::new(PlayerSnapshot {
        state: PlayerState::Playing {
            video_id: "V1".into(),
        },
        position_sec: 87.25,
        ..PlayerSnapshot::default()
    }));
    let mut runtime = FakeMpvRuntime::new_alive(Arc::clone(&snapshot), AudioQuality::AutoPremium);

    let (tx, rx) = mpsc::channel();
    tx.send(ActorCmd::Configure {
        cookie_jar: None,
        quality: AudioQuality::BalancedOpus,
    })
    .unwrap();
    tx.send(ActorCmd::ApplyPendingConfigure).unwrap();
    tx.send(ActorCmd::Shutdown).unwrap();
    run_mpv_actor_with(rx, Arc::clone(&snapshot), None, &mut runtime);

    assert_eq!(
        runtime.ops,
        vec![
            RuntimeOp::Spawn(None, AudioQuality::AutoPremium),
            RuntimeOp::Shutdown,
            RuntimeOp::Spawn(None, AudioQuality::BalancedOpus),
            RuntimeOp::Send(json!({ "command": ["set_property", "start", "87.250"] })),
            RuntimeOp::Send(json!({ "command": ["set_property", "pause", false] })),
            RuntimeOp::Send(json!({
                "command": ["loadfile", "https://www.youtube.com/watch?v=V1", "replace"]
            })),
            RuntimeOp::Shutdown,
        ]
    );
    let snap = snapshot.lock().unwrap();
    assert_eq!(snap.pending_configure, None);
    assert!((snap.position_sec - 87.25).abs() < f64::EPSILON);
}

#[test]
fn apply_now_while_paused_preserves_pause_state_and_position() {
    let snapshot = Arc::new(Mutex::new(PlayerSnapshot {
        state: PlayerState::Paused {
            video_id: "V1".into(),
        },
        position_sec: 33.5,
        ..PlayerSnapshot::default()
    }));
    let mut runtime = FakeMpvRuntime::new_alive(Arc::clone(&snapshot), AudioQuality::AutoPremium);

    let (tx, rx) = mpsc::channel();
    tx.send(ActorCmd::Configure {
        cookie_jar: None,
        quality: AudioQuality::DataSaver,
    })
    .unwrap();
    tx.send(ActorCmd::ApplyPendingConfigure).unwrap();
    tx.send(ActorCmd::Shutdown).unwrap();
    run_mpv_actor_with(rx, Arc::clone(&snapshot), None, &mut runtime);

    assert_eq!(
        runtime.ops,
        vec![
            RuntimeOp::Spawn(None, AudioQuality::AutoPremium),
            RuntimeOp::Shutdown,
            RuntimeOp::Spawn(None, AudioQuality::DataSaver),
            RuntimeOp::Send(json!({ "command": ["set_property", "start", "33.500"] })),
            RuntimeOp::Send(json!({ "command": ["set_property", "pause", true] })),
            RuntimeOp::Send(json!({
                "command": ["loadfile", "https://www.youtube.com/watch?v=V1", "replace"]
            })),
            RuntimeOp::Shutdown,
        ]
    );
}

#[test]
fn reverting_configure_to_active_values_clears_pending_without_restart() {
    let snapshot = Arc::new(Mutex::new(PlayerSnapshot {
        state: PlayerState::Playing {
            video_id: "V1".into(),
        },
        position_sec: 15.0,
        ..PlayerSnapshot::default()
    }));
    let mut runtime = FakeMpvRuntime::new_alive(Arc::clone(&snapshot), AudioQuality::AutoPremium);

    let (tx, rx) = mpsc::channel();
    tx.send(ActorCmd::Configure {
        cookie_jar: None,
        quality: AudioQuality::DataSaver,
    })
    .unwrap();
    tx.send(ActorCmd::Ipc(json!({ "marker": "after_change" })))
        .unwrap();
    tx.send(ActorCmd::Configure {
        cookie_jar: None,
        quality: AudioQuality::AutoPremium,
    })
    .unwrap();
    tx.send(ActorCmd::Ipc(json!({ "marker": "after_revert" })))
        .unwrap();
    tx.send(ActorCmd::Shutdown).unwrap();
    run_mpv_actor_with(rx, Arc::clone(&snapshot), None, &mut runtime);

    assert_eq!(
        runtime.pending_on_send,
        vec![Some(AudioQuality::DataSaver), None]
    );
    assert_eq!(
        runtime.ops,
        vec![
            RuntimeOp::Spawn(None, AudioQuality::AutoPremium),
            RuntimeOp::Send(json!({ "marker": "after_change" })),
            RuntimeOp::Send(json!({ "marker": "after_revert" })),
            RuntimeOp::Shutdown,
        ]
    );
    assert_eq!(snapshot.lock().unwrap().pending_configure, None);
}

#[test]
fn post_spawn_ipc_send_failure_sets_player_error_instead_of_loading() {
    let play_snap = Arc::new(Mutex::new(PlayerSnapshot {
        state: PlayerState::Loading {
            video_id: "V1".into(),
            title: "Song".into(),
        },
        ..PlayerSnapshot::default()
    }));
    let mut play_rt = FakeMpvRuntime::new_stopped_failing_sends(Arc::clone(&play_snap));
    let (tx, rx) = mpsc::channel();
    tx.send(ActorCmd::Play {
        video_id: "V1".into(),
    })
    .unwrap();
    tx.send(ActorCmd::Shutdown).unwrap();
    run_mpv_actor_with(rx, Arc::clone(&play_snap), None, &mut play_rt);
    assert_eq!(
        play_rt.ops,
        vec![
            RuntimeOp::Spawn(None, AudioQuality::AutoPremium),
            RuntimeOp::Shutdown,
        ]
    );
    {
        let locked = play_snap.lock().unwrap();
        assert_eq!(
            locked.state,
            PlayerState::Error("mpv IPC write failed".into())
        );
        assert_eq!(
            locked.last_error_kind,
            Some(PlaybackErrorKind::Other("mpv IPC write failed".into()))
        );
    }

    let apply_snap = Arc::new(Mutex::new(PlayerSnapshot {
        state: PlayerState::Playing {
            video_id: "V1".into(),
        },
        position_sec: 12.0,
        ..PlayerSnapshot::default()
    }));
    let mut apply_rt =
        FakeMpvRuntime::new_alive(Arc::clone(&apply_snap), AudioQuality::AutoPremium);
    apply_rt.fail_sends = true;
    let (tx2, rx2) = mpsc::channel();
    tx2.send(ActorCmd::Configure {
        cookie_jar: None,
        quality: AudioQuality::DataSaver,
    })
    .unwrap();
    tx2.send(ActorCmd::ApplyPendingConfigure).unwrap();
    tx2.send(ActorCmd::Shutdown).unwrap();
    run_mpv_actor_with(rx2, Arc::clone(&apply_snap), None, &mut apply_rt);
    let locked = apply_snap.lock().unwrap();
    assert_eq!(
        locked.state,
        PlayerState::Error("mpv IPC write failed".into())
    );
    assert_eq!(
        locked.last_error_kind,
        Some(PlaybackErrorKind::Other("mpv IPC write failed".into()))
    );
}
