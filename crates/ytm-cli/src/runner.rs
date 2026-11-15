use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ytm_core::{
    AuthCredentials, ClientConfig, InnerTubeClient, InnerTubeClientTrait, Playlist,
    ReqwestTransport, SessionStatus, parse_auth_input,
};
use ytm_enrich::MetadataProvider;
use ytm_store::{
    AppPaths, EnrichmentCacheStore, ExportFormat, SnapshotStore, export_playlist_to_path,
    write_secret_file,
};

use crate::cli::{AuthCommands, Commands, SnapshotCommands};
use crate::config::AppConfig;
use crate::mutations::{handle_dedupe, handle_health, handle_set, handle_sort};
use crate::render::{
    make_progress_bar, render_inspect_table, render_playlists_table, render_snapshot_detail,
    render_snapshots_table,
};
use crate::restore::{handle_snapshot_restore, handle_split_genre};
use crate::tui::TuiApp;

/// Verifies browser credentials against InnerTube (real network in CLI, mockable in tests).
#[async_trait]
pub trait SessionVerifier: Send + Sync {
    async fn verify_credentials(
        &self,
        credentials: &AuthCredentials,
        config: &ClientConfig,
    ) -> Result<SessionStatus>;
}

/// Live `ReqwestTransport` session verifier used by `ytm auth import`.
#[derive(Debug, Default)]
pub struct LiveSessionVerifier;

#[async_trait]
impl SessionVerifier for LiveSessionVerifier {
    async fn verify_credentials(
        &self,
        credentials: &AuthCredentials,
        config: &ClientConfig,
    ) -> Result<SessionStatus> {
        let transport = ReqwestTransport::new(config.max_retries)?;
        let client = InnerTubeClient::new(transport, credentials.clone(), config.clone());
        Ok(client.verify_session().await?)
    }
}

/// Shared dependency container passed into every CLI subcommand handler.
pub struct RuntimeContext<'a, C, S, P, V, W> {
    pub client: &'a C,
    pub store: &'a S,
    pub enricher: &'a P,
    pub verifier: &'a V,
    pub paths: &'a AppPaths,
    pub config: &'a AppConfig,
    pub raw_auth_input: Option<String>,
    pub out: &'a mut W,
}

/// Loads `"LM"` via `get_liked_songs` or any playlist by ID via `get_playlist`.
pub async fn load_playlist_by_id<C: InnerTubeClientTrait>(
    client: &C,
    id_or_lm: &str,
) -> Result<Playlist> {
    if id_or_lm.eq_ignore_ascii_case("LM") {
        Ok(client.get_liked_songs().await?)
    } else {
        Ok(client.get_playlist(id_or_lm).await?)
    }
}

/// Parses, verifies, and persists credentials with `0600` secret permissions.
pub async fn import_and_verify_auth<V: SessionVerifier>(
    raw_input: &str,
    verifier: &V,
    paths: &AppPaths,
    config: &AppConfig,
) -> Result<SessionStatus> {
    let creds = parse_auth_input(raw_input).context("parsing browser auth input")?;
    let status = verifier
        .verify_credentials(&creds, &config.innertube)
        .await?;
    if !status.authenticated {
        bail!("InnerTube session canary rejected credentials (not authenticated)");
    }
    paths.ensure_dirs()?;
    let bytes = serde_json::to_vec_pretty(&creds)?;
    write_secret_file(&paths.auth_file(), &bytes)?;
    Ok(status)
}

/// Builds an initial `TuiApp` populated from library playlists and local snapshots.
pub async fn build_tui_app<C, S>(client: &C, store: &S) -> Result<TuiApp>
where
    C: InnerTubeClientTrait,
    S: SnapshotStore,
{
    let playlists = client.get_library_playlists().await?;
    let snapshots = store.list_snapshots(None).await?;
    Ok(TuiApp::with_library(playlists, snapshots))
}

