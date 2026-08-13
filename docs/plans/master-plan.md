---
Type: Plan
Status: Draft v0.2 — oracle-reviewed 2026-08-12 (recommendations H.1–H.5 applied)
Audience: Contributors, maintainers, reviewers
Last verified: 2026-08-13
Supersedes: — (consolidation of ROADMAP.md + docs/plans/server-rewrite.md + docs/plans/locale-redesign.md + ADRs 0001–0011 + docs/CURRENT.md; the originals remain canonical sources)
---

# Metin2 Rust Rewrite — Consolidated Master Plan ("The Big Plan")

> **Purpose:** ONE document that joins the current plan and all project documentation, so a reviewer can evaluate the whole strategy at once. Built 2026-08-12 from: `ROADMAP.md` (phase tracker), `docs/plans/server-rewrite.md` (canonical design, v0.3), `docs/plans/locale-redesign.md`, ADRs 0001–0011, `docs/CURRENT.md` (snapshot), `CHANGELOG.md` (history), `AGENTS.md` (rules + verified facts).
> **Oracle review (2026-08-12):** the oracle reviewed the whole plan and its recommendations were applied — (H.1) ECS migration slice is the next F5 slice + provisional benchmark right after; (H.2) push + backup cadence (53 commits ahead, single-copy host); (H.3) Slint standalone → F7 and REST/metrics → post-cutover; (H.4) F3 milestone redefined + real-client E2E gate every N slices; (H.5) docs staleness sweep. **ECS implementation confirmed by the user** (mob-farming density is the core requirement — see §8).
> **Living trackers stay:** `ROADMAP.md` (per-session status), `CHANGELOG.md` (chronological), `docs/CURRENT.md` (snapshot). This document is the reviewable whole-plan view.

---

## 1. Mission and principles

**Mission:** completely rewrite the Metin2 server (legacy C++ MartySama 5.9, ~120k LOC: `game` ~104.7k + `db` ~12.8k + auth as a mode of the `game` binary) in **Rust** as a **structural redesign**, not a line-by-line translation. The legacy C++ client v40999 (S3llMetin2 v24) is the frozen byte-exact wire contract during the migration (F0–F6). **Motto: do more with less** — less code, less complexity, fewer dependencies; quality from what is necessary.

1. **Do more with less** — YAGNI; stdlib before dependencies; one line before fifty (ponytail rule).
2. **Structural redesign, not translation** — domain boundaries, data ownership, protocols, concurrency, failures, migration are decided in writing (ADRs) BEFORE implementation.
3. **Server-authoritative** — the client sends intentions, the server computes facts; the client is a view, never a source of truth.
4. **The DB does not compute, it guarantees** — game logic lives in Rust; PostgreSQL enforces integrity (constraints, transactions, locks, RLS, audit).
5. **Verifiable incremental replacement** (strangler fig) — each Rust module preserves the observable behavior of its C++ counterpart and passes verification before advancing; the C++ baseline is the oracle until the cutover.
6. **Client frozen as contract (F0–F6)** — only additive changes ≤1 week that unblock the server side (cost/benefit rule); **no partial Rust embedded in the legacy client** (ADR-0007, Accepted).
7. **Parity only where it matters** — observable behavior is preserved; internal code need not look the same.
8. **ADR before implementing** — every architecture decision is recorded first.
9. **Hot reload by design** — texts, items, quests, config edited in the DB, reloaded at runtime (NOTIFY + manifest) — no restarts, no recompiles.

## 2. Verified current state (2026-08-12)

