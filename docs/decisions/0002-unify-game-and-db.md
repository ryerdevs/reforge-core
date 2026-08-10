---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Date: 2026-08-08
Last verified: 2026-08-10
Supersedes: —
Superseded by: —
---

# ADR-0002: Unify `game` and `db` into a single Rust process (db as a crate)

## Context

The legacy (C++) server splits the logic into three binaries: `auth` (30001/30002), `db` (30000) and `game`/cores (30003+). The `db` binary is not a database: it is an **SQL broker + cross-channel coordinator** that:

- concentrates the SQL connections per slot (`SQL_PLAYER`/`SQL_ACCOUNT`/`SQL_COMMON`/`SQL_LOG`, 3 connections per slot — `DBManager.cpp`);
- keeps shared state between cores: `LoginData`, `GuildManager`, `ItemIDRangeManager`, `PrivManager`, `Monarch`, `ItemAwardManager`, `Marriage`;
- serves the proto tables at boot (`PROTO_FROM_DB`);
- talks to each core with its own socket protocol with request/response correlated by `ident` (`QID_*` + `ReturnQuery`), and **duplicates state** (login cache on both sides: `game/src/db.cpp:134` vs `db/src/LoginData`).

Cost of the split observed empirically (2026-08-08 fixes):

- a whole game↔db protocol to maintain;
- a reconnection machine with its own bugs (fdwatch WRITE flood — fix #10: the WRITE interest persists because `oneshot` is ignored; READ must be re-registered after draining);
- mandatory boot order (mariadb → db → auth → cores);
- double deploy and version coordination;
- the db as a single bottleneck and point of failure.

What the split DOES buy: (1) crash containment (one core dies without killing the SQL layer), (2) N cores against 1 db process, (3) isolation of the SQL layer from the single-threaded `libthecore` event loop (synchronous MySQL would block).

In Rust with tokio, the three benefits are recomposed without the protocol:

- crash containment is recreated with per-task panics (`catch_unwind`/`JoinHandle`) and a process-per-channel topology;
- "one db for N cores" is exactly what PostgreSQL already does (ADR-0001);
- SQL isolation comes from an async pool (`sqlx`/`PgPool`) without a blocking event loop.

The game↔db seam is clean: it communicates exclusively through a small typed packet (`HEADER_GD_*`/`HEADER_DG_*` in `common/tables.h`, `QID.h`) and the db binary is only ~12.8k LOC. The decision is low-risk in both directions; the cost of keeping the split is duplicating the packet contract, and the cost of unifying is losing the process boundary.

## Decision

**Unify `game` and `db` into a single Rust process per channel, with `db` as an internal crate** (library boundary, not process boundary).

- One binary per channel + `auth` as its own process (thin proxy). `db` is an embedded crate (`metin2-db`) exposing the same functionality of the legacy db as an internal API; there is no game↔db protocol in Rust.
- The cross-channel coordination that lives today in the db process moves to PostgreSQL: sequences with batches for `ItemIDRangeManager`, row/advisory locks for guilds/parties, `LISTEN/NOTIFY` for cache invalidation, and the login registry as a table.
- **During the migration (Phases 0–5):** the crate also compiles as a standalone daemon speaking the legacy peer protocol (`HEADER_GD_*`/`HEADER_DG_*`), so the C++ game keeps working against a Rust db (F3 milestone) and the real client never notices. The shim must be thin: only framing + dispatch, no business logic.
- **The final unification happens at F6**, when the last core is ported to Rust and the legacy shim is removed.

> Note (2026-08-10): the process topology wording was refined by ADR-0004 — `auth` is a **role of the single `server_realms` binary** (N processes of the same binary with different config), not a separate binary.

## Alternatives considered

### Keep the process separation (db as a separate Rust service)

Rejected. It keeps the protocol, the reconnection, the ident correlation, the duplicated state and the boot order — all the debt the rewrite wants to remove. The only real extra benefit (hard crash isolation) is recomposed with process-per-channel.

### Unify everything into a single process for all 9 channels

Rejected. A fatal panic or OOM in shared code would kill all channels. The project wants "do more with less", but not at the cost of a single global point of failure; the process-per-channel topology gives the right isolation.

## Consequences

### Positive

- The game↔db protocol, the reconnection, the WRITE flood (tokio manages write interest internally) and the duplicated state disappear.
- `db`-as-a-crate is directly testable with golden tests, without sockets.
- One binary+config per channel; simpler deploy.
- The process boundary is kept where it matters: client ↔ channel, channel ↔ Postgres.

### Negative

- Cross-channel coordination in Postgres changes latency and consistency semantics (e.g. double login between channels) vs the legacy in-memory cache — requires explicit contracts and a benchmark before porting `GuildManager`/`LoginData`.
- Loss of the hard db-process containment (a fatal abort in shared code takes down the channel; mitigate with `catch_unwind` at task boundaries and restart supervision).
- The legacy shim (F3–F5) is transition code that must stay thin or it becomes debt.

## Decision points this ADR fixes now

1. **State ownership:** what lives in Postgres vs in memory per channel; destination of each legacy db manager (LoginData → table + cache; ItemIDRangeManager → sequence with batches; GuildManager/Marriage/Monarch → tables + row locks + NOTIFY); owner and cadence of the write-behind save.
2. **Crash recovery:** what a channel loses on restart; fsync/save policy; boot without order between processes (only Postgres first).
3. **Deploy topology:** process per channel, `auth` as its own process; one binary+config.
4. **Migration:** shim contract (which legacy headers stay, which die) and the cutover point at F6.

## Not decided in this ADR

- The concrete PostgreSQL access crate (pending recommendation: `sqlx` 0.9 — see the G-PG task, ADR-0005).
- The internal concurrency model (tokio task-per-connection vs actors) — own ADR.
- Lua runtime for quests (mlua vs UTF-8 data migration) — own ADR.
- The definitive Postgres schema.
- Everything about the client (Phase 7).
