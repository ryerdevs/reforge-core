---
Type: Plan
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-30
---

# Roadmap — reforge-core

This is the one-page roadmap. [`ROADMAP.md`](../ROADMAP.md) is the master plan;
[`progress.md`](progress.md) is the current handoff; and the
[Gap Registry](plans/gap-registry.md) owns per-item state and exit criteria.

## Current state

- **Rust server:** the public implementation is the workspace in
  `source/reforge` (`protocol`, `network`, `database`, `game_core`,
  `quest_dsl`, and `server_realms`).
- **Runtime:** native Windows PostgreSQL plus Rust `auth` and `channel` roles;
  the frozen C++ server is a local, on-demand parity oracle.
- **G0:** the safe item-stack cap is 200 because the current item-count wire is
  byte-sized. G0.1b–G0.1e are locally checked and await their Oracle Gates;
  storage cleanup remains open.
- **G1:** normal/ignored verification, formatting, documentation CI, and
  redeployment remain open; the G1.14b immutable-history decision is closed, and
  changelog freshness/current archive navigation are reconciled.
- **G2:** gameplay, social, quest, data-channel, and deferred-content gaps
  remain in the [Gap Registry](plans/gap-registry.md).
- **G3:** stale code comments and ignored-test policy remain open.
- **F7:** the standalone Rust client is deferred outside this repository by
  [ADR-0015](adr/0015-rust-only-public-repository.md). An external compatible
  client is used only for real-client verification.

## Completed foundations

- F0 protocol and F1 transport foundations are verified.
- PostgreSQL is the canonical database; the legacy adapter remains a parity
  boundary until F6.
- World entry, the ECS world, dynamic spawning, selected gameplay slices, and
  locale push/pull have verified implementations.

## Next actions

1. Complete Oracle Gates and remaining real-client or wire checks for G0.1b–e.
2. Execute G1: full verification, formatting, documentation/link checks,
   changelog reconciliation, and current-binary redeployment.
3. Execute selected G2 gameplay/content gaps with G3 hygiene work alongside
   them.
4. Keep F7 outside this repository until a separate client project is
   justified and records its own decisions.

## Evidence

Use the [live handoff](progress.md), [Gap Registry](plans/gap-registry.md),
[master roadmap](../ROADMAP.md), and [changelog](../CHANGELOG.md) before marking
an item complete. No item is complete without its recorded verification
evidence.
