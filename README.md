# mc-launcher-template

> **Template** de launcher de Minecraft modeado escrito en **Rust + Tauri 2 + SvelteKit**.  
> Haz fork, edita `launcher.config.toml` + assets, publica un tag → tienes un launcher con tu propia marca y auto-updater.

---

## Características

- 🎮 **Login Microsoft** (OAuth PKCE) + modo offline
- 📦 **Sync automático** de mods requeridos y opcionales desde tu servidor
- 🔧 **Loaders**: Vanilla, Fabric, Quilt, NeoForge, Forge
- ☕ **Java auto-gestionado** — descarga Adoptium JRE si no hay Java instalado
- 🔄 **Auto-updater** firmado (Tauri updater + minisign)
- 🎨 **Personalizable** — colores, logo, fondo, fuentes via config
- 📊 **Consola de logs** con filtros, reporte de diagnóstico
- 🚀 **CI/CD incluido** — GitHub Actions compila Windows + Linux + macOS en paralelo

---

## Inicio rápido

### Prerrequisitos

- [Rust](https://rustup.rs) 1.77+
- [Node.js](https://nodejs.org) 20+
- (Windows) WebView2 — preinstalado en Win 11

### Pasos

```bash
# 1. Clona / haz fork del repositorio
git clone https://github.com/YOUR_ORG/mc-launcher-template mi-launcher
cd mi-launcher

# 2. Inicializa (genera claves del updater, instala dependencias)
bash scripts/init-template.sh       # Linux / macOS
# .\scripts\init-template.ps1       # Windows PowerShell

# 3. Edita launcher.config.toml con los datos de tu servidor
#    (al menos: internal_id, display_name, manifest_url)

# 4. Prueba en modo dev
npm run tauri dev

# 5. Cuando estés listo, publica la primera release
git tag v1.0.0 && git push origin v1.0.0
```

Lee la guía completa en **[docs/customization-guide.md](docs/customization-guide.md)**.

---

## Estructura

```
├── crates/
│   ├── core/            # tipos, errores, paths, hashing
│   ├── meta/            # metadatos de Mojang (versiones, assets, libs)
│   ├── auth/            # Microsoft OAuth PKCE + perfil de Minecraft
│   ├── loaders/         # Fabric, Quilt, NeoForge, Forge
│   ├── mods/            # Modrinth, SelfHosted
│   ├── manifest-client/ # manifest del servidor + diff de sync
│   ├── downloader/      # descargas paralelas con verificación SHA-1
│   ├── java-manager/    # detección y descarga automática de JRE (Adoptium)
│   ├── launcher/        # classpath, JVM args, lanzado del proceso
│   └── admin-cli/       # CLI para admins
├── src-tauri/           # backend Tauri (comandos, estado)
├── src/                 # frontend SvelteKit
├── assets/              # logo, background, icon (reemplaza estos)
├── docs/                # guías de personalización
├── scripts/             # init-template.sh / .ps1
├── manifest-server-examples/
│   └── rust-server/     # servidor de manifest minimal en axum (~150 líneas)
├── .github/workflows/
│   └── release.yml      # CI: build + sign + GitHub Release
└── launcher.config.toml # ← EDITA ESTO
```

---

## Campos clave de `launcher.config.toml`

| Campo | Descripción |
|-------|-------------|
| `branding.internal_id` | ID único (AppData path — **permanente**) |
| `branding.display_name` | Nombre visible del launcher |
| `server.address` | IP/dominio del servidor |
| `server.manifest_url` | URL del manifest JSON del modpack |
| `auth.microsoft_client_id` | Azure App Registration Client ID |
| `updater.enabled` | `true` para activar auto-updater |

---

## CI/CD

El workflow `.github/workflows/release.yml` se dispara con un tag `v*`:

1. Compila para **Windows**, **Linux** y **macOS** en paralelo
2. Firma los instaladores con `TAURI_SIGNING_PRIVATE_KEY`
3. Crea un **Draft Release** en GitHub con todos los artifacts

Configura estos **Repository Secrets**:

| Secret | Valor |
|--------|-------|
| `TAURI_SIGNING_PRIVATE_KEY` | Contenido de `updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Contraseña (vacío si no hay) |

---

## Servidor de manifest

Sirve un JSON estático desde cualquier hosting, o usa el ejemplo incluido:

```bash
cd manifest-server-examples/rust-server
cargo run
# → http://localhost:3000/manifest.json
```

---

## Licencia

MIT OR Apache-2.0
