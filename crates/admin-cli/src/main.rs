mod commands;
mod config;

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use clap::{Parser, Subcommand};
use launcher_core::LoaderType;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name    = "mc-launcher",
    about   = "mc-launcher-template CLI — manage and launch Minecraft",
    version
)]
struct Cli {
    /// Path to launcher.config.toml
    #[arg(long, global = true, default_value = "launcher.config.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download and launch a Minecraft version
    Launch {
        /// Minecraft version (e.g. 1.21.1)
        #[arg(default_value = "1.21.1")]
        mc_version: String,

        /// Mod loader: vanilla, fabric, quilt, neoforge, forge
        #[arg(long, default_value = "vanilla")]
        loader: String,

        /// Loader version (default: latest stable)
        #[arg(long)]
        loader_version: Option<String>,

        /// RAM in MB (overrides config default)
        #[arg(long)]
        ram: Option<u32>,

        /// Use offline auth with this username (skips Microsoft login)
        #[arg(long)]
        offline: Option<String>,

        /// Skip auto-connect to the server
        #[arg(long)]
        no_connect: bool,
    },

    /// Microsoft account authentication
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
}

#[derive(Subcommand)]
enum AuthAction {
    /// Log in with a Microsoft account (opens browser)
    Login,
    /// Show current login status
    Status,
    /// Log out and remove saved credentials
    Logout,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let mut config = config::LauncherConfig::load(&cli.config)
        .with_context(|| format!("Loading config from {:?}", cli.config))?;

    match cli.command {
        Commands::Launch { mc_version, loader, loader_version, ram, offline, no_connect } => {
            let loader_type = LoaderType::from_str(&loader)
                .map_err(|e| anyhow::anyhow!(e))?;
            if no_connect {
                config.features.quick_connect = false;
            }
            commands::launch::run(mc_version, loader_type, loader_version, offline, ram, &config).await?;
        }

        Commands::Auth { action } => match action {
            AuthAction::Login  => commands::auth::login(&config).await?,
            AuthAction::Status => commands::auth::status(&config).await?,
            AuthAction::Logout => commands::auth::logout(&config).await?,
        },
    }

    Ok(())
}
