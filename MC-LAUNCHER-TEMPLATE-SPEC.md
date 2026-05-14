# Minecraft Modded Server Launcher — Plantilla en Rust

> **Documento para Claude Code.** Este archivo es la especificación completa de un proyecto a implementar desde cero. Incluye decisiones técnicas tomadas, arquitectura, esquemas de datos, plan de fases y notas de implementación. Léelo entero antes de empezar a escribir código.

---

## 0. TL;DR del proyecto

**Qué es:** Una **plantilla** de launcher de Minecraft escrita en Rust + Tauri + SvelteKit, pensada para servidores **modeados**. Quien use la plantilla la customiza editando un único archivo (`launcher.config.toml`) + assets, y compila su propio launcher de marca para su servidor.

**Para qué:** Distribuir un launcher a los jugadores de UN servidor concreto, que descargue automáticamente los mods, configs y loader correctos, mantenga al usuario sincronizado con la última versión del modpack, y permita activar/desactivar mods opcionales.

**No es:** Un launcher genérico multi-instancia tipo Prism o Modrinth App. Es deliberadamente single-server por instalación (aunque bajo el capó la arquitectura podría extenderse).

---

## 1. Decisiones tomadas

Estas decisiones ya están cerradas. **No volver a debatirlas**, implementar acorde:

| Decisión | Elección |
|---|---|
| Lenguaje principal | **Rust** |
| Framework de app | **Tauri 2.x** |
| Frontend | **SvelteKit + TypeScript** |
| Estilos | **TailwindCSS** + componentes custom (referencia: shadcn-svelte) |
| Modelo del launcher | **Single-server** (una instalación = un servidor; la plantilla se compila personalizada) |
| Sync de modpack | **Automático al lanzar** (siempre última versión del manifest) |
| Host del manifest | **Soporta dos modos**: HTTP propio + Git público (configurable en `launcher.config.toml`) |
| Gestión de Java | **Detectar si existe; si no, descargarlo** (Adoptium/Temurin por defecto) |
| Loaders soportados | **Fabric, Quilt, NeoForge, Forge** (en ese orden de prioridad de implementación) |
| Fuentes de mods | **Modrinth, CurseForge, Self-hosted** |
| Auth | **Microsoft OAuth** (Mojang accounts ya no existen) |
| Firma de manifests | **Ed25519** opcional pero recomendada |
| Auto-update del launcher | **Sí** (Tauri updater plugin) |

### Prioridades de calidad (ordenadas)

1. **UI muy pulida y customizable** — la primera impresión vende
2. **Soporte robusto de loaders/versiones** — es lo que diferencia de un launcher genérico
3. **Velocidad de descarga/lanzado** — descargas paralelas, cache por hash
4. **Facilidad de admin del servidor** — herramientas CLI para publicar manifests

---

## 2. Stack técnico completo

### Backend Rust (workspace de Cargo)

```toml
# Dependencias principales (workspace)
tokio              = "1"     # async runtime
reqwest            = "0.12"  # HTTP con rustls (no openssl)
serde + serde_json = "1"     # serialización
toml               = "0.8"   # parseo de launcher.config.toml
thiserror          = "1"     # errores tipados
anyhow             = "1"     # errores en binarios CLI
tracing            = "0.1"   # logging estructurado
sha1, sha2         = "0.10"  # hashing
ed25519-dalek      = "2"     # firma de manifests
zip                = "2"     # extracción de installers de loaders
semver             = "1"     # comparación de versiones
sysinfo            = "0.32"  # detectar RAM/CPU
which              = "6"     # detectar Java en PATH
keyring            = "3"     # almacenar tokens MS de forma segura
dirs               = "5"     # paths del SO (AppData, etc.)
async-trait        = "0.1"
chrono             = "0.4"   # timestamps en manifest
url                = "2"
hex                = "0.4"
uuid               = "1"
```

### Frontend (SvelteKit en `ui/`)

```json
{
  "svelte": "^5",
  "@sveltejs/kit": "^2",
  "@sveltejs/adapter-static": "latest",
  "tailwindcss": "^3",
  "@tauri-apps/api": "^2",
  "@tauri-apps/plugin-updater": "^2",
  "lucide-svelte": "latest",
  "marked": "^14"  // para renderizar announcement.body_md
}
```

> SvelteKit configurado con `adapter-static` y `prerender: true` porque Tauri sirve archivos estáticos.

### Tauri 2.x

- Plugins: `tauri-plugin-updater`, `tauri-plugin-dialog`, `tauri-plugin-shell`, `tauri-plugin-os`, `tauri-plugin-process`, `tauri-plugin-log`.
- Configuración de seguridad CSP estricta. Solo se permiten `connect-src` a los dominios necesarios (Modrinth, CurseForge, Mojang, Adoptium, host del manifest).

---

## 3. Estructura del repositorio

```
mc-launcher-template/
├── Cargo.toml                    # workspace root
├── rust-toolchain.toml           # pin de versión de Rust
├── launcher.config.toml          # ⭐ archivo principal de personalización
├── README.md
├── LICENSE                       # MIT OR Apache-2.0 (dual)
│
├── crates/
│   ├── core/                     # tipos, errores, paths, hash, progress
│   ├── auth/                     # Microsoft OAuth + perfil de Minecraft
│   ├── meta/                     # manifest de Mojang (versions, assets, libs)
│   ├── loaders/                  # Fabric, Quilt, NeoForge, Forge
│   ├── mods/                     # providers Modrinth/CurseForge/SelfHosted
│   ├── manifest-client/          # schema + providers HTTP/Git/File del manifest del server
│   ├── downloader/               # descargas paralelas con verificación
│   ├── java-manager/             # detección + descarga de JDK
│   ├── launcher/                 # construcción de classpath, args, spawn del proceso
│   └── admin-cli/                # CLI para que el admin del server publique manifests firmados
│
├── src-tauri/
│   ├── tauri.conf.json
│   ├── build.rs                  # lee launcher.config.toml y genera constantes + JSON para frontend
│   ├── src/
│   │   ├── main.rs
│   │   ├── commands/             # comandos invocables desde el frontend
│   │   ├── events.rs             # eventos hacia el frontend (progress, etc.)
│   │   └── state.rs              # estado global de la app
│   └── icons/                    # generados desde branding.icon
│
├── ui/                           # SvelteKit
│   ├── package.json
│   ├── svelte.config.js
│   ├── tailwind.config.js
│   ├── src/
│   │   ├── routes/
│   │   │   ├── +layout.svelte
│   │   │   ├── +page.svelte              # Home (botón PLAY)
│   │   │   ├── login/+page.svelte
│   │   │   ├── splash/+page.svelte       # primer arranque, sync inicial
│   │   │   ├── optional-mods/+page.svelte
│   │   │   └── settings/+page.svelte
│   │   ├── lib/
│   │   │   ├── tauri/                    # wrappers tipados de invoke()
│   │   │   ├── stores/                   # stores Svelte (auth, progress, etc.)
│   │   │   ├── branding.ts               # importa el JSON generado por build.rs
│   │   │   └── theme.ts                  # aplica colores del branding como CSS vars
│   │   └── components/
│
├── assets/                       # logo, background, icon (referenciados por launcher.config.toml)
│
├── manifest-server-examples/     # ejemplos de cómo hostear el manifest
│   ├── static-host/              # README con instrucciones para S3/Nginx/Cloudflare Pages
│   ├── git-based/                # ejemplo de repo con manifest.json + GitHub Actions
│   └── rust-server/              # servidor mínimo en axum que sirve el manifest firmado
│
├── docs/
│   ├── architecture.md
│   ├── manifest-schema.md
│   ├── loaders-fabric.md
│   ├── loaders-quilt.md
│   ├── loaders-neoforge.md
│   ├── loaders-forge.md
│   ├── customization-guide.md    # cómo customizar la plantilla
│   ├── publishing-manifests.md   # cómo el admin del server publica updates
│   └── security.md               # firmas, CSP, validación
│
└── .github/
    └── workflows/
        ├── ci.yml                # tests + clippy + fmt
        └── release.yml           # build multiplataforma + firma + GitHub Release
```

