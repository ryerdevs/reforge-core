---
Type: Hub
Status: Current
Audience: All
Last verified: 2026-08-30
---

# Metin2 Reforge

Metin2 Reforge is an independent reimplementation of a Metin2-compatible
server, written from scratch in Rust. It is developed through behavioral
reverse engineering, packet and protocol analysis, compatibility fixtures, and
observation-driven tests. The goal is a server-authoritative implementation
whose observable behavior can interoperate with the compatibility client while
the server is replaced incrementally. The current result is a development/test
server: some end-to-end slices work, while other systems are partial or
deferred.

## What it is—and is not

**It is:**

- A project-authored Rust server implementation with a byte-oriented
  compatibility boundary.
- An incremental rewrite: each slice is implemented and checked against
  observed behavior before broader coverage is attempted.
- A server-authoritative design: clients send intentions; the server validates
  them, computes game facts, and owns durable state.

**It is not:**

- An official, endorsed, sponsored, or affiliated Metin2 product.
- A service running on, or dependent on, a rights holder's infrastructure.
- A claim that the Rust implementation is code from a rights holder, or that
  the project has complete feature or behavioral parity.

**Notice:** “Metin2” and other names and marks belong to their respective
holders. This is a neutral project notice, not legal advice. Each operator
must review the obligations applicable to their use, hosting, modification, and
distribution of the project and any related software or content.

## Server status at a glance

The matrix distinguishes real-client/runtime verification from focused
implementation checks. It is not a total-parity or production-readiness claim.
The [live handoff](documentation/progress.md) is the current summary, and the
[Gap Registry](documentation/plans/gap-registry.md) owns per-gap state, evidence,
owners, and exit criteria.

**Legend**

- ✅ Working and verified
- 🟡 Implemented but partial / limited
- 🔧 In progress
- ⏳ Not started / deferred

