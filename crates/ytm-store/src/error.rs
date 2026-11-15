use thiserror::Error;

/// Machine-readable error variants for local SQLite persistence, paths, and exports.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("failed to resolve platform directories via ProjectDirs")]
    ProjectDirUnavailable,

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("I/O error at '{path}': {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("snapshot '{0}' not found")]
    SnapshotNotFound(String),

    #[error("invalid export format '{0}' (expected json, csv, or m3u8)")]
    InvalidExportFormat(String),
}
