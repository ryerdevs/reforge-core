---
Type: Decision
Status: Proposed
Audience: Contributors, maintainers
Date: 2026-08-10
Last verified: 2026-08-10
Supersedes: —
Superseded by: —
---

# ADR-0005: PostgreSQL cutover and temporary legacy compatibility adapter

> **Status note:** Proposed (needs confirmation — not accepted yet). The direction was fixed by the user on 2026-08-10; the G-PG gate closes only on acceptance.

## Context

ADR-0001 established PostgreSQL as the primary database of the future Rust server, while the C++ baseline keeps MySQL/MariaDB during the compatibility phase. The Rust rewrite is now approaching F2 (auth): the `protocol` (F0) and `network` (F1) crates are implemented (56/56 tests) and the auth role of `server_realms` will need persistence for accounts, sessions and `dwLoginKey` tokens.

**User decision (2026-08-10): a single canonical PostgreSQL — no dual operational databases.** The earlier formulation of this ADR (C++ stays on MariaDB while Rust runs on PG until F6) is rejected. MariaDB is used only as the migration/export source (initial data extraction), never as a second operational database of the system.

The legacy C++ server speaks MySQL through `libsql`; the legacy client (v40999) is the frozen wire contract during F0–F6 (ROADMAP principle 6, ADR-0007). Two consequences of the user decision:

- The Rust side is written against PostgreSQL from the start — no MySQL-backed Rust path (per ADR-0001, no MySQL API patterns in the new server).
- The C++ baseline must keep working unchanged during the transition, but on the **same PostgreSQL**: a temporary compatibility adapter bridges its MySQL-speaking `libsql` layer to PostgreSQL (wire/SQL translation). The C++ baseline **source** stays untouched (frozen oracle, ADR-0003); only its runtime data path goes through the adapter.

## Decision (proposed)

1. **Cut the Rust server over to PostgreSQL 18 before F2** (new phase **G-PG**, before F2 in ROADMAP). The `database` crate targets PostgreSQL from the start (sqlx/PgPool is the candidate per the ADR-0001 recommendation; the concrete crate decision is a G-PG task — ADR-0001 left it undecided).
2. **A single canonical PostgreSQL** is the only operational database. A **temporary legacy compatibility adapter** lets the C++ baseline operate on that same PostgreSQL (its `libsql` speaks MySQL — the adapter translates); the legacy client behavior is unchanged. MariaDB is used **only as the migration/export source**: an initial dump/extraction to seed PostgreSQL, then it is retired. The adapter is temporary by contract — thin, explicit, removed at F6 (same rule as the ADR-0002 shim).
3. **F2 is gated by this ADR and by the G-PG cutover.** No auth work is done on a MySQL-backed Rust path; the F2a/F2b split assumes PostgreSQL underneath.

## Alternatives considered

### Dual-store: C++ keeps running on MariaDB while the Rust server runs on PostgreSQL (until F6)

Rejected (by the user, 2026-08-10): two operational databases double the surface, the data split between stores and the migration risk concentrated at the end. The single canonical PostgreSQL removes the second store; MariaDB is reduced to a migration/export source.

### Write F2 against MariaDB and migrate to PostgreSQL later (original plan, F3)

Rejected: new code would be written against the legacy SQL API and then migrated — the ADR-0001 outcome ("no MySQL patterns in the new server") would be deferred into the middle of the gameplay port.

### Cut over at F6 (full replacement)

Rejected: F3–F5 (data layer, world entry, gameplay) would run on the legacy store, duplicating the coupling the rewrite removes, and the cutover risk concentrates at the end instead of early.

### No adapter — modify the C++ baseline to speak PostgreSQL directly

Rejected: the C++ `libsql` layer is MySQL-specific; rewiring the frozen baseline contradicts the "oracle baseline untouched" rule (ADR-0003) and would destabilize the verified login flow. The adapter keeps the baseline source intact.

## Consequences

- **No dual-store**: one canonical PostgreSQL; MariaDB exists only as the migration/export source (initial data extraction), not as a second operational DB.
- The adapter is temporary by contract: thin, explicit, removed at F6 (same rule as the ADR-0002 shim). It must translate the MySQL wire/SQL of the legacy `libsql` layer to PostgreSQL without changing C++ behavior.
- Migration tooling and a data-comparison harness are G-PG deliverables (schema mapping, types/defaults/`ENUM`/`SET`/`UNSIGNED` adaptation per ADR-0001 negative consequences).
- F2 start date depends on G-PG completion; ROADMAP marks F2 as blocked.

## Gate (F2 unblocking checklist)

- [ ] ADR-0005 accepted (Proposed → Accepted)
- [ ] PostgreSQL 18 provisioned as the Rust server's backing store (schemas per domain, RLS)
- [ ] Legacy compatibility adapter working — C++ baseline and legacy client behavior unchanged
- [ ] Migration groundwork + data comparison harness in place

## Not decided in this ADR

- The concrete PostgreSQL crate (`sqlx`/PgPool recommendation stands — G-PG task).
- The final schema design (domain-module split, RLS details) — G-PG/F3.
- The exact adapter boundary (which peer headers/sessions it bridges) — G-PG.
