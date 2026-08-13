# stop_win.ps1 — Detiene el runtime nativo Windows (ADR-0012)
Get-Process server_realms -ErrorAction SilentlyContinue | Stop-Process -Force
Stop-Service postgresql-metin2 -ErrorAction SilentlyContinue
Write-Host "Runtime Windows detenido (auth + channel + PG)."