- **Login fully working** against the real client (auth + channel + character select; account `test`/`1234`). The full legacy chain of verified fixes is documented in `AGENTS.md` (§1–17).
- **G-PG COMPLETE (2026-08-10):** PostgreSQL 18.4 (PGDG, WSL Debian-M2) is **the single canonical store** (ADR-0005 Accepted, gate 4/4, backlog B1–B8). MariaDB is frozen as migration/export source only. The C++ baseline operates on PG through the **`mysql_proxy`** adapter (wire v10 + SQL translation, 67 tests) — real client login on PG verified (`LoginSuccess` 21:39:34), boot parity A/B green, migration parity 30/30.
- **Rust workspace** `source/reforge` (flat layout, ADR-0004, toolchain 1.97.0, edition 2024, `unsafe_code = "forbid"`): crates `protocol`, `network`, `database`, `game_core` (renamed from `realm` 2026-08-13, 42nd part), `mysql_proxy` (temporary, removed at F6), `locale_import`, `quest_dsl`, `bench_bot` + single binary `server_realms` (roles `auth` | `channel` by TOML config).
- **Runtime (2026-08-12, ADR-0012):** **native Windows primary** — PG 18 Windows (db `metin2`, schemas account/player/common/log + world, role `mt2`), `server_realms` auth :30001 / channel :30003, `REALM_WAL_DIR` Windows path, client connects to `127.0.0.1` (serverinfo.py repacked). **WSL = on-demand oracle box only** (frozen C++ db/core + `mysql_proxy`, cap 1 GB, off when unused): the frozen Linux-ELF C++ binaries run only there until F6 (parity A/B, golden captures, F6 side-by-side); proxy stays in WSL so the frozen C++ `conf.txt` is never touched (proxy TOML's PG target = WSL gateway IP, patched per boot). MariaDB archived + stopped. `scripts/start_win.ps1` / `stop_win.ps1` (bash scripts = parity path only).
- **Plan consolidation + oracle review (2026-08-12):** this document created joining all docs; oracle review applied (H.1–H.5, header above). **ECS migration DONE 2026-08-13** (`MobCache → bevy World`, per ADR-0010:148-150 — tail §8); **next slice: spawn dinámico** (channel-level shared World, IN PROGRESS).
- **F0–F2b DONE, F3 partial, F4 milestone MET, F5 in progress:**
  - F0: `protocol` crate byte-exact, 30/30 at close; **golden capture harness MET** (real LOGIN3 88B parses + re-serializes byte-identical).
  - F1: tokio listener/framer/handshake, 25/25; **F1.6 MET** (Rust peer ↔ live C++ auth, no floods).
  - F2a: Rust auth serving real client logins on PG (select screen reached; `protocol::legacy` 151–153 isolated per ADR-0006). Debt: `dwLoginKey` real flow (password still re-sent on reconnects).
  - F2b: client batch 1 — version check (40999 gate) + hardware ID in LOGIN3 (88B) verified end-to-end; server time verified working.
  - F3: `account`/`world` repos on PG + **WAL phase 2 DONE (2026-08-12)** (WalSink durable-first + idempotent replay + audit same-tx; gated `replay_wal` PG test pending by user directive) + snapshot harness (27 OK / 4 operational DIFFs) + `protocol::datachannel` 162/163 minimal wire (client registration inert). Pending: items/social QIDs, active data channel, PROTO_FROM_DB maintenance.
  - F4: **milestone MET 2026-08-11** — real client world entry against the Rust channel (select → DirectEnter → map 41, 50+ s sustained). ECS adoption decided (ADR-0010).
  - F5: **F5.3 gameplay slices 1–17 DONE (2026-08-12)** — see §8. Tail pending.
- **Client (legacy, additive changes only):** 3 recompiles verified (auth fix, world-entry crash fix `C7EAD7CC` — heap over-read in `string_replace_word`, locale cache + UTF-8 patches). Installed client lives OUTSIDE the repo (`C:\projects\metin2-extra\client`).
- **Repository:** `github.com/ryerdevs/reforge-core` (PUBLIC). ⚠️ Local is **53 commits ahead of origin/main** (last pushed `352b850`, HEAD `d6d80d3`, verified 2026-08-12) — push backlog pending user decision.
- **Graphs (graphify):** server 13,200 nodes / 33,251 links; client 17,501 / 39,258; merged 30,701 / 72,509.

## 3. Architecture decisions (ADR index)

| ADR | Title | Status |
|---|---|---|
| 0001 | PostgreSQL as primary DB, no TimescaleDB by default | Accepted (2026-08-06) |
| 0002 | Unify `game` + `db` into one process per region (db as crate) | Accepted; final unification at F6 |
| 0003 | Rust workspace in `source/reforge` (location, property boundary, verification policy) | Accepted; partially superseded by 0004 |
| 0004 | Flat layout: `protocol`/`network`/`database`/`game_core` (renamed from `realm` 2026-08-13) + single binary `server_realms` (roles by config, TOML) | Accepted (2026-08-10) |
| 0005 | PostgreSQL cutover (G-PG) + temporary legacy compatibility adapter; F2 gated | **Accepted** (2026-08-10) + implemented (B1–B8, gate 4/4) |
| 0006 | Legacy wire/pack compatibility boundary — `protocol::legacy`, deleted at F7 | **Accepted** + implemented in F2a |
| 0007 | No partial Rust embedded in the legacy client (F0–F6); new client standalone (Slint in F5, bevy+Slint in F7) | Accepted (2026-08-10) |
| 0008 | Data layer: tokio-postgres 0.7, domain repos, save-by-event + local WAL + idempotent replay (amended 2026-08-12), RLS post-WAL, Patroni F5/F6 | Accepted (2026-08-11) |
| 0009 | Server-side locale: server owns ALL text per player language (8 `common.*` tables, CG_LOCALE_REQUEST/GC_LOCALE, EN fallback) | Accepted (2026-08-12) |
| 0010 | Domain boundaries: pure-function modules + **bevy_ecs World adopted** (user decision) + per-connection session state + WorldStore; translator-vs-core boundary; wire debt inventory D1–D6 (F7 deletion list) | Accepted (2026-08-12) |
| 0011 | Anti-hack model: always-on controls (ratified), signed clock wrap decided, pending controls phased | Accepted (2026-08-12) |

**Not yet written (open ADRs):** concurrency (regions + ECS — largely ratified by ADR-0010), quest engine (own DSL), regional channels, server→client data (manifest + delta — partially in ADR-0009), migration review of ADR-0002.

## 4. Target architecture

### 4.1 Overview

```
Legacy C++ client (frozen + additive packets)   Future Rust client (F7: bevy + Slint)
        │                                                │
        ▼                                                ▼
┌──────────────────────────────────────────────────────────────────┐
│              Rust server — one process per region               │
│  ┌──────────┐     ┌───────────┐   ┌───────────┐                  │
│  │ network  │─▶   │ region 1  │   │ region N  │ …                │
│  │ (tokio)  │     │ (bevy_ecs │◀─▶│ World +   │  mpsc events     │
│  └──────────┘     │  World +  │   │ systems)  │  inter-region    │
│  ┌──────────┐     │  systems) │   └─────┬─────┘                  │
│  │  auth    │     │ mobs, players      │                         │
│  │  (role)  │     └─────┬──────────────┘                         │
│  └──────────┘           ▼                                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ database crate (tokio-postgres): queries → results via mpsc │   │
│  │ (regions NEVER await SQL inline; Batcher ≤100ms + local WAL)│  │
│  └──────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────┬───────────────────────┘
                                           ▼
                       PostgreSQL 18 (single canonical store)
```

### 4.2 Concurrency model

- tokio task-per-connection (network) + **one tokio task per region** running systems over a **bevy_ecs standalone World** (ADR-0010, adopted 2026-08-12 — user decision; mob-farming density is the core requirement: 145,876 spawns imported, map 41 = 10,026).
- **Single-writer per region**: each entity (with inventory/gold) belongs to ONE region — anti-dupe intact; entity migration between regions with handshake.
- **Golden rule: the region task NEVER `.await`s SQL inline** — queries flow to the `database` crate via mpsc; a slow query never stops the world.
- Inter-region via mpsc (global chat, guilds, migrations); broadcast with saturation policy (drop-to-newest positions, queue events).
- Timers: binary heap (legacy `event_queue` pattern). Tick 10–20 Hz; AI tick 500 ms.
- Scaling: finer/parallel regions first; multi-process (region groups) only if the F5 benchmark demands it.
- Pure domain modules (`game_core::combat`, `game_core::ai`, `game_core::movement`, `game_core::packets`, `game_core::npc`) keep formulas pure for parity tests; systems orchestrate; per-connection session state; `WorldStore` for persistence.

### 4.3 Domain decomposition (port by systems over minimal entity — never a god object)

| Domain | Legacy files | Difficulty | Notes |
|---|---|---|---|
| Protocol/wire | packet.h, packet_info.cpp, tables.h | 1 | Byte-exact; done in `protocol` crate (F0) |
| Net/transport | libthecore | 1–2 | tokio; done (F1) |
| Auth/login | input_auth, input_login, db auth path | 1 | AUTH_SERVER role; done (F2a) |
| Data layer | db/src (~12.8k) | 2 | Internal crate by domains (F3) |
| World/space | sectree, char_manager, dungeon, building | 3 | Whole world in RAM |
| Entities | char.cpp (6.6k), char.h, char_state | 3 | Port by systems (ADR-0010) |
| Movement | Sync, entity_view | 2–3 | Anti-speedhack (§4.7) |
| Items/inventory | char_item (6.6k), item, refine, blend, cube, safebox, shop | 3 | Max dupe risk; ACID |
| Combat | char_battle, battle, pvp, skill_power | 3 | Damage 100% server |
| Skills | char_skill, skill, buffs | 3 | Server-side cooldowns |
| NPC AI + spawn | mob_manager, regen, FSM | 2 | Trivial FSM; no A* |
| Quests | questmanager, questlua_* (~10k bindings) | 3 | Own DSL (§10) |
| Social | party, guild, guild_war, messenger, marriage | 2–3 | SQL-heavy; cross-region |
| Economy | exchange, safebox, shop, fishing, mining | 2–3 | Anti-dupe critical |
| Admin/GM | cmd_gm (3.8k), cmd_general, gm.cpp | 2 | Permissions re-checked in DB |
| Config/locale/encoding | config, locale_service | 2 | Known encoding traps |
| Events/raids | BlueDragon, DragonLair, OXEvent, wedding, arena, war_map | 2–3 | Port everything, deferred (§11) |

### 4.4 Regional channels (EUW/LAN/LAS — user requirement)

- **One central PostgreSQL** with ALL durable state (accounts, characters, inventory, gold, quests, guilds). **One process per channel-region** (Europe, EUW, Americas…), each with its own in-memory world copy (mobs, live market, PvP per-channel — Metin2 behavior). Changing region = pick a server and log in; the character loads from the central DB with everything; **no transfers**.
- **Cross-region anti-double-login**: row/advisory lock on the character row at entry, released at exit — never two regions with the same character.
- **Durable = write-through** (§4.5): changing region loses NOTHING; position/HP (volatile) are local.
- Cross-channel coordination in PG (guilds, caches) via LISTEN/NOTIFY. **Unified trading**: auction lives in the DB (`economy`); bid closing with the DB clock.
- **Cross-server regions** (permanent PvP island / temporary war maps): special region with its own process, typically co-located with the central DB; characters migrate temporarily (same single-writer mechanism) and return on exit. Ping 150–200 ms for distant players — playable (legacy is already played this way).
- **What it is NOT:** a single living world like EVE (transcontinental consistency is physically unsolved); regional channels + shared DB deliver 95% of the dream with 100% of the physics honored.

### 4.5 Data layer (ADR-0008)

- **PostgreSQL 18, single canonical store**; `database` crate by domain: `account` / `world` / `social` / `economy` / `log` (one PG schema each; per-schema grants are the provisioned boundary — **note:** today the single `mt2` role owns all four schemas, so the log-can't-write-economy separation is enforced by repo discipline until RLS lands). Repositories are the ONLY access path — no `direct-sql` at runtime. **Driver: tokio-postgres 0.7** (pool later via deadpool-postgres if measured; sqlx not adopted — ADR-0008, WAL phase done 2026-08-12).
- **Contract (fixed):** *Durable* = save-by-event → `Batcher` ≤100 ms (one tx) → **local WAL file first** (JSONL, `sync_all`, deleted only post-COMMIT, replayed idempotently at boot once per process) → PostgreSQL; audit in the same tx. *Volatile* (position/HP) = local, event-driven save via Batcher+WAL.
- **uuidv7 IDs; `CHECK gold>=0`; append-only audit log partitioned by date + retention + pg_stat_statements** from day one.
- **Deferred until justified:** RLS (`current_setting('app.pid')`) post-WAL; Patroni hot-standby failover (~2 min promotion) F5/F6.
- **Why PG:** redb/SQLite are embedded (cannot serve N regions), SurrealDB document-oriented + immature, CockroachDB proprietary license, TiDB/ScyllaDB unnecessary multi-node — PG is the only one fulfilling the full contract (multi-row ACID, constraints, RLS, LISTEN/NOTIFY, failover, 25 years of battle-testing).

### 4.6 Server→client data (versioned manifest + delta) + locale redesign (ADR-0009)

- **Problem:** the client is a data source (names/descriptions in `.epk` packs, quest texts in locale.lua per language) — changing a name = edit DB + repack + patcher.
- **Solution:** the server is the only data source; the client only renders. Items/NPCs/texts via **versioned manifest + delta** («version 42?» → «43 exists» → only what changed, KB). Pack stays for visuals only. Patcher remains only for binary/visuals. Manifest generated from the DB (single source of truth).
- **Hot reload:** DB edit → PG LISTEN/NOTIFY → server reloads → manifest bump → client asks delta → applies. No reload of in-flight world state (only data defining future behavior).
- **Locale (ADR-0009, plan `docs/plans/locale-redesign.md`):** 8 `common.*` tables (`mob_names`, `item_names`, `item_descriptions`, `skill_names`, `map_names`, `ui_texts`, `message_texts`, `item_icons` — one row per (entity, language); new language = INSERTs, never ALTER). Wire: `CG_LOCALE_REQUEST`/`GC_LOCALE` (stateless → both roles serve it → hot reload; chunked if needed). Client: one cache module (CPythonLocale) + fallback chain cache → pack → empty; loading screen at start; **language selector at login, default EN** (diverges from legacy ES — intentional). Refined items (+1..+9): derived from a single base row (name = base + " +N"), no manual duplication. Importer `locale_import` DONE (mob_names 8,628; item_names 34,281; item_descriptions 22,674; ui_texts 3,903; message_texts 12,489; maps 65; spawns 145,876). **Wire slice pending** (auth side implemented; client cache integration DONE 2026-08-12 9th part).
- **Maps & spawns** in PG (`world.maps`, `world.spawns` — importer reuses the verified `game_core::npc` parser; channel loads per map at each world entry → edits visible on next entry without restart).

### 4.7 Movement and anti-speedhack (§5.7 of the design)

Legacy: `ENABLE_TP_SPEED_CHECK` commented out — positions accepted unvalidated. Design — **the server owns the position**:

```
client: "I move to (x,y), running" → region validates (server clock):
  1. alive, not stunned/paralyzed?    → no → ignore
  2. movement cooldown respected?     → no → ignore
  3. max distance for the mode? (walk/run/mount envelopes) → no → correct
  4. destination walkable? (map data from SERVER, not client) → no → correct
  5. straight path without walls?     → no → correct
  OK → authoritative position → re-broadcast to viewers
```

- **Per-entity envelope**: max distance = speed × server time since last accepted move; the client cannot "gain time".
- **Correction, not ban**: on excess → discard + send real position (mini-teleport back); automatic ban only after N violations in T seconds (legacy `SyncPosition` + HackLog pattern, completed).
- **Lag tolerance**: +20% distance or +100 ms margin (300 ms ping playable). Mob/NPC movement 100% server-side. Client-side interpolation between 10–20 Hz snapshots; WoW-style lag compensation for hits.

### 4.8 Client modifications that unlock the server side (F0–F6, additive only, ≤1 week each)

| # | Client change | Unlocks | When |
|---|---|---|---|
| 1 | Additive **pull-based** packets (CG_QUERY → GC_RESPONSE, headers 162+) — PULL because the old client discards the buffer on unknown headers | The whole §4.6 channel (manifest + delta) | F3–F4 |
| 2 | In-memory name overrides around `CPythonNonPlayer`/`CItemData` (new API must be written — none exists) | Server sends UTF-8 names from DB; kills mojibake and the CP949 trap | F4–F5 |
| 3 | Channel list from the auth (override of `serverinfo.py`) | Channel IPs/ports runtime-configurable; goodbye baked IP | F5–F6 |
| 4 | (No client change) validate `dwLoginKey` (LOGIN_BY_KEY) at the channel | Password not re-sent in clear on reconnects; tokenized sessions | F2–F4 |
| 5 | Post-F6: on-disk manifest cache, remove `serverinfo.py`, revisit limits (24-char name, 5 chars, stack 200, inventory) with the new client | — | F7 |

Known client facts: text render is native UTF-8 (CP_UTF8) → server can send UTF-8 directly; skill cooldowns display-only (server validates); high risk: table parsing (`PythonSkill.cpp` — the 0xC0000374 crash source, fixed), DX9 render, pack formats (never change format, only content).

## 5. Anti-hack model (ADR-0011 — always-on, server-authoritative, zero client trust)

| Attack class | Defense | State |
|---|---|---|
| Speedhack | Server-clock delta, always on (`movement.rs:94-104`; C++ default OFF) | **Implemented** |
| Teleport | Max distance per MOVE, reject-no-move (`movement.rs:106-114`; C++ had it commented) | **Implemented** |
| Fake/malformed packets | Fixed-size framer; unknown header / 0x00 → close (`framer.rs:44-47`; deliberate divergence) | **Implemented** |
| Clock manipulation | Signed-clock-wrap → modular delta with tolerance + kick as explicit policy (decided) | Decided (ADR-0011 §3) |
| God-mode / fake stats | Server-computed HP/points + server-clock cooldowns (1250 ms attack interval enforced) | **Implemented** (combat); buffs pending F5 |
| Dupe / rollback | Single-writer per region + one-tx Batcher ≤100 ms + idempotent WAL replay + audit same-tx | **Implemented** (foundation); item-ACID (materials→result→gold one commit) pending F5; 2 non-idempotent plain-INSERT paths (`safebox size==1`, `messenger.add`) pending |
| Floods / DoS | Per-connection token buckets (CG_CHAT/CG_ITEM_MOVE/CG_ATTACK/CG_MOVE) — legacy has none | Pending F5 |
| SQL injection | Domain repos only, prepared statements, no direct SQL at runtime | **Implemented** |
| Movement through walls | Walkability from PG tile attributes (`IsMovablePosition`) | Pending F5 |
| Farm bots | Behavioral telemetry (movement/combat/economy patterns over log schema) — a differentiator | Pending F5.4/F6 |

## 6. Technology stack (2026)

| Layer | Choice |
|---|---|
| Language | Rust, edition 2024, `unsafe_code = "forbid"`, toolchain 1.97.0 |
| Async | tokio 1.x (rt-multi-thread, net, io-util, time, sync, macros) |
| Entities | **bevy_ecs standalone** (`default-features = false`) — adopted (ADR-0010); same ecosystem as the F7 client (bevy + Slint) |
| Database | PostgreSQL 18 (PGDG) — single canonical store |
| DB access | **tokio-postgres 0.7** (ADR-0008); pool later via deadpool-postgres if measured |
| Quests | Own declarative DSL (no scripting runtime — no mlua ever) |
| Config | TOML (ADR-0004); config-rs + clap enter when the binary needs args (deferred) |
| Observability | tracing + metrics (Prometheus/Grafana) — from F5 |
| Tests | cargo test + proptest + golden tests (real captures) + parity harness |

**Rejected with justification:** CockroachDB (license), TiDB/ScyllaDB (multi-node unnecessary), SurrealDB (document + immature), libSQL/Turso (on pause), SQLite/redb (embedded — break shared DB), TimescaleDB (ADR-0001, only if logs prove it), mlua (own DSL), actors (ADR-0010), "let the DB compute" (logic in SQL = anti-pattern).

## 7. The plan — phases F0–F7

| Phase | Goal | Verifiable milestone | Status |
|---|---|---|---|
| **F0 Foundations** | Workspace + ADRs + byte-exact `protocol` crate + capture harness | One real captured LOGIN3 parses + re-serializes identically | **DONE** (30/30 at close; 81 attrs by 08-12; golden `auth_login3_40999.bin` 88B) |
| **F1 Network** | tokio listener (result>0/EAGAIN semantics), framing, handshake with retries | C++ auth ↔ Rust peer, no floods | **DONE** (F1.6 MET 2026-08-10; 28 attrs; debt: retry-nonce rationale, partial-echo test) |
| **G-PG Cutover** (gate before F2) | PG 18 provisioned; schema/data migration; legacy adapter; behavior unchanged | F2 unblock checklist | **COMPLETE** (2026-08-10; ADR-0005 gate 4/4; B1–B8; parity A/B green; real login on PG) |
| **F2a Auth slice** | AUTH role on PG: LOGIN3, hash (`"*"+UPPER(SHA1(UNHEX(SHA1(pw))))` — asterisk IS part of the format), GC_AUTH_SUCCESS, dwLoginKey, `protocol::legacy` 151–153, timeout | Login against Rust auth on PG + C++ db | **DONE** (2026-08-10; hybrid stack, real client to select screen; 140/140 at close). Debt: `dwLoginKey` real flow |
| **F2b Client batch 1** | Version check, hardware ID, server time (≤1 week each) | Recompiled client passes the version check | **DONE** (2026-08-11; 88B LOGIN3; 99999 rejected, 68B backward-compatible) |
| **F3 Data layer + data channel** | `database` by domain on PG; port by QID (login → player → items → social); pull packets 162+ | **Redefined 2026-08-12 (review H.4):** ported QIDs behave identically on PG via the Rust `database` crate (parity harness — the C++ game operates on PG through `mysql_proxy`, never the Rust crate); pull channel 162+ active; client receives additive data w/o desync | **IN PROGRESS** — account/world repos + WAL phase 2 done; pending: items/social QIDs, active data channel, PROTO_FROM_DB |
| **F4 World entry + names** | CG_PLAYER_SELECT, spawn, map, stats; UTF-8 name overrides | Real client enters the world against the Rust core with correct names | **Milestone MET** (2026-08-11, 50+ s sustained). Tail: client UTF-8 overrides (partially done); ECS **decided** (ADR-0010 Accepted) but **implementation pending** — `MobCache → bevy World` is the next slice per ADR-0010:148-150 (bevy_ecs not yet in `Cargo.toml`) |
| **F5 Gameplay + scale** | Movement, combat, drops, items, NPCs, quests, chat, shops, safebox, trade, GM; **ECS migration** (ADR-0010); channel list from auth; config via manifest; **scale benchmark** (N bots × N regions) + benchmark instrumentation | Full game session without observable divergence (defined real-client session script: login → kill → loot → stack → equip → potion → death → revive → warp) + benchmark passed | **IN PROGRESS** — F5.3 slices 1–17 + **ECS migration DONE** + **benchmark harness DONE** (2026-08-13, tail §8); **next: spawn dinámico (shared World), walkability; full benchmark at the F5 milestone**; Slint standalone → F7, REST/metrics → post-cutover (review 2026-08-12) |
| **F6 Full parity** | Automated side-by-side (same input → diff), instance cutover, remove legacy adapter (ADR-0005) + verify final data migration (backup/restore, Patroni) | Rust server replaces the C++ one without client changes | Planned |
| **F7 Client** | Rust client (**bevy + Slint** — decided 2026-08-12), new protocol, real encryption; delete `protocol::legacy` (ADR-0006) + wire debt D1–D6 | — | Future |

## 8. F5.3 gameplay slices — status and tail

**17 slices DONE (2026-08-12, per-crate `cargo test` green + clippy clean):** s1 kill rewards + chat + client locale cache · s2 item drops + pickup · s3 NPC AI aggro + chase + GC_MOVE · s4 mob attack in range · s5 PC death + revive · s6 warp-to-city + de-aggro · s7 idle patrol · s8 stacking pickup · s9 player DEF in mob damage · s10 proactive aggro + `aggressive_sight` · s11 potions + latent framer fix (16→4 B) · s12 item move/stack/split · s13 equip/unequip · s14 equipped items affect combat · s15 ComputeParts · s16 FindEquipCell · s17 weapon attack_speed (1250/625 ms parity).

**Tail (order reviewed 2026-08-12 — H.1; ECS confirmed by the user — mob density is the core requirement):**

1. **ECS migration slice — DONE (2026-08-13, 39th part)** — `MobCache` → bevy World: components Position/Hp/Aggro/Mob/Item (+Vid/Player), resources Tick/Rand/NpcOutbox/SpawnCache, systems chase_attack/aggro_detect/patrol (chained, parity order), `WorldSim` wrapper; `game_core/src/ecs.rs` (986 lines); channel.rs refactored (AI tick → `world.update`). Workspace **359 passed / 0 failed**, clippy clean, release green, deployed. Accepted deviations: `multi_threaded` at the F5 benchmark; SpawnCache as Arc\<Mutex\> resource; armor at entry/equip/unequip. **Spawn dinámico DONE (2026-08-13, 40th part):** channel-level shared World — `spawn_despawn_system` (SPAWN_VIEW 2500 materialize, DESPAWN_RADIUS 4000 — ecs.rs:449 — combat mobs never despawn), entry via `Intent::Join` mpsc (ecs.rs:337-340, Veloren pattern), no static SPAWN_VIEW filter in channel.rs, bench_capture wired (4/4 call sites).
2. **Provisional N-bot benchmark — FIRST LOAD SIGNAL DONE (2026-08-13, 41st part)** — new wire-level bot simulator slice: **`bench_bot` crate** (27 attrs, live-PG green; smoke 1 bot world_ms 1524 / 12 mobs / no kick; 2/2 concurrent; `--cleanup-accounts` 0 rows, psql-verified) + `server_realms` `--bench-capture <dir>` (bench_capture.rs; wired 4/4 call sites with spawn dinámico). **Report `logs/bench-run-2026-08-13.md` (vs the 06:11 binary on auth7/chan7):** 5×30 s → 5/5 OK (world_ms median 2254); **20×60 s → 20/20 OK, 0 failures, 0 panics** (median 7829, p95 8214, ~13 % spread). **FINDING: spawn materialization reaches only first-comers under concurrency (19/20 bots saw 0 mobs) — fix IN PROGRESS (channel lane); the envelope rejects the 300 u/s harness walk — harness fix in progress; world_ms 1.5→7.8 s (1→20 bots) — per-connection entry bottleneck candidate.** Full ladder pending before the F5 milestone: WorldSim::metrics (tick duration + per-tick CPU export), spawn-visibility interpretation, harness walk ≤ ~250 u/s, sharded-region case + defined failure path, then 100/250/500/1000 bots (validates the adopted ECS core, 2–5× CPU headroom, AI tick ≪ 500 ms).
3. Walkability (`IsMovablePosition`) + per-entity speed envelope — **DONE (2026-08-13, 40th part, fix-2 lane):** NEW `game_core/src/map.rs` (553 lines — server_attr LZO1X parsing, real map 41 decoded: 2,176,848 blocked + 1,076,378 water cells; exact C++ cell arithmetic SECTREE_SIZE 6400 / CELL_SIZE 50, units→cell = (x%6400)/50; `MapStore` lazy + cached failures) + `movement.rs` (`PlayerMotion.speed` 300 u/s default; `MoveError::ExceedsEnvelope` movement.rs:53-67; envelope = speed×(dt+100ms)/1000×1.20, inert without anchor); channel CG_MOVE validates walkability BEFORE `process_move` (reject → stands, no ban). **Closes the last P0 anti-hack hole (ADR-0011).** Follow-ups: N-violations auto-ban (config), path-line sampling (diagonal corner-cut), buffs/mounts recompute speed, warp re-anchor.
4. The 2 non-idempotent WAL paths (`safebox size==1`, `messenger.add`) — they live inside the durability guarantee until fixed (ADR-0011 dupe row). — **DONE (2026-08-13, fix-2):** both INSERT paths now `ON CONFLICT DO NOTHING` on natural PKs (legacy quirks preserved); `replay_wal` PG test UN-GATED, 2/2 green (2.34 s) against live PG.
5. `dw_arrow` (quiver) → skills (+ server-timed buffs — **DONE 2026-08-13, 41st part:** `game_core/src/skill.rs` 732 lines/15 tests; ecs `SkillTable`/`process_skill`/`affects_system`/`Affects` 19 tests; `CG_USE_SKILL` 52/9 B + `GC_AFFECT_ADD`/`GC_AFFECT_REMOVE`; 481 passed / 0 failed, binary 3,935,232 B live; **GAPs: SPLASH/PARTY/HORSE, `skill_power.txt`, buff numeric application, quest-granted/passive**) → interactive NPCs/shops → quests (**DSL core + qc→DSL converter DONE 2026-08-13, 41st part — `quest_dsl` 44 tests; corpus 194/194 ~2 s; 5,513 unmapped → Rust-module territory (spec §8); 6 family proposals = 112/194 files; family parameter extraction + Value-as-Expr IN PROGRESS**) → safebox → trade (**un-gate the `replay_wal` PG test BEFORE trade/safebox** — it is the untested crash path of the anti-dupe guarantee — **MET 2026-08-13: un-gated, 2/2 green**) → GM.

**Structural refactor (wave 6, 2026-08-13, 42nd part):** crate **`realm` → `game_core`** (user decision — `source/reforge/game_core/`; 78 code refs; workspace **512 passed / 0 failed**; **N1 trap guard**: `game_core/src/ecs/systems/{social,quest}.rs` + `server_realms/src/channel/{social,quest}.rs` — empty-sub-enum delegates). channel.rs split → `server_realms/src/channel/` (13 files; `Session` + `Outcome` + dispatch one-liners); ecs.rs split → `game_core/src/ecs/` (components/events/resources/systems + `Intent`/`NpcEvent` wrapper sub-enums). PG migrations: `scripts/gpg/alter_gold_check.sql` (`CHECK gold>=0` on 3 wallet tables; `money_log` excluded) + `migrate_guild_tables.sql` (guild_member/grade/comment — closes the 41st-part gap). Quest similarity engine (spec §9.3; quest_dsl 66 tests). **Deploy note: servers still run the pre-refactor binary (3,935,232 B) — redeploy pending.**

**Wave 7 (2026-08-13, 43rd part):** **(1) NPC shops + player trade DONE** — `game_core/src/shop.rs` (406 lines, buy/sell parity shop.cpp:166-180 / shop_manager.cpp:297-319) + `trade.rs` (370 lines: TradeSession 12-cap, gold once, 2-phase accept, one-tx commit via `exchange_mutated` + `Batcher::flush()`, ids 100M-200M) + `ecs/systems/social.rs` ShopTable + `channel/{shop,trade}.rs` wire byte-exact (GC_SHOP 1888 B / GC_EXCHANGE 47 B) + `WorldStore::exchange`; workspace 548 passed. **(2) Quest runtime engine DONE** — `game_core/src/quest/engine.rs` (859 lines: `{quest}.__status` 1-based parity questpc.cpp:115-118, wait/select suspension, conditions + actions subset, `player.quest` persistence 0 = DELETE; GC_SCRIPT 45 / CG_SCRIPT_ANSWER 29); workspace 564 passed; pending actions: say_reward, send_letter, set_quest_state, target_vid, affect_*, input_number. **(3) GM commands DONE (subset, fix-4)** — `gm.rs` + `channel/gm.rs` (`/` prefix parity input_main.cpp:661-665, per-command gmlist permission check; warp/item/notice/level; mob spawn deferred — new intent; locale pending). **(4) dw_arrow DONE** (orchestrator direct) — `USE_ARROW_DAMAGE` arrow gate (char_battle.cpp:2919-2941), `consume_arrow` + `pending_arrow_shot` (session.rs:237), real `dw_arrow` count in ADDITIONAL_INFO (packets.rs:311-333); cargo check 4.52 s, game_core 154 + server_realms 42 passed 0 failed. **(5) Background subagents ENABLED** (`OPENCODE_EXPERIMENTAL_BACKGROUND_SUBAGENTS=true` in opencode.jsonc — restart needed). **Deploy note: servers still run the pre-wave-7 binary (3,935,232 B) — redeploy pending.** Remaining chain: quest pending actions, auction/unified trade, GM extensions, then the benchmark ladder (WorldSim::metrics, sharded-region, 100→1000 bots), data channel 162/163, locale wire slice, manifest/hot reload, auto-ban.

**Gate:** real-client E2E smoke every N slices (defined session script, see F5 milestone) — since F4 (2026-08-11) the gameplay slices have zero real-client verification; unit-green + clippy does not cover client rendering/interpolation of GC_MOVE/GC_DAMAGE_INFO/parts.

Then: full scale benchmark (validates bevy_ecs, 1,000+ players/instance, ≥2–5× CPU headroom, AI tick processing ≪ 500 ms interval). **Slint standalone → F7** and **REST/metrics → post-cutover** (deferred 2026-08-12, review H.3 — Slint in F5 would target the legacy wire and be re-ported at F7; REST has no F5 consumer; benchmark instrumentation stays in F5).

## 9. Locale redesign (plan summary — `docs/plans/locale-redesign.md`)

- **One table per text domain** (see §4.6), rows per (entity, language), human-readable; EN fallback.
- **One wire pair** `CG_LOCALE_REQUEST`/`GC_LOCALE`, stateless, chunked if needed; served by auth at connect under a loading screen; re-request anytime = hot reload (language change live, content rename in SQL visible next request). ⚠️ **Header numbers must be pinned before the wire slice** — 162/163 are already taken by the datachannel pair (verify free headers 162–207 list before allocating).
- **Client:** CPythonLocale cache module (5 touchpoints: `PythonApplication.cpp:859-948`, `PythonNetworkStreamPhaseGameActor.cpp:132-172`, `uiscriptlocale.py:62-63`, `uimapnameshower.py:158-166`); fallback cache → pack → empty. Map names: image kept (single set) + localized text overlay.
- **Phases:** F0 importer DONE · F1 fetch + names (wire + loading screen + cache) NEXT · F2 UI + maps + selector · F3 chat + hot reload (retires the 16 `locale_string_XX.txt` + fixed-ES quest texts) · F4 content delivery (updater over the game connection — no launcher).
- **Future workflow:** rename mob = one UPDATE; new language = INSERTs; new item = proto row + name/desc rows; no pack, no rebuild.

## 10. Quest DSL (spec: `docs/reference/quests/quest-dsl.md`, Proposed)

- **No Lua** (legacy: Lua 5.0 with EUC-KR 2-byte lexer; 194 `.quest` files, ~2,500 duplicated lines in `collect_quest_lv30..lv96` alone).
- **Own declarative DSL**: indentation-significant, `quest`/`state`/`on`/`->` structure, typed catalog of triggers/conditions/actions (no escape to free code); families + blocks + imports eliminate duplication; automatic converter (qc → DSL) + **parity harness** (same inputs → same final state + dialog output).
- Special coordination cases (oxevent, christmas_*, event flags) → Rust server modules, not DSL growth.
- **Open decisions:** `between` syntax, `if` depth (1 level + else), `select` capture, key naming, file extension, explicit `timer` trigger.

## 11. What is not ported (deliberate simplifications)

- Repeated code: near-identical raid events → **one encounter framework configurable by data**.
- Legacy encryption: plaintext only while the C++ client lives; real encryption with F7.
- CP949 encoding → UTF-8 (except boot data referenced by name, kept byte-compatible).
- Lua and all scripting. game↔db state duplication (ADR-0002). Boot order (only PG first).

**CP949 hard rules (from real failures — AGENTS.md §15/§17, restated for reviewers):**
1. Server locale lua files containing Korean MUST be **CP949/EUC-KR (2 bytes/char), NOT UTF-8** — the server's lua 5.0 lexer breaks on UTF-8 (parity misalignment → `unfinished string` → `LoadQuestLocale` fails → `locale.monster_chat` never defined).
2. `item_proto` names in PostgreSQL MUST stay **original CP949** — the boot drop files (`etc_drop_item.txt`, `common_drop_item.txt`, `drop_item_group.txt`) reference items BY CP949 NAME and the core aborts boot (`No such an item` → `cannot load ETCDropItem`) if they are missing. Visible item names come from the client/locale tables; server proto names are never touched. (`mob_proto` names CAN be changed — mobs are not referenced by name in boot.)

## 12. Risks and mitigations

| # | Risk | Mitigation |
|---|---|---|
| 1 | Byte-exact protocol parity (build-flag sizes) | Golden tests from real captures in every phase; single `protocol` crate; F6 side-by-side |
| 2 | Translating `char.cpp` god object | ADR-0010 before F4; systems over minimal entity; side-by-side validation |
| 3 | Quest corpus conversion (194 files) | Own DSL + automatic converter + parity harness; stragglers → direct Rust |
| 4 | Infinite monolith scope | Agreed feature set: core first, events deferred |
| 5 | Fragile verification env (4 GB RAM, WSL crashes) | Scripted verification from F0; minimal runtime; start_m2_min.sh |
| 6 | Cross-channel coordination in PG (latency vs cache) | Explicit contracts + benchmark before GuildManager/LoginData port |
| 7 | G-PG cutover risk | Data-comparison harness; adapter temporary by contract; C++ source untouched |
| 8 | Client-side data dependency (pack as text source) | Manifest + delta + locale tables (ADR-0009); pack → visuals only |

## 13. Open decisions / questions for reviewers

1. **Quest DSL**: `between` syntax; `if` depth; `select` capture; `@key` vs literal keys; `.quest`/`.qdsl`/`.mq` naming; explicit `timer` trigger.
2. **F7 client**: bevy + Slint decided; new protocol, real encryption — no detail until F6 (own ADRs then).
3. **Cross-server regions**: physical location of special-region processes (next to the central DB proposed).
4. **Unified trade**: bid closing with the DB clock (principle decided; auction details in F5).
5. **License**: MPL-2.0 confirmed in `Cargo.toml`; community confirmation still pending.
6. **Web API + metrics**: **RESOLVED 2026-08-12 (review H.3)** — REST/metrics deferred post-cutover (no F5 consumer); benchmark instrumentation stays in F5. **Slint standalone: RESOLVED — deferred to F7** (in F5 it would target the legacy wire and be re-ported at F7 — double protocol work, no server-side unblock; ADR-0007 amended). Docker removed (user decision 2026-08-11).
7. **`dwLoginKey` real flow** (F2a debt): tokenized sessions — password currently re-sent on reconnects.
8. **Capture harness** (F0 debt): extend golden fixtures beyond LOGIN3.
9. **NPC motion data** (pre-existing): `mob_proto.folder=''` for 20000+ custom NPCs (1144 races) — partial fix; folder audit pending.
10. **GitHub push**: local 53 commits ahead of origin/main (verified 2026-08-12) — push backlog pending user decision; the unstable 4 GB host holds the ONLY copy (history + PG data + client).
11. **Balance redesign**: full formula redesign → F6 ADR gated on the parity harness (ADR-0010); compatibility constants (e.g. `SPEEDHACK_LIMIT_BONUS=80`) carry an expiry.
12. **What are we missing?** (open invitation to reviewers)
13. **Schema-migration tooling**: DECIDED 2026-08-12 — plain SQL files + a small runner (no sqlx::migrate); details when the first migration after the locale tables lands.
14. **Single-region double-login**: same character logged in twice in ONE process — decide the semantics with the region entry gate (cross-region row locks are decided, §4.4).
15. **`replay_wal` gated PG test**: un-gate BEFORE the trade/safebox slices — it is the untested crash path of the anti-dupe guarantee (previously kept gated by user directive).
16. **Runtime hosting — RESOLVED (2026-08-12, ADR-0012 Accepted):** native Windows hosting decided — PG 18 + Rust auth/channel native on Windows; MariaDB archived + stopped; WSL shrinks to a minimal on-demand oracle box (frozen C++ binaries + proxy, cap 1 GB, off when unused), deleted at F6. The C++ binaries are frozen Linux ELF and never recompiled (user decision) — they can NEVER be native Windows; they only run in WSL until F6.

## 14. Dependency deferrals

- `clap` + `config-rs` → when the binary needs args/config beyond std (currently std-only main).
- `sqlx`/PgPool → not adopted; tokio-postgres + deadpool-postgres if measured (ADR-0008).
- `bevy_ecs` → **ADOPTED 2026-08-12** (ADR-0010; F5 benchmark validates).
- No `mlua` ever. `protocol` module split → done at F2 (PanamaPack/hybrid-crypt in `protocol::legacy`).
- RLS → post-WAL. Patroni failover → F5/F6. REST/metrics → post-cutover (review H.3). **Slint standalone → F7** (review H.3; ADR-0007 amended). Docker → removed (user decision).
- SHA-1 module: duplicated in `database`/`server_realms` (provenance notes); unify into a shared crate when a third consumer appears.

## 15. Repository, operations, environment facts

- **Repo:** `github.com/ryerdevs/reforge-core` (PUBLIC) — sources only (~150–200 MB); NOT in repo: `Extern/`, build artifacts, `graphify-out/`, `.opencode/`, installed client + backups (moved to `C:\projects\metin2-extra\` 2026-08-11). Binaries → Releases.
- **Two source copies rule — SUPERSEDED 2026-08-12 (ADR-0012):** the C++ server is **NEVER rebuilt** (frozen oracle — the Linux ELF binaries run only in the on-demand WSL box); `C:\projects\Metin2\source` (Windows) is the ONLY source copy; the Rust rewrite and the client live only on Windows. Historical (until 2026-08-12): `/home/m2/source` (WSL) compiled the server; sync + verify protocol defines after any change.
- **Runtime (2026-08-12, ADR-0012 — EXECUTED):** **native Windows primary** — PG 18.4 service `postgresql-metin2` :5432 (db `metin2`, role mt2/mt2), Rust auth :30001 + channel :30003 from `source\deploy\win`; client → `127.0.0.1` (serverinfo.py repacked). **WSL parity path only** (frozen C++ + `mysql_proxy` :3307): MariaDB archived + stopped (was :3306) · srv1-db :30000 · auth1 :30001/30002 · cores :30003+; boot order PG → db → auth → cores; WSL IP `172.25.104.175` — CHECK after every WSL restart.
- **Environment (2026-08-12, ADR-0012):** 4 GB host / WSL cap reduced to 1 GB, WSL 2.7.3 unstable (E_UNEXPECTED during heavy I/O; WHEA PCIe) → WSL is **on-demand only** (parity sessions) and off the rest of the time; `start_win.ps1` for the daily Windows stack; `sync` after WSL deploys; docs: keep `Last verified` fresh; graphs refreshed after relevant code changes. **RAM honesty:** native PG uses the same ~0.5 GB it used inside WSL — the win is the VM overhead/cap contention (~1.0–1.5 GB total), not PG's own memory; the 4 GB host constraint remains.
- **Backup cadence (review H.2, 2026-08-12) — LIVE:** nightly `scripts\backup_win.ps1` (native `pg_dump -Fc` of `metin2` → `C:\projects\metin2-extra\backups\metin2_<yyyy-MM-dd>.dump`, retention 7) + regular push policy (executed 2026-08-12: `origin/main = 294edb1`) — the 4 GB host holds the ONLY copy of history, PG data, WAL and the client. Data-loss risk above all feature work.

## Sources

- `ROADMAP.md` (phase tracker), `CHANGELOG.md` (chronological), `AGENTS.md` (rules + verified facts), `docs/CURRENT.md` (snapshot), `docs/plans/server-rewrite.md` (canonical design v0.3), `docs/plans/locale-redesign.md`, `docs/decisions/0001–0012`, `docs/reference/quests/quest-dsl.md`, `docs/reference/protocol/login-flow.md`, `docs/reference/protocol/legacy-compatibility.md`.
