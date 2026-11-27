use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use ytm_core::{LyricsPayload, PlaylistSummary};

use crate::db::SqliteStore;
use crate::error::StoreError;

pub const LATEST_SCHEMA_VERSION: i64 = 3;

const OFFLINE_CACHE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS library_cache (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    summaries_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS lyrics_cache (
    video_id TEXT PRIMARY KEY,
    browse_id TEXT,
    payload_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
"#;

pub(crate) fn migrate_offline_cache(conn: &Connection) -> Result<(), StoreError> {
    let ver: i64 = conn.query_row("PRAGMA user_version;", [], |r| r.get(0))?;
    conn.execute_batch(OFFLINE_CACHE_SQL)?;
    if ver < LATEST_SCHEMA_VERSION {
        conn.execute_batch(&format!("PRAGMA user_version = {LATEST_SCHEMA_VERSION};"))?;
    }
    Ok(())
}

impl SqliteStore {
    /// Persist library playlist summaries in SQLite for offline-first stale-while-revalidate startup.
    pub fn save_library_summaries(&self, summaries: &[PlaylistSummary]) -> Result<(), StoreError> {
        let json = serde_json::to_string(summaries)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let now = Utc::now().timestamp();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO library_cache (id, summaries_json, updated_at)
                 VALUES (1, ?1, ?2)
                 ON CONFLICT(id) DO UPDATE SET
                   summaries_json = excluded.summaries_json,
                   updated_at = excluded.updated_at",
                params![json, now],
            )?;
            Ok(())
        })
    }

    /// Load cached library playlist summaries from SQLite, if present.
    pub fn load_library_summaries(&self) -> Result<Option<Vec<PlaylistSummary>>, StoreError> {
        self.with_conn(|conn| {
            let raw: Option<String> = conn
                .query_row(
                    "SELECT summaries_json FROM library_cache WHERE id = 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(json) = raw else {
                return Ok(None);
            };
            let parsed = serde_json::from_str::<Vec<PlaylistSummary>>(&json)
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            Ok(Some(parsed))
        })
    }

    /// Persist track lyrics payload in SQLite for offline playback inspection.
    pub fn save_cached_lyrics(
        &self,
        video_id: &str,
        browse_id: Option<&str>,
        payload: &LyricsPayload,
    ) -> Result<(), StoreError> {
        let json =
            serde_json::to_string(payload).map_err(|e| StoreError::Serialization(e.to_string()))?;
        let now = Utc::now().timestamp();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO lyrics_cache (video_id, browse_id, payload_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(video_id) DO UPDATE SET
                   browse_id = excluded.browse_id,
                   payload_json = excluded.payload_json,
                   updated_at = excluded.updated_at",
                params![video_id, browse_id, json, now],
            )?;
            Ok(())
        })
    }

    /// Load cached lyrics payload for `video_id`, if present.
    pub fn load_cached_lyrics(&self, video_id: &str) -> Result<Option<LyricsPayload>, StoreError> {
        self.with_conn(|conn| {
            let raw: Option<String> = conn
                .query_row(
                    "SELECT payload_json FROM lyrics_cache WHERE video_id = ?1",
                    params![video_id],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(json) = raw else {
                return Ok(None);
            };
            let parsed = serde_json::from_str::<LyricsPayload>(&json)
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            Ok(Some(parsed))
        })
    }
}
