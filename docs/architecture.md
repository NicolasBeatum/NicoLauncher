# Arquitectura — MC Launcher Template

Descripción técnica de cómo están organizados los componentes del launcher y cómo se comunican entre sí.

---

## Visión general

```
┌─────────────────────────────────────────────────────────────────┐
│                        Tauri 2.x process                        │
│                                                                 │
│  ┌──────────────────┐          ┌──────────────────────────────┐ │
│  │   Frontend       │  invoke  │   Backend (Rust)             │ │
│  │   SvelteKit +    │◄────────►│                              │ │
│  │   TypeScript     │  events  │  src-tauri/src/              │ │
│  │                  │          │  ├── commands/               │ │
│  │  src/ (o ui/)    │          │  ├── events.rs               │ │
│  └──────────────────┘          │  └── state.rs                │ │
│                                └──────────┬───────────────────┘ │
└───────────────────────────────────────────┼─────────────────────┘
                                            │ usa
                          ┌─────────────────▼──────────────────────┐
                          │         Cargo workspace                 │
                          │                                         │
                          │  crates/                                │
                          │  ├── core            tipos, errores     │
                          │  ├── auth            Microsoft OAuth    │
                          │  ├── meta            Mojang API         │
                          │  ├── loaders         Fabric/Quilt/…    │
                          │  ├── mods            Modrinth/CF/self   │
                          │  ├── manifest-client manifest del srv   │
                          │  ├── downloader      descargas paralelas│
                          │  ├── java-manager    JDK detect/dl      │
                          │  └── launcher        spawn MC           │
                          └─────────────────────────────────────────┘
```

---

## Crates del workspace

### `crates/core`

Tipos compartidos entre todos los demás crates:

- `Error` / `Result` — error central con `thiserror`
- `Progress` — eventos de progreso tipados (descarga, instalación, etc.)
- `paths` — funciones para calcular `%APPDATA%/<internal_id>/`, carpeta de mods, Java, etc.
- `hash` — helpers SHA-1, SHA-512 para verificación

No depende de ningún otro crate del workspace.

### `crates/auth`

Flujo Microsoft OAuth 2.0 → cuenta de Minecraft:

```
Microsoft OAuth (PKCE)
  → Xbox Live (XBL)
  → XSTS
  → Mojang auth
  → Perfil de Minecraft (UUID + nombre + skin)
```

Almacena el `refresh_token` en el keyring del SO (Windows Credential Manager / Secret Service / Keychain).

### `crates/meta`

Consultas a la Mojang API:

- `version_manifest_v2.json` — lista todas las versiones de MC
- `<version>.json` — classpath de la versión, asset index, libs nativas
- `asset_index` — descarga y cachea assets del cliente

Todo se cachea en `<appdata>/meta/`.

### `crates/loaders`

Instalación de mod loaders. Cada loader implementa el trait `Loader`:

```rust
#[async_trait]
pub trait Loader {
    async fn install(&self, mc_version: &str, loader_version: &str, dir: &Path) -> Result<LoaderProfile>;
}
```

`LoaderProfile` devuelve el main class, argumentos JVM/game extra y las librerías adicionales.

Loaders implementados: **Fabric** → **Quilt** → **NeoForge** → **Forge** (orden de prioridad).

### `crates/mods`

Resolución y descarga de mods desde tres fuentes:

| Source | Identificador en manifest |
|--------|--------------------------|
| Modrinth | URL directa o `modrinth:<project_id>` |
| CurseForge | `curseforge:<project_id>/<file_id>` |
| Self-hosted | URL HTTPS directa |

Cada entrada del manifest lleva `sha512` y `filename` para verificación local. Si el archivo ya existe con hash correcto, no se descarga.

### `crates/manifest-client`

Lee el manifest del servidor en tres modos configurables:

| Modo | `manifest_provider` | Descripción |
|------|---------------------|-------------|
| HTTP | `"http"` | GET a `manifest_url`, verifica firma Ed25519 si `manifest_public_key` está definido |
| File | `"file"` | Lee un JSON local, útil en desarrollo |
| Git  | `"git"` | `git clone` / `git pull` en un path local, luego lee el JSON |

Schema del manifest: ver [`docs/manifest-schema.md`](manifest-schema.md).

### `crates/downloader`

Descargador paralelo con control de concurrencia:

- Pool de N workers (configurable con `download_concurrency`, default 8)
- Verificación SHA-512 post-descarga
- Progreso por evento (`Progress::Download { done, total }`)
- Reintentos automáticos (3 intentos, backoff exponencial)
- Hardlinks CAS: si un archivo con el mismo hash ya existe en caché, se crea hardlink en lugar de descargarlo

### `crates/java-manager`

