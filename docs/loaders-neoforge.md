# Loader: NeoForge

Guía de referencia para usar NeoForge con el launcher.

---

## Qué es NeoForge

[NeoForge](https://neoforged.net/) es el sucesor oficial de Minecraft Forge, mantenido por la mayor parte del equipo original de Forge tras un fork en 2023. Es el loader recomendado para el ecosistema Forge en versiones **1.20.2+**.

Características principales:
- Compatible con la mayoría de mods de Forge modernos (con adaptación mínima o ninguna)
- Ciclo de releases más rápido que Forge clásico
- Mejoras en la API de modding (mixins, eventos, registry)

---

## Configurar en el manifest

```json
{
  "mc_version": "1.21.1",
  "loader_type": "neoforge",
  "loader_version": "21.1.172"
}
```

Prueba desde CLI:

```bash
mc-launcher launch 1.21.1 --loader neoforge --loader-version 21.1.172
```

---

## Esquema de versiones

NeoForge usa un esquema de versiones basado en la versión de Minecraft:

```
<mc_major>.<mc_minor>.<mc_patch>.<build>
```

Por ejemplo, para Minecraft 1.21.1:
- Versión NeoForge: `21.1.xxx`

Para obtener las versiones disponibles:
```
https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml
```

---

## Instalación

El launcher descarga el installer JAR de NeoForge desde Maven y lo ejecuta en modo `--install-client`:

```
https://maven.neoforged.net/releases/net/neoforged/neoforge/<version>/neoforge-<version>-installer.jar
```

El proceso de instalación extrae las librerías necesarias y configura el perfil de cliente. Tarda más que Fabric/Quilt (10-30 segundos la primera vez).

---

## Diferencias con Forge clásico

| Característica | NeoForge | Forge |
|----------------|----------|-------|
| Versiones MC | 1.20.2+ | Todas (1.1+) |
| API moderna | Sí | Parcial |
| Velocidad de releases | Alta | Moderada |
| Compatibilidad cruzada | Alta (con Forge 1.20.x+) | Solo Forge |
| Recomendado para | Versiones recientes | Versiones antiguas |

---

## Mods populares en NeoForge

- **JEI** (Just Enough Items)
- **Waystones**
- **Alex's Mobs**
- **Immersive Engineering**
- **Applied Energistics 2**
- **Create**

---

## Cuándo usar NeoForge vs Fabric

| Caso | Recomendación |
|------|--------------|
| Mods de aventura/magia/tech | NeoForge — más mods del estilo |
| Performance pura | Fabric — mejores optimizadores (Sodium, etc.) |
| Versiones 1.20.2+ con mods Forge | NeoForge |
| Versiones 1.20.1 y anteriores | Forge clásico |

---

## Troubleshooting

**Instalación lenta o falla**
- NeoForge descarga muchas librerías de Maven. Asegúrate de que `maven.neoforged.net` sea accesible.
- El directorio de caché `<appdata>/libraries/neoforge/` debe tener espacio suficiente (~200 MB).

**Error: "Mod X requires NeoForge X.X+"**
- Actualiza `loader_version` en el manifest a la versión mínima requerida por los mods.

**Crash: "Caused by: java.lang.ClassNotFoundException"**
- Falta una dependencia. Revisa los `mods.toml` de los mods afectados y añade las dependencias al manifest.
