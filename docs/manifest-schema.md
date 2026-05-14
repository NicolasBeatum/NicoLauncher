# Schema del manifest — MC Launcher Template

El manifest es el archivo JSON que el administrador del servidor publica y el launcher descarga al arrancar. Define qué mods, configs y loader deben estar instalados.

> Para aprender a generar y firmar un manifest, consulta [`admin-guide.md`](admin-guide.md).

---

## Versión del schema

El campo `manifest_version` es actualmente `"1"`. Las versiones futuras podrían añadir campos. El launcher ignora campos desconocidos para compatibilidad hacia adelante.

---

## Schema completo

```jsonc
{
  // ── Metadatos ────────────────────────────────────────────────────────────
  "manifest_version": "1",            // string, requerido
  "generated_at": "2026-01-15T12:00:00Z", // ISO-8601, opcional (informativo)
  "version": "2026.01.15-1",          // string, opcional (versión del modpack)

  // ── Minecraft + Loader ───────────────────────────────────────────────────
  "mc_version": "1.21.1",             // string, requerido
  "java_version": 21,                 // número, requerido (versión mayor de Java)
  "loader_type": "fabric",            // "fabric" | "quilt" | "neoforge" | "forge" | "vanilla"
  "loader_version": "0.16.5",         // string, requerido si loader_type != "vanilla"

  // ── Mods requeridos ──────────────────────────────────────────────────────
  // Se instalan siempre. El jugador no puede desactivarlos.
  "required_mods": [
    {
      "id": "fabric-api",             // string, único dentro del manifest
      "name": "Fabric API",           // string, nombre visible
      "version": "0.100.0+1.21.1",    // string, versión (informativo)
      "url": "https://cdn.modrinth.com/data/.../fabric-api-0.100.0+1.21.1.jar",
      "sha512": "abc123...",           // hex, 128 chars, requerido
      "filename": "fabric-api-0.100.0+1.21.1.jar", // nombre del archivo en mods/
      "side": "client"                // "client" | "server" | "both", default "both"
    }
  ],

  // ── Mods opcionales ──────────────────────────────────────────────────────
  // El jugador puede activarlos o desactivarlos. Los activos se descargan.
  "optional_mods": [
    {
      "id": "sodium",                 // string, único dentro del manifest
      "name": "Sodium",
      "description": "Mejora el rendimiento gráfico (FPS).",
      "category": "Rendimiento",      // string libre, agrupa en la UI
      "icon_url": "https://cdn.modrinth.com/data/AANobbMI/icon.png", // opcional
      "default_enabled": true,        // bool, estado inicial
      "depends_on": [],               // [string], IDs de mods que deben estar activos
      "conflicts_with": ["optifine"], // [string], IDs incompatibles
      "version": "0.5.11+mc1.21.1",
      "url": "https://cdn.modrinth.com/data/.../sodium-fabric-0.5.11+mc1.21.1.jar",
      "sha512": "def456...",
      "filename": "sodium-fabric-0.5.11+mc1.21.1.jar",
      "side": "client"
    }
  ],

  // ── Configs ──────────────────────────────────────────────────────────────
  // Archivos que se copian dentro de .minecraft/ (o la carpeta del perfil).
  "configs": [
    {
      "path": "config/sodium-options.json", // ruta relativa a .minecraft/
      "url": "https://mi-servidor.com/configs/sodium-options.json",
      "sha512": "ghi789...",
      "apply_mode": "if_missing"      // "if_missing" | "always" | "never"
    }
  ],

  // ── Shaderpacks ──────────────────────────────────────────────────────────
  "shaderpacks": [
    {
      "id": "complementary",
      "name": "Complementary Shaders",
      "filename": "ComplementaryShaders_v5.3.zip",
      "url": "https://cdn.modrinth.com/data/.../ComplementaryShaders_v5.3.zip",
      "sha512": "jkl012...",
      "apply_mode": "if_missing"
    }
  ],

  // ── Resource packs ───────────────────────────────────────────────────────
  "resourcepacks": [
    {
      "id": "faithful",
      "name": "Faithful 32x",
      "filename": "Faithful32x-1.21.1.zip",
      "url": "https://cdn.faithfulpack.net/...",
      "sha512": "mno345...",
      "apply_mode": "if_missing"
    }
  ],

  // ── Archivos a eliminar ──────────────────────────────────────────────────
  // El launcher elimina estos archivos si existen (útil para limpiar mods viejos).
  "files_to_delete": [
    "mods/optifine-old.jar",
    "config/legacy-config.json"
  ],

  // ── Anuncio ──────────────────────────────────────────────────────────────
  // Se muestra como banner en la pantalla principal. Null para ocultar.
  "announcement": {
    "title": "¡Actualización disponible!",
    "body_md": "## Novedades\n\n- Nuevo mapa de aventura\n- Fixes de rendimiento",
    "color": "#7c3aed",             // color del banner (hex)
    "expires_at": "2026-02-01T00:00:00Z" // null = nunca expira
  }
}
```

