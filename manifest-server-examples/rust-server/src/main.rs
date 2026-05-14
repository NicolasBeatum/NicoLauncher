/// manifest-server — servidor minimal para MC Launcher Template
///
/// Sirve el manifest JSON con headers CORS correctos.
/// En producción, reemplaza la lectura de archivo por una base de datos,
/// S3, o cualquier fuente de datos que prefieras.
///
/// Uso:
///   cargo run                        # http://localhost:3000
///   PORT=8080 cargo run              # http://localhost:8080
///   MANIFEST_PATH=./mi-manifest.json cargo run
use axum::{
    extract::State,
    http::{Method, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

// ── Tipos ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub url: String,
    pub sha1: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalMod {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    pub default_enabled: bool,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
    pub version: String,
    pub url: String,
    pub sha1: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    /// Ruta relativa dentro del directorio .minecraft (ej: "config/modname.toml")
    pub path: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announcement {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub manifest_version: String,
    pub mc_version: String,
    pub loader_type: Option<String>,
    pub loader_version: Option<String>,
    #[serde(default)]
    pub required_mods: Vec<ModEntry>,
    #[serde(default)]
    pub optional_mods: Vec<OptionalMod>,
    #[serde(default)]
    pub configs: Vec<ConfigEntry>,
    /// Archivos a borrar del .minecraft del jugador (rutas relativas)
    #[serde(default)]
    pub files_to_delete: Vec<String>,
    pub announcement: Option<Announcement>,
}

// ── Estado del servidor ────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    manifest_path: PathBuf,
    /// Caché en memoria: (manifest, última carga)
    cache: Arc<RwLock<Option<(Manifest, Instant)>>>,
    cache_ttl: Duration,
}

impl AppState {
    fn new(manifest_path: PathBuf, cache_ttl: Duration) -> Self {
        Self {
            manifest_path,
            cache: Arc::new(RwLock::new(None)),
            cache_ttl,
        }
    }

    async fn get_manifest(&self) -> Result<Manifest, String> {
        // Intentar cache primero
        {
            let cache = self.cache.read().await;
            if let Some((ref m, loaded_at)) = *cache {
                if loaded_at.elapsed() < self.cache_ttl {
                    return Ok(m.clone());
                }
            }
        }

        // Releer del disco
        let raw = tokio::fs::read_to_string(&self.manifest_path)
            .await
            .map_err(|e| format!("Error leyendo manifest: {e}"))?;

        let manifest: Manifest = serde_json::from_str(&raw)
            .map_err(|e| format!("Error parseando manifest: {e}"))?;

        // Actualizar caché
        let mut cache = self.cache.write().await;
        *cache = Some((manifest.clone(), Instant::now()));

        info!(
            mc_version = %manifest.mc_version,
            loader = ?manifest.loader_type,
            required = manifest.required_mods.len(),
            optional = manifest.optional_mods.len(),
            "Manifest recargado"
        );

        Ok(manifest)
    }
}

// ── Handlers ───────────────────────────────────────────────────────────────

async fn get_manifest(State(state): State<AppState>) -> Response {
    match state.get_manifest().await {
        Ok(manifest) => Json(manifest).into_response(),
        Err(e) => {
            tracing::error!("{e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
        }
    }
}

async fn health() -> &'static str {
    "ok"
}

// ── Main ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "manifest_server=info,tower_http=info".into()),
        )
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let manifest_path = std::env::var("MANIFEST_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("manifest.json"));

    // Tiempo que el manifest se guarda en caché antes de releer del disco
    let cache_ttl_secs: u64 = std::env::var("CACHE_TTL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    info!(?manifest_path, port, cache_ttl_secs, "Iniciando servidor de manifest");

    if !manifest_path.exists() {
        tracing::warn!(
            "manifest.json no encontrado en {:?} — crea el archivo antes de arrancar",
            manifest_path
        );
    }

    let state = AppState::new(manifest_path, Duration::from_secs(cache_ttl_secs));

    // CORS: permite cualquier origen (el launcher hace peticiones desde tauri://)
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::OPTIONS])
        .allow_origin(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/manifest.json", get(get_manifest))
        .route("/health", get(health))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Escuchando en http://{addr}/manifest.json");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
