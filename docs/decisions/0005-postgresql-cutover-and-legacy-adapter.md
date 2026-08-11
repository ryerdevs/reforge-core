---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Date: 2026-08-10
Last verified: 2026-08-10
Supersedes: —
Superseded by: —
---

# ADR-0005: PostgreSQL cutover and temporary legacy compatibility adapter

> **Status note:** Accepted (2026-08-10, G-PG design lane). The direction was fixed by the user on
> 2026-08-10; the G-PG spec in `../plans/server-rewrite.md` §8.2.1 closes the design (provision,
> migration, adapter, harness) and the implementation backlog below is what remains. The G-PG gate
> closes when the backlog (B1–B8) is executed and the parity harness is green.

## Context

ADR-0001 established PostgreSQL as the primary database of the future Rust server, while the C++ baseline keeps MySQL/MariaDB during the compatibility phase. The Rust rewrite is now approaching F2 (auth): the `protocol` (F0) and `network` (F1) crates are implemented (56/56 tests) and the auth role of `server_realms` will need persistence for accounts, sessions and `dwLoginKey` tokens.

**User decision (2026-08-10): a single canonical PostgreSQL — no dual operational databases.** The earlier formulation of this ADR (C++ stays on MariaDB while Rust runs on PG until F6) is rejected. MariaDB is used only as the migration/export source (initial data extraction), never as a second operational database of the system.

The legacy C++ server speaks MySQL through `libsql`; the legacy client (v40999) is the frozen wire contract during F0–F6 (ROADMAP principle 6, ADR-0007). Two consequences of the user decision:

- The Rust side is written against PostgreSQL from the start — no MySQL-backed Rust path (per ADR-0001, no MySQL API patterns in the new server).
- The C++ baseline must keep working unchanged during the transition, but on the **same PostgreSQL**: a temporary compatibility adapter bridges its MySQL-speaking `libsql` layer to PostgreSQL (wire/SQL translation). The C++ baseline **source** stays untouched (frozen oracle, ADR-0003); only its runtime data path goes through the adapter.

## Decision (accepted)

1. **Cut the Rust server over to PostgreSQL 18 before F2** (new phase **G-PG**, before F2 in ROADMAP). The `database` crate targets PostgreSQL from the start (sqlx/PgPool is the candidate per the ADR-0001 recommendation; the concrete crate decision is an F2a task — the **adapter** driver is decided here, item 4).
2. **A single canonical PostgreSQL** is the only operational database. A **temporary legacy compatibility adapter** lets the C++ baseline operate on that same PostgreSQL (its `libsql` speaks MySQL — the adapter translates); the legacy client behavior is unchanged. MariaDB is used **only as the migration/export source**: an initial dump/extraction to seed PostgreSQL, then it is retired. The adapter is temporary by contract — thin, explicit, removed at F6 (same rule as the ADR-0002 shim).
3. **F2 is gated by this ADR and by the G-PG cutover.** No auth work is done on a MySQL-backed Rust path; the F2a/F2b split assumes PostgreSQL underneath.
4. **G-PG design resolved (2026-08-10)** — the executable spec is `../plans/server-rewrite.md` §8.2.1; the implementation backlog is at the end of this ADR (B1–B8). Key resolutions:
   - **Adapter form (OD-1): a wire-level MySQL server protocol v10 proxy**, not a link-time shim. The C++ keeps linking `libmariadb` and connects to the proxy as if it were MySQL (`127.0.0.1:3307`) — zero C++ source or linkage change; the runtime change is conf.txt only.
   - **Adapter location and stack:** `source/reforge/mysql_proxy` (workspace member, flat layout per ADR-0004; temporary, deleted at F6). Rust, tokio + **tokio-postgres** (decided here: async, 1:1 sessions, pure Rust). No MySQL-wire dependency — the v10 codec is hand-written. No prepared statements (`CStmt` has 0 call sites — `../reference/database/legacy-sql-compatibility.md` §2.1).
   - **Migration phase 1** = the login subset: `account.account`; the `player` boot/proto set (verified in `source/server/db/src/ClientManagerBoot.cpp`); `player`/`player_index`/`item`/`quest`/`affect`/`safebox` (character load); `common.locale`/`priv_settings`/`exp_table`/`spam_db`/`gmlist`/`gmhost`; all 26 `log` tables (DDL only, empty). Type adaptation per `../reference/database/legacy-schema.md` §7 (`unsigned`→`bigint`/`numeric`, `enum`/`set`→`text`+CHECK, varbinary CP949→`bytea` byte-exact, identity `setval`). Hash: `mysql_hash_password` recreated as a PG pgcrypto function; stored hashes copied verbatim (never rehashed).
   - **Parity harness:** `scripts/gpg/parity_check.py` (per-table count + md5, MariaDB vs PG) and `scripts/gpg/parity_boot.sh` (boot syslog diff + `LoginSuccess` for `test`/`1234`).

## Alternatives considered

### Dual-store: C++ keeps running on MariaDB while the Rust server runs on PostgreSQL (until F6)

