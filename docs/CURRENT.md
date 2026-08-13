---
Type: Snapshot
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-12
---

# CURRENT — State of the project (snapshot, 2026-08-12)

This is a point-in-time snapshot. It is the **source of truth for current status**; refresh it at the end of every session that changes the state it describes. Historical narrative lives in [`../CHANGELOG.md`](../CHANGELOG.md); the full plan lives in [`../ROADMAP.md`](../ROADMAP.md).

## Snapshot

- **Date:** 2026-08-12
- **Commit:** `d6d80d3` (HEAD, 2026-08-12, verified) + working tree ahead — F5.3/WAL/locale/plan work since the last commit may be uncommitted.
- **Rust rewrite workspace:** `source/reforge/` — crates `protocol`, `network`, `database`, `realm`, **`mysql_proxy`** (temporary G-PG adapter, removed at F6), **`locale_import`** (F1 locale importer, ADR-0009) + binary `server_realms` (flat layout, ADR-0004). Rust toolchain 1.97.0, edition 2024, `unsafe_code = "forbid"`.
- **G-PG CUTOVER COMPLETE (2026-08-10, loop):** PostgreSQL 18.4 (PGDG) is the single canonical store; the C++ baseline operates on it through `mysql_proxy` — real client login `test`/`1234` verified (character select reading from PG). MariaDB frozen as migration source. The C++ baseline remains the oracle — `source/reforge/**` is owned exclusively by the rewrite (ADR-0003).
- **Runtime state:** hybrid stack — **`server_realms --role auth` (Rust) on :30001** (replaces the C++ auth; restore = `scripts/gpg/hybrid_auth_test.sh --restore`) + C++ db/core on PostgreSQL via the proxy (`*_pg` conf variants active in `source/deploy/main/srv1`). MariaDB/PG/proxy up in WSL.
- **Docs structure:** reorganized 2026-08-10 into `plans/`, `reference/`, `history/`, `guardrails/`, `decisions/`, `explanation/` + hubs (see `docs/README.md`); superseded plans moved to `history/` (read-only). Diátaxis modes (`tutorials/`, `how-to/`) have no content yet — created on demand.

## Verified numbers

| Item | Value | Evidence |
|---|---|---|
| Workspace tests | **371 test attributes** counted 2026-08-12 — protocol 81, network 28, database 70, realm 64, server_realms 42, mysql_proxy 67, locale_import 19 (unit + integration + gated). Last full verified run: **227 passing / 0 failed / 31 ignored** (2026-08-11) — superseded by the F5.3 slices (08-12 per-crate runs green) | `#[test]`/`#[tokio::test]` scan in `source/reforge` (2026-08-12) |
| `mysql_proxy` adapter | 67 tests (counted 2026-08-12; 53/53 at the G-PG gate 2026-08-10) — wire v10 codec, translate (§4 inventory as test table), session (per-slot search_path), config | `source/reforge/mysql_proxy/src/{wire,translate,session,config,server,sha1}.rs` |
| Parity (migration) | **30/30 tables** count+md5 equal (volatile `account.last_play` excluded — live-login write lands on PG only) | `scripts/gpg/parity_check.py` |
| Parity (boot) | A/B green — 0 new SYSERR + identical boot table lines vs the MariaDB baseline | `scripts/gpg/parity_boot.sh` |
| G-PG gate | Real client login on PostgreSQL: `LoginSuccess` 21:39:34 (core1 syslog); proxy log shows translated queries (`mysql_hash_password(...)`, `LOCALTIMESTAMP`/`EXTRACT(EPOCH ...)`) | `source/deploy/main/srv1/chan/ch1/core1/syslog`, `/tmp/gpg/proxy.log` (WSL) |
| F2a (Rust auth) | Real client login → character select through `server_realms --role auth` on PostgreSQL (hybrid stack, 2026-08-10) — auth log `login OK test key ...`, core1 `LoginSuccess` 00:03:42 | `source/deploy/main/srv1` auth on :30001, `/tmp/gpg/hybrid_auth.log` (WSL) |
| Graphs (graphify, at `b85a019`) | server **13,200 nodes / 33,251 links**; client **17,501 nodes / 39,258 links**; merged **30,701 nodes / 72,509 links** | `source/server/graphify-out/graph.json`, `source/client/graphify-out/graph.json`, `graphify-out/graph.json` |

## Current architecture decisions

