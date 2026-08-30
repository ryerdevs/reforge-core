---
Type: Hub
Status: Current
Audience: Contributors
Last verified: 2026-08-30
---

# `reforge` Rust workspace

This directory contains the public Rust server workspace. It is an incremental,
server-authoritative reimplementation: the frozen C++ server is a local parity
oracle, not a build input for this checkout. See [ADR-0003](../../documentation/adr/0003-reforge-workspace-rust-layout.md),
[ADR-0012](../../documentation/adr/0012-windows-native-runtime-wsl-on-demand.md),
and [ADR-0015](../../documentation/adr/0015-rust-only-public-repository.md).

## Crates

| Path | Role |
|---|---|
| `protocol/` | Byte-oriented client/server packets and compatibility codecs. |
| `network/` | Tokio TCP transport, framing, handshake, and authentication support. |
| `database/` | PostgreSQL repositories, batching, and WAL-backed mutations. |
| `game_core/` | Pure gameplay modules plus the `bevy_ecs` world and systems. |
| `server_realms/` | One binary with `auth` and `channel` roles selected by configuration. |
| `mysql_proxy/` | Temporary MySQL-to-PostgreSQL adapter for local legacy parity work. |
| `locale_import/` | Locale-data importer for PostgreSQL. |
| `bench_bot/` | Compatibility and benchmark bot harness. |
| `quest_dsl/` | Quest language AST, parser, conversion, and related tooling. |

`auth` is a module of `network` and a role of `server_realms`, not a separate
workspace crate. The workspace membership is defined in [`Cargo.toml`](Cargo.toml).

## Verification

Run these commands from this directory:

```powershell
cargo build --workspace
cargo test --workspace
cargo fmt -- --check
cargo clippy --workspace -- -D warnings
```

For current status, verified test counts, open gaps, and the next handoff, use
the [project documentation hub](../../documentation/README.md) and
[`documentation/progress.md`](../../documentation/progress.md). Test totals are
not duplicated here because they change as verifiers are added.
