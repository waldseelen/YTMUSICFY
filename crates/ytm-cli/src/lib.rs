pub mod cli;

// Wave 3 dependencies pinned for Worker-CLI-TUI implementation.
use {
    anyhow as _, async_trait as _, chrono as _, comfy_table as _, crossterm as _, indicatif as _,
    ratatui as _, serde as _, serde_json as _, tokio as _, toml as _, tracing as _,
    tracing_subscriber as _, ytm_core as _, ytm_enrich as _, ytm_ops as _, ytm_store as _,
};

pub use cli::{
    AuthCommands, Cli, CliExportFormat, CliKeepStrategy, CliSetOp, Commands, DedupeArgs,
    HealthArgs, SetArgs, SnapshotCommands, SortArgs, SplitGenreArgs,
};
