---
Type: Decision
Status: Accepted (2026-08-12)
Audience: Contributors, maintainers
Date: 2026-08-12
Last verified: 2026-08-13
Supersedes: ROADMAP.md:158 (the ECS commitment of Phase 4)
Superseded by: —
---

# ADR-0010: Domain boundaries and data ownership

## Status

Accepted (2026-08-12). Ratifies the architecture that F4–F5.3 already implements
(verified 2026-08-12) and codifies the governing translator-vs-core principle
confirmed by the user on 2026-08-12: *contract parity lives inside the legacy
translator; everything we develop is new, studied, scalable, maintainable code.*
ECS entry re-decided the same day by the user: **bevy_ecs standalone is adopted
now** (see §2) — not benchmark-gated as originally proposed.

## Context

- The plan promised "minimal Entity core + ECS systems (bevy_ecs standalone) —
  NEVER port char.cpp as a single class" (ROADMAP.md:158). The implementation
  took a different, working shape (F4 milestone MET 2026-08-11; F5.3 slices
  1–17 green): **pure-function domain modules over row structs + per-connection
  session state + a durable WorldStore**. There is no Entity struct and no ECS
  in the workspace (`source/reforge/Cargo.toml` deps: tokio, tokio-postgres,
  uuid only).
- The legacy `char.cpp` god object (~815 graph edges) is the primary
  anti-pattern this rewrite exists to remove; the §3.3 audit lists P1 "global
  tick with allocs, O(all entities)" (char_manager.cpp:641) → "regions +
  parallel systems" and the 1,000+ players/instance ceiling
  (server-rewrite.md:116,125).
- The legacy C++ also ships data-model quirks (1-based slots, `180+wear` cells,
  `part_main` residue, `ITEM_ID_RANGE`, `safebox size==1 → INSERT`,
  `quest lValue==0 → DELETE`, default IP `0.0.0.0`) that the frozen client and
  the legacy DB expect. Those must not leak into the domain model.

## Decision

### 1. Realm architecture (accepted)

Four layers — pure logic, ECS world state, tokio per-connection, durable store:

- **Pure domain modules** — `game_core::combat` (combat.rs), `game_core::ai` (ai.rs),
  `game_core::movement` (movement.rs), `game_core::packets` (packets.rs),
  `game_core::npc` (npc.rs): explicit inputs/outputs, no hidden state, unit-testable
  (371 workspace tests, verified 2026-08-12). Called from systems; the formulas
  stay pure so the parity tests keep passing unchanged.
- **ECS world state** — `bevy_ecs` standalone `World` (components: Position,
  Hp, Aggro, Mob, Item…): replaces `Arc<Mutex<MobCache>>` (channel.rs:81);
  systems scheduled on a tick (AI 500 ms, movement, combat, drops) with
  parallel query iteration (SoA storage). `default-features = false` (no
  bevy_reflect).
- **Per-connection orchestration** — `server_realms::channel::handle_connection`
  (channel.rs:89-93): one tokio task per connection owning its session state;
  player intents flow to the `World` via mpsc (Veloren pattern), GC packets
  flow back through the connection's outbound queue.
- **Persistence** — `WorldStore` (world.rs): Batcher ≤100 ms + WalSink
  (uuidv7, durable-first, replay once per process via `OnceLock`, idempotent,
  audit in the same tx) + fail-fast on PG connection (world.rs:39).

### 2. ECS adoption — bevy_ecs standalone, decided NOW (user decision 2026-08-12)

The original benchmark gate is superseded. The user decided adoption today:

- **Domain fact:** Metin2 is a mob-farming game — dense mob simulation is the
  core requirement (real data: 145,876 spawns, map 41 = 10,026). The legacy P1
  "global tick with allocs, O(all entities)" (char_manager.cpp:641) and the
  single `Arc<Mutex<MobCache>>` are exactly the contended shapes ECS solves
  (SoA + parallel queries + no per-entity allocs).
- **Solo-dev maintenance:** the ecosystem maintains archetypes/queries/change
  detection; a hand-rolled store would grow into a mini-ECS maintained by one
  person. bevy_ecs standalone is proven for game servers (Veloren).
- **One paradigm:** the F7 client is bevy + Slint (user decision 2026-08-12,
  replacing the wgpu-from-scratch plan) — the same ecosystem on both sides.

The F5 benchmark (N bots × N regions) remains the **validation** of the choice
— it must sustain 1,000+ players/instance with ≥2–5× the C++ per-tick CPU
headroom and AI-tick under 500 ms — but it is no longer the gate for entry.

### 3. Data ownership

| Class | Data | Rule |
|---|---|---|
| **Volatile** | position, HP, MP, riding, transient aggro | Owned by session/AI state; persisted **event-driven** through Batcher+WAL (kill → durable save, potion → upsert+save, equip → upsert — F5.3 slices 4/11/13). This ADR **amends ADR-0008 §5** ("saved every 30 s + logout" is history) |
| **Durable** | items, exp, gold, quest state, safebox, messenger, account | ACID transactions to PostgreSQL via domain repos; WAL local + idempotent replay (`ON CONFLICT DO NOTHING`) as the crash guarantee |
| **Derived** | parts (`equipped_parts` from equipped items + row appearance base), stats (att_grade, def_grade, max points), speeds | Computed on demand, **never persisted**. `part_main` is legacy residue (D2) |

