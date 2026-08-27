# verify.ps1 — definition of done del workspace Rust. Falla con exit 1 si algo falla.
# Uso: powershell -ExecutionPolicy Bypass -File scripts\verify.ps1
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$mf = Join-Path $root 'source/reforge/Cargo.toml'

function Invoke-Step([string]$name, [scriptblock]$cmd) {
    Write-Host "== $name =="
    try {
        & $cmd
        if ($LASTEXITCODE -ne 0) { throw "exit $LASTEXITCODE" }
    }
    catch {
        Write-Host "FALLO: $name -> $($_.Exception.Message)"
        exit 1
    }
}

Push-Location $root
try {
    Invoke-Step 'fmt --check' { cargo fmt --manifest-path $mf --check }
    Invoke-Step 'test --workspace -- --ignored' { cargo test --manifest-path $mf --workspace -- --ignored }
    Invoke-Step 'clippy --workspace -D warnings' { cargo clippy --manifest-path $mf --workspace -- -D warnings }
    Invoke-Step 'git diff --check' { git diff --check }
    Write-Host 'OK: verificación completa'
}
finally { Pop-Location }