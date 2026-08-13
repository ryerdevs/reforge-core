---
Type: Decision
Status: Accepted (2026-08-12)
Audience: Contributors, maintainers
Date: 2026-08-12
Last verified: 2026-08-12
Supersedes: —
Superseded by: —
---

# ADR-0012: Runtime hosting — native Windows + on-demand WSL for the frozen C++ oracle until F6

## Status

Accepted (2026-08-12). User decision: migrate the whole runtime to native Windows and keep WSL only as a minimal on-demand box for the frozen legacy C++ binaries (the parity oracle), shut down when not in use.

## Context

- Host: Windows 10 22H2, 4 GB RAM, WSL 2.7.3 with a 2 GB memory cap (`.wslconfig`) and documented instability (WHEA PCIe errors, `E_UNEXPECTED` crashes during heavy I/O; CHANGELOG/AGENTS.md). The whole stack (PG + MariaDB + proxy + C++ binaries + Rust servers) currently runs inside WSL.
- The Rust rewrite (`source/reforge`: protocol/network/database/game_core/mysql_proxy/locale_import/server_realms — `realm` renamed `game_core` 2026-08-13) is cross-platform by design — tokio, tokio-postgres, bevy_ecs; `cargo test` already runs natively on Windows.
- The legacy C++ binaries (game_r41023, db_r41023) are **Linux ELF, frozen forever** — the user decided (2026-08-12) they will NEVER be recompiled ("that's the past"). They are the parity oracle: parity_boot A/B, golden captures, and the **F6 automated side-by-side** milestone (same packet input → diff of Rust vs C++ responses) depend on them running.
- The client is compiled from Windows (MSBuild Release|Win32) — already native.
- MariaDB is migration-source only since G-PG (ADR-0005): the C++ stack runs on PostgreSQL through `mysql_proxy`. It can be archived and stopped.

## Decision

**Phase 1 (now):** everything except the frozen C++ binaries moves to native Windows.

1. **PostgreSQL 18 → native Windows** (PGDG/EDB zip binaries): `pg_dump` from WSL → restore into Windows PG (same db `metin2`, role `mt2`, scram; `LC_COLLATE='C'` for deterministic text ordering; pgcrypto contrib for `account.mysql_hash_password`). Verify row counts (30 phase-1 tables + locale tables: mob_names 8,628, item_names 34,281, spawns 145,876).
2. **MariaDB archived + stopped**: one final `mysqldump --hex-blob` to `C:\projects\metin2-extra\archive`, then stop and purge (no Windows MariaDB ever needed).
3. **Rust servers native on Windows**: `server_realms` (auth 30001, channel 30003) with TOML `listen = "127.0.0.1:..."` and PG `127.0.0.1:5432`; `REALM_WAL_DIR` → Windows path.
4. **Client**: `serverinfo.py` host → `127.0.0.1` (client and servers now share the Windows host) + repack.
5. **Scripts**: new `scripts/start_win.ps1` / `stop_win.ps1` (PG service → Rust auth → Rust channel); the bash scripts survive only as the WSL parity path, trimmed to "frozen C++ db/core + proxy".
6. **WSL shrinks to on-demand oracle**: frozen C++ binaries + `mysql_proxy` only. The proxy STAYS in WSL so the frozen C++ `conf.txt` (SQL slots → `127.0.0.1:3307`) is never touched; the proxy's PG target is the WSL gateway IP of the Windows host (one TOML line patched per boot by a small helper; Windows Firewall allows 5432 from the WSL subnet). `.wslconfig` memory cap → 1 GB; `wsl --shutdown` when not in a parity session.
7. **WSL cleanup (disk)**: PG data, MariaDB data, build toolchains, and the `/home/m2/source` build copy (archived to `metin2-extra/archive` first — the repo already holds the C++ source) removed. The "two copies of source" rule (AGENTS.md) dies for the server: no more WSL builds; Rust + client live only on Windows.

**Phase 2 (at F6):** run the automated side-by-side against the on-demand WSL C++ stack (network replay, no co-location needed); after F6 acceptance: archive the frozen binaries + MariaDB dump + parity snapshots + golden fixtures, then `wsl --shutdown` + `wsl --unregister Debian-M2`. **WSL is deleted for free at the moment the oracle is no longer needed.**

## Alternatives considered

- **Full WSL status quo**: rejected — the user's motive is real: the VM overhead (double page-cache, 2 GB cap contention, I/O virtualization) costs ~1.0–1.5 GB and CPU on a 4 GB host; PG+MariaDB inside the VM compete for the cap → swap thrash contributing to the instability.
- **WSL off now (delete everything Linux, redefine F6)**: rejected — the marginal saving over Phase 1 is ~200–400 MB, but it destroys the F6 side-by-side milestone and the live oracle exactly in the highest-parity-risk phase (F5.3 gameplay has zero real-client E2E verification for slices 2–17). Not a sacrifice, just bad timing; it becomes correct at/after F6.
- **Proxy on Windows too**: rejected — the frozen C++ `conf.txt` would need the Windows host IP (changes per WSL boot) in every parity session; keeping the proxy in WSL means one TOML line patched per boot and zero churn on frozen configs.

## Consequences

- **RAM**: ~1.0–1.5 GB freed + less CPU/IO thrash; WSL off ~95% of the time. Honest note: native PG uses the same ~0.5 GB it used inside WSL — the win is the VM overhead/cap contention, not PG's own memory; the 4 GB host constraint remains.
- ADR-0005 unaffected: `mysql_proxy` is still temporary and removed at F6.
- Verification model: `capture_auth.sh` (tcpdump) stays for WSL parity sessions; a native `--capture` mode in `server_realms` (raw bytes per connection) will replace tcpdump for future golden fixtures.
- The F6 side-by-side harness runs over the network against the on-demand WSL oracle — no co-location requirement.
- Backup cadence (review H.2): nightly `pg_dump` moves to Windows PG.
- AGENTS.md runbook and the two-source-copies rule are updated to the new topology (done in the same session).

## Not decided in this ADR

- The concrete Windows service management for PG (pg_ctl vs NSSM service) — decided at implementation (zip binaries + pg_ctl for now, ponytail).
- Whether the frozen binaries also get a Windows-side archive copy at Phase 1 (yes — planned, `metin2-extra/archive`).