---

## 4. El archivo de personalización: `launcher.config.toml`

Este es el corazón del concepto "plantilla". Quien use el repo solo edita esto y los assets. Schema completo:

```toml
# ============================================================================
# launcher.config.toml — Edita este archivo para personalizar el launcher
# ============================================================================

[branding]
internal_id      = "mi-servidor-launcher"   # ID interno (paths AppData). Cambiarlo invalida instalaciones.
display_name     = "Mi Servidor MC"
window_title     = "Mi Servidor — Launcher"
description      = "Launcher oficial de Mi Servidor"
publisher        = "Tu Comunidad"
copyright        = "© 2026 Tu Nombre"
primary_color    = "#7c3aed"
secondary_color  = "#1e293b"
accent_color     = "#f59e0b"
logo             = "logo.png"               # 256x256+, usado en splash/header
icon             = "icon.ico"               # icono de la app (Windows)
icon_png         = "icon.png"               # icono de la app (Linux/Mac)
background       = "background.jpg"         # 1920x1080, fondo de la home
splash_video     = ""                       # opcional MP4
heading_font     = "Inter"
body_font        = "Inter"

[branding.social]
discord = "https://discord.gg/tu-server"
website = "https://miservidor.com"
twitter = ""
youtube = ""
donate  = ""

[server]
display_name              = "Mi Servidor"
address                   = "play.miservidor.com"
port                      = 25565
manifest_provider         = "http"            # "http" | "git" | "file"
manifest_url              = "https://api.miservidor.com/launcher/manifest.json"
manifest_git_repo         = ""                # si provider = git
manifest_git_branch       = "main"
manifest_git_path         = "manifest.json"
manifest_public_key       = ""                # hex Ed25519. Vacío = no verificar firma (no recomendado en prod)
update_check_interval_secs = 300
status_endpoint           = ""                # opcional, para mostrar online/MOTD/jugadores

[updater]
enabled            = true
release_url        = "https://api.miservidor.com/launcher/releases.json"
release_public_key = ""                       # clave pública de Tauri updater

[features]
allow_optional_mods         = true
allow_ram_config            = true
allow_jvm_args_edit         = false
allow_java_path_override    = true
allow_game_directory_override = false
show_advanced_logs          = true
show_news                   = true
quick_connect               = true            # botón "Unirse al servidor" tras lanzar

[runtime]
ram_min_mb            = 4096
ram_max_mb            = 16384
ram_default_mb        = 8192
default_jvm_args      = [
  "-XX:+UnlockExperimentalVMOptions",
  "-XX:+UseG1GC",
  "-XX:G1NewSizePercent=20",
  "-XX:G1ReservePercent=20",
  "-XX:MaxGCPauseMillis=50",
  "-XX:G1HeapRegionSize=32M",
]
download_concurrency  = 8
download_timeout_secs = 120

[java]
strategy     = "detect_or_download"   # "detect_or_download" | "always_download" | "system_only"
distribution = "temurin"              # "temurin" | "zulu" | "graalvm"

[curseforge]
api_key = ""        # opcional, también puede venir de env CURSEFORGE_API_KEY en build

[telemetry]
enabled          = false
endpoint         = ""
report_launches  = false
report_crashes   = false
report_hardware  = false
```

### Cómo se inyecta en el código

`src-tauri/build.rs` debe:

1. Leer `launcher.config.toml` desde la raíz del workspace.
2. Validar que los archivos referenciados (`logo`, `icon`, `background`) existen en `assets/`.
3. Generar `src-tauri/src/generated_branding.rs` con constantes Rust (`pub const DISPLAY_NAME: &str = "..."`, etc.).
4. Generar `ui/src/lib/generated-branding.json` con el subset que el frontend necesita (colores, social links, feature flags, nombres).
5. Generar dinámicamente `tauri.conf.json` o, si Tauri no lo permite en runtime, escribirlo en build time desde un template.
6. Procesar y copiar los iconos a `src-tauri/icons/` en los formatos que Tauri requiere.
7. Re-ejecutarse cuando cambien `launcher.config.toml` o los assets (`cargo:rerun-if-changed`).

---

## 5. Estructura de datos en disco del usuario final

Single-server, así que todo cuelga de un único directorio:

```
%APPDATA%\<internal_id>\           Windows
~/Library/Application Support/<internal_id>/   macOS
~/.local/share/<internal_id>/      Linux

├── minecraft/                     # el .minecraft único de la instalación
│   ├── mods/                      # gestionado por el launcher
│   ├── config/
│   ├── saves/
│   ├── resourcepacks/
│   ├── shaderpacks/
│   ├── options.txt
│   └── logs/
│
├── cache/
│   ├── libraries/                 # libs Java compartidas (Maven layout)
│   ├── assets/                    # assets de Minecraft (texturas, sonidos)
│   │   ├── indexes/
│   │   └── objects/
│   ├── mod-files/                 # CAS por sha512: aa/bb/<hash>.jar
│   ├── loader-installers/         # JARs de installers de Forge/NeoForge cacheados
│   └── manifest-cache/            # último manifest descargado (para offline)
│
├── java/                          # JDKs gestionados por el launcher
│   ├── 17/
│   └── 21/
│
├── optional-mods/                 # mods opcionales descargados, NO activos
│
├── logs/                          # logs del launcher (rotados)
│
├── current-state.json             # estado de sync (ver §7.4)
├── optional-choices.json          # qué opcionales tiene activos el usuario
└── account.json                   # session token cifrado o ref al keyring
```

