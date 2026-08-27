# clean.ps1 - reclaim storage: -Full = cargo clean (workspace); default = stale deps/incremental (>7 d) + .omo/evidence
param([switch]$Full)

$root = Split-Path -Parent $PSScriptRoot
$ws   = Join-Path $root "source\reforge"
$size = { (Get-ChildItem $args[0] -Recurse -File -ErrorAction SilentlyContinue | Measure-Object -Sum Length).Sum }
$targets = if ($Full) { @(Join-Path $ws "target") } else {
    @(Join-Path $ws "target\debug\deps", Join-Path $ws "target\debug\incremental", Join-Path $root ".omo\evidence") }
$before = ($targets | ForEach-Object { & $size $_ } | Measure-Object -Sum).Sum

if ($Full) { Push-Location $ws; cargo clean; Pop-Location }
else {
    Get-ChildItem $targets[0..1] -File -ErrorAction SilentlyContinue |
        Where-Object LastWriteTime -lt (Get-Date).AddDays(-7) | Remove-Item -Force
    Remove-Item $targets[2] -Recurse -Force -ErrorAction SilentlyContinue
}

$after = ($targets | ForEach-Object { & $size $_ } | Measure-Object -Sum).Sum
"Freed {0:N1} MB" -f (($before - $after) / 1MB)