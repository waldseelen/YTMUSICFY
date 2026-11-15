pub mod error;
pub mod models;
pub mod traits;

// Wave 2 dependencies pinned for Worker-Enrich implementation.
use {chrono as _, reqwest as _, serde_json as _, tokio as _, tracing as _};

pub use error::EnrichError;
pub use models::{
    EnrichedTrackMetadata, EnrichmentConfig, EnrichmentSource, GenreBucket, GenreRule,
    GenreSplitPlan, GenreTaxonomyConfig,
};
pub use traits::MetadataProvider;
