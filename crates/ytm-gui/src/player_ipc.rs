use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::player::{PlayerSnapshot, PlayerState};

/// Apply an `mpv` JSON IPC event line to `snapshot`; returns `true` when UI should repaint.
pub(crate) fn apply_ipc_event(event: &Value, snapshot: &Arc<Mutex<PlayerSnapshot>>) -> bool {
    let kind = event.get("event").and_then(Value::as_str).unwrap_or("");
    let mut snap = snapshot.lock().expect("lock player snapshot");
    match kind {
        "property-change" => apply_property_change(event, &mut snap),
        "playback-restart" => {
            if let Some(vid) = snap.state.video_id().map(str::to_string) {
                snap.state = PlayerState::Playing { video_id: vid };
                return true;
            }
            false
        }
        "end-file" => {
            let reason = event.get("reason").and_then(Value::as_str).unwrap_or("");
            if reason == "eof" {
                snap.ended_counter = snap.ended_counter.saturating_add(1);
                snap.position_sec = 0.0;
                return true;
            } else if reason == "error" {
                let err_txt = event
                    .get("file_error")
                    .and_then(Value::as_str)
                    .unwrap_or("Stream playback failed");
                snap.state = PlayerState::Error(err_txt.to_string());
                return true;
            }
            false
        }
        _ => false,
    }
}

fn apply_property_change(event: &Value, snap: &mut PlayerSnapshot) -> bool {
    let name = event.get("name").and_then(Value::as_str).unwrap_or("");
    let Some(data) = event.get("data") else {
        return false;
    };
    match name {
        "time-pos" => {
            if let Some(pos) = data.as_f64() {
                snap.position_sec = pos.max(0.0);
                return true;
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
        _ => {}
    }
    false
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn ipc_events_update_position_pause_and_eof_counter() {
        let snap = Arc::new(Mutex::new(PlayerSnapshot {
            state: PlayerState::Loading {
                video_id: "vid1".to_string(),
                title: "Song".to_string(),
            },
            ..PlayerSnapshot::default()
        }));

        assert!(apply_ipc_event(
            &json!({ "event": "playback-restart" }),
            &snap
        ));
        assert!(matches!(
            snap.lock().expect("lock").state,
            PlayerState::Playing { .. }
        ));

        assert!(apply_ipc_event(
            &json!({ "event": "property-change", "name": "time-pos", "data": 42.5 }),
            &snap
        ));
        assert!(apply_ipc_event(
            &json!({ "event": "property-change", "name": "pause", "data": true }),
            &snap
        ));
        assert!(matches!(
            snap.lock().expect("lock").state,
            PlayerState::Paused { .. }
        ));
        assert!(apply_ipc_event(
            &json!({ "event": "end-file", "reason": "eof" }),
            &snap
        ));
        assert_eq!(snap.lock().expect("lock").ended_counter, 1);
    }
}
