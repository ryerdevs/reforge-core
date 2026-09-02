# check_boundary.ps1 — A1: enforce the versioned Rust-server public boundary.
#
# The tracked index is the public checkout.  The status scan catches a newly
# added forbidden path without traversing ignored operator-only trees such as
# source/server.  It intentionally reports paths only and never prints secret
# values.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts\check_boundary.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$failures = New-Object 'System.Collections.Generic.List[string]'

$allowedBinaryFixtures = @(
    "source/reforge/protocol/tests/golden/auth_login3_40999.bin"
)
$binaryLookingPath = '(?i)\.(?:exe|bin|pack|dll|so|pdb|dylib|lib|a|o|obj|ilk|pyc|epk|eix|pak|gr2|dds)$'
$decompiledPath = '(?i)(?:^|/)(?:decompiled?|disassembly|reverse-engineered|reverse_engineering|ida|idapro|ghidra|x64dbg|dnspy|ilspy)(?:[._/-]|$)'
$decompiledExtension = '(?i)\.(?:i64|idb|gdt|dmp|ghidra)$'
$forbiddenPathRules = @(
    @{ Pattern = '(?i)^source/server(?:/|$)'; Reason = 'frozen C++ oracle' },
    @{ Pattern = '(?i)^source/deploy/(?!win(?:/|$))[^/]+(?:/|$)'; Reason = 'only source/deploy/win is public' },
    @{ Pattern = '(?i)^source/deploy/[^/]+/share(?:/|$)'; Reason = 'pack/config data' },
    @{ Pattern = '(?i)^source/(?:client|tools/pack)(?:/|$)'; Reason = 'client or pack source' }
)

function Add-Failure([string]$message) {
    if (-not $failures.Contains($message)) {
        [void]$failures.Add($message)
    }
}

function ConvertTo-DiagnosticText([string]$value) {
    $safe = New-Object System.Text.StringBuilder
    foreach ($character in $value.ToCharArray()) {
        if ([char]::IsControl($character)) {
            [void]$safe.Append(('\u{0:X4}' -f [int]$character))
        } else {
            [void]$safe.Append($character)
        }
    }
    return $safe.ToString()
}

