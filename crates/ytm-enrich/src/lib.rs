pub mod error;
pub mod models;
pub mod providers;
pub mod rate_limit;
pub mod splitter;
pub mod taxonomy;
pub mod traits;

pub use error::EnrichError;
pub use models::{
    EnrichedTrackMetadata, EnrichmentConfig, EnrichmentSource, GenreBucket, GenreRule,
    GenreSplitPlan, GenreTaxonomyConfig,
};
pub use providers::{EnrichmentHttp, MockEnrichmentHttp, ReqwestEnrichmentHttp};
pub use rate_limit::PacedLimiter;
pub use splitter::{EnrichmentService, plan_genre_split};
pub use taxonomy::GenreMatcher;
pub use traits::MetadataProvider;
