use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Top-level CLI for `ytm-architect`.
#[derive(Debug, Parser)]
#[command(
    name = "ytm",
    version,
    about = "Local-first YouTube Music library & playlist engineering suite"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Manage InnerTube browser credentials and check session health.
    Auth {
        #[command(subcommand)]
        action: AuthCommands,
    },
    /// List all library playlists and Liked Music (LM) with track counts.
    List {
        /// Output machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Inspect playlist tracks, musicVideoType classification, and availability.
    Inspect {
        /// Playlist ID or 'LM' for Liked Music.
        playlist: String,
        /// Output machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Snapshot a playlist (or 'LM' or 'all') to SQLite and optional file export.
    Backup {
        /// Playlist ID, 'LM', or 'all'.
        target: String,
        /// Export format (json, csv, or m3u8).
        #[arg(long, value_enum, default_value_t = CliExportFormat::Json)]
        format: CliExportFormat,
        /// Optional directory to export playlist files into.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// List, inspect, or restore local pre-mutation playlist snapshots.
    Snapshots {
        #[command(subcommand)]
        action: SnapshotCommands,
    },
    /// Detect and remove exact or smart fuzzy duplicates (dry-run by default).
    Dedupe(DedupeArgs),
    /// Sort a playlist using minimal LIS moves (dry-run by default).
    Sort(SortArgs),
    /// Compute set algebra (diff, intersect, merge) across two playlists.
    Set(SetArgs),
    /// Scan playlists for unavailable tracks and optional UGC -> ATV upgrades.
    Health(HealthArgs),
    /// Enrich track genres and split playlist idempotently by genre (dry-run by default).
    SplitGenre(SplitGenreArgs),
    /// Launch the interactive Ratatui dashboard.
    Tui,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    /// Import raw DevTools headers, Cookie header, or browser.json and verify session.
    Import {
        /// Optional path to headers/cookies/browser.json file (reads stdin if omitted).
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Verify active InnerTube session against canary endpoint.
    Status,
}

#[derive(Debug, Subcommand)]
pub enum SnapshotCommands {
    /// List saved local snapshots.
    List {
        /// Optional playlist ID filter.
        #[arg(long)]
        playlist: Option<String>,
    },
    /// Show detailed contents of a local snapshot.
    Show { snapshot_id: String },
    /// Restore a playlist state from a local snapshot (dry-run unless --execute).
    Restore {
        snapshot_id: String,
        /// Apply remote changes after writing a fresh pre-restore snapshot.
        #[arg(long, default_value_t = false)]
        execute: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliExportFormat {
    Json,
    Csv,
    M3u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliKeepStrategy {
    Quality,
    Oldest,
    Newest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliSetOp {
    Diff,
    Intersect,
    Merge,
}

#[derive(Debug, Args)]
pub struct DedupeArgs {
    /// Target playlist ID.
    pub playlist: String,
    /// Jaro-Winkler similarity threshold (0.0 - 1.0).
    #[arg(long, default_value_t = 0.90)]
    pub threshold: f64,
    /// Maximum duration difference in seconds for fuzzy matches.
    #[arg(long, default_value_t = 15)]
    pub duration_tol: u32,
    /// Ignore duration differences when matching duplicates.
    #[arg(long, default_value_t = false)]
    pub ignore_duration: bool,
    /// Only check exact videoId duplicates.
    #[arg(long, default_value_t = false)]
    pub exact_only: bool,
    /// Keeper selection strategy when collapsing duplicates.
    #[arg(long, value_enum, default_value_t = CliKeepStrategy::Quality)]
    pub keep: CliKeepStrategy,
    /// Execute remote deletions (default is safe dry-run preview).
    #[arg(long, default_value_t = false)]
    pub execute: bool,
}

#[derive(Debug, Args)]
pub struct SortArgs {
    /// Target playlist ID.
    pub playlist: String,
    /// Comma-separated sort keys (e.g., artist:asc,year:asc,title:asc or shuffle).
    #[arg(long)]
    pub by: String,
    /// Optional deterministic RNG seed when sorting with 'shuffle'.
    #[arg(long)]
    pub seed: Option<u64>,
    /// Execute remote moves (default is safe dry-run preview).
    #[arg(long, default_value_t = false)]
    pub execute: bool,
}

#[derive(Debug, Args)]
pub struct SetArgs {
    /// Set algebra operation (diff, intersect, merge).
    #[arg(value_enum)]
    pub op: CliSetOp,
    /// First source playlist ID or 'LM'.
    pub playlist_a: String,
    /// Second source playlist ID or 'LM'.
    pub playlist_b: String,
    /// Match tracks using normalized artist/title fuzzy matching instead of exact videoId.
    #[arg(long, default_value_t = false)]
    pub fuzzy: bool,
    /// Optional target playlist ID to insert resulting tracks into.
    #[arg(long)]
    pub into: Option<String>,
    /// Execute remote additions into --into playlist (default is dry-run preview).
    #[arg(long, default_value_t = false)]
    pub execute: bool,
}

#[derive(Debug, Args)]
pub struct HealthArgs {
    /// Playlist ID or 'all'.
    pub target: String,
    /// Search InnerTube for official ATV replacements for UGC/OMV items.
    #[arg(long, default_value_t = false)]
    pub find_official: bool,
    /// Remove unavailable/deleted/private entries when --execute is set.
    #[arg(long, default_value_t = false)]
    pub prune_unavailable: bool,
    /// Swap eligible UGC/OMV items with official ATV candidates when --execute is set.
    #[arg(long, default_value_t = false)]
    pub upgrade: bool,
    /// Execute remote pruning/upgrades (default is dry-run preview).
    #[arg(long, default_value_t = false)]
    pub execute: bool,
}

#[derive(Debug, Args)]
pub struct SplitGenreArgs {
    /// Source playlist ID or 'LM'.
    pub playlist: String,
    /// Path to custom genres.toml taxonomy mapping.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Minimum tracks required to emit a standalone genre playlist.
    #[arg(long, default_value_t = 5)]
    pub min_tracks: usize,
    /// Execute playlist creation/population (default is dry-run preview).
    #[arg(long, default_value_t = false)]
    pub execute: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutating_subcommands_default_to_dry_run() {
        let dedupe = Cli::try_parse_from(["ytm", "dedupe", "PL123"]).expect("parse dedupe");
        match dedupe.command {
            Commands::Dedupe(args) => {
                assert!(!args.execute);
                assert!((args.threshold - 0.90).abs() < f64::EPSILON);
                assert_eq!(args.duration_tol, 15);
            }
            other => panic!("unexpected command: {other:?}"),
        }

        let sort = Cli::try_parse_from(["ytm", "sort", "PL123", "--by", "artist:asc"])
            .expect("parse sort");
        match sort.command {
            Commands::Sort(args) => assert!(!args.execute),
            other => panic!("unexpected command: {other:?}"),
        }

        let set = Cli::try_parse_from(["ytm", "set", "diff", "LM", "PL123", "--into", "PL_OUT"])
            .expect("parse set");
        match set.command {
            Commands::Set(args) => assert!(!args.execute),
            other => panic!("unexpected command: {other:?}"),
        }

        let health =
            Cli::try_parse_from(["ytm", "health", "PL123", "--prune-unavailable"]).expect("health");
        match health.command {
            Commands::Health(args) => assert!(!args.execute),
            other => panic!("unexpected command: {other:?}"),
        }

        let split = Cli::try_parse_from(["ytm", "split-genre", "LM"]).expect("split-genre");
        match split.command {
            Commands::SplitGenre(args) => {
                assert!(!args.execute);
                assert_eq!(args.min_tracks, 5);
            }
            other => panic!("unexpected command: {other:?}"),
        }

        let restore =
            Cli::try_parse_from(["ytm", "snapshots", "restore", "snap_1"]).expect("restore");
        match restore.command {
            Commands::Snapshots {
                action: SnapshotCommands::Restore { execute, .. },
            } => assert!(!execute),
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
