# Ejemplo: manifest basado en Git

El admin actualiza el modpack haciendo push a un repo Git. Un GitHub Actions workflow valida, firma y despliega el manifest automáticamente.

---

## Cómo funciona

```
Admin (local)                    GitHub                    Jugadores
     │                              │                          │
     ├─ edita mods/                 │                          │
     ├─ mc-launcher manifest update │                          │
     ├─ git push ─────────────────► │                          │
     │                              ├─ CI: validate            │
     │                              ├─ CI: sign                │
     │                              ├─ CI: deploy to Pages ────►│
     │                              │                          │ (≤5 min)
```

---

## Estructura del repositorio

```
mi-servidor-modpack/
├── manifest.json                  ← editado por el admin (sin firmar)
├── mods/                          ← opcional: JARs para generar el manifest
├── configs/                       ← opcional: archivos de config del servidor
├── lockfile.toml                  ← generado por mc-launcher manifest init
├── public.key                     ← clave pública (no es secreta)
└── .github/
    └── workflows/
        └── update-manifest.yml    ← el workflow de esta carpeta
```

---

## Setup

### 1. Crea el repo

```bash
git clone https://github.com/TU_ORG/mi-servidor-modpack.git
cd mi-servidor-modpack
```

### 2. Inicializa el lockfile

```bash
mc-launcher manifest init
mc-launcher manifest update
```

### 3. Genera las claves de firma

```bash
mc-launcher sign gen-keys
# → signing.key  (PRIVADA)
# → public.key   (pública)
```

### 4. Configura el GitHub Secret

En tu repo: **Settings → Secrets and variables → Actions → New secret**

| Secret | Valor |
|--------|-------|
| `MANIFEST_SIGNING_KEY` | Contenido completo de `signing.key` |

### 5. Copia el workflow

```bash
cp .github/workflows/update-manifest.yml tu-repo/.github/workflows/
```

### 6. Activa GitHub Pages

**Settings → Pages → Source → GitHub Actions**

El workflow publicará el manifest firmado en:
```
https://TU_ORG.github.io/mi-servidor-modpack/manifest.json
```

### 7. Configura el launcher

```toml
[server]
manifest_provider  = "http"
manifest_url       = "https://TU_ORG.github.io/mi-servidor-modpack/manifest.json"
manifest_public_key = "contenido de public.key"
```

---

## Actualizar el modpack

```bash
# 1. Actualiza tus mods localmente
mc-launcher manifest update

# 2. Sube el manifest (sin firmar — el CI se encarga)
git add manifest.json lockfile.toml
git commit -m "feat: añadir Sodium 0.6.0"
git push
# El workflow valida, firma y publica automáticamente
```

---

## Variante: modo file (desarrollo local)

Para probar el manifest sin servidor:

```toml
[server]
manifest_provider = "file"
manifest_url      = "C:/ruta/a/manifest.json"
```

---

*Para servir el manifest desde un servidor con lógica propia, ver [`../rust-server/`](../rust-server/README.md).*
