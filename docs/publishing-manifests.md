# Publicar manifests — MC Launcher Template

Guía rápida para actualizar el modpack y distribuirlo a los jugadores.

> Para el flujo completo de configuración inicial, consulta [`admin-guide.md`](admin-guide.md).
> Para el schema detallado del manifest, consulta [`manifest-schema.md`](manifest-schema.md).

---

## Flujo normal de actualización

```
1. Añades/actualizas JARs en tu carpeta de mods
        │
2. mc-launcher manifest update
        │ (detecta cambios, consulta Modrinth, actualiza manifest.json)
        │
3. mc-launcher sign sign manifest.json    ← si usas firma
        │
4. Subes manifest-signed.json al hosting
        │
5. Los jugadores reciben la actualización
   automáticamente en el próximo arranque del launcher
```

---

## Paso 1 — Actualizar el modpack

Añade, elimina o actualiza los JARs en tu carpeta de mods (la que configuraste en `lockfile.toml → [paths] → mods`).

Si no tienes `lockfile.toml` todavía, créalo con:

```bash
mc-launcher manifest init
```

---

## Paso 2 — Regenerar el manifest

```bash
mc-launcher manifest update
```

El CLI:
- Escanea las carpetas configuradas en `lockfile.toml`
- Compara con el `manifest.json` anterior (solo consulta Modrinth para archivos nuevos/cambiados)
- Muestra un diff de qué cambia: ➕ nuevo · 🔄 actualizado · ❌ eliminado
- Pide confirmación antes de escribir

Si todo está bien, confirma y se genera el nuevo `manifest.json`.

---

## Paso 3 — Firmar (recomendado en producción)

```bash
mc-launcher sign sign manifest.json -k signing.key
# → manifest-signed.json
```

Verifica que la firma es correcta:

```bash
mc-launcher sign verify manifest-signed.json -k public.key
```

Valida el contenido completo (schema, hashes, dependencias):

```bash
mc-launcher sign validate manifest-signed.json --check-urls
```

---

## Paso 4 — Publicar en el hosting

Dependiendo de tu hosting (ver opciones en [`../manifest-server-examples/`](../manifest-server-examples/)):

### GitHub Pages / Cloudflare Pages / S3

```bash
cp manifest-signed.json public/manifest.json   # o el nombre que uses
git add public/manifest.json
git commit -m "chore: actualizar modpack v$(date +%Y.%m.%d)"
git push
```

### Servidor Rust propio

```bash
cp manifest-signed.json manifest-server-examples/rust-server/manifest.json
# el servidor sirve el archivo en tiempo real, no necesitas reiniciarlo
```

### GitHub Actions automático

Si usas el ejemplo `git-based`, el workflow detecta cambios en `manifest.json` y lo despliega automáticamente.

---

## Paso 5 — Verificar que los jugadores reciben la actualización

El launcher descarga el manifest cada vez que arranca y también cada 5 minutos en background. Los jugadores verán el toast **"📋 Manifest actualizado a vX"** y en el próximo PLAY se sincronizarán los cambios.

Para forzar el re-sync en tu propio launcher durante pruebas:
**Ajustes → Servidor → ⟳ Forzar sync**

---

## Versionado del manifest

El campo `version` del manifest sigue el formato `YYYY.MM.DD-N`:

```json
{ "version": "2026.01.15-1" }
```

El CLI lo incrementa automáticamente. Si publicas dos actualizaciones el mismo día, el número final incrementa: `2026.01.15-1`, `2026.01.15-2`, etc.

---

## Eliminar mods obsoletos

Añade los filenames en `files_to_delete` para que el launcher los borre del equipo del jugador:

```json
{
  "files_to_delete": [
    "mods/mod-viejo-1.2.3.jar",
    "config/config-vieja.json"
  ]
}
```

El CLI añade estas entradas automáticamente cuando detecta que un archivo fue eliminado de la carpeta.

---

## Rollback de emergencia

Si una actualización causa problemas:

1. Restaura el `manifest.json` anterior (git history)
2. Vuelve a firmar y publicar
3. Los jugadores recibirán el rollback en el próximo arranque

Si necesitas rollback inmediato sin esperar al ciclo de poll, configura un webhook en `manifest_url` que devuelva el manifest antiguo directamente.

---

## Checklist de release

```
□ Probado localmente con mc-launcher launch
□ mc-launcher sign validate --check-urls sin errores
□ manifest.json firmado y verificado
□ Publicado en el hosting
□ URL de manifest responde con 200 y Content-Type: application/json
□ Probado en un cliente limpio (sin mods previos)
```

---

*Para hosting estático (GitHub Pages, S3), ver [`../manifest-server-examples/static-host/README.md`](../manifest-server-examples/static-host/README.md).*
*Para hosting con Git + CI, ver [`../manifest-server-examples/git-based/README.md`](../manifest-server-examples/git-based/README.md).*
