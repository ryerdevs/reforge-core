---
Type: Decision
Status: Accepted (2026-08-27)
Audience: Contributors, maintainers
Date: 2026-08-27
Last verified: 2026-08-27
Supersedes: —
Superseded by: —
---

# ADR-0014: Infinite stats — 5 points per level, no per-stat cap, free reset

## Status

Accepted (2026-08-27). User decision: the stat system should be **better than the
original** — unlimited points per stat, 5 points per level, free redistribution.
The change is contained in the Rust channel/game-core modules only; no schema, wire
or client change.

## Context

- The current implementation replicates the legacy cap-90 model byte-for-byte:
  - **Per-stat cap:** `/stat` clamps the target stat to `STAT_MAX = 90`
    (`server_realms/src/channel/gm.rs:769`, parity `g_iStatusPointSetMaxValue`
    config.cpp:48); the excess is neither applied nor spent (`gm.rs:848`,
    no-op log at `gm.rs:849-855`).
  - **Level-point cap:** level-ups grant **+3 points only while `prev_level <= 90`**
    (`STATUS_POINT_GET_LEVEL_LIMIT` at `session.rs:345`, `level_up_step`
    `session.rs:357-365`). Levels 91–99 award **0 points** — parity
    `ResetPoint`'s `MINMAX(1, iLv, 90) * 3` (char.cpp:5857).
  - **Reset currency:** `/stat-` consumes `player.stat_reset_count`
    (`gm.rs:876-883` spends, `gm.rs:899` decrements; column mapped in
    `database/src/player.rs:57`), so redistribution is a scarce, bounded resource.
- The stat wire path is **chat `/stat`** through `interpret_command`:
  `CG_CHAT` dispatch at `channel/chat.rs:296-301`, command table at
  `channel/gm.rs:262-268`. Client v40999 has **no dedicated stat header**
  (`CG_STAT_ADD` does not exist in the protocol), so the chat-command path is the
  only wire surface available without a client patch.
- The rewrite mandate (AGENTS.md) is "do more with less", but also "better than the
  original" where the user says so: the cap-90 model makes late-game leveling dead
  (91–99 award nothing) and punishes exploratory builds (reset currency).

## Decision

1. **5 points per level, always.** Remove `STATUS_POINT_GET_LEVEL_LIMIT`
   (`session.rs:345`); `level_up_step` grants `+5` on every level-up
   (`session.rs:357-365`). Total at cap 99: 98 level-ups × 5 = **490 points**.
2. **No cap per stat.** Remove `STAT_MAX` (`gm.rs:769`) and the
   `min(STAT_MAX)` clamp (`gm.rs:848`); `/stat` applies the full amount while
   `stat_point` lasts.
3. **Free resets.** `/stat-` (`gm.rs:875-911`) no longer spends or requires
   `stat_reset_count`; the **job floor stays** (initial st/ht/dx/iq per job,
   `gm.rs:884-888`, parity `JobInitialPoints` constants.cpp:6-15) so a stat
   cannot be stripped below the class baseline.
4. **Types already reach.** Stats and points are `i16`, HP/SP derived values
   `i32` (`ecs/components.rs:32`, `ecs/events.rs:30`) — a
   **HT 500 → ~20,000 HP** build fits without widening; worst-case single stat
   (~496) is far inside `i16`.
5. **Linear balance stays.** No diminishing-returns curve; max HP/SP remain
   `f(ht)` / `f(iq)` as today (parity char.cpp:2230-2231). Simpler, understood,
   and easy to curve later if measured (see Consequences).
6. **Wire unchanged.** Allocation/redistribution keep going through chat `/stat`
   (client 40999 has no header; a protocol change would require a client patch).
7. **No retroactive migration.** Existing characters keep their current values;
   the new grant applies from the next level-up onward.

## Alternatives considered

- **Keep the cap-90 parity model** — rejected: the user's requirement is explicitly
  "better than the original"; under parity, levels 91–99 give 0 points and the
  per-stat wall at 90 dead-ends specialist builds.
- **Diminishing returns** — rejected (YAGNI): adds a balance curve with no measured
  demand; linear balance is simpler and the F5 benchmark can justify a curve later
  if the uncapped model proves degenerate.
- **New wire header `CG_STAT_ADD`** — rejected: requires patching and rebuilding the
  client (v40999 fork); chat `/stat` already works end-to-end
  (`gm.rs:262-268`) at zero client/protocol cost.

## Consequences

- **Footprint: 4 files** — `gm.rs` (cap removal), `session.rs`
  (5 points/level), `config.rs` (level-points value + the F5 soft-cap hook),
  and the existing test modules (verifiers). **0 schema / 0 wire / 0 client**
  changes.
- **Verifier tests: 5 cases** — level-up 90→91 grants +5; level-up 98→99 grants
  +5; `/stat` past 90 applies (no clamp); `/stat-` works with
  `stat_reset_count = 0`; job floor still enforced on reset.
- **`stat_reset_count` is deprecated**: no longer consumed by `/stat-`; the column
  and `PlayerRow` field stay for schema stability, documented as unused.
- **Progression ceiling:** ~496 in any single stat (job initial + 490);
  HP at HT 500 ≈ 20,000 — both within existing `i16`/`i32` types.
- **F5 benchmark gate:** if the benchmark (spawn-dinámico ladder, 100–1000 bots)
  shows the uncapped model breaks combat balance at scale, a **soft-cap config
  option** lands in `config.rs` — same pattern as the F5 rates
  (`exp_rate`/`gold_rate`/`drop_rate`, `config.rs:74-78`) — rather than a hard
  constant.
- **Balance stays linear**; item/skill tables untouched. Existing characters keep
  their stats (no retroactive migration, decision 7).