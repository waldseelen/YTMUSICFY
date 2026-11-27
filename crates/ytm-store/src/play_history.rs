use rusqlite::{Connection, params};

use crate::error::StoreError;

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

pub(crate) fn insert_play_history_sync(
    conn: &Connection,
    video_id: &str,
    title: &str,
    artists_json: &str,
    album: Option<&str>,
    duration_sec: Option<u32>,
    played_at: i64,
) -> Result<(), StoreError> {
    let duration = duration_sec.map(|d| d as i64);
    conn.execute(
        "INSERT INTO play_history (video_id, title, artists_json, album, duration_sec, played_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![video_id, title, artists_json, album, duration, played_at],
    )?;
    conn.execute(
        "DELETE FROM play_history WHERE id NOT IN (
            SELECT id FROM play_history ORDER BY played_at DESC, id DESC LIMIT 500
        )",
        [],
    )?;
    Ok(())
}

pub(crate) fn get_play_history_sync(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<PlayHistoryEntry>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, video_id, title, artists_json, album, duration_sec, played_at
         FROM play_history ORDER BY played_at DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |r| {
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
}

#[cfg(test)]
mod tests {
    use crate::db::SqliteStore;

    #[test]
    fn open_sets_busy_timeout_to_5000_ms() {
        let store = SqliteStore::open_in_memory(None).expect("open in-memory store");
        let timeout: i64 = store
            .with_conn(|conn| Ok(conn.query_row("PRAGMA busy_timeout;", [], |r| r.get(0))?))
            .expect("query pragma");
        assert_eq!(timeout, 5000);
    }
}
