use thiserror::Error;

/// Machine-readable error variants for playlist algorithms (dedupe, sort, set algebra, health).
#[derive(Debug, Error)]
pub enum OpsError {
    #[error("invalid sort key specification '{0}'")]
    InvalidSortSpec(String),

    #[error("invalid set algebra operation '{0}' (expected diff, intersect, or merge)")]
    InvalidSetOp(String),

    #[error("invalid keeper strategy '{0}' (expected quality, oldest, or newest)")]
    InvalidKeeperStrategy(String),

    #[error("playlist track '{title}' is missing setVideoId required for mutation")]
    MissingSetVideoId { title: String },

    #[error("fuzzy threshold must be between 0.0 and 1.0, got {0}")]
    InvalidThreshold(f64),
}
