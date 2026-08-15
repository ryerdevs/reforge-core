---
Type: Decision
Status: Accepted (2026-08-12)
Audience: Contributors, maintainers
Date: 2026-08-12
Last verified: 2026-08-13
Supersedes: —
Superseded by: —
---

# ADR-0011: Anti-hack model

## Status

Accepted (2026-08-12). Ratifies the controls already implemented (verified
2026-08-12 with file:line), decides the signed-clock-wrap question, and sets
the pending roadmap. User principle (2026-08-12): "as an MMORPG we are highly
hackable" — anti-hack is a first-class property of the core, not a checkbox.

## Context

- The plan promises server-authoritative + integrated anti-hack (ROADMAP.md:46;
  plan §6, §5.7). Audit §3.3 P0: "Movement without distance validation
  (`ENABLE_TP_SPEED_CHECK` commented out)" is a legacy decision NOT inherited.
- The legacy C++ shipped its own checks **disabled by default**:
  `gHackCheckEnable = false` (config.cpp:127; checks early-return in
  battle.cpp:810 and input_main.cpp:1308) and `ENABLE_TP_SPEED_CHECK` commented
  out (input_main.cpp:1463-1464).
- The rewrite already enforces several controls always-on; this ADR makes them
  policy and defines the rest.

## Decision

### 1. Invariant: server-authoritative, zero client trust

The client sends intents; the server computes facts — positions, HP, damage,
cooldowns, drops, quests. The client is a view. No game-state-affecting input
is accepted without server validation.

### 2. Implemented controls (ratified)

| Control | Where | Divergence from C++ |
|---|---|---|
| Timer speedhack check, always on (`iDelta` vs server delta; SlowTimer/FastTimer → kick) | movement.rs:94-104 | C++ default OFF (`gHackCheckEnable=false`, config.cpp:127; input_main.cpp:1308) |
| Anti-teleport: per-MOVE max distance (2500/6000 units), reject without updating position | movement.rs:106-114 | `ENABLE_TP_SPEED_CHECK` commented in C++ (input_main.cpp:1463-1464) |
| Unknown header / `0x00` → connection close | framer.rs:44-47 | C++ consumes `0x00` as 1-byte no-op (input.cpp:75-76) — deliberate divergence |
| DB down → deterministic fail-fast | world.rs:39 (`WorldStore::new` validates PG) | C++ hangs silently |
| Inactivity timeout (per-read, reset on ANY packet incl. keepalives) instead of absolute | channel.rs:97-105,119-129 | C++ has no global timeout (channel.rs:99); the earlier absolute 15 s killed playing clients |
| Server-clock cooldowns (1250 ms interval enforced server-side) | combat.rs handle_attack (slice 17; tests :637-661) | Client cannot attack faster than the server allows |
| **Speed envelope** — per-MOVE `speed × server time × 1.20` tolerance, inert without an anchor | movement.rs (`PlayerMotion.speed` 300 u/s default; `MoveError::ExceedsEnvelope` :53-67; envelope = speed×(dt+100ms)/1000×1.20) — **IMPLEMENTED 2026-08-13 (40th part); TUNED 2026-08-13 (45th part, real-client evidence):** tolerance 1.2→**1.5** (`ENVELOPE_TOLERANCE` movement.rs:95), lag 100→**250 ms** (`ENVELOPE_LAG_MS` movement.rs:96-97), **Δt = max(client clock, server clock)** — the real client sends MOVEs in bursts and the server-only Δt rejected legitimate walking; the real pattern now fits with margin while the 2500 cap + C++ timers keep the anti-cheat intact (sustained speedhack >450 u/s still bounded) | C++ has no per-MOVE envelope (per-packet distance only) |
| **Walkability** — server-side from decoded map attributes; **re-scoped 2026-08-13 (45th part, real-client evidence):** the `server_attr` parse was CORRECT — the village source cells are legitimate ATTR_BLOCK, so the pre-move reject gate froze the player; the gate is now **anti-teleport reinforcement** (normal steps accepted, jumps onto blocked terrain rejected) | map.rs (`MapStore`, server_attr LZO1X; real map 41: 2,176,848 blocked + 1,076,378 water cells; units→cell = (x%6400)/50) + channel movement — **IMPLEMENTED 2026-08-13 (40th part)** | C++ reads pack/client-side data; PG `world.maps` remains a later data source |

### 3. Signed clock wrap — decision

`movement.rs:95-98` replicates the C++ cast `(int)(dwCurTime - dwTime)`
(u32→i32): deltas beyond ±2³¹ flip sign, and a client with a skewed/manipulated
clock can receive a spurious SlowTimer/FastTimer kick.

**Decision:** replace the cast with a **modular difference with tolerance** —
correct delta for |Δ| < 2³¹; the SlowTimer/FastTimer kick remains an explicit
anti-cheat policy (impossible clock deltas → kick), not an artifact of the
cast. Documented deliberate divergence (0x00 pattern), with a regression test
for clock-bias ±2³¹.

### 4. Pending controls — proposal and phase

