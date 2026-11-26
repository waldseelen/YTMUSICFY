use rusqlite::{OptionalExtension, params};

use crate::db::SqliteStore;
use crate::error::StoreError;
use crate::models::PersistedPlaybackSession;

impl SqliteStore {
    /// Persist or update the singleton playback session state in SQLite.
    pub fn save_playback_session(
        &self,
        session: &PersistedPlaybackSession,
    ) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO playback_session (id, session_json, updated_at)
                 VALUES (1, ?1, ?2)
                 ON CONFLICT(id) DO UPDATE SET
                   session_json = excluded.session_json,
                   updated_at = excluded.updated_at",
                params![session.session_json, session.updated_at],
            )?;
            Ok(())
        })
    }

    /// Load the previously persisted playback session state, if present.
    pub fn load_playback_session(&self) -> Result<Option<PersistedPlaybackSession>, StoreError> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT session_json, updated_at FROM playback_session WHERE id = 1")?;
            let row = stmt
                .query_row([], |r| {
                    Ok(PersistedPlaybackSession {
                        session_json: r.get(0)?,
                        updated_at: r.get(1)?,
                    })
                })
                .optional()?;
            Ok(row)
        })
    }
}
