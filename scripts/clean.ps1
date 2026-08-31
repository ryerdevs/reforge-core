<#
.SYNOPSIS
    Preview and optionally remove safe, regenerable local artifacts.

.DESCRIPTION
    The script always prints its deletion plan first. Without -WhatIf it then
    requires the operator to type YES before anything is removed.
#>
[CmdletBinding(SupportsShouldProcess = $true, ConfirmImpact = 'Low')]
param(
    [Alias('Full')]
    [switch]$Target,
    [switch]$Logs,
    [switch]$Graphs,
    [switch]$Temp
)

$root = (Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
$workspace = Join-Path -Path $root -ChildPath 'source\reforge'
$serverRoot = Join-Path -Path $root -ChildPath 'source\server'
$clientRoot = Join-Path -Path $root -ChildPath 'source\client'
$serverGraph = Join-Path -Path $serverRoot -ChildPath 'graphify-out'
$actions = New-Object 'System.Collections.Generic.List[object]'
$seen = @{}

# TODO: evaluate hardlinks for source/deploy/main/locale/germany and spain;
# they duplicate 648 files (~18 MB), but runtime ownership and portability must
# be verified before changing either locale tree.

function Get-ByteCount {
    param([Parameter(Mandatory)][string]$Path)

    try {
        $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
        if (-not $item.PSIsContainer) {
            return [int64]$item.Length
        }

        $measure = Get-ChildItem -LiteralPath $Path -Recurse -File -Force -ErrorAction Stop |
            Measure-Object -Property Length -Sum
        if ($null -eq $measure.Sum) {
            return [int64]0
        }
        return [int64]$measure.Sum
    }
    catch {
        Write-Warning "Could not measure '$Path': $($_.Exception.Message)"
        return [int64]0
    }
}

function Test-EmptyDirectory {
    param([Parameter(Mandatory)][string]$Path)

    try {
        return $null -eq (Get-ChildItem -LiteralPath $Path -Force -ErrorAction Stop |
            Select-Object -First 1)
    }
    catch {
        return $false
    }
}

function Test-ProtectedPath {
    param([Parameter(Mandatory)][string]$Path)

    $fullPath = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    foreach ($protectedRoot in @($serverRoot, $clientRoot)) {
        $protected = [IO.Path]::GetFullPath($protectedRoot).TrimEnd('\')
        $inside = $fullPath.Equals($protected, [StringComparison]::OrdinalIgnoreCase) -or
            $fullPath.StartsWith($protected + '\', [StringComparison]::OrdinalIgnoreCase)
        if ($inside -and -not $fullPath.Equals($serverGraph, [StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }
    return $false
}

function Add-Candidate {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Reason
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }

    $fullPath = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    if (Test-ProtectedPath -Path $fullPath) {
        throw "Refusing to add protected path: $fullPath"
    }

    $key = $fullPath.ToLowerInvariant()
    if ($seen.ContainsKey($key)) {
        return
    }

    $seen[$key] = $true
    [void]$actions.Add([pscustomobject]@{
            Path   = $fullPath
            Reason = $Reason
            Bytes  = Get-ByteCount -Path $fullPath
        })
}

if (-not ($Target -or $Logs -or $Graphs -or $Temp)) {
    Write-Host 'Nothing selected. Use one or more of: -Target -Logs -Graphs -Temp.'
    return
}

if ($Target) {
    Add-Candidate -Path (Join-Path -Path $workspace -ChildPath 'target') -Reason 'Rust build output'
}

if ($Logs) {
    $logsPath = Join-Path -Path $root -ChildPath 'source\deploy\win\logs'
    $cutoff = (Get-Date).AddDays(-7)
    $benchmarkPattern = '^bench-run-.*\.md$'
    $keptBenchmarks = @()

    if (Test-Path -LiteralPath $logsPath -PathType Container) {
        Get-ChildItem -LiteralPath $logsPath -File -Force -ErrorAction Stop | ForEach-Object {
            if ($_.Name -match $benchmarkPattern) {
                $keptBenchmarks += $_
            }
            elseif ($_.LastWriteTime -lt $cutoff) {
                Add-Candidate -Path $_.FullName -Reason 'Log older than 7 days'
            }
        }
    }

    if ($keptBenchmarks.Count -gt 0) {
        Write-Host ('Keeping benchmark reports: ' + ($keptBenchmarks.Name -join ', '))
    }
}

if ($Graphs) {
    @(
        (Join-Path -Path $root -ChildPath '.codegraph'),
        (Join-Path -Path $root -ChildPath 'graphify-out'),
        $serverGraph
    ) | ForEach-Object {
        Add-Candidate -Path $_ -Reason 'Generated graph data'
    }
}

if ($Temp) {
    $walPaths = @(
        (Join-Path -Path $root -ChildPath 'wal')
    )
    Get-ChildItem -LiteralPath $workspace -Directory -Force -ErrorAction SilentlyContinue |
        Where-Object Name -ne 'target' |
        ForEach-Object {
            $walPaths += Join-Path -Path $_.FullName -ChildPath 'wal'
        }

    foreach ($walPath in $walPaths) {
        if ((Test-Path -LiteralPath $walPath -PathType Container) -and
            (Test-EmptyDirectory -Path $walPath)) {
            Add-Candidate -Path $walPath -Reason 'Empty runtime WAL directory'
        }
    }

    # A failed path conversion once left this disposable target under the
    # workspace. Match only the C-like root with the exact stale subtree.
    Get-ChildItem -LiteralPath $workspace -Directory -Force -ErrorAction SilentlyContinue |
        ForEach-Object {
            $name = $_.Name
            $looksLikeDriveRoot = $name -eq 'C' -or
                ($name.Length -eq 2 -and $name[0] -eq 'C' -and $name[1] -ne ':')
            $staleTarget = Join-Path -Path $_.FullName -ChildPath 'projects\metin2-extra\target'
            if ($looksLikeDriveRoot -and (Test-Path -LiteralPath $staleTarget -PathType Container)) {
                Add-Candidate -Path $staleTarget -Reason 'Stale malformed workspace target'
            }
        }
}

Write-Host ''
Write-Host 'Cleanup preview (nothing deleted yet):'
if ($actions.Count -eq 0) {
    Write-Host '  No matching artifacts found.'
}
else {
    foreach ($action in $actions) {
        Write-Host ('  DELETE {0} ({1:N2} MB) - {2}' -f
            $action.Path, ($action.Bytes / 1MB), $action.Reason)
    }
    $plannedBytes = ($actions | Measure-Object -Property Bytes -Sum).Sum
    Write-Host ('Potential reclaim: {0:N2} MB' -f ($plannedBytes / 1MB))
}

Write-Host 'Protected/manual-only: source/server source tree (~62.5 MB) and source/client (~2.55 GB); only generated source/server/graphify-out may be selected.'
Write-Host 'Deferred: source/deploy/main/locale/germany and spain duplicate ~18 MB; no locale files are changed.'

if ($actions.Count -eq 0) {
    return
}
if ($WhatIfPreference) {
    Write-Host 'Dry run only (-WhatIf); no files were deleted.'
    return
}

$confirmation = Read-Host 'Type YES to delete the listed artifacts'
if ($confirmation -cne 'YES') {
    Write-Host 'Cancelled; no files were deleted.'
    return
}

$removedBytes = [int64]0
$failures = 0
foreach ($action in $actions) {
    try {
        if (Test-ProtectedPath -Path $action.Path) {
            throw "Protected path detected at deletion time: $($action.Path)"
        }
        if ($PSCmdlet.ShouldProcess($action.Path, 'Remove cleanup artifact')) {
            Remove-Item -LiteralPath $action.Path -Recurse -Force -ErrorAction Stop
            $removedBytes += $action.Bytes
            Write-Host ('  DELETED {0}' -f $action.Path)
        }
    }
    catch {
        $failures++
        Write-Warning "Could not delete '$($action.Path)': $($_.Exception.Message)"
    }
}

Write-Host ('Removed: {0:N2} MB' -f ($removedBytes / 1MB))
if ($failures -gt 0) {
    throw "$failures cleanup action(s) failed."
}
