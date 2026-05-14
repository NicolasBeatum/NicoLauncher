# Checklist de testing manual — MC Launcher Template

Ejecutar este checklist antes de cada release. Marca cada ítem tras verificarlo en un equipo limpio (sin instalación previa).

---

## 0. Preparación

```
□ Entorno limpio: sin %APPDATA%\<internal_id>\ (o eliminar la carpeta)
□ Cliente distinto al de desarrollo (o usuario de Windows diferente)
□ Conexión a internet disponible para los tests que lo requieren
□ Log level en DEBUG: RUST_LOG=debug npm run tauri dev
```

---

## 1. Primer arranque (sin datos previos)

```
□ Splash screen aparece con logo y texto "Inicializando…"
□ Si Java 21 no está instalado → el launcher lo descarga automáticamente
  □ Barra de progreso visible durante la descarga de Java
  □ Java queda en <appdata>/java/21/
□ Sin internet al primer arranque → mensaje de error claro (no crash)
□ Tras inicialización, redirige a Login screen
```

---

## 2. Autenticación Microsoft

```
□ Botón "Iniciar sesión con Microsoft" abre ventana de OAuth
□ Login correcto → redirige a Home con nombre de usuario visible
□ Cerrar la app y reabrir → sigue logueado (sin re-login)
□ Revocar token desde cuenta MS → al reabrir pide login de nuevo
□ Cuenta MS sin Java Edition → mensaje específico "no tienes Minecraft"
□ Botón "Cerrar sesión" en Settings → limpia sesión y vuelve a Login
```

---

## 3. Sync inicial y descarga de mods

```
□ Con manifest correcto: la pantalla muestra etapas (Preparando / Descargando / etc.)
□ Barra de progreso avanza y muestra "X/Y mods"
□ Cada mod aparece en los logs de consola (⬇ nombre.jar o ✓ nombre.jar si en caché)
□ Mods quedan en <appdata>/instances/<id>/minecraft/mods/
□ Archivos en CAS: <appdata>/cache/mod-files/<sha[0:2]>/<sha>
□ Después de sync exitoso, botón vuelve a JUGAR (no queda en "Sincronizando")
□ Sin internet durante sync → error claro, no crash silencioso
```

---

## 4. Lanzamiento del juego

```
□ Pulsar JUGAR → sync si necesario, luego lanza Minecraft
□ Logs de Minecraft aparecen en la consola del launcher
□ PID visible en los logs de debug
□ Minecraft se lanza con el loader correcto (Fabric / NeoForge / etc.)
□ Versión mostrada en Home coincide con manifest_version
□ Botón cambia a "🟢 Minecraft está corriendo" mientras el proceso existe
□ Cerrar Minecraft → botón vuelve a JUGAR automáticamente
□ Botón "Cerrar juego" detiene el proceso
```

---

## 5. Cache y re-sync

```
□ Eliminar mods/ manualmente → Forzar sync → mods vuelven (sin re-descargar si están en CAS)
□ Actualizar manifest a versión nueva → al pulsar JUGAR se descarga solo lo nuevo
□ Rollback de manifest → versiones previas se restauran desde CAS sin re-descarga
□ Mod eliminado del manifest → archivo borrado de mods/ en el próximo sync
□ removed_files en manifest → archivos eliminados de .minecraft/
```

---

## 6. Mods opcionales

```
□ Pantalla "Mods opcionales" lista los definidos en el manifest
□ Toggle de un mod → se activa/desactiva correctamente
□ Mod con dependencia → al activarlo, propone activar también la dep
□ Mod con conflicto → al activarlo, propone desactivar el conflicto
□ Mod habilitado → se descarga en el próximo sync / aparece en mods/
□ Mod deshabilitado que estaba activo → se elimina de mods/ en el próximo sync
□ Pestaña "Tus mods" → lista los .jar en mods-optional/
□ Toggle en "Tus mods" → hardlink/copia a mods/ o eliminación
□ Botón "Reconstruir" → re-linkea mods opcionales desde CAS sin re-descargar
```

---

## 7. Settings

```
□ Slider de RAM cambia el valor y se persiste entre reinicios
□ Advertencia si se pide más RAM de la disponible en el sistema
□ Override de path de Java → el launcher usa el path especificado
□ Botón "Detectar automáticamente" → limpia override y detecta de nuevo
□ JVM args editables (si allow_jvm_args_edit = true) → persisten y se usan
□ Botón "Abrir carpeta de logs" → abre la carpeta en el explorador
□ Botón "Forzar sync" → limpia estado local, siguiente JUGAR re-sincroniza todo
□ Reset completo → confirmación, borra todo, vuelve a primer arranque
```

---

## 8. Auto-updater

```
□ Nueva release publicada en GitHub → el launcher detecta la actualización
□ Notificación de actualización visible (toast o banner en Home)
□ El usuario acepta → se descarga e instala
□ Firma del instalador verificada (no debe instalarse si la firma no coincide)
```

---

## 9. Modo offline / sin internet

```
□ Iniciar con manifest cacheado y sin internet → usa caché, no bloquea
□ Iniciar sin ningún caché y sin internet → mensaje claro de error
□ Minecraft ya descargado + sin internet → puede lanzar con token cacheado
```

---

## 10. Borde / edge cases

```
□ Manifest con campo announcement activo → banner visible en Home
□ Manifest con removed_files → archivos eliminados del .minecraft
□ Manifest con config_overrides apply="always" → config re-aplicada cada sync
□ Manifest con config_overrides apply="if_missing" → solo si no existe
□ Manifest firmado con clave correcta → acepta y lanza
□ Manifest firmado con clave incorrecta → rechaza con error claro
□ Manifest sin firma con public_key configurada → rechaza con error claro
□ Manifest con path traversal (../../etc/passwd) → rechaza, no crash
□ Archivo JAR con hash incorrecto en manifest → descarga fallida, error claro
□ Servidor de manifest caído → 3 reintentos, luego error claro
```

---

## 11. Multiplataforma (si aplica)

```
□ Windows 10/11 — rutas con espacios y letras de unidad distintas al cache
□ macOS — Keychain guarda los tokens correctamente
□ Linux — libsecret / KWallet detectado; si no, fallback de credenciales
□ Hardlinks funcionan en Windows (mismo volumen) — mods/ apunta a cache/
□ Copia como fallback si cache y .minecraft están en discos distintos
```

---

## Notas

- Si algo falla, adjuntar el reporte de **Ajustes → Diagnóstico → Crear reporte**.
- Los logs en `<appdata>/logs/` nunca deben contener tokens o access_token en texto plano.
- Después de un test exitoso completo, etiquetar como `tested-YYYY.MM.DD` en el commit.
