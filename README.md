---
Type: Hub
Status: Current
Audience: All
Last verified: 2026-08-30
---

# reforge-core

`reforge-core` is an independent reimplementation of a server compatible with
a classic East Asian MMORPG. It is written from scratch in Rust through reverse
engineering, behavioral analysis, packet and protocol analysis, compatibility
fixtures, and tests. It is a development and test server: the matrix below
separates verified behavior from partial and deferred work.

## What it is

- A server-authoritative Rust implementation with a byte-oriented compatibility
  boundary.
- An incremental project: each slice is checked against observed behavior
  before broader coverage is attempted.
- A structural reimplementation focused on observable contracts, durable state,
  and small, testable modules.

## Repository boundary

The public repository contains the authored Rust server and the documentation,
scripts, and supporting metadata needed to develop and verify it. It does not
contain client source, pack source or assets, generated client binaries, or
other game content.

End-to-end compatibility checks use an external, operator-provided compatible
client. That client and its assets are not distributed by this repository and
are not required for server builds or the ordinary test suite. The standalone
Rust client is F7 work deferred outside this repository; see [ADR-0015](documentation/adr/0015-rust-only-public-repository.md).

**Notice:** This project is not official or affiliated with any rights holder.
Each operator is responsible for the use, modification, deployment, and content
of their instance. This is a neutral project notice, not legal advice.

## Methodology

- **Behavioral reverse engineering:** record inputs, outputs, state transitions,
  timing, and failure behavior.
- **Packet and protocol analysis:** measure headers, framing, lengths, encoding,
  and wire-state transitions. The current login contract is recorded in the
  [wire reference](documentation/reference/login-flow.md).
- **Compatibility fixtures:** preserve observed bytes and outcomes as fixtures
  and regression cases.
- **Tests and verifiers:** use focused unit, integration, mutation, and runtime
  checks to distinguish a working slice from an unverified assumption.
- **Written decisions:** record architecture boundaries and migration choices
  in the [project documentation](documentation/README.md) before expanding the
  implementation.

## Server status at a glance

This is the center of the project status. A row may be working while a related
feature remains limited; a green row is not a claim of total parity or
production readiness. The [live handoff](documentation/progress.md) and
[gap registry](documentation/plans/gap-registry.md) contain the detailed state
and evidence.

**Legend**

- ✅ Working and verified
- 🟡 Partial or limited
- 🔧 In progress
- ⏳ Not started or deferred

