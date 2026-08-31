<#
.SYNOPSIS
    Build admin_tui and copy it to the deploy bundle automatically.

.DESCRIPTION
    Wrapper around cargo build -p admin_tui that always copies the
    resulting exe to source/deploy/win/admin_tui.exe so the deploy
    bundle stays self-contained without manual steps.

    Usage:
      powershell -File scripts/build_admin_tui.ps1           # release
      powershell -File scripts/build_admin_tui.ps1 -Debug    # debug
      powershell -File scripts/build_admin_tui.ps1 -Release -NoCopy  # build only

    The copy is local-only (source/deploy/**/*.exe is gitignored).
#>
[CmdletBinding()]
param(
    [switch]$DebugBuild,
    [switch]$Release,
    [switch]$NoCopy
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
$workspace = Join-Path -Path $root -ChildPath "source\reforge"
$deployWin = Join-Path -Path $root -ChildPath "source\deploy\win"

if (-not $DebugBuild -and -not $Release) { $Release = $true }

$profile = if ($DebugBuild) { "debug" } else { "release" }
$cargoArgs = @("build", "-p", "admin_tui")
if ($Release) { $cargoArgs += "--release" }

Write-Host "==> cargo $($cargoArgs -join ' ')  (in $workspace)"
Push-Location -Path $workspace
try {
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit $LASTEXITCODE" }
} finally {
    Pop-Location
}

if ($NoCopy) {
    Write-Host "Build done (NoCopy), not copying to deploy."
    return
}

$src = Join-Path -Path $workspace -ChildPath "target\$profile\admin_tui.exe"
$dst = Join-Path -Path $deployWin -ChildPath "admin_tui.exe"

if (-not (Test-Path -LiteralPath $src)) {
    throw "Expected binary not found: $src"
}

# Ensure deploy dir exists
if (-not (Test-Path -LiteralPath $deployWin)) {
    New-Item -ItemType Directory -Path $deployWin -Force | Out-Null
}

Copy-Item -LiteralPath $src -Destination $dst -Force
$bytes = (Get-Item -LiteralPath $dst).Length
Write-Host "==> Copied $src -> $dst ($bytes bytes)"

# Probe
& $dst --help 2>&1 | Select-Object -First 3 | ForEach-Object { Write-Host "  $_" }
Write-Host "OK: admin_tui $profile ready in deploy/win"
