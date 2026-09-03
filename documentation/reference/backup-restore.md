---
Type: Reference
Status: Current
Audience: Operators
Last verified: 2026-09-03
---

# Backup & restore runbook — cross-platform runtime (ADR-0012)

## What protects the data

- **Nightly `pg_dump -Fc`** of `metin2` to `backups/metin2_<timestamp>.dump`
  (`python scripts/manage.py backup` or `python scripts/backup.py`),
  scheduled by the operator outside the repo (Task Scheduler / cron).
- **Durable-first WAL** in the server (`database/src/wal.rs`): every mutation
  is persisted to a local WAL file BEFORE PG, flushed by the Batcher (<=100 ms)
  in ONE transaction with the audit row, replayed idempotently on boot. A hard
  server crash loses at most the in-flight batch; a PG crash replays from WAL.

## Cross-platform backup & restore operations

### 1. Create a backup
```bash
python scripts/manage.py backup
```
Emits a timestamped, custom-format PostgreSQL dump into `backups/metin2_<stamp>.dump`.

### 2. Verify or inspect database health
```bash
python scripts/manage.py db check
```
Verifies PostgreSQL connectivity, validates the 5 schemas (`account`, `common`, `player`, `log`, `world`), and reports table counts.

### 3. Restore from backup (disaster recovery)
```bash
# 1. Stop running servers
python scripts/manage.py stop

# 2. Restore database from chosen dump or SQL file
python scripts/manage.py db restore backups/metin2_<stamp>.dump

# 3. Start server stack and verify
python scripts/manage.py start
python scripts/manage.py status
```

Notes:

- The server-side WAL files (runtime dir) may replay mutations NEWER than the
  dump on boot — that is expected and idempotent (`ON CONFLICT DO NOTHING`).
  If the dump is newer than the WAL, the WAL replays nothing.
- Never restore over a running server; never drop the live `metin2` database
  with the Rust auth/channel still connected.

## Off-host copy (operator action)

The dumps live on the same 4 GB host as the data — the single biggest data-loss
risk in the project. Copy the newest `metin2_<date>.dump` to an external drive
or another machine regularly (weekly minimum). There is no second volume on
this host (verified 2026-08-30: only C:).

## Disk budget (G0.2)

- Keep C: free space > 15 GB; a full disk blocks the nightly backup.
- `scripts/clean.ps1` prunes cargo build artifacts; the workspace `target/`
  budget is <= 5 GB. Current status is tracked in
  [plans/gap-registry.md](../plans/gap-registry.md) (row G0.2).