### Por qué CAS (content-addressable storage) para mods

Si el manifest cambia un mod a una versión que el usuario tuvo antes, no se re-descarga. Si el usuario hace rollback, instantáneo. Y los archivos en `mods/` son **hardlinks** (Linux/macOS) o **junctions/copies** (Windows) hacia los archivos en `cache/mod-files/`.

---

## 6. Schema del manifest del servidor

Versionado con `schema_version` para evolucionar sin romper launchers desplegados.

### Manifest plano (sin firmar)

```json
{
  "schema_version": 1,
  "manifest_version": "2026.05.08-1",
  "released_at": "2026-05-08T10:00:00Z",

  "minecraft": {
    "version": "1.21.1",
    "java_version": 21
  },
  "loader": {
    "type": "neoforge",
    "version": "21.1.95"
  },

  "required_mods": [
    {
      "id": "create",
      "name": "Create",
      "source": {
        "type": "modrinth",
        "project_id": "LNytGWDc",
        "version_id": "abc123def",
        "download_url": "https://cdn.modrinth.com/data/.../create-1.0.jar"
      },
      "sha512": "ab12cd34...",
      "size": 12345678,
      "filename": "create-1.0.jar"
    },
    {
      "id": "private-mod",
      "name": "Mod Privado del Server",
      "source": {
        "type": "self_hosted",
        "url": "https://cdn.miservidor.com/mods/private-mod-1.0.jar"
      },
      "sha512": "fe98dc76...",
      "size": 234567,
      "filename": "private-mod-1.0.jar"
    }
  ],

  "optional_mods": [
    {
      "id": "jei",
      "name": "Just Enough Items",
      "source": {
        "type": "curseforge",
        "project_id": 238222,
        "file_id": 5678901
      },
      "sha512": "...",
      "size": 1234567,
      "filename": "jei-1.0.jar",
      "default_enabled": true,
      "category": "QoL",
      "description": "Muestra recetas de crafting...",
      "icon_url": "https://media.forgecdn.net/avatars/.../jei.png",
      "depends_on": [],
      "conflicts_with": ["rei"]
    }
  ],

  "config_overrides": [
    {
      "path": "config/server-ip.toml",
      "url": "https://cdn.miservidor.com/configs/server-ip.toml",
      "sha512": "...",
      "apply": "always"
    },
    {
      "path": "options.txt",
      "url": "https://cdn.miservidor.com/configs/options.txt",
      "sha512": "...",
      "apply": "if_missing"
    }
  ],

  "removed_files": [
    "config/old-mod.toml",
    "mods/legacy.jar"
  ],

  "additional_jvm_args": [],

  "announcement": {
    "id": "may-2026-update",
    "title": "Actualización de mayo",
    "body_md": "Hemos añadido **Create: New Age** y rebalanceado la economía...",
    "show_until": "2026-05-15T00:00:00Z"
  }
}
```

### Manifest firmado (formato wrapper)

```json
{
  "manifest": "<JSON serializado del ServerManifest como string>",
  "signature": "<firma Ed25519 del string anterior, en hex>"
}
```

El cliente intenta parsear primero como `SignedManifest`; si falla, como `ServerManifest` plano. Si hay `manifest_public_key` configurada y el manifest viene plano → error.

> **Importante:** Firmamos el **string JSON** literal, no el objeto re-serializado, para evitar problemas de canonicalización. El admin-cli debe garantizar que firma exactamente lo que sube.

### Tipos de `ModSource`

```rust
enum ModSource {
    Modrinth { project_id: String, version_id: String, download_url: Option<String> },
    CurseForge { project_id: u64, file_id: u64, download_url: Option<String> },
    SelfHosted { url: String },
}
```

> ⚠️ Sobre CurseForge: algunos proyectos tienen `allowModDistribution = false` y NO se pueden descargar vía API. Para esos casos hay que mirrorearlos en el host propio y usar `SelfHosted`. El launcher debe mostrar un error claro si recibe un mod de CurseForge no distribuible.

### Estado local persistido

```rust
// current-state.json
struct LocalState {
    applied_manifest_version: Option<String>,
    applied_at: Option<DateTime<Utc>>,
    installed_mods: HashMap<String, String>,        // mod_id -> sha512
    applied_configs: HashMap<String, String>,       // path -> sha512
    loader_installed: Option<InstalledLoader>,
}

// optional-choices.json
struct OptionalChoices {
    enabled: Vec<String>,
    last_seen_optional_ids: Vec<String>,            // para detectar opcionales nuevos
    dismissed_announcement_ids: Vec<String>,
}
```

---

## 7. Crates del workspace — responsabilidades y APIs

### 7.1 `launcher-core`

**Responsabilidad:** Tipos, errores, paths, hashing, progress reporting. Sin dependencias HTTP.

**Módulos:**

- `error.rs` — `enum Error` con `thiserror`, alias `Result<T>`.
- `paths.rs` — `struct LauncherPaths` con todos los paths del usuario, método `ensure_all()`.
- `hash.rs` — `hash_file(path, algo) -> String`, `verify_file(...)`, soporte SHA-1/256/512 streaming.
- `loader.rs` — `enum LoaderType { Vanilla, Fabric, Quilt, Forge, NeoForge }` con `FromStr`/`Display`.
- `progress.rs` — `enum ProgressEvent { Stage, Progress, Log, Done, Error }`, `struct ProgressReporter` con canal mpsc.

### 7.2 `launcher-meta`

**Responsabilidad:** Manifest de Mojang. Versions list, version JSON, asset index, libraries.

**APIs principales:**

```rust
pub struct MojangMetaClient { ... }

impl MojangMetaClient {
    pub async fn fetch_version_manifest(&self) -> Result<VersionManifestV2>;
    pub async fn fetch_version_json(&self, version_id: &str) -> Result<VersionJson>;
    pub async fn fetch_asset_index(&self, url: &str) -> Result<AssetIndex>;
}
```

