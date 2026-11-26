use serde_json::json;

use super::*;
use crate::player::should_defer_configure;

fn apply_ipc_event(event: &Value, snapshot: &Arc<Mutex<PlayerSnapshot>>) -> bool {
    let mut last = None;
    apply_ipc_event_throttled(event, snapshot, &mut last, Instant::now(), None)
}

#[test]
fn ipc_events_update_position_pause_codec_bitrate_and_eof() {
    let snap = Arc::new(Mutex::new(PlayerSnapshot {
        state: PlayerState::Loading {
            video_id: "vid1".into(),
            title: "Song".into(),
        },
        ..PlayerSnapshot::default()
    }));
    let prop =
        |name: &str, data: Value| json!({ "event": "property-change", "name": name, "data": data });
    let t0 = Instant::now();
    let mut last = None;

    assert!(apply_ipc_event(
        &json!({ "event": "playback-restart" }),
        &snap
    ));
    assert!(apply_ipc_event_throttled(
        &prop("time-pos", json!(12.0)),
        &snap,
        &mut last,
        t0,
        None
    ));
    assert!(!apply_ipc_event_throttled(
        &prop("time-pos", json!(12.1)),
        &snap,
        &mut last,
        t0 + Duration::from_millis(50),
        None
    ));
    assert!(apply_ipc_event_throttled(
        &prop("time-pos", json!(12.3)),
        &snap,
        &mut last,
        t0 + Duration::from_millis(250),
        None
    ));
    assert!(apply_ipc_event(
        &prop("audio-codec-name", json!("opus")),
        &snap
    ));
    assert!(apply_ipc_event(
        &prop("audio-bitrate", json!(160_400.0)),
        &snap
    ));
    assert!(apply_ipc_event(&prop("pause", json!(true)), &snap));
    assert!((snap.lock().expect("lock").position_sec - 12.3).abs() < f64::EPSILON);

    assert!(apply_ipc_event(
        &json!({ "event": "end-file", "reason": "eof" }),
        &snap
    ));

    let locked = snap.lock().expect("lock");
    assert!(matches!(locked.state, PlayerState::Paused { .. }));
    assert_eq!(locked.codec_name.as_deref(), Some("opus"));
    assert_eq!(locked.bitrate_kbps, Some(160));
    assert_eq!(locked.ended_counter, 1);
    assert!(locked.position_sec.abs() < f64::EPSILON);
}

#[test]
fn classify_mpv_log_error_and_defer_policy() {
    assert!(!should_defer_configure(&PlayerState::Stopped));
    for s in [
        PlayerState::Playing {
            video_id: "v".into(),
        },
        PlayerState::Paused {
            video_id: "v".into(),
        },
    ] {
        assert!(should_defer_configure(&s));
    }
    for (line, kind) in [
        (
            "Sign in to confirm you're not a bot",
            PlaybackErrorKind::CookieExpired,
        ),
        ("geo-restricted", PlaybackErrorKind::GeoBlocked),
        ("age-restricted", PlaybackErrorKind::AgeRestricted),
        ("yt-dlp: not found", PlaybackErrorKind::MissingYtdlp),
        ("could not resolve host", PlaybackErrorKind::Network),
        ("unknown failure", PlaybackErrorKind::Other("f".into())),
    ] {
        assert_eq!(classify_mpv_log_error(line, "f"), kind);
    }
}

#[test]
fn playback_restart_after_pause_preserves_paused_state() {
    let snap = Arc::new(Mutex::new(PlayerSnapshot {
        state: PlayerState::Loading {
            video_id: "vid1".into(),
            title: "Song".into(),
        },
        ..PlayerSnapshot::default()
    }));
    let pause_evt = json!({ "event": "property-change", "name": "pause", "data": true });
    let resume_evt = json!({ "event": "property-change", "name": "pause", "data": false });
    let restart_evt = json!({ "event": "playback-restart" });

    assert!(apply_ipc_event(&pause_evt, &snap));
    assert_eq!(
        snap.lock().expect("lock").state,
        PlayerState::Paused {
            video_id: "vid1".into()
        }
    );

    assert!(!apply_ipc_event(&restart_evt, &snap));
    assert_eq!(
        snap.lock().expect("lock").state,
        PlayerState::Paused {
            video_id: "vid1".into()
        }
    );

    assert!(apply_ipc_event(&resume_evt, &snap));
    assert_eq!(
        snap.lock().expect("lock").state,
        PlayerState::Playing {
            video_id: "vid1".into()
        }
    );
}
