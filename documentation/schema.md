---
Type: Reference
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-09-02
---

# Schema — PostgreSQL 18

Database `metin2` runs at `127.0.0.1:5432` through the
`postgresql-metin2` service with role `mt2/mt2`.

This page describes the schema contract, not the availability of a local
database. For current runtime and verification status, use the [Gap
Registry](plans/gap-registry.md) and [progress handoff](progress.md).

## Schemas

- **account** — `account`, `player_index` (login, empire, and character IDs)
- **player** — `player`, `item`, `quest`, `skill_proto`, `mob_proto`, `shop`, and
  `guild_*`
- **common** — locale data, `gmlist`, and `skill_power`
- **log** — `money_log` and `audit`

## Invariants

- `CHECK (gold >= 0)` is applied to the three wallet tables; `money_log` is
  append-only and excluded from that wallet constraint.
- Mutations flow `WAL → Batcher (100 ms) → PostgreSQL`; replay is idempotent with
  `ON CONFLICT DO NOTHING`.
- `pgcrypto` in `account` provides
  `mysql_hash_password(pw) = '*' + UPPER(SHA1(UNHEX(SHA1(pw))))`.

## Sources

- Domain repositories: [`source/reforge/database/src/`](../source/reforge/database/src/)
- PostgreSQL migrations: [`scripts/gpg/`](../scripts/gpg/)
- Data-layer decision: [ADR-0008](adr/0008-data-layer.md)