URLs:
- `https://launchermeta.mojang.com/mc/game/version_manifest_v2.json`
- Cada `VersionJson` se descarga desde la URL listada en el manifest.

> Cachear `version_manifest_v2.json` en disco con TTL de 1 hora; los `VersionJson` por versión son inmutables y se cachean indefinidamente.

### 7.3 `launcher-auth`

**Responsabilidad:** Microsoft OAuth → XBox Live → XSTS → Minecraft Services API → perfil.

**Recomendación:** Usar el crate **`minecraft-msa-auth`** o **`oauth2`** + implementación manual del flow de Xbox/XSTS. La cadena completa es:

1. **Microsoft OAuth** (Authorization Code con PKCE — preferido sobre Device Code para mejor UX en desktop).
2. **Xbox Live**: POST a `https://user.auth.xboxlive.com/user/authenticate` con el access_token de MS.
3. **XSTS**: POST a `https://xsts.auth.xboxlive.com/xsts/authorize` con el token XBL. Manejar errores específicos: `2148916233` (sin cuenta de Xbox), `2148916238` (menor de edad y necesita familia).
4. **Minecraft Services**: POST a `https://api.minecraftservices.com/authentication/login_with_xbox` con `userhash` y XSTS token. Devuelve un access_token de Minecraft.
5. **Perfil**: GET `https://api.minecraftservices.com/minecraft/profile` con el access_token de MC. Devuelve UUID + username.

**Almacenamiento de credenciales:**

- El **refresh_token de Microsoft** va al **keyring del SO** (`keyring` crate).
- En `account.json` solo se guarda metadata (UUID, username, avatar URL, expiración).
- Al lanzar el juego, refrescar si es necesario.

**Client ID de Microsoft:** El admin que usa la plantilla debe registrar su propia app en Azure (gratis, instrucciones en `docs/customization-guide.md`). El `client_id` va en `launcher.config.toml` o en variable de entorno de build.

### 7.4 `launcher-manifest-client`

**Responsabilidad:** Schema del manifest del server, providers (HTTP/Git/File), verificación de firma, comparación con estado local para sync.

**Trait principal:**

```rust
#[async_trait]
pub trait ManifestProvider: Send + Sync {
    async fn fetch(&self) -> Result<ServerManifest>;
    fn name(&self) -> &str;
}
```

**Implementaciones:**

- `HttpProvider` — `reqwest::get(url)` con User-Agent y timeout. Reintento exponencial (3 intentos).
- `GitProvider` — clona shallow al `cache/manifest-cache/git-repo/` con el crate `gix` (preferido por single-binary). Si `gix` resulta pesado, fallback a invocar `git` del sistema.
- `FileProvider` — para testing local con `file://` URLs.

**Sync diff:**

```rust
pub fn compute_sync_plan(
    current: &LocalState,
    remote: &ServerManifest,
    optional_choices: &OptionalChoices,
) -> SyncPlan {
    // SyncPlan contiene:
    //   - mods_to_download: Vec<ModEntry>
    //   - mods_to_remove: Vec<String> (paths)
    //   - configs_to_apply: Vec<ConfigOverride>
    //   - files_to_delete: Vec<String>
    //   - loader_action: LoaderAction (None, Install, Reinstall)
}
```

### 7.5 `launcher-loaders`

**Responsabilidad:** Resolver qué libs/main_class/args necesita cada loader. Mergear con manifest de Mojang.

**Trait:**

```rust
#[async_trait]
pub trait LoaderProvider: Send + Sync {
    fn id(&self) -> LoaderType;
    async fn list_versions(&self, mc_version: &str) -> Result<Vec<LoaderVersion>>;
    async fn recommended_version(&self, mc_version: &str) -> Result<String>;
    async fn resolve_manifest(
        &self,
        mc_version: &str,
        loader_version: &str,
        cache_dir: &Path,
    ) -> Result<MergedVersionManifest>;
    async fn post_install(&self, instance_dir: &Path) -> Result<()> { Ok(()) }
}
```

**`MergedVersionManifest`** es el formato común que el crate `launcher` consume para construir el comando Java. Incluye: `id`, `main_class`, `libraries`, `jvm_args`, `game_args`, `asset_index`, `java_version`, `client_jar`.

#### Notas por loader

**Fabric** (más simple, implementar primero):
- Endpoint meta: `https://meta.fabricmc.net/v2/`
- `GET /versions/loader/<mc>` — lista loaders compatibles.
- `GET /versions/loader/<mc>/<loader>/profile/json` — devuelve un JSON estilo Mojang directo. Solo hay que mergearlo con el de Mojang.
- No requiere ejecutar installer.

**Quilt** (clónico de Fabric):
- Endpoint meta: `https://meta.quiltmc.org/v3/`
- Misma forma de uso. Implementar reusando código de Fabric.

**NeoForge** (mediano):
- Lista de versiones: `https://maven.neoforged.net/api/maven/versions/releases/net%2Fneoforged%2Fneoforge`
- Schema de versión: `<mc-minor>.<mc-patch>.<build>` (ej: `21.1.95` para MC 1.21.1).
- Installer: `https://maven.neoforged.net/releases/net/neoforged/neoforge/<version>/neoforge-<version>-installer.jar`
- Para 1.20.2+: el installer contiene `version.json` (estilo Mojang) y `install_profile.json` con processors a ejecutar para parchear el cliente.
- Procesos: extraer libs, descargar dependencies del install_profile, ejecutar processors con Java.
- 1.20.1 y anteriores se distribuyen como Forge tradicional (legacy fork).

**Forge** (más complejo):
- Promotions: `https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json` (recomendados/latest).
- Maven metadata: `https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml`
- Installer: `https://maven.minecraftforge.net/net/minecraftforge/forge/<mc>-<forge>/forge-<mc>-<forge>-installer.jar`
- **Tres épocas**:
  - 1.5–1.12.x: launchwrapper, formato viejo. **Recomiendo NO soportar** para mantener el scope.
  - 1.13–1.16.x: ForgeWrapper, transformers.
  - 1.17+: similar a NeoForge (installer con processors).
- **Estrategia recomendada**: implementar solo Forge 1.17+ en v1; documentar limitación.

#### Merge de manifests (algoritmo común)

```
1. Descargar VersionJson de Mojang para mc_version
2. Descargar profile JSON del loader
3. Tomar el array `libraries` de Mojang
4. Para cada lib del loader:
   - Si group:artifact ya existe en Mojang: el loader gana (reemplazar)
   - Si no: añadir
5. main_class del loader reemplaza el de Mojang
6. JVM args y game args: concatenar (Mojang primero, loader después)
7. asset_index, java_version, client_jar: heredados de Mojang
```

