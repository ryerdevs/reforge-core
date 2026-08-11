---
Type: Snapshot
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-10
---

# CURRENT — State of the project (snapshot, 2026-08-10)

This is a point-in-time snapshot. It is the **source of truth for current status**; refresh it at the end of every session that changes the state it describes. Historical narrative lives in [`../CHANGELOG.md`](../CHANGELOG.md); the full plan lives in [`../ROADMAP.md`](../ROADMAP.md).

## Snapshot

- **Date:** 2026-08-10
- **Commit:** `03c03ad` + working tree ahead — the G-PG cutover session changes (docs, `scripts/gpg/`, `source/reforge/mysql_proxy`, `network/examples/f16_peer.rs`) are **uncommitted**.
- **Rust rewrite workspace:** `source/reforge/` — crates `protocol`, `network`, `database`, `realm`, **`mysql_proxy`** (temporary G-PG adapter) + binary `server_realms` (flat layout, ADR-0004). Rust toolchain 1.97.0, edition 2024, `unsafe_code = "forbid"`.
- **G-PG CUTOVER COMPLETE (2026-08-10, loop):** PostgreSQL 18.4 (PGDG) is the single canonical store; the C++ baseline operates on it through `mysql_proxy` — real client login `test`/`1234` verified (character select reading from PG). MariaDB frozen as migration source. The C++ baseline remains the oracle — `source/reforge/**` is owned exclusively by the rewrite (ADR-0003).
- **Runtime state:** hybrid stack — **`server_realms --role auth` (Rust) on :30001** (replaces the C++ auth; restore = `scripts/gpg/hybrid_auth_test.sh --restore`) + C++ db/core on PostgreSQL via the proxy (`*_pg` conf variants active in `source/deploy/main/srv1`). MariaDB/PG/proxy up in WSL.
- **Docs structure:** reorganized 2026-08-10 into `plans/`, `reference/`, `history/`, `guardrails/`, `decisions/`, `explanation/` + hubs (see `docs/README.md`); superseded plans moved to `history/` (read-only). Diátaxis modes (`tutorials/`, `how-to/`) have no content yet — created on demand.

## Verified numbers

| Item | Value | Evidence |
|---|---|---|
| Workspace tests | **196 passing / 0 failed / 22 ignored** (realm 6 + 4 gated, database 34 + 11 gated, mysql_proxy 66, network 24 + 2 f16 smoke, protocol 43 + 6 datachannel + 3 golden, server_realms 14 + 4 auth smoke) | `cargo test` in `source/reforge` |
| `mysql_proxy` adapter | 53/53 — wire v10 codec, translate (§4 inventory as test table), session (per-slot search_path), config | `source/reforge/mysql_proxy/src/{wire,translate,session,config,server,sha1}.rs` |
| Parity (migration) | **30/30 tables** count+md5 equal (volatile `account.last_play` excluded — live-login write lands on PG only) | `scripts/gpg/parity_check.py` |
| Parity (boot) | A/B green — 0 new SYSERR + identical boot table lines vs the MariaDB baseline | `scripts/gpg/parity_boot.sh` |
| G-PG gate | Real client login on PostgreSQL: `LoginSuccess` 21:39:34 (core1 syslog); proxy log shows translated queries (`mysql_hash_password(...)`, `LOCALTIMESTAMP`/`EXTRACT(EPOCH ...)`) | `source/deploy/main/srv1/chan/ch1/core1/syslog`, `/tmp/gpg/proxy.log` (WSL) |
| F2a (Rust auth) | Real client login → character select through `server_realms --role auth` on PostgreSQL (hybrid stack, 2026-08-10) — auth log `login OK test key ...`, core1 `LoginSuccess` 00:03:42 | `source/deploy/main/srv1` auth on :30001, `/tmp/gpg/hybrid_auth.log` (WSL) |
| Graphs (graphify, at `b85a019`) | server **13,200 nodes / 33,251 links**; client **17,501 nodes / 39,258 links**; merged **30,701 nodes / 72,509 links** | `source/server/graphify-out/graph.json`, `source/client/graphify-out/graph.json`, `graphify-out/graph.json` |

## Current architecture decisions

