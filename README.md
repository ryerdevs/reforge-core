---
Type: Hub
Status: Current
Audience: All
Last verified: 2026-08-30
---

# reforge-core

`reforge-core` is a server written **from scratch in Rust** whose wire
compatibility surface was reconstructed through reverse engineering. Every
packet, frame, and field was inferred from captured traces and live
observation of an external compatible server and client — **no code was
copied from any other project**. The implementation here is the result of
reading behavior and writing Rust against it.

The repository is a development and test server. The status matrix separates
verified behavior from partial and deferred work; a green row is not a
claim of total parity or production readiness.

## What it is

- A server-authoritative Rust implementation with a byte-oriented compatibility
  boundary at the wire.
- An incremental project: each slice is checked against observed behavior
  before broader coverage is attempted.
- A structural reimplementation focused on observable contracts, durable
  state, and small, testable modules.

## How the wire was reconstructed

The project avoids two failure modes: guessing and copying. Every visible
behavior the server exposes is anchored to one of three sources:

- **Captured packet traces** — raw bytes from a compatible client talking to
  a known-good server, with offsets, lengths, and field semantics inferred
  from the trace and not from any other code.
- **Live observation** — the server is run alongside a compatible client and
  every command's request/response is logged. The wire spec is rewritten from
  the exchange, not from prior memory of the protocol.
- **Behavioral tests** — every contract the implementation claims to honor has
  a focused unit or integration test that fails on a regression. The tests,
  not the implementation, are the source of truth.

The wire reference lives in
[`documentation/reference/login-flow.md`](documentation/reference/login-flow.md);
the architecture decisions (data layer, ECS boundary, runtime, scope) live
in the [ADRs](documentation/adr/). No code is imported, vendored, or
transcribed from any other project.

## Repository boundary

The public repository contains the authored Rust server, the documentation,
the scripts, and the supporting metadata needed to develop and verify it. It
does not contain client source, pack source, runtime client assets, generated
client artifacts, or any other game content.

End-to-end compatibility checks use an external, operator-provided compatible
client. That client and its assets are not distributed by this repository and
are not required for server builds or the ordinary test suite. The standalone
Rust client is deferred outside this repository; see
[ADR-0015](documentation/adr/0015-rust-only-public-repository.md).

**Notice:** This project is not official or affiliated with any rights holder.
Each operator is responsible for the use, modification, deployment, and
content of their instance. This is a neutral project notice, not legal
advice.

## What is verified today

This is the center of the project status. A row may be working while a
related feature remains limited. The [live handoff](documentation/progress.md)
and [gap registry](documentation/plans/gap-registry.md) contain the detailed
state and evidence.

**Legend**

- ✅ Working and verified
- 🟡 Partial or limited
- 🔧 In progress
- ⏳ Not started or deferred

