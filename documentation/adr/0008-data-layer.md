---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Date: 2026-08-11
Last verified: 2026-09-02
Supersedes: —
Superseded by: —
---

# ADR-0008: Data layer (F3) — domain repositories, PostgreSQL driver, durable/volatile contract

## Context

F3 (ROADMAP Phase 3, first slice) starts the `database` crate. The design target is set in
the historical server plan ([§5.5](../history/plans/server-rewrite.md#55-data-layer--final-design)):
a crate organized **by domain** (account/world/social/
economy/log), each with its own schema, permissions per schema, and repositories as the only
access path. Two decisions are due now:

1. **Driver**: ADR-0001 left the PostgreSQL crate undecided ("candidate: sqlx 0.9"; the concrete
   decision is a G-PG task per ADR-0005). Meanwhile `tokio-postgres 0.7` has been proven
   end-to-end in this project.
2. **First slice contract**: the `account` domain must port the QID login contract
   (`QUERY_LOGIN`, 13 columns, `ClientManagerLogin.cpp:405-426`) and the auth-side account
   mutations (`set lang`, `set hwid`) with the semantics already verified in `auth.rs`.

Evidence for the driver decision (both measured in this repo):

- **tokio-postgres 0.7** is already a `[workspace.dependencies]` entry and has served real
  traffic: the F2a auth (`server_realms --role auth`) validated real client logins
  (v40999, `test`/`1234`) against PostgreSQL 18.4 end-to-end (2026-08-10/11), and
  `mysql_proxy` (59 tests) exercises multi-statement sessions. Per-call connections are
  verified at ~ms latency locally (`auth.rs` doc comment).
- **sqlx 0.9** is not yet used anywhere in this repo. Its main advantages over
  tokio-postgres are the async pool (`PgPool`) and optional compile-time checked queries
  (`query!`, which requires `DATABASE_URL` at build time — friction for the dual
  Windows/WSL builds of this workspace).

## Decision

1. **PostgreSQL-only data layer.** `database` targets PostgreSQL exclusively; MariaDB
   remains only the migration/export source (ADR-0001, ADR-0005 — no dual-store).
2. **No `direct-sql` backend.** All persistence goes through domain repositories in the
   `database` crate; the runtime never builds SQL outside it. The legacy shim boundary
   (ADR-0002/0005) is not affected — it lives outside this crate.
3. **Driver: `tokio-postgres 0.7`** (already in the workspace). Repositories use one
   connection per call for now (the pattern verified in `auth.rs`: connect + spawn the
   connection task). A pool is **not** introduced yet: the current contract (login +
   two UPDATEs) is per-call-cheap, and the batch pipeline that would benefit from a pool
   is deferred (see below). If a pool is later measured to be needed, it can be added
   without changing the driver (e.g. `deadpool-postgres`) or by adopting sqlx then —
   both are compatible with this ADR's contract.

   > **EXECUTED (2026-08-13, 44th part) — the pool clause in Decision 3 landed.**
   > Trigger: the deferral recorded above ("If a pool is later measured to be
   > needed, it can be added without changing the driver (e.g.
   > `deadpool-postgres`)"); the orchestrator approved the oracle's plan and the
   > coder lane executed it: **`deadpool-postgres 0.14.1`** (pairing verified —
   > depends on tokio-postgres 0.7.9, the same workspace driver, no upgrade),
   > NEW `database/src/pool.rs` (`PgPool`), ~13 repos moved from per-call
   > `pg_conn`+`connect()` to `pool.get()`, `PgMutationSink::new(pool)`
   > (wal.rs — no reconnect-per-batch), `WorldStore::new(pool, Arc<Batcher>)`
   > with **one `Batcher` per channel** (was per player), `pool_max_size`
   > default 10 (config.rs:110), and the direct SQL in `channel/shop.rs`
   > absorbed by `ItemRepo::load_sell_proto` (ADR-0008 §2 "no direct-sql
   > backend" restored). Verified: workspace **565 passed / 0 failed / 35
   > ignored**, clippy identical to baseline, deployed 18:01:39 (binary
   > 4,509,184 B, SHA256 `77D8ACD2…C732`).

4. **Crate layout by domain** (ADR-0004 flat layout): `src/account.rs` first
   (`AccountRepo`: `login`, `set_lang`, `set_hwid` — port of `QUERY_LOGIN` +
   `input_auth.cpp:133-152` patterns), `src/world.rs`, `src/social.rs`, `src/economy.rs`,
   `src/log.rs` declared as doc stubs until their phases (F4/F5).
5. **Durable/volatile contract (fixed here, mechanism deferred):**
   - *Durable* = write-through transactional batches ≤100 ms to the central PostgreSQL;
     a region change loses NOTHING durable.
   - *Volatile* = position/HP are local; saved every 30 s + logout.
   - Idempotent replay (`ON CONFLICT DO NOTHING` + `mutation_id`) is the designed
     mechanism for the "no dupe window / crash = ≤100 ms in-flight" guarantees —
     **deferred** to the WAL phase (see Deferred).

   > **AMEND (2026-08-12, F3 phase 2 — save-by-event):** the implemented contract is
   > **save-by-event**, not a timer: every durable mutation flows
   > event → `Batcher` (≤100 ms, one tx) → **local WAL file** (`{wal_dir}/{uuidv7}.wal`,
   > JSONL, `sync_all` BEFORE PG) → PostgreSQL; the file is deleted only post-COMMIT and
   > re-applied idempotently at next boot (`replay_wal`, once per process via `OnceLock`).
   > The "volatile = saved every 30 s + logout" clause is **superseded** for durable state
   > (position/HP remain local/volatile). Implementation: `database/src/wal.rs` +
   > `game_core::WorldStore` wiring (2026-08-12, CHANGELOG 11th part; gated `replay_wal` PG
   > test still pending by user directive).
6. **Schema and permissions**: one PG schema per domain (account/player/common/log already
   migrated by G-PG); permissions per schema (log cannot write to economy). **RLS
   deferred** (see Deferred).

## Alternatives considered

### sqlx 0.9 (PgPool) as the driver

Rejected for now — not because sqlx is a bad fit, but because the evidence in this repo
favors the dependency that already works: tokio-postgres is verified end-to-end (auth
serving real logins + proxy sessions), it is already in the workspace, and it covers the
full contract (transactions via `client.transaction()`, LISTEN/NOTIFY for the §5.6 hot
reload, prepared statements). sqlx's pool is its main advantage and the pool is not needed
by the current contract; compile-time checked queries would add `DATABASE_URL` friction to
the dual-toolchain build. If a pool becomes necessary, `deadpool-postgres` adds it to
tokio-postgres without a driver change; sqlx remains adoptable later (this ADR does not
lock the driver for the batch pipeline — revisit at the WAL phase with measurements).

### A shared `common`/`utils` crate for sha1 before F3

Rejected for this slice: it would require touching `server_realms` (working, verified) to
consume it. `database` carries its own copy of the verified SHA-1 module (same provenance
note as `server_realms/src/sha1.rs`); the unification into a common crate is a follow-up
when a third consumer appears.

### MariaDB backend for `database`

Rejected: single canonical store (ADR-0001/0005). MariaDB exists only as migration source;
a MariaDB-capable backend in `database` would double the test surface for zero runtime value.

## Consequences

- The `account` domain is implementable now with proven pieces (hash, hex hwid, error
  handling) copied from `auth.rs` — no new dependency, no new untested pattern.
- The QID login contract is preserved: `login()` returns the 13-column row semantics
  (`None` = wrong credentials, exactly like `CreateAccountTableFromRes` returning nullptr
  after the `strcmp` guard).
- `auth.rs` is NOT refactored in this slice (it works); a follow-up migrates its queries
  to `AccountRepo` with zero behavior change.
- The deferred items keep their §5.5 targets recorded here, so F3/WAL can pick them up
  without re-deciding the contract.

## Deferred in this ADR (with target phase)

- ~~**Local WAL per region + `mutation_id` (uuidv7) + idempotent replay** (`ON CONFLICT
  DO NOTHING`) — F3 phase 2 (WAL pipeline), gated on measurement per §5.5.~~ **DONE
  2026-08-12** (`WalSink` durable-first + `replay_wal` — see the AMEND in §5; the gated
  PG replay test remains pending by user directive).
- **RLS** (`current_setting('app.pid')`) — after the WAL phase, per §5.5.
- **Patroni hot-standby failover** (~2 min promotion target) — F5/F6 ops phase.
- ~~**Pool (deadpool-postgres or sqlx adoption)** — with the batch pipeline, only if
  measurement shows per-call connections are insufficient.~~ — **EXECUTED
  2026-08-13 (44th part)** (deadpool-postgres 0.14.1; channel-level `PgPool`;
  one `Batcher` per channel — see the note in §Decision 3; sqlx remains a later
  alternative).
- **uuidv7 ID generation / `CHECK gold>=0` / append-only audit partition** — with the
  WAL batch pipeline (the account slice generates no IDs).

## Not decided in this ADR

- Repository API shape beyond `account` (world/social/economy/log stubs are doc-only).
- Migration tooling for future schema changes (sqlx::migrate vs plain SQL vs embedded
  migrations) — decided when the first schema change lands.
