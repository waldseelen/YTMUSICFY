use std::collections::HashMap;

use ytm_core::{ArtistRef, MusicVideoType, Playlist, PlaylistPrivacy, Track, TrackAvailability};

use super::*;

fn mk_track(vid: &str, sid: &str, pos: usize) -> Track {
    Track {
        video_id: vid.to_string(),
        set_video_id: Some(sid.to_string()),
        position: pos,
        title: format!("Title {vid}"),
        artists: vec![ArtistRef {
            name: "Artist".to_string(),
            browse_id: None,
        }],
        album: None,
        duration_sec: Some(180),
        release_year: Some(2024),
        music_video_type: MusicVideoType::AudioTrack,
        availability: TrackAvailability::Available,
    }
}

#[test]
fn coalesces_reorders_and_recomputes_working_draft() {
    let pl = Playlist {
        id: "PL1".to_string(),
        title: "My Mix".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 3,
        tracks: vec![
            mk_track("V1", "S1", 0),
            mk_track("V2", "S2", 1),
            mk_track("V3", "S3", 2),
        ],
    };
    let mut cache = HashMap::new();
    cache.insert("PL1".to_string(), pl);

    let mut q = SyncQueue::default();
    let moves = q
        .stage_reorder(
            "PL1",
            "My Mix",
            vec!["S3".to_string(), "S1".to_string(), "S2".to_string()],
            &cache,
        )
        .expect("stage reorder");
    assert_eq!(moves, 1);
    assert_eq!(q.len(), 1);

    let draft = q.working_playlist("PL1", &cache).expect("draft");
    assert_eq!(
        draft
            .tracks
            .iter()
            .map(|t| t.video_id.as_str())
            .collect::<Vec<_>>(),
        vec!["V3", "V1", "V2"]
    );

    let noop = q
        .stage_reorder(
            "PL1",
            "My Mix",
            vec!["S1".to_string(), "S2".to_string(), "S3".to_string()],
            &cache,
        )
        .expect("noop reorder clears entry");
    assert_eq!(noop, 0);
    assert!(q.is_empty());
}