| Area | Status | What works today | What is limited or missing | Evidence / gate |
|---|---|---|---|---|
| Runtime, login, and entry | ✅ Working and verified | Native Windows runtime with PostgreSQL; login, character selection, world entry, and movement are verified with an external compatible client. | The deployed runtime can lag the latest source until the redeployment gate closes. | [Live handoff](documentation/progress.md); [ADR-0012](documentation/adr/0012-windows-native-runtime-wsl-on-demand.md) |
| Protocol, framing, and authentication | ✅ Working and verified | Byte-oriented codecs, framing, handshake, and the verified authentication/channel login path work. | This covers the verified compatibility flow, not every packet or feature. | [Wire reference](documentation/reference/login-flow.md); [Project rules](AGENTS.md) |
| Database and persistence | 🟡 Partial or limited | PostgreSQL repositories, WAL idempotency, batching, and selected ACID item/economy mutations work. | Not every domain is persisted or complete; the remaining persistence gates are open. | [ADR-0008](documentation/adr/0008-data-layer.md); [Gap Registry](documentation/plans/gap-registry.md) |
| World, ECS, and movement systems | 🟡 Partial or limited | The ECS world, dynamic spawning, walkability checks, movement envelope, combat foundations, and selected server-authoritative checks work. | Gameplay tuning, broader world behavior, and anti-cheat coverage remain incomplete; the checks are not an exhaustive anti-cheat system. | [ADR-0010](documentation/adr/0010-domain-boundaries-and-data-ownership.md); [ADR-0011](documentation/adr/0011-anti-hack-model.md) |
| Items and economy | 🟡 Partial or limited | Attributes, sockets, refine behavior, shops, trade, safebox, belt, and the Phase 1 Dragon Soul handling are implemented in selected paths. | Dragon Soul reward-item creation and full grid validation remain; the higher stack target is still blocked. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Skills and buffs | 🟡 Partial or limited | Selected skill families, server-timed affects, splash/horse/party handling, and grand-master behavior are implemented. | Numeric `CASTING_SPEED`, passive and quest-granted skills, and remaining effect coverage are still limited. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Guild, party, and social | 🟡 Partial or limited | Guild basics, grades/comments/ranking, war declaration and score handling, and party core actions with LINK/UNLINK work. | Guild-war lifecycle/finish/scoreboard behavior, leadership rules, marriage, block mode, and observer mode remain. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Quests | 🟡 Partial or limited | The converter covers the 194/194-file corpus, and the runtime implements a tested subset with persistence and suspension. | `input_number`, additional actions, and broader quest content remain. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| GM commands | 🟡 Partial or limited | Parsing, permission checks, and the current command subset work. | Transfer/purge dispatch, targeting forms, and several commands remain incomplete. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Events and dungeons | 🟡 Partial or limited | Event scheduling/lifecycle and dungeon WAIT → START → END behavior work. | Raid, OX, three-way war, arena, wedding, monarch, and dungeon instances remain deferred. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Locale and data channel | 🟡 Partial or limited | Locale push and pull work for the current path. | A versioned manifest, delta delivery, and notification-driven hot reload are not implemented; data updates can require redeployment. | [ADR-0009](documentation/adr/0009-server-side-locale.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Caps and storage | 🟡 Partial or limited | The five implementation caps (stack, distance, view, boot speed, gold) are locally verified with mutation-tested verifiers. | The storage budget and the latest rebuild have not been fully closed in the registry. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Verification, documentation, and deployment | 🔧 In progress | Focused local checks, the current documentation handoff, and current archive navigation exist; the wire is runnable end-to-end against an external compatible client. | The normal and ignored test gates, formatting, documentation CI, and current-binary redeployment still need closure. | [Gap Registry](documentation/plans/gap-registry.md); [ROADMAP](ROADMAP.md) |
| Gameplay and content | 🔧 In progress | The implemented slices listed above provide a usable development/test path. | Remaining gameplay, social, quest, GM, data-channel, weight-data, and deferred-content gaps remain open. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Hygiene and test debt | 🔧 In progress | Mutation-tested focused work is present for the current cap lanes and most gameplay systems. | Stale comments and the policy for ignored or flaky tests still need execution and verification. | [Live handoff](documentation/progress.md); [Gap Registry](documentation/plans/gap-registry.md) |
| Standalone Rust client | ⏳ Not started or deferred | The external compatible client is used only for server verification. | The standalone Rust client is deferred outside this repository and has not started. | [ADR-0015](documentation/adr/0015-rust-only-public-repository.md); [ROADMAP](ROADMAP.md) |

## Current limitations

- This is not a claim of total parity, production readiness, or complete
  anti-cheat coverage.
- The verified compatibility flow covers the rows marked green above; it is
  not a complete wire spec.
- The safe stack cap is enforced at the current wire size; the higher target
  is blocked by the current wire's item-count field size.
- The cap, distance, view, and boot-speed lanes are implemented and locally
  verified, but each remains open until its remaining client or wire checks
  are complete.
- The protocol and database cover selected compatibility slices rather than
  every packet and domain.
- Data-channel manifest, delta, and notification-driven hot reload are still
  future work.
- The current benchmark evidence supports test runs only; it is not a player
  capacity promise.
- The standalone Rust client rewrite is deferred outside this repository and
  has not started.

## Next wave

1. Close the remaining real-client or wire checks for the current cap lanes,
   while keeping the safe stack cap in place.
2. Run the complete verification gates (format, normal suite, ignored live-PG
   suite, clippy, diff check), reconcile documentation, and redeploy the
   current Windows binary.
3. Take selected gameplay and content gaps, with hygiene and test debt
   handled alongside them.
4. Keep the standalone client work outside this repository until a separate
   project is justified and has its own decision record.

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

The stack is for local development and verification. Starting it does not
imply that every subsystem in the status matrix is complete.

To reproduce the real-client smoke path, provide a compatible client through a
separate, properly licensed installation. This repository does not build,
repack, or distribute that client.

## Contributing

- Read [AGENTS.md](AGENTS.md) and the [documentation hub](documentation/README.md)
  before making a change.
- Keep changes focused, add or update tests for behavior, and preserve a clear
  evidence path.
- Record architectural choices in the canonical ADR location before
  implementing them; use
  [ADR-0010](documentation/adr/0010-domain-boundaries-and-data-ownership.md)
  as the format and context example.
- Update the relevant canonical documentation, [ROADMAP](ROADMAP.md), and
  [CHANGELOG.md](CHANGELOG.md) when project knowledge changes.
- Run `scripts\verify.ps1` when the change is ready for review. Do not describe
  a partial subsystem as complete until its listed gate is closed.
- Keep client source, pack source, client assets, and generated client
  artifacts outside this repository; see
  [ADR-0015](documentation/adr/0015-rust-only-public-repository.md).

The claims in this README were last checked on **2026-08-30** against the
project instructions and the [live handoff](documentation/progress.md).
