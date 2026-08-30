---
Type: Hub
Status: Current
Audience: All
Last verified: 2026-08-30
---

# Documentation — reforge-core

Welcome. Everything you need in 5 minutes.

## Quick Start

```powershell
powershell -File scripts/status.ps1   # snapshot: HEAD, dirty, binary, ports, CHANGELOG
powershell -File scripts/verify.ps1   # definition of done: fmt + test + clippy
powershell -File scripts/start_win.ps1 # up: PG 5432 + auth 30001 + channel 30003
powershell -File scripts/stop_win.ps1  # down
Set-Location source\reforge
cargo test --workspace                 # Rust tests
Set-Location ..\..
```

## Team — preset OmO (`openai/gpt-5.6-luna`, variant `max`)

| Agent | Role |
|-------|------|
| **orchestrator** | You talk to him. Decides, delegates, reviews. |
| **coder** | Writes clean, maintainable Rust |
| **fixer** | Finds and fixes simple and structural bugs |
| **oracle** | Architecture, ADRs, priorities |
| **explorer** | Fast codebase recon (graphs first) |
| **librarian** | Keeps docs correct, tells you when it's wrong |
| **designer** | UI/UX (when client work) |

## Slice Cycle

```
status.ps1 → slice → verify.ps1 → atomic commit → update documentation/progress.md
```

Each slice updates the canonical docs and the `progress.md` handoff; the
orchestrator commits one logical change after verification.

## Index — What to Read

| Goal | Read |
|------|------|
| What is done / doing / next? | [progress.md](./progress.md) (live handoff) + [plans/gap-registry.md](./plans/gap-registry.md) (per-gap tracker) |
| Open gaps: owner, evidence, exit criteria | [plans/gap-registry.md](./plans/gap-registry.md) |
| Which document decides when they disagree? | [reference/document-authority.md](./reference/document-authority.md) |
| DB in human language | [schema.md](./schema.md) |
| Never repeat | [rules.md](./rules.md) |
| Why PostgreSQL, ECS, WAL…? | [adr/](./adr/) (15 ADRs; ADR-0013 is superseded) |
| Byte-exact wire contract | [reference/login-flow.md](./reference/login-flow.md) |
| What changed with evidence? | [../CHANGELOG.md](../CHANGELOG.md) |
| Mission + protocol + runbook | [../AGENTS.md](../AGENTS.md) |
| Documentation policy | [DOCUMENTATION.md](./DOCUMENTATION.md) |
| Phase history (F0–F7) | [roadmap.md](./roadmap.md) + [../ROADMAP.md](../ROADMAP.md) (historical master) |
| Old plans, snapshots | [Historical archive index](./history-index.md) (archive is read-only) |

## Layout

```
documentation/
  README.md            → you are here (index + cheat sheet)
  DOCUMENTATION.md     → mandatory documentation policy
  roadmap.md           → done / doing / future
  schema.md            → DB
  rules.md             → never repeat (7 rules)
  progress.md          → live handoff
  plans/               → live plans (gap-registry.md)
  adr/                 → architecture decisions
  reference/login-flow.md
  history-index.md     → current navigation for the archive
  history/             → archived, read-only
scripts/
  status.ps1 / verify.ps1 / handoff.ps1 / start_win.ps1
source/reforge/        → Rust (protocol, network, database, game_core, server_realms)
```

## Between Sessions

`documentation/progress.md` has a **Handoff** section at the end. Next session the orchestrator reads it and knows where we left off.

## Stack

Rust 1.97 + bevy_ecs + tokio-postgres 0.7 + PostgreSQL 18 on native Windows + frozen C++ oracle
