---
Type: Snapshot
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-10
---

# CURRENT — State of the project (snapshot, 2026-08-10)

This is a point-in-time snapshot. It is the **source of truth for current status**; refresh it at the end of every session that changes the state it describes. Historical narrative lives in [`../CHANGELOG.md`](../CHANGELOG.md); the full plan lives in [`../ROADMAP.md`](../ROADMAP.md).

## Snapshot

- **Date:** 2026-08-10
- **Commit:** documentation reorganization **pending commit** — this snapshot's files (`CURRENT.md`, `DOCUMENTATION.md`, ADRs 0005–0007, the reorganized `docs/` tree) are not yet committed; `b85a019` is the last committed state and does NOT contain them.
- **Rust rewrite workspace:** `source/reforge/` — crates `protocol`, `network`, `database`, `realm` + binary `server_realms` (flat layout, ADR-0004). Rust toolchain 1.97.0, edition 2024, `unsafe_code = "forbid"`.
- **Legacy baseline:** C++ login fully working (auth + channel + character select, account `test` / `1234`); runtime instances under `source/deploy/` (gitignored). The C++ baseline is the oracle — `source/reforge/**` is owned exclusively by the rewrite (ADR-0003).
- **Docs structure:** reorganized 2026-08-10 into `plans/`, `reference/`, `history/`, `guardrails/`, `decisions/` + hubs (see `docs/README.md`); superseded plans moved to `history/` (read-only). Diátaxis modes (`tutorials/`, `how-to/`, `explanation/`) have no content yet — created on demand.

## Verified numbers

| Item | Value | Evidence |
|---|---|---|
| Workspace tests | **56/56 passing** (protocol 30, network 23, server_realms 3) | `cargo test` in `source/reforge` |
| `protocol` crate | 30/30 — byte-exact login flow (golden vectors, roundtrips, sizes, bad lengths) | `source/reforge/protocol/src/lib.rs` |
| `network` crate | 23/23 — listener (2), framer (10), handshake (11) | `source/reforge/network/src/{server,framer,handshake}.rs` |
| `server_realms` binary | 3/3 — role config scaffold | `source/reforge/server_realms/src/main.rs` |
| Graphs (graphify, at `b85a019`) | server **13,200 nodes / 33,251 links**; client **17,501 nodes / 39,258 links**; merged **30,701 nodes / 72,509 links** | `source/server/graphify-out/graph.json`, `source/client/graphify-out/graph.json`, `graphify-out/graph.json` |

## Current architecture decisions

1. **PostgreSQL as the one canonical database**, with a **temporary compatibility adapter for the legacy C++ server**. Do **not** run dual databases (MariaDB + PostgreSQL) side by side. This refines ADR-0001 (PostgreSQL is no longer only "the future Rust DB" — it becomes the single store; legacy C++ reaches it through the adapter during the transition). **Documented as [ADR-0005](decisions/0005-postgresql-cutover-and-legacy-adapter.md) — status Proposed; the G-PG gate closes only on acceptance.**
2. **Legacy packet compatibility stays isolated and deletable.** Compatibility structs/types are confined to a dedicated boundary so they can be removed wholesale when the legacy peers are gone — never woven into new code. **Documented as [ADR-0006](decisions/0006-legacy-wire-pack-compat-boundary.md) — Proposed.**
3. **No partial Rust code embedded in the legacy client** during F0–F6. The Rust client ships standalone (Slint in F5, wgpu in F7). **Accepted — [ADR-0007](decisions/0007-no-partial-rust-in-legacy-client.md).**
4. **Config format: TOML** — decided (ADR-0004), implementation pending (config-rs in F2, clap for `--role`).

## Status by phase

| Phase | Status | Next action |
|---|---|---|
| **F0 — Foundations** | `protocol` crate complete, **30/30 tests**, F0 milestone (byte-exact LOGIN3) met at crate level | Capture harness: real packets via tcpdump/Wireshark against the C++ server in WSL (requires WSL runtime up) |
| **F1 — Network** | Listener, framer, handshake done — **23/23 tests** | **F1.6** integration milestone: Rust peer ↔ C++ auth, no timeouts or WRITE floods (requires WSL) |
| **F2 — Auth + first client batch** | **Blocked** — cannot start until the PostgreSQL cutover / database boundary is documented | **G-PG**: accept [ADR-0005](decisions/0005-postgresql-cutover-and-legacy-adapter.md) (Proposed) and close the cutover spec, then begin **F2a** (first unblocked F2 slice) |
| **F3+ — Data, world, parity** | Planned only (`database`, `realm` are scaffolds) | Depends on G-PG; details in [plans/server-rewrite.md](plans/server-rewrite.md) |

## Next gates

1. **G-PG** (planned, not done): PostgreSQL cutover + database boundary — single canonical PG database, temporary compatibility adapter for legacy C++, no dual databases. Deliverable drafted as ADR-0005 (**Proposed**); the gate remains open until acceptance.
2. **F2a** (planned, not done): first slice of F2 once G-PG is closed.

## Where to look next

- Full plan with acceptance criteria per task: [`../ROADMAP.md`](../ROADMAP.md)
- Architecture decisions: [`decisions/`](decisions/) (0001 PostgreSQL, 0002 game+db unification, 0003 workspace, 0004 names/config, 0005 cutover, 0006 compat boundary, 0007 client boundary)
- Design reference: [`plans/server-rewrite.md`](plans/server-rewrite.md)
- Wire contract: [`reference/protocol/login-flow.md`](reference/protocol/login-flow.md)
- Superseded plans/specs (read-only): [`history/`](history/)
- Change history: [`../CHANGELOG.md`](../CHANGELOG.md)
