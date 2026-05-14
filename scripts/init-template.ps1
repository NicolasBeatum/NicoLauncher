# ============================================================================
# init-template.ps1 — Inicializa el launcher template para tu servidor
# Uso: .\scripts\init-template.ps1
# ============================================================================

$ErrorActionPreference = 'Stop'

Write-Host ""
Write-Host "============================================" -ForegroundColor Cyan
Write-Host "  MC Launcher Template — Inicializacion    " -ForegroundColor Cyan
Write-Host "============================================" -ForegroundColor Cyan
Write-Host ""

# ── 1. Verificar requisitos ─────────────────────────────────────────────────
Write-Host "[1/4] Verificando requisitos..." -ForegroundColor Yellow

$missing = @()
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { $missing += "Rust (cargo)" }
if (-not (Get-Command node  -ErrorAction SilentlyContinue)) { $missing += "Node.js" }
if (-not (Get-Command npm   -ErrorAction SilentlyContinue)) { $missing += "npm" }
if ($missing.Count -gt 0) {
    Write-Host "Faltan las siguientes herramientas:" -ForegroundColor Red
    $missing | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
    Write-Host "Instalalas y vuelve a ejecutar el script." -ForegroundColor Red
    exit 1
}

Write-Host "  OK: Rust, Node.js, npm encontrados" -ForegroundColor Green

# ── 2. Instalar dependencias npm ────────────────────────────────────────────
Write-Host ""
Write-Host "[2/4] Instalando dependencias npm..." -ForegroundColor Yellow

if (-not (Test-Path "node_modules")) {
    npm ci --silent
    if ($LASTEXITCODE -ne 0) { npm install --silent }
}
Write-Host "  OK: dependencias instaladas" -ForegroundColor Green

# ── 3. Generar par de claves del updater ────────────────────────────────────
Write-Host ""
Write-Host "[3/4] Generando par de claves del updater..." -ForegroundColor Yellow

if (Test-Path "updater.key") {
    Write-Host "  AVISO: updater.key ya existe, se usara la existente." -ForegroundColor DarkYellow
} else {
    # tauri signer generate escribe updater.key y updater.key.pub
    npm run tauri -- signer generate -w updater.key --password "" 2>&1 | Out-Null
    if (-not (Test-Path "updater.key")) {
        Write-Host "  ERROR: No se pudo generar el par de claves." -ForegroundColor Red
        Write-Host "  Ejecuta manualmente: npm run tauri -- signer generate -w updater.key" -ForegroundColor Red
        exit 1
    }
    Write-Host "  OK: updater.key y updater.key.pub generados" -ForegroundColor Green
}

# Asegurarse de que updater.key está en .gitignore
$gitignorePath = ".gitignore"
if (Test-Path $gitignorePath) {
    $content = Get-Content $gitignorePath -Raw
    if ($content -notmatch "updater\.key$") {
        Add-Content $gitignorePath "`nupdater.key"
        Write-Host "  OK: updater.key anadido a .gitignore" -ForegroundColor Green
    }
} else {
    "updater.key" | Out-File -FilePath $gitignorePath -Encoding utf8
    Write-Host "  OK: .gitignore creado con updater.key" -ForegroundColor Green
}

# Leer la clave pública
$pubKey = ""
if (Test-Path "updater.key.pub") {
    $pubKey = Get-Content "updater.key.pub" -Raw
}

# ── 4. Instrucciones siguientes ──────────────────────────────────────────────
Write-Host ""
Write-Host "[4/4] Proximos pasos:" -ForegroundColor Yellow
Write-Host ""
Write-Host "  1. Edita launcher.config.toml:" -ForegroundColor White
Write-Host "       internal_id      = 'tu-servidor'           # PERMANENTE" -ForegroundColor Gray
Write-Host "       display_name     = 'Tu Servidor MC'" -ForegroundColor Gray
Write-Host "       microsoft_client_id = 'TU_CLIENT_ID'       # ver docs/" -ForegroundColor Gray
Write-Host "       manifest_url     = 'https://...'           # URL de tu manifest" -ForegroundColor Gray
Write-Host ""
Write-Host "  2. Reemplaza los assets en assets/:" -ForegroundColor White
Write-Host "       logo.png, background.jpg, icon.ico, icon.png" -ForegroundColor Gray
Write-Host ""
Write-Host "  3. Ejecuta el launcher en modo dev:" -ForegroundColor White
Write-Host "       npm run tauri dev" -ForegroundColor Cyan
Write-Host ""
Write-Host "  4. Configura GitHub Secrets para CI:" -ForegroundColor White
Write-Host "       TAURI_SIGNING_PRIVATE_KEY = (contenido de updater.key)" -ForegroundColor Gray
Write-Host "       TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (vacio si no hay password)" -ForegroundColor Gray
Write-Host ""
Write-Host "  5. Para publicar la primera release:" -ForegroundColor White
Write-Host "       git tag v1.0.0 && git push origin v1.0.0" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Lee la guia completa en: docs/customization-guide.md" -ForegroundColor DarkCyan
Write-Host ""

if ($pubKey) {
    Write-Host "============================================" -ForegroundColor Cyan
    Write-Host "  TU CLAVE PUBLICA DEL UPDATER:" -ForegroundColor Cyan
    Write-Host "============================================" -ForegroundColor Cyan
    Write-Host $pubKey -ForegroundColor White
    Write-Host "(Ya esta en launcher.config.toml y tauri.conf.json)" -ForegroundColor DarkGray
    Write-Host ""
}

Write-Host "Listo! Happy coding :)" -ForegroundColor Green
Write-Host ""
