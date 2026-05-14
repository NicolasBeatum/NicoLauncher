mod commands;
mod config;
mod lockfile;

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use clap::{Parser, Subcommand};
use launcher_core::LoaderType;
use tracing_subscriber::EnvFilter;

use lockfile::LOCKFILE_NAME;

#[derive(Parser)]
#[command(
    name    = "mc-launcher",
    about   = "mc-launcher-template CLI — gestiona y distribuye modpacks de Minecraft",
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
    /// Descarga y lanza una versión de Minecraft
    Launch {
        /// Versión de Minecraft (ej: 1.21.1)
        #[arg(default_value = "1.21.1")]
        mc_version: String,

        /// Mod loader: vanilla, fabric, quilt, neoforge, forge
        #[arg(long, default_value = "vanilla")]
        loader: String,

        /// Versión del loader (por defecto: latest stable)
        #[arg(long)]
        loader_version: Option<String>,

        /// RAM en MB (sobreescribe el default del config)
        #[arg(long)]
        ram: Option<u32>,

        /// Autenticación offline con este nombre de usuario
        #[arg(long)]
        offline: Option<String>,

        /// Omitir auto-connect al servidor
        #[arg(long)]
        no_connect: bool,
    },

    /// Autenticación con cuenta de Microsoft
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// Sincroniza mods y configs desde el manifest del servidor
    Sync,

    /// Herramientas de manifest (lockfile.toml + generación)
    Manifest {
        #[command(subcommand)]
        action: ManifestAction,
    },

    /// Herramientas de firma para distribución del manifest
    Sign {
        #[command(subcommand)]
        action: SignAction,
    },
}

// ── Subcomandos de manifest ────────────────────────────────────────────────────

#[derive(Subcommand)]
enum ManifestAction {
    /// Asistente interactivo que crea lockfile.toml en el directorio actual.
    ///
    /// Guarda toda la configuración del modpack (versiones, carpetas, URLs) para
    /// que no tengas que repetirla en cada actualización.
    Init {
        /// Ruta al lockfile de salida
        #[arg(long, default_value = LOCKFILE_NAME)]
        lockfile: PathBuf,

        /// Sobreescribir sin preguntar si ya existe
        #[arg(short, long)]
        force: bool,
    },

    /// Lee lockfile.toml, escanea las carpetas configuradas y genera/actualiza manifest.json.
    ///
    /// Sólo consulta Modrinth para los archivos que han cambiado desde la última
    /// generación — el resto se reutiliza del manifest anterior.
    Update {
        /// Ruta al lockfile (por defecto: lockfile.toml en el directorio actual)
        #[arg(long, default_value = LOCKFILE_NAME)]
        lockfile: PathBuf,

        /// Confirmar sin preguntar (útil en CI)
        #[arg(short, long)]
        yes: bool,
    },

    /// [Legacy] Genera manifest.json directamente sin lockfile.
    ///
    /// Útil para uso puntual o scripting. Para el flujo normal usa `init` + `update`.
    Generate {
        /// Carpeta con los .jar de mods (omitir para no incluir mods)
        #[arg(short, long)]
        mods_dir: Option<PathBuf>,

        /// Carpeta con los .zip de shaderpacks (opcional)
        #[arg(long)]
        shaderpacks_dir: Option<PathBuf>,

        /// Carpeta con los .zip/.jar de resourcepacks (opcional)
        #[arg(long)]
        resourcepacks_dir: Option<PathBuf>,

        /// Versión de Minecraft (ej: 1.21.1)
        #[arg(long, default_value = "1.21.1")]
        mc_version: String,

        /// Versión mayor de Java requerida (ej: 21)
        #[arg(long, default_value_t = 21)]
        java_version: u32,

        /// Mod loader: fabric, quilt, neoforge, forge, vanilla
        #[arg(long)]
        loader: Option<String>,

        /// Versión del loader (ej: 0.16.5). Si se omite usa "latest"
        #[arg(long)]
        loader_version: Option<String>,

        /// Archivo de salida
        #[arg(short, long, default_value = "manifest.json")]
        output: PathBuf,

        /// Carpeta con configs a copiar a .minecraft/ (escaneo recursivo)
        #[arg(long)]
        configs_dir: Option<PathBuf>,

        /// URL base para archivos no encontrados en Modrinth
        #[arg(long)]
        self_hosted_url: Option<String>,

        /// Modo de aplicación para shaderpacks/resourcepacks/configs
        #[arg(long, default_value = "if_missing")]
        apply_mode: String,
    },
}

