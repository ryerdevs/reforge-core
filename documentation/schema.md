# Schema — PostgreSQL 18

DB `metin2` en `127.0.0.1:5432` (service `postgresql-metin2`, role mt2/mt2).

## Schemas

- **account** — `account`, `player_index` (login, empire, pids)
- **player** — `player`, `item`, `quest`, `skill_proto`, `mob_proto`, `shop`, `guild_*`
- **common** — `locale`, `gmlist`, `skill_power`
- **log** — `money_log`, `audit`

## Reglas

- `CHECK gold >= 0` en 3 wallets (money_log excluido)
- `WAL` → `Batcher` (100ms) → PG, replay idempotente `ON CONFLICT DO NOTHING`
- `pgcrypto` en `account` — `mysql_hash_password(pw) = '*' + UPPER(SHA1(UNHEX(SHA1(pw))))`

## Fuente

`source/reforge/database/src/` — repos por dominio. DDL en `scripts/gpg/`.