function ConvertTo-RepoPath([string]$path) {
    $value = $path.Trim()
    if ($value.StartsWith('"') -and $value.EndsWith('"')) {
        $value = $value.Substring(1, $value.Length - 2)
    }
    while ($value.StartsWith('./')) {
        $value = $value.Substring(2)
    }
    return $value.Replace('\', '/')
}

function Invoke-GitLines([string[]]$arguments) {
    $output = @(& git -C $root @arguments 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "git $($arguments -join ' ') failed: $($output -join ' ')"
    }
    return @($output | ForEach-Object { [string]$_ })
}

function Get-StatusPaths {
    $paths = New-Object 'System.Collections.Generic.List[string]'
    foreach ($line in (Invoke-GitLines @('status', '--porcelain=v1', '--untracked-files=all'))) {
        if ($line.Length -lt 4) { continue }

        $statusCode = $line.Substring(0, 2)
        $pathPart = $line.Substring(3)
        if ($pathPart -match ' -> ') {
            $renamedPaths = $pathPart -split ' -> ', 2
            [void]$paths.Add((ConvertTo-RepoPath $renamedPaths[1]))
        } elseif ($statusCode -notmatch 'D') {
            [void]$paths.Add((ConvertTo-RepoPath $pathPart))
        } else {
            continue
        }
    }
    return @($paths | Select-Object -Unique)
}

function Test-TrackedPath([string]$path, [string]$source) {
    foreach ($rule in $forbiddenPathRules) {
        if ($path -match $rule.Pattern) {
            Add-Failure ("{0} forbidden path: {1} ({2})" -f $source, $path, $rule.Reason)
        }
    }

    if ($path -match $decompiledPath -or $path -match $decompiledExtension) {
        Add-Failure ("{0} decompiled artifact path: {1}" -f $source, $path)
    }

    if ($path -match $binaryLookingPath -and $allowedBinaryFixtures -notcontains $path) {
        Add-Failure ("{0} binary-looking public path: {1}" -f $source, $path)
    }
}

function Test-SecretScanPath([string]$path) {
    if ($path -match '(?i)^source/deploy/win/.+\.toml$') { return $true }
    if ($path -match '(?i)^source/deploy/win/scripts/.+\.(?:ps1|sh|bash|cmd|bat)$') { return $true }
    if ($path -match '(?i)^scripts/.+\.(?:ps1|sh|bash|cmd|bat)$') { return $true }
    if ($path -match '(?i)^source/reforge/(?:Cargo\.toml|.+/Cargo\.toml)$') { return $true }
    return $false
}

function Get-SecretValue([string]$rawValue) {
    $value = $rawValue.Trim()
    if ($value.StartsWith('"')) {
        $end = $value.IndexOf('"', 1)
        if ($end -gt 0) { return $value.Substring(1, $end - 1) }
    }
    if ($value.StartsWith("'")) {
        $end = $value.IndexOf("'", 1)
        if ($end -gt 0) { return $value.Substring(1, $end - 1) }
    }
    return ($value -split '\s+', 2)[0]
}

function Test-AllowedSecretValue([string]$value, [string]$line, [string]$key) {
    $candidate = $value.Trim()
    if ([string]::IsNullOrEmpty($candidate)) { return $true }

    # mt2 is the documented local development default in the deploy bundle. It
    # is allowed only in an explicit PG connection/env assignment; a generic
    # password/token assignment must still use supplied state or a placeholder.
    if ($candidate -match '(?i)^mt2$' -and
        ($key -match '(?i)^(?:PGPASSWORD|MYSQL_PASSWORD)$' -or $line -match '(?i)\bpg_conn\b')) {
        return $true
    }
    return $candidate -match '(?i)(?:<[^>\r\n]+>|CHANGE_ME|\$\{[^}\r\n]+\}|YOUR_)'
}

function Test-TrackedSecrets([string]$path) {
    if (-not (Test-SecretScanPath $path)) { return }

    $fullPath = Join-Path $root ($path.Replace('/', '\'))
    try {
        $lines = [System.IO.File]::ReadAllLines($fullPath, [System.Text.Encoding]::UTF8)
    } catch {
        Add-Failure ("unable to read tracked secret-scan file: {0}" -f $path)
        return
    }

    $extension = [System.IO.Path]::GetExtension($path).ToLowerInvariant()
    $isToml = $extension -eq '.toml'
    $tomlPattern = '(?i)(?<![A-Za-z0-9_])(?<key>password|token|secret|api_key)\s*=\s*(?<value>[^#\r\n]*)'
    $scriptPattern = '(?i)^\s*(?:[$](?:env:)?|export\s+)?(?<key>PGPASSWORD|MYSQL_PASSWORD|password|token|secret|api_key)\s*=\s*(?<value>[^#\r\n]*)'

    for ($lineNumber = 0; $lineNumber -lt $lines.Count; $lineNumber++) {
        $line = $lines[$lineNumber]
        $trimmed = $line.TrimStart()
        if ($trimmed.StartsWith('#') -or $trimmed.StartsWith('//') -or $trimmed.StartsWith(';')) {
            continue
        }

        if ($isToml) {
            $matches = [regex]::Matches($line, $tomlPattern)
        } else {
            $matches = @([regex]::Match($line, $scriptPattern))
        }

        foreach ($match in $matches) {
            if (-not $match.Success) { continue }
            $value = Get-SecretValue $match.Groups['value'].Value
            $key = $match.Groups['key'].Value
            if (-not (Test-AllowedSecretValue $value $line $key)) {
                Add-Failure ("{0} secret-like assignment at line {1} ({2})" -f $path, ($lineNumber + 1), $key)
            }
        }
    }
}

$trackedPaths = @(Invoke-GitLines @('ls-files', '--full-name') | ForEach-Object { ConvertTo-RepoPath $_ })
foreach ($path in $trackedPaths) {
    Test-TrackedPath $path 'tracked'
    Test-TrackedSecrets $path
}

# Only status-reported paths are inspected here.  Ignored source/server and
# runtime deploy trees are deliberately local operator state, not public files.
$statusPaths = @(Get-StatusPaths)
foreach ($path in $statusPaths) {
    Test-TrackedPath $path 'working-tree'
}

if ($failures.Count -gt 0) {
    Write-Host 'FALLO: check_boundary'
    $failures | ForEach-Object { Write-Host ("  - {0}" -f (ConvertTo-DiagnosticText $_)) }
    exit 1
}

Write-Host ("OK: check_boundary (tracked paths: {0}; status paths checked: {1})" -f $trackedPaths.Count, $statusPaths.Count)