---

## Campos requeridos vs opcionales

| Campo | Requerido | Default si ausente |
|-------|-----------|-------------------|
| `manifest_version` | Sí | — |
| `mc_version` | Sí | — |
| `java_version` | Sí | — |
| `loader_type` | Sí | — |
| `loader_version` | Si loader != vanilla | — |
| `required_mods` | No | `[]` |
| `optional_mods` | No | `[]` |
| `configs` | No | `[]` |
| `shaderpacks` | No | `[]` |
| `resourcepacks` | No | `[]` |
| `files_to_delete` | No | `[]` |
| `announcement` | No | `null` |

---

## Campos de entradas de mods

### Mods requeridos

| Campo | Tipo | Requerido | Descripción |
|-------|------|-----------|-------------|
| `id` | string | Sí | Identificador único en el manifest |
| `name` | string | Sí | Nombre visible en logs y UI |
| `version` | string | No | Versión informativa |
| `url` | string | Sí | URL de descarga directa |
| `sha512` | string | Sí | Hash SHA-512 en hex (128 chars) |
| `filename` | string | Sí | Nombre del archivo en `mods/` |
| `side` | `"client"` \| `"server"` \| `"both"` | No | Default: `"both"` |

### Mods opcionales (campos adicionales)

| Campo | Tipo | Requerido | Descripción |
|-------|------|-----------|-------------|
| `description` | string | No | Descripción corta para la UI |
| `category` | string | No | Grupo en la UI |
| `icon_url` | string | No | URL de icono 64×64+ |
| `default_enabled` | bool | No | Default: `false` |
| `depends_on` | `[string]` | No | IDs que deben estar activos |
| `conflicts_with` | `[string]` | No | IDs incompatibles |

### Configs

| Campo | Tipo | Requerido | Descripción |
|-------|------|-----------|-------------|
| `path` | string | Sí | Ruta relativa a `.minecraft/` |
| `url` | string | Sí | URL de descarga |
| `sha512` | string | Sí | Hash SHA-512 |
| `apply_mode` | `"if_missing"` \| `"always"` \| `"never"` | No | Default: `"if_missing"` |

---

## `apply_mode` explicado

| Valor | Comportamiento |
|-------|----------------|
| `"if_missing"` | Solo se copia si el archivo **no existe** en `.minecraft/`. El jugador puede modificarlo sin que se sobreescriba. |
| `"always"` | Se sobreescribe siempre en cada sync. Útil para configs críticas del servidor. |
| `"never"` | El archivo está en el manifest pero **no se instala**. Útil para configs opcionales documentadas. |

---

## Manifest firmado

Cuando el manifest pasa por `mc-launcher sign sign`, el JSON resultante tiene esta estructura:

```json
{
  "signature": "base64url-encoded-ed25519-signature",
  "signed_at": "2026-01-15T12:00:00Z",
  "payload": { ...manifest original... }
}
```

El launcher verifica la firma con la clave pública definida en `launcher.config.toml → [server] → manifest_public_key` antes de procesar el payload.

Si `manifest_public_key` está vacío, la firma se omite (modo sin verificación — no recomendado en producción).

---

## Validación con el CLI

```bash
# Validación completa (sin peticiones HTTP)
mc-launcher sign validate manifest.json

# Validación + verificar que todas las URLs responden
mc-launcher sign validate manifest.json --check-urls
```

El validador comprueba:
- Schema y tipos de datos
- Hash SHA-512 con formato correcto (128 hex chars)
- Dependencias declaradas existen en el manifest
- Conflictos simétricos (si A conflicts_with B, B debería declarar conflicts_with A)
- URLs accesibles (solo con `--check-urls`)

---

*Para publicar manifests paso a paso, consulta [`publishing-manifests.md`](publishing-manifests.md).*
