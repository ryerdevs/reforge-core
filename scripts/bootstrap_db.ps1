# bootstrap_db.ps1 - Automated PostgreSQL schema bootstrap and synthetic seed loader (A2.3)
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\bootstrap_db.ps1 -Command init
#   powershell -ExecutionPolicy Bypass -File scripts\bootstrap_db.ps1 -Command seed
#   powershell -ExecutionPolicy Bypass -File scripts\bootstrap_db.ps1 -Command reset -Force
#   powershell -ExecutionPolicy Bypass -File scripts\bootstrap_db.ps1 -Command check
#   powershell -ExecutionPolicy Bypass -File scripts\bootstrap_db.ps1 -Command restore -File <path>
param(
    [ValidateSet("init", "seed", "reset", "check", "restore")]
    [string]$Command = "check",
    [string]$File,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$pyScript = Join-Path $PSScriptRoot "bootstrap_db.py"

$pyArgs = @($pyScript, $Command)
if ($File) { $pyArgs += $File }
if ($Force) { $pyArgs += "--force" }

& python @pyArgs
exit $LASTEXITCODE
