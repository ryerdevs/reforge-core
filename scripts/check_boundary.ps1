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
$binaryLookingPath = '(?i)\.(?:exe|bin|pack|dll|so|pdb|dylib|lib|a|o|obj|ilk|pyc|epk|eix|pak|gr2|dds|dump|bak|pcap|pcapng|rar|zip|7z|tar|gz|bz2|xz|zst)$'
$decompiledPath = '(?i)(?:^|/)(?:decompiled?|disassembly|reverse-engineered|reverse_engineering|ida|idapro|ghidra|x64dbg|dnspy|ilspy)(?:[._/-]|$)'
$decompiledExtension = '(?i)\.(?:i64|idb|gdt|dmp|ghidra)$'
$forbiddenPathRules = @(
    @{ Pattern = '(?i)^source/server(?:/|$)'; Reason = 'frozen C++ oracle' },
    @{ Pattern = '(?i)^source/deploy/(?!win(?:/|$))[^/]+(?:/|$)'; Reason = 'only source/deploy/win is public' },
    @{ Pattern = '(?i)^source/deploy/[^/]+/share(?:/|$)'; Reason = 'pack/config data' },
    @{ Pattern = '(?i)^source/deploy/[^/]+/(?:logs|backups)(?:/|$)'; Reason = 'runtime logs or backups' },
    @{ Pattern = '(?i)^source/(?:client|client_rust|tools/pack)(?:/|$)'; Reason = 'client or pack source' },
    @{ Pattern = '(?i)^(?:client|client-om2)(?:/|$)'; Reason = 'external client material' },
    @{ Pattern = '(?i)^source/tools/proto/(?:_out(?:/|$)|(?:[^/]+/)*(?:item_proto|mob_proto)$)'; Reason = 'generated client proto output' },
    @{ Pattern = '(?i)(?:^|/)\.env(?:\.(?!example$)[^/]+)?$'; Reason = 'environment credentials' },
    @{ Pattern = '(?i)(?:^|/)target(?:/|$)'; Reason = 'generated build output' }
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

    if (Test-ExtensionlessBinaryContent $path) {
        Add-Failure ("{0} extensionless binary content: {1}" -f $source, $path)
    }
}

function Test-ExtensionlessBinaryContent([string]$path) {
    if ($allowedBinaryFixtures -contains $path -or
        -not [string]::IsNullOrEmpty([System.IO.Path]::GetExtension($path))) {
        return $false
    }

    $fullPath = Join-Path $root ($path.Replace('/', '\'))
    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) { return $false }

    try {
        $bytes = [System.IO.File]::ReadAllBytes($fullPath)
    } catch {
        Add-Failure ("unable to inspect extensionless public path: {0}" -f $path)
        return $false
    }
    if ($bytes.Length -eq 0) { return $false }

    $prefixLength = [Math]::Min($bytes.Length, 8)
    $prefix = ([System.BitConverter]::ToString($bytes, 0, $prefixLength)).Replace('-', '')
    if ($prefix -match '^(?:4D5A|7F454C46|504B0304|4D495058|4D434F5A|89504E47|FFD8FF)') {
        return $true
    }

    # Decode the small set of text encodings used by the retained tooling. A
    # strict decoder rejects arbitrary bytes that happen to have no known
    # magic, while the control-character check catches valid UTF-8 control
    # streams. UTF-16 C++ resource sources are decoded as text rather than
    # being mistaken for binary merely because their bytes contain zeroes.
    $offset = 0
    $encoding = [System.Text.UTF8Encoding]::new($false, $true)
    if ($bytes.Length -ge 2 -and $bytes[0] -eq 0xFF -and $bytes[1] -eq 0xFE) {
        $encoding = [System.Text.UnicodeEncoding]::new($false, $false, $true)
        $offset = 2
    } elseif ($bytes.Length -ge 2 -and $bytes[0] -eq 0xFE -and $bytes[1] -eq 0xFF) {
        $encoding = [System.Text.UnicodeEncoding]::new($true, $false, $true)
        $offset = 2
    } elseif ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) {
        $offset = 3
    }

    try {
        $text = $encoding.GetString($bytes, $offset, $bytes.Length - $offset)
    } catch {
        return $true
    }
    foreach ($character in $text.ToCharArray()) {
        if ([char]::IsControl($character) -and $character -notin @("`t", "`n", "`r")) {
            return $true
        }
    }
    return $false
}

