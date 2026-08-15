# Check del loop "Base jugable" — version PowerShell (sin bash/WSL).
# Exit 0 = criterio cumplido. SCORE: <n> = tests pasados.
# Uso: powershell -ExecutionPolicy Bypass -File scripts\loop_check.ps1
$ErrorActionPreference = "SilentlyContinue"
Set-Location "$PSScriptRoot\..\source\reforge"

# 1. El workspace debe compilar.
cargo build --workspace | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Output "SCORE: 0"
    Write-Output "BUILD FAILED"
    exit 1
}

# 2. Tests del workspace.
$out = cargo test --workspace 2>&1 | Out-String
$passed = 0; $failed = 0
$out | Select-String -Pattern '(\d+) passed' -AllMatches | ForEach-Object {
    $_.Matches | ForEach-Object { $passed += [int]$_.Groups[1].Value }
}
$out | Select-String -Pattern '(\d+) failed' -AllMatches | ForEach-Object {
    $_.Matches | ForEach-Object { $failed += [int]$_.Groups[1].Value }
}

Write-Output "SCORE: $passed"
Write-Output "passed=$passed failed=$failed"

# 3. Criterio: 0 fallidos y al menos 564 (los del wave 46).
if ($failed -eq 0 -and $passed -ge 564) { exit 0 } else { exit 1 }
