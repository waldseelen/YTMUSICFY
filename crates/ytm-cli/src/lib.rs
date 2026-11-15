pub mod cli;
pub mod config;
pub mod mutations;
pub mod render;
pub mod restore;
pub mod runner;
pub mod tui;
pub mod tui_render;

pub use cli::{
    AuthCommands, Cli, CliExportFormat, CliKeepStrategy, CliSetOp, Commands, DedupeArgs,
    HealthArgs, SetArgs, SnapshotCommands, SortArgs, SplitGenreArgs,
};
pub use config::{AppConfig, load_app_config, load_genre_matcher};
pub use runner::{
    LiveSessionVerifier, RuntimeContext, SessionVerifier, build_tui_app, execute_command,
    import_and_verify_auth, load_playlist_by_id,
};
pub use tui::{ActiveTab, TuiAction, TuiApp, run_tui};
