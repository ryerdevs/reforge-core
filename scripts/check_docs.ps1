# check_docs.ps1 — G1.18: documentation gate beyond link-checking.
#
# Validates that every LIVE document (documentation/ minus history/) carries
# the mandatory metadata block (Type/Status/Audience/Last verified) and that
# the two live state files exist. It also checks fragments on relative Markdown
# links in the active documentation surface. Fails with exit 1 listing
# offenders.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts\check_docs.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$docs = Join-Path $root "documentation"
$required = @("Type:", "Status:", "Audience:", "Last verified:")

$failures = @()

function Get-RelativePath([string]$path) {
    return $path.Substring($root.Length + 1).Replace('\', '/')
}

function ConvertTo-GitHubFragment([string]$heading) {
    $value = $heading.Trim()
    $value = [regex]::Replace($value, '\s+#+\s*$', '')
    $value = [regex]::Replace($value, '<[^>]+>', '')
    $value = [regex]::Replace($value, '!\[([^\]]*)\]\([^)]*\)', '$1')
    $value = [regex]::Replace($value, '\[([^\]]+)\]\([^)]*\)', '$1')

    $slug = New-Object System.Text.StringBuilder
    foreach ($character in $value.ToLowerInvariant().ToCharArray()) {
        if ([char]::IsWhiteSpace($character)) {
            [void]$slug.Append('-')
        } elseif ([char]::IsLetterOrDigit($character) -or $character -eq '-' -or $character -eq '_') {
            [void]$slug.Append($character)
        }
    }
    return $slug.ToString().Trim('-')
}

function Get-MarkdownFragments([string]$path) {
    $fragments = @{}
    $duplicates = @{}
    $inFence = $false

    foreach ($line in [System.IO.File]::ReadAllLines($path, [System.Text.Encoding]::UTF8)) {
        if ($line -match '^\s{0,3}(`{3,}|~{3,})') {
            $inFence = -not $inFence
            continue
        }
        if ($inFence) { continue }

        if ($line -match '^\s{0,3}#{1,6}(?:\s+|$)(.*)$') {
            $fragment = ConvertTo-GitHubFragment $Matches[1]
            if ([string]::IsNullOrEmpty($fragment)) { continue }

            if ($duplicates.ContainsKey($fragment)) {
                $duplicates[$fragment]++
                $anchor = "$fragment-$($duplicates[$fragment])"
            } else {
                $duplicates[$fragment] = 0
                $anchor = $fragment
            }
            $fragments[$anchor] = $true
        }
    }
    return $fragments
}

function Remove-MarkdownFences([string]$path) {
    $lines = @()
    $inFence = $false
    foreach ($line in [System.IO.File]::ReadAllLines($path, [System.Text.Encoding]::UTF8)) {
        if ($line -match '^\s{0,3}(`{3,}|~{3,})') {
            $inFence = -not $inFence
            $lines += ''
        } elseif ($inFence) {
            $lines += ''
        } else {
            $lines += $line
        }
    }
    return [string]::Join([Environment]::NewLine, $lines)
}

function Get-TargetPath([System.IO.FileInfo]$source, [string]$relativeTarget) {
    if ([string]::IsNullOrEmpty($relativeTarget)) { return $source.FullName }

    try {
        $decoded = [System.Uri]::UnescapeDataString($relativeTarget)
        $candidate = [System.IO.Path]::GetFullPath((Join-Path $source.DirectoryName ($decoded.Replace('/', '\'))))
    } catch {
        return $null
    }

    $rootPrefix = $root.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
    if (-not $candidate.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) { return $null }
    return $candidate
}

function Test-ExternalLink([string]$target) {
    return $target -match '^(?i:[a-z][a-z0-9+.-]*:|//)'
}

function Test-GeneratedArtifact([string]$path) {
    return $path -match '(?i)[\\/]target(?:[\\/]|$)|[\\/]source[\\/]client[\\/]graphify-out(?:[\\/]|$)|[\\/]source[\\/]deploy[\\/]win[\\/]logs(?:[\\/]|$)'
}

function Test-MarkdownFragments([System.IO.FileInfo]$source, [hashtable]$headingCache) {
    $linkPattern = '(?<!\!)\[(?<text>[^\]]+)\]\((?<target>[^)\r\n]+)\)'
    $document = Remove-MarkdownFences $source.FullName
    $found = @()

    foreach ($match in [regex]::Matches($document, $linkPattern)) {
        $target = $match.Groups['target'].Value.Trim()
        if ($target.StartsWith('<') -and $target.Contains('>')) {
            $target = $target.Substring(1, $target.IndexOf('>') - 1)
        } else {
            $target = ($target -split '\s+', 2)[0]
        }
        if (Test-ExternalLink $target) { continue }

        $hash = $target.IndexOf('#')
        if ($hash -lt 0) { continue }
        $relativeTarget = $target.Substring(0, $hash)
        $fragment = $target.Substring($hash + 1)
        if ([string]::IsNullOrEmpty($fragment)) { continue }

        try { $fragment = [System.Uri]::UnescapeDataString($fragment).ToLowerInvariant() }
        catch { continue }

        $targetPath = Get-TargetPath $source $relativeTarget
        if ($null -eq $targetPath) { continue }
        if (Test-GeneratedArtifact $targetPath) { continue }
        if ([System.IO.Path]::GetExtension($targetPath) -ine '.md') { continue }
        if ($targetPath -match '(?i)[\\/]documentation[\\/]history(?:[\\/]|$)') { continue }
        if (-not (Test-Path -LiteralPath $targetPath -PathType Leaf)) {
            $sourcePath = Get-RelativePath $source.FullName
            $linkText = ($match.Groups['text'].Value -replace '\s+', ' ').Trim()
            $found += "MISSING MARKDOWN TARGET FOR FRAGMENT '#$fragment': $sourcePath [$linkText] -> $target"
            continue
        }

        if (-not $headingCache.ContainsKey($targetPath)) {
            $headingCache[$targetPath] = Get-MarkdownFragments $targetPath
        }
        if (-not $headingCache[$targetPath].ContainsKey($fragment)) {
            $sourcePath = Get-RelativePath $source.FullName
            $linkText = ($match.Groups['text'].Value -replace '\s+', ' ').Trim()
            $found += "MISSING MARKDOWN FRAGMENT '#$fragment': $sourcePath [$linkText] -> $target"
        }
    }
    return $found
}

# 1. Live state files must exist.
foreach ($f in @("documentation/progress.md", "documentation/plans/gap-registry.md")) {
    if (-not (Test-Path (Join-Path $root $f))) { $failures += "MISSING live state file: $f" }
}

# 2. Metadata block on every live markdown document (history/ is read-only).
Get-ChildItem -LiteralPath $docs -Recurse -File -Filter *.md |
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

# 4. Active documentation fragments only. Root entry documents are explicitly
# enumerated; generated artifacts and the immutable history are not scanned.
$activeSources = @()
foreach ($entry in @("README.md", "ROADMAP.md", "CHANGELOG.md", "AGENTS.md")) {
    $entryPath = Join-Path $root $entry
    if (Test-Path -LiteralPath $entryPath -PathType Leaf) {
        $activeSources += Get-Item -LiteralPath $entryPath
    }
}
$activeSources += @(Get-ChildItem -LiteralPath $docs -Recurse -File -Filter *.md |
    Where-Object { $_.FullName -notmatch '(?i)[\\/]history(?:[\\/]|$)' })

$headingCache = @{}
foreach ($source in $activeSources) {
    $failures += @(Test-MarkdownFragments $source $headingCache)
}

if ($failures.Count -gt 0) {
    Write-Host "FALLO: check_docs"
    $failures | ForEach-Object { Write-Host "  - $_" }
    exit 1
}
Write-Host "OK: check_docs (metadata + live state files)"
