# Loader: Quilt

Guía de referencia para usar Quilt con el launcher.

---

## Qué es Quilt

[Quilt](https://quiltmc.org/) es un fork de Fabric que ofrece:

- Compatible con mods de Fabric (la mayoría funcionan sin cambios)
- API propia más completa (**Quilted Fabric API** = QFAPI)
- Mejor gestión de dependencias y conflictos entre mods
- Soporte experimental de características de Minecraft más nuevas

---

## Configurar en el manifest

```json
{
  "mc_version": "1.21.1",
  "loader_type": "quilt",
  "loader_version": "0.27.0"
}
```

Prueba desde CLI:

```bash
mc-launcher launch 1.21.1 --loader quilt
```

---

## Quilted Fabric API (QFAPI)

Quilt incluye una versión de Fabric API llamada **Quilted Fabric API** que añade compatibilidad con mods de Fabric. En el manifest, úsala como mod requerido:

```json
{
  "id": "qfapi",
  "name": "Quilted Fabric API",
  "version": "9.0.0+0.100.0-1.21.1",
  "url": "https://cdn.modrinth.com/data/qvIfYCYJ/versions/<id>/quilted-fabric-api-9.0.0+0.100.0-1.21.1.jar",
  "sha512": "<hash>",
  "filename": "quilted-fabric-api-9.0.0+0.100.0-1.21.1.jar"
}
```

> No incluyas Fabric API y QFAPI a la vez — son incompatibles entre sí.

---

## Versiones disponibles

El launcher consulta:

```
https://meta.quiltmc.org/v3/versions/loader
https://meta.quiltmc.org/v3/versions/loader/<mc_version>
```

---

## Compatibilidad con mods de Fabric

La gran mayoría de mods escritos para Fabric funcionan en Quilt sin modificación. Excepciones:

- Mods que usen internals de Fabric Loader directamente (raro pero posible)
- Mods que detecten explícitamente el loader y rechacen Quilt (también raro)

Si un mod no carga en Quilt, prueba añadir `quilt-fabric-api-bridge` como dependencia.

---

## Cuándo usar Quilt vs Fabric

| Situación | Recomendación |
|-----------|--------------|
| Servidor estable, mods bien conocidos | Fabric — más testado |
| Quieres las últimas features de Quilt | Quilt |
| Todos tus mods ya son de Quilt | Quilt |
| Mods mixtos Fabric + Quilt | Quilt (mejor compatibilidad cruzada) |

---

## Troubleshooting

**Mod de Fabric no carga en Quilt**
- Añade `quilt-fabric-api-bridge` al manifest
- Comprueba si el mod usa `net.fabricmc.loader.api.*` directamente

**Error: version incompatible**
- Quilt tiene su propio versionado de loader, distinto al de Fabric. Verifica la versión en https://quiltmc.org/en/usage/latest-versions/
