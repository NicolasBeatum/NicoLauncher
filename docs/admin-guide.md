# Guía del administrador — Gestión del modpack

Esta guía cubre todo lo que necesitas para crear, firmar y actualizar el manifest
de tu servidor usando el CLI `mc-launcher`.

> **¿Buscas cómo personalizar el launcher en sí?**
> Consulta [`docs/customization-guide.md`](customization-guide.md).

---

## Índice

1. [¿Cómo funciona el sistema de mods?](#1-cómo-funciona-el-sistema-de-mods)
2. [Instalar el CLI](#2-instalar-el-cli)
3. [Setup inicial del modpack](#3-setup-inicial-del-modpack)
4. [Ciclo de actualización habitual](#4-ciclo-de-actualización-habitual)
5. [Mods opcionales del servidor](#5-mods-opcionales-del-servidor)
6. [Firmar el manifest (seguridad)](#6-firmar-el-manifest-seguridad)
7. [Publicar en el hosting](#7-publicar-en-el-hosting)
8. [Referencia de `lockfile.toml`](#8-referencia-de-lockfiletoml)
9. [Solución de problemas](#9-solución-de-problemas)

---

## 1. ¿Cómo funciona el sistema de mods?

```
Admin                         Jugador
──────────────────────────    ─────────────────────────────
lockfile.toml                 launcher.config.toml
     │                              │
     ▼                              ▼
mc-launcher manifest update   descarga manifest-signed.json
     │                              │
     ▼                              ▼
manifest.json ─── sign ──► manifest-signed.json ─► verifica firma
                                    │
                                    ▼
                              descarga mods de Modrinth/CDN
                                    │
                                    ▼
                              lanza Minecraft
```

El **manifest** es un fichero JSON que describe exactamente:
- Versión de Minecraft y mod loader
- Lista de mods requeridos (con hash SHA-512 y URL de descarga)
- Mods opcionales que el jugador puede activar o desactivar
- Configs a copiar a `.minecraft/`

El launcher lo descarga al arrancar, lo compara con el estado instalado
y sincroniza solo lo que ha cambiado.

---

## 2. Instalar el CLI

### Desde código fuente (requiere Rust)

```bash
# En la raíz del repositorio del launcher:
cargo build --release -p mc-launcher-admin
```

El binario queda en `target/release/mc-launcher` (o `mc-launcher.exe` en Windows).

Para tenerlo disponible globalmente:

```bash
# Linux / macOS
cargo install --path crates/admin-cli

# Windows — copia el .exe a una carpeta en PATH, por ejemplo:
copy target\release\mc-launcher.exe C:\Tools\mc-launcher.exe
```

### Verificar instalación

```
mc-launcher --version
mc-launcher manifest --help
```

---

## 3. Setup inicial del modpack

Sigue estos pasos **una sola vez** cuando configures el modpack por primera vez.

### 3.1 Crear el par de claves de firma

La firma Ed25519 garantiza que los jugadores reciben el manifest que tú publicaste,
sin que nadie lo haya modificado en el camino.

```bash
mc-launcher sign gen-keys
```

Genera dos archivos:
- `signing.key` — **clave privada.** ¡No la subas al repositorio nunca!
  Guárdala en un lugar seguro (gestión de secretos, variable de entorno en CI…)
- `public.key` — clave pública. Va en `launcher.config.toml`.

```
✅  Par de claves generado:
   Clave privada → signing.key
   Clave pública → public.key

⚠️   IMPORTANTE:
   • Añade signing.key a .gitignore
   • En CI, guarda el contenido de signing.key como secreto SIGNING_KEY
   • Copia el contenido de public.key en launcher.config.toml:
     manifest_public_key = "950322f8beb87a14fb005599cab76d9c74f7db0b0ba86943004e54ef133cd43a"
```

Añade la clave pública a `launcher.config.toml`:

```toml
[server]
manifest_public_key = "950322f8beb87a14..."   # ← contenido de public.key
```

> **En modo desarrollo** puedes dejar `manifest_public_key = ""` y el launcher
> no verificará la firma (útil para pruebas locales).

### 3.2 Preparar las carpetas de mods

Crea la estructura de carpetas de tu modpack:

```
mi-modpack/
├── mods/               ← mods requeridos (.jar)
├── optional-mods/      ← mods opcionales del servidor (.jar)  [opcional]
├── shaderpacks/        ← shaderpacks (.zip)                   [opcional]
├── resourcepacks/      ← resourcepacks (.zip/.jar)            [opcional]
└── configs/            ← configs a copiar a .minecraft/       [opcional]
```

Coloca los `.jar` de tus mods en `mods/`.

### 3.3 Crear `lockfile.toml`

El asistente interactivo te pregunta toda la configuración y la guarda en
`lockfile.toml` para que no tengas que repetirla en cada actualización:

```bash
cd mi-modpack/
mc-launcher manifest init
```

```
🚀  Asistente de configuración — lockfile.toml
────────────────────────────────────────────────────────────
  Presiona Enter para aceptar el valor entre corchetes.

── Proyecto ──────────────────────────────────────────────────
  Versión de Minecraft (ej: 1.21.1) [1.21.1]: 1.21.4
  Versión de Java requerida [21]:
  Mod loader (neoforge / fabric / forge / vanilla) [neoforge]:
  Versión del loader (vacío = latest): 21.4.60

── Carpetas ──────────────────────────────────────────────────
  Carpeta de mods requeridos (vacío = ninguna) [mods]:
  ¿Incluir mods opcionales del servidor? [s/N]: s
    Carpeta de mods opcionales [optional-mods]:
  ¿Incluir shaderpacks? [s/N]:
  ¿Incluir resourcepacks? [s/N]:
  ¿Incluir configs (.minecraft/)? [s/N]:
  Archivo de salida [manifest.json]:

── Distribución ──────────────────────────────────────────────
  URL base del servidor (ej: https://cdn.example.com): https://files.miservidor.com
  Modo de aplicación para configs (if_missing / always) [if_missing]:

✅  lockfile.toml creado.
```

### 3.4 Generar el manifest por primera vez

```bash
mc-launcher manifest update
```

El CLI:
1. Escanea las carpetas configuradas
2. Consulta la API de Modrinth para obtener nombres y URLs oficiales de los mods
3. Muestra un resumen de lo encontrado
4. Pregunta si escribir `manifest.json`

```
📋  lockfile.toml
   MC 1.21.4 · neoforge 21.4.60  →  manifest.json
   Sin manifest anterior — se generará uno nuevo.

🔍  Escaneando archivos...
   Mods requeridos:  18 archivos .jar
   Mods opcionales:  3 archivos .jar

🌐  Consultando Modrinth (21 archivos nuevos/modificados)...

── Mods requeridos ────────────────────────────────────────────
  ➕  Sodium 0.6.1       [Modrinth]
  ➕  Lithium 0.13.0     [Modrinth]
  ➕  Iris 1.8.0         [Modrinth]
  ...

── Mods opcionales ────────────────────────────────────────────
  ➕  Bobby 5.2.0        [Modrinth]
  ...

  ¿Escribir manifest.json? [S/n]:

✅  manifest.json generado  (versión 2025.05.13-1, 18 mods requeridos, 3 opcionales)
```

### 3.5 Validar el manifest

```bash
mc-launcher sign validate manifest.json
```

Comprueba estructura, hashes, dependencias y paths seguros.
Añade `--check-urls` para verificar que todas las URLs son accesibles:

```bash
mc-launcher sign validate manifest.json --check-urls
```

### 3.6 Firmar el manifest

```bash
mc-launcher sign sign manifest.json
```

Genera `manifest-signed.json`. Este es el archivo que debes publicar.

### 3.7 Subir al hosting

Sube al hosting **todos los archivos** que el manifest referencia (URLs
`self_hosted_url/…`) y el propio `manifest-signed.json`.

Estructura típica en el CDN:

```
https://files.miservidor.com/
├── manifest-signed.json   ← apunta launcher.config.toml aquí
├── sodium-0.6.1.jar       ← mods self-hosted
├── mi-mod-custom.jar
└── configs/
    └── options.txt
```

Los mods de Modrinth no necesitan hosting propio — el launcher los descarga
directamente desde los CDNs de Modrinth.

### 3.8 Configurar el launcher

Edita `launcher.config.toml` con la URL del manifest firmado:

```toml
[server]
manifest_provider   = "http"
manifest_url        = "https://files.miservidor.com/manifest-signed.json"
manifest_public_key = "950322f8beb87a14..."   # contenido de public.key
```

O si el manifest está en un repositorio Git:

```toml
[server]
manifest_provider = "git"
manifest_url      = "https://github.com/tu-org/modpack/blob/main/manifest-signed.json"
```

El launcher convierte automáticamente las URLs de GitHub/GitLab a su versión raw.

---

## 4. Ciclo de actualización habitual

Cada vez que actualices el modpack (añadir/quitar mods, actualizar versiones):

```bash
cd mi-modpack/

# 1. Actualiza los .jar en mods/ y/o optional-mods/

# 2. Regenerar el manifest (solo consulta Modrinth para mods nuevos/modificados)
mc-launcher manifest update

# 3. Validar
mc-launcher sign validate manifest.json

# 4. Firmar
mc-launcher sign sign manifest.json

# 5. Subir manifest-signed.json al hosting
```

El comando `update` **es incremental**: si 17 de 18 mods no han cambiado,
solo hace una consulta a Modrinth para el mod nuevo. El resto se reutiliza
del `manifest.json` anterior sin petición de red.

### Ejemplo de diff típico

```
📋  lockfile.toml
   Manifest anterior: versión 2025.05.10-1 (18 mods requeridos, 3 opcionales)

── Mods requeridos ────────────────────────────────────────────
  ✅  Sodium 0.6.1           (sin cambios)
  ✅  Lithium 0.13.0         (sin cambios)
  🔄  Iris 1.8.0 → 1.8.1    (actualizado)  [Modrinth]
  ❌  OptiFine 1.21.4_HD     (eliminado)
  ➕  Nvidium 0.3.4          (nuevo)        [Modrinth]
  ...

  ¿Escribir manifest.json? [S/n]:
```

### Uso en CI (sin prompts)

```bash
mc-launcher manifest update --yes
mc-launcher sign validate manifest.json
mc-launcher sign sign manifest.json --key $SIGNING_KEY_PATH
```

---

## 5. Mods opcionales del servidor

Los mods opcionales aparecen en el launcher con un toggle que los jugadores
pueden activar o desactivar. Se descargan por demanda (solo si el jugador
los activa) y nunca se tocan en un sync normal si están desactivados.

### 5.1 Añadir mods opcionales

1. Coloca los `.jar` en la carpeta `optional-mods/` (configurada en `lockfile.toml`)
2. Ejecuta `mc-launcher manifest update` — los detectará automáticamente

### 5.2 Configurar metadatos

Por defecto, los mods opcionales heredan nombre e icono de Modrinth.
Puedes añadir o sobreescribir metadatos en `lockfile.toml`:

```toml
# Al final de lockfile.toml, una sección por mod:

[[optional_mod]]
id              = "bobby"          # slug de Modrinth o ID generado del filename
default_enabled = true             # activado por defecto para nuevos jugadores
category        = "gameplay"       # "performance" | "visuals" | "gameplay" | …
description     = "Amplía el render distance mucho más allá del límite del servidor"

[[optional_mod]]
id              = "iris"
default_enabled = false
category        = "visuals"
description     = "Shaders con excelente rendimiento"
depends_on      = ["sodium"]       # requiere que Sodium esté activado

[[optional_mod]]
id              = "optifine"
default_enabled = false
category        = "visuals"
conflicts_with  = ["iris"]         # incompatible con Iris
```

**Campos disponibles:**

| Campo | Tipo | Descripción |
|---|---|---|
| `id` | string | **Obligatorio.** Slug de Modrinth o ID del filename |
| `default_enabled` | bool | Si está activado para jugadores nuevos (default: `false`) |
| `category` | string | Etiqueta visible en la UI |
| `description` | string | Descripción corta que ven los jugadores |
| `icon_url` | string | URL de icono personalizado (por defecto: el de Modrinth) |
| `depends_on` | array | IDs de mods que deben estar activados para que este funcione |
| `conflicts_with` | array | IDs de mods incompatibles con este |

### 5.3 Cómo ve el jugador los mods opcionales

En el launcher, la pantalla "Mods opcionales → Del servidor" muestra cada mod
con su toggle. Al activar uno:

- Si **ya está en caché** (descargado antes) → se activa instantáneamente
- Si **no está en caché** → se marca como pendiente y se descarga en el próximo sync

Si un mod tiene dependencias no activadas, el launcher las propone automáticamente.
Si entra en conflicto con otro mod activo, propone desactivarlo.

---

## 6. Firmar el manifest (seguridad)

La firma Ed25519 garantiza integridad y autenticidad del manifest. El launcher
rechaza cualquier manifest cuya firma no corresponda a la clave pública configurada.

### Flujo completo

```bash
# Una sola vez: generar par de claves
mc-launcher sign gen-keys

# Cada actualización:
mc-launcher sign sign manifest.json              # genera manifest-signed.json
mc-launcher sign verify manifest-signed.json     # verificación local

# Si quieres validar antes de firmar:
mc-launcher sign validate manifest.json --check-urls
```

### Guardar la clave privada en CI

En GitHub Actions / GitLab CI, guarda el contenido de `signing.key` como secreto:

- GitHub: **Settings → Secrets → Actions → New repository secret**
  - Nombre: `MANIFEST_SIGNING_KEY`
  - Valor: contenido del archivo `signing.key` (una línea hex)

En el workflow:

```yaml
- name: Sign manifest
  run: |
    echo "${{ secrets.MANIFEST_SIGNING_KEY }}" > /tmp/signing.key
    mc-launcher sign sign manifest.json --key /tmp/signing.key
    rm /tmp/signing.key
```

### Modo sin firma (desarrollo)

Deja `manifest_public_key = ""` en `launcher.config.toml`.
El launcher cargará el manifest sin verificar la firma.
**No uses esto en producción.**

---

## 7. Publicar en el hosting

### Opciones de hosting

| Opción | Precio | Configuración |
|---|---|---|
| **Cloudflare R2** | Gratis hasta 10 GB/mes | Bucket público + CORS |
| **GitHub Releases** | Gratis | Subir archivos como assets |
| **Bunny CDN** | ~$0.01/GB | Storage zone pública |
| **Tu propio servidor** | Depende | Nginx/Caddy sirviendo estáticos |
| **GitHub Pages** | Gratis | Solo archivos pequeños (<100 MB) |

### Lo que debes subir

1. `manifest-signed.json` (o `manifest.json` si no usas firma)
2. Todos los mods con `source.url` que apunten a tu CDN (los de Modrinth no)
3. Todos los `config_overrides` con URLs en tu CDN

### Estructura recomendada

```
CDN raíz (self_hosted_url en lockfile.toml)
├── manifest-signed.json
├── mods/
│   ├── mi-mod-custom-1.0.jar
│   └── otro-mod-privado.jar
└── configs/
    ├── options.txt
    └── config/
        └── sodium-options.json
```

Con `self_hosted_url = "https://cdn.miservidor.com"` en `lockfile.toml`,
el manifest generará URLs como:
- `https://cdn.miservidor.com/mi-mod-custom-1.0.jar`
- `https://cdn.miservidor.com/configs/options.txt`

---

## 8. Referencia de `lockfile.toml`

```toml
# ── Proyecto ───────────────────────────────────────────────────────────────────
[project]
mc_version    = "1.21.4"      # Versión de Minecraft — OBLIGATORIO
java_version  = 21            # Major de Java requerido (default: 21)
loader        = "neoforge"    # neoforge | fabric | forge | vanilla | quilt
loader_version = "21.4.60"   # Versión del loader (omitir = "latest")

# ── Carpetas ───────────────────────────────────────────────────────────────────
[paths]
mods          = "mods"           # Mods requeridos (.jar)
optional_mods = "optional-mods"  # Mods opcionales del servidor (.jar)
shaderpacks   = "shaderpacks"    # Shaderpacks (.zip)
resourcepacks = "resourcepacks"  # Resourcepacks (.zip/.jar)
configs       = "configs"        # Configs a copiar en .minecraft/ (recursivo)
output        = "manifest.json"  # Archivo de salida (default: manifest.json)

# ── Distribución ───────────────────────────────────────────────────────────────
[hosting]
self_hosted_url = "https://cdn.miservidor.com"
# URL base para mods no encontrados en Modrinth y para todos los configs.
# Genera: https://cdn.miservidor.com/<filename>

apply_mode = "if_missing"
# "if_missing" → aplica configs/shaderpacks/resourcepacks solo si el jugador
#                no los tiene (recomendado — respeta cambios locales del jugador)
# "always"     → sobreescribe siempre (útil para configs críticas)

# ── Mods opcionales: metadatos adicionales ─────────────────────────────────────
# Una sección [[optional_mod]] por cada mod que quieras configurar.
# Los mods sin sección heredan nombre/icono de Modrinth.

[[optional_mod]]
id              = "sodium"
default_enabled = true
category        = "performance"
description     = "Mejora drásticamente el rendimiento gráfico"

[[optional_mod]]
id              = "iris"
default_enabled = false
category        = "visuals"
description     = "Shaders modernos con excelente rendimiento"
depends_on      = ["sodium"]
conflicts_with  = ["optifine"]

[[optional_mod]]
id              = "optifine"
default_enabled = false
category        = "visuals"
description     = "Mod clásico de optimización y shaders"
conflicts_with  = ["iris"]
```

---

## 9. Solución de problemas

### "No se encontró Modrinth para X.jar"

El mod no está en Modrinth (es privado o custom). Soluciones:

1. **Usar self_hosted_url**: configura `self_hosted_url` en `lockfile.toml`
   y sube el archivo a tu CDN. El manifest generará la URL automáticamente.
2. **Editar `manifest.json` manualmente**: busca la entrada con `"TU_SERVIDOR"` y
   reemplaza la URL.

### El launcher dice "Firma inválida"

- Comprueba que `manifest_public_key` en `launcher.config.toml` coincide exactamente
  con el contenido de `public.key`.
- Asegúrate de subir el archivo `manifest-signed.json` generado por
  `mc-launcher sign sign`, no el `manifest.json` sin firmar.
- Verifica localmente: `mc-launcher sign verify manifest-signed.json`

### El launcher descarga los mods aunque ya los tiene

El sync compara hashes SHA-512. Si el hash en el manifest no coincide con el
archivo en disco, descarga de nuevo. Causas habituales:

- Se generó el manifest con un `.jar` diferente al que tienen los jugadores
- El `.jar` fue modificado después de generar el manifest

Genera siempre el manifest con los mismos archivos que distribuyes.

### `manifest update` no detecta cambios en un mod

El diff es por SHA-512. Si el archivo `.jar` tiene el mismo hash
(mismo contenido) que la versión en el manifest anterior, se marcará como
"sin cambios" aunque hayas renombrado el archivo. Esto es correcto.

### Quiero actualizar solo el manifest sin cambiar los mods

Edita el `manifest.json` directamente (por ejemplo, para cambiar el
`announcement`) y vuelve a firmarlo:

```bash
mc-launcher sign sign manifest.json
```

No necesitas ejecutar `manifest update` si no cambiaron los archivos.

### Modo dev: lanzar sin manifest de servidor

En `launcher.config.toml`:

```toml
[server]
manifest_provider = "file"
manifest_url      = ""         # vacío = sin manifest
```

El launcher lanzará Minecraft con los mods que tenga instalados localmente,
sin sync.

---

## Flujo completo de referencia rápida

```bash
# ── Una sola vez ───────────────────────────────────────────────
mc-launcher sign gen-keys              # genera signing.key + public.key
mc-launcher manifest init              # crea lockfile.toml (asistente interactivo)

# ── Cada actualización del modpack ─────────────────────────────
mc-launcher manifest update            # genera manifest.json con diff incremental
mc-launcher sign validate manifest.json [--check-urls]
mc-launcher sign sign manifest.json    # genera manifest-signed.json

# subir manifest-signed.json y archivos nuevos al CDN

# ── Comprobaciones adicionales ─────────────────────────────────
mc-launcher sign verify manifest-signed.json    # verifica firma local
mc-launcher sign validate manifest-signed.json  # funciona también con manifests firmados
```