/// Executes any parsed CLI command against injected traits and output writer.
pub async fn execute_command<C, S, P, V, W, T>(
    cmd: &Commands,
    ctx: &mut RuntimeContext<'_, C, S, P, V, W>,
    tui_runner: T,
) -> Result<()>
where
    C: InnerTubeClientTrait,
    S: SnapshotStore + EnrichmentCacheStore,
    P: MetadataProvider,
    V: SessionVerifier,
    W: Write,
    T: FnOnce(TuiApp) -> Result<()>,
{
    match cmd {
        Commands::Auth { action } => match action {
            AuthCommands::Import { file } => {
                let raw = if let Some(path) = file {
                    fs::read_to_string(path)
                        .with_context(|| format!("reading auth file {}", path.display()))?
                } else if let Some(inline) = &ctx.raw_auth_input {
                    inline.clone()
                } else {
                    bail!("provide --file <path> or pipe credentials via stdin");
                };
                let status =
                    import_and_verify_auth(&raw, ctx.verifier, ctx.paths, ctx.config).await?;
                writeln!(
                    ctx.out,
                    "Authenticated as {} (handle={}, authuser={}, page_id={}) -> saved {}",
                    status.account_name.as_deref().unwrap_or("unknown"),
                    status.channel_handle.as_deref().unwrap_or("-"),
                    status.auth_user,
                    status.brand_page_id.as_deref().unwrap_or("-"),
                    ctx.paths.auth_file().display()
                )?;
            }
            AuthCommands::Status => {
                let status = ctx.client.verify_session().await?;
                writeln!(
                    ctx.out,
                    "authenticated={} | account={} | handle={} | authuser={} | page_id={}",
                    status.authenticated,
                    status.account_name.as_deref().unwrap_or("-"),
                    status.channel_handle.as_deref().unwrap_or("-"),
                    status.auth_user,
                    status.brand_page_id.as_deref().unwrap_or("-")
                )?;
            }
        },
        Commands::List { json } => {
            let playlists = ctx.client.get_library_playlists().await?;
            if *json {
                writeln!(ctx.out, "{}", serde_json::to_string_pretty(&playlists)?)?;
            } else {
                writeln!(ctx.out, "{}", render_playlists_table(&playlists))?;
            }
        }
        Commands::Inspect { playlist, json } => {
            let pl = load_playlist_by_id(ctx.client, playlist).await?;
            if *json {
                writeln!(ctx.out, "{}", serde_json::to_string_pretty(&pl)?)?;
            } else {
                writeln!(ctx.out, "{}", render_inspect_table(&pl))?;
            }
        }
        Commands::Backup {
            target,
            format,
            out,
        } => {
            handle_backup(ctx, target, (*format).into(), out.as_deref()).await?;
        }
        Commands::Snapshots { action } => match action {
            SnapshotCommands::List { playlist } => {
                let snaps = ctx.store.list_snapshots(playlist.as_deref()).await?;
                writeln!(ctx.out, "{}", render_snapshots_table(&snaps))?;
            }
            SnapshotCommands::Show { snapshot_id } => {
                let snap = ctx.store.load_snapshot(snapshot_id).await?;
                writeln!(ctx.out, "{}", render_snapshot_detail(&snap))?;
            }
            SnapshotCommands::Restore {
                snapshot_id,
                execute,
            } => {
                handle_snapshot_restore(ctx, snapshot_id, *execute).await?;
            }
        },
        Commands::Dedupe(args) => handle_dedupe(ctx, args).await?,
        Commands::Sort(args) => handle_sort(ctx, args).await?,
        Commands::Set(args) => handle_set(ctx, args).await?,
        Commands::Health(args) => handle_health(ctx, args).await?,
        Commands::SplitGenre(args) => handle_split_genre(ctx, args).await?,
        Commands::Tui => {
            let app = build_tui_app(ctx.client, ctx.store).await?;
            tui_runner(app)?;
        }
    }
    Ok(())
}

async fn handle_backup<C, S, P, V, W>(
    ctx: &mut RuntimeContext<'_, C, S, P, V, W>,
    target: &str,
    format: ExportFormat,
    out_dir: Option<&Path>,
) -> Result<()>
where
    C: InnerTubeClientTrait,
    S: SnapshotStore,
    W: Write,
{
    let targets = if target.eq_ignore_ascii_case("all") {
        ctx.client
            .get_library_playlists()
            .await?
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>()
    } else {
        vec![target.to_string()]
    };

    let pb = make_progress_bar(targets.len() as u64, "Backing up playlists");
    let ext = match format {
        ExportFormat::Json => "json",
        ExportFormat::Csv => "csv",
        ExportFormat::M3u8 => "m3u8",
    };

    for id in &targets {
        let playlist = load_playlist_by_id(ctx.client, id).await?;
        let meta = ctx.store.save_snapshot(&playlist, "backup").await?;
        writeln!(
            ctx.out,
            "Snapshot {} saved for '{}' ({} tracks).",
            meta.id, meta.playlist_title, meta.item_count
        )?;
        if let Some(dir) = out_dir {
            let safe_id: String = playlist
                .id
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let file_path = dir.join(format!("{safe_id}.{ext}"));
            export_playlist_to_path(&playlist, &file_path, format)?;
            writeln!(ctx.out, "Exported {}", file_path.display())?;
        }
        pb.inc(1);
    }
    pb.finish_and_clear();
    Ok(())
}
