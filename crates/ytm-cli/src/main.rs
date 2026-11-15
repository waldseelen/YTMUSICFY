use std::fs;
use std::io::{self, Read};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::Parser;
use tracing_subscriber::EnvFilter;
use ytm_cli::{
    AuthCommands, Cli, Commands, LiveSessionVerifier, RuntimeContext, execute_command,
    import_and_verify_auth, load_app_config, load_genre_matcher, run_tui,
};
use ytm_core::{AuthCredentials, InnerTubeClient, ReqwestTransport};
use ytm_enrich::{EnrichmentService, ReqwestEnrichmentHttp};
use ytm_store::{AppPaths, SqliteStore};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let paths = AppPaths::from_project_dirs()?;
    paths.ensure_dirs()?;
    let config = load_app_config(&paths)?;
    let verifier = LiveSessionVerifier;

    // Handle `auth import` before requiring existing saved `auth.json`.
    if let Commands::Auth {
        action: AuthCommands::Import { ref file },
    } = cli.command
    {
        let raw = if let Some(path) = file {
            fs::read_to_string(path)
                .with_context(|| format!("reading auth file {}", path.display()))?
        } else {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .context("reading auth credentials from stdin")?;
            buf
        };
        let status = import_and_verify_auth(&raw, &verifier, &paths, &config).await?;
        println!(
            "Authenticated as {} (handle={}, authuser={}, page_id={}) -> saved {}",
            status.account_name.as_deref().unwrap_or("unknown"),
            status.channel_handle.as_deref().unwrap_or("-"),
            status.auth_user,
            status.brand_page_id.as_deref().unwrap_or("-"),
            paths.auth_file().display()
        );
        return Ok(());
    }

    let auth_path = paths.auth_file();
    if !auth_path.exists() {
        bail!(
            "No saved credentials at {}. Run `ytm auth import --file <browser.json>` first.",
            auth_path.display()
        );
    }

    let auth_bytes = fs::read(&auth_path)
        .with_context(|| format!("reading credentials from {}", auth_path.display()))?;
    let creds: AuthCredentials = serde_json::from_slice(&auth_bytes)
        .with_context(|| format!("decoding credentials JSON at {}", auth_path.display()))?;

    let transport = ReqwestTransport::new(config.innertube.max_retries)?;
    let client = InnerTubeClient::new(transport, creds, config.innertube.clone());
    let store = Arc::new(SqliteStore::open(
        &paths.db_file(),
        Some(paths.snapshots_dir()),
    )?);

    let custom_taxonomy = match &cli.command {
        Commands::SplitGenre(args) => args.config.as_deref(),
        _ => None,
    };
    let matcher = load_genre_matcher(custom_taxonomy, &paths)?;
    let enrich_http = ReqwestEnrichmentHttp::new()?;
    let enricher = EnrichmentService::new(
        Arc::clone(&store),
        enrich_http,
        config.enrichment.clone(),
        matcher,
    );

    let mut stdout = io::stdout();
    let mut ctx = RuntimeContext {
        client: &client,
        store: store.as_ref(),
        enricher: &enricher,
        verifier: &verifier,
        paths: &paths,
        config: &config,
        raw_auth_input: None,
        out: &mut stdout,
    };

    execute_command(&cli.command, &mut ctx, run_tui).await
}
