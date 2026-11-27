use super::*;

#[test]
fn mutation_action_serializes_exact_innertube_wire_shapes() {
    let add = MutationAction::AddVideo {
        added_video_id: Some("vid123".to_string()),
        dedupe_option: Some("DEDUPE_OPTION_SKIP".to_string()),
    };
    let add_json = serde_json::to_value(&add).expect("serialize add");
    assert_eq!(add_json["action"], "ACTION_ADD_VIDEO");
    assert_eq!(add_json["addedVideoId"], "vid123");
    assert_eq!(add_json["dedupeOption"], "DEDUPE_OPTION_SKIP");

    let remove = MutationAction::RemoveVideo {
        set_video_id: "SET_1".to_string(),
        removed_video_id: "vid123".to_string(),
    };
    let rem_json = serde_json::to_value(&remove).expect("serialize remove");
    assert_eq!(rem_json["action"], "ACTION_REMOVE_VIDEO");
    assert_eq!(rem_json["setVideoId"], "SET_1");
    assert_eq!(rem_json["removedVideoId"], "vid123");

    let move_end = MutationAction::from(MoveAction {
        set_video_id: "SET_1".to_string(),
        moved_set_video_successor: None,
    });
    let move_end_json = serde_json::to_value(&move_end).expect("serialize move_end");
    assert_eq!(move_end_json["action"], "ACTION_MOVE_VIDEO_BEFORE");
    assert_eq!(move_end_json["setVideoId"], "SET_1");
    assert!(move_end_json.get("movedSetVideoIdSuccessor").is_none());

    let move_before = MutationAction::from(MoveAction {
        set_video_id: "SET_1".to_string(),
        moved_set_video_successor: Some("SET_2".to_string()),
    });
    let move_before_json = serde_json::to_value(&move_before).expect("serialize move_before");
    assert_eq!(move_before_json["movedSetVideoIdSuccessor"], "SET_2");
}

#[test]
fn music_video_type_parses_innertube_enums() {
    assert_eq!(
        MusicVideoType::from_innertube_str("MUSIC_VIDEO_TYPE_ATV"),
        MusicVideoType::AudioTrack
    );
    assert_eq!(
        MusicVideoType::from_innertube_str("MUSIC_VIDEO_TYPE_UGC"),
        MusicVideoType::UserGenerated
    );
    assert_eq!(
        MusicVideoType::from_innertube_str("UNRECOGNIZED"),
        MusicVideoType::Unknown
    );
}

#[test]
fn client_config_normalizes_lowercase_and_invalid_gl_hl() {
    let cfg = ClientConfig {
        gl: " tr ".to_string(),
        hl: " tr ".to_string(),
        ..ClientConfig::default()
    };
    assert_eq!(cfg.normalized_gl(), "TR");
    assert_eq!(cfg.normalized_hl(), "tr");

    let empty = ClientConfig {
        gl: "1".to_string(),
        hl: " ".to_string(),
        ..ClientConfig::default()
    };
    assert_eq!(empty.normalized_gl(), "US");
    assert_eq!(empty.normalized_hl(), "en");
}
