use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;
use ytm_cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    tracing::debug!(?cli, "parsed CLI arguments");
    println!("ytm-architect contract CLI initialized; await Wave 3 handler wiring.");
    Ok(())
}
