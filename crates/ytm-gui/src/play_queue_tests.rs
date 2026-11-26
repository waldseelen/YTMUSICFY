use super::*;

fn track(id: &str) -> Track {
    let s = format!(
        r#"{{"video_id":"{id}","set_video_id":null,"position":0,"title":"T {id}","artists":[],"album":null,"duration_sec":180,"release_year":null,"music_video_type":"AudioTrack","availability":"Available"}}"#
    );
    serde_json::from_str(&s).expect("valid track json")
}

#[test]
fn queue_supports_play_next_repeat_shuffle_and_move() {
    let mut q = PlayQueue::default();
    q.start_with(vec![track("a"), track("b"), track("c")], 0, 42);
    assert!(!q.play_next(track("next1")));
    let ids: Vec<&str> = q.tracks().iter().map(|t| t.video_id.as_str()).collect();
    assert_eq!(ids, vec!["a", "next1", "b", "c"]);
    q.move_item(2, 0);
    assert_eq!(q.current_index(), Some(1));
    q.cycle_repeat();
    assert_eq!(q.jump_to(3).map(|t| t.video_id.as_str()), Some("c"));
    assert_eq!(q.advance().map(|t| t.video_id.as_str()), Some("b"));
    q.toggle_shuffle(99);
    assert_eq!(q.current_track().map(|t| t.video_id.as_str()), Some("b"));
    q.toggle_shuffle(99);
    assert_eq!(q.current_track().map(|t| t.video_id.as_str()), Some("b"));
}

#[test]
fn snapshot_roundtrip_and_restore_preserve_queue_and_clamps_index() {
    let mut q = PlayQueue::default();
    q.start_with(vec![track("a"), track("b")], 1, 7);
    q.cycle_repeat();
    let snap = q.to_snapshot(72.5);
    let parsed: PlayQueueSnapshot =
        serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap();
    assert_eq!(
        (
            parsed.current_index,
            parsed.repeat,
            parsed.recently_played.len()
        ),
        (Some(1), RepeatMode::All, 1)
    );
    assert!((parsed.volume - 72.5).abs() < f64::EPSILON);

    let mut restored = PlayQueue::default();
    restored.restore_from_snapshot(&PlayQueueSnapshot {
        current_index: Some(99),
        ..parsed
    });
    assert_eq!(
        (restored.current_index(), restored.repeat_mode()),
        (Some(1), RepeatMode::All)
    );
    assert_eq!(restored.recently_played()[0].video_id, "b");
}
