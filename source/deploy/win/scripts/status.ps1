# status.ps1 — snapshot mínimo del estado actual (PowerShell, sin dependencias).
# Uso: powershell -ExecutionPolicy Bypass -File scripts\status.ps1
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

function Write-Snapshot {
    "== HEAD =="
    git rev-parse HEAD
    "== STATUS =="
    git status --short --branch
    "== DIFF --STAT =="
    git diff --stat
    "== BINARIO =="
    $exe = 'source/deploy/win/server_realms.exe'
    if (Test-Path $exe) { (Get-FileHash $exe -Algorithm SHA256).Hash } else { "no existe: $exe" }
    "== PUERTOS =="
    foreach ($p in 5432, 30001, 30003) {
        $ok = Test-NetConnection -ComputerName 127.0.0.1 -Port $p -InformationLevel Quiet -WarningAction SilentlyContinue
        "$p abierto: $ok"
    }
    "== CHANGELOG (ultima linea) =="
    Get-Content CHANGELOG.md -Tail 1 -Encoding UTF8
}

$snapshot = Write-Snapshot
$snapshot

$ev = '.omo/evidence'
if (Test-Path $ev) {
    $file = Join-Path $ev "status-$((Get-Date).ToString('yyyyMMdd-HHmmss')).txt"
    $snapshot | Set-Content -Encoding utf8 $file
    "guardado: $file"
}