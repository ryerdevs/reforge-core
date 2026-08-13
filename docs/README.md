---
Type: Hub
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-12
---

# Metin2 Documentation Hub

This is the entry point to the project's documentation. It tells you **which document to read for which goal** — you should be oriented in about five minutes.

The rewrite goal, verified state of the legacy baseline, and working rules live in the repository root:

- [`../AGENTS.md`](../AGENTS.md) — mission, verified protocol facts, runbook, working rules.
- [`../ROADMAP.md`](../ROADMAP.md) — the master plan: phases F0–F7, gates (G-PG), milestones.
- [`../CHANGELOG.md`](../CHANGELOG.md) — chronological record of verified changes (history).
- [`CURRENT.md`](CURRENT.md) — **snapshot of the current state** (what is true today).
- [`DOCUMENTATION.md`](DOCUMENTATION.md) — mandatory documentation policy (kinds, metadata, guardrails, workflow).

## Sections

| Section | What it holds | Entry point |
|---|---|---|
| **[Plans](plans/)** | Active design and migration plans (single-file canonical design) | [`plans/server-rewrite.md`](plans/server-rewrite.md) |
| **[Decisions](decisions/)** | Architecture decision records (ADRs 0001–0012) | [`decisions/`](decisions/) (table below) |
| **[Reference](reference/)** | Technical contracts: wire protocol, legacy compatibility, quest DSL, legacy system notes | [`reference/`](reference/README.md) |
| **[Guardrails](guardrails/)** | Lessons and rules not to repeat (encoding traps, operations, crash postmortem) | [`guardrails/`](guardrails/README.md) |
| **[History](history/)** | Superseded plans, specs and snapshots — read-only, never deleted | [`history/`](history/README.md) |

> The Diátaxis learning/goal/understanding modes (`tutorials/`, `how-to/`, `explanation/`) have **no content yet**; they are created on demand, not as empty links (policy `DOCUMENTATION.md` §2).

## What to read, by goal

| Your goal | Read |
|---|---|
| "What is the state of the rewrite right now?" | [`CURRENT.md`](CURRENT.md) |
| "What is the plan and what is next?" | [`../ROADMAP.md`](../ROADMAP.md) (phases + gates) |
| "What changed and why, with evidence?" | [`../CHANGELOG.md`](../CHANGELOG.md) |
| "How is this repository documented and reviewed?" | [`DOCUMENTATION.md`](DOCUMENTATION.md) |
| "Why is the architecture the way it is?" | [`decisions/`](decisions/) (ADRs, see table below) |
| "Full design of the Rust server" | [`plans/server-rewrite.md`](plans/server-rewrite.md) |
| "Byte-exact wire contract of the login flow" | [`reference/protocol/login-flow.md`](reference/protocol/login-flow.md) |
| "What must I never break / repeat?" | [`guardrails/`](guardrails/README.md) |
| "Where is a specific definition, table, header?" | [`reference/`](reference/README.md) + crate docs in [`../source/reforge`](../source/reforge) |
| "What was decided in the past (superseded)?" | [`history/`](history/README.md) (read-only) |

## Phase → code → docs → verification → next gate

Real state as of 2026-08-10 (see [`CURRENT.md`](CURRENT.md) for the full snapshot).

> ⚠️ **Stale snapshot (2026-08-10), kept for reference. Current state (2026-08-12):** G-PG COMPLETE, F2a/F2b DONE, F4 milestone MET, F5.3 slices 1–17 DONE, ECS migration slice next; **WSL retirement EXECUTED (ADR-0012)** — all-Windows native stack verified with the real client (login → select → world → movement). See [`CURRENT.md`](CURRENT.md) and [`plans/master-plan.md`](plans/master-plan.md) (oracle-reviewed 2026-08-12).

