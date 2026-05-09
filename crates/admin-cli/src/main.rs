mod commands;
mod config;

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name    = "mc-launcher",
    about   = "mc-launcher-template CLI — manage and launch Minecraft",
    version
)]
struct Cli {
    /// Path to launcher.config.toml (default: ./launcher.config.toml)
    #[arg(long, global = true, default_value = "launcher.config.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download and launch a Minecraft version (vanilla, offline auth)
    Launch {
        /// Minecraft version to launch (e.g. 1.21.1)
        #[arg(default_value = "1.21.1")]
        mc_version: String,

        /// Username for offline mode
        #[arg(long, default_value = "Player")]
        username: String,

        /// RAM in MB (overrides launcher.config.toml default)
        #[arg(long)]
        ram: Option<u32>,

        /// Skip auto-connect to server (even if quick_connect = true in config)
        #[arg(long)]
        no_connect: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let config = config::LauncherConfig::load(&cli.config)
        .with_context(|| format!("Loading config from {:?}", cli.config))?;

    match cli.command {
        Commands::Launch { mc_version, username, ram, no_connect } => {
            let mut cfg = config;
            if no_connect {
                cfg.features.quick_connect = false;
            }
            commands::launch::run(mc_version, Some(username), ram, &cfg).await?;
        }
    }

    Ok(())
}
