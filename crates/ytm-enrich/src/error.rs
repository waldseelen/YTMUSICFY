use thiserror::Error;
use ytm_store::StoreError;

/// Machine-readable error variants for metadata enrichment and genre taxonomy splitting.
#[derive(Debug, Error)]
pub enum EnrichError {
    #[error("cache store error: {0}")]
    Store(#[from] StoreError),

    #[error("HTTP request error for provider '{provider}': {detail}")]
    Http { provider: String, detail: String },

    #[error("rate limit exceeded for provider '{0}'")]
    RateLimited(String),

    #[error("invalid genres taxonomy TOML: {0}")]
    InvalidTaxonomy(#[from] toml::de::Error),

    #[error("failed to read taxonomy file '{path}': {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}