| Area | Status | What works now | What is missing/limited | Evidence |
|---|---|---|---|---|
| Runtime/login/world entry | ✅ Working and verified | Native Windows + PostgreSQL; real-client login → character select → world → movement is verified. | The current `source\deploy\win` binary still needs redeployment (G1.5), so the live deployment can lag the source tree. | [Live handoff](documentation/progress.md); [Gap Registry — G1.5](documentation/plans/gap-registry.md) |
| Protocol/network/auth | ✅ Working and verified | Byte-oriented protocol, framing, handshake, and auth/channel login paths work for the current compatibility flow. | This is not total packet parity. | [Wire reference](documentation/reference/login-flow.md); [Live handoff](documentation/progress.md) |
| Database/persistence | ✅ Working and verified | PostgreSQL repositories, `tokio-postgres`, WAL/idempotency, and selected ACID mutation paths. | Not every legacy domain is persisted or complete. | [ADR-0008](documentation/adr/0008-data-layer.md); [Gap Registry](documentation/plans/gap-registry.md) |
| World/ECS/movement | 🟡 Implemented but partial / limited | `bevy_ecs`, dynamic spawn, walkability checks, and the movement envelope. | Combat/world systems and tuning remain partial; cap and verification gates remain open. | [ADR-0010](documentation/adr/0010-domain-boundaries-and-data-ownership.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Items/economy | 🟡 Implemented but partial / limited | Attributes/sockets, refine, shops/trade, safebox, belt, and partial Dragon Soul support. | The effective item stack is **200**. The requested **2000** is blocked by BYTE-sized wire fields and requires a coordinated `u16` protocol/client migration. Dragon Soul reward-item creation and grid validation remain. | [Live handoff](documentation/progress.md); [Gap Registry — G0.1a and G2.7](documentation/plans/gap-registry.md) |
| Skills/buffs | 🟡 Implemented but partial / limited | Selected skill families and server-timed buffs. | Numeric `CASTING_SPEED`, remaining family coverage, passive skills, and quest-granted skills remain limited. | [Live handoff](documentation/progress.md); [Gap Registry — G2.4](documentation/plans/gap-registry.md) |
| Guild/party/social | 🟡 Implemented but partial / limited | Guild basics, grades/comments/ranking, war declaration/score handling, and party core actions with LINK/UNLINK. | Guild-war lifecycle/finish/scoreboard, marriage, block mode, and observer mode remain. | [Live handoff](documentation/progress.md); [Gap Registry — G2.1–G2.3](documentation/plans/gap-registry.md) |
| Quests | 🟡 Implemented but partial / limited | The DSL parser/converter covers the 194/194-file corpus; the runtime implements a subset. | `input_number`, additional actions, and additional quest content remain. | [Live handoff](documentation/progress.md); [Gap Registry — G2.5](documentation/plans/gap-registry.md) |
| GM | 🟡 Implemented but partial / limited | Command parsing, permission checks, and the main command subset. | Real `/transfer` and `/ipurge` behavior, `/set`, `/makeguild`, `/priv_empire`, targeting forms, `/view_equip`, `/observer`, and `/mount` remain. | [Live handoff](documentation/progress.md); [Gap Registry — G2.6](documentation/plans/gap-registry.md) |
| Events/dungeons | 🟡 Implemented but partial / limited | Event scheduling/lifecycle and dungeon WAIT → START → END behavior. | Raid, OX, three-way war, arena, wedding, monarch, and dungeon instances remain deferred. | [Live handoff](documentation/progress.md); [Gap Registry — G2.8 and G2.11c](documentation/plans/gap-registry.md) |
| Locale/data channel | 🟡 Implemented but partial / limited | Locale push/pull works. | A versioned manifest, delta delivery, and `LISTEN/NOTIFY` hot reload remain; data updates still require redeployment. | [Live handoff](documentation/progress.md); [Gap Registry — G2.10](documentation/plans/gap-registry.md) |
| Operations/quality | 🟡 Implemented but partial / limited | Target cleanup and the PostgreSQL backup have been verified for the current work. | The verify gate, formatting, changelog coverage, live documentation policy, storage-state closure, and redeploy remain. | [Live handoff](documentation/progress.md); [Gap Registry — G0.2 and G1](documentation/plans/gap-registry.md) |
| Client | ⏳ Not started / deferred | The existing compatibility client boundary is used for server verification. | The Rust client rewrite (F7) has not started. | [ADR-0013](documentation/adr/0013-client-rewrite.md); [Project rules](AGENTS.md) |

## What you can do today

Run the native Windows development stack and exercise the verified smoke path:

1. Ensure native PostgreSQL 18 is available on `127.0.0.1:5432` with the local
   `metin2` development database.
2. Start the stack:

   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\start_win.ps1
   ```

3. Open the compatibility client. If the disposable local fixture exists, use
   `test` / `1234` only for local development.
4. Log in, select a character, enter the world, and move.
5. Check status or stop the stack as separate operations:

   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\status.ps1
   powershell -ExecutionPolicy Bypass -File scripts\stop_win.ps1
   ```

This demonstrates the current login → select → world → movement smoke flow. It
does not make the project a complete production MMORPG, and the deployed binary
must still be refreshed before the latest source changes are represented in the
live stack ([G1.5](documentation/plans/gap-registry.md)).

## Current limitations

- The native deployment still runs a pre-Phase-1 binary until G1.5 is closed;
  source and runtime behavior can therefore differ.
- The protocol covers the verified compatibility flow, not every legacy packet
  or feature, and the database does not yet persist every legacy domain.
- Item stacks are capped at 200 on the current wire. Reaching 2000 needs a
  coordinated `u16` protocol/client migration and real-client verification.
- Dragon Soul refinement does not yet create the reward item or validate the
  complete 15-cell input grid.
- Skill/buff coverage is incomplete: numeric `CASTING_SPEED`, passive and
  quest-granted skills, and remaining family behavior are still limited.
- Party leadership rules, guild-war completion/scoreboard behavior, and
  marriage, block, and observer social behavior remain open.
- Quest `input_number`, additional GM dispatch/targeting behavior, and several
  event/dungeon contents or instances are deferred.
- Locale push/pull is not the complete data channel: manifest, delta, and
  notification-driven hot reload are not implemented, so data changes can
  require redeployment.
- Verification, formatting, changelog, documentation-policy, storage, and
  redeploy gates are not all closed. The benchmark is evidence for test runs,
  not a player-capacity promise, and anti-cheat coverage is not exhaustive.
- The Rust client rewrite is F7 work and has not started.

See the [live handoff](documentation/progress.md) and [Gap Registry](documentation/plans/gap-registry.md)
before treating any partial subsystem as complete.

## Next execution wave

Follow this order, with the [live handoff](documentation/progress.md) as the
session handoff and the [Gap Registry](documentation/plans/gap-registry.md) as
the per-item tracker:

1. Finish and gate **G0.1b–G0.1e**.
2. Close or reconcile the **G0.2** documented state if needed.
3. Execute **G1 verification, documentation, and deployment**, including the
   current Windows binary redeploy.
4. Take the selected **G2 gameplay** gaps.

## How it is built

- **Behavioral reverse engineering:** observe inputs, outputs, state transitions,
  and failure behavior of the compatibility target.
- **Packet/protocol analysis:** measure headers, framing, lengths, encodings,
  and wire state transitions. The [wire reference](documentation/reference/login-flow.md)
  records the current login contract.
- **Compatibility fixtures:** preserve observations as byte-level fixtures,
  regression tests, and runtime checks.
- **Original Rust implementation:** build the server in `source/reforge`; the
  compatibility boundary is an observable contract, not shared internal code.
- **Server authority:** validate movement and gameplay requests on the server;
  the client is a view, not the source of truth.

## Architecture

| Component | Responsibility |
|---|---|
| `protocol` | Byte-oriented client/server packets and compatibility codecs. |
| `network` | Tokio TCP transport, framing, handshake, and the auth module. |
| `database` | PostgreSQL access through `tokio-postgres`, domain repositories, batching, and WAL. |
| `game_core` | Pure gameplay modules plus the `bevy_ecs` world and systems. |
| `quest_dsl` | Quest language AST, parser, conversion, and related tooling. |
| `server_realms` | One server binary with `auth` and `channel` roles selected by configuration. |

Supporting workspace crates include `locale_import`, `bench_bot`, and the
temporary `mysql_proxy` compatibility adapter used for local parity work.

The runtime target is native Windows with PostgreSQL 18. WSL is retained only
as an on-demand compatibility environment, as recorded in [ADR-0012](documentation/adr/0012-windows-native-runtime-wsl-on-demand.md).

## Repository map

```text
source/
├── client/      # existing compatibility client source
├── reforge/     # Rust workspace
│   ├── protocol/
│   ├── network/
│   ├── database/
│   ├── game_core/
│   ├── quest_dsl/
│   └── server_realms/
├── tools/       # data, pack, and protocol tools
└── deploy/      # ignored local runtime artifacts
documentation/   # documentation hub, plans, ADRs, references, and history
scripts/        # Windows runtime/operations and WSL parity scripts
ROADMAP.md      # master plan
CHANGELOG.md    # chronological evidence record
AGENTS.md       # project rules and verified facts
```

## Build and run locally

From the repository root, build and test the Rust workspace:

```powershell
Set-Location source\reforge
cargo build --workspace
cargo test --workspace
Set-Location ..\..
```

The native Windows runtime requires the local PostgreSQL 18 setup and runtime
deployment described by [ADR-0012](documentation/adr/0012-windows-native-runtime-wsl-on-demand.md).
The scripts are intentionally separate launch/status/stop operations:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\start_win.ps1
powershell -ExecutionPolicy Bypass -File scripts\status.ps1
powershell -ExecutionPolicy Bypass -File scripts\stop_win.ps1
```

Runtime logs are under `source\deploy\win\logs`. WSL is not the daily runtime;
it is retained only for on-demand compatibility checks.

## Documentation and contribution context

- [Documentation hub](documentation/README.md)
- [Live handoff](documentation/progress.md)
- [Gap Registry](documentation/plans/gap-registry.md)
- [Master roadmap](ROADMAP.md)
- [Changelog](CHANGELOG.md)
- [Project rules](AGENTS.md)
- [Wire reference](documentation/reference/login-flow.md)
- [Architecture decisions](documentation/adr/)

Architecture and boundary decisions are recorded in
[`documentation/adr/`](documentation/adr/), including the data-layer and ECS
decisions in [ADR-0008](documentation/adr/0008-data-layer.md) and
[ADR-0010](documentation/adr/0010-domain-boundaries-and-data-ownership.md).

## Current checkout

This README does not pin an old commit. Obtain the exact commit for the
checkout being inspected with:

```powershell
git rev-parse HEAD
```

The claims above were last checked on **2026-08-30** against the repository
instructions and the [live handoff](documentation/progress.md).
