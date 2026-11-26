use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use eframe::egui;
use serde_json::{Value, json};

use crate::auth_store::{AudioQuality, remove_mpv_cookie_jar_at};
use crate::player::{PlaybackErrorKind, PlayerSnapshot, PlayerState};

#[path = "player_process.rs"]
mod process;

use process::RealMpvRuntime;

pub(crate) enum ActorCmd {
    Configure {
        cookie_jar: Option<PathBuf>,
        quality: AudioQuality,
    },
    ApplyPendingConfigure,
    Play {
        video_id: String,
    },
    PrefetchNext {
        video_id: String,
    },
    Ipc(Value),
    Stop,
    Shutdown,
}

pub(crate) trait MpvRuntime {
    fn is_alive(&mut self) -> bool;
    fn spawn(
        &mut self,
        vol: f64,
        cookie_jar: Option<&Path>,
        quality: AudioQuality,
    ) -> Result<(), PlaybackErrorKind>;
    fn send(&mut self, value: &Value) -> bool;
    fn shutdown(&mut self);
}

pub(crate) fn should_defer_configure(state: &PlayerState) -> bool {
    state.is_active()
}

fn watch_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

fn request_repaint(ctx: Option<&egui::Context>) {
    if let Some(c) = ctx {
        c.request_repaint();
    }
}

fn set_spawn_error(
    snapshot: &Arc<Mutex<PlayerSnapshot>>,
    ctx: Option<&egui::Context>,
    kind: PlaybackErrorKind,
) {
    if let Ok(mut snap) = snapshot.lock() {
        snap.state = PlayerState::Error(kind.user_message());
        snap.last_error_kind = Some(kind);
    }
    request_repaint(ctx);
}

fn apply_pending<R: MpvRuntime>(
    pending: &mut Option<(Option<PathBuf>, AudioQuality)>,
    cookie_jar: &mut Option<PathBuf>,
    quality: &mut AudioQuality,
    runtime: &mut R,
    prefetched_video_id: &mut Option<String>,
    snapshot: &Arc<Mutex<PlayerSnapshot>>,
) {
    if let Some((jar, qual)) = pending.take() {
        *cookie_jar = jar;
        *quality = qual;
        *prefetched_video_id = None;
        if let Ok(mut snap) = snapshot.lock() {
            snap.pending_configure = None;
        }
        runtime.shutdown();
    }
}

pub(crate) fn run_mpv_actor(
    cmd_rx: Receiver<ActorCmd>,
    snapshot: Arc<Mutex<PlayerSnapshot>>,
    ctx: Option<egui::Context>,
) {
    let mut runtime = RealMpvRuntime::new(Arc::clone(&snapshot), ctx.clone());
    run_mpv_actor_with(cmd_rx, snapshot, ctx, &mut runtime);
}

