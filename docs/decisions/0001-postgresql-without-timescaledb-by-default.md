---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Date: 2026-08-06
Last verified: 2026-08-10
Supersedes: —
Superseded by: —
---

# ADR-0001: PostgreSQL as the primary database, no TimescaleDB by default

## Context

The current server uses C++ and a `libsql` layer based on the MySQL/MariaDB API. Its model contains transactional game state — accounts, characters, items, inventory, guilds and trading — plus historical and log tables.

The project has two horizons:

1. Keep the C++ server compatible with Alpine Linux and Docker.
2. Structurally rewrite the server in Rust, with a more coherent architecture and a possible unification of the current `game` and `db` responsibilities.

TimescaleDB was evaluated because part of the model contains events and records with timestamps. However, it has not been demonstrated yet that the volume, retention or analytical queries justify adding an extension and specific operations for time series.

## Decision

Standard PostgreSQL will be the main database of the future Rust server.

TimescaleDB will not be installed nor become an initial dependency. It will be evaluated later only for telemetry, metrics, audit or historical event tables if real measurements show a clear need for temporal partitioning, retention, compression or high-volume analytics.

The main game state tables remain normal PostgreSQL relational tables.

The C++ server compatibility phase keeps MySQL/MariaDB until a verified migration strategy exists. **This timing is refined by proposed ADR-0005:** the target plan is a single canonical PostgreSQL database with a temporary compatibility adapter; no dual-store operation is intended. The PostgreSQL target and the rejection of TimescaleDB remain accepted by this ADR.

## Alternatives considered

### PostgreSQL with TimescaleDB from the start

Rejected for now. It provides useful capabilities for time series, but adds a dependency and operational restrictions before there is evidence they are necessary.

### MariaDB as the permanent target

Not chosen for the rewrite. It is the most compatible transition with the current C++, but it keeps the conceptual dependency on the MySQL API and patterns we want to move past.

### Distributed database such as CockroachDB or YugabyteDB

Not chosen. Multi-node distribution, transactional retries and operational complexity are not justified for the initial deployment of a self-contained game server.

### Specialized time-series database

Not chosen. It would introduce another technology and operational boundary when PostgreSQL can initially cover both the game state and the lower-volume logs.

## Consequences

### Positive

- Fewer components and a smaller operational surface.
- Future schema oriented to PostgreSQL without dragging MySQL limitations.
- Game state transactions and relations stay in a clear relational model.
- TimescaleDB can be added later without making it an irreversible decision for the whole system.
- The Docker infrastructure can start with a standard PostgreSQL server.

### Negative

- Standard PostgreSQL may not be enough for a high-volume telemetry platform.
- If logs grow a lot, partitioning, retention or a specialized solution must be designed.
- The MySQL/MariaDB migration will still require adapting types, defaults, `ENUM`, `SET`, `UNSIGNED` integers, invalid dates and specific queries.

## Conditions to re-evaluate TimescaleDB

The decision is revised only with measurements and a concrete case. The indicators will be:

- sustained volume of event insertions;
- size and growth of historical tables;
- query latency over time ranges;
- cost of retention, compression and deletion of old data;
- index and maintenance pressure on standard PostgreSQL;
- need for real-time temporal aggregations.

The re-evaluation must include a reproducible benchmark, a backup/restore test and a review of the impact on Docker, Alpine and daily operations.

## Not decided in this ADR

- The Rust library/crate to access PostgreSQL (candidate: `sqlx` — the concrete decision is a G-PG task, ADR-0005).
- The definitive design of the Rust schema.
- The final split between transactional state, events and telemetry.
- The exact data migration procedure from MySQL 5.6.
- Whether historical events live in the same PostgreSQL cluster or in a separate instance.
