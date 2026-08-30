---
Type: Reference
Status: Current
Audience: Operators
Last verified: 2026-08-30
---

# Backup & restore runbook — native Windows runtime (ADR-0012)

## What protects the data

- **Nightly `pg_dump -Fc`** of `metin2` to
  `C:\projects\metin2-extra\backups\metin2_<yyyy-MM-dd>.dump`
  (`scripts/backup_win.ps1`), retention 7, scheduled by the operator outside
  the repo (Task Scheduler; verify with the file dates in the backups dir).
- **Durable-first WAL** in the server (`database/src/wal.rs`): every mutation
  is persisted to a local WAL file BEFORE PG, flushed by the Batcher (<=100 ms)
  in ONE transaction with the audit row, replayed idempotently on boot. A hard
  server crash loses at most the in-flight batch; a PG crash replays from WAL.

## The drill (run it monthly, and after any schema change)

```powershell
powershell -ExecutionPolicy Bypass -File scripts\restore_drill.ps1            # newest nightly dump
powershell -ExecutionPolicy Bypass -File scripts\restore_drill.ps1 -KeepDb    # leave the copy to inspect
```

What it proves: the dump is restorable, the schema is complete, and the key
tables carry data. It restores into a DISPOSABLE `m2_drill_<stamp>` database,
counts `account.account`, `player.player`, `player.item`, `player.mob_proto`,
`player.item_proto` (must be non-empty), `player.guild`, `player.quest`, and
drops the copy. A dump that has never been restored is not a backup.

Last drill: **2026-08-30 PASSED** (metin2_2026-08-29.dump -> m2_drill_20260830-102840;
account=3, player=6, item=42, mob_proto=2864, item_proto=11002, guild=0, quest=2).

## Real restore (disaster recovery)

```powershell
# 1. Stop the servers (PG keeps running)
powershell -ExecutionPolicy Bypass -File scripts\stop_win.ps1

# 2. Recreate the database and restore the chosen dump
$env:PGUSER="mt2"; $env:PGPASSWORD="mt2"
& C:\projects\metin2-extra\pg18\pgsql\bin\dropdb.exe   -h 127.0.0.1 -p 5432 --force metin2
& C:\projects\metin2-extra\pg18\pgsql\bin\createdb.exe -h 127.0.0.1 -p 5432 metin2
& C:\projects\metin2-extra\pg18\pgsql\bin\pg_restore.exe -h 127.0.0.1 -p 5432 -d metin2 -Fc C:\projects\metin2-extra\backups\metin2_<date>.dump

# 3. Start and verify
powershell -ExecutionPolicy Bypass -File scripts\start_win.ps1
# ports 5432 / 30001 / 30003; login test/1234 in the external client
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
