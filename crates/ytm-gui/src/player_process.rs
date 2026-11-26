use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use serde_json::{Value, json};

use crate::auth_store::AudioQuality;
use crate::player::{PlaybackErrorKind, PlayerSnapshot};
use crate::player_ipc::{
    IpcStream, classify_mpv_log_error, cleanup_ipc_endpoint, connect_with_backoff,
    default_ipc_endpoint, read_mpv_log_tail, resolve_mpv_binary, spawn_ipc_reader, write_json_line,
};

use super::MpvRuntime;

struct MpvProcess {
    child: Child,
    write_stream: IpcStream,
    ipc_endpoint: String,
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
        cleanup_ipc_endpoint(&self.ipc_endpoint);
    }
}

pub(super) struct RealMpvRuntime {
    ipc_endpoint: String,
    log_path: PathBuf,
    proc: Option<MpvProcess>,
    snapshot: Arc<Mutex<PlayerSnapshot>>,
    ctx: Option<egui::Context>,
}

impl RealMpvRuntime {
    pub(super) fn new(snapshot: Arc<Mutex<PlayerSnapshot>>, ctx: Option<egui::Context>) -> Self {
        Self {
            ipc_endpoint: default_ipc_endpoint(std::process::id()),
            log_path: std::env::temp_dir().join("ytm-architect-mpv.log"),
            proc: None,
            snapshot,
            ctx,
        }
    }
}

impl MpvRuntime for RealMpvRuntime {
    fn is_alive(&mut self) -> bool {
        self.proc.as_mut().is_some_and(MpvProcess::is_alive)
    }

    fn spawn(
        &mut self,
        vol: f64,
        cookie_jar: Option<&Path>,
        quality: AudioQuality,
    ) -> Result<(), PlaybackErrorKind> {
        match spawn_mpv(
            &self.ipc_endpoint,
            &self.log_path,
            vol,
            cookie_jar,
            quality,
            &self.snapshot,
            &self.ctx,
        ) {
            Ok(p) => {
                self.proc = Some(p);
                Ok(())
            }
            Err(err) => Err(classify_mpv_log_error(
                &read_mpv_log_tail(&self.log_path),
                &err,
            )),
        }
    }

    fn send(&mut self, value: &Value) -> bool {
        self.proc.as_mut().is_some_and(|p| p.send(value))
    }

    fn shutdown(&mut self) {
        if let Some(mut old) = self.proc.take() {
            old.shutdown();
        }
    }
}

fn spawn_mpv(
    ipc_endpoint: &str,
    log_path: &Path,
    initial_vol: f64,
    cookie_jar: Option<&Path>,
    quality: AudioQuality,
    snapshot: &Arc<Mutex<PlayerSnapshot>>,
    ctx: &Option<egui::Context>,
) -> Result<MpvProcess, String> {
    cleanup_ipc_endpoint(ipc_endpoint);
    let mpv_bin = resolve_mpv_binary();
    let mut args = vec![
        "--no-video".into(),
        "--idle=yes".into(),
        "--really-quiet".into(),
        "--ytdl=yes".into(),
        "--audio-display=no".into(),
        format!("--ytdl-format={}", quality.ytdl_format()),
        format!("--volume={initial_vol:.0}"),
        format!("--log-file={}", log_path.display()),
        format!("--input-ipc-server={ipc_endpoint}"),
    ];
    match cookie_jar {
        Some(jar) => {
            args.push("--ytdl-raw-options-append=extractor-args=youtube:player_client=web_music,tv,android".into());
            args.push(format!(
                "--ytdl-raw-options-append=cookies={}",
                jar.display()
            ));
        }
        None => args
            .push("--ytdl-raw-options-append=extractor-args=youtube:player_client=android".into()),
    }
    let mut child = Command::new(mpv_bin)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to launch mpv ({mpv_bin}): {e}"))?;

    let mut write_stream =
        connect_with_backoff(ipc_endpoint, Duration::from_secs(4)).map_err(|err| {
            let _ = child.kill();
            let _ = child.wait();
            format!("mpv IPC connect failed: {err}")
        })?;
    let read_stream = write_stream
        .try_clone()
        .map_err(|e| format!("mpv IPC clone failed: {e}"))?;
    let props = [
        "time-pos",
        "duration",
        "pause",
        "core-idle",
        "audio-codec-name",
        "audio-bitrate",
    ];
    for (idx, prop) in props.iter().enumerate() {
        let _ = write_json_line(
            &mut write_stream,
            &json!({ "command": ["observe_property", idx + 1, prop] }),
        );
    }
    spawn_ipc_reader(
        read_stream,
        Arc::clone(snapshot),
        ctx.clone(),
        log_path.to_path_buf(),
    );
    Ok(MpvProcess {
        child,
        write_stream,
        ipc_endpoint: ipc_endpoint.to_string(),
    })
}
