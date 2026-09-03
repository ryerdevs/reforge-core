---
Type: Reference
Status: Current
Audience: Operators
Last verified: 2026-09-02
---

# reforge-core deploy

This directory is what the **end-user** of the reforge-core server gets
when they download a release. It is a **self-contained bundle**: the
binary, the two TOML config files, the operator scripts, and the
admin_tui panel.

## What is in here

| File / directory | What it is |
|---|---|
| `admin_tui.exe` | Interactive visual operator panel. Also supports CLI commands: `admin_tui start`, `stop`, `status`, `backup`, `doctor`. |
| `server_realms.exe` | The Rust server binary. Single binary; role selected by `--role` (auth or channel) and `--config` (TOML path). |
| `config/auth.toml` | Configuration for the auth process (port 30001, PG connection). |
| `config/channel.toml` | Configuration for the channel process (port 30003, rates, map path, quest path). |
| `config/examples/` | Clean reference configuration templates without private credentials. |
| `scripts/manage.py` | Cross-platform management CLI (`python scripts/manage.py start|stop|status|backup|doctor`). |
| `scripts/start_win.ps1` | Start auth + channel in the correct order (PowerShell wrapper). |
| `scripts/stop_win.ps1` | Stop auth + channel in the correct order (PowerShell wrapper). |
| `scripts/status.ps1` | Print a one-shot snapshot of the stack (PowerShell wrapper). |
| `scripts/backup_win.ps1` | Run the nightly `pg_dump` of the metin2 database. |
| `scripts/restore_drill.ps1` | Restore a dump into a disposable database (verify it before pointing the server at it). |
| `logs/` | The auth and channel stdout/stderr logs (created at first start). |
| `backups/` | The nightly `pg_dump` files (created by `backup_win.ps1` or `manage.py backup`). |
| `README.md` | This file. |

## First-time setup

1. Install PostgreSQL 18 and create the `metin2` database with the
   schema in the project's history. The default user is `mt2` /
   `mt2` and the connection string in the TOMLs assumes `127.0.0.1:5432`.
2. Edit `config/auth.toml` and `config/channel.toml` to match your environment
   (PG credentials, listen ports, map path).
3. Run `admin_tui.exe doctor` (or `python scripts/manage.py doctor`) to verify your environment.
4. Open `admin_tui.exe` (or run `python scripts/manage.py start`) to bring the stack up.
5. Run `python scripts/manage.py backup` once to create the first dump in `backups/`.

## Daily operations

| Action | How |
|---|---|
| Start the stack | Press `s` in `admin_tui`, or run `admin_tui start`, or `python scripts/manage.py start` |
| Stop the stack | Press `x` in `admin_tui`, or run `admin_tui stop`, or `python scripts/manage.py stop` |
| Restart | Press `r` in `admin_tui`, or run `admin_tui restart`, or `python scripts/manage.py restart` |
| Status / probe | Press `admin_tui status`, or `python scripts/manage.py status` |
| System doctor | Press `d` in `admin_tui`, or run `admin_tui doctor`, or `python scripts/manage.py doctor` |
| View logs | Press `l` in `admin_tui` (Tab to switch auth/channel, F for follow tail) |
| Run a backup | Press `b` in `admin_tui`, or run `admin_tui backup`, or `python scripts/manage.py backup` |
| Restore a dump | Run `scripts\restore_drill.ps1` for a disposable drill |
| Edit configs | `notepad config/auth.toml` or `notepad config/channel.toml` |

## Building the admin panel

The TUI source is in `source/reforge/admin_tui/`. The canonical build is:

```powershell
powershell -File scripts/build_admin_tui.ps1             # release -> source/deploy/win/admin_tui.exe
powershell -File scripts/build_admin_tui.ps1 -DebugBuild # debug  -> source/deploy/win/admin_tui.exe
```

The script builds `admin_tui` and **automatically copies** the exe to
`source/deploy/win/admin_tui.exe` so the bundle stays self-contained.
`source/deploy/**/*.exe` is gitignored — the copy is local-only, just like
`server_realms.exe`. Raw `cargo build -p admin_tui` still works but won't
copy; use the wrapper when you want the deploy bundle updated.

The TUI intentionally does not perform database restores. Use
`scripts\restore_drill.ps1` only for a disposable restore drill.

## Script ownership

The root `scripts/*.ps1` files are canonical. The matching files under
`source/deploy/win/scripts/` are intentionally byte-for-byte copies so this
deploy bundle stays self-contained. After changing a root script, copy it to
the bundle and verify the SHA-256 hashes; do not maintain divergent versions.
They remain regular files rather than symlinks so the bundle is portable on
Windows.

## Backup cadence

The nightly `pg_dump` produces a file like
`backups\metin2_2026-08-30.dump`. Keep at least the last 7
(default retention in `backup_win.ps1`).

## What is NOT in this directory

- The Rust source tree (it is in `source/reforge/`).
- The C++ oracle (it is in `source/server/` and is **not built**;
  it is only used for parity sessions).
- The compatible client (you supply it yourself; the server
  does not distribute it).
- The historical documentation (it is in `documentation/`).

## License

This deploy bundle is part of the reforge-core project, licensed
under Apache-2.0.
