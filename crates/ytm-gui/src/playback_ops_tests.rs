use std::sync::mpsc;

use ytm_core::{MusicVideoType, TrackAvailability};

use super::*;

fn mk_track(vid: &str) -> Track {
    Track {
        video_id: vid.to_string(),
        set_video_id: None,
        position: 0,
        title: format!("Song {vid}"),
        artists: Vec::new(),
        album: None,
        duration_sec: Some(180),
        release_year: None,
        music_video_type: MusicVideoType::AudioTrack,
        availability: TrackAvailability::Available,
        thumbnail_url: None,
    }
}

#[test]
fn strict_lock_stages_set_like_status_without_emitting_worker_cmd() {
    let (tx, rx) = mpsc::channel();
    let mut pq = PlayQueue::default();
    let mut sq = SyncQueue::default();
    let mut toast = None;

    handle_toggle_like(
        mk_track("V1"),
        MutationPolicy::StrictLock,
        &mut pq,
        &mut sq,
        &mut toast,
        &tx,
    );
    assert!(rx.try_recv().is_err());
    assert!(!pq.is_liked("V1"));
    assert_eq!(sq.len(), 1);
    assert!(matches!(
        sq.entries()[0].op,
        StagedOp::SetLikeStatus {
            ref video_id,
            liked: true,
            ..
        } if video_id == "V1"
    ));
}

#[test]
fn quick_like_dispatches_worker_cmd_and_toggles_optimistically() {
    let (tx, rx) = mpsc::channel();
    let mut pq = PlayQueue::default();
    let mut sq = SyncQueue::default();
    let mut toast = None;

    handle_toggle_like(
        mk_track("V2"),
        MutationPolicy::QuickLike,
        &mut pq,
        &mut sq,
        &mut toast,
        &tx,
    );
    assert!(pq.is_liked("V2"));
    assert!(sq.is_empty());
    assert!(matches!(
        rx.try_recv(),
        Ok(WorkerCmd::SetLikeStatus {
            video_id,
            liked: true
        }) if video_id == "V2"
    ));
}
