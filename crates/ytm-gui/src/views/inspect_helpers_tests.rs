use ytm_core::{MusicVideoType, PlaylistPrivacy, TrackAvailability};

use super::*;

fn mk_track(vid: &str, sid: &str, pos: usize) -> Track {
    Track {
        video_id: vid.into(),
        set_video_id: Some(sid.into()),
        position: pos,
        title: vid.into(),
        artists: Vec::new(),
        album: None,
        duration_sec: Some(120),
        release_year: None,
        music_video_type: MusicVideoType::AudioTrack,
        availability: TrackAvailability::Available,
        thumbnail_url: None,
    }
}

#[test]
fn compute_drop_reorder_moves_to_middle_and_end() {
    let pl = Playlist {
        id: "PL1".into(),
        title: "Mix".into(),
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
    assert_eq!(
        compute_drop_reorder(&pl, &["S1".into()], 3).expect("end"),
        vec!["S2", "S3", "S1"]
    );
    assert_eq!(
        compute_drop_reorder(&pl, &["S3".into()], 0).expect("top"),
        vec!["S3", "S1", "S2"]
    );
}

#[test]
fn sort_stage_memo_hits_same_revision_and_invalidates_on_change() {
    let ctx = egui::Context::default();
    let pl = Playlist {
        id: "PL1".into(),
        title: "Mix".into(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 2,
        tracks: vec![mk_track("Z", "S1", 0), mk_track("A", "S2", 1)],
    };
    let sort_asc = InspectSortState {
        column: InspectColumn::Title,
        asc: true,
    };
    let first = cached_sort_stage_state(&ctx, &pl, DraftRevision(1), sort_asc);
    let second = cached_sort_stage_state(&ctx, &pl, DraftRevision(1), sort_asc);
    assert_eq!(first, second);
    if let (
        SortStageComputed::Moves { target_sids: a, .. },
        SortStageComputed::Moves { target_sids: b, .. },
    ) = (&first, &second)
    {
        assert!(Arc::ptr_eq(a, b));
    } else {
        panic!("expected Moves");
    }
    let sort_desc = InspectSortState {
        column: InspectColumn::Title,
        asc: false,
    };
    assert!(matches!(
        cached_sort_stage_state(&ctx, &pl, DraftRevision(2), sort_desc),
        SortStageComputed::MatchesCurrent
    ));
}

#[test]
fn lm_selection_stages_unlikes_and_editable_stages_remove_tracks() {
    let lm = Playlist {
        id: "LM".into(),
        title: "Liked Music".into(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 2,
        tracks: vec![mk_track("V1", "S1", 0), mk_track("V2", "S2", 1)],
    };
    let mut queue = SyncQueue::default();
    let selected = HashSet::from([0usize, 1usize]);
    let res = stage_selected_removals(&lm, false, &selected, &mut queue).expect("lm unlikes");
    assert_eq!((res.0, res.2), (2, true));
    assert_eq!(queue.entries().len(), 1); // single batched entry
    assert!(
        queue
            .entries()
            .iter()
            .all(|e| matches!(&e.op, StagedOp::SetLikeStatusBatch { liked: false, .. }))
    );

    let mut owned = lm;
    owned.id = "PL_USER".into();
    let mut q2 = SyncQueue::default();
    let res2 = stage_selected_removals(&owned, true, &HashSet::from([1usize]), &mut q2)
        .expect("owned remove");
    assert_eq!((res2.0, res2.2), (1, false));
    assert!(matches!(q2.entries()[0].op, StagedOp::RemoveTracks { .. }));
}
