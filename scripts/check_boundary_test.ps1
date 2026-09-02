# check_boundary_test.ps1 — mutation tests for check_boundary.ps1.
#
# The fixtures are short-lived files in the current checkout.  They exercise
# both the status-path scan and the tracked-index scan without adding anything
# to the index permanently.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$checker = Join-Path $PSScriptRoot 'check_boundary.ps1'
$createdFiles = New-Object 'System.Collections.Generic.List[string]'
$createdDirectories = New-Object 'System.Collections.Generic.List[string]'

function Assert-True([bool]$condition, [string]$message) {
    if (-not $condition) { throw "ASSERTION FAILED: $message" }
}

function Invoke-Checker {
    $output = @(& powershell -NoProfile -ExecutionPolicy Bypass -File $checker 2>&1)
    [pscustomobject]@{
        ExitCode = $LASTEXITCODE
        Output = [string]::Join([Environment]::NewLine, ($output | ForEach-Object { [string]$_ }))
    }
}

function New-Fixture([string]$relativePath, [string]$content) {
    $fullPath = Join-Path $root ($relativePath.Replace('/', '\'))
    $parent = Split-Path $fullPath -Parent
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
        [void]$createdDirectories.Add($parent)
    }
    Set-Content -LiteralPath $fullPath -Value $content -Encoding UTF8
    [void]$createdFiles.Add($fullPath)
    return $fullPath
}

function New-BinaryFixture([string]$relativePath, [byte[]]$content) {
    $fullPath = Join-Path $root ($relativePath.Replace('/', '\'))
    $parent = Split-Path $fullPath -Parent
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
        [void]$createdDirectories.Add($parent)
    }
    [System.IO.File]::WriteAllBytes($fullPath, $content)
    [void]$createdFiles.Add($fullPath)
    return $fullPath
}

function Invoke-Git([string[]]$arguments) {
    & git -C $root @arguments *> $null
    if ($LASTEXITCODE -ne 0) { throw "git $($arguments -join ' ') failed" }
}

function Test-UntrackedSecretIsRejected {
    $relative = "source/deploy/win/boundary-test-$([Guid]::NewGuid().ToString('N')).toml"
    $path = New-Fixture $relative ('password = "not-a-placeholder"')
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'untracked non-placeholder secret assignment must fail'
        Assert-True ($result.Output -match 'secret-like assignment') 'secret failure should identify the check'
        Assert-True ($result.Output -notmatch 'not-a-placeholder') 'secret value must not be printed'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-UntrackedJsonSecretIsRejected {
    $relative = "source/tools/Mysql2Proto/HOWTO/boundary-test-$([Guid]::NewGuid().ToString('N')).json"
    $path = New-Fixture $relative '{"password":"not-a-placeholder"}'
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'untracked JSON secret assignment must fail'
        Assert-True ($result.Output -match 'secret-like assignment') 'JSON secret failure should identify the check'
        Assert-True ($result.Output -notmatch 'not-a-placeholder') 'JSON secret value must not be printed'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-UntrackedMysqlConfigSecretIsRejected {
    $relative = "source/tools/DBManager/.worker/config/boundary-test-$([Guid]::NewGuid().ToString('N'))/mysql.conf"
    $path = New-Fixture $relative 'localhost user not-a-placeholder database'
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'untracked positional MySQL password must fail'
        Assert-True ($result.Output -match 'secret-like assignment') 'MySQL config failure should identify the check'
        Assert-True ($result.Output -notmatch 'not-a-placeholder') 'MySQL config secret value must not be printed'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-ExactPlaceholdersAreAllowed {
    $relative = "source/deploy/win/boundary-test-$([Guid]::NewGuid().ToString('N')).toml"
    $content = @(
        'password = "<PASSWORD>"'
        'token = "CHANGE_ME"'
        'secret = "${SECRET}"'
        'api_key = "YOUR_API_KEY"'
    ) -join [Environment]::NewLine
    $path = New-Fixture $relative $content
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -eq 0) 'exact documented placeholders must pass'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-JsonPlaceholdersAreAllowed {
    $relative = "source/tools/Mysql2Proto/HOWTO/boundary-test-$([Guid]::NewGuid().ToString('N')).json"
    $content = @(
        '{"password":"<MYSQL_PASSWORD>",'
        ' "token":"${TOKEN}",'
        ' "secret":"CHANGE_ME",'
        ' "api_key":"YOUR_API_KEY"}'
    ) -join [Environment]::NewLine
    $path = New-Fixture $relative $content
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -eq 0) 'exact JSON placeholders must pass'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-MysqlConfigPlaceholdersAreAllowed {
    $relative = "source/tools/DBManager/.worker/config/boundary-test-$([Guid]::NewGuid().ToString('N'))/mysql.conf"
    $path = New-Fixture $relative 'localhost <MYSQL_USER> <MYSQL_PASSWORD> <MYSQL_DATABASE>'
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -eq 0) 'exact MySQL config placeholders must pass'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-PlaceholderSubstringIsRejected {
    $relative = "source/deploy/win/boundary-test-$([Guid]::NewGuid().ToString('N')).toml"
    $path = New-Fixture $relative ('password = "real-secret-CHANGE_ME"')
    $staged = $false
    try {
        Invoke-Git @('add', '--intent-to-add', '--', $relative)
        $staged = $true
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'a real value containing a marker must fail'
    } finally {
        if ($staged) { Invoke-Git @('reset', '--', $relative) }
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-ForbiddenArtifactPathIsRejected {
    $relative = "source/deploy/win/share/boundary-test-$([Guid]::NewGuid().ToString('N')).pack"
    $path = New-Fixture $relative 'synthetic fixture'
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'forbidden pack path must fail'
        Assert-True ($result.Output -match 'forbidden path|binary-looking public path') 'pack failure should identify its path rule'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-ClientRustPathIsRejected {
    $relative = "source/client_rust/boundary-test-$([Guid]::NewGuid().ToString('N')).rs"
    $path = New-Fixture $relative 'synthetic client source'
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'client_rust source must fail the public boundary'
        Assert-True ($result.Output -match 'forbidden path') 'client_rust failure should identify its path rule'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-RootClientPathIsRejected {
    $relative = "client/boundary-test-$([Guid]::NewGuid().ToString('N')).txt"
    $path = New-Fixture $relative 'synthetic client artifact'
    $staged = $false
    try {
        Invoke-Git @('add', '-f', '--intent-to-add', '--', $relative)
        $staged = $true
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'forced root client path must fail the public boundary'
        Assert-True ($result.Output -match 'forbidden path') 'root client failure should identify its path rule'
    } finally {
        if ($staged) { Invoke-Git @('reset', '--', $relative) }
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-ForcedBuildOutputPathIsRejected {
    $relative = "source/reforge/target/boundary-test-$([Guid]::NewGuid().ToString('N')).txt"
    $path = New-Fixture $relative 'synthetic build output'
    $staged = $false
    try {
        Invoke-Git @('add', '-f', '--intent-to-add', '--', $relative)
        $staged = $true
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'forced target output must fail the public boundary'
        Assert-True ($result.Output -match 'forbidden path') 'target failure should identify its path rule'
    } finally {
        if ($staged) { Invoke-Git @('reset', '--', $relative) }
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-ForcedRuntimePathIsRejected {
    $relative = "source/deploy/win/logs/boundary-test-$([Guid]::NewGuid().ToString('N')).txt"
    $path = New-Fixture $relative 'synthetic runtime log'
    $staged = $false
    try {
        Invoke-Git @('add', '-f', '--intent-to-add', '--', $relative)
        $staged = $true
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'forced runtime log must fail the public boundary'
        Assert-True ($result.Output -match 'forbidden path') 'runtime log failure should identify its path rule'
    } finally {
        if ($staged) { Invoke-Git @('reset', '--', $relative) }
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-ForcedDumpExtensionIsRejected {
    $relative = "source/deploy/win/boundary-test-$([Guid]::NewGuid().ToString('N')).dump"
    $path = New-Fixture $relative 'synthetic dump'
    $staged = $false
    try {
        Invoke-Git @('add', '-f', '--intent-to-add', '--', $relative)
        $staged = $true
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'forced dump output must fail the public boundary'
        Assert-True ($result.Output -match 'binary-looking public path') 'dump failure should identify its path rule'
    } finally {
        if ($staged) { Invoke-Git @('reset', '--', $relative) }
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-ForcedEnvironmentFileIsRejected {
    $relative = ".env.boundary-test-$([Guid]::NewGuid().ToString('N'))"
    $path = New-Fixture $relative 'PASSWORD=real-secret'
    $staged = $false
    try {
        Invoke-Git @('add', '-f', '--intent-to-add', '--', $relative)
        $staged = $true
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'forced environment file must fail the public boundary'
        Assert-True ($result.Output -match 'forbidden path') 'environment-file failure should identify its path rule'
    } finally {
        if ($staged) { Invoke-Git @('reset', '--', $relative) }
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-ExtensionlessBinaryContentIsRejected {
    $relative = "source/deploy/win/boundary-test-$([Guid]::NewGuid().ToString('N'))"
    $path = New-BinaryFixture $relative ([byte[]]@(0x4D, 0x49, 0x50, 0x58, 0x00, 0x01))
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'extensionless binary content must fail'
        Assert-True ($result.Output -match 'extensionless binary content') 'binary failure should identify content scanning'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-UnknownExtensionlessBinaryContentIsRejected {
    $relative = "source/deploy/win/boundary-test-$([Guid]::NewGuid().ToString('N'))"
    $path = New-BinaryFixture $relative ([byte[]](1..32))
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'unknown extensionless binary content must fail'
        Assert-True ($result.Output -match 'extensionless binary content') 'unknown binary failure should identify content scanning'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-ExtensionlessTextContentIsAllowed {
    $relative = "source/deploy/win/boundary-test-$([Guid]::NewGuid().ToString('N'))"
    $path = New-Fixture $relative 'synthetic text fixture without an extension'
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -eq 0) 'extensionless text content must remain allowed'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-Utf16TextContentIsAllowed {
    $relative = "source/deploy/win/boundary-test-$([Guid]::NewGuid().ToString('N'))"
    $text = [System.Text.Encoding]::Unicode.GetBytes('synthetic UTF-16 C++ resource text')
    $content = [byte[]](@([System.Text.Encoding]::Unicode.GetPreamble()) + @($text))
    $path = New-BinaryFixture $relative $content
    try {
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -eq 0) 'UTF-16 text source must remain allowed'
    } finally {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

function Test-GeneratedProtoOutputPathIsRejected {
    $relative = "source/tools/proto/generated-$([Guid]::NewGuid().ToString('N'))/nested/item_proto"
    $path = New-BinaryFixture $relative ([byte[]]@(0x4D, 0x49, 0x50, 0x58, 0x00, 0x01))
    $staged = $false
    try {
        Invoke-Git @('add', '-f', '--intent-to-add', '--', $relative)
        $staged = $true
        $result = Invoke-Checker
        Assert-True ($result.ExitCode -ne 0) 'generated proto output path must fail'
        Assert-True ($result.Output -match 'forbidden path') 'generated proto failure should identify its path rule'
    } finally {
        if ($staged) { Invoke-Git @('reset', '--', $relative) }
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
}

try {
    $baseline = Invoke-Checker
    Assert-True ($baseline.ExitCode -eq 0) 'clean boundary must pass before mutations'
    Test-UntrackedSecretIsRejected
    Test-UntrackedJsonSecretIsRejected
    Test-UntrackedMysqlConfigSecretIsRejected
    Test-ExactPlaceholdersAreAllowed
    Test-JsonPlaceholdersAreAllowed
    Test-MysqlConfigPlaceholdersAreAllowed
    Test-PlaceholderSubstringIsRejected
    Test-ForbiddenArtifactPathIsRejected
    Test-ClientRustPathIsRejected
    Test-RootClientPathIsRejected
    Test-ForcedBuildOutputPathIsRejected
    Test-ForcedRuntimePathIsRejected
    Test-ForcedDumpExtensionIsRejected
    Test-ForcedEnvironmentFileIsRejected
    Test-ExtensionlessBinaryContentIsRejected
    Test-UnknownExtensionlessBinaryContentIsRejected
    Test-ExtensionlessTextContentIsAllowed
    Test-Utf16TextContentIsAllowed
    Test-GeneratedProtoOutputPathIsRejected
    Write-Host 'OK: check_boundary mutation tests'
} finally {
    foreach ($file in $createdFiles) {
        Remove-Item -LiteralPath $file -Force -ErrorAction SilentlyContinue
    }
    foreach ($directory in ($createdDirectories | Sort-Object Length -Descending)) {
        if ((Test-Path -LiteralPath $directory -PathType Container) -and
            ((Get-ChildItem -LiteralPath $directory -Force | Measure-Object).Count -eq 0)) {
            Remove-Item -LiteralPath $directory -Force -ErrorAction SilentlyContinue
        }
    }
}
