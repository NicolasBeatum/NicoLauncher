# Loader: Forge (Legacy)

Guía de referencia para usar Forge clásico con el launcher.

> **¿Minecraft 1.20.2 o más nuevo?** Usa [NeoForge](loaders-neoforge.md) en su lugar — es el sucesor oficial con mejor soporte y más actualizaciones.

---

## Qué es Forge

[Minecraft Forge](https://minecraftforge.net/) es el mod loader más veterano, con soporte desde Minecraft 1.1. Es la elección obligatoria para:

- Versiones de Minecraft **1.20.1 y anteriores**
- Mods clásicos que nunca fueron portados a NeoForge/Fabric
- Servidores con modpacks de la "era dorada" (1.7.10, 1.12.2, 1.16.5)

---

## Configurar en el manifest

```json
{
  "mc_version": "1.20.1",
  "loader_type": "forge",
  "loader_version": "47.3.0"
}
```

Prueba desde CLI:

```bash
mc-launcher launch 1.20.1 --loader forge --loader-version 47.3.0
```

---

## Esquema de versiones

Forge usa el esquema `<mc_version>-<forge_build>`:

| Minecraft | Forge version | Ejemplo |
|-----------|--------------|---------|
| 1.20.1 | 47.x.x | `47.3.0` |
| 1.19.4 | 45.x.x | `45.3.0` |
| 1.18.2 | 40.x.x | `40.2.21` |
| 1.16.5 | 36.x.x | `36.2.39` |
| 1.12.2 | 14.x.x | `14.23.5.2860` |
| 1.7.10 | 10.x.x | `10.13.4.1614` |

Para ver todas las versiones disponibles:
```
https://files.minecraftforge.net/net/minecraftforge/forge/
```

---

## Proceso de instalación

El launcher descarga el Forge installer JAR desde:
```
https://maven.minecraftforge.net/net/minecraftforge/forge/<version>/forge-<version>-installer.jar
```

El installer ejecuta varios procesadores (similar a NeoForge) para parchear el cliente. En versiones antiguas (1.12.2 y anteriores) el proceso es más rápido porque no hay procesadores de bytecode complejos.

La primera instalación puede tardar **30-120 segundos** dependiendo de la versión.

---

## Versiones recomendadas por era

| Época | MC Version | Forge rec. | Notas |
|-------|-----------|-----------|-------|
| Moderna | 1.20.1 | 47.3.0+ | Última versión Forge pura |
| Netherite | 1.16.5 | 36.2.39 | Muy estable, gran ecosistema |
| Aquatic | 1.12.2 | 14.23.5.2860 | La más popular históricamente |
| Clásica | 1.7.10 | 10.13.4.1614 | Para mods legacy |

---

## Diferencias con NeoForge

| Aspecto | Forge | NeoForge |
|---------|-------|----------|
| Versiones MC | Todas (1.1+) | 1.20.2+ |
| Actualizaciones | Más lentas post-fork | Rápidas |
| API | Forge API clásica | Extendida |
| Compatibilidad cruzada | No | Alta (con Forge 1.20.x+) |

---

## Mods populares solo en Forge (legacy)

- **Thaumcraft** — magia arcana (1.12.2)
- **Thermal Expansion** — tech modular (todas las eras)
- **Tinkers' Construct** — herramientas (todas las eras)
- **Applied Energistics 2** — almacenamiento (todas las eras, también NeoForge)
- **Botania** — magia floral (también Fabric en versiones modernas)

---

## Troubleshooting

**Error: "This version of Forge requires Java X"**
- Forge 1.16.5 y anteriores requieren Java 8
- Forge 1.17+ requiere Java 16+
- Forge 1.18+ requiere Java 17+
- Forge 1.20.1 requiere Java 17+

Ajusta `java_version` en el manifest según corresponda.

**Error durante los procesadores del installer**
- Verifica que `maven.minecraftforge.net` sea accesible
- Borra el directorio `<appdata>/libraries/net/minecraftforge/` y vuelve a intentar

**Mods no cargan / "Missing required mod"**
- Forge tiene un sistema de dependencias en `mods.toml`. Añade todas las dependencias como mods requeridos en el manifest.
- Usa `mc-launcher sign validate manifest.json` para verificar que todas las dependencias declaradas existen en el manifest.

**Crash con Java 21 en Forge 1.12.2**
- Forge 1.12.2 no es compatible con Java 21. Usa Java 8 u 11.
- Configura `java_version: 8` en el manifest para que el launcher use la versión correcta.
