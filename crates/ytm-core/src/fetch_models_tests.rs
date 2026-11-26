use chrono::TimeZone;

use super::*;
use crate::models::{
    MusicVideoType, PlaylistAccess, PlaylistPrivacy, PlaylistSummary, TrackAvailability,
};

#[test]
fn playlist_access_serde_and_derivation_cover_all_tiers() {
    for tier in [
        PlaylistAccess::EditableOwned,
        PlaylistAccess::LikedMusicAuto,
        PlaylistAccess::ReadOnlyExternal,
    ] {
        let json = serde_json::to_string(&tier).expect("serialize PlaylistAccess");
        let parsed: PlaylistAccess =
            serde_json::from_str(&json).expect("deserialize PlaylistAccess");
        assert_eq!(parsed, tier);
    }

    let summary_lm = PlaylistSummary {
        id: "LM".to_string(),
        title: "Liked Music".to_string(),
        track_count: Some(120),
        owned: true,
        is_liked_music: true,
    };
    assert_eq!(summary_lm.access(), PlaylistAccess::LikedMusicAuto);

    let summary_owned = PlaylistSummary {
        id: "PL_OWNED".to_string(),
        title: "My Mix".to_string(),
        track_count: Some(10),
        owned: true,
        is_liked_music: false,
    };
    assert_eq!(summary_owned.access(), PlaylistAccess::EditableOwned);

    let summary_ext = PlaylistSummary {
        id: "PL_EXT".to_string(),
        title: "Curated".to_string(),
        track_count: Some(50),
        owned: false,
        is_liked_music: false,
    };
    assert_eq!(summary_ext.access(), PlaylistAccess::ReadOnlyExternal);

    let playlist_vllm = Playlist {
        id: "VLLM".to_string(),
        title: "Liked Music".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: false,
        track_count: 0,
        tracks: Vec::new(),
    };
    assert_eq!(playlist_vllm.access(), PlaylistAccess::LikedMusicAuto);

    let playlist_owned = Playlist {
        id: "PL_OWNED".to_string(),
        owned: true,
        ..playlist_vllm.clone()
    };
    assert_eq!(playlist_owned.access(), PlaylistAccess::EditableOwned);

    let playlist_ext = Playlist {
        id: "PL_EXT".to_string(),
        owned: false,
        ..playlist_vllm
    };
    assert_eq!(playlist_ext.access(), PlaylistAccess::ReadOnlyExternal);
}

#[test]
fn fetch_completeness_and_manifest_roundtrip_json() {
    let variants = [
        (FetchCompleteness::Complete, false),
        (
            FetchCompleteness::PartialHidden {
                declared: 247,
                fetched: 243,
                hidden_estimate: 4,
            },
            false,
        ),
        (
            FetchCompleteness::Truncated {
                pages_fetched: 500,
                max_pages: 500,
            },
            true,
        ),
        (
            FetchCompleteness::Degraded {
                dropped_renderers: vec!["unknownMusicRenderer".to_string()],
            },
            true,
        ),
    ];

    let fetched_at = Utc.with_ymd_and_hms(2026, 11, 26, 12, 0, 0).unwrap();
    for (completeness, expected_lock) in variants {
        assert_eq!(completeness.is_mutation_locked(), expected_lock);
        let c_json = serde_json::to_string(&completeness).expect("serialize completeness");
        let c_back: FetchCompleteness =
            serde_json::from_str(&c_json).expect("deserialize completeness");
        assert_eq!(c_back, completeness);

        let manifest = FetchManifest {
            declared_track_count: Some(247),
            fetched_tracks: 243,
            available_tracks: 240,
            unavailable_tracks: 3,
            missing_set_video_id: 1,
            pages_fetched: 3,
            completeness,
            fetched_at,
        };
        let m_json = serde_json::to_string(&manifest).expect("serialize FetchManifest");
        let m_back: FetchManifest =
            serde_json::from_str(&m_json).expect("deserialize FetchManifest");
        assert_eq!(m_back, manifest);
    }
}

#[test]
fn mutation_batch_report_roundtrip_and_helpers() {
    let report = MutationBatchReport {
        total_chunks: 3,
        outcomes: vec![
            ChunkOutcome::Succeeded {
                chunk_index: 0,
                action_count: 50,
                result: EditPlaylistResult {
                    status: "STATUS_SUCCEEDED".to_string(),
                    added_items: vec![AddedPlaylistItemResult {
                        video_id: "vid1".to_string(),
                        set_video_id: "set1".to_string(),
                    }],
                },
            },
            ChunkOutcome::Failed {
                chunk_index: 1,
                action_count: 50,
                error: "HTTP 503".to_string(),
            },
        ],
    };

    let json = serde_json::to_string(&report).expect("serialize MutationBatchReport");
    let parsed: MutationBatchReport =
        serde_json::from_str(&json).expect("deserialize MutationBatchReport");
    assert_eq!(parsed, report);
    assert_eq!(parsed.succeeded_chunks(), 1);
    assert_eq!(parsed.failed_chunks(), 1);
    assert!(!parsed.is_complete_success());
    assert_eq!(parsed.added_items().len(), 1);
    assert_eq!(parsed.first_error(), Some("HTTP 503"));
}

#[test]
fn legacy_playlist_and_track_json_deserialize_without_new_fields() {
    let legacy_playlist_json = r#"{
        "id": "PLLegacy01",
        "title": "Legacy Snapshot",
        "description": "Saved before Phase 0",
        "privacy": "PRIVATE",
        "owned": true,
        "track_count": 1,
        "tracks": [
            {
                "video_id": "vid_legacy",
                "set_video_id": "set_legacy",
                "position": 0,
                "title": "Old Song",
                "artists": [{ "name": "Artist", "browse_id": null }],
                "album": null,
                "duration_sec": 210,
                "release_year": 2020,
                "music_video_type": "AudioTrack",
                "availability": "Available"
            }
        ]
    }"#;

    let playlist: Playlist =
        serde_json::from_str(legacy_playlist_json).expect("deserialize legacy Playlist");
    assert_eq!(playlist.access(), PlaylistAccess::EditableOwned);
    assert_eq!(playlist.tracks.len(), 1);
    assert_eq!(playlist.tracks[0].thumbnail_url, None);
    assert_eq!(
        playlist.tracks[0].music_video_type,
        MusicVideoType::AudioTrack
    );
    assert_eq!(
        playlist.tracks[0].availability,
        TrackAvailability::Available
    );

    let legacy_summary_json = r#"{
        "id": "LM",
        "title": "Liked Music",
        "track_count": 42,
        "owned": true,
        "is_liked_music": true
    }"#;
    let summary: PlaylistSummary =
        serde_json::from_str(legacy_summary_json).expect("deserialize legacy PlaylistSummary");
    assert_eq!(summary.access(), PlaylistAccess::LikedMusicAuto);
}
