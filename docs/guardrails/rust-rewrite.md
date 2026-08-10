---
Type: Guardrail
Status: Current
Audience: Contributors, agents
Last verified: 2026-08-10
---

# Guardrail: Rust rewrite work rules

Rules for working inside the Rust rewrite (`source/reforge`) and around the C++ baseline. Source of truth: `../../AGENTS.md` (work rules §, guardrails for the Rust rewrite) and `../../CHANGELOG.md`.

## 1. Never edit the legacy C++ baseline from the rewrite lane

- **Rule:** `source/reforge/**` is owned exclusively by the rewrite. Nobody edits `source/server`, `source/client` or `source/deploy` from this lane. The C++ baseline is the **oracle** for behavior parity.
- **Why:** the previous model's disaster came from editing both source copies inconsistently and from opposing protocol defines between client and server. Physical separation is the protection.
- **Evidence:** [ADR-0003](../decisions/0003-reforge-workspace-rust-layout.md) (ownership boundary); [ADR-0004](../decisions/0004-reforge-structure-and-names.md); `AGENTS.md` "CRITICAL RULE: two copies of source".
- **Consequence:** mixed-domain diffs, diverging protocol defines, and an unverifiable baseline. Review rejects any PR that touches legacy code from the rewrite lane.
- **Status:** Active (since 2026-08-10).

## 2. Two copies of server source — WSL compiles, Windows references

- **Rule:** `/home/m2/source` (WSL Debian-M2) is **the copy that compiles the server**; `C:\projects\Metin2\source` is a reference copy. After any change, sync both copies (diff/md5sum) and verify the protocol defines match on both sides. The client is compiled from the Windows copy.
- **Why:** compiling from the wrong copy bakes wrong paths (`VERSION.txt`) and produces binaries that do not match the source.
- **Evidence:** `AGENTS.md` "CRITICAL RULE: two copies of source"; the 2026-08-08 login-fix session (each fix applied to both copies, md5-verified).
- **Consequence:** deployed binaries diverge from source; debugging becomes archaeology.
- **Status:** Active.

## 3. ADR before implementing

- **Rule:** domain boundaries, data ownership, protocols, concurrency, failures and migration are decided **in writing (ADR) before** implementation.
- **Why:** the project's methodology (AGENTS.md rule 8; `../DOCUMENTATION.md` §9). Architecture is not improvised in code.
- **Evidence:** ADRs 0001–0007 in [`../decisions/`](../decisions/); the G-PG/F2 gate depends on ADR-0005 acceptance.
- **Consequence:** rejected work and re-architecture churn; the oracle review flags code that precedes its ADR.
- **Status:** Active.

## 4. Tests and evidence before claiming done

- **Rule:** no phase/task is complete without verification evidence (AGENTS.md rule 5): exact command + output. `cargo test` must pass from the first commit of every crate.
- **Why:** the project's advancement rule ("no phase is complete without verification evidence"); every CHANGELOG entry carries its evidence.
- **Evidence:** F0 `protocol` 30/30, F1 `network` 23/23, `server_realms` 3/3 → **56/56 tests** (`cargo test` in `source/reforge`); `../CURRENT.md` verified numbers table.
- **Consequence:** unverified checkboxes; the orchestrator rejects "done" without output.
- **Status:** Active.

## 5. Minimal dependencies (ponytail / YAGNI)

- **Rule:** stdlib/native before dependencies; a dependency enters only when the phase requires it (deferral list in `../../ROADMAP.md` "Dependency deferrals"). `protocol` stays zero-deps.
- **Why:** "do more with less" is the project motto; every dependency is a review and maintenance surface.
- **Evidence:** AGENTS.md rule 14; ROADMAP deferrals (clap/config-rs → F2, sqlx → G-PG/F3, bevy_ecs → F4, no mlua ever).
- **Consequence:** dependency creep; the adversarial review rejects unneeded crates.
- **Status:** Active.

## 6. No partial Rust embedded in the legacy client (F0–F6)

- **Rule:** no FFI/C-ABI Rust inside the legacy client binary during F0–F6. The Rust client ships standalone (Slint app in F5, wgpu client in F7).
- **Why:** the legacy client is a frozen, verified contract and the source of the 0xC0000374 heap-corruption history; a second implementation inside the oracle binary adds crash risk.
- **Evidence:** [ADR-0007](../decisions/0007-no-partial-rust-in-legacy-client.md) (Accepted for the already-agreed boundary).
- **Consequence:** build complexity and crash risk in the oracle; the ADR boundary is violated.
- **Status:** Active.

Related: [`legacy-compatibility.md`](legacy-compatibility.md) (wire/pack boundary), [`world-entry-crash.md`](world-entry-crash.md) (client crash history).
