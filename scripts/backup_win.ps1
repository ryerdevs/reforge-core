# backup_win.ps1 — Nightly backup of the native PostgreSQL runtime (ADR-0012)
#
# Dumps the `metin2` database with the NATIVE pg_dump (custom format -Fc) to
#   C:\projects\metin2-extra\backups\metin2_<yyyy-MM-dd>.dump
# and prunes old dumps, keeping the last 7 (retention).
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\backup_win.ps1         # real run
#   powershell -ExecutionPolicy Bypass -File scripts\backup_win.ps1 -WhatIf  # DRY RUN
#
# Dry run (-WhatIf): shows the actions that WOULD run (dump + prune) and
# executes nothing — pg_dump is never invoked. Use it to validate the script
# without touching the filesystem or the database.
#
# Prerequisites (ADR-0012 native stack):
#   - PostgreSQL 18.4 Windows service `postgresql-metin2` Running (start_win.ps1 starts it)
#   - Native binaries at C:\projects\metin2-extra\pg18\pgsql\bin
#   - Role mt2 / password mt2 on db `metin2` (credentials passed via the
#     environment, never on the command line)
[CmdletBinding(SupportsShouldProcess = $true)]
param()

$ErrorActionPreference = "Stop"

$pgDump = if ($env:PGDUMP_PATH) {
    $env:PGDUMP_PATH
} elseif (Get-Command pg_dump.exe -ErrorAction SilentlyContinue) {
    (Get-Command pg_dump.exe).Source
} else {
    "C:\projects\metin2-extra\pg18\pgsql\bin\pg_dump.exe"
}

$backupDir = if ($env:REFORGE_BACKUP_DIR) {
    $env:REFORGE_BACKUP_DIR
} else {
    "C:\projects\metin2-extra\backups"
}
$retention = 7

if (-not (Test-Path -LiteralPath $pgDump)) { throw "pg_dump not found: $pgDump" }
New-Item -ItemType Directory -Force -Path $backupDir | Out-Null

# Credentials via the environment so they never appear in the process command line
$env:PGUSER     = "mt2"
$env:PGPASSWORD = "mt2"

$stamp   = Get-Date -Format "yyyy-MM-dd"
$outFile = Join-Path $backupDir "metin2_$stamp.dump"

# 1. Dump (custom format, localhost, native binary)
if ($PSCmdlet.ShouldProcess($outFile, "pg_dump -Fc metin2")) {
    & $pgDump -h 127.0.0.1 -p 5432 -U mt2 -d metin2 -Fc -f $outFile
    if ($LASTEXITCODE -ne 0) { throw "pg_dump FAILED with exit code $LASTEXITCODE" }
    $mb = [math]::Round((Get-Item -LiteralPath $outFile).Length / 1MB, 2)
    Write-Host "Backup OK: $outFile ($mb MB)"
}

# 2. Retention: keep the last $retention dumps (newest first by name, oldest pruned)
$dumps = @(Get-ChildItem -LiteralPath $backupDir -Filter "metin2_*.dump" | Sort-Object Name -Descending)
if ($dumps.Count -gt $retention) {
    foreach ($old in ($dumps | Select-Object -Skip $retention)) {
        if ($PSCmdlet.ShouldProcess($old.FullName, "Remove-Item (retention > $retention)")) {
            Remove-Item -LiteralPath $old.FullName -Force
            Write-Host "Pruned: $($old.Name)"
        }
    }
} else {
    Write-Host "Retention OK: $($dumps.Count) dump(s) of max $retention."
}
