use rusqlite::params;

use crate::db::SqliteStore;
use crate::error::StoreError;
use crate::models::PersistedStagedOp;

impl SqliteStore {
    /// Atomically replace all persisted `SyncQueue` entries inside a single SQLite transaction.
    pub fn replace_staged_ops(&self, ops: &[PersistedStagedOp]) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            tx.execute("DELETE FROM staged_ops", [])?;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO staged_ops (entry_id, seq, selected, op_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )?;
                for op in ops {
                    let entry_id_i64 = i64::from_ne_bytes(op.entry_id.to_ne_bytes());
                    let seq_i64 = i64::try_from(op.seq).unwrap_or(i64::MAX);
                    let selected_i64 = i64::from(op.selected);
                    stmt.execute(params![
                        entry_id_i64,
                        seq_i64,
                        selected_i64,
                        op.op_json,
                        op.created_at
                    ])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// Load all persisted `SyncQueue` entries ordered by `seq` ascending.
    pub fn load_staged_ops(&self) -> Result<Vec<PersistedStagedOp>, StoreError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT entry_id, seq, selected, op_json, created_at
                 FROM staged_ops
                 ORDER BY seq ASC, entry_id ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                let raw_id: i64 = r.get(0)?;
                let raw_seq: i64 = r.get(1)?;
                let raw_selected: i64 = r.get(2)?;
                Ok(PersistedStagedOp {
                    entry_id: u64::from_ne_bytes(raw_id.to_ne_bytes()),
                    seq: usize::try_from(raw_seq).unwrap_or(0),
                    selected: raw_selected != 0,
                    op_json: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Delete a single persisted staged operation by `entry_id` inside a transaction.
    pub fn delete_staged_op(&self, entry_id: u64) -> Result<(), StoreError> {
        let entry_id_i64 = i64::from_ne_bytes(entry_id.to_ne_bytes());
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            tx.execute(
                "DELETE FROM staged_ops WHERE entry_id = ?1",
                params![entry_id_i64],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Clear all persisted staged operations inside a transaction.
    pub fn clear_staged_ops(&self) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            tx.execute("DELETE FROM staged_ops", [])?;
            tx.commit()?;
            Ok(())
        })
    }
}