### 7.6 `launcher-mods`

**Responsabilidad:** Resolver URL final y metadata de cada mod según su `ModSource`.

**Trait:**

```rust
#[async_trait]
pub trait ModProvider: Send + Sync {
    async fn resolve(&self, source: &ModSource) -> Result<ResolvedMod>;
    fn name(&self) -> &str;
}

pub struct ResolvedMod {
    pub download_url: String,
    pub sha512: String,
    pub size: u64,
    pub filename: String,
}
```

**Implementaciones:**

- `ModrinthProvider`:
  - `GET https://api.modrinth.com/v2/version/<version_id>` → JSON con `files[]` (url, hashes.sha512, size, filename).
  - User-Agent obligatorio: `<launcher_id>/<version> (<contact>)` según ToS de Modrinth.
  - Sin auth necesaria.

- `CurseForgeProvider`:
  - `GET https://api.curseforge.com/v1/mods/<project_id>/files/<file_id>`
  - Header `x-api-key: <CURSEFORGE_API_KEY>`
  - Verificar `data.allowModDistribution` antes de devolver URL. Si es false → error específico que la UI puede mostrar bien.
  - Game ID de Minecraft: `432`.

- `SelfHostedProvider`:
  - Devuelve directamente lo que viene en el manifest (URL, hash, size, filename del entry).
  - No hace fetch porque ya está todo en el manifest.

**ModResolver** (combinador):

```rust
pub struct ModResolver {
    modrinth: ModrinthProvider,
    curseforge: Option<CurseForgeProvider>,  // None si no hay API key
    self_hosted: SelfHostedProvider,
}

impl ModResolver {
    pub async fn resolve_all(&self, mods: &[ModEntry]) -> Vec<Result<(ModEntry, ResolvedMod)>>;
}
```

### 7.7 `launcher-downloader`

**Responsabilidad:** Descargas paralelas con verificación de hash en streaming.

**APIs:**

```rust
pub struct Downloader {
    client: reqwest::Client,
    concurrency: usize,
    timeout: Duration,
    reporter: ProgressReporter,
}

pub struct DownloadJob {
    pub url: String,
    pub dest: PathBuf,
    pub expected_sha512: Option<String>,
    pub expected_sha1: Option<String>,
    pub expected_size: Option<u64>,
}

impl Downloader {
    pub async fn download_one(&self, job: DownloadJob) -> Result<()>;
    pub async fn download_many(&self, jobs: Vec<DownloadJob>) -> Result<()>;
}
```

**Implementación:**

- `tokio::sync::Semaphore` para limitar concurrencia.
- `futures::stream::iter(jobs).buffer_unordered(concurrency)` para ejecutar.
- Por cada job: si `dest` ya existe y matches el hash, skip. Si no, descargar a `<dest>.partial`, hashear mientras se escribe (`Sha512::update` por chunk), verificar al final, rename atómico a `<dest>`.
- Soporte de **resume** vía header `Range` si el servidor lo permite y `<dest>.partial` existe.
- Reintentos con backoff exponencial (3 intentos por defecto).

### 7.8 `launcher-java-manager`

**Responsabilidad:** Detectar Java compatible, o descargar JDK si no existe.

**Estrategias** (controladas por `[java].strategy` de la config):

- `system_only` — solo busca en PATH y locations conocidas, error si no encuentra compatible.
- `detect_or_download` — busca primero, descarga si no encuentra (default).
- `always_download` — siempre usa el JDK gestionado, ignora el del sistema.

**Detección:**

