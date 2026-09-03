# restore_drill.ps1 - Recovery drill for the nightly pg_dump backups (ADR-0012)
#
# Restores the newest nightly dump into a DISPOSABLE database (m2_drill_<stamp>)
# on the same native PostgreSQL instance, counts key tables, and drops the drill
# database. Proves the backup is restorable - a dump that has never been
# restored is not a backup.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\restore_drill.ps1                # newest dump
#   powershell -ExecutionPolicy Bypass -File scripts\restore_drill.ps1 -DumpFile <path>
#   powershell -ExecutionPolicy Bypass -File scripts\restore_drill.ps1 -KeepDb        # do not drop (inspect)
#
# The drill database is disposable: prefix m2_drill_, never named `metin2`,
# and it is dropped at the end unless -KeepDb is passed.
[CmdletBinding()]
param(
    [string]$DumpFile,
    [switch]$KeepDb
)

$ErrorActionPreference = "Stop"

$pgBin = if ($env:PGBIN_PATH) {
    $env:PGBIN_PATH
} elseif (Get-Command psql.exe -ErrorAction SilentlyContinue) {
    Split-Path -Parent (Get-Command psql.exe).Source
} else {
    "C:\projects\metin2-extra\pg18\pgsql\bin"
}
$psql     = Join-Path $pgBin "psql.exe"
$createdb = Join-Path $pgBin "createdb.exe"
$dropdb   = Join-Path $pgBin "dropdb.exe"
$pgRestore = Join-Path $pgBin "pg_restore.exe"
$backupDir = if ($env:REFORGE_BACKUP_DIR) {
    $env:REFORGE_BACKUP_DIR
} else {
    "C:\projects\metin2-extra\backups"
}

foreach ($tool in @($psql, $createdb, $pgRestore)) {
    if (-not (Test-Path -LiteralPath $tool)) { throw "Tool not found: $tool" }
}

# Credentials via the environment so they never appear on the command line
$env:PGUSER     = "mt2"
$env:PGPASSWORD = "mt2"

# 1. Pick the dump (explicit path or newest nightly). The nightly naming is
#    metin2_<yyyy-MM-dd>.dump; exclude metin2_pg_* (the migration dump) and
#    sort by name date, not LastWriteTime.
if (-not $DumpFile) {
    $latest = Get-ChildItem -LiteralPath $backupDir -Filter "metin2_*.dump" -ErrorAction Stop |
        Where-Object { $_.BaseName -match '^metin2_\d{4}-\d{2}-\d{2}$' } |
        Sort-Object Name -Descending | Select-Object -First 1
    if (-not $latest) { throw "No nightly metin2_<date>.dump found in $backupDir" }
    $DumpFile = $latest.FullName
}
if (-not (Test-Path -LiteralPath $DumpFile)) { throw "Dump not found: $DumpFile" }
$mb = [math]::Round((Get-Item -LiteralPath $DumpFile).Length / 1MB, 2)
Write-Host "Drill dump: $DumpFile ($mb MB)"

# 2. Create the disposable drill database (createdb takes dbname positionally)
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$drillDb = "m2_drill_$stamp"
& $createdb -h 127.0.0.1 -p 5432 $drillDb
if ($LASTEXITCODE -ne 0) { throw "createdb FAILED ($drillDb) exit $LASTEXITCODE" }
Write-Host "Drill db created: $drillDb"

# 3. Restore, count, cleanup-on-failure
$restored = $false
try {
    & $pgRestore -h 127.0.0.1 -p 5432 -d $drillDb -Fc $DumpFile
    if ($LASTEXITCODE -ne 0) { throw "pg_restore FAILED with exit code $LASTEXITCODE" }
    $restored = $true

    # 4. Key-table counts (a missing table here means schema drift -> fail)
    $counts = & $psql -v ON_ERROR_STOP=1 -h 127.0.0.1 -p 5432 -t -A -d $drillDb -c @"
SELECT 'account.account=' || count(*) FROM account.account
UNION ALL SELECT 'player.player=' || count(*) FROM player.player
UNION ALL SELECT 'player.item=' || count(*) FROM player.item
UNION ALL SELECT 'player.mob_proto=' || count(*) FROM player.mob_proto
UNION ALL SELECT 'player.item_proto=' || count(*) FROM player.item_proto
UNION ALL SELECT 'player.guild=' || count(*) FROM player.guild
UNION ALL SELECT 'player.quest=' || count(*) FROM player.quest;
"@
    if ($LASTEXITCODE -ne 0) { throw "psql count FAILED with exit code $LASTEXITCODE" }
    Write-Host "== Key-table counts in $drillDb =="
    # guild/quest MAY legitimately be 0 on a fresh server; account, player,
    # mob_proto and item_proto must have rows.
    $mustBeNonEmpty = @("account.account", "player.player", "player.mob_proto", "player.item_proto")
    $counts | ForEach-Object {
        Write-Host "  $_"
        if ($_ -match "^(.+)=") {
            $table = $Matches[1]
            if ($mustBeNonEmpty -contains $table -and $_ -match "=(0|)$") {
                throw "Key table EMPTY: $table - schema drift or bad dump"
            }
        }
    }
    Write-Host "OK: restore drill PASSED ($DumpFile -> $drillDb)"
}
finally {
    if (-not $KeepDb) {
        & $dropdb -h 127.0.0.1 -p 5432 --force $drillDb 2>$null
        if ($LASTEXITCODE -eq 0) { Write-Host "Cleanup: $drillDb dropped" }
        else { Write-Host "WARNING: could not drop $drillDb - drop manually (dropdb --force $drillDb)" }
    } else {
        Write-Host "KeepDb: $drillDb left in place for inspection (restored=$restored)"
    }
}
