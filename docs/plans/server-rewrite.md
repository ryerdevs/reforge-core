---
Type: Plan
Status: Current
Audience: Contributors, maintainers, reviewers
Last verified: 2026-08-10
---

# Metin2 Server Rewrite in Rust — Canonical Plan

> **Status: Draft v0.3 (canonical).** Consolidated design document for the Rust rewrite. Supersedes the Spanish drafts `2026-08-09-servidor-rust-plan-unico.md` (v0.2, see `../history/2026-08-09-server-rewrite-plan-v0.2.md`) and `2026-08-09-servidor-rust-draft-discusion.md` (v0.1, historical, see `../history/2026-08-09-server-rewrite-draft.md`).
> **Purpose:** a single file with the full design for third-party review. Feedback: §13 «Open questions for reviewers».
> **Update 2026-08-10:** incorporates the approved/recommended new order (canonical PostgreSQL, legacy compatibility adapter, G-PG gate, F2a/F2b split, `protocol::legacy`) — see §2 for the exact status of each item, and ADRs 0005–0007.
> **Update (G-PG design lane, 2026-08-10):** ADR-0005 Accepted; the G-PG spec is closed in §8.2.1 (provision / migration / adapter / harness). Remaining work is the implementation backlog B1–B8 in ADR-0005.

## Document map

| Document | Role |
|---|---|
| **This document** (`docs/plans/server-rewrite.md`) | Canonical design and migration plan |
| [`../reference/protocol/login-flow.md`](../reference/protocol/login-flow.md) | Byte-exact wire spec of the login flow (contract of the `protocol` crate) |
| [`../reference/protocol/legacy-compatibility.md`](../reference/protocol/legacy-compatibility.md) | Legacy wire/pack compatibility boundary: PanamaPack 151, hybrid-crypt 152/153 |
| [`../reference/quests/quest-dsl.md`](../reference/quests/quest-dsl.md) | Quest DSL specification (replaces Lua) |
| [`../decisions/`](../decisions/) (ADRs 0001–0007) | Architecture decision records — the statuses below refer to them |
| `../../ROADMAP.md` | Phase tracker (root; kept in sync by the orchestrator) |
| `../../AGENTS.md` | Project rules and verified facts (root) |
| [`../history/2026-08-09-server-rewrite-draft.md`](../history/2026-08-09-server-rewrite-draft.md) | Superseded Spanish draft v0.1 (read-only) |
| [`../history/2026-08-09-server-rewrite-plan-v0.2.md`](../history/2026-08-09-server-rewrite-plan-v0.2.md) | Superseded Spanish draft v0.2 (read-only) |

---

## Table of contents