Rejected (by the user, 2026-08-10): two operational databases double the surface, the data split between stores and the migration risk concentrated at the end. The single canonical PostgreSQL removes the second store; MariaDB is reduced to a migration/export source.

### Write F2 against MariaDB and migrate to PostgreSQL later (original plan, F3)

Rejected: new code would be written against the legacy SQL API and then migrated — the ADR-0001 outcome ("no MySQL patterns in the new server") would be deferred into the middle of the gameplay port.

### Cut over at F6 (full replacement)

Rejected: F3–F5 (data layer, world entry, gameplay) would run on the legacy store, duplicating the coupling the rewrite removes, and the cutover risk concentrates at the end instead of early.

### No adapter — modify the C++ baseline to speak PostgreSQL directly

Rejected: the C++ `libsql` layer is MySQL-specific; rewiring the frozen baseline contradicts the "oracle baseline untouched" rule (ADR-0003) and would destabilize the verified login flow. The adapter keeps the baseline source intact.

### Adapter as link-time shim (mysql C API subset over libpq) — inventory OD-1 option (a)

Rejected (2026-08-10 design): a link shim requires rebuilding/re-linking the C++ binaries, the proxy does not; the proxy is fully removable at F6 with no residue in the build; the wire surface needed (handshake + `mysql_native_password` + COM_QUERY/QUIT/PING, no prepared statements) is small and well-bounded. The shim option remains documented in `../reference/database/legacy-sql-compatibility.md` §7 as the fallback if the proxy hit an insurmountable wire issue — none is known.

## Consequences

- **No dual-store**: one canonical PostgreSQL; MariaDB exists only as the migration/export source (initial data extraction), not as a second operational DB.
- The adapter is temporary by contract: thin, explicit, removed at F6 (same rule as the ADR-0002 shim). It is `source/reforge/mysql_proxy` and translates the MySQL wire/SQL of the legacy `libsql` layer to PostgreSQL without changing C++ behavior (spec §8.2.1 of `../plans/server-rewrite.md`).
- Migration tooling and a data-comparison harness are G-PG deliverables (`scripts/gpg/`); the phase-1 DDL is vendored in `scripts/gpg/schema_gpg.sql` (resolves the vendoring pending item of `../reference/database/legacy-schema.md` §8 for phase 1).
- F2 start date depends on G-PG implementation (backlog B1–B8); ROADMAP marks F2 as blocked.

## Gate (F2 unblocking checklist) — resolved 2026-08-10 (design; implementation = backlog B1–B8)