| Control | Proposal | Phase |
|---|---|---|
| ~~Per-entity **speed envelope** (speed × server time)~~ — **IMPLEMENTED 2026-08-13 (40th part):** envelope = speed×(dt+100ms)/1000×1.20, `ExceedsEnvelope` movement.rs:53-67, inert without anchor | Closes the slow-move-accumulate hole per plan §5.7; per-MOVE distance check alone is per-packet | DONE (with walkability) |
| ~~**Walkability** server-side~~ — **IMPLEMENTED 2026-08-13 (40th part):** `MapStore` from decoded `server_attr` LZO1X (map 41: 2,176,848 blocked + 1,076,378 water cells); CG_MOVE validates before `process_move` (reject → stands, no ban) | Was pending in every F5.3 slice (`IsMovablePosition`); map-file parse makes it data-driven and tamper-proof vs pack files; PG `world.maps` remains a later source | DONE; follow-ups: path-line sampling (diagonal corner-cut), N-violations auto-ban (config), warp re-anchor |
| **Floods / rate limits** | Per-connection token buckets for CG_CHAT/CG_ITEM_MOVE/CG_ATTACK/CG_MOVE; legacy has none (the 20 GC_MOVE/tick cap, channel.rs:1811, is S→C only) | F5 |
| **God-mode / buffs** | Server-timed affects — **IMPLEMENTED 2026-08-13 (41st part):** `Affects` component + `affects_system` (ecs.rs:209,870), `SkillTable`/`process_skill`, `GC_AFFECT_ADD`/`GC_AFFECT_REMOVE` (channel.rs:1958-2002). **Residual GAPs:** MOV_SPEED/ATT_SPEED/CRITICAL buffs stored+shown but numeric application pending; SPLASH/PARTY/HORSE families; quest-granted/passive skills; `database::affect` stub | F5 (skills — GAPs next) |
| **Dupe completion** — **items as ACID units DONE 2026-08-13 (40th part):** `Batcher::flush()` (wal.rs:398) + `ItemExchange::exchange_mutated()` materials→result→gold in one tx (item.rs:233; proven vs real PG: 4 audit rows same `applied_at`) — **the 2 non-idempotent plain-INSERT paths DONE (39th part):** `ON CONFLICT DO NOTHING` (safebox.rs, messenger.rs); `replay_wal` PG test UN-GATED 2/2 | Foundation (single-writer + one-tx Batcher ≤100 ms + idempotent WAL replay + audit same-tx, ADR-0008) + the listed items | DONE (foundation + listed items) |
| **Farm bots** | Behavioral telemetry per account (movement/combat/economy patterns over the log schema) — a differentiator the legacy lacks | F5.4/F6 |

### 5. Attack class table (kept updated per slice)

| Attack class | Defense | State | Evidence |
|---|---|---|---|
| Speedhack | Server-clock delta + per-MOVE speed envelope, always on | Implemented | movement.rs:94-104 (:53-67 envelope); C++ OFF config.cpp:127 |
| Teleport | Max-distance per MOVE, reject-no-move | Implemented | movement.rs:106-114; C++ commented input_main.cpp:1463 |
| Fake packets / malformed wire | Fixed-size framer table; unknown/`0x00` → close | Implemented | framer.rs:44-47,68-89 |
| Fake timestamps / clock manipulation | Modular delta + tolerance; kick as policy | Decided (this ADR) | movement.rs:95-98 |
| God-mode / fake stats | Server-computed HP/points + cooldowns + **server-timed buffs** | Implemented (combat + buffs 2026-08-13; numeric application GAPs next) | combat.rs; ecs.rs:209,870; channel.rs:1958-2002; GC_POINTS from server |
| Dupe / rollback | Single-writer + one-tx batches + idempotent replay + **item-ACID (`exchange_mutated` one tx) + both plain-INSERT paths idempotent** | Implemented (foundation + listed items, 2026-08-13) | wal.rs:398, item.rs:233, safebox.rs/messenger.rs `ON CONFLICT DO NOTHING`; `replay_wal` PG 2/2 |
| Floods / DoS | Per-connection token buckets (none in legacy) | Pending F5 | channel.rs:1811 (S→C cap only) |
| SQL injection | Domain repos only, prepared statements, no direct SQL at runtime | Implemented | ADR-0008 §2; database crate |
| Movement through walls | **Walkability from decoded server_attr maps (`MapStore`)** | **Implemented 2026-08-13 (40th part)** | map.rs; movement.rs:53-67; CG_MOVE validates before `process_move` |
| Farm bots | Behavioral telemetry | Pending F5.4/F6 | plan differentiator |

## Alternatives considered

- **Client-side trust (legacy default):** rejected — the C++ shipped with its
  checks off and the audit rates the TP hole P0; trusting the client violates
  the plan's core invariant (ROADMAP.md:46).
- **EAC-style kernel/anti-cheat client:** rejected — impossible with the frozen
  legacy client (ADR-0007) and disproportionate; the server cannot verify
  client memory; behavior-level detection is the available and sufficient tool.
- **Total server-authoritative (chosen):** all facts server-side, client as
  view; the wire contract stays byte-exact in the translator, so the hardened
  core needs no client cooperation.

## Consequences

- Always-on enforcement (0x00, timers, teleport) is policy, not accident: the
  divergences are listed in `docs/reference/protocol/legacy-compatibility.md`.
- The wrap change is one function + one regression test; kick behavior is
  preserved.
- F5 slices implement the pending controls in order: **walkability + speed
  envelope DONE (2026-08-13, 40th part — map.rs/movement.rs; envelope tuned
  45th part to the real client pattern — see §2)**, floods
  (per-connection), server-timed buffs (skills — **DONE 2026-08-13, 41st part —
  `affects_system`/`SkillTable` live; numeric-application GAPs next**),
  **item-ACID DONE (2026-08-13 — `exchange_mutated` one-tx,
  item.rs:233)**.
- Farm-bot telemetry defines the log-schema requirement — coordinate with the
  `database` log domain at F5.4.
- This table is the living "anti-hack tableado" promised by the plan: every
  slice updates its row with evidence.