1. Variables de entorno `JAVA_HOME`.
2. PATH (`which::which("java")`).
3. Locations típicas:
   - Windows: `C:\Program Files\Java\`, `C:\Program Files\Eclipse Adoptium\`, `C:\Program Files\Microsoft\jdk-*`
   - macOS: `/Library/Java/JavaVirtualMachines/*/Contents/Home`
   - Linux: `/usr/lib/jvm/*`
4. Para cada candidato, ejecutar `<path>/bin/java -version` y parsear la salida.

**Descarga:**

- Adoptium API: `https://api.adoptium.net/v3/assets/feature_releases/<feature_version>/ga?architecture=<arch>&heap_size=normal&image_type=jre&os=<os>&vendor=eclipse`
- Versiones que necesitamos: 8 (legacy), 17 (1.17–1.20.x), 21 (1.21+).
- Descargar a `~/.local/share/<id>/java/<feature>/`, descomprimir, verificar.
- Persistir mapping `feature -> path` en un index file.

### 7.9 `launcher-launcher`

**Responsabilidad:** Construir el comando Java y lanzar el proceso.

**API:**

```rust
pub struct LaunchSpec {
    pub merged_manifest: MergedVersionManifest,
    pub java_path: PathBuf,
    pub game_dir: PathBuf,            // .minecraft del usuario
    pub assets_dir: PathBuf,
    pub libraries_dir: PathBuf,
    pub natives_dir: PathBuf,
    pub auth: AuthSession,            // username, uuid, access_token
    pub ram_mb: u32,
    pub jvm_args_extra: Vec<String>,
    pub server: Option<(String, u16)>, // si quick_connect activo, --server <host> --port <port>
}

pub struct GameProcess { ... }

pub async fn launch(spec: LaunchSpec, reporter: ProgressReporter) -> Result<GameProcess>;
```

**Pasos:**

1. Filtrar libs/args según reglas de OS/arch (las `Rule` del MergedVersionManifest).
2. Extraer libs nativas a `natives_dir` (si las hay).
3. Construir classpath: `lib1.jar:lib2.jar:...:client.jar` (`;` en Windows).
4. Construir comando:
   ```
   <java> <jvm_args> -Xmx<ram>M -Xms<ram>M -cp <classpath> <main_class> <game_args>
   ```
5. Sustituir placeholders en args: `${auth_player_name}`, `${version_name}`, `${game_directory}`, `${assets_root}`, `${assets_index_name}`, `${auth_uuid}`, `${auth_access_token}`, `${user_type}`, `${version_type}`, etc.
6. Spawn con `tokio::process::Command`.
7. Stream stdout/stderr, parsear líneas, emitir `ProgressEvent::Log` (con detección de crashes y warnings comunes).
8. Devolver `GameProcess` con métodos `wait()`, `kill()`, `pid()`.

### 7.10 `launcher-admin-cli`

**Responsabilidad:** CLI para admins del server. Comandos:

- `gen-keys` — genera par Ed25519, escribe `signing.key` y `public.key` (advertir de seguridad).
- `sign <manifest.json>` — firma un manifest, escribe `manifest-signed.json`.
- `verify <manifest-signed.json> <public.key>` — verifica firma.
- `validate <manifest.json>` — valida schema y refs (que los hashes/URLs sean alcanzables).
- `lockfile <pack.toml>` — toma una definición humana del modpack y resuelve a un manifest.json completo (con hashes calculados al vuelo).

> El modo `lockfile` es importante para usabilidad del admin: que pueda escribir solo `["modrinth:create@latest", "modrinth:jei@5.x"]` y la herramienta resuelva versiones, descargue, calcule hashes, genere el JSON.

---

## 8. Flujo del usuario (UI/UX)

### Primer arranque

1. **Splash screen** (1–3s) — logo grande, "Inicializando…".
2. **Detección de Java**: si falta y la estrategia lo permite, descargar (mostrar progreso). Si no hay internet, fallar con mensaje claro.
3. **Sync inicial del manifest** — barra de progreso por etapas.
4. **Login screen** — botón "Iniciar sesión con Microsoft", abre OAuth en webview.
5. Tras login: redirigir a Home.

### Arranque normal (con sesión existente)

1. **Splash** (medio segundo).
2. Refresh de tokens en background.
3. Fetch del manifest en background.
4. **Home** se renderiza inmediatamente con datos cacheados, y se actualiza cuando llega el manifest fresco.

### Home

Layout sugerido:

```
+----------------------------------------------------------+
| [logo]  Mi Servidor MC          [usuario ▾] [⚙]          |
+----------------------------------------------------------+
|                                                          |
|   [Background art enorme con overlay sutil]              |
|                                                          |
|              ┌────────────────────┐                       |
|              │      ▶ JUGAR       │  ← botón gigante     |
|              └────────────────────┘                       |
|         Versión 2026.05.08-1 · NeoForge 21.1.95          |
|                                                          |
|  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  |
|  📰 Novedades                     🟢 24/100 jugadores    |
|  • Nueva expansión Create...      Ping: 32ms              |
|  • Evento de mayo este sábado     [Copiar IP]             |
+----------------------------------------------------------+
| [Mods opcionales]  [Ajustes]  [Discord]  [Web]           |
+----------------------------------------------------------+
```

### Estado del botón JUGAR

- **Idle** — "▶ JUGAR"
- **Sync requerido** — "▶ JUGAR" (al pulsar, primero hace sync con barra de progreso, luego lanza)
- **Sync en curso** — barra de progreso reemplaza el botón, con texto de la etapa actual
- **Lanzando** — "Iniciando Minecraft..." con spinner
- **En juego** — "🟢 Minecraft está corriendo" + botón "Cerrar juego"

### Pantalla de Optional Mods

- Tabs por categoría (Performance, QoL, Shaders, Visual, etc.).
- Cards con thumbnail, nombre, descripción truncada (expandible), toggle on/off.
- Resolver dependencias automáticamente: si activas A que depende de B, activar B y mostrar toast.
- Detectar conflictos: si activas A que choca con B, preguntar cuál mantener.
- Banner si hay opcionales nuevos desde la última visita.

### Pantalla de Settings

Solo lo que `[features]` permita:

- **RAM** — slider visual entre `ram_min_mb` y `ram_max_mb`, default `ram_default_mb`. Detectar RAM total del sistema y advertir si pide demasiado.
- **Java** — path override (botón "Browse"), botón "Detectar automáticamente".
- **JVM args** — textarea (solo si `allow_jvm_args_edit`).
- **Idioma** — selector.
- **Tema** — claro/oscuro/auto.
- **Logs** — botón "Abrir carpeta de logs".
- **Reset completo** — borra todo (con confirmación).

---

## 9. Comandos Tauri (puente backend ↔ frontend)

Lista de comandos a exponer desde `src-tauri/src/commands/`. Todos `async`, todos devuelven `Result<T, String>` (Tauri serializa errores como string).

```rust
// Branding (ya inyectado en el frontend, pero accesible para tooling)
get_branding() -> BrandingDto

// Auth
auth_login_microsoft() -> AuthSession
auth_logout()
auth_current_session() -> Option<AuthSession>
auth_refresh() -> AuthSession

// Manifest
manifest_fetch() -> ServerManifestDto
manifest_get_cached() -> Option<ServerManifestDto>
manifest_status() -> ServerStatusDto  // online/offline + jugadores

// Sync & launch
sync_compute_plan() -> SyncPlanDto
sync_apply()  // emite ProgressEvent vía Tauri events
launch_game() -> u32  // pid
game_is_running() -> bool
game_kill()

// Mods opcionales
optional_mods_list() -> Vec<OptionalModDto>
optional_mods_set_enabled(id: String, enabled: bool) -> Vec<String>  // devuelve set tras resolver deps
optional_mods_get_enabled() -> Vec<String>

// Settings
settings_get() -> SettingsDto
settings_set(settings: SettingsDto)

// Java
java_detect() -> Vec<JavaInstallation>
java_download(version: u8)  // emite progress

// Logs y diagnóstico
logs_open_folder()
diagnostics_collect() -> String  // bundle de info para soporte
```

**Eventos** (backend → frontend, vía `app.emit_all`):

```rust
"progress"        // ProgressEvent del sync/download
"game-log-line"   // línea de stdout/stderr del juego
"game-exited"     // { code: i32 }
"manifest-updated" // hubo update mientras la app estaba abierta
"toast"           // notificaciones cortas
```

---

## 10. Seguridad

### Firmas de manifest

- Algoritmo: **Ed25519**.
- Clave pública embebida en el binario (de `launcher.config.toml`).
- El admin firma cada manifest antes de subirlo (con `admin-cli sign`).
- El launcher rechaza manifests sin firma válida si hay clave pública configurada.
- Si la clave pública está vacía → modo dev, se acepta cualquier manifest (LOG warning).

### Tauri CSP

Whitelistear solo lo necesario en `tauri.conf.json`:

```json
"security": {
  "csp": "default-src 'self'; img-src 'self' https://cdn.modrinth.com https://media.forgecdn.net data:; connect-src 'self' ipc: https://api.modrinth.com https://api.curseforge.com https://launchermeta.mojang.com https://piston-meta.mojang.com https://piston-data.mojang.com https://meta.fabricmc.net https://meta.quiltmc.org https://maven.neoforged.net https://files.minecraftforge.net https://maven.minecraftforge.net https://api.adoptium.net https://login.microsoftonline.com https://user.auth.xboxlive.com https://xsts.auth.xboxlive.com https://api.minecraftservices.com <manifest_host>; script-src 'self'; style-src 'self' 'unsafe-inline';"
}
```

> El `<manifest_host>` se inyecta en build time desde `launcher.config.toml`.

### Validación de paths

- Los `path` de `config_overrides` y `removed_files` deben ser **relativos** y **no escapar** del directorio `.minecraft` (no `..`, no rutas absolutas).
- Validar al recibir el manifest. Si falla, abortar sync con error claro.

### Almacenamiento de tokens

- **Refresh token de Microsoft** en el keyring del SO (`keyring` crate). Nunca en disco plano.
- **Access tokens** solo en memoria.
- **Logs nunca incluyen tokens** — sanitizar antes de escribir.

### Auto-update

- Tauri updater plugin con clave pública propia (distinta de la del manifest).
- Endpoint sirve un JSON con la última versión + URLs firmadas.
- Update silencioso opcional, o con prompt al usuario (configurable).

---

## 11. Plan de fases de implementación

> **Para Claude Code: implementa en este orden estricto.** No saltes a una fase superior sin tener la anterior funcionando con tests básicos.

### Fase 1 — Fundamentos (semana 1–2)

- [ ] Workspace de Cargo con todos los crates vacíos pero compilando.
- [ ] `launcher-core` completo (errores, paths, hashing, progress).
- [ ] `launcher-meta` con fetch del version manifest de Mojang y un version JSON.
- [ ] `launcher-downloader` con descargas paralelas + verificación.
- [ ] `launcher.config.toml` parseado por `build.rs` con generación de constantes Rust.
- [ ] CLI mínima en `admin-cli` que lance vanilla MC con auth offline (mock) — sirve para validar todo el pipeline antes de Tauri.

**Criterio de éxito:** Desde CLI, lanzar Minecraft 1.21.1 vanilla en una carpeta limpia.

### Fase 2 — Auth + Fabric (semana 3–4)

- [ ] `launcher-auth` con Microsoft OAuth completo (PKCE) + chain XBL/XSTS/MC.
- [ ] Keyring para refresh token.
- [ ] `launcher-loaders::fabric` completamente funcional (incluido merge con Mojang).
- [ ] `launcher-launcher` con construcción de comando + spawn.
- [ ] CLI lanza un Fabric instalado con cuenta MS real.

**Criterio de éxito:** Desde CLI, autenticarse y lanzar MC con Fabric, con un mod en `mods/` funcionando.

### Fase 3 — Manifest del server + sync (semana 5–6)

- [ ] `launcher-manifest-client` con schema completo + 3 providers.
- [ ] Verificación de firma Ed25519.
- [ ] `launcher-mods` con providers Modrinth + SelfHosted.
- [ ] Algoritmo de sync diff.
- [ ] CAS de mod-files con hardlinks.
- [ ] CLI sincroniza desde un manifest local de prueba.

**Criterio de éxito:** Cambiar el manifest dispara descargas/eliminaciones correctas; rollback re-usa cache.

### Fase 4 — UI Tauri + SvelteKit (semana 7–10)

> **Más larga porque es la prioridad #1.** No escatimar en pulido aquí.

- [ ] Scaffolding Tauri 2.x + SvelteKit + Tailwind.
- [ ] `build.rs` genera también el JSON de branding para el frontend.
- [ ] Comandos Tauri implementados (todos los del §9).
- [ ] Splash screen con animación.
- [ ] Login screen con OAuth en ventana embebida.
- [ ] Home screen con botón PLAY, news, status.
- [ ] Optional mods screen con cards, categorías, deps/conflicts.
- [ ] Settings screen.
- [ ] Sistema de toast/notificaciones.
- [ ] Modo claro/oscuro con CSS vars desde `launcher.config.toml`.
- [ ] Animaciones (transiciones de Svelte, no Framer Motion).

**Criterio de éxito:** Demo grabable de un usuario nuevo: instalar → login → ver home → pulsar play → jugar.

### Fase 5 — NeoForge + Forge (semana 11–13)

- [ ] `launcher-loaders::neoforge` completo (1.20.2+).
- [ ] Ejecución de processors del install_profile (requiere Java disponible).
- [ ] `launcher-loaders::forge` para 1.17+ (omitir legacy).
- [ ] Quilt (clónico de Fabric, fácil).
- [ ] Tests con servidores reales de cada loader.

**Criterio de éxito:** El launcher puede sincronizar y lanzar modpacks de Fabric, Quilt, NeoForge y Forge moderno.

### Fase 6 — CurseForge + Java auto + auto-update + pulido (semana 14–15)

- [ ] `launcher-mods::curseforge` con manejo de `allowModDistribution = false`.
- [ ] `launcher-java-manager` con detección + descarga Adoptium.
- [ ] Tauri updater plugin configurado y firmado.
- [ ] Logs visibles en UI con filtros.
- [ ] Diagnostics bundle (botón "Crear reporte de soporte").
- [ ] Sistema de telemetría opt-in (si el admin lo activa).

**Criterio de éxito:** Listo para entregar a usuarios reales.

### Fase 7 — Sistema de plantilla (semana 16)

- [ ] `docs/customization-guide.md` paso a paso.
- [ ] GitHub Action de ejemplo: build multiplataforma + firma + release.
- [ ] Script `scripts/init-template.sh` que genera un par de keys, un `launcher.config.toml` de ejemplo y un manifest mínimo.
- [ ] Servidor de manifest de ejemplo en `manifest-server-examples/rust-server/` (axum mínimo, ~150 líneas).

**Criterio de éxito:** Una persona externa puede forkear el repo, leer el README, y tener su launcher branded compilando en menos de 30 minutos.

---

## 12. Tests

### Unit tests

- `launcher-core`: hashing con vectores conocidos, paths cross-platform.
- `launcher-manifest-client`: parseo de manifests válidos/inválidos, firma/verificación con keys de test.
- `launcher-loaders`: merge de manifests con fixtures JSON de Mojang/Fabric guardadas en el repo.
- `launcher-mods`: parseo de respuestas de Modrinth/CurseForge con fixtures.

### Integration tests

- `tests/sync_e2e.rs`: levanta un servidor HTTP local con `wiremock`, sirve un manifest, ejecuta sync, verifica estado en disco.
- `tests/launch_dry_run.rs`: construye el comando Java pero no ejecuta el proceso (solo asserta el array de args).

### Tests manuales

Checklist en `docs/manual-testing.md`:

- [ ] Primer arranque sin internet → mensaje claro
- [ ] Login MS, cerrar app, reabrir → sigue logueado
- [ ] Cambiar manifest mientras la app está abierta → notificación
- [ ] Activar opcional con dep → ambos descargan
- [ ] Desactivar required (no debería ser posible desde UI)
- [ ] Borrar `.minecraft/mods` manualmente → re-sync recupera
- [ ] Cuenta MS sin Java owned → mensaje específico
- [ ] Modpack que requiere Java 21, sistema solo tiene 17 → descarga 21

---

## 13. Notas, gotchas y cosas a recordar

### Hardlinks vs copia en Windows

- Linux/macOS: `std::fs::hard_link` funciona siempre.
- Windows: requiere mismo volumen. Si el `.minecraft` y el `cache/` están en discos distintos (poco probable porque ambos cuelgan de la misma raíz), fallback a copia.
- NTFS junctions son para directorios, no para archivos. Para archivos en Windows es hardlink o copia.

### Microsoft OAuth: registro de la app

El admin que usa la plantilla DEBE registrar su propia app en Azure Portal (es gratis):

1. https://portal.azure.com → Azure Active Directory → App registrations → New
2. Tipo: "Personal Microsoft accounts only"
3. Redirect URI: `http://localhost:<random_port>/callback` (la app abre un servidor local efímero durante el OAuth).
4. API permissions: `XboxLive.signin offline_access`
5. Copiar el **Application (client) ID** y ponerlo en `launcher.config.toml` o env var de build.

Documentar esto bien en `docs/customization-guide.md`.

### Asset index "legacy" y "pre-1.6"

Para versiones antiguas de Minecraft (1.6.x y anteriores), los assets se mapean a `resources/` con nombres legacy en vez de hashes. Si la plantilla solo apunta a versiones modernas (1.17+), se puede ignorar. Documentarlo como limitación.

### CurseForge: el flag de distribución

Cuando `allowModDistribution = false`, la respuesta de la API tiene `downloadUrl: null`. No intentar adivinar URLs de la CDN — viola los ToS y es frágil. Mostrar error tipo "Este mod no permite distribución por API; el admin del server debe mirrorearlo".

### gix vs git CLI

Para `GitProvider`, evaluar `gix` (puro Rust, sin deps externas) vs invocar `git` del sistema. `gix` es preferible para single-binary distribuible, pero pesa ~5-10MB extra y tiene API en flujo. **Decisión sugerida:** empezar con `git` CLI del sistema (más simple), migrar a `gix` si pesa mucho.

### Tamaño del binario final

Objetivo: < 15 MB para el ejecutable Tauri stripped. Con LTO y `strip = true` en release profile es alcanzable.

### Internacionalización

No es prioridad para v1, pero diseñar los strings de UI pasando por una capa `i18n` desde el principio (ej: `t("home.play_button")`) facilita añadir idiomas después. Backend Rust: usar `fluent` o simple HashMap.

---

## 14. Convenciones de código

- **Rust**: `cargo fmt` + `cargo clippy --all-targets -- -D warnings` en CI.
- **TypeScript**: `prettier` + `eslint`. No `any` salvo justificado.
- **Naming**: snake_case en Rust, camelCase en TS, kebab-case en archivos Svelte.
- **Errors**: en libraries, `thiserror`. En binaries, `anyhow`. Nunca `unwrap()` en código de producción excepto en builders/locks documentados.
- **Comments en español o inglés**, consistentes dentro de un mismo crate. Recomendación: inglés para los crates (potencial open source), español para `docs/` (audiencia hispanohablante).
- **Commits**: Conventional Commits (`feat:`, `fix:`, `docs:`, etc.). Branch `main` protegido.

---

## 15. Decisiones aún abiertas

Estas decisiones NO están tomadas y se postergan a cuando llegue su fase. Si Claude Code se topa con ellas, debe **preguntar antes de implementar**:

- ¿Soporte de modpacks locales (cargar `.mrpack` de Modrinth) además del manifest? Probablemente no para v1.
- ¿UI para que el usuario vea el changelog entre versiones del manifest? Útil pero no crítico.
- ¿Soporte de servidores con whitelist Discord/web? El launcher podría bloquear el play hasta verificar membresía. Out of scope para v1.
- ¿Modo "offline" cracked para testing? Polémico, **no incluir** en la plantilla por defecto.

---

## 16. Referencias externas

Documentación y repos a consultar durante la implementación:

- **Modrinth App (Theseus)**: https://github.com/modrinth/code/tree/main/apps/app — referencia de Tauri + Rust + launcher.
- **Prism Launcher**: https://github.com/PrismLauncher/PrismLauncher — referencia de arquitectura de instances (C++).
- **lyceris**: https://github.com/eikix/lyceris — librería Rust de launcher.
- **Modrinth API**: https://docs.modrinth.com/api/
- **CurseForge API**: https://docs.curseforge.com/
- **Mojang piston-meta**: https://piston-meta.mojang.com/mc/game/version_manifest_v2.json
- **Fabric meta**: https://meta.fabricmc.net/
- **Quilt meta**: https://meta.quiltmc.org/
- **NeoForged maven**: https://maven.neoforged.net/
- **Forge maven**: https://maven.minecraftforge.net/
- **Adoptium API**: https://api.adoptium.net/
- **MS Auth flow para Minecraft**: https://wiki.vg/Microsoft_Authentication_Scheme

---

## 17. Resumen ejecutivo para Claude Code

Estás construyendo una **plantilla open-source** de launcher de Minecraft modeado en **Rust + Tauri 2 + SvelteKit**. El usuario final del launcher abre la app, hace login con Microsoft, ve un botón PLAY, lo pulsa, y juega. Detrás de escena el launcher sincroniza mods (Modrinth/CurseForge/host propio), instala el loader correcto (Fabric/Quilt/NeoForge/Forge), gestiona Java, y lanza el juego con sesión válida.

Es **single-server por instalación**: cada admin de servidor que use la plantilla cambia un único `launcher.config.toml` + assets, registra su app de Microsoft, publica un `manifest.json` firmado en su host, y compila. El sync es **automático al lanzar** — siempre la última versión.

Implementa **estrictamente en el orden de fases del §11**. Cada fase debe terminar con su criterio de éxito verificable antes de pasar a la siguiente. La calidad de la UI es la **prioridad #1**: no escatimar en la fase 4.

Cuando dudes en una decisión que no esté en este documento, **pregunta antes de implementar**. Cuando una sub-tarea exceda 200 líneas de código, propón el diseño en un comentario antes de escribirlo todo.
