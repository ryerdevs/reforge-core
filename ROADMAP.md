# ROADMAP — Metin2 server rewrite in Rust

> **Living plan.** This document is the project's master plan and is updated every session.
> Tracking methodology: `AGENTS.md` (rules + verified state) + `CHANGELOG.md` (chronological record) + `ROADMAP.md` (this plan) + `docs/decisions/` (ADRs).
> **Progress rule: no phase is complete without verification evidence** (rule 5 of AGENTS.md).
> **Canonical design reference:** `docs/plans/server-rewrite.md` (single-file plan). The previous single-file draft (`docs/history/2026-08-09-server-rewrite-draft.md`) is preserved as historical.

## Current state (2026-08-10)

- **C++ baseline verified:** full login working (auth + channel + character select) with the real client. Account `test` / `1234`.
- **RUST REWRITE STARTED (2026-08-10):** ADR-0003 + ADR-0004 + flat workspace `source/reforge` — `protocol` (F0: byte-exact wire, 30/30), `network` (F1: tokio + framer + handshake, 23/23), `database` (F3), `realm` (F4+) + single binary `server_realms` with `auth|channel` roles by config (3/3). **56/56 tests.** Key finding: spec §3 sizes for `TSimplePlayer` (71B packed, not 76B natural) and `TPacketGCLoginSuccess` (449B, not 474B) corrected with dual-toolchain evidence; errata in spec §7. Adversarial review (oracle): no critical findings. Legacy runtime: `source/deploy` (unchanged). Binary configs: **TOML** (decision 2026-08-10). Pending: F1.6 integration milestone (needs WSL), PanamaPack 151 + hybrid-crypt 152/153 isolated in `protocol::legacy` at F2 (ADR-0006), real capture harness (WSL).
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
- [ ] ADR: data layer (local WAL + mutation_id + RLS + failover; durable/volatile contract)
- [ ] ADR: server→client data (versioned manifest + delta + hot reload)
- [x] **Cargo workspace in `source/reforge`** (2026-08-10, flat layout — ADR-0004): crates `protocol`, `network`, `database`, `realm` + binary `server_realms` (role `auth|channel` by config) — edition 2024, resolver 3, `[workspace.dependencies]`, lints, rust-toolchain 1.97.0, `**/target/` ignored. `cargo build` OK (56/56 tests)
- [x] **Crate `protocol` implemented (2026-08-10)**: 17 packets of the login flow (spec §3) + TSimplePlayer 71B packed — zero-deps, manual LE, panic-free parsing. **30/30 tests** (golden byte vectors + roundtrips + sizes + bad-lengths). Adversarial review (oracle): no critical findings. **F0 milestone (LOGIN3 byte-exact) MET at crate level** — only the real capture harness is missing
- [ ] Verification harness: real packet capture (tcpdump/Wireshark against the C++ server) as golden tests — **pending: requires the C++ server up in WSL** (next session)
- [ ] **GitHub repository**: sources only (~150–200 MB); binaries/packs/backups to Releases or external storage; `.gitignore` for build artifacts, installed clients, graphify-out, .opencode

**F0 milestone:** one real captured LOGIN3 parses and re-serializes byte-for-byte identical.

### Phase 1 — Network and transport (IN PROGRESS 2026-08-10)

Goal: replace `libthecore` + fdwatch with tokio, with behavior parity.

