---
Type: Decision
Status: Accepted (2026-08-12)
Audience: Contributors, maintainers
Date: 2026-08-12
Last verified: 2026-08-12
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
| Per-entity **speed envelope** (speed × server time) | Closes the slow-move-accumulate hole per plan §5.7; per-MOVE distance check alone is per-packet | F5 (with walkability) |
| **Walkability** server-side from PG (`world.maps` tile attributes) | Pending in every F5.3 slice (`IsMovablePosition`); PG makes it data-driven and tamper-proof vs pack files | F5 |
| **Floods / rate limits** | Per-connection token buckets for CG_CHAT/CG_ITEM_MOVE/CG_ATTACK/CG_MOVE; legacy has none (the 20 GC_MOVE/tick cap, channel.rs:1811, is S→C only) | F5 |
| **God-mode / buffs** | Server-timed affects (combat HP/cooldowns already server-side; `database::affect` is a stub) | F5 (skills) |
| **Dupe completion** | Foundation DONE: single-writer per region + one-tx Batcher ≤100 ms + idempotent WAL replay + audit same-tx (wal.rs, world.rs; ADR-0008). Pending: items as ACID units (materials → result → gold in one commit, ROADMAP.md:168) + the 2 non-idempotent plain-INSERT paths (`safebox size==1`, `messenger.add` — ROADMAP.md:25) | F5 |
| **Farm bots** | Behavioral telemetry per account (movement/combat/economy patterns over the log schema) — a differentiator the legacy lacks | F5.4/F6 |

### 5. Attack class table (kept updated per slice)

| Attack class | Defense | State | Evidence |
|---|---|---|---|
| Speedhack | Server-clock delta, always on | Implemented | movement.rs:94-104; C++ OFF config.cpp:127 |
| Teleport | Max-distance per MOVE, reject-no-move | Implemented | movement.rs:106-114; C++ commented input_main.cpp:1463 |
| Fake packets / malformed wire | Fixed-size framer table; unknown/`0x00` → close | Implemented | framer.rs:44-47,68-89 |
| Fake timestamps / clock manipulation | Modular delta + tolerance; kick as policy | Decided (this ADR) | movement.rs:95-98 |
| God-mode / fake stats | Server-computed HP/points + cooldowns | Implemented (combat); buffs pending F5 | combat.rs; GC_POINTS from server |
| Dupe / rollback | Single-writer + one-tx batches + idempotent replay | Implemented (foundation); item-ACID pending F5 | wal.rs, world.rs; ROADMAP.md:25 |
| Floods / DoS | Per-connection token buckets (none in legacy) | Pending F5 | channel.rs:1811 (S→C cap only) |
| SQL injection | Domain repos only, prepared statements, no direct SQL at runtime | Implemented | ADR-0008 §2; database crate |
| Movement through walls | Walkability from PG tile attributes | Pending F5 | IsMovablePosition pending |
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
- F5 slices implement the pending controls in order: walkability + speed
  envelope (movement), floods (per-connection), server-timed buffs (skills),
  item-ACID (dupe completion).
- Farm-bot telemetry defines the log-schema requirement — coordinate with the
  `database` log domain at F5.4.
- This table is the living "anti-hack tableado" promised by the plan: every
  slice updates its row with evidence.
