#!/usr/bin/env bash
# ============================================================================
# init-template.sh — Inicializa el launcher template para tu servidor
# Uso: bash scripts/init-template.sh
# ============================================================================
set -euo pipefail

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
GRAY='\033[0;37m'
RESET='\033[0m'

echo ""
echo -e "${CYAN}============================================${RESET}"
echo -e "${CYAN}  MC Launcher Template — Inicializacion    ${RESET}"
echo -e "${CYAN}============================================${RESET}"
echo ""

# ── 1. Verificar requisitos ─────────────────────────────────────────────────
echo -e "${YELLOW}[1/4] Verificando requisitos...${RESET}"

missing=()
command -v cargo &>/dev/null || missing+=("Rust (cargo)")
command -v node  &>/dev/null || missing+=("Node.js")
command -v npm   &>/dev/null || missing+=("npm")

if [ ${#missing[@]} -gt 0 ]; then
    echo -e "${RED}Faltan las siguientes herramientas:${RESET}"
    for t in "${missing[@]}"; do echo -e "${RED}  - $t${RESET}"; done
    echo -e "${RED}Instalalas y vuelve a ejecutar el script.${RESET}"
    exit 1
fi

echo -e "${GREEN}  OK: Rust, Node.js, npm encontrados${RESET}"

# Linux: verificar dependencias del sistema para Tauri
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    for pkg in libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev; do
        if ! dpkg -s "$pkg" &>/dev/null 2>&1; then
            echo -e "${YELLOW}  AVISO: $pkg no encontrado. Instala con: sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf${RESET}"
        fi
    done
fi

# ── 2. Instalar dependencias npm ────────────────────────────────────────────
echo ""
echo -e "${YELLOW}[2/4] Instalando dependencias npm...${RESET}"

if [ ! -d "node_modules" ]; then
    npm ci --silent 2>/dev/null || npm install --silent
fi
echo -e "${GREEN}  OK: dependencias instaladas${RESET}"

# ── 3. Generar par de claves del updater ────────────────────────────────────
echo ""
echo -e "${YELLOW}[3/4] Generando par de claves del updater...${RESET}"

if [ -f "updater.key" ]; then
    echo -e "${YELLOW}  AVISO: updater.key ya existe, se usará la existente.${RESET}"
else
    npm run tauri -- signer generate -w updater.key --password "" 2>/dev/null
    if [ ! -f "updater.key" ]; then
        echo -e "${RED}  ERROR: No se pudo generar el par de claves.${RESET}"
        echo -e "${RED}  Ejecuta manualmente: npm run tauri -- signer generate -w updater.key${RESET}"
        exit 1
    fi
    echo -e "${GREEN}  OK: updater.key y updater.key.pub generados${RESET}"
fi

# Asegurarse de que updater.key está en .gitignore
GITIGNORE=".gitignore"
if [ -f "$GITIGNORE" ]; then
    if ! grep -qxF "updater.key" "$GITIGNORE"; then
        echo "" >> "$GITIGNORE"
        echo "updater.key" >> "$GITIGNORE"
        echo -e "${GREEN}  OK: updater.key añadido a .gitignore${RESET}"
    fi
else
    echo "updater.key" > "$GITIGNORE"
    echo -e "${GREEN}  OK: .gitignore creado con updater.key${RESET}"
fi

# Leer la clave pública
PUB_KEY=""
if [ -f "updater.key.pub" ]; then
    PUB_KEY=$(cat "updater.key.pub")
fi

# ── 4. Instrucciones siguientes ──────────────────────────────────────────────
echo ""
echo -e "${YELLOW}[4/4] Próximos pasos:${RESET}"
echo ""
echo -e "${RESET}  1. Edita launcher.config.toml:"
echo -e "${GRAY}       internal_id         = 'tu-servidor'        # PERMANENTE${RESET}"
echo -e "${GRAY}       display_name        = 'Tu Servidor MC'${RESET}"
echo -e "${GRAY}       microsoft_client_id = 'TU_CLIENT_ID'       # ver docs/${RESET}"
echo -e "${GRAY}       manifest_url        = 'https://...'        # URL de tu manifest${RESET}"
echo ""
echo -e "${RESET}  2. Reemplaza los assets en assets/:"
echo -e "${GRAY}       logo.png, background.jpg, icon.ico, icon.png${RESET}"
echo ""
echo -e "${RESET}  3. Ejecuta el launcher en modo dev:"
echo -e "${CYAN}       npm run tauri dev${RESET}"
echo ""
echo -e "${RESET}  4. Configura GitHub Secrets para CI:"
echo -e "${GRAY}       TAURI_SIGNING_PRIVATE_KEY = (contenido de updater.key)${RESET}"
echo -e "${GRAY}       TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (vacío si no hay password)${RESET}"
echo ""
echo -e "${RESET}  5. Para publicar la primera release:"
echo -e "${CYAN}       git tag v1.0.0 && git push origin v1.0.0${RESET}"
echo ""
echo -e "${CYAN}  Lee la guía completa en: docs/customization-guide.md${RESET}"
echo ""

if [ -n "$PUB_KEY" ]; then
    echo -e "${CYAN}============================================${RESET}"
    echo -e "${CYAN}  TU CLAVE PÚBLICA DEL UPDATER:${RESET}"
    echo -e "${CYAN}============================================${RESET}"
    echo -e "${RESET}$PUB_KEY${RESET}"
    echo -e "${GRAY}(Ya está en launcher.config.toml y tauri.conf.json)${RESET}"
    echo ""
fi

echo -e "${GREEN}¡Listo! Happy coding :)${RESET}"
echo ""
