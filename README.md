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
| Runtime, login, and entry | ✅ Working and verified | Native Windows runtime with PostgreSQL 18; login → character selection → world → movement verified with an external compatible client (`test` / `1234`). Ports 5432/30001/30003 LISTEN; `source\deploy\win\server_realms.exe` is the release build of HEAD (SHA-256 verified). | The live stack can lag the latest source until the redeploy step runs again. The G0.1b/c/d real-client checks remain operator actions. | [Live handoff](documentation/progress.md); [ADR-0012](documentation/adr/0012-windows-native-runtime-wsl-on-demand.md) |
| Protocol, framing, and authentication | ✅ Working and verified | Byte-oriented codecs (`protocol` crate, 30/30 packets), tokio TCP framing, handshake, and the auth/channel login path. The login chain (LOGIN3 → GD_LOGIN → RESULT_LOGIN → GC_EMPIRE → 449 B) is byte-exact. | This covers the verified compatibility flow, not every legacy packet. The channel stopped handshaking on 2026-08-14 (FIX `entry.rs:74`); the `channel_pg` test helpers were rewired to match. | [Wire reference](documentation/reference/login-flow.md); [AGENTS.md](AGENTS.md) |
| Database and persistence | ✅ Working and verified | PostgreSQL 18.4 on `127.0.0.1:5432` (service `postgresql-metin2`); `tokio-postgres` repositories; WAL durable with `ON CONFLICT DO NOTHING` idempotency; `Batcher::flush()` for one-tx commits; selected ACID paths for items, economy, and safebox. `replay_wal` test runs against the live PG (2/2 green). | Not every legacy domain is persisted; the remaining persistence gates are open. `CHECK gold>=0` is enforced in Rust (`economy.rs:97`); the PG-side `CHECK` constraint is a follow-up. Nightly `pg_dump` is verified by `scripts/restore_drill.ps1`; an off-host copy of the dumps is still an operator action. | [ADR-0008](documentation/adr/0008-data-layer.md); [ADR-0005](documentation/adr/0005-postgresql-cutover-and-legacy-adapter.md); [Backup/restore runbook](documentation/reference/backup-restore.md); [Gap Registry](documentation/plans/gap-registry.md) |
| World, ECS, and movement systems | 🟡 Partial or limited | `bevy_ecs` 0.19 World replaces `MobCache`; components `Vid/Position/Hp/Aggro/Mob/Item/Player`; systems `chase_attack/aggro_detect/patrol` (parity order). `SPAWN_VIEW = 300000` and `DESPAWN_RADIUS = 310000` (per mutation tests). CG_MOVE validates walkability BEFORE `process_move` (reject → stands, no ban). The absolute movement distance is clamped to 6000 units. | Gameplay tuning, broader world behavior, and anti-cheat coverage remain incomplete. The `multi_threaded` World flag stays off until the F5 benchmark. The 250/500/1000 bot ladder is still open. The G0.1b/c/d real-client checks (movement envelope, spawn visibility, boot speed) are committed but unverified at the wire. | [ADR-0010](documentation/adr/0010-domain-boundaries-and-data-ownership.md); [ADR-0011](documentation/adr/0011-anti-hack-model.md); [Gap Registry — G0.1b/c/d](documentation/plans/gap-registry.md) |
| Items and economy | 🟡 Partial or limited | Shop buy/sell at `game_core/src/shop.rs` (parity `shop.cpp:166-180`, `shop_manager.cpp:297-319`); player trade at `game_core/src/trade.rs` (`TradeSession` 12-cap, 2-phase accept, **ONE-tx commit** via `ItemExchange::exchange_mutated` + `Batcher::flush()`); safebox persist; belt; refine basics; partial Dragon Soul handling. The five implementation caps (stack, distance, view, boot speed, gold) are locally verified with mutation-tested verifiers. | **The effective item stack is 200** (`server_realms/src/channel/mod.rs:397-400`); the requested 2000 is blocked by the current wire's BYTE-sized item-count fields. Dragon Soul reward-item creation is not wired (`dragon_soul.rs:40-60`); the 15-cell grid validation is not enforced (`dragon_soul.rs:21-28`). Item-pickup weight is fail-open because no `item_proto.weight` source exists in the classical sources. | [Live handoff](documentation/progress.md); [Gap Registry — G0.1a, G2.7, G2.9](documentation/plans/gap-registry.md) |
| Skills and buffs | 🟡 Partial or limited | `SkillRepo` from `player.skill_proto`; poly evaluator; `skill_damage`; server-timed affects (`process_skill` + `affects_system`); selected families including SPLASH/PARTY/HORSE; grand-master bonus via `kMasterBonusPoly` (`ecs/systems/skill.rs:329-334`); MOV_SPEED/ATT_SPEED/CRITICAL buffs stored+shown. `CG_USE_SKILL` (52) + `GC_AFFECT_ADD/REMOVE` (126/127) are wire-exact. Five stat points per level with no per-stat cap (ADR-0014). | Numeric `CASTING_SPEED` is stored but does not yet change cast timing (G2.4). Passive and quest-granted skills remain limited. Remaining family coverage is partial. | [Live handoff](documentation/progress.md); [ADR-0014](documentation/adr/0014-infinite-stats-five-per-level.md); [Gap Registry — G2.4](documentation/plans/gap-registry.md) |
| Guild, party, and social | 🟡 Partial or limited | Guild basics, grades/comments/ranking, war declaration and score handling; party core actions with LINK/UNLINK emission (`channel/party.rs:581-584`, commit `91b389c`); `WorldStore` exchange; 12 party tests cover invite/answer/exp/parameter/kick/disband/handle_msg. | **Guild war lifecycle is stubbed** (G2.3a): wars start at 0–0 directly, not via `WAIT → ON_WAR → END`. Finish conditions and the scoreboard wire are not yet emitted (G2.3b/c). Marriage uses a stub `is_married_to`; block-mode (`BLOCK_MESSENGER_INVITE`) and observer messenger modes are not yet wired. The party leader +30% item-drop bonus (G2.1a), the leadership-gated party heal (G2.1b), and the leadership-gated 2-attacker cap (G2.1c) are open. Periodic `Update()` of `PartyPeer.hp_percent` (G2.1d) is not wired. | [Live handoff](documentation/progress.md); [Gap Registry — G2.1–G2.3](documentation/plans/gap-registry.md) |
| Quests | 🟡 Partial or limited | The `quest_dsl` converter covers the **194/194-file** corpus (Lua 5.0 dialect, `qc.rs` + `map.rs`, 44 tests green). The runtime at `game_core/src/quest/engine.rs` (859 lines) implements `{quest}.__status` persistence, `wait()/select()` suspension, conditions + a tested action subset, and 6 family proposals = 112/194 files. | The `InputNumber` action and its `CG_QUEST_INPUT_STRING` handler are missing (G2.5). `say_reward`, `send_letter`, `set_quest_state`, `target_vid`, and `affect_*` actions remain pending in the action subset. | [Live handoff](documentation/progress.md); [ADR-0016](documentation/adr/0016-quest-system-dsl-and-runtime.md); [Gap Registry — G2.5](documentation/plans/gap-registry.md) |
| GM commands | 🟡 Partial or limited | Parsing, permission checks (per-command DB check against `common.gmlist`), and the main command subset (warp / item / notice / level) work. EN message on unknown/rejected. | `/transfer` and `/ipurge` only log (no real teleport/purge) (G2.6a/b). `/set`, `/makeguild`, `/priv_empire` are absent from the parse table (G2.6c/d/e). `/mob` accepts only vnum, no name lookup (G2.6f). `/kill` accepts only mob targets, not players (G2.6g). `/goto` accepts only name, not `x y map` (G2.6h). `/view_equip`, `/observer`, `/mount` return "not implemented" (G2.6i/j/k). | [Live handoff](documentation/progress.md); [Gap Registry — G2.6](documentation/plans/gap-registry.md) |
| Events and dungeons | 🟡 Partial or limited | Event scheduling/lifecycle and dungeon `WAIT → START → END` behavior work as pure domain functions. | Raid, OX, three-way war, arena, wedding, and monarch events are deferred by the user (2026-08-21, G2.8a–f). Dungeon instances (`game_core/src/dungeon.rs:1-23`) are pure domain functions; no live party instance flow (G2.11c). | [Live handoff](documentation/progress.md); [Gap Registry — G2.8, G2.11c](documentation/plans/gap-registry.md) |
| Locale and data channel | 🟡 Partial or limited | Locale push/pull works (`GC_LOCALE 140` with chunked envelope, `source/reforge/server_realms/src/channel/locale.rs:55-76`); the 162/163 envelope serves locale data on demand. | A versioned manifest, delta delivery, and `LISTEN/NOTIFY` hot reload are not implemented; data updates still require redeployment (G2.10). ADR-0009 (server-side locale ownership) is accepted; the channel-side implementation is not yet wired. | [ADR-0009](documentation/adr/0009-server-side-locale.md); [ADR-0017](documentation/adr/0017-regional-channels-deferred.md); [Gap Registry — G2.10](documentation/plans/gap-registry.md) |
| Caps and storage | 🟡 Partial or limited | The five caps (item stack 200, absolute move distance 6000, spawn view 300k/310k, boot speed 200, gold 2 000 000 000) are locally verified with mutation-tested verifiers (12 + 13 + 22 + 147 tests). `target/` is now 425 MB (release only), within the 5 GB budget. | The 2000 stack target remains blocked on the wire (G0.1a). Storage gate is closed but the off-host dump copy is still an operator action. The five real-client checks (G0.1a–G0.1d) are committed but unverified at the wire. | [Live handoff](documentation/progress.md); [Gap Registry — G0.1a–G0.1e, G0.2](documentation/plans/gap-registry.md) |
| Verification, documentation, and deployment | ✅ Working and verified | `scripts/verify.ps1` runs `cargo fmt --check` + `cargo test --workspace` + ignored informative leg + `cargo clippy --workspace -- -D warnings` + `git diff --check`. The CI workflow `.github/workflows/docs.yml` runs the same script on every push and every PR, plus `check_docs.ps1` (metadata) and the handoff check (source touched ⇒ `progress.md` / `CHANGELOG.md` / `gap-registry.md` updated). Local run is `OK: verificacion completa`. | The `--ignored` leg is informative; full live-PG execution needs the runbook. The WSL parity leg of G1.18 still needs an operator action. The three `channel_pg` follow-ups (G3.2g/h/i) and the party drain test (G3.2c) are excluded from the gate. | [Gap Registry](documentation/plans/gap-registry.md); [AGENTS.md](AGENTS.md) |
| Standalone Rust client | ⏳ Not started / deferred | — | F7 is deferred outside this repository per ADR-0015; the public repository contains the server only. | [ADR-0015](documentation/adr/0015-rust-only-public-repository.md); [ROADMAP](ROADMAP.md) |

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
