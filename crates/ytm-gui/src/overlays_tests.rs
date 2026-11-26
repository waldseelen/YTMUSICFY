use ytm_core::{MusicVideoType, PlaylistPrivacy, TrackAvailability};

use super::*;

fn mk_playlist(id: &str, tracks: Vec<Track>) -> Playlist {
    Playlist {
        id: id.to_string(),
        title: id.to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: tracks.len(),
        tracks,
    }
}

#[test]
fn stage_move_tracks_creates_single_entry_and_single_undo_reverts_both() {
    let track = Track {
        video_id: "V1".to_string(),
        set_video_id: Some("S1".to_string()),
        position: 0,
        title: "Song".to_string(),
        artists: Vec::new(),
        album: None,
        duration_sec: Some(120),
        release_year: None,
        music_video_type: MusicVideoType::AudioTrack,
        availability: TrackAvailability::Available,
        thumbnail_url: None,
    };
    let mut cache = HashMap::new();
    cache.insert("SRC".to_string(), mk_playlist("SRC", vec![track.clone()]));
    cache.insert("DST".to_string(), mk_playlist("DST", Vec::new()));

    let mut q = SyncQueue::default();
    let mut toast = None;
    stage_move_tracks(&mut q, "SRC", "Src", "DST", "Dst", vec![track], &mut toast);

    assert_eq!(q.len(), 1);
    assert!(matches!(q.entries()[0].op, StagedOp::MoveTracks { .. }));
    assert!(q.working_playlist("SRC", &cache).unwrap().tracks.is_empty());
    assert_eq!(q.working_playlist("DST", &cache).unwrap().tracks.len(), 1);

    let eid = q.last_entry_id().expect("entry id");
    assert!(q.remove_entry(eid));
    assert_eq!(q.working_playlist("SRC", &cache).unwrap().tracks.len(), 1);
    assert!(q.working_playlist("DST", &cache).unwrap().tracks.is_empty());
}
