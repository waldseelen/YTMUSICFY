use std::collections::HashMap;
use std::str::FromStr;

use ytm_core::{
    AlbumRef, ArtistRef, MusicVideoType, Playlist, PlaylistPrivacy, Track, TrackAvailability,
};
use ytm_ops::{
    DedupeConfig, HealthIssueKind, KeeperStrategy, MatchMode, SetAlgebraOp, SortSpec,
    apply_move_actions, compute_minimal_moves, normalize_track, plan_dedupe, plan_set_op,
    plan_sort, scan_playlist_health,
};

fn make_track(
    video_id: &str,
    set_video_id: Option<&str>,
    position: usize,
    title: &str,
    artist: &str,
    duration_sec: Option<u32>,
    music_video_type: MusicVideoType,
) -> Track {
    Track {
        video_id: video_id.to_string(),
        set_video_id: set_video_id.map(str::to_string),
        position,
        title: title.to_string(),
        artists: if artist.is_empty() {
            Vec::new()
        } else {
            vec![ArtistRef {
                name: artist.to_string(),
                browse_id: None,
            }]
        },
        album: Some(AlbumRef {
            name: "Album".to_string(),
            browse_id: None,
        }),
        duration_sec,
        release_year: Some(2020),
        music_video_type,
        availability: TrackAvailability::Available,
    }
}

fn make_playlist(id: &str, title: &str, tracks: Vec<Track>) -> Playlist {
    Playlist {
        id: id.to_string(),
        title: title.to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: tracks.len(),
        tracks,
    }
}

fn next_permutations(seq: &mut [usize]) -> bool {
    let n = seq.len();
    if n < 2 {
        return false;
    }
    let mut i = n - 2;
    while seq[i] >= seq[i + 1] {
        if i == 0 {
            return false;
        }
        i -= 1;
    }
    let mut j = n - 1;
    while seq[j] <= seq[i] {
        j -= 1;
    }
    seq.swap(i, j);
    seq[i + 1..].reverse();
    true
}

#[test]
fn lis_minimal_moves_exhaustive_all_5_factorial_permutations() {
    let target: Vec<String> = (0..5).map(|i| format!("S{i}")).collect();
    let mut perm = vec![0usize, 1, 2, 3, 4];
    let mut count = 0usize;

    loop {
        let current: Vec<String> = perm.iter().map(|&i| format!("S{i}")).collect();
        let (anchored, moves) = compute_minimal_moves(&current, &target);
        assert_eq!(anchored + moves.len(), target.len());
        assert_eq!(apply_move_actions(&current, &moves), target);
        count += 1;
        if !next_permutations(&mut perm) {
            break;
        }
    }
    assert_eq!(count, 120);
}

#[test]
fn lis_minimal_moves_handles_sorted_reverse_and_100_item_shuffle() {
    let target: Vec<String> = (0..100).map(|i| format!("SET_{i:03}")).collect();

    let (anchored_sorted, moves_sorted) = compute_minimal_moves(&target, &target);
    assert_eq!(anchored_sorted, 100);
    assert!(moves_sorted.is_empty());

    let reversed: Vec<String> = target.iter().cloned().rev().collect();
    let (anchored_rev, moves_rev) = compute_minimal_moves(&reversed, &target);
    assert_eq!(anchored_rev, 1);
    assert_eq!(moves_rev.len(), 99);
    assert_eq!(apply_move_actions(&reversed, &moves_rev), target);

    let tracks: Vec<Track> = target
        .iter()
        .enumerate()
        .rev()
        .map(|(idx, sid)| {
            make_track(
                &format!("VID_{idx:03}"),
                Some(sid),
                idx,
                &format!("Track {idx:03}"),
                &format!("Artist {:02}", idx % 10),
                Some(200),
                MusicVideoType::AudioTrack,
            )
        })
        .collect();
    let playlist = make_playlist("PL_100", "Hundred", tracks);
    let spec = SortSpec::from_str("title:asc").expect("valid sort spec");
    let plan = plan_sort(&playlist, &spec, None).expect("sort plan");
    let current_ids: Vec<String> = playlist
        .tracks
        .iter()
        .filter_map(|t| t.set_video_id.clone())
        .collect();
    let target_ids: Vec<String> = plan
        .target_order
        .iter()
        .filter_map(|t| t.set_video_id.clone())
        .collect();
    assert_eq!(apply_move_actions(&current_ids, &plan.moves), target_ids);
}

#[test]
fn artist_spread_shuffle_is_deterministic_and_separates_consecutive_artists() {
    let mut tracks = Vec::new();
    for i in 0..12 {
        let artist = if i < 4 {
            "Artist A"
        } else if i < 8 {
            "Artist B"
        } else {
            "Artist C"
        };
        tracks.push(make_track(
            &format!("V{i}"),
            Some(&format!("S{i}")),
            i,
            &format!("Song {i}"),
            artist,
            Some(190),
            MusicVideoType::AudioTrack,
        ));
    }
    let playlist = make_playlist("PL_SHUF", "Shuffle", tracks);
    let mut spec = SortSpec::from_str("shuffle").expect("shuffle spec");
    spec.seed = Some(42);

    let plan_1 = plan_sort(&playlist, &spec, None).expect("first shuffle");
    let plan_2 = plan_sort(&playlist, &spec, None).expect("second shuffle");
    assert_eq!(plan_1.target_order, plan_2.target_order);

    for pair in plan_1.target_order.windows(2) {
        assert_ne!(
            normalize_track(&pair[0]).artist_norm,
            normalize_track(&pair[1]).artist_norm
        );
    }
}

