# deploy_win.ps1 - Build release + despliegue + arranque del runtime (ADR-0012).
#
# Automatiza el ciclo que antes se hacia a mano en 4 pasos:
#   1. cargo build --release -p server_realms
#   2. Parar auth + channel (si corren)
#   3. Copiar el binario a source\deploy\win\server_realms.exe
#   4. start_win.ps1 (PG + auth + channel, detached) + verificacion de puertos
#
# Uso (desde la raiz C:\projects\Metin2):
#   powershell -ExecutionPolicy Bypass -File scripts\deploy_win.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\deploy_win.ps1 -SkipBuild
#   powershell -ExecutionPolicy Bypass -File scripts\deploy_win.ps1 -NoStart
#
# Reglas AGENTS.md 15/16: cada paso es una operacion separada DENTRO del script
# (nada de concatenar con ';' para saltarse un fallo); logs con timestamp.

param(
    [switch]$SkipBuild,
    [switch]$NoStart,
    [string]$BenchCapture = ""
)

$ErrorActionPreference = "Stop"
$root    = "C:\projects\Metin2"
$reforge = Join-Path $root "source\reforge"
$win     = Join-Path $root "source\deploy\win"
$exe     = Join-Path $win "server_realms.exe"
$srcExe  = Join-Path $reforge "target\release\server_realms.exe"
$ts      = Get-Date -Format "HHmmss"

function Step($msg) { Write-Host ""; Write-Host "==> $msg" -ForegroundColor Cyan }
function Ok($msg)   { Write-Host "    OK: $msg" -ForegroundColor Green }
function Fail($msg) { Write-Host "    FALLO: $msg" -ForegroundColor Red; exit 1 }

# --- 1. Build ---------------------------------------------------------------
if (-not $SkipBuild) {
    Step "1/4 cargo build --release -p server_realms"
    Push-Location $reforge
    try {
        cargo build --release -p server_realms
        if ($LASTEXITCODE -ne 0) { Fail "build: exit $LASTEXITCODE" }
    } finally { Pop-Location }
    if (-not (Test-Path $srcExe)) { Fail "no existe $srcExe tras el build" }
    Ok "build release listo"
} else {
    Step "1/4 build omitido (-SkipBuild)"
    if (-not (Test-Path $srcExe)) { Fail "no existe $srcExe - quita -SkipBuild" }
    Ok "usando binario existente"
}

# --- 2. Parar procesos ------------------------------------------------------
Step "2/4 parar auth + channel (si corren)"
Get-Process server_realms -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Host "    deteniendo PID $($_.Id)"
    Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
}
Start-Sleep 1
$running = Get-Process server_realms -ErrorAction SilentlyContinue
if ($running) { Fail "no se pudieron detener los procesos" } else { Ok "procesos detenidos" }

# --- 3. Copiar binario ------------------------------------------------------
Step "3/4 copiar binario a deploy\win"
$backup = "$exe.$ts.bak"
if (Test-Path $exe) { Copy-Item $exe $backup -Force; Ok "backup previo: $(Split-Path $backup -Leaf)" }
Copy-Item $srcExe $exe -Force
Ok "desplegado $ts"

# --- 4. Arrancar + verificar ------------------------------------------------
if (-not $NoStart) {
    Step "4/4 start_win.ps1 (PG + auth + channel)"
    $startArgs = @("-NoProfile","-ExecutionPolicy","Bypass","-File",(Join-Path $root "scripts\start_win.ps1"))
    if ($BenchCapture -ne "") { $startArgs += @("-BenchCapture", $BenchCapture) }
    & powershell $startArgs
    if ($LASTEXITCODE -ne 0) { Fail "start_win.ps1: exit $LASTEXITCODE" }
    Start-Sleep 5
    Step "verificacion de puertos (5432 PG, 30001 auth, 30003 channel)"
    foreach ($port in 5432, 30001, 30003) {
        $listen = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
        if ($listen) { Ok "puerto $port LISTENING (PID $($listen.OwningProcess))" }
        else         { Fail "puerto $port NO escucha" }
    }
} else {
    Step "4/4 arranque omitido (-NoStart)"
    Ok "binario desplegado; inicia manualmente con scripts\start_win.ps1"
}

Write-Host ""
Write-Host "Deploy completo. Logs: $win\logs\*.$ts.*" -ForegroundColor Green
