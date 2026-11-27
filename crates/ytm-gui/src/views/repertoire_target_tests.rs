use std::collections::HashMap;

use ytm_core::{
    AlbumRef, ArtistRef, MusicVideoType, Playlist, PlaylistPrivacy, PlaylistSummary, Track,
    TrackAvailability,
};

use super::*;

fn mk_playlist_with_album(
    id: &str,
    title: &str,
    artist: &str,
    track_title: &str,
    album_name: Option<&str>,
) -> Playlist {
    Playlist {
        id: id.to_string(),
        title: title.to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: !id.starts_with("MPRE"),
        track_count: 1,
        tracks: vec![Track {
            video_id: format!("{id}_v1"),
            set_video_id: Some(format!("{id}_s1")),
            position: 0,
            title: track_title.to_string(),
            artists: vec![ArtistRef {
                name: artist.to_string(),
                browse_id: None,
            }],
            album: album_name.map(|n| AlbumRef {
                name: n.to_string(),
                browse_id: None,
            }),
            duration_sec: Some(210),
            release_year: Some(2020),
            music_video_type: MusicVideoType::AudioTrack,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        }],
    }
}

#[test]
fn collect_repertoire_matches_finds_artist_across_cached_playlists() {
    let mut cache = HashMap::new();
    cache.insert(
        "PL1".to_string(),
        mk_playlist_with_album("PL1", "2025", "Daft Punk", "Instant Crush", None),
    );
    cache.insert(
        "PL2".to_string(),
        mk_playlist_with_album("PL2", "Chill", "Sagopa Kajmer", "Galiba", None),
    );
    cache.insert(
        "PL3".to_string(),
        mk_playlist_with_album("PL3", "Cyberpunk", "Daft Punk", "Derezzed", None),
    );

    let sums = vec![
        PlaylistSummary {
            id: "PL1".to_string(),
            title: "2025".to_string(),
            track_count: Some(1),
            owned: true,
            is_liked_music: false,
        },
        PlaylistSummary {
            id: "PL2".to_string(),
            title: "Chill".to_string(),
            track_count: Some(1),
            owned: true,
            is_liked_music: false,
        },
    ];

    let hits = collect_repertoire_matches("daft punk", &sums, &cache, &SyncQueue::default());
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].playlist.id, "PL1");
    assert_eq!(hits[1].playlist.id, "PL3");
}

#[test]
fn scoped_and_album_grouped_search_finds_album_tracks_and_deduplicates() {
    let mut cache = HashMap::new();
    let shared_track = Track {
        video_id: "VID_SHARED".into(),
        set_video_id: None,
        position: 0,
        title: "Never Fade Away".into(),
        artists: vec![ArtistRef {
            name: "SAMURAI".into(),
            browse_id: None,
        }],
        album: Some(AlbumRef {
            name: "Cyberpunk 2077: Radio, Vol. 1".into(),
            browse_id: Some("MPRE_CP1".into()),
        }),
        duration_sec: Some(190),
        release_year: Some(2020),
        music_video_type: MusicVideoType::AudioTrack,
        availability: TrackAvailability::Available,
        thumbnail_url: None,
    };
    cache.insert(
        "LM".into(),
        Playlist {
            id: "LM".into(),
            title: "Liked Songs".into(),
            description: None,
            privacy: PlaylistPrivacy::Private,
            owned: true,
            track_count: 1,
            tracks: vec![shared_track.clone()],
        },
    );
    cache.insert(
        "MPRE_CP1".into(),
        Playlist {
            id: "MPRE_CP1".into(),
            title: "Cyberpunk 2077: Radio, Vol. 1 — SAMURAI".into(),
            description: None,
            privacy: PlaylistPrivacy::Private,
            owned: false,
            track_count: 1,
            tracks: vec![shared_track],
        },
    );

    let album_hits = collect_repertoire_matches_scoped(
        "cyberpunk",
        RepertoireMatchScope::Album,
        RepertoireGroupMode::ByAlbum,
        &[],
        &cache,
        &SyncQueue::default(),
    );
    assert_eq!(album_hits.len(), 1);
    assert_eq!(album_hits[0].tracks.len(), 1);
    assert_eq!(
        album_hits[0].playlist.title,
        "Cyberpunk 2077: Radio, Vol. 1"
    );
}
