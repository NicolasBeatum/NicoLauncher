# Seguridad — MC Launcher Template

Este documento describe el modelo de seguridad del launcher: cómo se protegen las claves, cómo se verifican los manifests y qué superficie de ataque existe.

---

## Firma del manifest (Ed25519)

### Por qué firmar

El manifest controla qué archivos se instalan en el equipo del jugador. Sin firma, un atacante que comprometa el CDN o el servidor podría sustituir el manifest por uno malicioso que instale JARs arbitrarios.

### Cómo funciona

```
Admin                      CDN / servidor              Launcher (jugador)
  │                             │                             │
  ├─ genera manifest.json       │                             │
  ├─ firma con clave privada ──►│ manifest-signed.json        │
  │  (Ed25519, local)           │                             │
  │                             ├────────────────────────────►│
  │                             │   GET manifest-signed.json  │
  │                             │                             ├─ verifica firma
  │                             │                             │  (clave pública)
  │                             │                             ├─ procesa payload
```

La clave privada **nunca sale del equipo del admin**. El launcher solo conoce la clave pública (compilada en el binario a través de `launcher.config.toml` → `build.rs`).

### Generar el par de claves

```bash
mc-launcher sign gen-keys
# → signing.key    (PRIVADA — no compartir, no subir)
# → public.key     (pública — va en launcher.config.toml)
```

### Firmar el manifest

```bash
mc-launcher sign sign manifest.json -k signing.key
# → manifest-signed.json
```

El archivo firmado tiene esta estructura:

```json
{
  "signature": "<base64url Ed25519 signature>",
  "signed_at": "2026-01-15T12:00:00Z",
  "payload": { ...manifest original... }
}
```

La firma cubre el contenido completo de `payload` serializado con claves ordenadas (JSON canónico).

### Verificar

```bash
mc-launcher sign verify manifest-signed.json -k public.key
```

### Configurar en el launcher

En `launcher.config.toml`:
```toml
[server]
manifest_public_key = "hex-encoded-32-byte-public-key"
```

Si `manifest_public_key` está vacío, el launcher acepta manifests sin firma (**no recomendado en producción**).

---

## Verificación de integridad de archivos (SHA-512)

Cada entrada de mod en el manifest incluye `sha512`. El launcher verifica el hash **después de descargar** y **antes de linkear** al directorio de mods:

```
descarga → .partial → verifica SHA-512 → renombra a dest
                            │
                    mismatch → borra .partial
                               log error
                               no instala el mod
```

Esto protege contra:
- Corrupciones de red (parciales, truncados)
- CDN comprometido que sirve archivos modificados (si la firma del manifest no está habilitada, la verificación de hash sigue protegiéndote a nivel de archivo)
- Ataques man-in-the-middle (en combinación con HTTPS)

**El hash SHA-512 debe ser el del archivo JAR real**, no del manifest. Genera los hashes con:

```bash
mc-launcher manifest update   # los obtiene automáticamente de Modrinth
# o manualmente:
sha512sum archivo.jar
```

---

## Auto-updater del launcher (Tauri Updater)

El launcher puede actualizarse a sí mismo usando el sistema de Tauri. Los instaladores se firman con **minisign** (clave separada de la del manifest):

```bash
# El script de init genera esta clave:
.\scripts\init-template.ps1
# → updater.key     (PRIVADA — va en GitHub Secret TAURI_SIGNING_PRIVATE_KEY)
# → updater.key.pub (pública — va en launcher.config.toml)
```

En `launcher.config.toml`:
```toml
[updater]
enabled            = true
release_url        = "https://github.com/TU_ORG/mi-launcher/releases/latest/download/latest.json"
release_public_key = "contenido de updater.key.pub"
```

El workflow de CI firma los instaladores automáticamente usando `TAURI_SIGNING_PRIVATE_KEY`.

---

## Almacenamiento de credenciales (OAuth)

Los tokens de Microsoft se almacenan en el **keyring del sistema operativo**:

| OS | Almacén |
|----|---------|
| Windows | Windows Credential Manager |
| macOS | Keychain |
| Linux | Secret Service (libsecret) / KWallet |

Los tokens **nunca se escriben en disco** en texto plano. El `refresh_token` se usa para renovar la sesión sin re-login manual.

---

## Protección de claves privadas

| Clave | Descripción | Dónde va |
|-------|-------------|----------|
| `signing.key` | Firma manifests | Solo equipo del admin. En `.gitignore`. |
| `updater.key` | Firma instaladores del launcher | Solo como GitHub Secret `TAURI_SIGNING_PRIVATE_KEY`. En `.gitignore`. |

El `.gitignore` del proyecto excluye `*.key` para evitar commits accidentales.

> **Si una clave privada se compromete:**
> - **`signing.key`**: genera un nuevo par, actualiza `manifest_public_key` en el config, publica una nueva release del launcher.
> - **`updater.key`**: genera un nuevo par, actualiza `release_public_key` en el config y `TAURI_SIGNING_PRIVATE_KEY` en GitHub Secrets, publica una nueva release. Los launchers anteriores necesitarán actualización manual.

---

## Content Security Policy (CSP)

Tauri aplica una CSP estricta en la WebView. La configuración está en `src-tauri/tauri.conf.json`:

```json
{
  "app": {
    "security": {
      "csp": "default-src 'self'; img-src 'self' https: data:; style-src 'self' 'unsafe-inline'"
    }
  }
}
```

Esto impide que el frontend cargue scripts externos o ejecute código inline no autorizado. Si necesitas cargar recursos externos (avatares de Minecraft, iconos de mods desde Modrinth), añade los dominios a la CSP explícitamente.

---

## Superficie de ataque y mitigaciones

| Vector | Mitigación |
|--------|-----------|
| Manifest comprometido en CDN | Firma Ed25519 verificada en el launcher |
| Archivo JAR corrupto o modificado | Verificación SHA-512 post-descarga |
| Installer del launcher falso | Firma minisign del auto-updater |
| Tokens de sesión robados | Keyring del OS (no en disco plano) |
| Script injection en la WebView | CSP estricta de Tauri |
| Clave privada en el repo | `.gitignore` + scripts de init lo previenen |
| MITM en descargas | HTTPS obligatorio + verificación de hash |

---

## Auditoría y logs

El launcher guarda logs en `<appdata>/logs/`. Para un reporte de soporte con info del sistema:

```
Ajustes → Diagnóstico → Crear reporte
```

Los logs no contienen tokens ni credenciales — solo rutas, versiones y mensajes de estado.

---

*Para gestionar el modpack y firmar manifests, consulta [`admin-guide.md`](admin-guide.md).*
