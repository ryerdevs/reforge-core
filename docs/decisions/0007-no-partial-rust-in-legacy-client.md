---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Date: 2026-08-10
Last verified: 2026-08-10
Supersedes: —
Superseded by: —
---

# ADR-0007: No partial Rust embedded in the legacy client during F0–F6

> **Status note:** Accepted — scope limited to the already-agreed boundary (see Decision).
>
> **AMEND (2026-08-12):** the standalone Slint app is **deferred from F5 to F7** — oracle review of the consolidated plan (`docs/plans/master-plan.md`, recommendation H.3): built in F5 it would target the legacy wire and be re-ported against the new protocol at F7 (double protocol work) and it unblocks nothing server-side. The boundary decision itself (no partial Rust embedded in the legacy client during F0–F6) is unchanged.

## Context

The legacy client (v40999) is frozen as the wire contract during F0–F6; only additive C++ changes ≤1 week that unblock the server side are allowed (ROADMAP principle 6 — already agreed with the user before this ADR). Embedding Rust incrementally inside the legacy client (e.g., FFI/C-ABI modules compiled into the client binary) was discussed as a way to modernize the client during the rewrite.

## Decision

**No partial Rust embedded in the legacy client during F0–F6.** The Rust client ships standalone: the Slint login/select/HUD UI is built as a standalone app **at F7** (deferred from F5 — see AMEND above) and integrated into the new client (bevy + Slint — engine decided 2026-08-12, replacing the wgpu-from-scratch plan; ADR-0010 §2 shares the same ECS ecosystem). Client-side work during F0–F6 remains limited to the agreed additive C++ changes.

This ADR is **Accepted only for the boundary that was already agreed** (client frozen as contract; no partial Rust embedding during F0–F6; standalone/new client later). Everything else about the new client — engine, protocol, encryption, pack formats — remains open and needs its own ADRs at F7.

## Alternatives considered

### Progressive Rust embedding in the legacy client (FFI modules)

Rejected for F0–F6: the legacy client is a frozen, verified contract and the source of the 0xC0000374 heap-corruption history; embedding a second implementation inside the oracle binary adds crash risk and build complexity while the rewrite target is the server.

### Rewrite the client first

Rejected: the server-first order is already agreed (ROADMAP F0–F6 server, F7 client); the client is a view, never a source of truth (principle 3).

## Consequences

- The legacy client stays C++ until F7; the wire contract remains stable.
- Rust client work during F0–F6 is limited to the additive C++ changes; the standalone Slint app is deferred to F7 (2026-08-12 review H.3).
- The F7 client ADRs will decide engine/protocol/encryption; this ADR does not pre-decide them.
