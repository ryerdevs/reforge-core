# check_docs.ps1 — G1.18: documentation gate beyond link-checking.
#
# Validates that every LIVE document (documentation/ minus history/) carries
# the mandatory metadata block (Type/Status/Audience/Last verified) and that
# the two live state files exist. Fails with exit 1 listing offenders.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts\check_docs.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$docs = Join-Path $root "documentation"
$required = @("Type:", "Status:", "Audience:", "Last verified:")

$failures = @()

# 1. Live state files must exist.
foreach ($f in @("documentation/progress.md", "documentation/plans/gap-registry.md")) {
    if (-not (Test-Path (Join-Path $root $f))) { $failures += "MISSING live state file: $f" }
}

# 2. Metadata block on every live markdown document (history/ is read-only).
Get-ChildItem -LiteralPath $docs -Recurse -Filter *.md |
    Where-Object { $_.FullName -notmatch '\\history\\' } |
    ForEach-Object {
        $rel = $_.FullName.Substring($root.Length + 1)
        $head = Get-Content -LiteralPath $_.FullName -TotalCount 10 -Encoding UTF8
        foreach ($field in $required) {
            if (-not ($head | Where-Object { $_ -like "*$field*" })) {
                $failures += "NO METADATA '$field': $rel"
            }
        }
    }

# 3. No duplicated current-status hub: only progress.md may claim "Status: Current" + "Type: Snapshot",
#    roadmap.md is the phase map (Type: Reference), registry is the tracker (Type: Plan).
$phase = Join-Path $docs "roadmap.md"
if (Test-Path $phase) {
    $head = Get-Content -LiteralPath $phase -TotalCount 10 -Encoding UTF8
    if (-not ($head | Where-Object { $_ -match '^\s*Type:\s*Reference' })) {
        $failures += "documentation/roadmap.md must be Type: Reference (phase map), see document-authority.md"
    }
}

if ($failures.Count -gt 0) {
    Write-Host "FALLO: check_docs"
    $failures | ForEach-Object { Write-Host "  - $_" }
    exit 1
}
Write-Host "OK: check_docs (metadata + live state files)"
