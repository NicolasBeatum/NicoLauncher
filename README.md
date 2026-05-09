# mc-launcher-template

> **Template** de launcher de Minecraft modeado escrito en **Rust + Tauri 2 + SvelteKit**.  
> Edita `launcher.config.toml` + assets, compila, y tienes un launcher con tu propia marca.

---

## Inicio rápido

```bash
# 1. Edita launcher.config.toml con los datos de tu servidor
# 2. Pon tus assets en assets/ (logo, background, icon)
# 3. Compila
cargo build --release
```

## Estructura

```
crates/
  core/            # tipos, errores, paths, hashing
  meta/            # metadatos de Mojang (versiones, assets, libs)
  auth/            # Microsoft OAuth + perfil de Minecraft
  loaders/         # Fabric, Quilt, NeoForge, Forge
  mods/            # Modrinth, CurseForge, SelfHosted
  manifest-client/ # manifest del servidor + sync diff
  downloader/      # descargas paralelas con verificación
  java-manager/    # detección y descarga de JDK
  launcher/        # construcción de classpath y lanzado del juego
  admin-cli/       # CLI para admins: firmar manifests, publicar updates
src-tauri/         # app de escritorio Tauri (Fase 4)
ui/                # frontend SvelteKit (Fase 4)
assets/            # logo, background, icon
```

## Personalización

Edita [`launcher.config.toml`](launcher.config.toml). Campos obligatorios:

| Campo | Descripción |
|---|---|
| `branding.internal_id` | ID único (afecta paths en AppData — no cambiar tras desplegar) |
| `branding.display_name` | Nombre visible del launcher |
| `server.address` | IP/dominio del servidor |
| `server.manifest_url` | URL donde hosteas el manifest del modpack |

Ver [`docs/customization-guide.md`](docs/customization-guide.md) para el guía completa.

## Licencia

MIT OR Apache-2.0