| Phase | Code (all under `source/reforge/`) | Docs | Verification | Next gate |
|---|---|---|---|---|
| **F0 — Foundations** | crate `protocol` (17 login-flow packets, zero-deps, byte-exact) | ADR-0003, ADR-0004, [`reference/protocol/login-flow.md`](reference/protocol/login-flow.md) | **done**: 30/30 tests (golden vectors, roundtrips, sizes, bad lengths) | **pending**: real-capture harness (tcpdump against C++ server in WSL) |
| **F1 — Network** | crate `network` (tokio: `server.rs` listener, `framer.rs`, `handshake.rs`; `auth` module comes in F2) | F1.x tasks in [`../ROADMAP.md`](../ROADMAP.md) | **done**: 23/23 tests (framer 10, handshake 11, server 2) | **pending**: F1.6 integration milestone (Rust peer ↔ C++ auth, needs WSL runtime) |
| **G-PG — PostgreSQL cutover** | `database` crate (PostgreSQL-only, per ADR-0005) | [ADR-0005](decisions/0005-postgresql-cutover-and-legacy-adapter.md) (**Proposed**) | **blocked**: gate open until ADR-0005 acceptance | **F2 unblock checklist** (ADR-0005 accepted; PG provisioned; adapter verified) |
| **F2 — Auth + first client batch** | binary `server_realms` (roles `auth` \| `channel` by config — scaffold only), `network::auth` | config TOML decided (ADR-0004), implementation pending | **blocked**: 3/3 tests in `server_realms` scaffold | **G-PG first** → then F2a |
| **F3+ — Data, world, parity** | crates `database`, `game_core` (scaffolds) | [`plans/server-rewrite.md`](plans/server-rewrite.md) | not started | planned; requires G-PG first |

> Do not claim G-PG or F2 as implemented — they are **planned gates**. G-PG's deliverable exists only as ADR-0005 in **Proposed** status.

## Plans

- [`plans/master-plan.md`](plans/master-plan.md) — **the consolidated master plan** (joins ROADMAP + canonical design + locale plan + ADRs 0001–0012 + CURRENT snapshot; **oracle-reviewed 2026-08-12**, recommendations H.1–H.5 applied). Status: **Draft v0.2**.
- [`plans/server-rewrite.md`](plans/server-rewrite.md) — canonical single-file design: architecture, anti-hack, data layer, migration order (G-PG before F2), quest DSL, regional channels, modifiable client. Status: **Draft v0.3 (canonical)**.
- [`plans/locale-redesign.md`](plans/locale-redesign.md) — server-side locale: one PG table + one packet (GC_LOCALE) + client cache; per-language text owned by the server. Status: **F1 importer DONE (2026-08-12); wire slice next; ADR-0009 Accepted**.

## Decisions (ADRs)

| ADR | Title | Status |
|---|---|---|
| [0001](decisions/0001-postgresql-without-timescaledb-by-default.md) | PostgreSQL as primary DB, no TimescaleDB by default | Accepted (2026-08-06) |
| [0002](decisions/0002-unify-game-and-db.md) | Unify `game` + `db` into one process per region | Accepted |
| [0003](decisions/0003-reforge-workspace-rust-layout.md) | Rust workspace in `source/reforge` | Accepted (partially superseded by 0004) |
| [0004](decisions/0004-reforge-structure-and-names.md) | Flat workspace: `protocol`, `network`, `database`, `game_core` (renamed from `realm` 2026-08-13), `server_realms`; config TOML | Accepted (2026-08-10) |
| [0005](decisions/0005-postgresql-cutover-and-legacy-adapter.md) | PostgreSQL cutover (G-PG) + temporary legacy compatibility adapter (single canonical PG) | Accepted (2026-08-10, gate 4/4, implemented) |
| [0006](decisions/0006-legacy-wire-pack-compat-boundary.md) | Legacy wire/pack compatibility boundary, isolated/deletable (`protocol::legacy`) | Accepted (2026-08-10, implemented in F2a) |
| [0007](decisions/0007-no-partial-rust-in-legacy-client.md) | No partial Rust embedded in the legacy client (F0–F6) | Accepted (2026-08-10; Slint standalone deferred to F7 2026-08-12) |
| [0008](decisions/0008-data-layer.md) | Data layer: tokio-postgres 0.7, domain repos, save-by-event + WAL + idempotent replay | Accepted (2026-08-11) |
| [0009](decisions/0009-server-side-locale.md) | Server-side locale (server owns all text per language) | Accepted (2026-08-12) |
| [0010](decisions/0010-domain-boundaries-and-data-ownership.md) | Domain boundaries + bevy_ecs World + per-connection state + WorldStore | Accepted (2026-08-12) |
| [0011](decisions/0011-anti-hack-model.md) | Anti-hack model (always-on controls) | Accepted (2026-08-12) |
| [0012](decisions/0012-windows-native-runtime-wsl-on-demand.md) | Runtime hosting: native Windows + on-demand WSL oracle until F6 (C++ frozen, never rebuilt) | Accepted (2026-08-12, executed + verified) |