function Test-SecretScanPath([string]$path) {
    if ($path -match '(?i)^source/deploy/win/.+\.toml$') { return $true }
    if ($path -match '(?i)(?:^|/)\.env(?:\.example)?$') { return $true }
    if ($path -match '(?i)^source/deploy/win/scripts/.+\.(?:ps1|sh|bash|cmd|bat)$') { return $true }
    if ($path -match '(?i)^scripts/.+\.(?:ps1|sh|bash|cmd|bat)$') { return $true }
    if ($path -match '(?i)^source/reforge/(?:Cargo\.toml|.+/Cargo\.toml)$') { return $true }
    if ($path -match '(?i)\.json(?:\.sample)?$') { return $true }
    if ($path -match '(?i)(?:^|/)mysql\.conf$') { return $true }
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
        ($key -match '(?i)^(?:PGPASSWORD|MYSQL_PASSWORD)$' -or $line -match '(?i)^\s*pg_conn\s*=')) {
        return $true
    }
    return $candidate -match '(?i)^(?:<[^>\r\n]+>|CHANGE_ME|\$\{[^}\r\n]+\}|YOUR_[A-Za-z0-9_]*)$'
}

function Test-TrackedSecrets([string]$path) {
    if (-not (Test-SecretScanPath $path)) { return }

    $fullPath = Join-Path $root ($path.Replace('/', '\'))
    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) { return }
    try {
        $lines = [System.IO.File]::ReadAllLines($fullPath, [System.Text.Encoding]::UTF8)
    } catch {
        Add-Failure ("unable to read tracked secret-scan file: {0}" -f $path)
        return
    }

    $extension = [System.IO.Path]::GetExtension($path).ToLowerInvariant()
    $isToml = $extension -eq '.toml'
    $isJson = $path -match '(?i)\.json(?:\.sample)?$'
    $isMysqlConfig = $path -match '(?i)(?:^|/)mysql\.conf$'
    $tomlPattern = '(?i)(?<![A-Za-z0-9_])(?<key>password|token|secret|api_key)\s*=\s*(?<value>[^#\r\n]*)'
    $jsonPattern = '(?i)"(?<key>password|token|secret|api_key)"\s*:\s*(?<value>"(?:\\.|[^"\\])*"|null|[^,\r\n}]*)'
    $mysqlConfigPattern = '^\s*\S+\s+\S+\s+(?<value>\S+)\s+\S+\s*$'
    $scriptPattern = '(?i)^\s*(?:[$](?:env:)?|export\s+)?(?<key>PGPASSWORD|MYSQL_PASSWORD|password|token|secret|api_key)\s*=\s*(?<value>[^#\r\n]*)'

    for ($lineNumber = 0; $lineNumber -lt $lines.Count; $lineNumber++) {
        $line = $lines[$lineNumber]
        $trimmed = $line.TrimStart()
        if ($trimmed.StartsWith('#') -or $trimmed.StartsWith('//') -or $trimmed.StartsWith(';')) {
            continue
        }

        if ($isToml) {
            $matches = [regex]::Matches($line, $tomlPattern)
        } elseif ($isJson) {
            $matches = [regex]::Matches($line, $jsonPattern)
        } elseif ($isMysqlConfig) {
            $matches = @([regex]::Match($line, $mysqlConfigPattern))
        } else {
            $matches = @([regex]::Match($line, $scriptPattern))
        }

        foreach ($match in $matches) {
            if (-not $match.Success) { continue }
            $value = Get-SecretValue $match.Groups['value'].Value
            $key = if ($isMysqlConfig) { 'MYSQL_PASSWORD' } else { $match.Groups['key'].Value }
            if (-not (Test-AllowedSecretValue $value $line $key)) {
                Add-Failure ("{0} secret-like assignment at line {1} ({2})" -f $path, ($lineNumber + 1), $key)
            }
        }
    }
}

$trackedPaths = @(Invoke-GitLines @('ls-files', '--full-name') | ForEach-Object { ConvertTo-RepoPath $_ })
foreach ($path in $trackedPaths) {
    Test-TrackedPath $path 'tracked'
}

# Only status-reported paths are inspected here.  Ignored source/server and
# runtime deploy trees are deliberately local operator state, not public files.
$statusPaths = @(Get-StatusPaths)
foreach ($path in $statusPaths) {
    Test-TrackedPath $path 'working-tree'
}

# Scan both the index and newly reported working-tree paths.  A contributor can
# run this gate before staging a file, so secret checks must not wait for the
# next `git add` or CI checkout.
$secretScanPaths = @($trackedPaths + $statusPaths | Select-Object -Unique)
foreach ($path in $secretScanPaths) {
    Test-TrackedSecrets $path
}

if ($failures.Count -gt 0) {
    Write-Host 'FALLO: check_boundary'
    $failures | ForEach-Object { Write-Host ("  - {0}" -f (ConvertTo-DiagnosticText $_)) }
    exit 1
}

Write-Host ("OK: check_boundary (tracked paths: {0}; status paths checked: {1})" -f $trackedPaths.Count, $statusPaths.Count)