> **Progress rule (alignment guardrail):** each task = code + tests + adversarial review + docs + commit. The review (oracle) verifies TWO things: (1) the code cannot be broken, and (2) what was built IS what the task says (no programming for programming's sake). A task is not marked done without its acceptance criterion verified.

- [x] **F1.1 — Crate `network` (formerly `net`) with tokio** (plan: tokio 1.x, features rt-multi-thread/net/io-util/time/sync + macros for tests). ACCEPTANCE: clean `cargo build`; `network` depends on `protocol` (edition 2024 workspace). ✓ 2026-08-10 (tokio 1.53.1, version centralized in `[workspace.dependencies]`)
- [x] **F1.2 — TCP listener** with the verified contract semantics: write and consume `result > 0` bytes; `0` = EAGAIN (backpressure); `-1` = error (fixes #1/#2/#6). ACCEPTANCE: local integration test — raw TCP client connects, sends bytes, receives response/clean close without WRITE floods. ✓ 2026-08-10 (`Connection` + `serve`, documented and proven tokio equivalence)
- [x] **F1.3 — Framing** (BYTE header + fixed-size payload, no length prefix — spec §2): client→server size table (0xff=13, 0xfe=1, 1=49, 4=34, 5=10, 6=2, 109=52, 111=65 channel/68 auth by role, 0xfc=13, **+ CG_ENTERGAME 10=1 and CG_STATE_CHECKER 206=1 added 2026-08-10 after adversarial review — F2/F4 need them**); server→client = sizes of the `protocol` crate structs. Handles split packets and multiple packets per read. ACCEPTANCE: framing tests with fragmented and concatenated packets; **unknown header → clean connection close** (parity `input.cpp:77-84`; documented deliberate divergence: 0x00 is consumed as no-op by C++, closed by the framer). ✓ 2026-08-10 (11/11 table verified against packet_info.cpp)
- [x] **F1.4 — Keepalive filtering** (spec §7 errata): `CG_TIME_SYNC` (0xfc) and `CG_PONG` (0xfe) do not break flow parsing. ACCEPTANCE: test with real sequence handshake → time sync → pong → login3 parses correctly (the original criterion's GC_PHASE C→S was corrected: 0xfd is strictly S→C, verified in packet.h + CPacketInfoCG; deviation justified by the implementer and validated adversarially). ✓ 2026-08-10
- [x] **F1.5 — Handshake** with clock-bias retries (~40–80ms, limit 32): the server sends `GC_PHASE` + `GC_HANDSHAKE`, validates the `CG_HANDSHAKE` echo and moves to the next phase (not removed: it runs once at login, zero benefit, high risk). ACCEPTANCE: correct handshake test + timeout/retry. ✓ 2026-08-10 (`network/src/handshake.rs`: nonce u32 never 0, symmetric bias ±80ms, 500ms/intent timeout, 50ms breather, keepalive 0xfc/0xfe filtering + out-of-order discard — parity input.cpp:625-626; 11 new tests → network 23/23; adversarial review: READY for F2; known debt in CHANGELOG: retry-on-wrong-nonce rationale, delta≈0 with legacy client, partial-echo test pending)
- [ ] **F1.6 — Integration milestone**: the C++ auth binary connects to a Rust peer and vice versa without timeouts or WRITE floods. REQUIRES: WSL with the C++ server up (environment) — if unavailable in the session, it is documented as deferred, not done.

**F1 milestone:** the C++ auth binary connects to a Rust peer and vice versa, without timeouts or WRITE floods.

### Phase G-PG — PostgreSQL cutover (BEFORE F2 — blocks F2)

Goal: PostgreSQL 18 becomes **the single canonical store** (ADR-0001 target) before any auth code is written; a temporary legacy compatibility adapter lets the C++ baseline operate on the **same PostgreSQL** with the legacy client behavior unchanged (ADR-0005). MariaDB is used only as the migration/export source.

- [ ] **ADR-0005 accepted** (Proposed → Accepted): PostgreSQL cutover + temporary legacy compatibility adapter; F2 gated by it
- [ ] PostgreSQL 18 provisioned (schemas per domain, per-schema permissions, RLS)
- [ ] Temporary legacy compatibility adapter: the C++ baseline (source untouched) operates on the **same PostgreSQL** through the adapter (its MySQL-speaking `libsql` is bridged by translation); legacy client behavior unchanged; removed at F6. MariaDB used only as migration/export source
- [ ] Migration groundwork: MySQL → PostgreSQL schema mapping (types/defaults/`ENUM`/`SET`/`UNSIGNED` adaptation per ADR-0001), data comparison harness
- [ ] Concrete PostgreSQL crate decision (sqlx/PgPool per ADR-0001 recommendation)

**G-PG milestone:** the Rust auth (F2) persists against PostgreSQL 18 while the C++ baseline and the legacy client run unchanged.

**Note: F2 is BLOCKED on this phase and on ADR-0005.**

### Phase 2 — Auth + first client batch (BLOCKED on G-PG + ADR-0005)

> **Gate:** F2 does not start until G-PG completes and ADR-0005 is accepted. Split per the 2026-08-10 plan reorder: **F2a** = server-side auth slice; **F2b** = client batch 1 (additive, ≤1 week each, ADR-0007).

**F2a — server-side auth (Rust: `network::auth` module + `server_realms --role auth`):**

- [ ] Flow: `GC_PHASE` + `GC_HANDSHAKE` → `CG_HANDSHAKE` echo → `LOGIN3` (65 bytes: `0x6F` + name[31] + pwd[17] + keys[16])
- [ ] Hash verification: **`mysql5_password` = `"*" + UPPER(SHA1(UNHEX(SHA1(pw))))`** — the asterisk is part of the format (fixes #5/#11); legacy-hash parity kept only for the compatibility window
- [ ] `GC_AUTH_SUCCESS` (0x96 + key + result)
- [ ] Serverside (no client change): validate `dwLoginKey` (LOGIN_BY_KEY) — no cleartext password on reconnects (tokenized sessions)
- [ ] Global connection timeout in the auth (F1.5 debt: a silent connection lives up to 17.6s — CHANGELOG 2026-08-10 3rd part)

**F2b — client batch 1 (additive C++ changes, ≤1 week each, ADR-0007):**

- [ ] Version check on connect (clean reject; gates protocol evolution)
- [ ] Hardware ID in LOGIN3 (hardware bans, anti-multibox)
- [ ] Server time (timers consistent with the server clock)

**Compatibility packets (isolated — ADR-0006):** PanamaPack (151, 289B) + hybrid-crypt (152/153) are implemented only inside `protocol::legacy` — never in the new wire core. Boundary documented in `docs/reference/protocol/legacy-compatibility.md`; the whole layer is deleted at the new client (F7).

**F2 milestone:** login against the Rust auth on PostgreSQL (F2a) + the recompiled client passes the version check (F2b).

### Phase 3 — Data layer + data channel

Goal: `database` crate organized by domains behind a backend trait + porting onto PostgreSQL (G-PG already done) + pull-based data packets in the client.

- [ ] Crate `database` organized by domain modules: account/world/social/economy/log (separate PG schemas, per-schema permissions, RLS) — **PostgreSQL-only after G-PG** (no MariaDB backend; MariaDB is only the migration/export source)
- [ ] Backend: `postgres` (sqlx candidate — concrete crate decided at G-PG/F3); no `direct-sql` backend
- [ ] Port by QID: login → player load/save → items → social
- [ ] Durable pipeline: **local WAL per region + `mutation_id` + batch ≤100ms + idempotent replay** (`ON CONFLICT DO NOTHING`)
- [ ] SQL routing: `SQL_ACCOUNT` vs `SQL_PLAYER` (fix #8); `QUERY_LOGIN` 13 columns (fix #7) — ported semantics on PostgreSQL
- [ ] Data comparison harness extended to all ported QIDs (groundwork from G-PG)
- [ ] **Client: additive pull-based packets** (headers 162+: CG_QUERY/GC_RESPONSE; table registration + case in PhaseLogin) — the data channel §5.6
- [ ] `PROTO_FROM_DB` maintained

**F3 milestone:** the C++ game runs against the Rust `database` without behavior changes; the recompiled client receives additive data without desynchronizing.

### Phase 4 — World entry + names

Goal: character select + spawn with parity + UTF-8 name overrides.

- [ ] `CG_PLAYER_SELECT` (header 6) → `GC_LOGIN_SUCCESS3`
- [ ] Character spawn, map (`Venter_the_east.mp3`), stats
- [ ] **Client: in-memory overrides** (new override API to be added around `CPythonNonPlayer`/`CItemData` after `LoadLocaleData` — no `SetLocaleName`/`SetItemLocaleName` exist in the legacy client; they must be written first) — the server sends UTF-8 names from the DB; goodbye mojibake and the CP949 trap
- [ ] Entities: minimal Entity core + ECS systems (bevy_ecs standalone) — NEVER port char.cpp as a single class

**F4 milestone:** the real client enters the world against the Rust core with correct names.

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
- [ ] REST API + metrics (Prometheus/Grafana) + Docker (first-class features)

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
- **NOT in the repo**: `client\` (installed client, 2.1 GB pack), `client-om2\` (downloaded reference), `archive\` (backups), `Extern\` (dependencies), build artifacts (obj/bin/Debug/Release ~2.4 GB), `graphify-out\`, `.opencode\`, `systems\`.
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
