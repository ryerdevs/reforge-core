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
the server is replaced incrementally.

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

## How it is built

1. **Behavioral reverse engineering:** observe a reference runtime's inputs,
   outputs, state transitions, and failure behavior.
2. **Packet/protocol analysis:** measure headers, framing, lengths, encodings,
   and wire state transitions.
3. **Compatibility fixtures:** preserve observations as byte-level fixtures,
   regression tests, and runtime checks. The [wire reference](documentation/reference/login-flow.md)
   records the current login contract.
4. **Original Rust implementation:** implement the server in the
   `source/reforge` workspace; compatibility is an observable contract, not a
   shared internal implementation.
5. **PostgreSQL 18:** use PostgreSQL as the durable store, with domain
   repositories, transactional mutation paths, batching, and local WAL
   durability/replay.
6. **Server authority:** validate movement and gameplay requests on the server
   and do not treat client state as authoritative.

## Verified slices

These are bounded slices verified by tests, packet fixtures, logs, or a
real-client check. They are **not** a claim of total parity.

- **Runtime:** native Windows PostgreSQL 18 runtime; real-client
  login → character select → world → movement was verified. The local
  deployment still needs the current Phase 1 binary redeployed (G1.5).
- **Protocol and network:** byte-oriented protocol codecs, Tokio transport,
  framing, handshake/authentication paths, and compatibility fixtures.
- **Data:** PostgreSQL repositories, durable local WAL and idempotent replay,
  and selected ACID mutation paths.
- **World:** `bevy_ecs` world state, dynamic spawn/despawn, map walkability,
  movement-envelope validation, and related world-entry behavior.
- **Gameplay:** selected skills and server-timed buffs; NPC shops and player
  trade; guild and party slices; safebox and belt storage; Dragon Soul and
  refine slices.
- **Quests:** the quest DSL parser/converter and a runtime subset, including
  suspension and selected persistence/effect paths.
- **Locale:** locale import/cache work and the verified locale push/pull wire
  slice.
- **Benchmarking:** a bot benchmark harness with wire capture and runtime
  metrics. Its measurements are test evidence, not a player-capacity promise.

## Open work

The [live handoff](documentation/progress.md) is the current status summary;
the [Gap Registry](documentation/plans/gap-registry.md) owns each item's state,
evidence, owner, and exit criteria.

- **G0 — architecture and storage:**
  - `G0.1a`: the effective item-stack cap is **200**. The requested **2000**
    target is blocked by the current BYTE-sized item-count wire fields and
    requires a coordinated `u16` protocol/client migration plus real-client
    verification.
  - `G0.1b`: retained absolute movement limit `6000` units; Oracle Gate is
    pending.
  - `G0.1c`: retained spawn view `300000` and despawn radius `310000`; Oracle
    Gate is pending.
  - `G0.1d`: retained boot movement-speed cap `200` before BYTE serialization;
    real-client verification and the gate are pending.
  - `G0.1e`: retained `GOLD_MAX = 2_000_000_000`; Oracle Gate is pending.
   - `G0.2`: build-artifact storage was cleaned and the PostgreSQL backup was
     verified on 2026-08-29; the storage gate is closed for this block.
- **G1 — gates, documentation, and deployment** (`G1.1a–b`, `G1.2`, `G1.3`,
  `G1.5–G1.6`, `G1.8a–b`, `G1.9`, `G1.10a–b`, `G1.11a–c`, `G1.12a–b`,
  `G1.13`, `G1.14a–b`, `G1.15–G1.18`): normal and ignored test gates,
  formatting, changelog coverage, the live documentation policy and
  link/metadata checks, and redeployment of the current Windows binary remain
  open.
- **G2 — residual gameplay and content** (`G2.1a–d`, `G2.2a–d`, `G2.3a–c`,
  `G2.4–G2.7b`, `G2.8a–f`, `G2.9–G2.10`, `G2.11a–c`): party
  leadership/bonus/update rules; messenger marriage, block, observer, and
  locale behavior; guild-war lifecycle, finish, and scoreboard; numeric
  `CASTING_SPEED`; quest `input_number`; remaining GM dispatch/targeting
  commands; Dragon Soul reward creation and grid validation; deferred
  raid/OX/three-way-war/arena/wedding/monarch content; real item weights; the
  data-channel manifest and hot reload; and land, horse/pet, and
  dungeon-instance work.
- **G3 — hygiene and test debt** (`G3.1a–c`, `G3.2a–b`): stale comments and
  the two ignored/flaky lifecycle tests still require resolution or an explicit
  isolated-gate policy.
- **F7 — Rust client:** the client rewrite is planned but **has not started**.

Closed prerequisite rows in the registry do not close G0–G3 or imply total
parity.

## Architecture

| Component | Responsibility |
|---|---|
| `protocol` | Byte-exact client/server packets and compatibility codecs. |
| `network` | Tokio TCP transport, framing, handshake, and the auth module. |
| `database` | PostgreSQL access through `tokio-postgres`, domain repositories, batching, and WAL. |
| `game_core` | Pure gameplay modules plus the `bevy_ecs` world and systems. |
| `quest_dsl` | Quest language AST, parser, conversion, and related tooling. |
| `server_realms` | One server binary with `auth` and `channel` roles selected by configuration. |

Supporting workspace crates include `locale_import`, `bench_bot`, and the
temporary `mysql_proxy` compatibility adapter used for local parity work.

The runtime target is native Windows with PostgreSQL 18. WSL is retained only
as an on-demand compatibility environment, as recorded in [ADR-0012](documentation/adr/0012-windows-native-runtime-wsl-on-demand.md).
The repository distributes the project-authored Rust implementation and does
not require or distribute third-party server infrastructure or server-side
code.

## Repository map

```text
source/
├── client/      # client source/reference used for compatibility work
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

## Plan and contribution context

- [Documentation hub](documentation/README.md)
- [Live handoff](documentation/progress.md)
- [Gap Registry](documentation/plans/gap-registry.md)
- [Master roadmap](ROADMAP.md)
- [Changelog](CHANGELOG.md)
- [Project rules](AGENTS.md)

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
