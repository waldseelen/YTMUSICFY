use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension, params};
use ytm_core::MusicVideoType;

use crate::error::StoreError;
use crate::models::{CachedArtistGenre, CachedTrack, CachedTrackEnrichment};
use crate::traits::EnrichmentCacheStore;

/// A single entry in the play history.
#[derive(Debug, Clone)]
pub struct PlayHistoryEntry {
    pub id: i64,
    pub video_id: String,
    pub title: String,
    pub artists_json: String,
    pub album: Option<String>,
    pub duration_sec: Option<u32>,
    pub played_at: i64,
}

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS track_cache (
    video_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    artists_json TEXT NOT NULL,
    album TEXT,
    duration_sec INTEGER,
    release_year INTEGER,
    music_video_type TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS artist_genres (
    artist_name_norm TEXT PRIMARY KEY,
    raw_tags_json TEXT NOT NULL,
    primary_genre TEXT NOT NULL,
    source TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS track_enrichment (
    cache_key TEXT PRIMARY KEY,
    video_id TEXT,
    tags_json TEXT NOT NULL,
    primary_genre TEXT NOT NULL,
    release_year INTEGER,
    bpm REAL,
    source TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS snapshots (
    id TEXT PRIMARY KEY,
    playlist_id TEXT NOT NULL,
    playlist_title TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    item_count INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_snapshots_playlist_created
    ON snapshots (playlist_id, created_at DESC);

CREATE TABLE IF NOT EXISTS playlist_cache (
    playlist_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    manifest_json TEXT,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS staged_ops (
    entry_id INTEGER PRIMARY KEY,
    seq INTEGER NOT NULL,
    selected INTEGER NOT NULL DEFAULT 1,
    op_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_staged_ops_seq
    ON staged_ops (seq ASC, entry_id ASC);

CREATE TABLE IF NOT EXISTS playback_session (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    session_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS play_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    video_id TEXT NOT NULL,
    title TEXT NOT NULL,
    artists_json TEXT NOT NULL,
    album TEXT,
    duration_sec INTEGER,
    played_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_play_history_played_at
    ON play_history (played_at DESC);
"#;

/// Local SQLite persistence and atomic disk snapshot store.
#[derive(Clone)]
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
    pub(crate) snapshots_dir: Option<PathBuf>,
}

impl SqliteStore {
    /// Open or initialize SQLite database at `db_path`, optionally mirroring snapshots to `snapshots_dir`.
    pub fn open(db_path: &Path, snapshots_dir: Option<PathBuf>) -> Result<Self, StoreError> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| StoreError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000;",
        )?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            snapshots_dir,
        };
        store.migrate()?;
        Ok(store)
    }

    /// Open an isolated `:memory:` SQLite database for deterministic unit/integration tests.
    pub fn open_in_memory(snapshots_dir: Option<PathBuf>) -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA busy_timeout = 5000;")?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            snapshots_dir,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute_batch(SCHEMA_SQL)?;
            ensure_column_exists(conn, "playlist_cache", "manifest_json", "TEXT")?;
            crate::offline_cache::migrate_offline_cache(conn)?;
            Ok(())
        })
    }

    pub(crate) fn with_conn<F, R>(&self, f: F) -> Result<R, StoreError>
    where
        F: FnOnce(&Connection) -> Result<R, StoreError>,
    {
        let guard = self.conn.lock().expect("sqlite mutex");
        f(&guard)
    }

    pub(crate) async fn with_conn_blocking<F, R>(&self, f: F) -> Result<R, StoreError>
    where
        F: FnOnce(&Connection) -> Result<R, StoreError> + Send + 'static,
        R: Send + 'static,
    {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.with_conn(f))
            .await
            .map_err(|e| StoreError::TaskJoin(e.to_string()))?
    }
}

fn ensure_column_exists(
    conn: &Connection,
    table: &str,
    column: &str,
    sql_type: &str,
) -> Result<(), StoreError> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma)?;
    let mut rows = stmt.query([])?;
    let mut found = false;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            found = true;
            break;
        }
    }
    if !found {
        let alter = format!("ALTER TABLE {table} ADD COLUMN {column} {sql_type}");
        conn.execute_batch(&alter)?;
    }
    Ok(())
}

fn music_video_type_to_db(kind: MusicVideoType) -> &'static str {
    kind.short_label()
}

