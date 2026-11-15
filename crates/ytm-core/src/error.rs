use thiserror::Error;

/// Machine-readable error variants for YouTube Music InnerTube core operations.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("authentication expired or logged-out payload detected")]
    AuthExpired,

    #[error("missing required auth cookie (__Secure-3PAPISID or SAPISID)")]
    MissingCookie,

    #[error("invalid authentication input: {0}")]
    InvalidAuthInput(String),

    #[error("missing setVideoId for playlist mutation on video '{video_id}'")]
    MissingSetVideoId { video_id: String },

    #[error("pre-mutation snapshot is required before mutating playlist '{playlist_id}'")]
    PreMutationSnapshotRequired { playlist_id: String },

    #[error("HTTP transport error: {0}")]
    HttpError(String),

    #[error("rate limited by InnerTube (status {status})")]
    RateLimited {
        status: u16,
        retry_after_ms: Option<u64>,
    },

    #[error("failed to parse InnerTube response at '{context}': {detail}")]
    ParseError { context: String, detail: String },

    #[error("InnerTube API returned failure ({code}): {message}")]
    ApiError { code: String, message: String },

    #[error("invalid playlist input: {0}")]
    InvalidInput(String),
}