#[test]
fn dedupe_merges_ugc_and_atv_while_respecting_duration_guard() {
    let tracks = vec![
        make_track(
            "VID_UGC",
            Some("SET_1"),
            0,
            "Daft Punk - Get Lucky (Official Music Video)",
            "RandomUploader",
            Some(248),
            MusicVideoType::UserGenerated,
        ),
        make_track(
            "VID_ATV",
            Some("SET_2"),
            1,
            "Get Lucky",
            "Daft Punk",
            Some(249),
            MusicVideoType::AudioTrack,
        ),
        make_track(
            "VID_LIVE",
            Some("SET_3"),
            2,
            "Get Lucky (Live Acoustic)",
            "Daft Punk",
            Some(320),
            MusicVideoType::OfficialMusicVideo,
        ),
        make_track(
            "VID_ATV",
            None,
            3,
            "Get Lucky",
            "Daft Punk",
            Some(249),
            MusicVideoType::AudioTrack,
        ),
    ];
    let playlist = make_playlist("PL_DEDUPE", "Dedupe", tracks);

    let plan = plan_dedupe(&playlist, &DedupeConfig::default()).expect("dedupe plan");
    assert_eq!(plan.skipped_without_set_id, 1);
    assert_eq!(plan.clusters.len(), 1);
    assert_eq!(plan.clusters[0].keeper.video_id, "VID_ATV");
    assert_eq!(plan.clusters[0].removals.len(), 1);
    assert_eq!(plan.clusters[0].removals[0].video_id, "VID_UGC");

    let oldest_cfg = DedupeConfig {
        keep: KeeperStrategy::Oldest,
        ..Default::default()
    };
    let oldest_plan = plan_dedupe(&playlist, &oldest_cfg).expect("oldest dedupe");
    assert_eq!(oldest_plan.clusters[0].keeper.video_id, "VID_UGC");
}

#[test]
fn set_algebra_and_health_scanner_cover_all_modes() {
    let t_a1 = make_track(
        "V1",
        Some("SA1"),
        0,
        "Time",
        "Pink Floyd",
        Some(413),
        MusicVideoType::AudioTrack,
    );
    let mut t_a2 = make_track(
        "V2",
        Some("SA2"),
        1,
        "Numb (Official Video)",
        "Linkin Park",
        Some(187),
        MusicVideoType::OfficialMusicVideo,
    );
    t_a2.availability = TrackAvailability::Deleted;

    let t_b1 = make_track(
        "V3",
        Some("SB1"),
        0,
        "Time (Lyrics)",
        "Pink Floyd",
        Some(415),
        MusicVideoType::UserGenerated,
    );
    let t_b2 = make_track(
        "V4",
        Some("SB2"),
        1,
        "Hysteria",
        "Muse",
        Some(227),
        MusicVideoType::AudioTrack,
    );

    let pl_a = make_playlist("PLA", "A", vec![t_a1, t_a2.clone()]);
    let pl_b = make_playlist("PLB", "B", vec![t_b1, t_b2]);

    let exact_diff = plan_set_op(
        SetAlgebraOp::Diff,
        MatchMode::ExactVideoId,
        &pl_a,
        &pl_b,
        None,
        0.90,
    )
    .expect("exact diff");
    assert_eq!(exact_diff.result_tracks.len(), 2);

    let fuzzy_intersect = plan_set_op(
        SetAlgebraOp::Intersect,
        MatchMode::FuzzyNormalized,
        &pl_a,
        &pl_b,
        None,
        0.90,
    )
    .expect("fuzzy intersect");
    assert_eq!(fuzzy_intersect.result_tracks.len(), 1);
    assert_eq!(fuzzy_intersect.result_tracks[0].video_id, "V1");

    let merge = plan_set_op(
        SetAlgebraOp::Merge,
        MatchMode::FuzzyNormalized,
        &pl_a,
        &pl_b,
        Some("PLT".to_string()),
        0.90,
    )
    .expect("fuzzy merge");
    assert_eq!(merge.result_tracks.len(), 3);

    let mut ugc = make_track(
        "V_UGC",
        None,
        2,
        "Muse - Hysteria (Official Video)",
        "FanChannel",
        Some(228),
        MusicVideoType::UserGenerated,
    );
    ugc.availability = TrackAvailability::Available;
    let pl_health = make_playlist("PLH", "Health", vec![t_a2, ugc]);

    let mut search_map = HashMap::new();
    search_map.insert(
        "V_UGC".to_string(),
        vec![make_track(
            "V4",
            None,
            0,
            "Hysteria",
            "Muse",
            Some(227),
            MusicVideoType::AudioTrack,
        )],
    );

    let report = scan_playlist_health(&pl_health, &search_map, 0.90);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.kind == HealthIssueKind::Deleted)
    );
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.kind == HealthIssueKind::MissingSetVideoId)
    );
    assert_eq!(report.upgrade_candidates.len(), 1);
    assert_eq!(report.upgrade_candidates[0].official_atv.video_id, "V4");
}
