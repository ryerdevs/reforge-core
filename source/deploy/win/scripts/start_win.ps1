# start_win.ps1 — Runtime nativo Windows (ADR-0012) — SOLO LANZAMIENTO.
# PG 18 (servicio postgresql-metin2) + Rust auth :30001 + Rust channel :30003.
#
# REGLA (AGENTS.md regla 16 / guardrails/operations.md): este script NO
# verifica puertos ni lee la salida de los procesos — imprime OK y termina.
# La verificacion es un comando APARTE (netstat) en el siguiente turno.
# Uso: powershell -ExecutionPolicy Bypass -File scripts\start_win.ps1
#      [-BenchCapture <rel>] — pasa --bench-capture <win\logs\<rel>> al channel
#      (benchmark F5: tick_ms.csv + captura wire; relativo a win\logs).
param([string]$BenchCapture = "", [switch]$HsDebug)
$ErrorActionPreference = "Stop"
$win = "C:\projects\Metin2\source\deploy\win"
$exe = Join-Path $win "server_realms.exe"
$logs = Join-Path $win "logs"
New-Item -ItemType Directory -Force -Path $logs | Out-Null

# 1. PostgreSQL (servicio registrado como NETWORK SERVICE — no arranca elevado)
$svc = Get-Service postgresql-metin2 -ErrorAction SilentlyContinue
if (-not $svc) { Write-Host "ERROR: servicio postgresql-metin2 no existe (ADR-0012)" -ForegroundColor Red; exit 1 }
if ($svc.Status -ne "Running") { Start-Service postgresql-metin2 }

# 2. Rust auth + channel — detached, salida a archivos con timestamp, SIN espera
$ts = Get-Date -Format "HHmmss"
if ($HsDebug) { $env:MT2_HS_DEBUG = "1" }  # instrumentación del handshake (handshake.rs)
Get-Process server_realms -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep 1
Start-Process -FilePath $exe -ArgumentList "--role","auth","--config",(Join-Path $win "auth.toml") -WindowStyle Hidden -RedirectStandardOutput (Join-Path $logs "auth.$ts.out.log") -RedirectStandardError (Join-Path $logs "auth.$ts.err.log")
$chArgs = @("--role","channel","--config",(Join-Path $win "channel.toml"))
if ($BenchCapture -ne "") { $chArgs += @("--bench-capture",(Join-Path $logs $BenchCapture)) }
Start-Process -FilePath $exe -ArgumentList $chArgs -WindowStyle Hidden -RedirectStandardOutput (Join-Path $logs "channel.$ts.out.log") -RedirectStandardError (Join-Path $logs "channel.$ts.err.log")

Write-Host "OK: auth + channel lanzados. Logs: auth.$ts.* / channel.$ts.* en $logs"
Write-Host "Verificar aparte: netstat -ano | findstr :30001 :30003"