1. **PostgreSQL as the one canonical database** — **IMPLEMENTED 2026-08-10**. ADR-0005 Accepted; gate closed (backlog B1–B8): PG 18.4 provisioned, phase-1 subset migrated (30 tables + 26 log DDL + `account.mysql_hash_password`), `mysql_proxy` adapter operating, parity A/B green, real client login on PG. MariaDB is the frozen migration/export source.
2. **Legacy packet compatibility stays isolated and deletable.** Compatibility structs/types are confined to a dedicated boundary so they can be removed wholesale when the legacy peers are gone — never woven into new code. **Accepted + implemented — [ADR-0006](decisions/0006-legacy-wire-pack-compat-boundary.md)** (`protocol::legacy` 151–153 in F2a 2026-08-10; deleted at F7).
3. **No partial Rust code embedded in the legacy client** during F0–F6. The Rust client ships standalone (Slint in F5→**deferred to F7** (review 2026-08-12), bevy in F7). **Accepted — [ADR-0007](decisions/0007-no-partial-rust-in-legacy-client.md).**
4. **Config format: TOML** — decided (ADR-0004); the proxy uses a minimal hand-rolled TOML subset (no config-rs — deferral); config-rs + clap enter at F2.
5. **Data layer driver + durability** — **ADR-0008 Accepted 2026-08-11**: tokio-postgres 0.7 as the `database` driver (sqlx not adopted — WAL phase done 2026-08-12); PostgreSQL-only repos, no direct-sql backend; durable = **save-by-event** (event → Batcher ≤100 ms → local WAL → PG, replay idempotent — implemented 2026-08-12, F3 phase 2); RLS post-WAL; Patroni F5/F6.
6. **Server-side locale** — **ADR-0009 Accepted (2026-08-12)**: the server owns all text per player language (one `common.*` table per domain, `CG_LOCALE_REQUEST`/`GC_LOCALE`, EN fallback); importer live (`locale_import`, 2026-08-12), wire slice pending.
7. **Domain boundaries + ECS** — **ADR-0010 Accepted (2026-08-12)**: pure-function domain modules + **bevy_ecs World** (user decision — mob density is the core requirement; the migration slice `MobCache → World` is next) + per-connection session state + WorldStore; translator-vs-core boundary; wire debt D1–D6 (F7 deletion list).
8. **Anti-hack** — **ADR-0011 Accepted (2026-08-12)**: always-on controls ratified (speedhack/teleport/unknown-header/timeout/server-clock cooldowns), signed clock wrap decided, pending controls phased (walkability + speed envelope, rate limits, buffs, item-ACID, farm bots).

## Status by phase

| Phase | Status | Next action |
|---|---|---|
| **F0 — Foundations** | `protocol` crate complete — **30/30 at F0 close** (81 test attributes incl. golden/datachannel/world by 2026-08-12); F0 milestone (byte-exact LOGIN3) **MET 2026-08-11** (golden capture `auth_login3_40999.bin`, 88B) | F0 closed; pending: GitHub repository preparation (sources only) |
| **F1 — Network** | Listener, framer, handshake done — **25/25 tests**; **F1.6 MET** (f16_peer ↔ live auth, no floods, 2026-08-10) | F1 milestone closed; remaining debt: retry-on-wrong-nonce rationale, partial-echo test |
| **G-PG — PostgreSQL cutover** | **COMPLETE (2026-08-10)** — ADR-0005 Accepted, B1–B8 done, real login on PG, parity A/B green | Close-out only: sync WSL/Windows source copies; optional graph refresh |
| **F2 — Auth + first client batch** | **F2a + F2b DONE (2026-08-11)** — Rust auth + client version/hwid batch, verified end-to-end with the real client (world entry) | **F3** (data layer + data channel) |
| **F3+ — Data, world, parity** | **F3 done + F4 MILESTONE MET (2026-08-11)** — the REAL client enters the world against the Rust core and stays (select → DirectEnter → loading → map 41, 50+ s sustained; world empty — NPCs F5). World repos + WAL (phase 2 done 2026-08-12) + snapshot harness + data channel 162+ + client instrumentation (`python_error.log`) | **F5 (gameplay)**: F5.3 slices 1–17 done (2026-08-12, below); pending: movement (speed envelope + walkability), skills, interactive NPCs/shops, quests (DSL), safebox, trade, GM, benchmark, Slint; F4 tail: client UTF-8 names, ECS |

