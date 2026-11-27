use rusqlite::{Connection, params};
use tempfile::TempDir;
use ytm_core::{LyricsPayload, Playlist, PlaylistPrivacy, PlaylistSummary, TimedLyricLine};
use ytm_store::SqliteStore;

#[test]
fn offline_cache_roundtrips_library_and_lyrics_across_reopen() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("ytm.sqlite");
    let store = SqliteStore::open(&db_path, None).expect("open store");

    let summaries = vec![
        PlaylistSummary {
            id: "LM".to_string(),
            title: "Liked Music".to_string(),
            track_count: Some(42),
            is_liked_music: true,
            owned: true,
        },
        PlaylistSummary {
            id: "PL_FAV".to_string(),
            title: "Synthwave".to_string(),
            track_count: Some(18),
            is_liked_music: false,
            owned: true,
        },
    ];
    store
        .save_library_summaries(&summaries)
        .expect("save library");

    let synced = LyricsPayload::Synced(vec![TimedLyricLine {
        start_ms: 1000,
        end_ms: 4500,
        text: "Around the world".to_string(),
    }]);
    store
        .save_cached_lyrics("vid_sync", Some("MPLYt_1"), &synced)
        .expect("save lyrics");
    drop(store);

    let reopened = SqliteStore::open(&db_path, None).expect("reopen");
    let loaded_libs = reopened
        .load_library_summaries()
        .expect("load library")
        .expect("some summaries");
    assert_eq!(loaded_libs.len(), 2);
    assert_eq!(loaded_libs[0].id, "LM");
    assert_eq!(loaded_libs[1].title, "Synthwave");

    let loaded_lyrics = reopened
        .load_cached_lyrics("vid_sync")
        .expect("load lyrics")
        .expect("some lyrics");
    assert_eq!(loaded_lyrics, synced);
}

#[test]
fn v0_1_1_legacy_sqlite_schema_migrates_and_preserves_rows() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("legacy.sqlite");
    let legacy_pl = Playlist {
        id: "PL_OLD".to_string(),
        title: "Old List".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 0,
        tracks: Vec::new(),
    };
    let legacy_json = serde_json::to_string(&legacy_pl).expect("serialize legacy playlist");
    {
        let conn = Connection::open(&db_path).expect("open raw legacy sqlite");
        conn.execute_batch(
            "CREATE TABLE playlist_cache (
                playlist_id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
             );",
        )
        .expect("seed legacy schema");
        conn.execute(
            "INSERT INTO playlist_cache (playlist_id, title, payload_json, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params!["PL_OLD", "Old List", legacy_json, 1_700_000_000_i64],
        )
        .expect("insert legacy row");
    }

    let store = SqliteStore::open(&db_path, None).expect("migrate legacy store");
    let playlists = store
        .load_all_cached_playlists()
        .expect("load preserved playlist");
    assert_eq!(playlists.len(), 1);
    assert_eq!(playlists[0].id, "PL_OLD");
    assert!(
        store
            .load_library_summaries()
            .expect("query migrated table")
            .is_none()
    );
    drop(store);

    let check_conn = Connection::open(&db_path).expect("reopen to check user_version");
    let ver: i64 = check_conn
        .query_row("PRAGMA user_version;", [], |r| r.get(0))
        .expect("read user_version");
    assert_eq!(ver, 3);
}