fn music_video_type_from_db(raw: &str) -> MusicVideoType {
    match raw {
        "ATV" => MusicVideoType::AudioTrack,
        "OMV" => MusicVideoType::OfficialMusicVideo,
        "UGC" => MusicVideoType::UserGenerated,
        "OSM" => MusicVideoType::OfficialSourceMusic,
        _ => MusicVideoType::Unknown,
    }
}

#[async_trait]
impl EnrichmentCacheStore for SqliteStore {
    async fn get_cached_track(&self, video_id: &str) -> Result<Option<CachedTrack>, StoreError> {
        let video_id = video_id.to_string();
        self.with_conn_blocking(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT video_id, title, artists_json, album, duration_sec, release_year, music_video_type, updated_at
                 FROM track_cache WHERE video_id = ?1",
            )?;
            let row = stmt
                .query_row(params![video_id], |r| {
                    let mvt_raw: String = r.get(6)?;
                    Ok(CachedTrack {
                        video_id: r.get(0)?,
                        title: r.get(1)?,
                        artists_json: r.get(2)?,
                        album: r.get(3)?,
                        duration_sec: r.get::<_, Option<i64>>(4)?.and_then(|v| u32::try_from(v).ok()),
                        release_year: r.get::<_, Option<i64>>(5)?.and_then(|v| u16::try_from(v).ok()),
                        music_video_type: music_video_type_from_db(&mvt_raw),
                        updated_at: r.get(7)?,
                    })
                })
                .optional()?;
            Ok(row)
        })
        .await
    }

    async fn upsert_cached_track(&self, track: &CachedTrack) -> Result<(), StoreError> {
        let track = track.clone();
        self.with_conn_blocking(move |conn| {
            conn.execute(
                "INSERT INTO track_cache (video_id, title, artists_json, album, duration_sec, release_year, music_video_type, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(video_id) DO UPDATE SET
                   title = excluded.title,
                   artists_json = excluded.artists_json,
                   album = excluded.album,
                   duration_sec = excluded.duration_sec,
                   release_year = excluded.release_year,
                   music_video_type = excluded.music_video_type,
                   updated_at = excluded.updated_at",
                params![
                    track.video_id,
                    track.title,
                    track.artists_json,
                    track.album,
                    track.duration_sec.map(i64::from),
                    track.release_year.map(i64::from),
                    music_video_type_to_db(track.music_video_type),
                    track.updated_at,
                ],
            )?;
            Ok(())
        })
        .await
    }

    async fn get_artist_genre(
        &self,
        artist_name_norm: &str,
    ) -> Result<Option<CachedArtistGenre>, StoreError> {
        let key = artist_name_norm.to_string();
        self.with_conn_blocking(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT artist_name_norm, raw_tags_json, primary_genre, source, updated_at
                 FROM artist_genres WHERE artist_name_norm = ?1",
            )?;
            let row = stmt
                .query_row(params![key], |r| {
                    let tags_raw: String = r.get(1)?;
                    let raw_tags =
                        serde_json::from_str::<Vec<String>>(&tags_raw).unwrap_or_default();
                    Ok(CachedArtistGenre {
                        artist_name_norm: r.get(0)?,
                        raw_tags,
                        primary_genre: r.get(2)?,
                        source: r.get(3)?,
                        updated_at: r.get(4)?,
                    })
                })
                .optional()?;
            Ok(row)
        })
        .await
    }

    async fn upsert_artist_genre(&self, entry: &CachedArtistGenre) -> Result<(), StoreError> {
        let raw_tags_json = serde_json::to_string(&entry.raw_tags)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let entry = entry.clone();
        self.with_conn_blocking(move |conn| {
            conn.execute(
                "INSERT INTO artist_genres (artist_name_norm, raw_tags_json, primary_genre, source, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(artist_name_norm) DO UPDATE SET
                   raw_tags_json = excluded.raw_tags_json,
                   primary_genre = excluded.primary_genre,
                   source = excluded.source,
                   updated_at = excluded.updated_at",
                params![
                    entry.artist_name_norm,
                    raw_tags_json,
                    entry.primary_genre,
                    entry.source,
                    entry.updated_at,
                ],
            )?;
            Ok(())
        })
        .await
    }

    async fn get_track_enrichment(
        &self,
        cache_key: &str,
    ) -> Result<Option<CachedTrackEnrichment>, StoreError> {
        let key = cache_key.to_string();
        self.with_conn_blocking(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT cache_key, video_id, tags_json, primary_genre, release_year, bpm, source, updated_at
                 FROM track_enrichment WHERE cache_key = ?1",
            )?;
            let row = stmt
                .query_row(params![key], |r| {
                    let tags_raw: String = r.get(2)?;
                    let tags = serde_json::from_str::<Vec<String>>(&tags_raw).unwrap_or_default();
                    let bpm_f64: Option<f64> = r.get(5)?;
                    Ok(CachedTrackEnrichment {
                        cache_key: r.get(0)?,
                        video_id: r.get(1)?,
                        tags,
                        primary_genre: r.get(3)?,
                        release_year: r.get::<_, Option<i64>>(4)?.and_then(|v| u16::try_from(v).ok()),
                        bpm: bpm_f64.map(|b| b as f32),
                        source: r.get(6)?,
                        updated_at: r.get(7)?,
                    })
                })
                .optional()?;
            Ok(row)
        })
        .await
    }

    async fn upsert_track_enrichment(
        &self,
        entry: &CachedTrackEnrichment,
    ) -> Result<(), StoreError> {
        let tags_json = serde_json::to_string(&entry.tags)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let entry = entry.clone();
        self.with_conn_blocking(move |conn| {
            conn.execute(
                "INSERT INTO track_enrichment (cache_key, video_id, tags_json, primary_genre, release_year, bpm, source, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(cache_key) DO UPDATE SET
                   video_id = excluded.video_id,
                   tags_json = excluded.tags_json,
                   primary_genre = excluded.primary_genre,
                   release_year = excluded.release_year,
                   bpm = excluded.bpm,
                   source = excluded.source,
                   updated_at = excluded.updated_at",
                params![
                    entry.cache_key,
                    entry.video_id,
                    tags_json,
                    entry.primary_genre,
                    entry.release_year.map(i64::from),
                    entry.bpm.map(f64::from),
                    entry.source,
                    entry.updated_at,
                ],
            )?;
            Ok(())
        })
        .await
    }

    /// Insert a track into the play history.
    async fn insert_play_history(
        &self,
        video_id: &str,
        title: &str,
        artists_json: &str,
        album: Option<&str>,
        duration_sec: Option<u32>,
    ) -> Result<(), StoreError> {
        let video_id = video_id.to_string();
        let title = title.to_string();
        let artists_json = artists_json.to_string();
        let album = album.map(str::to_string);
        let duration = duration_sec.map(|d| d as i64);
        let played_at = chrono::Utc::now().timestamp_millis();
        self.with_conn_blocking(move |conn| {
            conn.execute(
                "INSERT INTO play_history (video_id, title, artists_json, album, duration_sec, played_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![video_id, title, artists_json, album, duration, played_at],
            )?;
            // Cap history at 500 entries
            conn.execute(
                "DELETE FROM play_history WHERE id NOT IN (
                    SELECT id FROM play_history ORDER BY played_at DESC, id DESC LIMIT 500
                )",
                [],
            )?;
            Ok(())
        })
        .await
    }

    /// Get recent play history entries, most recent first.
    async fn get_play_history(&self, limit: usize) -> Result<Vec<PlayHistoryEntry>, StoreError> {
        let limit = limit as i64;
        self.with_conn_blocking(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, video_id, title, artists_json, album, duration_sec, played_at
                 FROM play_history ORDER BY played_at DESC, id DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], |r| {
                Ok(PlayHistoryEntry {
                    id: r.get(0)?,
                    video_id: r.get(1)?,
                    title: r.get(2)?,
                    artists_json: r.get(3)?,
                    album: r.get(4)?,
                    duration_sec: r.get::<_, Option<i64>>(5)?.map(|d| d as u32),
                    played_at: r.get(6)?,
                })
            })?;
            let mut entries = Vec::new();
            for row in rows {
                entries.push(row?);
            }
            Ok(entries)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_sets_busy_timeout_to_5000_ms() {
        let store = SqliteStore::open_in_memory(None).expect("open in-memory store");
        let timeout: i64 = store
            .with_conn(|conn| Ok(conn.query_row("PRAGMA busy_timeout;", [], |r| r.get(0))?))
            .expect("query pragma");
        assert_eq!(timeout, 5000);
    }
}
