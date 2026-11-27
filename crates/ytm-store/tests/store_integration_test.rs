use chrono::{TimeZone, Utc};
use tempfile::TempDir;
use ytm_core::{
    AlbumRef, ArtistRef, FetchCompleteness, FetchManifest, MusicVideoType, Playlist,
    PlaylistPrivacy, Track, TrackAvailability,
};
use ytm_store::{
    AppPaths, CachedArtistGenre, CachedPlaylistRecord, CachedTrack, CachedTrackEnrichment,
    EnrichmentCacheStore, ExportFormat, PersistedPlaybackSession, PersistedStagedOp, SnapshotStore,
    SqliteStore, StoreError, export_playlist_to_path, export_playlist_to_string,
    import_playlist_from_json, write_secret_file,
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
                thumbnail_url: None,
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
                thumbnail_url: None,
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

#[tokio::test]
async fn v1_to_v2_migration_preserves_existing_rows_and_is_idempotent() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("legacy_v1.sqlite");
    let legacy_playlist = sample_playlist();
    let legacy_pl_json = serde_json::to_string(&legacy_playlist).expect("serialize playlist");
    let snap_json = format!(
        r#"{{"metadata":{{"id":"snap_v1","playlist_id":"PL_TEST_01","playlist_title":"Midnight Synth","reason":"pre-v2","created_at":"2026-01-01T00:00:00Z","item_count":2}},"playlist":{legacy_pl_json}}}"#
    );
    {
        let conn = rusqlite::Connection::open(&db_path).expect("create v1 db");
        conn.execute_batch(
            "CREATE TABLE snapshots (id TEXT PRIMARY KEY, playlist_id TEXT NOT NULL, playlist_title TEXT NOT NULL, reason TEXT NOT NULL, created_at INTEGER NOT NULL, item_count INTEGER NOT NULL, payload_json TEXT NOT NULL);
             CREATE TABLE playlist_cache (playlist_id TEXT PRIMARY KEY, title TEXT NOT NULL, payload_json TEXT NOT NULL, updated_at INTEGER NOT NULL);",
        ).expect("create v1 schema");
        conn.execute(
            "INSERT INTO snapshots VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "snap_v1",
                "PL_TEST_01",
                "Midnight Synth",
                "pre-v2",
                1_700_000_000i64,
                2i64,
                snap_json
            ],
        )
        .expect("insert v1 snapshot");
        conn.execute(
            "INSERT INTO playlist_cache VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "PL_TEST_01",
                "Midnight Synth",
                legacy_pl_json,
                1_700_000_050i64
            ],
        )
        .expect("insert v1 playlist_cache");
    }

    drop(SqliteStore::open(&db_path, None).expect("migrate v1 -> v2"));
    let store = SqliteStore::open(&db_path, None).expect("reopen migrated db");

    let snaps = store.list_snapshots(None).await.expect("list snapshots");
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].id, "snap_v1");

    let records = store
        .load_all_cached_playlist_records()
        .expect("load records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].playlist, legacy_playlist);
    assert_eq!(records[0].manifest, None);
    assert_eq!(records[0].fetched_at, 1_700_000_050);
}

#[test]
fn staged_ops_roundtrip_preserves_order_and_selection_across_reopen() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("staged_ops.sqlite");
    let ops = vec![
        PersistedStagedOp {
            entry_id: 101,
            seq: 0,
            selected: true,
            op_json: r#"{"kind":"AddTracks","id":101}"#.into(),
            created_at: 1_700_000_001,
        },
        PersistedStagedOp {
            entry_id: 55,
            seq: 1,
            selected: false,
            op_json: r#"{"kind":"RemoveTracks","id":55}"#.into(),
            created_at: 1_700_000_002,
        },
        PersistedStagedOp {
            entry_id: 209,
            seq: 2,
            selected: true,
            op_json: r#"{"kind":"Reorder","id":209}"#.into(),
            created_at: 1_700_000_003,
        },
    ];
    SqliteStore::open(&db_path, None)
        .expect("open store")
        .replace_staged_ops(&ops)
        .expect("replace");

    let reopened = SqliteStore::open(&db_path, None).expect("reopen store");
    assert_eq!(reopened.load_staged_ops().expect("load staged ops"), ops);

    reopened.delete_staged_op(55).expect("delete op 55");
    let after_del = reopened.load_staged_ops().expect("load after delete");
    assert_eq!(after_del.len(), 2);
    assert_eq!((after_del[0].entry_id, after_del[1].entry_id), (101, 209));

    reopened
        .replace_staged_ops(&[ops[1].clone()])
        .expect("atomic replace");
    assert_eq!(reopened.load_staged_ops().expect("load replaced").len(), 1);

    reopened.clear_staged_ops().expect("clear staged ops");
    assert!(reopened.load_staged_ops().expect("empty").is_empty());
}

