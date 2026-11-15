use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension, params};
use ytm_core::MusicVideoType;

use crate::error::StoreError;
use crate::models::{CachedArtistGenre, CachedTrack, CachedTrackEnrichment};
use crate::traits::EnrichmentCacheStore;

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
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
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
        self.with_conn(|conn| {
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
    }

    async fn upsert_cached_track(&self, track: &CachedTrack) -> Result<(), StoreError> {
        let track = track.clone();
        self.with_conn(|conn| {
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
    }

    async fn get_artist_genre(
        &self,
        artist_name_norm: &str,
    ) -> Result<Option<CachedArtistGenre>, StoreError> {
        let key = artist_name_norm.to_string();
        self.with_conn(|conn| {
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
    }

    async fn upsert_artist_genre(&self, entry: &CachedArtistGenre) -> Result<(), StoreError> {
        let raw_tags_json = serde_json::to_string(&entry.raw_tags)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let entry = entry.clone();
        self.with_conn(|conn| {
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
    }

    async fn get_track_enrichment(
        &self,
        cache_key: &str,
    ) -> Result<Option<CachedTrackEnrichment>, StoreError> {
        let key = cache_key.to_string();
        self.with_conn(|conn| {
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
    }

    async fn upsert_track_enrichment(
        &self,
        entry: &CachedTrackEnrichment,
    ) -> Result<(), StoreError> {
        let tags_json = serde_json::to_string(&entry.tags)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let entry = entry.clone();
        self.with_conn(|conn| {
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
    }
}
