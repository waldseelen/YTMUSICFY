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
        thumbnail_url: None,
    }
}

fn mk_playlist(id: &str, title: &str, tracks: Vec<Track>) -> Playlist {
    let track_count = tracks.len();
    Playlist {
        id: id.to_string(),
        title: title.to_string(),
        description: Some("Original desc".to_string()),
        privacy: PlaylistPrivacy::Unlisted,
        owned: true,
        track_count,
        tracks,
    }
}

#[test]
fn coalesces_reorders_and_recomputes_working_draft() {
    let pl = mk_playlist(
        "PL1",
        "My Mix",
        vec![
            mk_track("V1", "S1", 0),
            mk_track("V2", "S2", 1),
            mk_track("V3", "S3", 2),
        ],
    );
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

#[test]
fn move_entry_and_last_entry_id_work_within_bounds() {
    let mut q = SyncQueue::default();
    let id1 = q.push(StagedOp::CreatePlaylist {
        title: "First".to_string(),
        description: String::new(),
        privacy: PlaylistPrivacy::Private,
        tracks: Vec::new(),
    });
    let id2 = q.push(StagedOp::CreatePlaylist {
        title: "Second".to_string(),
        description: String::new(),
        privacy: PlaylistPrivacy::Private,
        tracks: Vec::new(),
    });
    assert_eq!(q.last_entry_id(), Some(id2));
    assert!(q.move_entry(id1, -1).is_err());
    assert!(q.move_entry(id2, -1).is_ok());
    assert_eq!(q.entries()[0].id, id2);
    assert!(q.remove_entry(id2));
    assert_eq!(q.last_entry_id(), Some(id1));
}

#[test]
fn atomic_move_tracks_projects_both_playlists_and_single_undo_reverts() {
    let src = mk_playlist(
        "SRC",
        "Source",
        vec![mk_track("V1", "S1", 0), mk_track("V2", "S2", 1)],
    );
    let dst = mk_playlist("DST", "Target", vec![mk_track("V9", "S9", 0)]);
    let mut cache = HashMap::new();
    cache.insert("SRC".to_string(), src);
    cache.insert("DST".to_string(), dst);

    let mut q = SyncQueue::default();
    let eid = q.push(StagedOp::MoveTracks {
        source_playlist_id: "SRC".to_string(),
        source_playlist_title: "Source".to_string(),
        target_playlist_id: "DST".to_string(),
        target_playlist_title: "Target".to_string(),
        tracks: vec![mk_track("V1", "S1", 0)],
    });
    assert_eq!(q.len(), 1);
    let affected = q.affected_playlist_ids();
    assert!(affected.contains("SRC") && affected.contains("DST"));

    let src_draft = q.working_playlist("SRC", &cache).expect("src draft");
    let dst_draft = q.working_playlist("DST", &cache).expect("dst draft");
    assert_eq!(src_draft.tracks.len(), 1);
    assert_eq!(src_draft.tracks[0].video_id, "V2");
    assert_eq!(dst_draft.tracks.len(), 2);
    assert_eq!(dst_draft.tracks[1].video_id, "V1");
    assert_eq!(dst_draft.tracks[1].set_video_id, None);

    // Single undo removes both sides of the move.
    assert_eq!(q.last_entry_id(), Some(eid));
    assert!(q.remove_entry(eid));
    assert_eq!(q.working_playlist("SRC", &cache).unwrap().tracks.len(), 2);
    assert_eq!(q.working_playlist("DST", &cache).unwrap().tracks.len(), 1);
}

#[test]
fn sparse_update_meta_and_delete_playlist_overlay() {
    let pl = mk_playlist("PL1", "Old Title", vec![mk_track("V1", "S1", 0)]);
    let mut cache = HashMap::new();
    cache.insert("PL1".to_string(), pl);

    let mut q = SyncQueue::default();
    q.push(StagedOp::UpdatePlaylistMeta {
        playlist_id: "PL1".to_string(),
        playlist_title: "Old Title".to_string(),
        title: Some("New Title".to_string()),
        description: None,
        privacy: None,
    });

    let draft = q.working_playlist("PL1", &cache).expect("meta draft");
    assert_eq!(draft.title, "New Title");
    assert_eq!(draft.description.as_deref(), Some("Original desc"));
    assert_eq!(draft.privacy, PlaylistPrivacy::Unlisted);

    let del_id = q.push(StagedOp::DeletePlaylist {
        playlist_id: "PL1".to_string(),
        playlist_title: "New Title".to_string(),
    });
    assert!(q.working_playlist("PL1", &cache).is_none());
    assert!(q.remove_entry(del_id));
    assert_eq!(
        q.working_playlist("PL1", &cache).expect("restored").title,
        "New Title"
    );
}

#[test]
fn remove_entries_toggle_selected_and_draft_revision_bump() {
    let mut q = SyncQueue::default();
    assert_eq!(q.revision(), DraftRevision(0));

    let id1 = q.push(StagedOp::DeletePlaylist {
        playlist_id: "P1".to_string(),
        playlist_title: "One".to_string(),
    });
    let id2 = q.push(StagedOp::DeletePlaylist {
        playlist_id: "P2".to_string(),
        playlist_title: "Two".to_string(),
    });
    let id3 = q.push(StagedOp::DeletePlaylist {
        playlist_id: "P3".to_string(),
        playlist_title: "Three".to_string(),
    });
    assert_eq!(q.revision(), DraftRevision(3));
    assert_eq!(q.selected_entries().len(), 3);

    assert!(q.toggle_selected(id2));
    assert_eq!(q.revision(), DraftRevision(4));
    assert_eq!(
        q.selected_entries()
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        vec![id1, id3]
    );

    assert_eq!(q.remove_entries(&[id1, id3, 999]), 2);
    assert_eq!(q.revision(), DraftRevision(5));
    assert_eq!(q.entries().len(), 1);
    assert_eq!(q.entries()[0].id, id2);
    assert_eq!(q.remove_entries(&[999]), 0);
    assert_eq!(q.revision(), DraftRevision(5));
}

#[test]
fn serde_roundtrip_and_load_entries_advance_next_id() {
    let entries = vec![
        StagedEntry {
            id: 10,
            op: StagedOp::SetLikeStatus {
                video_id: "VID1".to_string(),
                title: "Song".to_string(),
                liked: true,
            },
            selected: true,
        },
        StagedEntry {
            id: 25,
            op: StagedOp::RestoreSnapshot {
                snapshot_id: "snap-1".to_string(),
                playlist_id: "PL1".to_string(),
                playlist_title: "Mix".to_string(),
            },
            selected: false,
        },
    ];
    let json = serde_json::to_string(&entries).expect("serialize entries");
    let decoded: Vec<StagedEntry> = serde_json::from_str(&json).expect("deserialize entries");
    assert_eq!(decoded, entries);

    // Backwards compatibility: missing `selected` and `preserve_position` default cleanly.
    let legacy = r#"{"id":3,"op":{"UpgradeTracks":{"playlist_id":"PL1","playlist_title":"Mix","upgrades":[]}}}"#;
    let legacy_entry: StagedEntry = serde_json::from_str(legacy).expect("legacy deserialize");
    assert!(legacy_entry.selected);
    assert!(matches!(
        legacy_entry.op,
        StagedOp::UpgradeTracks {
            preserve_position: false,
            ..
        }
    ));

    let mut q = SyncQueue::default();
    q.load_entries(decoded);
    let next = q.push(StagedOp::DeletePlaylist {
        playlist_id: "PL2".to_string(),
        playlist_title: "Two".to_string(),
    });
    assert_eq!(next, 26);
}

#[test]
fn move_entry_blocks_reorder_crossing_structural_op_on_same_playlist() {
    let pl = mk_playlist(
        "PL1",
        "Mix",
        vec![mk_track("V1", "S1", 0), mk_track("V2", "S2", 1)],
    );
    let mut cache = HashMap::new();
    cache.insert("PL1".to_string(), pl);

    let mut q = SyncQueue::default();
    let add_id = q.push(StagedOp::AddTracks {
        playlist_id: "PL1".to_string(),
        playlist_title: "Mix".to_string(),
        tracks: vec![mk_track("V3", "S3", 2)],
        allow_duplicates: false,
    });
    let meta_id = q.push(StagedOp::UpdatePlaylistMeta {
        playlist_id: "PL1".to_string(),
        playlist_title: "Mix".to_string(),
        title: Some("Renamed".to_string()),
        description: None,
        privacy: None,
    });
    let reorder_id = q.push(StagedOp::Reorder {
        playlist_id: "PL1".to_string(),
        playlist_title: "Renamed".to_string(),
        target_set_video_ids: vec!["S2".to_string(), "S1".to_string()],
        moves: Vec::new(),
    });

    // Crossing non-structural UpdatePlaylistMeta on the same playlist is allowed.
    let rev_before = q.revision();
    assert!(q.move_entry(reorder_id, -1).is_ok());
    assert!(q.revision() > rev_before);

    // Crossing structural AddTracks on PL1 is blocked in both directions.
    let rev_after_meta_swap = q.revision();
    assert!(q.move_entry(reorder_id, -1).is_err());
    assert!(q.move_entry(add_id, 1).is_err());
    assert_eq!(q.revision(), rev_after_meta_swap);
    assert_eq!(q.entries()[1].id, reorder_id);
    assert_eq!(q.entries()[2].id, meta_id);
}