**F5.3 gameplay slices — 17 done (2026-08-12, direct orchestrator implementation; per-crate `cargo test` green + clippy clean):** s1 kill rewards + chat + client locale cache; s2 item drops + pickup; s3 NPC AI aggro + chase + `GC_MOVE`; s4 mob attack in range; s5 PC death + revive; s6 warp-to-city + de-aggro; s7 idle patrol; s8 stacking pickup; s9 player DEF in mob damage; s10 proactive aggro + `aggressive_sight`; s11 potions + latent framer fix; s12 item move/stack/split; s13 equip/unequip; s14 equipped items affect combat; s15 ComputeParts; s16 FindEquipCell; s17 weapon attack_speed. **Next (review 2026-08-12, H.1): ECS migration slice (`MobCache` → bevy World, ADR-0010:148-150) + provisional N-bot benchmark**, then walkability + speed envelope, the 2 non-idempotent WAL paths, `dw_arrow` (quiver), skills, interactive NPCs/shops, quests (DSL), safebox, trade, GM. Slint → F7, REST → post-cutover.

## Next gates

1. **F3 milestone (redefined 2026-08-12)**: port the remaining QIDs to the `database` crate (items, social), activate the pull data channel (162/163) end-to-end, maintain `PROTO_FROM_DB`. WAL phase 2 (durable-first + idempotent replay) is **DONE (2026-08-12, 11th part)**; gated `replay_wal` PG test to un-gate BEFORE trade/safebox.
2. **F5 gameplay tail (order reviewed 2026-08-12 — H.1)**: **ECS migration slice first** (`MobCache` → bevy World, ADR-0010), then **provisional N-bot benchmark** (wire-level bot simulator, sharded-region case, defined failure path), then walkability + speed envelope (anti-speedhack), the 2 non-idempotent WAL paths, `dw_arrow` (quiver), skills, interactive NPCs/shops, quests (DSL engine), safebox, trade, GM; **real-client E2E smoke every N slices**; then full scale benchmark, Slint standalone (→ F7), REST/metrics (→ post-cutover).
3. **F2a debt**: `dwLoginKey` real flow (LoginKeyStore skeleton exists — the real flow still re-sends the password, AGENTS.md §14).
4. **F4 tail**: client UTF-8 name overrides (partially done — locale cache + UTF-8 conversion 2026-08-12 9th part), minimal Entity core + ECS decision (deferred to benchmark).
5. **Capture harness** (F0 debt): LOGIN3 golden fixture DONE 2026-08-11 (`auth_login3_40999.bin` 88B); extend to more packets.
6. **NPC motion data** (pre-existing data gap, not PG): `mob_proto.folder=''` for the 20000+ custom NPCs (1144 races) — partially fixed 2026-08-11 (9 races mapped to existing folders); remaining folder audit.

## Where to look next

- Full plan with acceptance criteria per task: [`../ROADMAP.md`](../ROADMAP.md)
- **Consolidated master plan (oracle-reviewed 2026-08-12):** [`plans/master-plan.md`](plans/master-plan.md)
- Architecture decisions: [`decisions/`](decisions/) (0001 PostgreSQL, 0002 game+db unification, 0003 workspace, 0004 names/config, 0005 cutover — Accepted+implemented, 0006 compat boundary, 0007 client boundary)
- Design reference: [`plans/server-rewrite.md`](plans/server-rewrite.md)
- Wire contract: [`reference/protocol/login-flow.md`](reference/protocol/login-flow.md)
- Legacy compatibility boundary: [`reference/protocol/legacy-compatibility.md`](reference/protocol/legacy-compatibility.md)
- **G-PG database inventories:** [`reference/database/legacy-schema.md`](reference/database/legacy-schema.md) · [`reference/database/legacy-sql-compatibility.md`](reference/database/legacy-sql-compatibility.md)
- **G-PG runbook + harness:** [`scripts/gpg/`](../scripts/gpg/) — install/provision chain (02–09), `parity_check.py`, `parity_boot.sh`, `start_pg_stack.sh`
- Agent team model (roster, hierarchy, session rules): [`explanation/agent-organization.md`](explanation/agent-organization.md)
- Superseded plans/specs (read-only): [`history/`](history/)
- Change history: [`../CHANGELOG.md`](../CHANGELOG.md)
