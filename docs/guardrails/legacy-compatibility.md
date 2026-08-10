---
Type: Guardrail
Status: Current
Audience: Contributors, agents
Last verified: 2026-08-10
---

# Guardrail: legacy compatibility

Rules about the legacy client contract, the legacy wire constructs, and the database topology. Source of truth: [`../../AGENTS.md`](../../AGENTS.md) (protocol facts), [`../reference/protocol/legacy-compatibility.md`](../reference/protocol/legacy-compatibility.md), ADRs 0005/0006.

## 1. PanamaPack is a wire packet, not a library or a pack format

- **Rule:** PanamaPack (header **151**, 289B) is a **wire packet** the server sends at auth success; hybrid-crypt (152/153) likewise. They are NOT a library, an `.eix`/`.epk` pack format, or a file format. Do not confuse them with the client pack tools (TEA/LZO, PackMakerLite, DumpProto — see [`operations.md`](operations.md) and the pack tooling in `source/tools/pack`).
- **Why:** the legacy client needs 151/152/153 to decrypt its pack entries after auth; confusing the layers leads to implementing the wrong thing in the wrong crate.
- **Evidence:** [`../reference/protocol/legacy-compatibility.md`](../reference/protocol/legacy-compatibility.md) (packet inventory, layouts, deletion list); `../reference/protocol/login-flow.md` §4a step 6; [ADR-0006](../decisions/0006-legacy-wire-pack-compat-boundary.md).
- **Consequence:** wrong implementation target; the adversarial review fails the task.
- **Status:** Active.

## 2. `protocol::legacy` is temporary and isolated

- **Rule:** all legacy-client-only packets (PanamaPack 151/289B, hybrid-crypt 152/153) live behind the `protocol::legacy` module/feature boundary — never in the new protocol core. The whole layer is **deleted at the new client (F7)**.
- **Why:** if implemented inline, the new wire core accumulates legacy quirks that F7 must strip out again — a hunt instead of a deletion.
- **Evidence:** [ADR-0006](../decisions/0006-legacy-wire-pack-compat-boundary.md); [`../reference/protocol/legacy-compatibility.md`](../reference/protocol/legacy-compatibility.md).
- **Consequence:** F7 deletion debt; new-client protocol polluted with legacy behavior.
- **Status:** Active (boundary Proposed until ADR-0006 acceptance; direction fixed 2026-08-10).

## 3. Single canonical PostgreSQL — no dual DB

- **Rule:** one canonical PostgreSQL 18 is the only operational database. The C++ baseline operates on the **same PostgreSQL** through a temporary compatibility adapter (its MySQL `libsql` layer is bridged). MariaDB is used **only as the migration/export source** — never as a second operational DB, and there is no `direct-sql (MariaDB)` backend in the Rust `database` crate.
- **Why:** the user decision 2026-08-10: two operational databases double the surface and concentrate migration risk at the end.
- **Evidence:** [ADR-0005](../decisions/0005-postgresql-cutover-and-legacy-adapter.md) (Proposed; G-PG gate); `../plans/server-rewrite.md` §5.5; `../../ROADMAP.md` phase G-PG.
- **Consequence:** dual-store claims in code/docs are rejected; F2 stays blocked until ADR-0005 acceptance.
- **Status:** Active (direction fixed; ADR-0005 pending acceptance).

## 4. Legacy client contract: frozen, pull-based, no pushes

- **Rule:** the legacy client (v40999) is the frozen wire contract during F0–F6. Only additive C++ changes ≤1 week that unblock the server side are allowed. Additive packets must be **pull-based**: the client asks, the server answers — the server never pushes unknown headers.
- **Why:** the old client, faced with an unknown header, discards the whole receive buffer → a push desynchronizes the session.
- **Evidence:** `../plans/server-rewrite.md` §5.8 (the immovable contract, `PythonNetworkStream.cpp:571-578, 654-662`); [ADR-0007](../decisions/0007-no-partial-rust-in-legacy-client.md).
- **Consequence:** desynchronized legacy sessions; the oracle review rejects server-initiated additive traffic.
- **Status:** Active.

Related: [`rust-rewrite.md`](rust-rewrite.md), [`data-and-encoding.md`](data-and-encoding.md).