1. **PostgreSQL as the one canonical database** — **IMPLEMENTED 2026-08-10**. ADR-0005 Accepted; gate closed (backlog B1–B8): PG 18.4 provisioned, phase-1 subset migrated (30 tables + 26 log DDL + `account.mysql_hash_password`), `mysql_proxy` adapter operating, parity A/B green, real client login on PG. MariaDB is the frozen migration/export source.
2. **Legacy packet compatibility stays isolated and deletable.** Compatibility structs/types are confined to a dedicated boundary so they can be removed wholesale when the legacy peers are gone — never woven into new code. **Documented as [ADR-0006](decisions/0006-legacy-wire-pack-compat-boundary.md) — Proposed.**
3. **No partial Rust code embedded in the legacy client** during F0–F6. The Rust client ships standalone (Slint in F5, wgpu in F7). **Accepted — [ADR-0007](decisions/0007-no-partial-rust-in-legacy-client.md).**
4. **Config format: TOML** — decided (ADR-0004); the proxy uses a minimal hand-rolled TOML subset (no config-rs — deferral); config-rs + clap enter at F2.

## Status by phase

| Phase | Status | Next action |
|---|---|---|
| **F0 — Foundations** | `protocol` crate complete, **30/30 tests**, F0 milestone (byte-exact LOGIN3) met at crate level | Capture harness: real packets via tcpdump/Wireshark against the C++ server in WSL |
| **F1 — Network** | Listener, framer, handshake done — **25/25 tests**; **F1.6 MET** (f16_peer ↔ live auth, no floods, 2026-08-10) | F1 milestone closed; remaining debt: retry-on-wrong-nonce rationale, partial-echo test |
| **G-PG — PostgreSQL cutover** | **COMPLETE (2026-08-10)** — ADR-0005 Accepted, B1–B8 done, real login on PG, parity A/B green | Close-out only: sync WSL/Windows source copies; optional graph refresh |
| **F2 — Auth + first client batch** | **F2a + F2b DONE (2026-08-11)** — Rust auth + client version/hwid batch, verified end-to-end with the real client (world entry) | **F3** (data layer + data channel) |
| **F3+ — Data, world, parity** | **F3 tail done (2026-08-11)** — world repos + WAL wired + snapshot harness + data channel 162+ (protocol + client contract). **F4 slice 1 done** — realm `WorldStore` + select/spawn packet mappings | **F4 slice 2 (the channel)**: listener + handshake + select flow end-to-end + spawn; details in [plans/server-rewrite.md](plans/server-rewrite.md) |

## Next gates

1. **F3** (data layer + data channel): `database` crate organized by domains (account/world/social/economy/log) + port by QID (login → player load/save → items → social) + durable pipeline (WAL + mutation_id) + client pull-based packets (headers 162+). Carry-overs: parity harness snapshot target, dwLoginKey real flow (LoginKeyStore skeleton exists), capture harness (F0), `Extern` boost `error.hpp` restore (client rebuilds).
2. **Capture harness** (F0 debt): real packet capture (tcpdump) against the C++ server in WSL as golden tests for `protocol`.
3. **NPC motion data** (pre-existing data gap, not PG): `mob_proto.folder=''` for the 20000+ custom NPCs (1144 races) — Uriel/Mirine animate wrong; fix = set folder from the client pack or an existing human folder + core restart.

## Where to look next

- Full plan with acceptance criteria per task: [`../ROADMAP.md`](../ROADMAP.md)
- Architecture decisions: [`decisions/`](decisions/) (0001 PostgreSQL, 0002 game+db unification, 0003 workspace, 0004 names/config, 0005 cutover — Accepted+implemented, 0006 compat boundary, 0007 client boundary)
- Design reference: [`plans/server-rewrite.md`](plans/server-rewrite.md)
- Wire contract: [`reference/protocol/login-flow.md`](reference/protocol/login-flow.md)
- Legacy compatibility boundary: [`reference/protocol/legacy-compatibility.md`](reference/protocol/legacy-compatibility.md)
- **G-PG database inventories:** [`reference/database/legacy-schema.md`](reference/database/legacy-schema.md) · [`reference/database/legacy-sql-compatibility.md`](reference/database/legacy-sql-compatibility.md)
- **G-PG runbook + harness:** [`scripts/gpg/`](../scripts/gpg/) — install/provision chain (02–09), `parity_check.py`, `parity_boot.sh`, `start_pg_stack.sh`
- Agent team model (roster, hierarchy, session rules): [`explanation/agent-organization.md`](explanation/agent-organization.md)
- Superseded plans/specs (read-only): [`history/`](history/)
- Change history: [`../CHANGELOG.md`](../CHANGELOG.md)
