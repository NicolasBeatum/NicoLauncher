use anyhow::Context;

use launcher_auth::AuthClient;
use launcher_core::LauncherPaths;

use crate::config::LauncherConfig;

pub async fn login(config: &LauncherConfig) -> anyhow::Result<()> {
    let client_id = config.auth.microsoft_client_id.trim();
    if client_id.is_empty() {
        anyhow::bail!(
            "Microsoft client_id not set.\n\
             Edit launcher.config.toml → [auth] microsoft_client_id = \"...\"\n\
             Instructions: docs/customization-guide.md#microsoft-oauth"
        );
    }

    let paths = LauncherPaths::new(&config.branding.internal_id)?;
    paths.ensure_all().await?;

    let auth = AuthClient::new(client_id).context("Creating auth client")?;
    let session = auth
        .login(&paths.root.join("account.json"))
        .await
        .context("Microsoft login")?;

    println!("\n✓ Logged in as: {} ({})", session.username, session.uuid);
    Ok(())
}

pub async fn status(config: &LauncherConfig) -> anyhow::Result<()> {
    let paths = LauncherPaths::new(&config.branding.internal_id)?;
    let account_json = paths.root.join("account.json");

    match launcher_auth::storage::load_account(&account_json).await {
        Some(meta) => {
            println!("Logged in as:  {}", meta.username);
            println!("UUID:          {}", meta.uuid);
            if meta.is_expired() {
                println!("Session:       expired (will refresh on next launch)");
            } else {
                println!("Session:       valid until {}", meta.expires_at.format("%Y-%m-%d %H:%M UTC"));
            }
        }
        None => {
            println!("Not logged in. Run `mc-launcher auth login`.");
        }
    }
    Ok(())
}

pub async fn logout(config: &LauncherConfig) -> anyhow::Result<()> {
    let client_id = config.auth.microsoft_client_id.trim();
    let paths = LauncherPaths::new(&config.branding.internal_id)?;

    let auth = AuthClient::new(if client_id.is_empty() { "placeholder" } else { client_id })
        .context("Creating auth client")?;
    auth.logout(&paths.root.join("account.json"))
        .await
        .context("Logout")?;

    println!("✓ Logged out.");
    Ok(())
}
