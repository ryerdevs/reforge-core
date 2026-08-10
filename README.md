# reforge-core

> **A classic 2004 MMORPG reimagined from scratch: Rust server, PostgreSQL 18, server-authoritative architecture, designed to scale.**
>
> An independent alternative server for a classic Asian hack & slash game, recreated by **reverse engineering** from the original binary — no affiliation with the original developer or publisher.

[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)
[![PostgreSQL](https://img.shields.io/badge/postgresql-18-blue.svg)](https://www.postgresql.org)
[![License: pending decision](https://img.shields.io/badge/license-pending--decision-lightgrey.svg)](docs/plans/server-rewrite.md)

---

## What is this?

**reforge-core** is the complete rewrite of the server of a **classic 2004 MMORPG** (hack & slash with a persistent world, guilds and PvP) in **Rust**, with 2026 technology and one clear goal: **do more with less**.

Not a line-by-line translation of the original C++ — a **structural redesign** that:

- 🛡️ **Removes the hacks at the root**: server-authoritative — the client sends *intentions*, the server computes *facts*. Speedhack, god-mode, dupe and memory hacking stop existing by design.
- 🚀 **Performs at its best**: tokio + per-region ECS, no shared locks, no SQL in the hot path. Ceiling of **1,000+ players per instance** (the original: ~300–500).
- 🌍 **One server for everyone**: regional channels (EUW/LAN/LAS) sharing the same database — play with local ping, switch region by just logging in, unified market.
- 🔄 **Hot reload**: texts, items and quests are edited in the DB and reloaded live — no restarts, no recompiles, no repacks.
- 📦 **No scripting**: quests move from Lua to a **declarative own DSL** — elegant, typed, with reusable families and blocks.
- 🗄️ **PostgreSQL 18** as transactional safety net: local WAL, idempotent `mutation_id`, RLS, failover — dupe is impossible by construction.

> ⚠️ **Legal note:** this project is an independent alternative server, built by reverse engineering. It has no affiliation with the developer or publisher of the original game and includes none of their assets or protected content — only original code.

## Current status

> Detailed, always-updated status: **`docs/CURRENT.md`** · Documentation index: **`docs/README.md`**

| Phase | Status |
|---|---|
| Original binary baseline verified (full login against the client) | ✅ |
| Legacy vs 2026-standards audit | ✅ |
| Unified rewrite plan | ✅ [plan](docs/plans/server-rewrite.md) |
| Quest DSL spec | ✅ [spec](docs/reference/quests/quest-dsl.md) |
| **F0** — Foundations (workspace, ADRs, byte-exact `protocol` crate) | ✅ 30/30 tests |
| **F1** — Network and transport (tokio: listener, framer, handshake) | ✅ 23/23 tests, 56/56 workspace — integration milestone pending (WSL) |
| **G-PG** — PostgreSQL cutover (ADR-0005) | ⏳ next — blocks F2 |
| **F2** — Auth (F2a server-side / F2b client batch) | ⏳ blocked on G-PG + ADR-0005 |
| F3–F6 (Rust server) | ⏳ in design |
| F7 (new client) | ⏳ after the server |

## Architecture in 30 seconds

```
Client (frozen original binary + 2 additive packs) ──► server_realms (single binary, roles auth|channel)
                                                           │  network (tokio: framer + handshake)
                                                           │  realm (parallel regions, ECS)
                                                           │  database (sqlx, never inline)
                                                           ▼
                                                   PostgreSQL 18 central
```

- One process per region (map clusters) with ECS (`bevy_ecs`): parallel simulation, single-writer per entity.
- Shared central DB across channels-regions: character, gold, market and guilds unified.
- Persistence in two classes: durable (items/gold → WAL + transaction) vs volatile (position → periodic).
- Full design: [docs/plans/server-rewrite.md](docs/plans/server-rewrite.md) · docs: [docs/README.md](docs/README.md) · status: [docs/CURRENT.md](docs/CURRENT.md).

## Repository — what it contains

```
source/
├── client/     # C++ client source v40999 (protocol contract)
├── server/     # C++ legacy server source (the reference to port)
├── reforge/    # RUST REWRITE (Cargo workspace): protocol, network, database, realm
│   └── server_realms/  # single binary, roles auth|channel by config
├── tools/      # Tools: DBManager, DumpProto, Mysql2Proto, switch_compiler
│   ├── pack/   #   Pack sources (python, uiscript, PackMakerLite)
│   └── proto/  #   Protocol metadata
└── deploy/     # Deployed runtime (local, not in git)
docs/           # Documentation hub: README.md, CURRENT.md, DOCUMENTATION.md,
                # plans/, reference/, guardrails/, decisions/ (ADRs),
                # history/ (superseded docs; Diátaxis modes on demand)
scripts/        # Server startup scripts (WSL/Linux)
ROADMAP.md      # Master plan by phases
CHANGELOG.md    # Chronological change log
AGENTS.md       # Agent instructions, verified facts, work rules
```

> **Binaries and packs are not in git.** The installed client, the `.epk` files, the build dependencies (`source/client/Extern/`) and the runtime (`source/deploy/`, WSL mirror) stay local or are distributed as Releases.

## Roadmap

| Phase | Content |
|---|---|
| **F0** | Rust workspace, ADRs, byte-exact `protocol` crate, capture harness — ✅ |
| **F1** | Network/transport with tokio: listener, framer, handshake — ✅ (WSL integration milestone pending) |
| **G-PG** | PostgreSQL 18 cutover + temporary legacy adapter (ADR-0005) — blocks F2 |
| **F2** | Auth (F2a) + first client batch (F2b) — blocked on G-PG |
| **F3** | Data layer (PostgreSQL) + server→client data channel |
| **F4** | World entry + UTF-8 names |
| **F5** | Full gameplay, scale benchmark, hot reload, API + metrics |
| **F6** | Full parity and replacement of the C++ baseline |
| **F7** | New client (wgpu + Slint UI) |

## Want to participate?

This project wants to **revive the classic genre with the community**: public protocol documentation, effective anti-bot, and a modern server operators can adopt. Issues, PRs and opinions on the plan are welcome — architecture decisions are discussed in `docs/` before being implemented.

## License

**Pending decision.** MPL-2.0 is proposed (permissive for private-server operators: they can run and modify without opening their entire work) but not yet accepted by the community — no `LICENSE` file exists until it is confirmed (ROADMAP open decision, plan §13). See the open question in [docs/plans/server-rewrite.md](docs/plans/server-rewrite.md).