## Reference

- [`reference/README.md`](reference/README.md) — index of the technical contracts.
- [`reference/protocol/login-flow.md`](reference/protocol/login-flow.md) — byte-exact wire contract of the login flow (contract of the `protocol` crate).
- [`reference/protocol/legacy-compatibility.md`](reference/protocol/legacy-compatibility.md) — PanamaPack 151/hybrid-crypt 152/153 boundary (ADR-0006).
- [`reference/quests/quest-dsl.md`](reference/quests/quest-dsl.md) — quest DSL specification (replaces Lua).
- [`reference/legacy/language-system.md`](reference/legacy/language-system.md) — Language System integration state (read-only, historical).

## Guardrails

- [`guardrails/README.md`](guardrails/README.md) — index and purpose.
- [`guardrails/rust-rewrite.md`](guardrails/rust-rewrite.md) — rewrite work rules (property boundary, two source copies, ADR-before-code, evidence, minimal deps, no partial Rust in client).
- [`guardrails/legacy-compatibility.md`](guardrails/legacy-compatibility.md) — PanamaPack is a wire packet; `protocol::legacy` temporary; single canonical PostgreSQL; legacy client contract.
- [`guardrails/data-and-encoding.md`](guardrails/data-and-encoding.md) — CP949, `PROTO_FROM_DB`, `item_proto` names, PostgreSQL encoding, units vs cells.
- [`guardrails/operations.md`](guardrails/operations.md) — WSL memory, boot order, `sync` after deploy, IP check, no artifacts in git.
- [`guardrails/world-entry-crash.md`](guardrails/world-entry-crash.md) — 0xC0000374 postmortem (closed 2/2) and diagnostic lessons.

## History

- [`history/README.md`](history/README.md) — index of all superseded documents (read-only; no-hide-history rule).

## Documents tree

```
docs/
├── README.md            ← you are here (hub)
├── CURRENT.md           current snapshot (status source of truth)
├── DOCUMENTATION.md     documentation policy (mandatory reading before writing docs)
├── plans/               active design/migration plans (master-plan.md, server-rewrite.md)
├── decisions/           ADRs 0001–0012 (architecture decisions)
├── reference/           technical contracts (protocol, quests, legacy notes)
├── guardrails/          lessons and rules not to repeat (index + 5 files)
├── history/             superseded plans and specs (read-only; index)
└── (tutorials/, how-to/, explanation/ — created on demand, empty today)
```

## How documentation is maintained

- **Policy:** [`DOCUMENTATION.md`](DOCUMENTATION.md) — kinds, metadata, guardrail structure, review checklist.
- **Workflow:** librarian audits → fixer applies → oracle reviews → orchestrator commits (policy §10).
- **Sources of truth:** [`CURRENT.md`](CURRENT.md) (state) + [`../ROADMAP.md`](../ROADMAP.md) (plan) + [`../CHANGELOG.md`](../CHANGELOG.md) (history). When they disagree, fix in the same session.
