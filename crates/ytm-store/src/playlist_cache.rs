use chrono::Utc;
use rusqlite::params;
use ytm_core::Playlist;

use crate::db::SqliteStore;
use crate::error::StoreError;

impl SqliteStore {
    /// Persist or refresh a full `Playlist` payload in local SQLite cache for instant offline search.
    pub fn upsert_cached_playlist(&self, playlist: &Playlist) -> Result<(), StoreError> {
        let payload_json = serde_json::to_string(playlist)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let now = Utc::now().timestamp();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO playlist_cache (playlist_id, title, payload_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(playlist_id) DO UPDATE SET
                   title = excluded.title,
                   payload_json = excluded.payload_json,
                   updated_at = excluded.updated_at",
                params![playlist.id, playlist.title, payload_json, now],
            )?;
            Ok(())
        })
    }

    /// Load all locally cached playlists ordered by title.
    pub fn load_all_cached_playlists(&self) -> Result<Vec<Playlist>, StoreError> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT payload_json FROM playlist_cache ORDER BY title ASC")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                let raw = row?;
                if let Ok(pl) = serde_json::from_str::<Playlist>(&raw) {
                    out.push(pl);
                }
            }
            Ok(out)
        })
    }

    /// Remove a deleted or invalidated playlist from the local SQLite repertoire cache.
    pub fn delete_cached_playlist(&self, playlist_id: &str) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM playlist_cache WHERE playlist_id = ?1",
                params![playlist_id],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use ytm_core::{MusicVideoType, PlaylistPrivacy, Track, TrackAvailability};

    use super::*;

    #[test]
    fn playlist_cache_upsert_load_and_delete_roundtrips() {
        let store = SqliteStore::open_in_memory(None).expect("open memory store");
        let pl = Playlist {
            id: "PL_TEST".to_string(),
            title: "Electronic Mix".to_string(),
            description: Some("Daft Punk & friends".to_string()),
            privacy: PlaylistPrivacy::Private,
            owned: true,
            track_count: 1,
            tracks: vec![Track {
                video_id: "vid1".to_string(),
                set_video_id: Some("sid1".to_string()),
                position: 0,
                title: "Around the World".to_string(),
                artists: Vec::new(),
                album: None,
                duration_sec: Some(429),
                release_year: Some(1997),
                music_video_type: MusicVideoType::AudioTrack,
                availability: TrackAvailability::Available,
                thumbnail_url: None,
            }],
        };

        store.upsert_cached_playlist(&pl).expect("upsert");
        let loaded = store.load_all_cached_playlists().expect("load all");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "PL_TEST");
        assert_eq!(loaded[0].tracks[0].title, "Around the World");

        store.delete_cached_playlist("PL_TEST").expect("delete");
        assert!(
            store
                .load_all_cached_playlists()
                .expect("empty load")
                .is_empty()
        );
    }
}
