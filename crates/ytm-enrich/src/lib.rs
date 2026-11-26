pub mod error;
pub mod genre_split;
pub mod models;
pub mod normalize;
pub mod providers;
pub mod rate_limit;
pub mod splitter;
pub mod taxonomy;
pub mod traits;

pub use error::EnrichError;
pub use genre_split::plan_genre_split;
pub use models::{
    EnrichedTrackMetadata, EnrichmentConfig, EnrichmentSource, GenreBucket, GenreRule,
    GenreSplitPlan, GenreTaxonomyConfig,
};
pub use normalize::build_cache_key;
pub use providers::{EnrichmentHttp, MockEnrichmentHttp, ReqwestEnrichmentHttp};
pub use rate_limit::PacedLimiter;
pub use splitter::EnrichmentService;
pub use taxonomy::GenreMatcher;
pub use traits::MetadataProvider;
