use std::collections::HashSet;

use chrono::Utc;
use rusqlite::params;
use ytm_core::{FetchManifest, Playlist};

use crate::db::SqliteStore;
use crate::error::StoreError;
use crate::models::CachedPlaylistRecord;

impl SqliteStore {
    /// Persist or refresh a full `Playlist` payload in local SQLite cache for instant offline search.
    pub fn upsert_cached_playlist(&self, playlist: &Playlist) -> Result<(), StoreError> {
        self.upsert_cached_playlist_inner(playlist, None, Utc::now().timestamp())
    }

    /// Persist or refresh a `CachedPlaylistRecord` including its `FetchManifest` and `fetched_at` timestamp.
    pub fn upsert_cached_playlist_record(
        &self,
        record: &CachedPlaylistRecord,
    ) -> Result<(), StoreError> {
        self.upsert_cached_playlist_inner(
            &record.playlist,
            record.manifest.as_ref(),
            record.fetched_at,
        )
    }

    fn upsert_cached_playlist_inner(
        &self,
        playlist: &Playlist,
        manifest: Option<&FetchManifest>,
        fetched_at: i64,
    ) -> Result<(), StoreError> {
        let payload_json = serde_json::to_string(playlist)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let manifest_json = manifest
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO playlist_cache (playlist_id, title, payload_json, manifest_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(playlist_id) DO UPDATE SET
                   title = excluded.title,
                   payload_json = excluded.payload_json,
                   manifest_json = excluded.manifest_json,
                   updated_at = excluded.updated_at",
                params![
                    playlist.id,
                    playlist.title,
                    payload_json,
                    manifest_json,
                    fetched_at
                ],
            )?;
            Ok(())
        })
    }

    /// Load all locally cached playlists ordered by title.
    pub fn load_all_cached_playlists(&self) -> Result<Vec<Playlist>, StoreError> {
        Ok(self
            .load_all_cached_playlist_records()?
            .into_iter()
            .map(|r| r.playlist)
            .collect())
    }

    /// Load all locally cached playlist records including `FetchManifest` and `fetched_at`.
    pub fn load_all_cached_playlist_records(
        &self,
    ) -> Result<Vec<CachedPlaylistRecord>, StoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT payload_json, manifest_json, updated_at
                 FROM playlist_cache
                 ORDER BY title ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (raw_playlist, raw_manifest, fetched_at) = row?;
                if let Ok(playlist) = serde_json::from_str::<Playlist>(&raw_playlist) {
                    let manifest = raw_manifest
                        .and_then(|json| serde_json::from_str::<FetchManifest>(&json).ok());
                    out.push(CachedPlaylistRecord {
                        playlist,
                        manifest,
                        fetched_at,
                    });
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

    /// Remove cached playlists whose IDs are no longer present in `live_playlist_ids` (preserving `MPRE...` albums).
    pub fn prune_missing_cached_playlists(
        &self,
        live_playlist_ids: &[String],
    ) -> Result<usize, StoreError> {
        let live_set: HashSet<&str> = live_playlist_ids.iter().map(String::as_str).collect();
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            if live_set.is_empty() {
                let deleted = tx.execute(
                    "DELETE FROM playlist_cache WHERE playlist_id NOT LIKE 'MPRE%'",
                    [],
                )?;
                tx.commit()?;
                return Ok(deleted);
            }

            let mut to_delete = Vec::new();
            {
                let mut stmt = tx.prepare(
                    "SELECT playlist_id FROM playlist_cache WHERE playlist_id NOT LIKE 'MPRE%'",
                )?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                for row in rows {
                    let id = row?;
                    if !live_set.contains(id.as_str()) {
                        to_delete.push(id);
                    }
                }
            }

            let mut deleted = 0usize;
            {
                let mut del_stmt =
                    tx.prepare("DELETE FROM playlist_cache WHERE playlist_id = ?1")?;
                for id in to_delete {
                    deleted += del_stmt.execute(params![id])?;
                }
            }
            tx.commit()?;
            Ok(deleted)
        })
    }

    /// Remove cached saved albums (`MPRE...`) no longer present in `live_album_ids`.
    pub fn prune_missing_cached_albums(
        &self,
        live_album_ids: &[String],
    ) -> Result<usize, StoreError> {
        let live_set: HashSet<&str> = live_album_ids.iter().map(String::as_str).collect();
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let mut to_delete = Vec::new();
            {
                let mut stmt = tx.prepare(
                    "SELECT playlist_id FROM playlist_cache WHERE playlist_id LIKE 'MPRE%'",
                )?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                for row in rows {
                    let id = row?;
                    if !live_set.contains(id.as_str()) {
                        to_delete.push(id);
                    }
                }
            }
            let mut deleted = 0usize;
            {
                let mut del_stmt =
                    tx.prepare("DELETE FROM playlist_cache WHERE playlist_id = ?1")?;
                for id in to_delete {
                    deleted += del_stmt.execute(params![id])?;
                }
            }
            tx.commit()?;
            Ok(deleted)
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

    #[test]
    fn prune_missing_playlists_preserves_albums_and_vice_versa() {
        let store = SqliteStore::open_in_memory(None).expect("open memory store");
        let mk = |id: &str, owned: bool| Playlist {
            id: id.into(),
            title: id.into(),
            description: None,
            privacy: PlaylistPrivacy::Private,
            owned,
            track_count: 0,
            tracks: Vec::new(),
        };
        store.upsert_cached_playlist(&mk("PL_LIVE", true)).unwrap();
        store.upsert_cached_playlist(&mk("PL_DEL", true)).unwrap();
        store
            .upsert_cached_playlist(&mk("MPRE_KEEP", false))
            .unwrap();
        store
            .upsert_cached_playlist(&mk("MPRE_DEL", false))
            .unwrap();

        assert_eq!(
            store
                .prune_missing_cached_playlists(&["PL_LIVE".into()])
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .prune_missing_cached_albums(&["MPRE_KEEP".into()])
                .unwrap(),
            1
        );
        let ids: Vec<String> = store
            .load_all_cached_playlists()
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, vec!["MPRE_KEEP", "PL_LIVE"]);
    }
}
