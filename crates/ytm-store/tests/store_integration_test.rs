use tempfile::TempDir;
use ytm_core::{
    AlbumRef, ArtistRef, MusicVideoType, Playlist, PlaylistPrivacy, Track, TrackAvailability,
};
use ytm_store::{
    AppPaths, CachedArtistGenre, CachedTrack, CachedTrackEnrichment, EnrichmentCacheStore,
    ExportFormat, SnapshotStore, SqliteStore, StoreError, export_playlist_to_path,
    export_playlist_to_string, import_playlist_from_json, write_secret_file,
};

fn sample_playlist() -> Playlist {
    Playlist {
        id: "PL_TEST_01".to_string(),
        title: "Midnight Synth".to_string(),
        description: Some("Test backup".to_string()),
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 2,
        tracks: vec![
            Track {
                video_id: "vid1".to_string(),
                set_video_id: Some("set1".to_string()),
                position: 0,
                title: "Time, Forever".to_string(),
                artists: vec![ArtistRef {
                    name: "Pink Floyd".to_string(),
                    browse_id: Some("UC1".to_string()),
                }],
                album: Some(AlbumRef {
                    name: "Dark Side".to_string(),
                    browse_id: Some("MP1".to_string()),
                }),
                duration_sec: Some(413),
                release_year: Some(1973),
                music_video_type: MusicVideoType::AudioTrack,
                availability: TrackAvailability::Available,
            },
            Track {
                video_id: String::new(),
                set_video_id: Some("set2".to_string()),
                position: 1,
                title: "[Deleted video]".to_string(),
                artists: vec![],
                album: None,
                duration_sec: None,
                release_year: None,
                music_video_type: MusicVideoType::Unknown,
                availability: TrackAvailability::Deleted,
            },
        ],
    }
}

#[tokio::test]
async fn in_memory_sqlite_caches_roundtrip() {
    let store = SqliteStore::open_in_memory(None).expect("open memory sqlite");

    let track = CachedTrack {
        video_id: "vid1".to_string(),
        title: "Time".to_string(),
        artists_json: r#"["Pink Floyd"]"#.to_string(),
        album: Some("Dark Side".to_string()),
        duration_sec: Some(413),
        release_year: Some(1973),
        music_video_type: MusicVideoType::AudioTrack,
        updated_at: 1_700_000_000,
    };
    store
        .upsert_cached_track(&track)
        .await
        .expect("upsert track");
    let got_track = store
        .get_cached_track("vid1")
        .await
        .expect("get track")
        .expect("exists");
    assert_eq!(got_track, track);

    let artist = CachedArtistGenre {
        artist_name_norm: "pink floyd".to_string(),
        raw_tags: vec![
            "progressive rock".to_string(),
            "psychedelic rock".to_string(),
        ],
        primary_genre: "Rock".to_string(),
        source: "musicbrainz".to_string(),
        updated_at: 1_700_000_100,
    };
    store
        .upsert_artist_genre(&artist)
        .await
        .expect("upsert artist");
    let got_artist = store
        .get_artist_genre("pink floyd")
        .await
        .expect("get artist")
        .expect("exists");
    assert_eq!(got_artist, artist);

    let enrich = CachedTrackEnrichment {
        cache_key: "pink floyd|time".to_string(),
        video_id: Some("vid1".to_string()),
        tags: vec!["art rock".to_string()],
        primary_genre: "Rock".to_string(),
        release_year: Some(1973),
        bpm: Some(120.5),
        source: "musicbrainz".to_string(),
        updated_at: 1_700_000_200,
    };
    store
        .upsert_track_enrichment(&enrich)
        .await
        .expect("upsert enrichment");
    let got_enrich = store
        .get_track_enrichment("pink floyd|time")
        .await
        .expect("get enrichment")
        .expect("exists");
    assert_eq!(got_enrich.primary_genre, "Rock");
    assert_eq!(got_enrich.release_year, Some(1973));
}

#[tokio::test]
async fn snapshot_store_and_paths_persist_atomically_with_permissions() {
    let temp = TempDir::new().expect("temp dir");
    let paths = AppPaths::from_root(temp.path());
    paths.ensure_dirs().expect("ensure dirs");

    let auth_path = paths.auth_file();
    write_secret_file(&auth_path, br#"{"cookie":"SAPISID=x"}"#).expect("write secret");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&auth_path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    let store = SqliteStore::open(&paths.db_file(), Some(paths.snapshots_dir())).expect("open db");
    let playlist = sample_playlist();

    let meta = store
        .save_snapshot(&playlist, "pre-dedupe")
        .await
        .expect("save snapshot");
    assert_eq!(meta.item_count, 2);

    let disk_file = paths
        .snapshots_dir()
        .join("PL_TEST_01")
        .join(format!("{}.json", meta.id));
    assert!(disk_file.exists());

    let listed = store
        .list_snapshots(Some("PL_TEST_01"))
        .await
        .expect("list snapshots");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, meta.id);

    let loaded = store.load_snapshot(&meta.id).await.expect("load snapshot");
    assert_eq!(loaded.playlist, playlist);

    let missing = store.load_snapshot("nonexistent").await.unwrap_err();
    assert!(matches!(missing, StoreError::SnapshotNotFound(_)));
}

#[test]
fn export_formats_json_csv_m3u8_and_import_roundtrip() {
    let temp = TempDir::new().expect("temp dir");
    let playlist = sample_playlist();

    let json_path = temp.path().join("out.json");
    export_playlist_to_path(&playlist, &json_path, ExportFormat::Json).expect("export json");
    let reimported = import_playlist_from_json(&json_path).expect("import json");
    assert_eq!(reimported, playlist);

    let csv_str = export_playlist_to_string(&playlist, ExportFormat::Csv).expect("export csv");
    assert!(csv_str.contains("\"Time, Forever\""));
    assert!(csv_str.contains("Pink Floyd"));
    assert!(csv_str.contains("Deleted"));

    let m3u8_str = export_playlist_to_string(&playlist, ExportFormat::M3u8).expect("export m3u8");
    assert!(m3u8_str.starts_with("#EXTM3U"));
    assert!(m3u8_str.contains("#EXTINF:413,Pink Floyd - Time, Forever"));
    assert!(m3u8_str.contains("https://music.youtube.com/watch?v=vid1"));
    assert!(!m3u8_str.contains("https://music.youtube.com/watch?v=\n"));
}
