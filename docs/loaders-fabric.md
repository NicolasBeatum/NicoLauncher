# Loader: Fabric

Guía de referencia para usar Fabric con el launcher.

---

## Qué es Fabric

[Fabric](https://fabricmc.net/) es un mod loader modular y ligero para Minecraft. Se compone de dos partes:

- **Fabric Loader** — el loader en sí (carga los mods)
- **Fabric API** — librería de APIs comunes que la mayoría de mods requieren

Fabric es el loader con el ecosistema de mods más activo para versiones recientes de Minecraft.

---

## Configurar en el launcher

En `launcher.config.toml` no hace falta especificar el loader (es una decisión de cada manifest). Pero para pruebas con el CLI:

```bash
mc-launcher launch 1.21.1 --loader fabric --loader-version 0.16.5
```

En el manifest del servidor:

```json
{
  "mc_version": "1.21.1",
  "loader_type": "fabric",
  "loader_version": "0.16.5"
}
```

---

## Versiones disponibles

El launcher consulta automáticamente la API de Fabric Loader para obtener versiones:

```
https://meta.fabricmc.net/v2/versions/loader
```

Para obtener la última versión estable:

```
https://meta.fabricmc.net/v2/versions/loader/<mc_version>
```

Si `loader_version` está vacío en el manifest, el launcher usa la última versión estable disponible para esa versión de Minecraft.

---

## Fabric API como mod requerido

Casi todos los mods de Fabric dependen de Fabric API. Añádela siempre como mod requerido:

```json
{
  "id": "fabric-api",
  "name": "Fabric API",
  "version": "0.100.0+1.21.1",
  "url": "https://cdn.modrinth.com/data/P7dR8mSH/versions/<version_id>/fabric-api-0.100.0+1.21.1.jar",
  "sha512": "<hash>",
  "filename": "fabric-api-0.100.0+1.21.1.jar"
}
```

Para obtener la URL y hash actuales, usa el CLI del admin:

```bash
mc-launcher manifest update
```

El asistente consulta Modrinth automáticamente y rellena `sha512` y `url`.

---

## Compatibilidad de versiones

| Minecraft | Fabric Loader mínimo | Notas |
|-----------|---------------------|-------|
| 1.21.x | 0.15.0+ | Recomendado: latest |
| 1.20.x | 0.14.0+ | |
| 1.19.x | 0.14.0+ | |
| 1.18.x | 0.13.0+ | |

> Para versiones de MC anteriores a 1.14, Fabric no existe. Usa Forge.

---

## Mods incompatibles con Fabric

Algunos mods populares solo funcionan en Forge/NeoForge:

- OptiFine (usa Fabric: **Sodium** + **Iris** como alternativa)
- La mayoría de mods de aventura/magia de edad dorada (1.7.10)

Si necesitas OptiFine-like features, configúralos como opcionales:

```json
"optional_mods": [
  { "id": "sodium", "conflicts_with": ["optifine"], ... },
  { "id": "iris", "depends_on": ["sodium"], ... }
]
```

---

## Troubleshooting

**Error: "Could not find Fabric Loader version X"**
- Verifica que la versión especificada en el manifest existe en https://meta.fabricmc.net/v2/versions/loader
- Deja `loader_version` vacío para usar la última versión estable

**Error: "Mixin apply failed"**
- Conflicto entre mods. Revisa incompatibilidades con `mc-launcher sign validate manifest.json`

**Mods no cargan / crash al iniciar**
- Falta Fabric API. Añádela como mod requerido.