Gestión del JDK según la estrategia configurada en `[java]`:

| Estrategia | Comportamiento |
|------------|---------------|
| `detect_or_download` | Busca Java en PATH y `JAVA_HOME`; si no encuentra la versión correcta, descarga Temurin |
| `always_download` | Siempre descarga y usa la versión fija, ignora Java del sistema |
| `system_only` | Solo usa Java del sistema; error si no está disponible |

Descarga desde la API de Adoptium (antes AdoptOpenJDK). Distribución configurable: `temurin`, `zulu`, `graalvm`.

### `crates/launcher`

Construye el classpath completo y lanza el proceso de Minecraft:

1. Recopila libs de Mojang (`crates/meta`) + loader (`crates/loaders`) + mods del classpath
2. Filtra reglas de OS/arquitectura (igual que el launcher oficial)
3. Extrae libs nativas en `natives/`
4. Construye los argumentos JVM y de juego
5. Ejecuta `java [...args] net.minecraft.client.main.Main`
6. Redirige stdout/stderr al logger del launcher (nivel `TRACE`)

Si `server.address` está definido y `features.quick_connect = true`, añade `--server <address>` a los argumentos del juego.

### `crates/admin-cli`

CLI para administradores del servidor. Binario independiente del launcher.

Subcomandos:

```
mc-launcher manifest init     → crea lockfile.toml interactivamente
mc-launcher manifest update   → genera/actualiza manifest.json desde lockfile.toml
mc-launcher manifest generate → [legacy] genera manifest.json directamente
mc-launcher sign gen-keys     → genera par de claves Ed25519
mc-launcher sign sign         → firma manifest.json → manifest-signed.json
mc-launcher sign verify       → verifica firma
mc-launcher sign validate     → valida schema, hashes, dependencias, URLs
mc-launcher launch            → lanza MC desde CLI (modo dev)
mc-launcher auth login/status/logout
mc-launcher sync              → sincroniza mods sin lanzar
```

---

## Flujo de arranque del launcher

```
1. Tauri inicia
      │
2. build.rs leyó launcher.config.toml en tiempo de compilación
   → constantes disponibles en el binario
      │
3. Frontend carga, aplica colores CSS del branding
      │
4. Comprueba sesión (keyring)
   ├─ Sin sesión → pantalla Login
   └─ Con sesión → Splash (sync + instalación)
         │
5. Splash descarga manifest del servidor
      │
6. Compara manifest con estado local
   ├─ Sin cambios → Play disponible
   └─ Con cambios → descarga mods/configs nuevos, borra eliminados
         │
7. Verifica/descarga Java
      │
8. Pantalla Home → botón PLAY
      │
9. Lanza proceso Minecraft
      │
10. Launcher queda minimizado en tray (o cierra, configurable)
```

---

## Comunicación frontend ↔ backend

Usa el sistema `invoke` / `emit` de Tauri:

```typescript
// Frontend llama al backend
const result = await invoke<SyncStatus>('sync_mods');

// Backend emite eventos de progreso
listen<ProgressEvent>('progress', (event) => {
  progress = event.payload;
});
```

Los comandos disponibles están en `src-tauri/src/commands/`. Los eventos en `src-tauri/src/events.rs`.

---

## Generación de constantes en tiempo de compilación

`src-tauri/build.rs` lee `launcher.config.toml` y genera:

- `src-tauri/src/generated_branding.rs` — constantes Rust (`INTERNAL_ID`, `DISPLAY_NAME`, etc.)
- `src/lib/generated-branding.json` (o `ui/src/lib/`) — JSON importado por el frontend para aplicar colores y textos

Esto significa que los valores de branding **no están en el binario como strings planos**; están compilados como constantes, lo que facilita el tree-shaking y evita configuraciones en runtime.

---

## Persistencia

| Dato | Dónde se guarda |
|------|----------------|
| Tokens Microsoft | Keyring del SO |
| Mods instalados | `<appdata>/mods/` (CAS hardlinks) |
| Java descargado | `<appdata>/java/<version>/` |
| Assets Minecraft | `<appdata>/assets/` |
| Manifest en caché | `<appdata>/manifest-cache.json` |
| Perfil del jugador | `<appdata>/profile.json` |
| Configuración del jugador | `<appdata>/settings.toml` (RAM, JVM args, etc.) |

`<appdata>` = `%APPDATA%\<internal_id>` en Windows, `~/.local/share/<internal_id>` en Linux, `~/Library/Application Support/<internal_id>` en macOS.

---

*Para detalles sobre el schema del manifest, ver [`manifest-schema.md`](manifest-schema.md).*
*Para detalles de seguridad, ver [`security.md`](security.md).*
