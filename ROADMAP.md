# ROADMAP — Metin2 server rewrite in Rust

> **Living plan.** This document is the project's master plan and is updated every session.
> Tracking methodology: `AGENTS.md` (rules + verified state) + `CHANGELOG.md` (chronological record) + `ROADMAP.md` (this plan) + `docs/decisions/` (ADRs).
> **Progress rule: no phase is complete without verification evidence** (rule 5 of AGENTS.md).
> **Canonical design reference:** `docs/plans/server-rewrite.md` (single-file plan). The previous single-file draft (`docs/history/2026-08-09-server-rewrite-draft.md`) is preserved as historical.

## Current state (2026-08-12)

- **F5.3 SLICE 7 — PATRULLAJE DE MOBS IDLE (2026-08-12, 16th part):** implemented directly by the orchestrator (no delegation). `realm::ai::patrol_step` (pure, parity `UpdateState` IDLE — `char_state.cpp:668-688`): probability 1/7 per tick, random direction 0..359°, step 300-700 UNITS, destination clamped to the spawn radius (documented) + 4 tests; `LiveNpc` + `home_x/home_y` + `nomove` (parity `AIFLAG_NOMOVE`); AI tick patrol branch — visible idle mobs (≤ 2 500 units, sectree parity) walk with their `move_speed`, max 20 `GC_MOVE`/tick (no flood). Verified: workspace green (excluding pre-existing cold-start-flaky `f16_peer_smoke`), clippy clean. Pending: walkability (`IsMovablePosition`), `aggressive_sight` data-driven, player-DEF in mob damage (`char.cpp:2113-2114`), multicast.
- **F5.3 SLICE 6 — WARP A LA CIUDAD + DE-AGGRO (2026-08-12, 15th part):** implemented directly by the orchestrator (no delegation). `protocol::TPacketGCWarp` (15 B, header 65 — `packet.h:1381-1388`); revive `CG_SCRIPT_ANSWER` answer==1 → hp/mp to maxima + save + `GC_WARP` (destino exit_x/y o village 969600/278400, addr/port del canal) — el cliente reconecta con el flujo DirectEnter completo (F4); de-aggro por distancia en el AI tick (umbral 5 000 units — el mob hostil abandona la persecución; data-driven con `aggressive_sight` pendiente). Verified: workspace green (excluding pre-existing cold-start-flaky `f16_peer_smoke`), clippy clean. Pending: `aggressive_sight` data-driven, player-DEF in mob damage (`char.cpp:2113-2114`), patrol/states (`ai_flag`) (DONE 16th part), multicast.
- **F5.3 SLICE 5 — PC DEATH + REVIVE (2026-08-12, 14th part):** implemented directly by the orchestrator (no delegation). Mob attack subtracts from `row.hp` with NO floor; `hp <= 0` → `GC_DEAD` (14) + `GC_POINTS` 0 + save (client shows the death screen). `CG_SCRIPT_ANSWER` (29, 2 B) revives with `RestartAtSamePos` parity (`cmd_general.cpp:534` + `char.cpp:838-873`): hp/mp to subset maxima (`compute_max_points`) + `GC_CHARACTER_DEL` + ADD + INFO + `GC_POINTS` + save. Verified: workspace green (excluding pre-existing cold-start-flaky `f16_peer_smoke`), clippy clean. Pending: warp-to-city revive (DONE 15th part), player-DEF in mob damage, patrol/states (`ai_flag`), de-aggro by distance (DONE 15th part), multicast.
- **F5.3 SLICE 4 — EL MOB ATACA EN RANGO (2026-08-12, 13th part):** implemented directly by the orchestrator (no delegation). `MobRow.damage_min/max` (mob_proto); `realm::ai::attack_damage` (`number(min,max)` inclusive, pure — min==max fixed, min>max defensive, no player-DEF subtraction yet: `char.cpp:2113-2114` pending); channel AI tick: aggro mob **in range** (parity `melee_max_range`) now ATTACKS — `GC_MOVE(FUNC_ATTACK)` (parity `char_state.cpp:386`) + `GC_DAMAGE_INFO` + damage to `row.hp` + `GC_POINTS` (bar) + durable save; out of range keeps chasing. Verified: workspace green (excluding the pre-existing cold-start-flaky `f16_peer_smoke` — passes 2/2 on second run), clippy clean. Pending: player-DEF in mob damage, PC death/respawn (DONE 14th part), patrol/states (`ai_flag`), de-aggro by distance, multicast.
- **F5.3 SLICE 3 — NPC AI: AGGRO + PERSECUCIÓN + BROADCAST GC_MOVE (2026-08-12, 12th part):** implemented directly by the orchestrator (no delegation). `protocol::TPacketGCMove` (24 B, header 3 — `Packet.h:1912-1923` + `EncodeMovePacket` `char.cpp:825-836`); `MobRow.move_speed` (mob_proto, UNITS/seg); new `realm::ai` module — `step_toward` (normalized step, clamp to target; **speed 0 = no movement — unit test caught the teleport bug**) + `rotation_5deg` (bRot, cardinal verified) + 5 tests; channel: mobs become hostile on damage (`aggro`) and an AI tick (500 ms) moves them toward the player broadcasting `GC_MOVE` (dwTime/dwDuration for client interpolation). Verified: workspace green (realm 47/47, protocol 73/73), clippy clean. Pending: mob attack in range (FUNC_ATTACK), patrol/states (`ai_flag`), de-aggro by distance, multicast.
- **F3 PHASE 2 — WAL LOCAL A DISCO + REPLAY (2026-08-12, 11th part):** implemented directly by the orchestrator (no delegation, per fix-3's 8-point review spec). `database::wal::WalSink` durable-first: each batch persists to `{wal_dir}/{uuidv7}.wal` (JSONL + sync_all) BEFORE applying to PG; the file is deleted ONLY post-COMMIT; on error it stays for the next-boot `replay_wal` (pure fn, one batch per file, one tx + audit, idempotent). Inverse JSON parser without serde (round-trip exact for Text/Bytes/Int/Null — `\x` escaped = valid JSON). Idempotency audit documented (5 wired paths idempotent; 2 plain-INSERT paths NOT wired: `safebox.set_size` size==1, `messenger.add`). `WorldStore::new`/`with_audit_table` rebuild the same WAL→Batcher→PG wiring (never silently disabled); replay ONCE per process (`OnceLock`); dir = env `REALM_WAL_DIR` or `./wal`. Verified: workspace tests green (database 48/48 incl. 4 new WAL tests), clippy no new warnings. Pending: gated `replay_wal` PG test (not run — user directive), pre-existing wal.rs clippy (2), social/economy/log stubs (fix-3: correct answer).
- **F5.3 SLICE 2 — ITEM DROPS + PICKUP (2026-08-12, 10th part):** implemented directly by the orchestrator (no delegation). Mob kills now drop the primary item (`mob_proto.drop_item` × `drop_rate` config) on the ground (`GC_ITEM_GROUND_ADD` 58 B with `ENABLE_ITEM_GROUND_EX` + `GC_ITEM_OWNERSHIP` with the player name); `CG_ITEM_PICKUP` validates distance ≤ 600 (parity `CItem::DistanceValid`), finds the first free inventory cell (0..90, `INVENTORY_MAX_NUM`), sends `GC_ITEM_SET` + `GC_ITEM_GROUND_DEL` and persists with `ItemRepo::upsert` (id from `ITEM_ID_RANGE` 100M-200M). Verified: workspace tests green, clippy clean on touched files. Pending: `etc_drop_item`/`common_drop_item` tables (CP949-name TRAP), item stacking, ownership expiry, multicast.
- **F5.3 SLICE 1 — KILL REWARDS + CHAT + CLIENT LOCALE CACHE (2026-08-12, 9th part):** implemented directly by the orchestrator (no delegation — the fixer lanes returned reviews instead of code). **Rust:** mob kills now award exp/gold (`kill_reward` pure fn, rates from config), level-up loop (exp_table via CommonRepo), re-sent `GC_POINTS`, durable save (Batcher); chat `CG_CHAT`→`GC_CHAT` echo (framer variable-size + handler); `MobRow` + exp/gold_min/gold_max. **Client C++:** the 4 F1 patches from the fix-2 audit applied — `CItemData` locale name provider (pack fallback), `CPythonLocale::Utf8ToDisplay` (UTF-8→codepage, kills the "JabalÃ" mojibake), empty-bundle no-disconnect, `CPythonNonPlayer::GetName` cache-first. Client rebuilt (5,128,192 B, SHA 26DC9FDD) and deployed. Verified: `cargo test --workspace` green, clippy no new warnings, MSBuild 0 errors (no gated PG tests — user directive). Next: NPC AI, movement broadcast, WAL local-disk+replay (fix-3 8-point spec).
- **F0 CLOSED + F1 IMPORTER LIVE (2026-08-12):** spawn-resolve perf fix VERIFIED end-to-end from WSL (`channel_pg` 6/6; `entry + 23033 spawns en 12.2 s` vs the previous 3–4 min stall); test bug fixed (`channel_pg.rs:887` inverted logic — the 2 s timeout is the SUCCESS case); F0 cleanup done (user-approved: `DROP SCHEMA world` debris + `scripts/gpg/migrate_spawns.py`). **ADR-0009 written** (server-side locale, Proposed) + design closed in `docs/plans/locale-redesign.md` (8 `common.*` locale tables + `world.maps`/`world.spawns`, EN fallback, hot reload, language selector). **New crate `locale_import`** (one subcommand per domain, idempotent, reuses the verified `realm::npc` parser) — schema applied (`scripts/gpg/f1-locale-world-schema.sql`) and **live data verified by orchestrator**: mob_names 8,628 (es/en/de × 2,876), item_names 34,281, item_descriptions 22,674, ui_texts 3,903, message_texts 12,489 (16 langs), `world.maps` 65, `world.spawns` 145,876 (map 41: 10,026 entries / Σ 23,033 — parity with `map41_spawns.rs`). Workspace all green, clippy clean. Gaps documented: `item_icons` (EPK extraction, parked for panel), `map_names` (no text source — images). **NEXT: F1 wire slice** (`CG_LOCALE_REQUEST`/`GC_LOCALE` + client cache — **client side DONE 2026-08-12 9th part: cache integration + UTF-8 conversion + item provider; server auth side already implemented**).
- **G-PG CUTOVER COMPLETE + F1.6 VERIFIED (2026-08-10, loop):** ADR-0005 Accepted (gate 4/4); PostgreSQL 18.4 (PGDG) on WSL Debian-M2 — db `metin2`, schemas account/player/common/log; phase-1 subset migrated (30 tables + 26 log DDL + `account.mysql_hash_password` pgcrypto, parity_check 30/30); `mysql_proxy` adapter (`source/reforge/mysql_proxy`, wire v10 + translate + session, 53 tests — 4 gate bugs fixed) — **the C++ baseline boots and serves the REAL client on PostgreSQL: `test`/`1234` → character select, `LoginSuccess` 21:39:34, boot parity A/B green vs the MariaDB baseline; MariaDB frozen as migration source (srv1 runtime now on PG via the proxy, conf variants `*_pg`)**. F1.6 transport verified (`f16_peer` ↔ live auth, no floods). **F2a UNBLOCKED** (first slice: auth over PG; pending there: sqlx/PgPool decision, crate gaps 22P02/42703/22021).
- **C++ baseline verified:** full login working (auth + channel + character select) with the real client. Account `test` / `1234`.
- **RUST REWRITE STARTED (2026-08-10):** ADR-0003 + ADR-0004 + flat workspace `source/reforge` — `protocol` (F0: byte-exact wire, 30/30), `network` (F1: tokio + framer + handshake, 23/23), `database` (F3), `realm` (F4+) + single binary `server_realms` with `auth|channel` roles by config (3/3). **56/56 tests.** Key finding: spec §3 sizes for `TSimplePlayer` (71B packed, not 76B natural) and `TPacketGCLoginSuccess` (449B, not 474B) corrected with dual-toolchain evidence; errata in spec §7. Adversarial review (oracle): no critical findings. Legacy runtime: `source/deploy` (unchanged). Binary configs: **TOML** (decision 2026-08-10). Pending: PanamaPack 151 + hybrid-crypt 152/153 isolated in `protocol::legacy` at F2 (ADR-0006), real capture harness (WSL), crate gaps 22P02/42703/22021 + sqlx/PgPool decision (F2a).
- **PLAN REORDER (2026-08-10):** **G-PG (PostgreSQL cutover) comes before F2** — **one canonical PostgreSQL** (no dual-store; MariaDB used only as migration/export source; the C++ baseline operates on the same PG through the temporary adapter); F2 is split into **F2a** (server-side auth) / **F2b** (client batch 1) and is **blocked until the PostgreSQL cutover + ADR-0005**; compatibility packets (PanamaPack 151/289B, hybrid-crypt 152/153) are **isolated in `protocol::legacy`** (ADR-0006) and deleted at the new client; **no partial Rust embedded in the legacy client during F0–F6** (ADR-0007, accepted — the already-agreed boundary); dependency deferrals documented (clap/config-rs → F2, sqlx → G-PG/F3, bevy_ecs → F4).
- **WORLD-ENTRY CRASH — CLOSED (2026-08-09):** root cause in the client — heap over-read in `string_replace_word` (PythonSkill.cpp:62). 2-line fix deployed (`metin2client.exe` 5,115,904 B, 14:12, hash C7EAD7CC) + garbage coordinates fixed (`UPDATE player SET x=969600, y=278400`). **Closed by field test 2/2 (2026-08-09):** two consecutive world entries with the recovered characters. Details in AGENTS.md and CHANGELOG.
- **Language System 1.2.6:** integrated and loading (16 languages, 764–775 entries each). Server-side text gaps A+B+C and the 181 missing keys — **superseded by the new design** (server→client texts by manifest, plan §5.6).
- **Unified rewrite plan written (2026-08-09):** `docs/plans/server-rewrite.md` — architecture, anti-hack, DB, quest DSL, migration, regional channels, modifiable client. 12 open questions for external reviewers.
- **Legacy vs 2026-standards audit completed:** 14 P0/P1/P2 decisions NOT carried over (with file:line evidence) + 7 things done right and kept. Estimated gain: 2–5x CPU, 1,000+ players/instance ceiling.
- **Quests: own DSL DECIDED (no Lua):** spec in `docs/reference/quests/quest-dsl.md` (integrated in the plan §11). Families + blocks + imports remove the ~2,500 duplicated lines of the 194-quest corpus.
- **Client: ≤1 week per change effort rule** (nothing forbidden; cost/benefit). 7 additive modifications identified with evidence (version check, hardware ID, server time, dwLoginKey, pull packets 162+, UTF-8 overrides, channel list from auth + config via manifest).
- Graphify graphs (2026-08-10): server **13,200 nodes / 33,251 edges**, client **17,501 nodes / 39,258 edges**, merged **30,701 / 72,509** (refresh after code changes).
- C++ baseline pending: verify crash fix, review 17 pre-existing boot SYSERRs.

## Rewrite principles

1. **Do more with less**: less code, less complexity, fewer dependencies; quality comes from what is necessary.
2. **Structural redesign, not line-by-line translation** (ADR-0001).
3. **Server-authoritative**: the client sends intentions, the server computes facts; the client is a view, never a source of truth.
4. **The DB does not compute, it guarantees**: game logic lives in Rust; PostgreSQL enforces integrity (constraints, transactions, locks, RLS, audit).
5. **Verifiable incremental replacement** (strangler fig): each Rust module preserves the observable behavior of its C++ counterpart and passes verification (parity harness) before moving on.
6. **Client frozen as contract during F0–F6**, with one exception: additive changes ≤1 week that unblock the server side (cost/benefit rule, not prohibition — ADR-0007).
7. **Parity only where it matters**: observable behavior is preserved; internal code need not look the same.
8. **ADR before implementing**: domain boundaries, data ownership, protocols, concurrency, failures and migration are decided in writing first.
9. **Hot reload by design**: texts, items, quests and config are edited in the DB and reloaded at runtime (NOTIFY + manifest) — no restarts, no recompiles.

## Phases

### Phase 0 — Foundations (workspace, ADRs, protocol) — DONE (2026-08-10)

Goal: Rust workspace skeleton + architecture decisions closed by ADR + protocol crate with the verified login flow.

- [x] **ADR-0002: unify `game` + `db`** (ACCEPTED: one process per region, db as crate; legacy shim during F3–F5, unification in F6)
- [x] Rust stack researched and fixed: **tokio 1.49 + bevy_ecs standalone + config-rs + clap 4.6 + tracing + proptest**; **sqlx 0.9 (PgPool) as candidate** — the concrete DB crate decision is a G-PG task (ADR-0001 left it undecided) (no mlua — quests in own DSL; regions + ECS, not actors)
- [x] Crate `protocol`: **byte-exact login-flow spec completed** (`docs/reference/protocol/login-flow.md` — supersedes the 2026-08-08 wire-protocol spec draft)
- [x] **Unified plan written** (`docs/plans/server-rewrite.md` — original draft `docs/history/2026-08-09-server-rewrite-draft.md` preserved as historical)
- [x] Legacy audit complete (plan §3.3)
- [x] **ADR-0003: Rust workspace in `source/reforge`** (location, layout, policies, ownership boundary — 2026-08-10; partially superseded by ADR-0004: flat layout + names)
- [x] **ADR-0004: structure and names** (2026-08-10): flat layout `protocol`/`network`/`database`/`realm` + single binary `server_realms` with roles (auth|channel by config), `[workspace.dependencies]` + lint `unsafe_code = "forbid"`, rust-toolchain 1.97.0, legacy runtime `source/deploy` (unchanged)
- [ ] ADR: domain boundaries and data ownership (char.cpp split into systems over a minimal Entity in ECS)
- [ ] ADR: concurrency (regions + ECS; world task never awaits SQL inline)
- [ ] ADR: quest engine (own DSL, no scripting)
- [ ] ADR: anti-hack model (server-authoritative + envelope + transactions)
- [ ] ADR: regional channels (central DB + one process per region; anti-double-login with row locks)
- [x] ADR: data layer (local WAL + mutation_id + RLS + failover; durable/volatile contract) — **ADR-0008 Accepted 2026-08-11** (tokio-postgres decided; WAL/RLS/failover deferred with phases)
- [ ] ADR: server→client data (versioned manifest + delta + hot reload)
- [x] **Cargo workspace in `source/reforge`** (2026-08-10, flat layout — ADR-0004): crates `protocol`, `network`, `database`, `realm` + binary `server_realms` (role `auth|channel` by config) — edition 2024, resolver 3, `[workspace.dependencies]`, lints, rust-toolchain 1.97.0, `**/target/` ignored. `cargo build` OK (56/56 tests)
- [x] **Crate `protocol` implemented (2026-08-10)**: 17 packets of the login flow (spec §3) + TSimplePlayer 71B packed — zero-deps, manual LE, panic-free parsing. **30/30 tests** (golden byte vectors + roundtrips + sizes + bad-lengths). Adversarial review (oracle): no critical findings. **F0 milestone (LOGIN3 byte-exact) MET at crate level** — only the real capture harness is missing
- [x] Verification harness: real packet capture (tcpdump/Wireshark against the C++ server) as golden tests — **MET 2026-08-11: `scripts/gpg/capture_auth.sh` + `extract_pcap_login3.py` → golden fixture `protocol/tests/golden/auth_login3_40999.bin` (88B, md5 6a93aa8f) + `golden_auth.rs` — the real captured LOGIN3 parses and re-serializes byte-for-byte identical**
- [ ] **GitHub repository**: sources only (~150–200 MB); binaries/packs/backups to Releases or external storage; `.gitignore` for build artifacts, installed clients, graphify-out, .opencode

**F0 milestone:** one real captured LOGIN3 parses and re-serializes byte-for-byte identical. — **MET 2026-08-11 (golden capture, 88B auth LOGIN3 with version+hwid).**

### Phase 1 — Network and transport (IN PROGRESS 2026-08-10)

Goal: replace `libthecore` + fdwatch with tokio, with behavior parity.

> **Progress rule (alignment guardrail):** each task = code + tests + adversarial review + docs + commit. The review (oracle) verifies TWO things: (1) the code cannot be broken, and (2) what was built IS what the task says (no programming for programming's sake). A task is not marked done without its acceptance criterion verified.

- [x] **F1.1 — Crate `network` (formerly `net`) with tokio** (plan: tokio 1.x, features rt-multi-thread/net/io-util/time/sync + macros for tests). ACCEPTANCE: clean `cargo build`; `network` depends on `protocol` (edition 2024 workspace). ✓ 2026-08-10 (tokio 1.53.1, version centralized in `[workspace.dependencies]`)
- [x] **F1.2 — TCP listener** with the verified contract semantics: write and consume `result > 0` bytes; `0` = EAGAIN (backpressure); `-1` = error (fixes #1/#2/#6). ACCEPTANCE: local integration test — raw TCP client connects, sends bytes, receives response/clean close without WRITE floods. ✓ 2026-08-10 (`Connection` + `serve`, documented and proven tokio equivalence)
- [x] **F1.3 — Framing** (BYTE header + fixed-size payload, no length prefix — spec §2): client→server size table (0xff=13, 0xfe=1, 1=49, 4=34, 5=10, 6=2, 109=52, 111=65 channel/68 auth by role, 0xfc=13, **+ CG_ENTERGAME 10=1 and CG_STATE_CHECKER 206=1 added 2026-08-10 after adversarial review — F2/F4 need them**); server→client = sizes of the `protocol` crate structs. Handles split packets and multiple packets per read. ACCEPTANCE: framing tests with fragmented and concatenated packets; **unknown header → clean connection close** (parity `input.cpp:77-84`; documented deliberate divergence: 0x00 is consumed as no-op by C++, closed by the framer). ✓ 2026-08-10 (11/11 table verified against packet_info.cpp)
- [x] **F1.4 — Keepalive filtering** (spec §7 errata): `CG_TIME_SYNC` (0xfc) and `CG_PONG` (0xfe) do not break flow parsing. ACCEPTANCE: test with real sequence handshake → time sync → pong → login3 parses correctly (the original criterion's GC_PHASE C→S was corrected: 0xfd is strictly S→C, verified in packet.h + CPacketInfoCG; deviation justified by the implementer and validated adversarially). ✓ 2026-08-10
- [x] **F1.5 — Handshake** with clock-bias retries (~40–80ms, limit 32): the server sends `GC_PHASE` + `GC_HANDSHAKE`, validates the `CG_HANDSHAKE` echo and moves to the next phase (not removed: it runs once at login, zero benefit, high risk). ACCEPTANCE: correct handshake test + timeout/retry. ✓ 2026-08-10 (`network/src/handshake.rs`: nonce u32 never 0, symmetric bias ±80ms, 500ms/intent timeout, 50ms breather, keepalive 0xfc/0xfe filtering + out-of-order discard — parity input.cpp:625-626; 11 new tests → network 23/23; adversarial review: READY for F2; known debt in CHANGELOG: retry-on-wrong-nonce rationale, delta≈0 with legacy client, partial-echo test pending)
- [x] **F1.6 — Integration milestone**: the C++ auth binary connects to a Rust peer and vice versa without timeouts or WRITE floods. REQUIRES: WSL with the C++ server up (environment) — if unavailable in the session, it is documented as deferred, not done. **VERIFIED 2026-08-10 (loop):** `network/examples/f16_peer` ↔ auth C++ live (`172.25.104.175:30001`) — `GC_PHASE` + `GC_HANDSHAKE` (clock-aligned echo, lDelta=0) → handshake completed, no timeouts, no WRITE floods; workspace 111/111.

**F1 milestone:** the C++ auth binary connects to a Rust peer and vice versa, without timeouts or WRITE floods.

### Phase G-PG — PostgreSQL cutover (BEFORE F2 — blocks F2)

Goal: PostgreSQL 18 becomes **the single canonical store** (ADR-0001 target) before any auth code is written; a temporary legacy compatibility adapter lets the C++ baseline operate on the **same PostgreSQL** with the legacy client behavior unchanged (ADR-0005). MariaDB is used only as the migration/export source.

- [x] **ADR-0005 accepted (Accepted 2026-08-10; gate checklist 4/4; backlog B1-B8 all done)**** (Proposed → Accepted): PostgreSQL cutover + temporary legacy compatibility adapter; F2 gated by it
- [x] PostgreSQL 18 provisioned (18.4 PGDG on Debian-M2 2026-08-10: db metin2, 4 schemas, role mt2; RLS deferred) (schemas per domain, per-schema permissions, RLS)
- [x] Temporary legacy compatibility adapter (mysql_proxy - REAL client login on PG verified 2026-08-10): the C++ baseline (source untouched) operates on the **same PostgreSQL** through the adapter (its MySQL-speaking `libsql` is bridged by translation); legacy client behavior unchanged; removed at F6. MariaDB used only as migration/export source
- [x] Migration groundwork (30 tables + 26 log DDL + account.mysql_hash_password fn; parity_check 30/30): MySQL → PostgreSQL schema mapping (types/defaults/`ENUM`/`SET`/`UNSIGNED` adaptation per ADR-0001), data comparison harness
- [ ] Concrete PostgreSQL crate decision (sqlx/PgPool per ADR-0001 recommendation)

**G-PG milestone:** the Rust auth (F2) persists against PostgreSQL 18 while the C++ baseline and the legacy client run unchanged. — **C++/legacy-client half MET 2026-08-10 (real login on PG); Rust-auth persistence = F2a.**

**Note: F2 is BLOCKED on this phase and on ADR-0005.** — **UNBLOCKED 2026-08-10: G-PG complete (B1–B8), ADR-0005 Accepted.**

### Phase 2 — Auth + first client batch (UNBLOCKED 2026-08-10 — G-PG complete + ADR-0005 accepted)

> **Gate:** F2 does not start until G-PG completes and ADR-0005 is accepted. Split per the 2026-08-10 plan reorder: **F2a** = server-side auth slice; **F2b** = client batch 1 (additive, ≤1 week each, ADR-0007).

**F2a — server-side auth (Rust: `server_realms --role auth`):**

> **IMPLEMENTED + VERIFIED (2026-08-10):** Rust auth serving REAL client logins on PostgreSQL (select screen reached — hybrid stack: Rust auth :30001, C++ channel :30003); 140/140 tests; tokio-postgres decided for F2a (documented in `auth.rs`); `GC_PHASE(PHASE_AUTH)` after the handshake echo (the client sends LOGIN3 only on it); `protocol::legacy` (ADR-0006 Accepted) 151-153 implemented (runtime-file conditional, parity with C++).

- [x] Flow: `GC_PHASE` + `GC_HANDSHAKE` → `CG_HANDSHAKE` echo → `GC_PHASE(PHASE_AUTH)` → `LOGIN3` (68 bytes to auth: `0x6F` + name[31] + pwd[17] + keys[16] + lang[3])
- [x] Hash verification: **`mysql5_password` = `"*" + UPPER(SHA1(UNHEX(SHA1(pw))))`** — the asterisk is part of the format (fixes #5/#11); legacy-hash parity kept only for the compatibility window
- [x] `GC_AUTH_SUCCESS` (0x96 + key + result)
- [ ] Serverside (no client change): validate `dwLoginKey` (LOGIN_BY_KEY) — no cleartext password on reconnects (tokenized sessions) — *skeleton `LoginKeyStore` done 2026-08-10; the real flow still re-sends the password (AGENTS.md §14)*
- [x] Global connection timeout in the auth (F1.5 debt: a silent connection lives up to 17.6s — CHANGELOG 2026-08-10 3rd part) — *15s timeout implemented and observed firing in the hybrid test*

**F2b — client batch 1 (additive C++ changes, ≤1 week each, ADR-0007):**

- [x] Version check on connect (clean reject; gates protocol evolution) — *2026-08-11: gate in the Rust auth (`expected_version=40999`); clean close on mismatch; verified with the new client + f16_peer (99999 → reject, 68B → compat)*
- [x] Hardware ID in LOGIN3 (hardware bans, anti-multibox) — *2026-08-11: 88B auth LOGIN3 with MachineGuid hwid (Hwid.h); stored in `account.hwid` on PG; verified end-to-end*
- [x] Server time (timers consistent with the server clock) — *verified already working (handshake `ELTimer_SetServerMSec` alignment + `GC_TIME` at world entry); no change needed — recon 2026-08-11*

**Compatibility packets (isolated — ADR-0006):** PanamaPack (151, 289B) + hybrid-crypt (152/153) are implemented only inside `protocol::legacy` — never in the new wire core. Boundary documented in `docs/reference/protocol/legacy-compatibility.md`; the whole layer is deleted at the new client (F7).

**F2 milestone:** login against the Rust auth on PostgreSQL (F2a) + the recompiled client passes the version check (F2b). — **F2a half MET 2026-08-10** (real client login → select screen through the Rust auth on PostgreSQL); F2b (client version check) pending.

### Phase 3 — Data layer + data channel

Goal: `database` crate organized by domains behind a backend trait + porting onto PostgreSQL (G-PG already done) + pull-based data packets in the client.

- [ ] Crate `database` organized by domain modules: account/world/social/economy/log (separate PG schemas, per-schema permissions, RLS) — **PostgreSQL-only after G-PG** (no MariaDB backend; MariaDB is only the migration/export source) — *started 2026-08-11: `account` domain (`AccountRepo::login`/`set_lang`/`set_hwid`, 7 unit + 2 gated integration tests 2/2 vs real PG); world/social/economy/log stubs; auth migration to the repo pending*
- [x] Backend: `postgres` — **tokio-postgres 0.7 decided (ADR-0008, 2026-08-11)**: proven end-to-end here (auth serving real clients, proxy); 0 new deps; contract complete (transactions, LISTEN/NOTIFY, prepared). sqlx deferred to the WAL phase with measurements; pool later via deadpool-postgres without a driver change. No `direct-sql` backend.
- [ ] Port by QID: login → player load/save → items → social — *login (account) + player load/save (world) ported 2026-08-11; items/social next*
- [x] Durable pipeline: **local WAL per region + `mutation_id` + batch ≤100ms + idempotent replay** (`ON CONFLICT DO NOTHING`) — *2026-08-11: `database/src/wal.rs` (uuidv7 + Batcher ≤100ms one-tx + idempotent replay + audit same-tx; integration 2/2 vs real PG; DDL exported, not applied); realm wiring + local replay after crash pending*
- [ ] SQL routing: `SQL_ACCOUNT` vs `SQL_PLAYER` (fix #8); `QUERY_LOGIN` 13 columns (fix #7) — ported semantics on PostgreSQL
- [x] Data comparison harness extended to all ported QIDs (groundwork from G-PG) — *snapshot mode 2026-08-11: `--make-snapshot` (cutover reference) + `--snapshot` (PG vs reference, deterministic; 27 OK / 4 operational DIFFs post-cleanup)*
- [x] **Client: additive pull-based packets** (headers 162+: CG_QUERY/GC_RESPONSE; table registration + case in PhaseLogin) — the data channel §5.6 — *2026-08-11: `protocol::datachannel` (162/163 minimal wire) + client PhaseLogin contract registration (inert — framing map pending with the channel activation)*
- [ ] `PROTO_FROM_DB` maintained

**F3 milestone:** the C++ game runs against the Rust `database` without behavior changes; the recompiled client receives additive data without desynchronizing.

### Phase 4 — World entry + names

> **Slice 1 DONE (2026-08-11):** `realm` `WorldStore` (list/select/save via Batcher) + byte-exact select/spawn packet mappings (TSimplePlayer 71B, GC_LOGIN_SUCCESS_NEWSLOT 449B, GCCharacterAdd 37B, AdditionalInfo 70B — C++ file:line contracts; GAPs documented). **Slice 2 (the channel) is next:** listener + channel handshake/LOGIN3 + select flow end-to-end + spawn.

Goal: character select + spawn with parity + UTF-8 name overrides.

- [ ] `CG_PLAYER_SELECT` (header 6) → `GC_LOGIN_SUCCESS3`
- [ ] Character spawn, map (`Venter_the_east.mp3`), stats
- [ ] **Client: in-memory overrides** (new override API to be added around `CPythonNonPlayer`/`CItemData` after `LoadLocaleData` — no `SetLocaleName`/`SetItemLocaleName` exist in the legacy client; they must be written first) — the server sends UTF-8 names from the DB; goodbye mojibake and the CP949 trap
- [ ] Entities: minimal Entity core + ECS systems (bevy_ecs standalone) — NEVER port char.cpp as a single class

**F4 milestone:** the real client enters the world against the Rust core with correct names. — **MET 2026-08-11** (world entry + sustained session through the Rust channel: select → DirectEnter → loading → map 41 with the character, 50+ s; world empty — NPCs are F5; names from the client's pack).

### Phase 5 — Basic gameplay + scale

Goal: playable core by domains, side-by-side, scale benchmark, and the rest of the client.

- [ ] Movement: per-entity speed envelope + map walkability + correction (anti-speedhack) + lag tolerance
- [ ] Combat: full server-side damage + server-clock cooldowns + range/LoS
- [ ] Drops, items, inventory: atomic transactions (materials → result → gold in one commit)
- [ ] NPCs, quests (DSL engine + automatic corpus converter + parity harness), chat, shops, safebox, trade, GM
- [ ] **Client: channel list from the auth** (override of serverinfo.py — goodbye baked IP)
- [ ] **Client: config via manifest** (rates, visible limits — tuning without recompiling)
- [ ] Hot reload operational: NOTIFY → reload → manifest bump → delta
- [ ] **Slint standalone** (login/select/HUD UI against the real server, in parallel — reused in F7; standalone per ADR-0007)
- [ ] Scale benchmark: N bots × N regions (gate before considering multi-process)
- [ ] REST API + metrics (Prometheus/Grafana) — **Docker: REMOVED from the plan (2026-08-11, user decision — no containerization for now; the docker-development skill stays available if revisited)**

**F5 milestone:** a full game session with no observable divergence + benchmark passed.

### Phase 6 — Full parity and integration

- [ ] Automated side-by-side: same packet input → diff of Rust vs C++ responses
- [ ] Golden test suite extended to all traffic of a real session
- [ ] Complete verified data migration (backup/restore; Patroni failover)
- [ ] Final replacement: `srv1` instances running 100% Rust
- [ ] Removal: legacy compatibility adapter (ADR-0005) + `protocol::legacy` (ADR-0006) deleted at replacement

**F6 milestone:** the Rust server replaces the C++ one in test production without client changes.

### Phase 7 — Client (after the server)

> Open decisions are resolved by their own ADRs. The UI is designed in Slint (standalone app from F5, integrated as texture into the new client). The client is rebuilt with wgpu; the existing Slint UI is reused (the `.slint` files survive). Per ADR-0007, nothing Rust is embedded in the legacy client during F0–F6 — the new client is standalone.

- [ ] Rust client (wgpu), new protocol, real encryption
- [ ] Integrated Slint UI (login → select → HUD — the F5 standalone work is kept)
- [ ] Legacy client limits (24 chars, 5 characters, stack 200) revisable with the new client
- [ ] Pack formats: only the tools are preserved (PackMakerLite, TEA/LZO, DumpProto) if reused
- [ ] Delete `protocol::legacy` (ADR-0006) — nothing legacy survives in the new wire

## Dependency deferrals (2026-08-10)

Ponytail rule: dependencies enter only when the phase requires them.

- `clap` + `config-rs` → **F2** (binary args/config; `server_realms` main is std-only today)
- `sqlx`/PgPool → **G-PG / F3** (`database` crate)
- `bevy_ecs` → **F4** (`realm`)
- No `mlua` ever — quests use the own DSL (decided)
- `protocol` module split → **F2**, with PanamaPack/hybrid-crypt under `protocol::legacy` (ADR-0004 consequence, ADR-0006)

## Open decisions (for ADRs and reviewers)

1. **Quest DSL** (spec §11): native `between`; `if` 1 level + else; `select` with `as` capture; `@key` vs literal keys; `.quest`/`.qdsl`/`.mq` naming; explicit `timer` trigger.
2. **F7 client**: engine (wgpu), new protocol, encryption — no detail until F6.
3. **Cross-server regions**: physical location of special-region processes (next to the central DB proposed).
4. **Unified trade**: bid closing with the DB clock (principle decided; auction details in F5).
5. **License**: MPL-2.0 proposed (AGPL repels pserver operators) — confirm with the community.
6. **Web API + metrics timing**: from F5 (proposed) vs after the cutover.
7. **ADR-0005 and ADR-0006 are Proposed** (PostgreSQL cutover + legacy compat boundary) — pending review/confirmation. ADR-0007 is Accepted for the already-agreed boundary only.

## GitHub repository (preparation)

- **Sources only to the repo** (~150–200 MB): `source\server`, `source\client` (no build artifacts), `source\tools\pack` (no .epk), `source\tools` (includes `proto\`), `scripts\`, `docs\`, `AGENTS.md`, `ROADMAP.md`, `CHANGELOG.md`.
- **NOT in the repo**: `Extern\` (dependencies), build artifacts (obj/bin/Debug/Release ~2.4 GB), `graphify-out\`, `.opencode\`, `systems\`.
- **Outside the repo since 2026-08-11** (cleanup): installed `client\` (2.2 GB) and `archive\` backups (1.6 GB) → `C:\projects\metin2-extra\`; `client-om2\` deleted. Rust build artifacts stay in `source\reforge\target` (gitignored).
- **Binaries** (installed client, .epk, builds) → GitHub Releases (does not count against the repo limit) or external storage; generated by the build scripts.
- Root `.gitignore` with all the above patterns before the first push.

## How the count is kept

- **`docs/README.md`** — documentation index (entry point to all docs).
- **`docs/CURRENT.md`** — current verified state of the project.
- **`docs/DOCUMENTATION.md`** — documentation rules and workflow (Keep a Changelog, ADR template, graph workflow).
- **`docs/decisions/`** — ADRs. Every architecture decision is written BEFORE implementation.
- **`docs/plans/server-rewrite.md`** — the canonical design reference (single file).
- **`docs/history/`** — superseded/historical plans, specs and status docs (nothing is deleted; old `docs/superpowers/` content is indexed there).
- **`docs/guardrails/`** — lessons and rules not to repeat (index + 5 files, each rule with Rule/Why/Evidence/Consequence/Status).
- **`docs/reference/`** — protocol, quests and compatibility reference (`docs/reference/protocol/login-flow.md`, `docs/reference/protocol/legacy-compatibility.md`, `docs/reference/quests/quest-dsl.md`).
- **Graphs** — `graphify update` on `source\server` and `source\client` after relevant code changes; re-merge to the root (`graphify merge-graphs server client --out graphify-out\graph.json`).