// ── Otros subcomandos ──────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum SignAction {
    /// Genera un par de claves Ed25519 para firmar manifests
    GenKeys {
        #[arg(short, long)]
        output_dir: Option<PathBuf>,
    },
    /// Firma un manifest.json con la clave privada
    Sign {
        manifest: PathBuf,
        #[arg(short, long, default_value = "signing.key")]
        key: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Verifica la firma de un manifest-signed.json
    Verify {
        manifest: PathBuf,
        #[arg(short, long, default_value = "public.key")]
        key: PathBuf,
    },
    /// Valida schema, hashes, dependencias, conflictos y URLs de un manifest
    Validate {
        /// Ruta al manifest.json o manifest-signed.json
        manifest: PathBuf,
        /// Verificar accesibilidad HTTP de todas las URLs (hace peticiones HEAD)
        #[arg(long)]
        check_urls: bool,
    },
}

#[derive(Subcommand)]
enum AuthAction {
    /// Inicia sesión con una cuenta de Microsoft (abre el navegador)
    Login,
    /// Muestra el estado de la sesión actual
    Status,
    /// Cierra la sesión y borra las credenciales guardadas
    Logout,
}

// ── Main ───────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    // Config is only needed for commands that talk to the launcher runtime.
    // Manifest/Sign commands are standalone and skip this load.
    let needs_config = matches!(
        &cli.command,
        Commands::Launch { .. } | Commands::Auth { .. } | Commands::Sync
    );
    let mut config_opt: Option<config::LauncherConfig> = if needs_config {
        Some(
            config::LauncherConfig::load(&cli.config)
                .with_context(|| format!("Cargando config desde {:?}", cli.config))?,
        )
    } else {
        None
    };

    match cli.command {
        Commands::Launch {
            mc_version,
            loader,
            loader_version,
            ram,
            offline,
            no_connect,
        } => {
            let mut config = config_opt.take().expect("config loaded");
            let loader_type = LoaderType::from_str(&loader).map_err(|e| anyhow::anyhow!(e))?;
            if no_connect {
                config.features.quick_connect = false;
            }
            commands::launch::run(mc_version, loader_type, loader_version, offline, ram, &config)
                .await?;
        }

        Commands::Auth { action } => {
            let config = config_opt.take().expect("config loaded");
            match action {
                AuthAction::Login => commands::auth::login(&config).await?,
                AuthAction::Status => commands::auth::status(&config).await?,
                AuthAction::Logout => commands::auth::logout(&config).await?,
            }
        }

        Commands::Sync => {
            let config = config_opt.take().expect("config loaded");
            commands::sync::run(&config).await?;
        }

        Commands::Sign { action } => match action {
            SignAction::GenKeys { output_dir } => {
                commands::sign::gen_keys(output_dir.as_deref()).await?
            }
            SignAction::Sign { manifest, key, output } => {
                commands::sign::sign_manifest(&manifest, &key, output.as_deref()).await?
            }
            SignAction::Verify { manifest, key } => {
                commands::sign::verify_manifest(&manifest, &key).await?
            }
            SignAction::Validate { manifest, check_urls } => {
                commands::sign::validate_manifest(&manifest, check_urls).await?
            }
        },

        Commands::Manifest { action } => match action {
            ManifestAction::Init { lockfile, force } => {
                commands::manifest::run_init(&lockfile, force).await?;
            }

            ManifestAction::Update { lockfile, yes } => {
                commands::manifest::run_update(&lockfile, yes).await?;
            }

            ManifestAction::Generate {
                mods_dir,
                shaderpacks_dir,
                resourcepacks_dir,
                configs_dir,
                mc_version,
                java_version,
                loader,
                loader_version,
                output,
                self_hosted_url,
                apply_mode,
            } => {
                commands::manifest::run(
                    mods_dir.as_deref(),
                    shaderpacks_dir.as_deref(),
                    resourcepacks_dir.as_deref(),
                    configs_dir.as_deref(),
                    &mc_version,
                    java_version,
                    loader.as_deref(),
                    loader_version.as_deref(),
                    &output,
                    self_hosted_url.as_deref(),
                    &apply_mode,
                )
                .await?;
            }
        },
    }

    Ok(())
}