1. [Executive summary](#1-executive-summary)
2. [Status of the new order (2026-08-10)](#2-status-of-the-new-order-2026-08-10)
3. [Context: the legacy server and why rewrite it](#3-context-the-legacy-server-and-why-rewrite-it)
4. [Rewrite principles](#4-rewrite-principles)
5. [Target architecture](#5-target-architecture)
6. [Anti-hack model (server-authoritative)](#6-anti-hack-model-server-authoritative)
7. [Technology stack (2026)](#7-technology-stack-2026)
8. [Migration strategy (strangler fig)](#8-migration-strategy-strangler-fig)
9. [What is not ported (deliberate simplifications)](#9-what-is-not-ported-deliberate-simplifications)
10. [Risks and mitigations](#10-risks-and-mitigations)
11. [Decisions taken](#11-decisions-taken)
12. [Quest DSL](#12-quest-dsl)
13. [Open questions for reviewers](#13-open-questions-for-reviewers)
14. [Next steps](#14-next-steps)

---

## 1. Executive summary

Metin2 (2004 MMORPG, Ymir Entertainment) has a monolithic C++ server of ~120k LOC with decades of debt: spaghetti code, client-validated logic (source of most hacks), massive duplication, and 2004-era decisions that are a burden today.

**Proposal:** rewrite **the entire server** in Rust as a **structural redesign** (not a translation): same observable contracts (the real client keeps working during the migration), modern architecture, **server-authoritative** model (the server computes everything; the client only sends intentions), and **PostgreSQL 18** as the transactional safety net.

**Motto: do more with less** — less code, less complexity, fewer dependencies; quality comes from what is necessary.

**Incremental, verifiable replacement** (strangler fig): module by module, with the C++ server as the test oracle until the final cutover. The client is frozen as the contract (with 1–2 additive data packets, see §5.6).

## 2. Status of the new order (2026-08-10)

The following items were recommended in the 2026-08-10 review of the migration order. Each is labeled with its current status; **anything not yet accepted is a proposal and must not be treated as decided.**

| # | Item | Status | Source |
|---|---|---|---|
| 1 | **One canonical PostgreSQL database** for the Rust server (single operational store on the Rust side; no MySQL-backed Rust path) | **Accepted** (ADR-0005, 2026-08-10) | ADR-0001, ADR-0005 |
| 2 | **Temporary legacy compatibility adapter**; no dual-store (a single canonical PostgreSQL; no second operational database); C++ baseline source untouched | **Accepted** (ADR-0005, 2026-08-10; spec §8.2.1c) | ADR-0005 |
| 3 | **Pre-F2 gate G-PG**: PostgreSQL 18 provisioned as the **single canonical store**; schema/data migration groundwork; the C++ baseline operates on the **same PostgreSQL** through the temporary adapter (MariaDB is used only as the migration/export source); verification that C++ login→world→combat still passes during the transition (behavior unchanged through the adapter + data-comparison harness) | **Accepted** (ADR-0005, 2026-08-10; spec closed in §8.2.1; implementation pending) | ADR-0005 |
| 4 | **F2 blocked** until G-PG is implemented (ADR-0005 accepted; F2a unblocks when the backlog B1–B8 is green) | **Resolved** (2026-08-10) | ADR-0005 |
| 5 | **F2 split into F2a/F2b** (F2a = auth slice against PostgreSQL; F2b = first client batch) | **Accepted** (ADR-0005, 2026-08-10) | ADR-0005, §8.2 |
| 6 | **Legacy packets 151/152/153** (PanamaPack, hybrid-crypt) live in `protocol::legacy` and are **deletable** at the new client (F7) | **Proposed** | ADR-0006, `../reference/protocol/legacy-compatibility.md` |
| 7 | **No Rust embedded inside the legacy client** during F0–F6; Slint standalone login/select later (F5), integrated into the new client (F7) | **Accepted** (for the already-agreed boundary) | ADR-0007 |
| 8 | **Minimal dependency policy** — YAGNI; stdlib before dependencies | **Approved** (project principle) | AGENTS.md, §4, §7 |
| 9 | **Defer until justified**: local WAL + `mutation_id` replay, RLS, Patroni failover, bevy_ecs, REST API/Docker | **Proposed** (target design kept in §5.5/§7, gated) | this plan |

Note on item 1/3 wording: G-PG means **one canonical PostgreSQL** — the C++ baseline operates on the **same PostgreSQL** through a temporary compatibility adapter (its `libsql` layer speaks MySQL wire/SQL; the adapter bridges that), and MariaDB is used **only as the migration/export source** (initial data extraction), never as a second operational database. This direction was fixed by the user on 2026-08-10 and is recorded in ADR-0005 (**Accepted**, 2026-08-10); the G-PG gate closes when the implementation backlog B1–B8 is green (§8.2.1).

## 3. Context: the legacy server and why rewrite it

### 3.1 Current state

- C++ server (MartySama 5.9): binaries `game` (~104.7k LOC) and `db` (~12.8k LOC); `auth` is a mode of the `game` binary (`AUTH_SERVER`), not a separate process.
- C++ client v40999 (S3llMetin2 v24) — **untouched during the port**; it is the byte-exact protocol contract.
- Current database: MariaDB, legacy schema, data in CP949 (2-byte Korean).
- Full login verified and working against the real client (account `test`/`1234`).

### 3.2 Ymir decisions we do NOT repeat

| # | Ymir decision (2004) | Real consequence | Replacement decision |
|---|---|---|---|
| 1 | Computation and validation on the **client** | Speedhack, teleport, god-mode, memory hacking | **Server-authoritative**: the client sends intentions |
| 2 | Economic mutations without atomic transactions | Dupe by races and rollbacks | ACID transactions + single-writer per region |
| 3 | SQL queries by concatenation | SQL injection | Compile-time parameterized queries (sqlx) |
| 4 | Near-symbolic encryption, off by default | Sniffing, trivial packet forging | Real encryption with the new client (F7) |
| 5 | God object `char.cpp` (6.6k LOC) + copy-paste | Infinite debt, divergence bugs | Minimal entity + systems |
| 6 | Lua 5.0 with EUC-KR lexer (2 bytes/char) | Broken encoding, fragile quests | **Own declarative quest DSL** (no scripting; §12) |
| 7 | `fdwatch`/`select` event loop with backpressure bugs | WRITE floods, broken reconnections | tokio |
| 8 | Duplicated state between `game` and `db` | Inconsistencies, internal protocol to maintain | game+db unification (ADR-0002); shared PostgreSQL |
| 9 | Structs whose size changes with build flags | Extreme fragility | Single `protocol` crate with byte-exact golden tests |
| 10 | No tests, no reproducible verification | Every fix broke another | Parity tests + real-packet capture harness |

### 3.3 Audit of the legacy against 2026 standards (code evidence)

Direct audit of `source/server` + graphify graph (god node #1: `CHARACTER`, 815 edges) + industry research (August 2026). **Every legacy decision that is NOT carried over is listed with file:line.**

**What the legacy does WELL and is preserved:** sectree as interest management, minimal mob AI without pathfinding, partial anti-hack pattern in `SyncPosition` (input_main.cpp:1758), async save outside the world loop, typed QID catalog, `MoneyLog`, and the lock-free single-thread (the «single-writer» property is inherited, elevated to **single-writer per region**).

| Prio | Legacy decision (evidence) | Modern standard | Gain |
|---|---|---|---|
| **P0** | Movement without distance validation (`ENABLE_TP_SPEED_CHECK` commented out, input_main.cpp:1455) | Per-entity speed envelope, server clock | Kills speedhack/teleport |
| **P0** | Skill cooldowns without server check (`ENABLE_SKILL_COOLDOWN_CHECK` absent, char_skill.cpp:107) | Server-side cooldowns | Kills skill-spam/attack-rate |
| **P0** | 30-min write-behind persistence without atomicity (Cache.cpp:21,101) | Local WAL + batch ≤100ms + idempotent replay | Kills dupe by rollback |
| **P0** | Compile-time configuration (CommonDefines.h ~80 flags) | Runtime config + feature flags | End of cross-rebuilds |
| **P0** | Observability `sys_log`+`fflush` per call (log.c:171,218) | Structured tracing + metrics | Production debugging in minutes |
| **P1** | `select()` O(fds) (fdwatch.c:400) | tokio/epoll | Scalability; kills WRITE floods |
| **P1** | Global tick with allocs, O(all entities) (char_manager.cpp:641) | Regions + parallel systems | Ceiling 1,000+ players/instance |
| **P1** | Per-receiver serialized broadcast (entity_view.cpp:36-239) — 40k serializations/sec | Serialize once + references | ~40x less serialization |
| **P1** | God object `CHARACTER` (815 edges) | Minimal entity + systems | Testable and extensible |
| **P1** | Dead code (tea.s 121KB, liblua/5.2) | Port nothing without callers | Less surface |
| **P2** | Deploy build with AddressSanitizer enabled (Makefile) | Clean release; sanitizers in CI | 2-3x CPU recovered |
| **P2** | Content copy-paste (11+ `collect_quest_lvXX`) | Parameterized families in the DSL | Content without duplication |
| **P2** | Parallel systems (BlueDragon vs DragonLair; shop vs shopEx) | One system per concept, config-driven data | Less divergence |
| **P2** | Zero tests across the repo | Golden tests + parity harness | Advancement condition |

**Estimated gain:** 2-5x CPU available per tick (same implementation, better) → ceiling 300-500 → **1,000+ players/instance** with the new model.

**Industry validation (2026):** TCP is correct (WoW, FFXIV, EVE, GW2, ESO use TCP; Veloren abandoned UDP; Ember uses Boost.Asio). **ECS yes** for this vision (Veloren: millions of entities with ECS + regions; bevy_ecs standalone). No CQRS/event sourcing/outbox (over-engineering in a single process; only the append-only economic log). World tick 10-20 Hz (TrinityCore: 20 Hz). Ceiling reference: EVE (2,670-6,557 CCU per system with nodes).

## 4. Rewrite principles

1. **Do more with less**: YAGNI; stdlib before dependencies; one line before fifty.
2. **Structural redesign, not translation**: domain boundaries, data ownership, protocols, concurrency and failure are decided in writing (ADRs) before implementation.
3. **Server-authoritative**: the client is a view, never a source of truth.
4. **The DB does not compute, it guarantees**: logic lives in Rust; PostgreSQL enforces integrity.
5. **Incremental verifiable replacement**: each module preserves observable behavior and passes verification before advancing.
6. **Client frozen as contract** (F0–F6), with 1–2 additive data packets (§5.6). No Rust embedded inside the legacy client (ADR-0007, Accepted).
7. **Parity only where it matters**: observable behavior is preserved; internal code need not look alike.
8. **One process per region**: `game` + `db` unified (ADR-0002); `auth` is a role of the same binary (ADR-0004).
9. **Minimal dependency policy**: every dependency must justify itself (stack §7; deferrals §2.9).

## 5. Target architecture

### 5.1 Overview

```
Legacy C++ client (frozen + 2 additive packets)   Future Rust client (F7)
        │                                                │
        ▼                                                ▼
┌──────────────────────────────────────────────────────────────────┐
│              Rust server — world per region (process)            │
│  ┌──────────┐                                                    │
│  │ network  │      ┌───────────┐   ┌───────────┐                 │
│  │ (tokio)  │─▶    │ region 1  │   │ region N  │  …              │
│  └──────────┘      │ (task +   │   │ (task +   │                 │
│                    │  systems) │◀─▶│  systems) │  mpsc events    │
│  ┌──────────┐      │ mobs,     │   │ mobs,     │  inter-region   │
│  │  auth    │      │ players   │   │ players   │                 │
│  │  (role)  │      └─────┬─────┘   └─────┬─────┘                 │
│  └──────────┘            │               │                       │
│  ┌───────────────────────▼───────────────▼───────────────────┐  │
│  │  database crate (sqlx, async): queries → results via mpsc │  │
│  │  (regions NEVER await SQL inline)                        │  │
│  └───────────────────────┬───────────────────────────────────┘  │
└──────────────────────────┼──────────────────────────────────────┘
                           ▼
               ┌───────────────────────┐
               │   PostgreSQL 18       │  ← canonical store
               │  (state + integrity)  │
               └───────────────────────┘
```

Note: each entity (with its inventory/gold) belongs to ONE region at any moment — single-writer per region, anti-dupe intact.

### 5.2 Concurrency model

- **tokio, task-per-connection** for the network. **World per region**: a region = a grouping of map sectrees with its simulation task. One server = a collection of parallel regions.
- **One tokio task per region**, with systems over a minimal entity core: parallelizable queries, zero shared locks. Each entity (and its inventory/gold) belongs to ONE region → **single-writer per region, anti-dupe intact**; entity migration between regions with handshake.
- **Golden rule: the region task NEVER `.await`s SQL inline.** Legacy `ReturnQuery` pattern: emit the query, the `database` crate executes it on the multi-thread runtime, the result returns via mpsc. A slow query never stops the world.
- Inter-region communication via `mpsc` events (global chat, guilds, migrations). Broadcast with a saturation policy (drop-to-newest for position, queue for events).
- Timers with a binary heap (legacy `event_queue` pattern).
- **Scaling**: more players/mobs → finer region or more regions, without multiplying processes. Future multi-process (region groups) is not built until the benchmark demands it (gate F5: N bots × N regions).
- **ECS (bevy_ecs standalone) is the target design but deferred until justified [Proposed, §2.9]**: F4 starts with a plain per-region task over minimal entities; the ECS is introduced only if profiling shows the query-parallelism win.

### 5.3 Domain decomposition

The monolith is ported **by systems over a minimal Entity core** (VID, position, sectree, life state) — never as a single class. This is the most important architecture decision of the project.

| Domain | Legacy files | Difficulty | Notes |
|---|---|---|---|
| Protocol/wire | `packet.h`, `packet_info.cpp`, `tables.h` | 1 | Byte-exact contract; already specced (`../reference/protocol/login-flow.md`) |
| Net/transport | `libthecore` | 1-2 | Reimplement semantics, not port C |
| Auth/login | `input_auth`, `input_login`, auth path of `db.cpp` | 1 | AUTH_SERVER role; first vertical slice (F2a) |
| Data layer | `db/src/*` (~12.8k) | 2 | Internal crate; by domains (§5.5) |
| World/space | `sectree_manager`, `char_manager`, `dungeon`, `building` | 3 | Whole world in RAM |
| Entities | `char.cpp` (6.6k), `char.h`, `char_state` | 3 | **Port by systems, not as a god object** |
| Movement | Sync in `char.cpp`, `entity_view` | 2-3 | Anti-speedhack (§5.7) |
| Items/inventory | `char_item` (6.6k), `item`, `refine`, `blend`, `cube`, `safebox`, `shop` | 3 | Max dupe risk; transactions |
| Combat | `char_battle`, `battle`, `pvp`, `skill_power` | 3 | Damage 100% server |
| Skills | `char_skill`, `skill`, buffs | 3 | Server-side cooldowns |
| NPC AI + spawn | `mob_manager`, `regen`, FSM | 2 | Trivial FSM; no A* |
| Quests | `questmanager`, `questlua_*` (~10k bindings) | 3 | Own DSL (§12) |
| Social | `party`, `guild`, `guild_war`, `messenger`, `marriage` | 2-3 | SQL-heavy; cross-region |
| Economy | `exchange`, `safebox`, `shop`, `fishing`, `mining` | 2-3 | Anti-dupe critical |
| Admin/GM | `cmd_gm` (3.8k), `cmd_general`, `gm.cpp` | 2 | Permissions re-checked in DB |
| Config/locale/encoding | `config`, `locale_service` | 2 | Known encoding traps |
| Events/raids | `BlueDragon`, `DragonLair`, `OXEvent`, `wedding`, `arena`, `war_map` | 2-3 | Port everything, deferred (§9) |

### 5.4 Regional channels (EUW/LAN/LAS) — the distributed Metin2 model

**User requirement:** a single server for everyone — one account, one character with absolutely everything (inventory, gold, quests, guild) in any region. Channel-servers per region because of ping; changing region = pick a server and log in, no transfers.

**Model:** Metin2's channel system (channels share the DB → the character exists on all), distributed geographically. Players see «Server 1: Europe», «Server 2: EUW», «Server 3: Americas» — the internal term is channel/region.

- **One central PostgreSQL DB** with ALL durable state: accounts, characters, inventory, gold, quests, guilds.
- **One process per channel-region** (EUW, LAN, LAS…): each runs its own in-memory world copy (mobs, live market, PvP are per-channel — Metin2 behavior).
- **Login on any region**: you choose a server and log in; the character loads from the central DB with everything. The DB is the source of truth.
- **Cross-region anti-double-login**: row lock/advisory lock on the character row at entry; released at exit. Never two regions with the same character (that would be the clone/dupe).
- **Durable = write-through** (§5.5): the DB is always up to date → changing region loses NOTHING. Position/HP (volatile) are local.
- **Cross-channel coordination in PG**: guilds and caches via LISTEN/NOTIFY.
- **Unified trading**: the market (auction) lives in the DB (`economy`) — all channels share the same auction with advisory locks; bid closing with the DB clock (no per-region clock tricks).

**Latency — honest nuance:** the central DB is not in the hot path; player ping depends only on their regional channel. Persistence lag (100-200ms LAS↔EUW) is irrelevant for gameplay. Single point of failure (central DB) → mitigated with failover (§5.5).

**Cross-server regions (permanent and temporary):** maps that belong to no server-region, accessible from all — e.g. a war island or a permanent PvP continent where players from all regions meet and fight. Technically a **special region** with its own process (typically co-located with the central DB), to which characters migrate temporarily (same migration mechanism, single-writer intact) and return to their server on exit. Ping ~150-200ms for distant players (playable — the legacy is already played this way from LAT against EU servers). It is one more case of the region boundary, not a new system.

**What it is NOT:** a single living world like EVE (same instant for all continents, hot state migration between nodes) — transcontinental consistency is unsolved by any MMO, and physics limits it: light takes ~150ms across the Atlantic; someone always eats the ping. Regional channels with a shared DB deliver 95% of the dream with 100% of the physics honored.

### 5.5 Data layer — final design

**Base:** central PostgreSQL 18 (ADR-0001) + **sqlx 0.9 PgPool as candidate** (ADR-0001 left the concrete crate undecided; the crate choice is a G-PG task per ADR-0005). The `database` crate is organized **by domain**, each with its own schema, versioned migrations and repositories:

```
database crate
├── account/  → account schema (accounts, login, bans)
├── player/   → world schema   (characters, items, quests)
├── social/   → social schema  (guilds, parties, messenger)
├── economy/  → economy schema (auction, money log, trade history)
└── log/      → log schema     (append-only audit)
```

PG permissions per schema (log cannot write to economy — defense in depth). Contract: **in-memory world = live authority (zero SQL in the hot path); DB = persistence** — durable writes in transactional batches, reads only at boot/region change. All durable state (items, gold, quests, characters, guilds) is persistent by requirement.

**Cutover and legacy compatibility adapter (G-PG, Accepted — ADR-0005; closed spec in §8.2.1):**

- **One canonical PostgreSQL 18**: the Rust server targets it from the start (no auth/data-layer work on a MySQL-backed Rust path); F2a/F2b assume PG underneath. There is **no dual-store**: MariaDB is used only as the migration/export source (initial data extraction), never as a second operational database.
- The **C++ baseline source is not rewired** (frozen oracle; ADR-0003). To make it operate on the **same PostgreSQL**, a **temporary legacy compatibility adapter** bridges its MySQL-speaking `libsql` layer to PostgreSQL (wire/SQL translation). The adapter is temporary by contract — thin, explicit, removed at F6 (same rule as the ADR-0002 shim).
- G-PG deliverables: schema/data migration groundwork (types, defaults, `ENUM`/`SET`/`UNSIGNED` adaptation per ADR-0001 negative consequences) + a **data-comparison harness**; verification that C++ login→world→combat is unchanged through the adapter.
- F2 is **blocked** until G-PG is implemented (§8.2.1; ADR-0005 backlog B1–B8).

**Durable persistence pipeline (target design — deferred components marked):**

```
region (in-memory world)
  │  durable mutation
  ▼
PostgreSQL central (transactional batch ≤100ms, uuidv7, CHECK gold>=0)
  │
  └─ append-only audit log (same tx, OLD/NEW via RETURNING)
```

- Baseline (not deferred): transactional writes to PG in batches ≤100ms; uuidv7 IDs; `CHECK gold>=0` constraints; append-only audit log in the same transaction; partitioned audit by date + retention + `pg_stat_statements` from day one.
- **Deferred until justified [Proposed]:** local WAL per region with `mutation_id` (uuid) + idempotent replay (`ON CONFLICT DO NOTHING`), RLS (`current_setting('app.pid')`), Patroni hot-standby failover (~2 min promotion). The deferred items are the safety net for the "no dupe window" and "crash = max ~100ms in-flight loss" guarantees; the baseline contract keeps those guarantees as targets, with the exact mechanism gated on measurement.
- **Contract fixed in an ADR (pending):** durable = transactional batch ≤100ms; volatile = save every 30s + logout; failover ≤2 min (target).

**Why PostgreSQL and not redb/SQLite/SurrealDB/etc.:** redb is an **embedded** library (local file of ONE process) — it cannot serve N regions sharing the same DB; it breaks the regional channels. SQLite likewise (single-writer). SurrealDB is document-oriented (the character/items/guilds model is pure relational), immature, and without an ops ecosystem. CockroachDB: proprietary license + multi-node that does not exist here. TiDB/ScyllaDB: same + Scylla is not even relational. libSQL/Turso: on pause. **PG is the only one that fulfills the full contract: multi-row ACID, constraints, RLS, LISTEN/NOTIFY, failover, 25 years of battle-testing.**

### 5.6 Server→client data (versioned manifest + delta download)

**Legacy problem:** the client is a data source (item/NPC names and descriptions in its `.epk` pack, quest texts in locale.lua per language) separate from the server — changing a name = edit DB + repack client + patcher.

**Solution (legacy C++ client with additive modifications — only 1-2 new packets, no render/gameplay changes):** the server is the only data source; the client only renders.

- **Items/NPCs**: the server sends the data (names, descriptions) at login via **versioned manifest + delta**: the client asks «version 42?», the server answers «43 exists» and sends ONLY what changed (KB). The pack stays for visuals only (icons, models). Goodbye CP949 trap, repacking and the patcher for trivia.
- **Quest texts**: live in the DB per locale (`account.lang` decides the language); the server sends them localized — kills the 181 missing keys and the ES/EN mix. Zero files per language.
- **Patcher** remains only for the client binary and visuals; data travels via delta.
- The manifest is generated from the DB (single source of truth) — no manual copies.
- The file the client downloads is **ultra-light** (delta, KB); the base is downloaded once.

**Hot reload (no restarts):**

```
DB edited → LISTEN/NOTIFY from PG → server reloads table/quest in memory
          → manifest version bump → client asks delta → applies
```

- **Yes hot reload:** texts/languages, items/equipment/accessories (stats, names, descriptions), DSL quests (the next instance uses the new version), rates/config via manifest.
- **No reload:** in-flight world state (a half-finished quest keeps its instance until completion; an item in inventory keeps its instance). Only the data defining future behavior is reloaded.
- The old client applies the delta at login (safe); the recompiled client can refresh texts live.
- This is the legacy `PROTO_FROM_DB` pattern completed: the legacy loaded from the DB at boot; we do it at runtime with NOTIFY + manifest.

### 5.7 Movement and anti-speedhack — the design

**The legacy problem:** `ENABLE_TP_SPEED_CHECK` was commented out — the server accepted client positions without validation. Speedhack and teleport trivial.

**The design — the server owns the position:**

```
client sends: "I move to (x,y), running mode"
     │
     ▼
region validates (all with server clock):
  1. Alive and not stunned/paralyzed?         → no → ignore
  2. Movement cooldown respected?             → no → ignore
  3. Max distance for the mode? (walk/run/
     mount = distinct envelopes)              → no → correct
  4. Destination walkable? (map data from the
     SERVER, not the client)                  → no → correct
  5. Straight path without crossing walls?    → no → correct
     │
     ▼
  OK → authoritative position → re-broadcast to viewers
       (the client receives its corrected position if it deviated)
```

**Key rules:**
- **Per-entity envelope**: max distance = speed(entity) × time since last accepted movement. Server clock — the client cannot "gain time" by editing its local clock.
- **Correction, not ban**: on excess, discard and send the real position (mini-teleport back). Automatic ban only after N violations in T seconds (legacy `SyncPosition` + HackLog pattern, completed).
- **Client-side interpolation**: other entities look smooth between snapshots (10-20Hz).
- **Lag tolerance** (so 300ms does not break validation): explicit margin (+20% distance or +100ms) — fair with high ping.
- **Mobs/NPCs move 100% server-side**: the client never "moves" anyone but its own character.
- **Latency (already in Metin2)**: tick-based combat tolerates 150-300ms; optimized with client-side prediction (immediate animation, server value), interpolation, 10-20Hz tick, WoW-style lag compensation (validate the hit against the position the attacker saw). Everything perceived is optimized so 300ms feels like 20; everything validated uses the server clock.

### 5.8 Client modifications that unlock the server side (F0–F6)

Audit of `source/client` (evidence file:line). The client is recompiled (already proven 3 times); the golden rule for F0–F6: **changes only in (1) header table + phase cases, (2) in-memory data overrides, (3) pack python. NOTHING of render, pack formats or existing structs.**

**The immovable contract (the Rust server is built inside these walls):** existing headers/structs (LOGIN3 65/68B, TSimplePlayer with build flags), handshake, phase machine, limits (24-char name, 5 characters/account, stack 200, inventory). Changing them = post-F6 protocol project.

**CRITICAL — additive packets must be PULL-based:** the old client, faced with an unknown header, discards the whole receive buffer (`PythonNetworkStream.cpp:571-578, 654-662`) → a server pushing to a non-recompiled client desynchronizes the session. Pull = the client asks (only the recompiled one asks) and the server answers. The old client never desynchronizes.

**Free headers verified on both sides:** 139-149 (11), 154-160 (7), **162-207 (46, recommended)**, 211-214 + 216-255 (44). Registration mechanics: `Packet.h` + `Set(HEADER_GC_X, ...)` in `PythonNetworkStream.cpp:60-184` + `case` in the phase.

| # | Client modification | What it unlocks | Effort | Risk | When |
|---|---|---|---|---|---|
| 1 | **Additive pull-based packets** (CG_QUERY → GC_RESPONSE, headers 162+) | The whole §5.6 channel: versioned manifest + item/NPC/text delta | Low | Low | **F3-F4** |
| 2 | **In-memory overrides**: a new in-memory override API to be added around `CPythonNonPlayer`/`CItemData` after `LoadLocaleData` (these functions do NOT exist in the legacy client yet — no `SetLocaleName`/`SetItemLocaleName` in `PythonApplication.cpp:867-911`; the API must be written first) | The Rust server sends UTF-8 names from the DB; kills mojibake; the pack stops being the text source of truth | Low | Low | **F4-F5** |
| 3 | **Channel list from the auth** (override of `serverinfo.py`) | Channel IP/ports out of the pack (only the auth IP remains); runtime reconfiguration without repack | Medium | Medium-low | **F5-F6** |
| 4 | **(No client change) validate `dwLoginKey` (LOGIN_BY_KEY) at the channel** | The password is no longer resent in clear on reconnects (fix #14 without password); basis for tokenized sessions | Low | Low | **F2-F4** |
| 5 | Post-F6: on-disk manifest cache, remove `serverinfo.py`, revisit limits with the new client | — | — | — | **F7** |

**Other findings:** the client's text render is **native UTF-8** (`GrpTextInstance.cpp:124` `CP_UTF8`) → the server can send UTF-8 directly without conversion. Skill cooldowns are only displayed client-side (not validated) → the server validates. High risk: table parsing (`PythonSkill.cpp` — the 0xC0000374 crash), DX9 render, pack formats (TEA/LZO/MMPT0 — never change the format, only the content). The client already has NPC fallback to the server name.

### 5.9 Server-side freedoms thanks to the modifiable client

Rule: **touch the client only if (a) ≤1 week of work and (b) it unlocks something on the server side that cannot be achieved alone.** Nothing is forbidden; everything is cost/benefit. With that, the server side gains:

| Client (change) | Freedom in the Rust server | When |
|---|---|---|
| Additive pull-based packets (headers 162+) | Dynamic data channel: new items/NPCs/texts **without touching the pack** — the server is the only content source | F3-F4 |
| UTF-8 name overrides | The server controls **all visible text** (languages, corrections) from the DB; goodbye mojibake | F4-F5 |
| Channel list from the auth | Channels/servers **runtime-configurable** (IP, ports, open/close without repack); goodbye baked-in IP | F5-F6 |
| Version check on connect | The server **gates protocol evolution**: rejects old versions with a clear message; can add new packets without breaking recompiled clients | F2 |
| Hardware ID in LOGIN3 | **Hardware bans, anti-multibox** — without kernel drivers | F2 |
| Server time at login | Timers/events/cooldowns **consistent with the server clock**; kills local-clock tricks | F2 |
| Config via manifest (rates, visible limits) | Tune the game **without recompiling or repacking** | F5 |
| `dwLoginKey` (LOGIN_BY_KEY, no client change) | **Tokenized sessions**: the password is not resent in clear on reconnects | F2-F4 |

**Preparation for F7 (no C++ refactor — reusable artifacts):** spec of the client walls (limits/structs/headers), reproducible client build (script/CI), client as thin viewer (server data), UI design in Slint standalone (the `.slint` files survive). Refactoring "well-made" C++ modules = wasted work, not done. **No Rust embedded in the legacy client** (ADR-0007, Accepted).

## 6. Anti-hack model (server-authoritative)

**Governing principle: the client sends intentions, never facts.** Any datum the client could have edited in memory is untrusted; the server recomputes from its own state.

| Hack | Countermeasure |
|---|---|
| Speedhack / teleport | Per-entity speed envelope + map walkability (§5.7) |
| God-mode / one-shot / attack-speed | Full server-side damage; server-clock cooldowns; range and LoS via sectree |
| Dupe (the queen class) | (1) single-writer per region; (2) atomic transactions + deferred local WAL + `mutation_id` (§5.5); (3) uuidv7; (4) explicit save policy |
| Client memory hacking | The client is only a view; nothing it shows is read back from the client |
| Fake packets / GM commands | Strict phase state machine; permissions re-checked in DB; rate limits |
| SQL injection | Compile-time parameterized queries (sqlx) |
| Packet floods / spam | Per-connection rate limiting: packets/sec, bytes/sec, per action (chat, trade, skill, loot) — the legacy has none |
| Farm bots | Server-side behavior telemetry (farm routes/rhythms + flags for human review) — a selling differentiator |

**Persistence in two explicit classes:**
- **Durable** (items, gold, quest flags, guilds, safebox): transactional pipeline to PG (§5.5). A crash NEVER loses or duplicates items.
- **Volatile** (position, HP, cooldowns): save every 30s + logout. Losing seconds of position is acceptable; losing items is not.

## 7. Technology stack (2026)

| Layer | Choice | Justification |
|---|---|---|
| Language | **Rust** (edition 2024) | Memory safety, zero-cost abstractions |
| Async runtime | **tokio 1.x** | Standard; tasks, mpsc, timers |
| Entities | Plain per-region task first; **bevy_ecs standalone** deferred until justified [Proposed] | Parallelizable queries without the graphics engine; only if profiling demands it |
| Database | **PostgreSQL 18** | ACID, uuidv7, OLD/NEW in RETURNING, LISTEN/NOTIFY, advisory locks, RLS, incremental backups, failover |
| DB access | **sqlx 0.9** (candidate — concrete crate decision is a G-PG task, ADR-0005) | Compile-time checked queries, migrations, own pool |
| Quests | **Own declarative DSL** (§12) | Zero scripting runtime |
| Config | config-rs + clap 4.x (TOML, ADR-0004) | — |
| Observability | tracing + metrics (Prometheus/Grafana) | — |
| Tests | cargo test + proptest + golden tests + parity harness | — |

**Deferred until justified [Proposed]:** bevy_ecs (§5.2), local WAL + `mutation_id`, RLS, Patroni failover (§5.5), REST API + Docker (§8.3, F5+). The stack table above lists them as target design; each becomes a build dependency only with evidence.

**Rejected with justification:** CockroachDB (proprietary license), TiDB/ScyllaDB (unnecessary multi-node; Scylla not relational), SurrealDB (document-oriented + immature), libSQL/Turso (on pause), SQLite/redb (embedded, break the shared DB between regions), TimescaleDB (ADR-0001: only if logs prove it).

**Note on «let the DB compute everything»:** rejected. Logic in SQL triggers/procedures is an anti-pattern (untestable, latency, single bottleneck) and does not remove hacks: dupe is a save race, not a computation race. The DB **guarantees**; the server **computes**.

## 8. Migration strategy (strangler fig)

### 8.1 General shape

Vertical slices (client→auth→db→client) with the client frozen. The legacy `db` remains the oracle until the cutover. The `database` crate is built directly against **PostgreSQL** (G-PG); the ADR-0002 shim becomes a test artifact (golden tests), not a deployment path, and the legacy compatibility adapter (ADR-0005) covers interop.

### 8.2 Phases

| Phase | Goal | Verifiable milestone | Status |
|---|---|---|---|
| **F0** Foundations | Cargo workspace, ADRs, byte-exact `protocol` crate (login flow), packet capture harness | One real captured LOGIN3 parses and re-serializes identically | **Done** (30/30; capture harness pending WSL) |
| **F1** Network/transport | tokio listener with the verified semantics (`result > 0`/EAGAIN), framing, handshake with retries | C++ auth connects to a Rust peer and vice versa without floods | **Done through F1.5** (23/23); F1.6 integration milestone pending WSL |
| **G-PG** (gate before F2) [Accepted, ADR-0005] | PostgreSQL 18 provisioned (schemas per domain); schema/data migration groundwork + comparison harness; legacy compatibility adapter working — C++ baseline and legacy client behavior unchanged (login→world→combat smoke test) | F2 unblock checklist: ADR-0005 accepted ✓ (2026-08-10); PG provisioned; adapter verified; migration groundwork in place — implementation backlog B1–B8 (ADR-0005), spec §8.2.1 | **Spec closed; implementation pending** |
| **F2a** Auth slice [Accepted, ADR-0005] | AUTH_SERVER role on PG: LOGIN3, hash `"*"+UPPER(SHA1(UNHEX(SHA1(pw))))`, GC_AUTH_SUCCESS, `dwLoginKey` validation, PanamaPack 151 + hybrid-crypt 152/153 in `protocol::legacy` (ADR-0006), connection timeout | Login against Rust auth on PG + C++ db; legacy client completes auth | **Blocked by G-PG implementation (B1–B8)** |
| **F2b** Client batch 1 [Accepted, ADR-0005] | Additive client changes (≤1 week each): version check on connect, hardware ID in LOGIN3, server time | Recompiled client passes the version check | **Blocked by F2a** |
| **F3** Data layer + data channel | `database` crate by domain on PG; port by QID; pull-based packets 162+ (CG_QUERY/GC_RESPONSE) | The C++ game runs against the Rust data layer without behavior change; the recompiled client receives additive data without desynchronizing | Planned |
| **F4** World entry + names | CG_PLAYER_SELECT, spawn, map, stats; UTF-8 name overrides | The real client enters the world against the Rust core with correct names | Planned (requires domain-boundary ADR first, risk #2) |
| **F5** Gameplay | Movement, combat, drops, items, NPCs, quests, chat, shops, trade, GM — by domains, side-by-side; channel list from auth; config via manifest; **Slint standalone** (Accepted, ADR-0007); scale benchmark (N bots × N regions); REST + Docker deferred until justified [Proposed] | Full session without divergences + benchmark passed | Planned |
| **F6** Full parity | Automated side-by-side (same input → diff), instance-by-instance cutover; legacy compatibility adapter removed (ADR-0005); final data migration verified (backup/restore) | The Rust server replaces the C++ one without client changes | Planned |
| **F7** Client (after) | Rust client (wgpu), Slint UI (the `.slint` from F5 integrate), new protocol, real encryption; **delete `protocol::legacy`** (151/152/153) [Proposed, ADR-0006] | — | Future |

### Phase G-PG — PostgreSQL cutover

> Spec closed 2026-08-10 (G-PG design lane; ADR-0005 Accepted). Implementation backlog: ADR-0005 (items B1–B8). Inventories: [`../reference/database/legacy-schema.md`](../reference/database/legacy-schema.md) (77 tables) and [`../reference/database/legacy-sql-compatibility.md`](../reference/database/legacy-sql-compatibility.md) (204 SQL sites; its §4 translation map is the adapter's unit-test table).

#### a. Provision

- **PostgreSQL 18 on Debian 12 bookworm (WSL Debian-M2) via PGDG** (`apt.postgresql.org`, `bookworm-pgdg`): repo line `deb [signed-by=/usr/share/keyrings/pgdg.gpg] http://apt.postgresql.org/pub/repos/apt bookworm-pgdg main` (signing key `https://www.postgresql.org/media/keys/ACCC4CF8.asc`); packages `postgresql-18 postgresql-contrib-18` (pgcrypto ships in contrib).
- **Contingency (repo unreachable): `postgresql-15` from Debian bookworm main** — same feature surface for everything used here (`search_path`, temp tables, `ON CONFLICT`, identity, `interval`, pgcrypto); no spec change.
- Cluster `main` on `127.0.0.1:5432` (pg_hba scram for local); database `metin2`; schemas `account`, `player`, `common`, `log`; user `mt2` (owner of the four schemas, no SUPERUSER; password mt2, reused by the proxy). RLS stays deferred (§2.9 item 9) — per-schema permissions are the provisioned boundary.

#### b. Migration (phase 1 = login subset)

Scope = the tables the db boot + login path actually touch (verified 2026-08-10):

- `account`: `account`
- `player`: `player`, `player_index`, `item`, `quest`, `affect`, `safebox` (character load `ClientManagerPlayer.cpp:302-341`, `ClientManager.cpp:603`) + the PROTO_FROM_DB boot set: `mob_proto`, `item_proto` (`ClientManagerBoot.cpp:1290,1466`), `refine_proto` (121), `shop` + `shop_item` (248-254), `skill_proto` (476-482), `item_attr` / `item_attr_rare` (594-607, 719-732), `banword` (566), `land` (847-848), `object_proto` / `object` (950-951, 1021), `monarch` (boot join `Monarch.cpp:179`), `item` (id-range probes `ItemIDRangeManager.cpp:93,121`). `quest_item_proto` is **not** booted — the call is commented out (`ClientManagerBoot.cpp:438`).
- `common`: `locale` (boot `ClientManager.cpp:3078`), `priv_settings` (`ClientManager.cpp:112-115`), `exp_table` / `spam_db` (game boot `config.cpp:1389`, `db.cpp:575-590`), `gmlist` / `gmhost` (GM lists at game connect `ClientManager.cpp:3480,3531`)
- `log`: **all 26 tables, DDL only (empty)** — the game writes append-only logs during login (`loginlog2` `log.cpp:298-313`); empty-but-present tables are required so inserts never error
- Not migrated: `hotbackup` and the `srv1_*` clones (dropped — `legacy-schema.md` §2)

Type adaptation (`legacy-schema.md` §7): `int unsigned`→`bigint`, `smallint unsigned`→`integer`, `tinyint unsigned`→`smallint`, `bigint unsigned`→`numeric(20,0)`, display widths dropped; `enum`→`text`+CHECK and `set`→`text`+CHECK comma-joined (literals byte-identical, incl. `REMOVE_MEMEBER` §7.2); `datetime`→`timestamp` (no tz); `tinyint(1)`→`smallint` (never PG `boolean` — text-protocol parity); zero dates→NULL (OD-5); varbinary/CP949 columns (`item_proto.name`/`locale_name`, `mob_proto.locale_name`, `skill_proto.szName`)→`bytea`, bytes preserved exactly (AGENTS.md §17; `legacy-schema.md` §5 rules); `loginlog2.playtime`→`interval` (§7.3); `loginlog2.ip`/`hackshield_log.ip`→`bigint` (§9.9).

Identity: `GENERATED BY DEFAULT AS IDENTITY` (BY DEFAULT, not ALWAYS - B5 finding 2026-08-10: the proxy rewrites MySQL `VALUES(0, ...)` to `DEFAULT`, but explicit non-zero ids from `ITEM_ID_RANGE` pass through; ALWAYS would reject them) + `setval` — `item` 50 000 006, `player` 4, `land` 293, `refine_proto` 760, `exp_table` 121, `account` 2 (§7.5); new item ids come from `ITEM_ID_RANGE` (conf.txt), independent of the identity.

Logic re-implemented: `account.mysql_hash_password(text)` as a PG function with pgcrypto — `'*' || upper(encode(digest(decode(digest($1::bytea,'sha1'),'hex'),'sha1'),'hex'))` (`legacy-sql-compatibility.md` §6, OD-2); `MakeCharacter` trigger → CHECK `name ~ '^[A-Za-z0-9]+$'` (`legacy-schema.md` §7.4). Stored `account.password` values are copied verbatim — never rehashed.

Export/import: `mysqldump --hex-blob --no-create-info --skip-triggers --skip-comments` per database (hex-blob protects the CP949 varbinary bytes) → `scripts/gpg/import_py.py` (hex→`\x`, zero dates→NULL, `setval` seeding) → `scripts/gpg/schema_gpg.sql` (hand-written phase-1 DDL from `legacy-schema.md` §4 + live `SHOW CREATE TABLE`; the DDL vendoring pending from `legacy-schema.md` §8 is done here for phase 1).

Data parity: `scripts/gpg/parity_check.py` — per table, row count + md5 over the streamed sorted rows from both engines (bytea normalized to hex); non-zero exit on mismatch.

#### c. Adapter (boundary)

- **Form (OD-1 resolved): wire-level MySQL server protocol v10 proxy** — not a link shim: the C++ keeps linking `libmariadb` and connects to the proxy as if it were MySQL (`127.0.0.1:3307`). Zero C++ source change; runtime conf.txt only.
- **Location:** `source/reforge/mysql_proxy` (workspace member, flat layout per ADR-0004; temporary — deleted at F6). Rust, tokio + **tokio-postgres** (decided here: async, 1:1 sessions, pure Rust; sqlx remains the F2a `database`-crate candidate — non-blocking for G-PG). No MySQL-wire dependency — the v10 codec is hand-written. Modules: `wire` (HandshakeV10, HandshakeResponse41, COM_QUERY/COM_QUIT/COM_PING, OK/ERR/EOF/result set), `translate` (SQL rewrite), `session` (PG session + slot mapping).
- **Wire surface:** capabilities `CLIENT_PROTOCOL_41|PLUGIN_AUTH|SECURE_CONNECTION|CONNECT_WITH_DB|MULTI_STATEMENTS|TRANSACTIONS`; auth `mysql_native_password` (SHA1 scramble) validated against the proxy config (same user/password as conf.txt); no prepared statements (`CStmt` 0 call sites — `legacy-sql-compatibility.md` §2.1). Charset (`SET NAMES`, latin1/cp949) answered as pass-through — no transcoding anywhere (OD-6: PG db UTF8 + `bytea` for CP949 bytes).
- **Session mapping:** 1 MySQL connection = 1 PG session; per-slot `search_path`: `SQL_ACCOUNT`→`account,player` (QUERY_LOGIN cross-schema `player.player_index` — `ClientManagerLogin.cpp:413`), `SQL_PLAYER`→`player`, `SQL_COMMON`→`common`, `SQL_LOG`→`log`; game `player_sql`→`player,account` (the auth queries `account` through its player slot — `input_auth.cpp:144-218`), `common_sql`→`common`, `log_sql`→`log`. Session init: `standard_conforming_strings=off` (MySQL backslash escaping parity) and `TimeZone` server-local (OD-7).
- **SQL translation** (per COM_QUERY, mechanical; `legacy-sql-compatibility.md` §4 is the test table): backticks→double quotes; `+0` dropped; `NOW()`→`LOCALTIMESTAMP`; `UNIX_TIMESTAMP(x)`→`EXTRACT(EPOCH FROM x)`; `DATE_ADD(NOW(), INTERVAL n SECOND)`→`LOCALTIMESTAMP + make_interval(secs => n)`; `availDt - NOW() > 0`→`availDt > LOCALTIMESTAMP`; `REPLACE INTO`→`INSERT … ON CONFLICT (pk) DO UPDATE SET` (PK introspected from pg_catalog, cached per table); `INSERT … SET`→column-list form; `ON DUPLICATE KEY UPDATE`→`ON CONFLICT (id) DO UPDATE` (bare names = existing row = MySQL semantics); `SET sql_mode = ''`→no-op; `@var`→per-session temp table `pg_temp.m2var_<name>` (OD-4; the only pair is two separate queries — `log.cpp:309-313`); `inet_aton(x)`→`x::inet - '0.0.0.0'::inet`; `TIMEDIFF(a,b)`→`(a - b)`; `FROM_UNIXTIME(n)`→`to_timestamp(n)`; `CAST(x AS unsigned)`→`x::bigint`; `collate sjis_japanese_ci`→dropped; `UPDATE … LIMIT 1`→LIMIT dropped (PK-unique WHERE); `mysql_hash_password(...)` passes through (function in the `account` schema). One result set per COM_QUERY (no multi-statement strings exist).
- **Result contract** (`SQLMsg::Store`, `AsyncSQL.h:59-80`): uiNumRows = row count; uiAffectedRows = PG command-tag count (OD-8 decided: matched-rows; phase-1 consumers verified not to branch on changed-vs-matched); uiInsertID = `lastval()` after INSERT (error→0; item inserts carry explicit `ITEM_ID_RANGE` ids → 0, matching MySQL). Column metadata from PG OIDs: `bytea`→`MYSQL_TYPE_BLOB` with raw bytes (decode `\x` hex) — the Lua BLOB path (`questlua_global.cpp:1616-1624`) and the escaped-binary path (`ClientManagerPlayer.cpp:171-175`) depend on it; `IS_NUM` on numeric OIDs; `NOT_NULL_FLAG` from nullability; NULL = 0xfb.
- **Runtime change (only):** `db/conf.txt` `SQL_PLAYER/SQL_ACCOUNT/SQL_COMMON/SQL_LOG 127.0.0.1 <db> mt2 mt2 3307` (line format `Main.cpp:244-354`) and game conf `player_sql`/`common_sql`/`log_sql` (format `config.cpp:368-437`). MariaDB stays on 3306 untouched during the transition. Proxy config: TOML (ADR-0004) — listen, PG connect string, slot→search_path map, expected MySQL credentials.

> **Implementation notes (B5, 2026-08-10):** (1) column metadata resolved bytea-by-name via `pg_catalog` (simple query protocol exposes only names; covers `item_proto.name/locale_name`, `mob_proto.locale_name`, `skill_proto.szName`, `player.skill_level/quickslot`); everything else reported as VAR_STRING; `NOT_NULL_FLAG=0` (Lua bridge keeps working). (2) MySQL `INSERT ... VALUES(0, ...)` (generated id) → `DEFAULT` + hint `Generated` (`ClientManagerPlayer.cpp:863`); explicit non-zero ids → `Explicit` (item awards, `ClientManager.cpp:922-925`). (3) `item.window` ENUM index → literal (`Cache.cpp:56` writes 1..7). (4) PG errors mapped SQLSTATE→MySQL errno (42P01→1146, 42703→1054, 23505→1062); COM_QUERY non-UTF8 → ERR 1105 (never corruption; phase-1 traffic is ASCII). (5) **bytea literals → `decode('<hex>', 'hex')`** (2026-08-10, fixes 22021): MySQL `mysql_real_escape_string` blobs arrive as `\0` sequences; with `standard_conforming_strings=off` PG turns them into NUL bytes inside text literals → 22021. bytea columns in INSERT VALUES and UPDATE SET are re-emitted as hex-only text via `decode()`. (The `'\x...'` bytea literal form was rejected: with SCS=off PG would process the `\x` before bytea input — ambiguous double interpretation.)

#### d. Harness (parity)

- `scripts/gpg/parity_boot.sh`: (1) baseline — `start_m2_min.sh` on MariaDB, snapshot `db|auth|core` syslogs; (2) PG run — same with conf pointed at the proxy, snapshot; (3) compare — no NEW `SYSERR` lines, boot table lines equal (REFINE/SHOP/MOB/ITEM/GM — `sys_log(0)` lines); (4) assert `LoginSuccess` for account `test` in core1 syslog after a real client login `test`/`1234` (AGENTS.md runbook).
- `scripts/gpg/parity_check.py`: migration verification — counts + md5 per phase-1 table (MariaDB vs PG).
- **Exit criteria (gate close):** parity_boot.sh green on the PG run (0 SYSERR diff + `LoginSuccess`) AND parity_check.py green (all phase-1 tables equal). Then F2a unblocks.

> **Gate status (2026-08-10, loop):** B1–B8 complete. parity_boot A/B green on the PG run (0 SYSERR nuevos, boot table lines identical); REAL client login `test`/`1234` on PostgreSQL through the adapter — `LoginSuccess` 21:39:34 (core1 syslog), proxy log shows the translated QUERY_LOGIN (`mysql_hash_password(...)`, `LOCALTIMESTAMP`/`EXTRACT(EPOCH ...)`) and the character-select reads (`FROM player WHERE account_id=1` → 3 rows, from PG). MariaDB frozen as migration source; srv1 runtime operates on PG via the proxy (conf variants `*_pg`; revert = `cp *_mariadb` over the active files). F2a UNBLOCKED. Residual: parity_check excludes volatile `account.last_play` (live-login write); crate gaps 22P02/42703/22021 queued at F2a.

### 8.3 Feature set

**Port everything** (events are NOT discarded). **Order:** playable core first (movement, combat, skills, items, drops, NPCs, quests, chat, shops, safebox, trade, GM), **events/raids/massive social deferred** (OXEvent, weddings, BlueDragon, DragonLair, 3 empires, arena, guild wars) to be ported after — YAGNI: without this ordering cut, F5 is infinite.

## 9. What is not ported (deliberate simplifications)

- **Repeated code**: near-identical raid events (BlueDragon, DragonLair, xmas…) → **one encounter framework configurable by data**.
- **Legacy encryption**: plaintext is inherited ONLY while the C++ client lives (contract). The internal server and the future client (F7) use real encryption.
- **CP949 encoding**: reverted to UTF-8 for quest/locale content. Boot data tables (CP949 names referenced by `etc_drop_item.txt`) are kept byte-compatible or migrated atomically with `item_proto`.
- **Lua and all scripting**: removed. Quests = own declarative DSL (§12). The Rust server has no scripting runtime.
- **game↔db state duplication**: removed by design (ADR-0002).
- **Boot order**: disappears (only PostgreSQL first).

## 10. Risks and mitigations

| # | Risk | Mitigation |
|---|---|---|
| 1 | Byte-exact protocol parity (build flags change sizes) | Golden tests with real tcpdump captures in every phase; single `protocol` crate as source of truth; automated F6 side-by-side |
| 2 | Translating the `char.cpp` god object | Domain-boundary ADR BEFORE F4; port by systems over a minimal entity; side-by-side validation |
| 3 | Converting the quest corpus (194 `.quest`) | Own DSL + automatic converter + parity harness; quests that do not fit → direct Rust |
| 4 | Infinite monolith scope | Agreed feature set (core first, events deferred) |
| 5 | Fragile/manual verification (4GB/WSL unstable environment) | Scripted verification from F0 (smoke test login→world→combat); cross-region deferred |
| 6 | Cross-channel coordination in PG (latency vs cache) | Explicit contracts + benchmark before porting GuildManager/LoginData |
| 7 | G-PG cutover risk (schema mapping, adapter bugs) | Data-comparison harness; adapter temporary by contract; C++ baseline **source** untouched — it operates on PostgreSQL only through the adapter (ADR-0005, Accepted; spec §8.2.1) |

## 11. Decisions taken

**ADR statuses (see `../decisions/`):**

| ADR | Title | Status |
|---|---|---|
| 0001 | PostgreSQL as primary DB, no TimescaleDB by default | Accepted |
| 0002 | Unify `game` + `db` into one process per region | Accepted |
| 0003 | Rust workspace in `source/reforge` | Accepted (partially superseded by 0004) |
| 0004 | Flat workspace: `protocol`, `network`, `database`, `realm`, `server_realms`; config TOML | Accepted |
| 0005 | PostgreSQL cutover (G-PG) + temporary legacy compatibility adapter (single canonical PG; C++ operates on it through the adapter); F2 gated | **Accepted** |
| 0006 | Legacy wire/pack compatibility boundary (`protocol::legacy`, deletion at F7) | **Proposed** |
| 0007 | No partial Rust embedded in the legacy client (F0–F6) | Accepted |

**Design decisions (from this plan, previously agreed):**

- Rust stack: tokio 1.x + sqlx 0.9 + config-rs + clap + tracing + proptest. No scripting (own DSL). bevy_ecs/WAL/RLS/Patroni/REST/Docker deferred until justified [Proposed].
- Model: server-authoritative + DB as atomic safety net.
- Strategy: strangler by vertical slices; client frozen (+2 additive packets); cutover at F6.
- Audit §3.3: 14 P0/P1/P2 legacy decisions are NOT carried over; the 7 good things are preserved.
- Scale: world per region with single-writer per region (anti-dupe intact); TCP; tick 10-20Hz; no CQRS/event sourcing.
- Regional channels (§5.4): central DB + one process per region; changing region = logout→login; unified trading; permanent and temporary cross-server regions. Global uuidv7 IDs.
- Data layer (§5.5): domain modules, transactional batches ≤100ms, audit log; WAL + mutation_id, RLS, failover deferred [Proposed].
- Server→client data (§5.6): versioned manifest + delta; patcher only for binary/visuals.
- Community adoption: public protocol/quest documentation from F0, MPL-2.0 license proposal (AGPL repels pserver operators), anti-bot as differentiator, and F6 with the real client working as the unique argument (no Metin2 server in Rust with traction exists).

## 12. Quest DSL

Full specification: [`../reference/quests/quest-dsl.md`](../reference/quests/quest-dsl.md) (Status: Proposed; runtime implementation is future, F5+).

Summary of the decisions:

- **No Lua.** Legacy quests run on Lua 5.0 (EUC-KR lexer) compiled from Metin2's own `qc` DSL. Real content: 194 `.quest` files (~2,500+ duplicated lines in the `collect_quest_lv30..lv96` family alone).
- **Own declarative DSL**, typed and validated by a Rust parser, with composition (families + blocks + imports).
- Indentation-significant (2 spaces), `quest`/`state`/`on`/`->` structure, `#` comments; every action/trigger/condition is **known to the parser** (typed catalog) — no escape to free code.
- Triggers/conditions/actions inventories derived from the real corpus (§3–§5 of the spec).
- Families (`quest X = base(level: 30, mob: 601, …)`) eliminate the duplicated quest families; automatic converter (qc → DSL) with a **parity harness** (same inputs → same final state and dialog output).
- Special cases with real coordination logic (`oxevent`, `christmas_*`, `game.set_event_flag`) → **Rust server modules**, not DSL growth.
- Open decisions (§11 of the spec): `between` syntax, `if` depth, `select` capture, locale key naming, file extension, explicit `timer` trigger.

## 13. Open questions for reviewers

1. **Authority model**: any counterargument to «client sends intentions, server computes, DB guarantees»? Are there known Metin2 hacks this model does not cover?
2. **Stack**: anything better than PostgreSQL 18 + sqlx for a single-node MMO in 2026? Any PG 19 feature (GA Oct 2026) worth waiting for?
3. **Concurrency**: is world-per-region with systems (ECS deferred) the right choice for scaling? Or actors from the start? (Position: regions + deferred ECS; YAGNI on multi-process until benchmark.)
4. **Quests**: is the DSL grammar elegant and complete for the corpus? Missing triggers/conditions/actions? (open decisions in the spec §11)
5. **Migration**: is the F0→G-PG→F2a/F2b→F6 order correct? Is a validation step missing between phases?
6. **Scope**: is deferring events/raids/massive social to the end correct?
7. **Audit §3.3**: is any legacy decision missing from the P0/P1/P2 table?
8. **Adoption**: is MPL-2.0 the right license? Web API + metrics + Docker from F5 or after the cutover?
9. **Regional channels (§5.4)**: is «central DB + process per region, change region = logout→login» correct? Is a shared living world like EVE definitively out by design?
10. **Persistence (§5.5)**: is the transactional batch ≤100ms pipeline correct? Is the deferral of WAL/RLS/Patroni acceptable until justified? [Deferrals confirmed with ADR-0005 acceptance, 2026-08-10 — §2.9 item 9]
11. **Server→client data (§5.6)**: is the versioned manifest + delta the right mechanism? Are the 2 additive client packets acceptable before F7?
12. **What I do not see**: what are we missing?

## 14. Next steps

1. Collect reviewer feedback on this document.
2. **Confirm or reject ADRs 0005 and 0006** (G-PG cutover + adapter; `protocol::legacy` boundary) — they gate F2.
3. Write the pending ADRs: domain boundaries (char.cpp by systems), concurrency (regions + deferred ECS), quest engine (own DSL), anti-hack model, regional channels (central DB + process per region), data layer (transactional batches; deferral list), server→client data (manifest + delta), migration review of ADR-0002.
4. Update `../../ROADMAP.md` with the corrections (G-PG before F2, F2a/F2b, blocked-by markers) — maintained by the orchestrator.
5. Write the formal implementation plan with granular tasks (by domain, TDD, scripted verification).
