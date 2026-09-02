---
Type: Reference
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-09-02
---

# Phase map — reforge-core

This page is the **sole phase map** (what F0–F7 mean and their phase-level
state at a dated glance). It does not track per-item, runtime, or worktree
status. The phase states below are a map snapshot, not a live status source:

- Live per-item state: [Gap Registry](plans/gap-registry.md)
- Live handoff / current snapshot: [progress.md](progress.md)
- Precedence when documents disagree: [document-authority.md](reference/document-authority.md)
- Long-form phase history: [`ROADMAP.md`](../ROADMAP.md) (historical compatibility narrative)

> **A0 navigation note (2026-09-02):** A0 classification is tracked by
> **ARQ-E** in the [Gap Registry](plans/gap-registry.md). Current status remains
> in the [Gap Registry](plans/gap-registry.md) / [progress handoff](progress.md)
> pair; this phase map is navigational, not a third current-state source.

## Phase map snapshot (2026-08-30)

| Phase | Meaning | State |
|---|---|---|
| F0 | Byte-exact wire protocol | DONE (2026-08-10) |
| F1 | Transport + handshake + auth wire | DONE (2026-08-10/11) |
| G-PG | MariaDB → PostgreSQL migration | DONE (2026-08-10; residuals absorbed into the registry) |
| F2 | Auth flows (LOGIN3, LOGIN_BY_KEY) | DONE (2026-08-11) |
| F3 | Data layer (repos, WAL durable, save-by-event) | DONE (2026-08-13; formally closed 2026-08-30) |
| F4 | World entry + ECS world | DONE (2026-08-11/13) |
| F5 | Gameplay breadth + scale validation | IN PROGRESS — breadth largely landed; ladder 250/500/1000 bots, CPU/tick and the ECS parallelism decision (ADR-0010) remain |
| F6 | Side-by-side parity vs the frozen C++ oracle and retirement of legacy pieces | NOT STARTED |
| F7 | Standalone Rust client | DEFERRED outside this repository ([ADR-0015](adr/0015-rust-only-public-repository.md)) |

Gate-2 execution blocks live in the registry: **G0** (caps/storage), **G1**
(gates/docs/deploy), **G2** (gameplay/content), **G3** (hygiene).

## Rules of the map

- These dated phase labels summarize historical milestone evidence; they do not
  replace the registry or handoff.
- A phase is DONE only when its milestone evidence is recorded in the registry
  or the handoff — never by wish.
- New work opens registry rows; it does not reopen phases here.
