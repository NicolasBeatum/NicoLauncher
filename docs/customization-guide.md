# Guía de personalización — MC Launcher Template

Esta guía explica cómo convertir esta plantilla en el launcher de **tu** servidor de Minecraft.
Tiempo estimado: 30-60 minutos.

> **¿Buscas cómo gestionar el modpack (manifest, mods, firma)?**
> Consulta [`docs/admin-guide.md`](admin-guide.md).

---

## Índice

1. [Requisitos previos](#1-requisitos-previos)
2. [Hacer fork / clonar la plantilla](#2-hacer-fork--clonar-la-plantilla)
3. [Ejecutar el script de inicialización](#3-ejecutar-el-script-de-inicialización)
4. [Editar `launcher.config.toml`](#4-editar-launcherconfigtoml)
5. [Personalizar assets (logo, icono, fondo)](#5-personalizar-assets)
6. [Configurar el manifest del servidor](#6-configurar-el-manifest-del-servidor)
7. [Autenticación Microsoft (OAuth)](#7-autenticación-microsoft-oauth)
8. [Primer build y prueba local](#8-primer-build-y-prueba-local)
9. [Configurar GitHub Secrets para CI](#9-configurar-github-secrets-para-ci)
10. [Publicar la primera release](#10-publicar-la-primera-release)
11. [Activar el auto-updater](#11-activar-el-auto-updater)
12. [Desplegar el servidor de manifest](#12-desplegar-el-servidor-de-manifest)
13. [Referencia de `launcher.config.toml`](#13-referencia-de-launcherconfigtoml)

---

## 1. Requisitos previos

| Herramienta | Versión mínima | Instalar |
|-------------|---------------|---------|
| Rust        | 1.77+         | [rustup.rs](https://rustup.rs) |
| Node.js     | 20+           | [nodejs.org](https://nodejs.org) |
| Git         | —             | [git-scm.com](https://git-scm.com) |
| (Windows) WebView2 | — | preinstalado en Win 11; [descargar](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) para Win 10 |

Instala las dependencias del proyecto una sola vez:

```bash
npm install
```

---

## 2. Hacer fork / clonar la plantilla

### Opción A — Fork en GitHub (recomendado)

1. Ve a `https://github.com/YOUR_ORG/mc-launcher-template`
2. Pulsa **Fork** → elige tu organización o cuenta personal
3. Clona tu fork:

```bash
git clone https://github.com/TU_ORG/mi-launcher.git
cd mi-launcher
```

### Opción B — Copiar sin historial

```bash
git clone --depth 1 https://github.com/YOUR_ORG/mc-launcher-template mi-launcher
cd mi-launcher
git remote remove origin
git remote add origin https://github.com/TU_ORG/mi-launcher.git
git push -u origin main
```

---

## 3. Ejecutar el script de inicialización

El script genera el par de claves del updater y guía los siguientes pasos:

**Windows (PowerShell):**
```powershell
.\scripts\init-template.ps1
```

**Linux / macOS:**
```bash
bash scripts/init-template.sh
```

El script:
- Genera `updater.key` (privada) y `updater.key.pub` (pública)
- Imprime la clave pública para que la pegues en `launcher.config.toml`
- Añade `updater.key` a `.gitignore` automáticamente (¡nunca la subas!)

---

## 4. Editar `launcher.config.toml`

Abre `launcher.config.toml` y cambia al menos:

```toml
[branding]
internal_id   = "mi-servidor"        # ← identificador único, solo letras/números/guiones
display_name  = "Mi Servidor MC"
window_title  = "Mi Servidor — Launcher"

[server]
address       = "play.miservidor.com"
manifest_url  = "https://api.miservidor.com/launcher/manifest.json"

[auth]
microsoft_client_id = "TU_CLIENT_ID"  # ver sección 7
```

> **⚠️ `internal_id` es permanente.** Se usa para la ruta `%APPDATA%\mi-servidor\` en los equipos de los jugadores. Si lo cambias después de publicar, los jugadores perderán su instalación.

---

## 5. Personalizar assets

Reemplaza los archivos en la carpeta `assets/`:

| Archivo | Descripción | Tamaño recomendado |
|---------|-------------|-------------------|
| `logo.png` | Logo que aparece en splash y sidebar | 256×256 px |
| `icon.ico` | Icono de la ventana (Windows) | 256×256 px (multi-res) |
| `icon.png` | Icono (Linux/macOS) | 512×512 px |
| `background.jpg` | Fondo de pantalla del launcher | 1920×1080 px |

También actualiza los iconos en `src-tauri/icons/` — puedes generarlos con:

```bash
npm run tauri icon assets/icon.png
```

---

## 6. Configurar el manifest del servidor

El manifest es un JSON que el launcher descarga al arrancar para saber qué mods instalar.

### Formato mínimo

```json
{
  "manifest_version": "1",
  "mc_version": "1.21.1",
  "loader_type": "fabric",
  "loader_version": "0.16.5",
  "required_mods": [],
  "optional_mods": [],
  "configs": [],
  "files_to_delete": [],
  "announcement": null
}
```

### Opciones de `manifest_provider`

| Valor | Descripción |
|-------|-------------|
| `"http"` | Descarga desde `manifest_url` vía HTTPS |
| `"file"` | Lee un archivo local (solo para desarrollo) |
| `"git"` | Clona/pull de un repo Git |

Para producción usa `"http"` y despliega el manifest en tu servidor (ver sección 12).

### Ejemplo con mods

```json
{
  "manifest_version": "1",
  "mc_version": "1.21.1",
  "loader_type": "fabric",
  "loader_version": "0.16.5",
  "required_mods": [
    {
      "id": "fabric-api",
      "name": "Fabric API",
      "version": "0.100.0+1.21.1",
      "url": "https://cdn.modrinth.com/data/P7dR8mSH/versions/xxx/fabric-api-0.100.0+1.21.1.jar",
      "sha1": "abc123...",
      "filename": "fabric-api-0.100.0+1.21.1.jar"
    }
  ],
  "optional_mods": [
    {
      "id": "sodium",
      "name": "Sodium",
      "description": "Mejora el rendimiento gráfico",
      "category": "Rendimiento",
      "icon_url": "https://cdn.modrinth.com/data/AANobbMI/icon.png",
      "default_enabled": true,
      "depends_on": [],
      "conflicts_with": [],
      "version": "0.5.11+mc1.21.1",
      "url": "https://cdn.modrinth.com/data/AANobbMI/versions/xxx/sodium-fabric-0.5.11+mc1.21.1.jar",
      "sha1": "def456...",
      "filename": "sodium-fabric-0.5.11+mc1.21.1.jar"
    }
  ]
}
```

---

## 7. Autenticación Microsoft (OAuth)

Para login con cuentas Microsoft (no solo offline):

1. Ve a [portal.azure.com](https://portal.azure.com) → **Azure Active Directory** → **App registrations** → **New registration**
2. Nombre: `Mi Servidor Launcher`
3. Tipo de cuenta: **Personal Microsoft accounts only**
4. Redirect URI: plataforma `Mobile and desktop applications` → URI `http://localhost`
5. Pulsa **Register**
6. Copia el **Application (client) ID** y pégalo en `launcher.config.toml`:

```toml
[auth]
microsoft_client_id = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

7. En **API permissions** → **Add a permission** → **APIs my organization uses** → busca **Xbox Live** → añade `XboxLive.signin` y `offline_access`

> Los usuarios con cuentas no-premium (offline) pueden jugar sin configurar esto — el launcher tiene modo offline de fallback.

---

## 8. Primer build y prueba local

```bash
# Modo desarrollo (hot-reload)
npm run tauri dev

# Build de producción
npm run tauri build
```

El instalador aparecerá en `src-tauri/target/release/bundle/`.

---

## 9. Configurar GitHub Secrets para CI

En tu repositorio: **Settings → Secrets and variables → Actions → New repository secret**

| Secret | Valor |
|--------|-------|
| `TAURI_SIGNING_PRIVATE_KEY` | Contenido completo del archivo `updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Contraseña de la clave (vacío si no tiene) |

> **Nunca subas `updater.key` al repositorio.** El script de init lo añade a `.gitignore`.

---

## 10. Publicar la primera release

```bash
git tag v1.0.0
git push origin v1.0.0
```

Esto dispara el workflow `.github/workflows/release.yml` que:
1. Compila para Windows, Linux y macOS en paralelo
2. Firma los instaladores con tu clave privada
3. Crea un **Draft Release** en GitHub con todos los artifacts

Ve a **Releases** en tu repo, revisa el draft y pulsa **Publish**.

---

## 11. Activar el auto-updater

Cuando hayas publicado tu primera release y tengas una URL estable:

1. En `launcher.config.toml`:

```toml
[updater]
enabled     = true
release_url = "https://github.com/TU_ORG/mi-launcher/releases/latest/download/latest.json"
```

2. El workflow de CI ya genera y sube `latest.json` automáticamente.

3. Distribuye el launcher a tus jugadores — las actualizaciones futuras se instalarán solas.

---

## 12. Desplegar el servidor de manifest

La forma más sencilla es servir un archivo JSON estático desde GitHub Pages, Cloudflare Pages, o cualquier hosting.

Para un servidor con más control (lógica, autenticación, etc.) usa el ejemplo en Rust:

```bash
cd manifest-server-examples/rust-server
cargo run
# Sirve en http://localhost:3000/manifest.json
```

En `launcher.config.toml`:
```toml
[server]
manifest_provider = "http"
manifest_url      = "https://api.miservidor.com/manifest.json"
```

Edita `manifest-server-examples/rust-server/manifest.json` con los mods de tu servidor.

---

## 13. Referencia de `launcher.config.toml`

### `[branding]`

| Campo | Tipo | Descripción |
|-------|------|-------------|
| `internal_id` | string | Identificador único (AppData path). **Permanente.** |
| `display_name` | string | Nombre visible en la UI |
| `window_title` | string | Título de la ventana |
| `primary_color` | `#RRGGBB` | Color de botones y acentos principales |
| `secondary_color` | `#RRGGBB` | Color de fondo |
| `accent_color` | `#RRGGBB` | Color de acentos secundarios |
| `logo` | filename | Logo en `assets/` |
| `background` | filename | Fondo en `assets/` |

### `[server]`

| Campo | Tipo | Descripción |
|-------|------|-------------|
| `manifest_provider` | `"http"` \| `"file"` \| `"git"` | Origen del manifest |
| `manifest_url` | string | URL o path del manifest |
| `manifest_public_key` | hex string | Clave Ed25519 para verificar firma del manifest. Vacío = sin verificación |
| `update_check_interval_secs` | int | Cada cuántos segundos re-descarga el manifest (default 300) |

### `[updater]`

| Campo | Tipo | Descripción |
|-------|------|-------------|
| `enabled` | bool | Activa el auto-updater del launcher |
| `release_url` | string | URL del endpoint de Tauri updater |
| `release_public_key` | string | Clave pública minisign (de `updater.key.pub`) |

### `[features]`

| Campo | Default | Descripción |
|-------|---------|-------------|
| `allow_optional_mods` | `true` | Muestra la pantalla de mods opcionales |
| `allow_ram_config` | `true` | Permite al jugador ajustar RAM |
| `allow_jvm_args_edit` | `false` | Permite editar args JVM |
| `allow_java_path_override` | `true` | Permite especificar ruta de Java |

### `[runtime]`

| Campo | Default | Descripción |
|-------|---------|-------------|
| `ram_min_mb` | 2048 | RAM mínima en el slider (MB) |
| `ram_max_mb` | 16384 | RAM máxima en el slider (MB) |
| `ram_default_mb` | 4096 | RAM por defecto al crear perfil nuevo |
| `default_jvm_args` | (G1GC flags) | Argumentos JVM por defecto |
| `download_concurrency` | 8 | Descargas paralelas |

### `[java]`

| Campo | Valores | Descripción |
|-------|---------|-------------|
| `strategy` | `detect_or_download` \| `always_download` \| `system_only` | Cómo gestionar Java |
| `distribution` | `temurin` \| `zulu` \| `graalvm` | Distribución a descargar (solo `detect_or_download` y `always_download`) |

---

*¿Preguntas? Abre un issue en el repositorio.*