| Area | Status | What works today | What is limited or missing | Evidence / gate |
|---|---|---|---|---|
| Runtime, login, and world entry | ✅ Working and verified | Native Windows runtime with PostgreSQL; login, character selection, world entry, and movement are verified with an external compatible client. | The deployed runtime can lag the latest source until G1.5 redeployment is complete. | [Live handoff](documentation/progress.md); [ADR-0012](documentation/adr/0012-windows-native-runtime-wsl-on-demand.md) |
| Protocol, framing, and authentication | ✅ Working and verified | Byte-oriented codecs, framing, handshake, and the verified authentication/channel login path work. | This covers the verified compatibility flow, not every packet or feature. | [Wire reference](documentation/reference/login-flow.md); [Project rules](AGENTS.md) |
| Database and persistence | 🟡 Partial or limited | PostgreSQL repositories, WAL idempotency, batching, and selected ACID item/economy mutations work. | Not every domain is persisted or complete, and the remaining persistence gates are open. | [ADR-0008](documentation/adr/0008-data-layer.md); [Gap Registry](documentation/plans/gap-registry.md) |
| G0.1a — item stack cap | 🟡 Partial or limited | The effective cap is **200**. Channel and GM paths share it, and entry serialization rejects counts above it instead of silently wrapping them. | The requested **2000** is blocked by BYTE-sized item-count fields on the wire. Reaching it requires a coordinated `u16` client/protocol migration and a real-client stack check above 200. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| G0.1b — movement distance | 🟡 Partial or limited | The inclusive 6000-unit mounted and unmounted limit is preserved; widened `i128` arithmetic and 12 focused tests were verified locally. | Oracle Gate and the real-client movement check are still pending. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| G0.1c — spawn materialization | 🟡 Partial or limited | The retained `SPAWN_VIEW = 300000` and `DESPAWN_RADIUS = 310000` values and predicates are covered by boundary/hysteresis checks; 13 focused tests passed locally. | Oracle Gate, the benchmark ladder, and the real-client visibility check are still pending. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| G0.1d — boot movement speed | 🟡 Partial or limited | The cap remains **200** before BYTE serialization; wide/saturating accumulation and capped ADD/UPDATE fields passed 22 focused tests, including the mutation baseline. | The real-client equipped-boot check and Oracle Gate are still pending. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| G0.1e — gold | 🟡 Partial or limited | `GOLD_MAX` remains **2,000,000,000**; checked bounds cover economy, shop, and channel consumers. The focused local run passed **147 tests, 0 failed, 4 ignored**. | Oracle Gate and the final consistent economic/wire checks are still pending. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| World, ECS, and movement systems | 🟡 Partial or limited | The ECS world, dynamic spawning, walkability checks, movement envelope, combat foundations, and selected server-authoritative checks work. | Gameplay tuning, broader world behavior, and anti-cheat coverage remain incomplete; the checks are not an exhaustive anti-cheat system. | [ADR-0010](documentation/adr/0010-domain-boundaries-and-data-ownership.md); [ADR-0011](documentation/adr/0011-anti-hack-model.md) |
| Items and economy beyond G0 | 🟡 Partial or limited | Attributes, sockets, refine behavior, shops, trade, safebox, belt, and Phase 1 Dragon Soul handling are implemented in selected paths. | Dragon Soul reward-item creation and full grid validation remain; the 2000 stack target is still blocked by G0.1a. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Skills and buffs | 🟡 Partial or limited | Selected skill families, server-timed affects, splash/horse/party handling, and grand-master behavior are implemented. | Numeric `CASTING_SPEED`, passive and quest-granted skills, and remaining effect coverage are still limited. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Guild, party, and social | 🟡 Partial or limited | Guild basics, grades/comments/ranking, war declaration and score handling, and party core actions with LINK/UNLINK work. | Guild-war lifecycle/finish/scoreboard behavior, leadership rules, marriage, block mode, and observer mode remain. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Quests | 🟡 Partial or limited | The converter covers the 194/194-file corpus, and the runtime implements a tested subset with persistence and suspension. | `input_number`, additional actions, and broader quest content remain. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| GM commands | 🟡 Partial or limited | Parsing, permission checks, and the current command subset work. | Transfer/purge dispatch, targeting forms, and several commands remain incomplete. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Events and dungeons | 🟡 Partial or limited | Event scheduling/lifecycle and dungeon WAIT → START → END behavior work. | Raid, OX, three-way war, arena, wedding, monarch, and dungeon instances remain deferred. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Locale and data channel | 🟡 Partial or limited | Locale push and pull work for the current path. | A versioned manifest, delta delivery, and notification-driven hot reload are not implemented; data updates can require redeployment. | [ADR-0009](documentation/adr/0009-server-side-locale.md); [Gap Registry](documentation/plans/gap-registry.md) |
| G0.2 — storage operations | 🟡 Partial or limited | The PostgreSQL backup cadence is active and some target cleanup was verified on **2026-08-29**. | The storage gate remains open until the target budget and post-cleanup backup check are recorded in the [Gap Registry](documentation/plans/gap-registry.md). | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| G1 — verification, documentation, and deployment | 🔧 In progress | Focused local checks, the current documentation handoff, and current archive navigation exist. | The normal and ignored test gates, formatting, documentation CI, and current-binary redeployment still need closure; the immutable-history metadata decision is recorded and closed. | [Gap Registry](documentation/plans/gap-registry.md); [ROADMAP](ROADMAP.md) |
| G2 — gameplay and content | 🔧 In progress | The implemented slices listed above provide a usable development/test path. | Remaining gameplay, social, quest, GM, data-channel, weight-data, and deferred-content gaps remain open. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| G3 — hygiene and test debt | 🔧 In progress | Mutation-tested focused work is present for the current cap lanes. | Stale comments and the policy for ignored or flaky tests still need execution and verification. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| F7 — Rust client rewrite | ⏳ Not started or deferred | The external compatible client is used only for server verification. | The standalone Rust client is deferred outside this repository and has not started. | [ADR-0015](documentation/adr/0015-rust-only-public-repository.md); [ROADMAP](ROADMAP.md) |

## Current limitations

- This is not a claim of total parity, production readiness, or complete
  anti-cheat coverage.
