---
Type: Hub
Status: Current
Audience: All
Last verified: 2026-08-29
---

# reforge-core

> A classic MMORPG server being structurally rewritten in Rust with a server-authoritative design and PostgreSQL 18.
>
> This is an independent alternative server built by reverse engineering the original binary. It is not affiliated with the original developer or publisher and contains no original protected assets.

[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)
[![PostgreSQL](https://img.shields.io/badge/postgresql-18-blue.svg)](https://www.postgresql.org)
[![License: pending decision](https://img.shields.io/badge/license-pending--decision-lightgrey.svg)](documentation/history/plans/server-rewrite.md)

---

## What is this?

**reforge-core** is an incremental, structural rewrite of a classic 2004 MMORPG server. It preserves observable wire behavior where required while replacing the legacy implementation module by module.

- **Server-authoritative:** the client sends intentions; the server computes facts.
- **Rust rewrite:** protocol, transport, authentication, world entry, movement, persistence, and gameplay modules are being replaced incrementally.
- **Transactional persistence:** durable mutations use PostgreSQL transactions and the local WAL path.
- **Legacy compatibility:** the original client remains the playable protocol reference while the Rust server progresses.

## Current status

> HEAD: `4579fcb` (`docs(progress): record item cap slice`), verified 2026-08-29.
>
> Live handoff: [documentation/progress.md](documentation/progress.md) · [Gap Registry](documentation/plans/gap-registry.md) · [documentation hub](documentation/README.md)

| Area | Status |
|---|---|
| Runtime | ✅ Native Windows runtime with PostgreSQL 18 and `server_realms` roles `auth`/`channel` (ADR-0012) |
| F0/F1/G-PG/F2 | ✅ Protocol, transport, PostgreSQL cutover, authentication, world entry, and movement implemented and verified |
| Database, WAL, ACID paths | ✅ PostgreSQL persistence, local WAL, and transactional mutation paths implemented and verified |
| `game_core`, social, quests, GM | 🟡 Partial; see the [live handoff](documentation/progress.md) and [Gap Registry](documentation/plans/gap-registry.md) |
| Tests | 891 workspace tests listed at a historical measurement point; this is not a fresh suite result or a pass guarantee |
| G0–G3 | ⏳ Pending execution |
| G0.1a item stacks | ✅ Safely enforced at an effective cap of 200; the 2000 target is blocked until a coordinated BYTE-to-u16 protocol/client migration |
| G0.2 storage | ✅ Target cleanup and backup verified 2026-08-29; G0 remains open because other cap rows still require execution |
| F7 Rust client rewrite | ⏳ Not started |

## Architecture

```text
Legacy C++ client (playable reference)
                 │
                 ▼
server_realms (single binary; role = auth or channel)
        ├── protocol   (byte-exact wire contract)
        ├── network    (Tokio transport and authentication)
        ├── game_core  (ECS and gameplay domain)
        └── database   (tokio-postgres, WAL, transactions)
                              │
                              ▼
                    PostgreSQL 18 on Windows
```

ADR-0012 defines native Windows as the runtime. WSL is retained only as an on-demand environment for the frozen C++ parity oracle. RLS, failover, the versioned data manifest, and notification-driven hot reload remain future work; they are not presented as complete here.

## Repository map

```text
source/
├── client/      # C++ client source and protocol reference
├── server/      # frozen C++ parity oracle; local-only and not tracked by Git
├── reforge/     # Rust workspace
│   ├── protocol/
│   ├── network/
│   ├── database/
│   ├── game_core/
│   └── server_realms/  # one binary, auth|channel roles by config
├── tools/       # DBManager, DumpProto, Mysql2Proto, pack and protocol tools
└── deploy/      # local runtime/deployment artifacts; not tracked by Git
documentation/   # documentation hub, ADRs, plans, references and history
scripts/        # Windows runtime and operations scripts
ROADMAP.md      # master plan
CHANGELOG.md    # chronological evidence record
AGENTS.md       # project instructions and verified facts
```

Build artifacts, installed clients, packs, backups, the runtime, and the frozen C++ oracle remain local or are distributed separately.

## Roadmap

- Foundations, transport, PostgreSQL cutover, authentication, world entry, and movement: implemented and verified.
- `game_core` gameplay and social/content coverage: partial; the [Gap Registry](documentation/plans/gap-registry.md) owns the open items.
- G0–G3: pending execution.
- F7 Rust client: not started; the decision is recorded in [ADR-0013](documentation/adr/0013-client-rewrite.md).

## Local runtime

```powershell
powershell -ExecutionPolicy Bypass -File scripts\start_win.ps1
powershell -ExecutionPolicy Bypass -File scripts\status.ps1
powershell -ExecutionPolicy Bypass -File scripts\stop_win.ps1
```

## Contributing

Issues, pull requests, and architecture discussions are welcome. Read the [documentation hub](documentation/README.md) and [live handoff](documentation/progress.md) first; architecture decisions are recorded in `documentation/` before implementation.

## License

**Pending decision.** MPL-2.0 is proposed, but no `LICENSE` file exists until the community confirms it. The open question is recorded in the [historical server rewrite plan](documentation/history/plans/server-rewrite.md).
