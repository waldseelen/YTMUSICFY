use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use ytm_core::Playlist;

use crate::db::SqliteStore;
use crate::error::StoreError;
use crate::models::{Snapshot, SnapshotMetadata};
use crate::paths::write_atomic;
use crate::traits::SnapshotStore;

static SNAPSHOT_SEQ: AtomicU64 = AtomicU64::new(0);

fn sanitize_path_segment(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[async_trait]
impl SnapshotStore for SqliteStore {
    async fn save_snapshot(
        &self,
        playlist: &Playlist,
        reason: &str,
    ) -> Result<SnapshotMetadata, StoreError> {
        let now = Utc::now();
        let seq = SNAPSHOT_SEQ.fetch_add(1, Ordering::Relaxed);
        let safe_id = sanitize_path_segment(&playlist.id);
        let id = format!("{}_{}_{}", safe_id, now.timestamp_millis(), seq);

        let metadata = SnapshotMetadata {
            id: id.clone(),
            playlist_id: playlist.id.clone(),
            playlist_title: playlist.title.clone(),
            reason: reason.to_string(),
            created_at: now,
            item_count: playlist.tracks.len(),
        };

        let snapshot = Snapshot {
            metadata: metadata.clone(),
            playlist: playlist.clone(),
        };
        let payload_json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;

        if let Some(dir) = self.snapshots_dir.clone() {
            let file_path = dir.join(&safe_id).join(format!("{id}.json"));
            let bytes = payload_json.as_bytes().to_vec();
            tokio::task::spawn_blocking(move || write_atomic(&file_path, &bytes))
                .await
                .map_err(|e| StoreError::TaskJoin(e.to_string()))??;
        }

        let created_at_secs = now.timestamp();
        let item_count_i64 = i64::try_from(metadata.item_count).unwrap_or(i64::MAX);
        let meta_for_db = metadata.clone();
        self.with_conn_blocking(move |conn| {
            conn.execute(
                "INSERT INTO snapshots (id, playlist_id, playlist_title, reason, created_at, item_count, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    meta_for_db.id,
                    meta_for_db.playlist_id,
                    meta_for_db.playlist_title,
                    meta_for_db.reason,
                    created_at_secs,
                    item_count_i64,
                    payload_json,
                ],
            )?;
            Ok(())
        })
        .await?;

        tracing::debug!(snapshot_id = %metadata.id, playlist_id = %metadata.playlist_id, "saved snapshot");
        Ok(metadata)
    }

    async fn list_snapshots(
        &self,
        playlist_id: Option<&str>,
    ) -> Result<Vec<SnapshotMetadata>, StoreError> {
        let filter = playlist_id.map(str::to_string);
        self.with_conn_blocking(move |conn| {
            let map_row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<SnapshotMetadata> {
                let ts: i64 = r.get(4)?;
                let count: i64 = r.get(5)?;
                Ok(SnapshotMetadata {
                    id: r.get(0)?,
                    playlist_id: r.get(1)?,
                    playlist_title: r.get(2)?,
                    reason: r.get(3)?,
                    created_at: DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now),
                    item_count: usize::try_from(count).unwrap_or(0),
                })
            };

            let mut results = Vec::new();
            if let Some(pid) = filter {
                let mut stmt = conn.prepare(
                    "SELECT id, playlist_id, playlist_title, reason, created_at, item_count
                     FROM snapshots WHERE playlist_id = ?1
                     ORDER BY created_at DESC, id DESC",
                )?;
                let rows = stmt.query_map(params![pid], map_row)?;
                for row in rows {
                    results.push(row?);
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, playlist_id, playlist_title, reason, created_at, item_count
                     FROM snapshots
                     ORDER BY created_at DESC, id DESC",
                )?;
                let rows = stmt.query_map([], map_row)?;
                for row in rows {
                    results.push(row?);
                }
            }
            Ok(results)
        })
        .await
    }

    async fn load_snapshot(&self, snapshot_id: &str) -> Result<Snapshot, StoreError> {
        let key = snapshot_id.to_string();
        let payload_json: Option<String> = self
            .with_conn_blocking(move |conn| {
                let mut stmt = conn.prepare("SELECT payload_json FROM snapshots WHERE id = ?1")?;
                let row = stmt
                    .query_row(params![key], |r| r.get::<_, String>(0))
                    .optional()?;
                Ok(row)
            })
            .await?;

        let raw =
            payload_json.ok_or_else(|| StoreError::SnapshotNotFound(snapshot_id.to_string()))?;
        serde_json::from_str::<Snapshot>(&raw).map_err(|e| StoreError::Serialization(e.to_string()))
    }
}