- [x] **ADR-0005 accepted (Proposed → Accepted)** — this document (G-PG design lane, 2026-08-10).
- [x] **PostgreSQL 18 provisioned as the Rust server's backing store (schemas per domain)** — spec closed (§8.2.1a): PGDG `bookworm-pgdg` → `postgresql-18` + `postgresql-contrib-18` (pgcrypto); cluster `127.0.0.1:5432`; database `metin2`; schemas `account`/`player`/`common`/`log`; user `mt2` (owner of the four schemas, no SUPERUSER). RLS stays deferred (`../plans/server-rewrite.md` §2.9 item 9) — per-schema permissions are the provisioned boundary. Contingency: `postgresql-15` from Debian bookworm main (identical feature surface for everything used). Implementation: B1–B2. **Executed 2026-08-10 (G-PG env lane):** PostgreSQL 18.4 (PGDG) live on `127.0.0.1:5432`; db `metin2` with the 4 schemas; 17 phase-1 tables migrated with per-table count parity 18/18 vs MariaDB (runbook chain vendored in `scripts/gpg/`); login SQL for `test`/`*A4B61573…` verified; MariaDB untouched (migration source only).
- [x] **Legacy compatibility adapter working — C++ baseline and legacy client behavior unchanged** — spec closed (§8.2.1c): wire v10 proxy `source/reforge/mysql_proxy`, `127.0.0.1:3307`, handshake + `mysql_native_password` + COM_QUERY/QUIT/PING, SQL translation per `legacy-sql-compatibility.md` §4, per-slot `search_path` (incl. the game auth's account queries through its player slot — inventory §2.3), result contract per `SQLMsg::Store` (`source/server/libsql/AsyncSQL.h:59-80`). C++ source untouched; runtime conf.txt only. Implementation + verification: B5–B7. **Executed + verified 2026-08-10 (loop):** the C++ baseline boots and serves the REAL client on PostgreSQL through the adapter — `LoginSuccess` for `test`/`1234` at 21:39:34 (core1 syslog); the proxy log shows the translated login queries (`mysql_hash_password(...)`, `LOCALTIMESTAMP`/`EXTRACT(EPOCH ...)`); boot parity A/B green vs the MariaDB baseline.
- [x] **Migration groundwork + data comparison harness in place** — spec closed (§8.2.1b/d): phase-1 table set (boot + login, verified in `ClientManagerBoot.cpp`), type adaptation per `legacy-schema.md` §7, `mysql_hash_password` as PG pgcrypto function, `mysqldump --hex-blob` → `scripts/gpg/{schema_gpg.sql,import_pg.py}`, comparison harness `scripts/gpg/parity_check.py`. Full 77-table schema mapping: F3. Implementation: B2–B4.

## Implementation backlog (G-PG — spec: `../plans/server-rewrite.md` §8.2.1)

- **B1.** Provision runbook executed in WSL Debian-M2 (PGDG repo, `postgresql-18` + `postgresql-contrib-18`, cluster, db `metin2`, schemas, user `mt2`).
- **B2.** `scripts/gpg/schema_gpg.sql` — phase-1 PG DDL (from `legacy-schema.md` §4 + live `SHOW CREATE TABLE`), identity `setval` seeds. **Executed** via the vendored runbook chain `scripts/gpg/{02-install-pg18.sh,03-pg-config.sh,05-gen-ddl.py,06-translate.py,07-dump-import.sh,08-verify.sh}` (DDL generated by `05-gen-ddl.py` from live `SHOW CREATE TABLE`).
- **B3.** `scripts/gpg/import_pg.py` — dump transform + load (hex→`\x`, zero dates→NULL, `setval`). **Executed** as `06-translate.py` + `07-dump-import.sh` (16 tables loaded, counts 18/18 vs MariaDB).
- **B4.** `scripts/gpg/parity_check.py` — per-table count + md5, MariaDB vs PG. **Executed** (2026-08-10): 18/18 tables OK (exit 0); method: python3 + pymysql/psycopg2, streamed rows, canonical serialization (bytea→hex, NULL→NULL), sort over the normalized form, md5 incremental.
- **B5.** `source/reforge/mysql_proxy` — `wire` (v10 codec) + `translate` (rewrite; unit tests use `legacy-sql-compatibility.md` §4 as the test table) + `session` (1:1 PG sessions, slot `search_path`). **Executed** (2026-08-10): crate built, 46 tests, workspace 102/102; implementation deviations documented in the spec §8.2.1c; DDL requirement updated there (identity BY DEFAULT). **Fixed after gate** (2026-08-10): 4 gate bugs (text-row cell-count prefix, SET AUTOCOMMIT no-op, session init order, debug logging) + bytea literals → `decode('<hex>', 'hex')` (22021 on player create/save). Workspace 114/114. **Closed after gameplay (2026-08-10):** bytea result-set bug (`first_from_table` parenthesis depth — world-entry client crash on PG, A/B proven) + translate gaps (CAST AS unsigned → leading-numeric-prefix regexp, 22P02; double-quoted strings → single quotes, 42703); workspace 140/140 after F2a.
- **B6.** Runtime switch: `db/conf.txt` `SQL_*` and game conf `player_sql`/`common_sql`/`log_sql` → `127.0.0.1:3307`. **Prepared** (2026-08-10): `*_mariadb`/`*_pg` variants in srv1 (`db/conf.txt`, `auth1/CONFIG`, `chan/ch1/core1/CONFIG`); active confs untouched (MariaDB); activation = copy variant over active file (parity_boot.sh does it).
- **B7.** `scripts/gpg/parity_boot.sh` — baseline vs PG boot diff + `LoginSuccess` for `test`/`1234`; iterate on translation edge cases until green. **Executed** (2026-08-10): baseline snapshot green; PG half validated (fails without proxy as expected, evidence captured); boot signal = `BANWORD: total` in db syslog (`Complete!` is stdout-buffered — inventory note §2.2); restores confs and stops at exit.
- **B8.** Gate close: B2–B7 green; F2a unblocks. **DONE 2026-08-10 (loop):** parity_boot A/B green on the PG run + real client login `test`/`1234` on PostgreSQL (LoginSuccess 21:39:34) + F1.6 transport verified (f16_peer ↔ live auth, no floods). **F2a UNBLOCKED.** Residual for F2a: parity_check volatile-column exclusion (`account.last_play` — live login writes only to PG), crate translation gaps (22P02 `ORDER BY (mValue)::bigint`, 42703 `LIKE "LOCALE"`, 22021 NUL in player INSERT), sqlx/PgPool concrete decision for the `database` crate.

## Not decided in this ADR (explicitly non-blocking for G-PG)

- The concrete `database` crate driver (sqlx 0.9 — ADR-0001 recommendation) — decided at F2a start. The adapter's driver (tokio-postgres) is decided here and does not constrain it.
- The full 77-table schema mapping (domain-module split, FK declarations per `legacy-schema.md` §6, RLS details) — F3. The phase-1 subset is the complete, decided scope for G-PG.
- `guild_invite_limit` ghost table (`legacy-schema.md` §7.7) — not in phase 1 (guild code is not exercised by the login path); decided with the guild port (F3).
- `mysql_affected_rows` changed-vs-matched parity beyond phase 1 (OD-8): PG command-tag counts are decided for the adapter; the consumers listed in `legacy-sql-compatibility.md` §4 are audited per phase as they become reachable (F3+).
