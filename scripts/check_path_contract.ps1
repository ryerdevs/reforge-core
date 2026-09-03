# check_path_contract.ps1 — A2.1: verify path contract and relative discovery.
#
# Verifies:
# 1. No hardcoded repository paths (e.g. C:\projects\Metin2) in deploy/runtime scripts.
# 2. Scripts and TUI correctly discover executable, deploy directory, and scripts
#    from outside the repository root using repository-relative defaults and overrides.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts\check_path_contract.ps1
param([switch]$Verbose)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$failures = New-Object System.Collections.Generic.List[string]

function Fail([string]$msg) {
    [void]$failures.Add($msg)
}

Write-Host "== check_path_contract =="

# --- Check 1: Static scan for hardcoded maintainer repository root ---
$scriptFiles = @(
    (Join-Path $root "scripts\start_win.ps1"),
    (Join-Path $root "scripts\deploy_win.ps1")
)

foreach ($file in $scriptFiles) {
    if (-not (Test-Path $file)) {
        Fail "missing script: $file"
        continue
    }
    $lines = Get-Content -LiteralPath $file
    for ($i = 0; $i -lt $lines.Length; $i++) {
        $line = $lines[$i]
        # Ignore comments
        if ($line.Trim().StartsWith("#")) { continue }
        if ($line -match 'C:\\projects\\Metin2') {
            Fail "hardcoded repo path in $(Split-Path $file -Leaf):$($i+1): $line"
        }
    }
}

# --- Check 2: Relative execution in an external mock checkout ---
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("reforge_path_contract_" + [System.Guid]::NewGuid().ToString("N"))
try {
    $mockScripts = Join-Path $tempRoot "scripts"
    $mockDeploy  = Join-Path $tempRoot "source\deploy\win"
    $mockTarget  = Join-Path $tempRoot "source\reforge\target\release"

    New-Item -ItemType Directory -Force -Path $mockScripts | Out-Null
    New-Item -ItemType Directory -Force -Path $mockDeploy | Out-Null
    New-Item -ItemType Directory -Force -Path $mockTarget | Out-Null

    Copy-Item (Join-Path $root "scripts\start_win.ps1") (Join-Path $mockScripts "start_win.ps1")
    Copy-Item (Join-Path $root "scripts\deploy_win.ps1") (Join-Path $mockScripts "deploy_win.ps1")

    # Create dummy release binary
    $dummyExe = Join-Path $mockTarget "server_realms.exe"
    [System.IO.File]::WriteAllBytes($dummyExe, [byte[]]@(0x4D, 0x5A, 0x90, 0x00))

    # Run deploy_win.ps1 -SkipBuild -NoStart from [System.IO.Path]::GetTempPath()
    $prevPwd = Get-Location
    try {
        Set-Location ([System.IO.Path]::GetTempPath())
        $deployScript = Join-Path $mockScripts "deploy_win.ps1"
        $output = & powershell -NoProfile -ExecutionPolicy Bypass -File $deployScript -SkipBuild -NoStart 2>&1
        $exitCode = $LASTEXITCODE

        if ($exitCode -ne 0) {
            Fail "deploy_win.ps1 in mock clone exited with code ${exitCode}: $($output -join "`n")"
        }

        $deployedExe = Join-Path $mockDeploy "server_realms.exe"
        if (-not (Test-Path $deployedExe)) {
            Fail "deploy_win.ps1 did not copy server_realms.exe to mock deploy dir $mockDeploy"
        }
    }
    finally {
        Set-Location $prevPwd
    }

    # --- Check 3: admin_tui --probe with --deploy-dir ---
    $adminTuiExe = Join-Path $root "source\reforge\target\debug\admin_tui.exe"
    if (Test-Path $adminTuiExe) {
        $prevEap = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        $tuiOutput = & $adminTuiExe --deploy-dir $mockDeploy --probe
        $tuiExit = $LASTEXITCODE
        $ErrorActionPreference = $prevEap

        if ($tuiExit -ne 0) {
            Fail "admin_tui --probe failed on mock deploy dir: exit ${tuiExit} ($($tuiOutput -join ' '))"
        }
    }
}
finally {
    if (Test-Path $tempRoot) {
        Remove-Item -Recurse -Force -LiteralPath $tempRoot -ErrorAction SilentlyContinue
    }
}

if ($failures.Count -gt 0) {
    Write-Host "FALLO: check_path_contract" -ForegroundColor Red
    foreach ($f in $failures) {
        Write-Host "  - $f" -ForegroundColor Red
    }
    exit 1
}

Write-Host "OK: check_path_contract" -ForegroundColor Green
exit 0