- G0.1a is intentionally safe at 200; 2000 remains blocked until the coordinated
  `u16` client/protocol migration and real-client verification.
- G0.1b–G0.1e are implemented and locally verified, but each remains open until
  its Oracle Gate and remaining client or wire checks are complete.
- The protocol and database cover selected compatibility slices rather than
  every legacy packet and domain.
- Data-channel manifest, delta, and notification-driven hot reload are still
  future work.
- The current benchmark evidence supports test runs only; it is not a player
  capacity promise.
- The Rust client rewrite is F7 work deferred outside this repository and has not started.

## Next wave

1. Complete the Oracle Gate and remaining real-client or wire checks for
   **G0.1b–G0.1e**, while retaining the safe G0.1a cap of 200.
2. Execute **G1**: run the complete verification gates, format the workspace,
   reconcile documentation and links, update the changelog, and redeploy the
   current Windows binary.
3. Take the selected **G2** gameplay and content gaps, with G3 hygiene and test
   debt handled alongside them.
4. Keep **F7** outside this repository until a separate client project is
   justified by the server boundary and has its own decision record.

## Architecture

The workspace is split by responsibility and keeps the compatibility boundary
small:

| Component | Responsibility |
|---|---|
| `protocol` | Byte-oriented client/server packets and compatibility codecs. |
| `network` | Tokio TCP transport, framing, handshake, and authentication support. |
| `database` | PostgreSQL access, domain repositories, batching, and WAL. |
| `game_core` | Pure gameplay modules plus the `bevy_ecs` world and systems. |
| `quest_dsl` | Quest language AST, parser, conversion, and related tooling. |
| `server_realms` | One server binary with `auth` and `channel` roles selected by configuration. |

Architecture and data-ownership decisions are recorded in the [architecture
decisions](documentation/README.md), including the data layer, ECS boundary,
runtime, and client plan.

## Repository map

```text
source/
├── reforge/     # Rust workspace
│   ├── protocol/
│   ├── network/
│   ├── database/
│   ├── game_core/
│   ├── quest_dsl/
│   └── server_realms/
├── tools/       # supporting data and protocol tools
└── deploy/      # local runtime artifacts
documentation/   # hub, plans, ADRs, references, and history
scripts/         # local runtime and verification operations
ROADMAP.md       # master plan
CHANGELOG.md     # chronological evidence record
AGENTS.md        # project rules and verified facts
```

The compatible client used for real-client checks is supplied separately by the
operator; it is intentionally absent from this map and from the repository.

## Build and run

Build and test the Rust workspace from the repository root:

```powershell
Set-Location source\reforge
cargo build --workspace
cargo test --workspace
Set-Location ..\..
```

The native Windows runtime uses the local PostgreSQL setup described in
[ADR-0012](documentation/adr/0012-windows-native-runtime-wsl-on-demand.md).
Run lifecycle operations separately:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\start_win.ps1
powershell -ExecutionPolicy Bypass -File scripts\status.ps1
powershell -ExecutionPolicy Bypass -File scripts\stop_win.ps1
```

For the definition-of-done check:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\verify.ps1
```

The stack is for local development and verification. Starting it does not imply
that every subsystem in the status matrix is complete.

To reproduce the real-client smoke path, provide a compatible client through a
separate, properly licensed installation. This repository does not build,
repack, or distribute that client.

## Contributing

- Read [AGENTS.md](AGENTS.md) and the [documentation hub](documentation/README.md)
  before making a change.
- Keep changes focused, add or update tests for behavior, and preserve a clear
  evidence path.
- Record architectural choices in the canonical ADR location before implementing
  them; use [ADR-0010](documentation/adr/0010-domain-boundaries-and-data-ownership.md)
  as the format and context example.
- Update the relevant canonical documentation, [ROADMAP](ROADMAP.md), and
  [CHANGELOG](CHANGELOG.md) when project knowledge changes.
- Run `scripts\verify.ps1` when the change is ready for review. Do not describe a
  partial subsystem as complete until its listed gate is closed.
- Keep client source, pack source, client assets, and generated client artifacts
  outside this repository; see [ADR-0015](documentation/adr/0015-rust-only-public-repository.md).

The claims in this README were last checked on **2026-08-30** against the
project instructions and the [live handoff](documentation/progress.md).
