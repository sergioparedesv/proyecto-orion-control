# ORION NetControl - script de compilacion para Windows
#
# Uso (desde PowerShell, en la carpeta Release):
#   powershell -ExecutionPolicy Bypass -File .\construir.ps1           # genera el instalador
#   powershell -ExecutionPolicy Bypass -File .\construir.ps1 -Test     # ademas corre las pruebas
#   powershell -ExecutionPolicy Bypass -File .\construir.ps1 -Dev      # abre la app en modo desarrollo
#
# Resultado: los instaladores (.msi y .exe) quedan en Release\instaladores\

param(
    [switch]$Test,
    [switch]$Dev
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Zip = Get-ChildItem -Path $Here -Filter 'orion-netcontrol-*-fuente.zip' | Select-Object -First 1
$Src = Join-Path $Here 'orion-netcontrol'

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Yellow }
function Fail($msg) { Write-Host "`nERROR: $msg" -ForegroundColor Red; exit 1 }

function Require($cmd, $hint) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) { Fail "No se encontro '$cmd'. $hint" }
}

function Run($exe, [string[]]$argv) {
    & $exe @argv
    if ($LASTEXITCODE -ne 0) { Fail "Fallo: $exe $($argv -join ' ') (codigo $LASTEXITCODE)" }
}

# 1. Requisitos
Step 'Comprobando requisitos'
Require 'cargo' 'Instala Rust desde https://rustup.rs (toolchain stable-x86_64-pc-windows-msvc).'
Require 'node'  'Instala Node.js LTS desde https://nodejs.org.'
Require 'npm'   'Se instala junto con Node.js.'
& cargo tauri --version *> $null
if ($LASTEXITCODE -ne 0) {
    Step 'Instalando Tauri CLI (solo la primera vez, tarda unos minutos)'
    Run 'cargo' @('install', 'tauri-cli', '--version', '^2', '--locked')
}
Write-Host ("cargo: " + (& cargo --version))
Write-Host ("node : " + (& node --version))

# 2. Descomprimir el codigo fuente
if (-not (Test-Path (Join-Path $Src 'Cargo.toml'))) {
    if (-not $Zip) { Fail 'No se encontro orion-netcontrol-*-fuente.zip junto a este script.' }
    Step "Descomprimiendo $($Zip.Name)"
    Expand-Archive -Path $Zip.FullName -DestinationPath $Here -Force
}

# 3. Frontend
Step 'Instalando dependencias del frontend (npm ci)'
Push-Location (Join-Path $Src 'ui')
try { Run 'npm' @('ci', '--no-audit', '--no-fund') } finally { Pop-Location }

Push-Location $Src
try {
    # 4. Pruebas (opcional)
    if ($Test) {
        Step 'Ejecutando pruebas del nucleo'
        Run 'cargo' @('test')
    }

    # 5. Modo desarrollo o instalador
    if ($Dev) {
        Step 'Abriendo la app en modo desarrollo (Ctrl+C para salir)'
        Run 'cargo' @('tauri', 'dev')
        exit 0
    }

    Step 'Compilando el instalador (la primera vez tarda 5-15 minutos)'
    Run 'cargo' @('tauri', 'build')

    Step 'Compilando la herramienta de consola (orion-cli)'
    Run 'cargo' @('build', '--release', '-p', 'orion-cli')
} finally { Pop-Location }

# 6. Copiar instaladores
$Bundle = Join-Path $Src 'target\release\bundle'
$Out = Join-Path $Here 'instaladores'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Files = @()
$Files += Get-ChildItem -Path (Join-Path $Bundle 'msi') -Filter '*.msi' -ErrorAction SilentlyContinue
$Files += Get-ChildItem -Path (Join-Path $Bundle 'nsis') -Filter '*.exe' -ErrorAction SilentlyContinue
if ($Files.Count -eq 0) { Fail "No se encontraron instaladores en $Bundle" }
$Files | Copy-Item -Destination $Out -Force

# Ejecutable portable (sin instalar), util para pruebas rapidas
$Exe = Join-Path $Src 'target\release\orion-netcontrol.exe'
if (Test-Path $Exe) { Copy-Item $Exe -Destination (Join-Path $Out 'orion-netcontrol-portable.exe') -Force }
$Cli = Join-Path $Src 'target\release\orion-cli.exe'
if (Test-Path $Cli) { Copy-Item $Cli -Destination $Out -Force }

Step 'Listo'
Get-ChildItem $Out | ForEach-Object { Write-Host ("  " + $_.Name) -ForegroundColor Green }
Write-Host "`nInstaladores en: $Out"