pub(crate) fn run_mpv_actor_with<R: MpvRuntime>(
    cmd_rx: Receiver<ActorCmd>,
    snapshot: Arc<Mutex<PlayerSnapshot>>,
    ctx: Option<egui::Context>,
    runtime: &mut R,
) {
    let mut cookie_jar: Option<PathBuf> = None;
    let mut quality = AudioQuality::default();
    let mut pending: Option<(Option<PathBuf>, AudioQuality)> = None;
    let mut prefetched_video_id: Option<String> = None;

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            ActorCmd::Shutdown => break,
            ActorCmd::Configure {
                cookie_jar: nj,
                quality: nq,
            } => {
                if nj == cookie_jar && nq == quality {
                    if pending.take().is_some() {
                        if let Ok(mut snap) = snapshot.lock() {
                            snap.pending_configure = None;
                        }
                        request_repaint(ctx.as_ref());
                    }
                    continue;
                }
                pending = Some((nj, nq));
                let active = snapshot
                    .lock()
                    .is_ok_and(|s| should_defer_configure(&s.state));
                if !active || !runtime.is_alive() {
                    apply_pending(
                        &mut pending,
                        &mut cookie_jar,
                        &mut quality,
                        runtime,
                        &mut prefetched_video_id,
                        &snapshot,
                    );
                } else if let Ok(mut snap) = snapshot.lock() {
                    snap.pending_configure = Some(nq);
                    drop(snap);
                    request_repaint(ctx.as_ref());
                }
            }
            ActorCmd::ApplyPendingConfigure => {
                if pending.is_none() {
                    continue;
                }
                let (active_vid, saved_pos, was_paused, prev_title, vol) = {
                    let snap = snapshot.lock().expect("lock player snapshot");
                    let active_vid = snap.state.video_id().map(str::to_string);
                    let prev_title = match &snap.state {
                        PlayerState::Loading { title, .. } => title.clone(),
                        _ => active_vid.clone().unwrap_or_default(),
                    };
                    (
                        active_vid,
                        snap.position_sec,
                        matches!(snap.state, PlayerState::Paused { .. }),
                        prev_title,
                        snap.volume,
                    )
                };
                apply_pending(
                    &mut pending,
                    &mut cookie_jar,
                    &mut quality,
                    runtime,
                    &mut prefetched_video_id,
                    &snapshot,
                );
                if let Some(vid) = active_vid {
                    if let Ok(mut snap) = snapshot.lock() {
                        snap.state = PlayerState::Loading {
                            video_id: vid.clone(),
                            title: prev_title,
                        };
                        snap.last_error_kind = None;
                        snap.position_sec = saved_pos;
                    }
                    match runtime.spawn(vol, cookie_jar.as_deref(), quality) {
                        Ok(()) => {
                            let start_ok = if saved_pos > 0.0 {
                                runtime.send(&json!({
                                    "command": ["set_property", "start", format!("{saved_pos:.3}")]
                                }))
                            } else {
                                true
                            };
                            let ok = start_ok
                                && runtime.send(
                                    &json!({ "command": ["set_property", "pause", was_paused] }),
                                )
                                && runtime.send(
                                    &json!({ "command": ["loadfile", watch_url(&vid), "replace"] }),
                                );
                            if !ok {
                                runtime.shutdown();
                                set_spawn_error(
                                    &snapshot,
                                    ctx.as_ref(),
                                    PlaybackErrorKind::Other("mpv IPC write failed".into()),
                                );
                            }
                        }
                        Err(kind) => set_spawn_error(&snapshot, ctx.as_ref(), kind),
                    }
                }
                request_repaint(ctx.as_ref());
            }
            ActorCmd::Play { video_id } => {
                apply_pending(
                    &mut pending,
                    &mut cookie_jar,
                    &mut quality,
                    runtime,
                    &mut prefetched_video_id,
                    &snapshot,
                );
                if !runtime.is_alive() {
                    runtime.shutdown();
                    prefetched_video_id = None;
                    let vol = snapshot.lock().expect("lock").volume;
                    if let Err(kind) = runtime.spawn(vol, cookie_jar.as_deref(), quality) {
                        set_spawn_error(&snapshot, ctx.as_ref(), kind);
                        continue;
                    }
                }
                prefetched_video_id = None;
                let ok = runtime.send(&json!({ "command": ["set_property", "start", "none"] }))
                    && runtime.send(&json!({ "command": ["set_property", "pause", false] }))
                    && runtime
                        .send(&json!({ "command": ["loadfile", watch_url(&video_id), "replace"] }));
                if !ok {
                    runtime.shutdown();
                    set_spawn_error(
                        &snapshot,
                        ctx.as_ref(),
                        PlaybackErrorKind::Other("mpv IPC write failed".into()),
                    );
                }
            }
            ActorCmd::PrefetchNext { video_id } => {
                if runtime.is_alive()
                    && prefetched_video_id.as_deref() != Some(video_id.as_str())
                    && runtime.send(&json!({ "command": ["set_property", "start", "none"] }))
                    && runtime
                        .send(&json!({ "command": ["loadfile", watch_url(&video_id), "append"] }))
                {
                    prefetched_video_id = Some(video_id);
                }
            }
            ActorCmd::Ipc(val) => {
                let _ = runtime.send(&val);
            }
            ActorCmd::Stop => {
                prefetched_video_id = None;
                let _ = runtime.send(&json!({ "command": ["stop"] }));
                apply_pending(
                    &mut pending,
                    &mut cookie_jar,
                    &mut quality,
                    runtime,
                    &mut prefetched_video_id,
                    &snapshot,
                );
            }
        }
    }

    runtime.shutdown();
    if let Some(jar) = cookie_jar.as_deref() {
        remove_mpv_cookie_jar_at(jar);
    }
}

#[cfg(test)]
#[path = "player_tests.rs"]
mod tests;
