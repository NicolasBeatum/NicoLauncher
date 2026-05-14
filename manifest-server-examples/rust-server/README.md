# Manifest Server — Ejemplo en Rust

Servidor HTTP minimal (~150 líneas) que sirve el `manifest.json` del launcher con CORS correctos y caché en memoria.

## Uso rápido

```bash
cargo run
# → Escuchando en http://0.0.0.0:3000/manifest.json
```

## Variables de entorno

| Variable | Default | Descripción |
|----------|---------|-------------|
| `PORT` | `3000` | Puerto HTTP |
| `MANIFEST_PATH` | `manifest.json` | Ruta al archivo manifest |
| `CACHE_TTL_SECS` | `30` | Segundos antes de releer el archivo del disco |
| `RUST_LOG` | `manifest_server=info` | Nivel de logs |

## Editar el manifest

Modifica `manifest.json` — el servidor lo recarga automáticamente cada `CACHE_TTL_SECS` segundos sin reiniciar.

## Despliegue

### Docker

```dockerfile
FROM rust:1.77 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/manifest-server /usr/local/bin/
COPY manifest.json /data/manifest.json
ENV MANIFEST_PATH=/data/manifest.json
EXPOSE 3000
CMD ["manifest-server"]
```

### systemd (Linux VPS)

```ini
[Unit]
Description=MC Launcher Manifest Server
After=network.target

[Service]
ExecStart=/usr/local/bin/manifest-server
Environment=PORT=3000
Environment=MANIFEST_PATH=/opt/launcher/manifest.json
Restart=always

[Install]
WantedBy=multi-user.target
```

### Detrás de nginx / Caddy

En `launcher.config.toml`:
```toml
manifest_url = "https://api.miservidor.com/manifest.json"
```

El servidor ya incluye los headers CORS necesarios para que el launcher pueda acceder desde cualquier origen.
