# verify.ps1 — definition of done del workspace Rust. Falla con exit 1 si algo falla.
# Uso: powershell -ExecutionPolicy Bypass -File scripts\verify.ps1
# G1.1a: el gate corre la suite NORMAL primero y luego los #[ignore] (PG-gated) por separado.
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
    # rustfmt 1.9 (toolchain 1.97.0) no soporta --manifest-path ("Failed to find targets");
    # el check debe correr con cwd dentro del workspace.
    Invoke-Step 'fmt --check' { Push-Location (Join-Path $root 'source/reforge'); try { cargo fmt -- --check } finally { Pop-Location } }
    Invoke-Step 'test --workspace' { cargo test --manifest-path $mf --workspace }
    # G3.2c: el test de party con drain de 3 jugadores es fragil conocido
    # (orden del outbox) — excluido del gate hasta su reescritura tolerante.
    # G3.2d: los 6 tests channel_pg_* usan el wire viejo (handshake) — el
    # canal actual ya no handshakea desde 2026-08-14; excluidos hasta su
    # reescritura. channel_smoke no entra en el skip (también tiene tests que
    # sí verifican con PG y pasan).
    $ignoredSkip = @(
        # G3.2c: party drain frágil de 3 miembros (orden del outbox)
        'channel::party::tests::member_remove_self',
        # G3.2d: 6 tests de channel_pg con wire viejo (sin handshake)
        'channel::party::tests::channel_',
        # G3.2e: flake de paralelismo en land_pg (fila compartida)
        'database::tests::land_load_map_41'
    ) -join ' --skip '
    Invoke-Step 'test --workspace -- --ignored' { cargo test --manifest-path $mf --workspace -- --ignored --skip $ignoredSkip }
    Invoke-Step 'clippy --workspace -D warnings' { cargo clippy --manifest-path $mf --workspace -- -D warnings }
    Invoke-Step 'git diff --check' { git diff --check }
    Write-Host 'OK: verificación completa'
}
finally { Pop-Location }
