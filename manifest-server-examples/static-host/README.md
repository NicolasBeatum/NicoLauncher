# Ejemplo: hosting estático

La forma más sencilla de servir el manifest: un archivo JSON estático en cualquier CDN o servicio de hosting.

---

## Opciones recomendadas

| Servicio | Coste | Configuración |
|----------|-------|--------------|
| **GitHub Pages** | Gratis | 2 min |
| **Cloudflare Pages** | Gratis | 5 min |
| **Netlify** | Gratis | 5 min |
| **AWS S3 + CloudFront** | ~$0.01/GB | 15 min |
| **Nginx propio** | Coste del VPS | Variable |

---

## GitHub Pages (más rápido)

### 1. Crea un repositorio para el manifest

```bash
mkdir mi-servidor-manifest
cd mi-servidor-manifest
git init
git remote add origin https://github.com/TU_ORG/mi-servidor-manifest.git
```

### 2. Estructura del repo

```
mi-servidor-manifest/
├── manifest.json          ← el manifest firmado
└── .github/
    └── workflows/
        └── deploy.yml     ← opcional: valida antes de publicar
```

### 3. Activa GitHub Pages

En tu repo: **Settings → Pages → Source → Deploy from branch → main → / (root)**

El manifest quedará en:
```
https://TU_ORG.github.io/mi-servidor-manifest/manifest.json
```

### 4. Configura el launcher

```toml
[server]
manifest_provider = "http"
manifest_url      = "https://TU_ORG.github.io/mi-servidor-manifest/manifest.json"
```

### 5. Actualizar el modpack

```bash
# Genera y firma el nuevo manifest
mc-launcher manifest update
mc-launcher sign sign manifest.json

# Sube al repo
cp manifest-signed.json /path/to/mi-servidor-manifest/manifest.json
cd /path/to/mi-servidor-manifest
git add manifest.json
git commit -m "chore: actualizar modpack $(date +%Y.%m.%d)"
git push
# GitHub Pages publica en ~1 min
```

---

## Cloudflare Pages

### 1. Crea el repo con el manifest (igual que arriba)

### 2. Conecta en Cloudflare

1. Ve a [pages.cloudflare.com](https://pages.cloudflare.com)
2. **Create a project → Connect to Git**
3. Selecciona tu repo
4. Build command: *(vacío)*
5. Output directory: `/`

### 3. URL resultante

```
https://mi-servidor-manifest.pages.dev/manifest.json
```

O configura un dominio personalizado: `manifest.miservidor.com/manifest.json`

---

## Nginx propio

Si ya tienes un VPS con Nginx:

```nginx
server {
    listen 443 ssl;
    server_name manifest.miservidor.com;

    # SSL configurado con certbot
    ssl_certificate     /etc/letsencrypt/live/manifest.miservidor.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/manifest.miservidor.com/privkey.pem;

    root /var/www/manifest;

    location /manifest.json {
        add_header Content-Type application/json;
        add_header Cache-Control "no-cache, must-revalidate";
        add_header Access-Control-Allow-Origin *;
    }
}
```

Sube el manifest:

```bash
scp manifest-signed.json usuario@vps:/var/www/manifest/manifest.json
```

---

## Caché y headers

Para que el launcher siempre reciba la versión más reciente, configura:

```
Cache-Control: no-cache, must-revalidate
```

GitHub Pages y Cloudflare Pages aplican caché agresiva por defecto — el launcher tiene un TTL de 5 minutos (`update_check_interval_secs = 300`) así que los jugadores recibirán la actualización en máximo 5 minutos.

---

## Verificar que funciona

```bash
curl -I https://TU_ORG.github.io/mi-servidor-manifest/manifest.json
# → HTTP/2 200
# → content-type: application/json

mc-launcher sign verify https://TU_URL/manifest.json -k public.key
```

---

*Para un servidor con lógica propia (autenticación, A/B testing, etc.), usa [`../rust-server/`](../rust-server/README.md).*
