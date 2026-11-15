pub mod dedupe;
pub mod error;
pub mod health;
pub mod models;
pub mod normalize;
pub mod set_ops;
pub mod sort;

pub use dedupe::{keeper_score, plan_dedupe};
pub use error::OpsError;
pub use health::scan_playlist_health;
pub use models::{
    DedupeConfig, DedupePlan, DiffPlan, DuplicateCluster, HealthIssue, HealthIssueKind,
    HealthReport, KeeperStrategy, MatchMode, SetAlgebraOp, SortDirection, SortField, SortKey,
    SortPlan, SortSpec, UpgradeCandidate,
};
pub use normalize::{NormalizedTrack, normalize_track};
pub use set_ops::plan_set_op;
pub use sort::{apply_move_actions, compute_minimal_moves, plan_sort};
