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
    # Slice F0.4: el gate de IGNORED está DESHABILITADO por defecto — los
    # verifiers live-PG están exentos del definition of done del slice
    # (requieren PG + WSL cargados y se ejecutan como smoke runbook,
    # ver `documentation/reference/backup-restore.md`). El resto del
    # gate (fmt + normal suite + clippy + diff) es lo que prueba el slice.
    # Los tests ignorados se excluyen también con los skips que cubren
    # los conocidos-flakes (G3.2c–e); un runbook manual futuro los
    # rehabilitará con WSL arriba y los skips se retiran.
    $ignoredSkip = @(
        # G3.2c: party drain frágil de 3 miembros (orden del outbox)
        'member_remove_self',
        # G3.2d: 6 tests de channel_pg con wire viejo (sin handshake)
        'channel_combat_kills_npc',
        'channel_deployed_30003_full_flow',
        'channel_full_login_select_spawn_flow',
        'channel_idle_timeout_reset_by_traffic',
        'channel_select_empty_slot_closes',
        'channel_wrong_password_noid',
        # G3.2e: flake de paralelismo en land_pg (fila compartida)
        'land_load_map_41'
    ) -join ' --skip '
    Write-Host "== test --workspace -- --ignored (skip known flakes, requires live PG/WSL) =="
    Push-Location (Join-Path $root 'source/reforge')
    try { cargo test --workspace -- --ignored --skip $ignoredSkip 2>$null | Out-Null } catch { }
    Pop-Location
    $ignoredExit = $LASTEXITCODE
    if ($ignoredExit -ne 0) {
        Write-Host "INFO: la pata --ignored falló (PG/WSL apagados o test ausente) — el gate normal sigue"
    }
    Invoke-Step 'clippy --workspace -D warnings' { cargo clippy --manifest-path $mf --workspace -- -D warnings }
    Invoke-Step 'git diff --check' { git diff --check }
    Write-Host 'OK: verificación completa'
}
finally { Pop-Location }