#[test]
fn playlist_cache_record_manifest_roundtrip_and_orphan_pruning() {
    let store = SqliteStore::open_in_memory(None).expect("open memory store");
    let mut pl1 = sample_playlist();
    pl1.id = "PL_KEEP".to_string();
    pl1.title = "Alpha Mix".to_string();
    let mut pl2 = sample_playlist();
    pl2.id = "PL_ORPHAN".to_string();
    pl2.title = "Beta Mix".to_string();

    let fetched_at = Utc.with_ymd_and_hms(2026, 11, 26, 10, 0, 0).unwrap();
    let manifest = FetchManifest {
        declared_track_count: Some(2),
        fetched_tracks: 2,
        available_tracks: 1,
        unavailable_tracks: 1,
        missing_set_video_id: 0,
        pages_fetched: 1,
        completeness: FetchCompleteness::Complete,
        fetched_at,
    };
    let rec1 = CachedPlaylistRecord {
        playlist: pl1,
        manifest: Some(manifest),
        fetched_at: fetched_at.timestamp(),
    };
    let rec2 = CachedPlaylistRecord {
        playlist: pl2,
        manifest: None,
        fetched_at: fetched_at.timestamp() + 10,
    };

    store
        .upsert_cached_playlist_record(&rec1)
        .expect("upsert 1");
    store
        .upsert_cached_playlist_record(&rec2)
        .expect("upsert 2");
    assert_eq!(
        store.load_all_cached_playlist_records().expect("load"),
        vec![rec1.clone(), rec2]
    );

    assert_eq!(
        store
            .prune_missing_cached_playlists(&["PL_KEEP".to_string()])
            .expect("prune"),
        1
    );
    assert_eq!(
        store.load_all_cached_playlist_records().expect("remaining"),
        vec![rec1]
    );
    assert_eq!(
        store
            .prune_missing_cached_playlists(&[])
            .expect("prune all"),
        1
    );
    assert!(
        store
            .load_all_cached_playlist_records()
            .expect("empty")
            .is_empty()
    );
}

#[test]
fn playback_session_roundtrips_across_reopen() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("playback.sqlite");
    {
        let store = SqliteStore::open(&db_path, None).expect("open store");
        assert_eq!(store.load_playback_session().expect("initial none"), None);
        let first = PersistedPlaybackSession {
            session_json: r#"{"queue":["v1","v2"],"index":0}"#.into(),
            updated_at: 1_700_001_000,
        };
        store.save_playback_session(&first).expect("save first");
        assert_eq!(
            store.load_playback_session().expect("load first"),
            Some(first)
        );
        let updated = PersistedPlaybackSession {
            session_json: r#"{"queue":["v1","v2","v3"],"index":2}"#.into(),
            updated_at: 1_700_002_000,
        };
        store.save_playback_session(&updated).expect("save updated");
    }
    let reopened = SqliteStore::open(&db_path, None).expect("reopen");
    let loaded = reopened
        .load_playback_session()
        .expect("load after reopen")
        .expect("exists");
    assert_eq!(loaded.updated_at, 1_700_002_000);
    assert!(loaded.session_json.contains("\"index\":2"));
}

#[tokio::test]
async fn play_history_insert_and_get() {
    let store = SqliteStore::open_in_memory(None).expect("mem store");
    use ytm_store::EnrichmentCacheStore;
    // Insert a few entries
    store
        .insert_play_history(
            "vid1",
            "Track One",
            "[\"Artist A\"]",
            Some("Album X"),
            Some(200),
        )
        .await
        .expect("insert 1");
    store
        .insert_play_history("vid2", "Track Two", "[\"Artist B\"]", None, Some(180))
        .await
        .expect("insert 2");
    store
        .insert_play_history(
            "vid3",
            "Track Three",
            "[\"Artist C\"]",
            Some("Album Y"),
            Some(220),
        )
        .await
        .expect("insert 3");

    // Get recent history
    let history = store.get_play_history(10).await.expect("get history");
    assert_eq!(history.len(), 3);
    // Most recent first
    assert_eq!(history[0].video_id, "vid3");
    assert_eq!(history[0].title, "Track Three");
    assert_eq!(history[1].video_id, "vid2");
    assert_eq!(history[2].video_id, "vid1");

    // Limit works
    let limited = store.get_play_history(2).await.expect("get limited");
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].video_id, "vid3");
    assert_eq!(limited[1].video_id, "vid2");
}

#[tokio::test]
async fn play_history_caps_at_500() {
    let store = SqliteStore::open_in_memory(None).expect("mem store");
    use ytm_store::EnrichmentCacheStore;
    // Insert 510 entries
    for i in 0..510 {
        store
            .insert_play_history(
                &format!("vid{}", i),
                &format!("Track {}", i),
                "[\"Artist\"]",
                None,
                Some(200),
            )
            .await
            .expect("insert");
    }
    let history = store.get_play_history(600).await.expect("get history");
    assert_eq!(history.len(), 500);
    // Most recent should be vid509
    assert_eq!(history[0].video_id, "vid509");
}