### 4. Governing boundary — translator vs core (user principle, codified)

- **Legacy quirks live ONLY in the translator layer:** `protocol` byte-exact
  codecs (the wire contract), `protocol::legacy` (ADR-0006: PanamaPack
  151/289B, hybrid-crypt 152/153), `mysql_proxy` (MySQL wire v10 + SQL dialect
  translation: `CAST(x AS unsigned)`→bigint, bytea fix — translate.rs:30,215),
  and the wire-facing data mapping in `game_core::packets` (cell↔slot).
- **The core is new design:** clean arithmetic, config-driven constants, no
  inherited truncation quirks. Where observable output parity with the C++ is
  still required during the strangler window (damage numbers, ranges), parity
  is enforced by tests/harness and the few shaping constants are
  **compatibility constants with an expiry** — config-driven, not formulas
  (SPEEDHACK_LIMIT_BONUS=80, combat.rs:185 → config; signed clock wrap →
  ADR-0011; full formula redesign → F6 balance ADR with the side-by-side
  harness measuring each delta).
- Anything that exists **because** the frozen client or the legacy DB expects
  it is debt of the boundary, not of the domain model.

### 5. Wire debt — explicit inventory with F7 removal plan

Mapped at the boundary, absent from the domain model, deleted wholesale at F7
(ADR-0006 deletion pattern; inventory mirrored in
`docs/reference/protocol/legacy-compatibility.md`):

| # | Quirk | Where it lives today |
|---|---|---|
| D1 | Slots 1-based: `find_equip_cell` slot order 0..11 by C++ else-if (item.cpp:568-592; F5.3 slice 16) | packets.rs mapping; domain slots are 0-based |
| D2 | Equipment cells = `180 + wear` (packets.rs:48-49; length.h:827); `part_main` absent from load (parts derived; row `part_base` = appearance base; create SQL writes `part_main = 0` — translate.rs:1920) | packets.rs + player schema |
| D3 | Item ids from `ITEM_ID_RANGE` 100M-200M assigned by the game (item.rs:48,216; channel.rs:891) | channel/item repos |
| D4 | `safebox size == 1 → INSERT` (safebox.rs:108; C++ first-page parity) | safebox repo |
| D5 | Quest `lValue == 0 → DELETE` (quest.rs:40,85; ClientManager.cpp:577-579) | quest repo |
| D6 | Default IP `0.0.0.0` in player rows (player.rs:269,446) | player repo |

## Alternatives considered

- **Pure modules + per-connection state + WorldStore without ECS** (the F4–F5.3
  shape): rejected as the FINAL design — it works today (single-player
  sessions, map 41), but the single `Arc<Mutex<MobCache>>` serializes the AI
  tick and every world read, and the domain is mob-farming (dense mob
  simulation is the core requirement — 145,876 spawns imported). ECS (SoA +
  parallel queries) is adopted instead (§2).
- **Actor model (regions as message-passing actors):** rejected — the legacy
  single-writer property is preserved and elevated ("single-writer per region",
  server-rewrite.md:106); tokio tasks + the bevy World (single-writer access
  from the region task) already provide isolation; actors add a messaging layer
  with no measured benefit.
- **Ported god-object (char.cpp as one class):** rejected — explicit plan
  prohibition (ROADMAP.md:158); the god object is the anti-pattern the rewrite
  removes.

## Consequences

- Skills, multicast and party (next F5 slices) build on this ADR: world state
  lives in the bevy World (systems over the pure domain modules) +
  per-connection sessions + WorldStore; the god-object pattern is not
  introduced at any point.
- ~~The next implementation slice is the ECS adoption: `MobCache` → World
  components/systems (Position, Hp, Aggro, Mob, Item), player intents via mpsc
  (Veloren pattern), the 371 existing tests stay green.~~ — **DONE (2026-08-13,
  39th part):** `MobCache` → bevy World landed in `game_core/src/ecs.rs` (986
  lines): components Vid/Position/Hp/Aggro/Mob/Item/Player; resources Tick,
  Rand, NpcOutbox, SpawnCache; systems `chase_attack`/`aggro_detect`/`patrol`
  (chained, parity order); `WorldSim` wrapper (resolve_spawns/spawn_npcs/
  damage_npc/spawn_item/update); channel.rs refactored (AI tick →
  `world.update`; player intents sync state into the World). Workspace **359
  passed / 0 failed**, clippy clean, release green, deployed. **Accepted
  deviations** (implementer-documented, accepted): `multi_threaded` not enabled
  yet (one-line toggle at the F5 benchmark); SpawnCache stays
  `Arc<Mutex<>>` as a World resource (cross-connection PG-row cache, not world
  state); the World is per-connection for now (channel-level shared World = the
  spawn-dinámico slice, IN PROGRESS); armor computed at entry/equip/unequip
  instead of per-tick (same values, zero per-tick PG round-trips); the mpsc
  player-intent channel is deferred with the spawn-dinámico slice (ecs.rs:34).
- ADR-0008 §5 is amended (volatile = event-driven save via Batcher+WAL).
- ROADMAP.md:158 and the reforge README ("realm ... ECS (F4+)") must be
  updated in the staleness sweep (done 2026-08-12).
- The D1–D6 inventory is the F7 deletion checklist (extend
  legacy-compatibility.md).
- Compatibility constants get an expiry note; the balance redesign is an F6
  ADR gated on the parity harness.
