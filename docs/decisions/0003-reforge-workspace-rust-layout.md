---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Date: 2026-08-10
Last verified: 2026-08-10
Supersedes: —
Superseded by: ADR-0004 (flat layout, no `crates/`; `network`/`database`/`game_core` (renamed from `realm` 2026-08-13); `server_realms` binary with roles). Kept: the `source/reforge` folder, the property boundary over the C++ baseline, the verification policies.
---

# ADR-0003: Rust workspace in `source/reforge` — layout and policy of the new server

## Context

The project rewrites the Metin2 server in Rust (ROADMAP F0–F7, unified plan `docs/plans/server-rewrite.md` — the original draft remains as historical in `docs/history/2026-08-09-server-rewrite-draft.md`). The legacy C++ code lives in `source/{client,server,tools,deploy}` (the client pack in `source/tools/pack`) and must remain **intact and stable** during the whole migration (AGENTS.md rule: two source copies, the WSL one compiles the server; the C++ baseline is the test oracle).

The user decided (2026-08-10): the new Rust server goes in a **new folder** `source/reforge`, inside the same repository, to avoid modifying anything of the C++ baseline.

The F0 plan requires: "Cargo workspace with crates: `protocol`, `net`, `db`, `game`, `auth`" + "Implement the `protocol` crate (login flow) with golden tests of the spec structs". The byte-exact contract is already specified (`docs/reference/protocol/login-flow.md` — replaces the original spec `docs/superpowers/specs/2026-08-08-wire-protocol-login-flow.md`).

## Decision

1. **Location:** Cargo workspace in `source/reforge`, in the current repo (`origin → github.com/ryerdevs/reforge-core.git`). No separate repo or branch is created; the existing repo IS the project repo.
2. **Workspace layout** (one crate per layer, per plan F0):
   - `protocol` — byte-exact wire packets (client↔server and legacy peer), no dependencies (std only). First implementation: the full login flow of spec §3.
   - `net` — tokio transport (F1): framing, listener, `result > 0`/EAGAIN semantics.
   - `db` — data layer by domains (F3), internal crate (ADR-0002).
   - `game` — game logic by regions/ECS (F4+).
   - `auth` — auth mode of the binary (F2).
3. **Crate policy:** edition 2024, `resolver = "3"`, dependencies ONLY when the phase requires them (ponytail: YAGNI). `protocol` starts **zero-deps**: manual byte-exact LE serialization, no serde/bincode.
4. **Verification policy:** every crate compiles with `cargo build` and passes `cargo test` from the first commit; the `protocol` crate includes byte-exact golden tests built from the spec (and later from real tcpdump captures — F0 harness).
5. **Property boundary:** `source/reforge/**` is owned exclusively by the Rust workspace. Nobody edits `source/server`, `source/client` or `source/deploy` in this lane.

> Note (2026-08-10): the crate names and layout of point 2 are superseded by ADR-0004 (`net`→`network`, `db`→`database`, `game`→`realm`, `auth` crate→`network::auth` module, binary `server_realms`). The decisions that stay: the `source/reforge` location, the property boundary and the verification policies. (Lineage: `realm` was further renamed `game_core` on 2026-08-13, 42nd part.)

## Alternatives considered

### Separate repo for the Rust server

Rejected: the current repo is already named `reforge-core`; a second repo duplicates issue/CI management and complicates referencing the contract (specs, ADRs) that live in this repo. The plan's "GitHub repository" section already defines what goes into the repo (sources only); `source/reforge` meets that criterion.

### Separate `reforge` branch in the same repo

Rejected for now: the C++ baseline is stable and is the oracle; working on `main` with the new folder keeps the user's simple commit+push flow. If the Rust work starts hindering the baseline, a branch is evaluated then (YAGNI).

## Consequences

### Positive

- The C++ baseline is physically separated: zero cross-editing risk (the lesson of the two-source-copies disaster).
- The contract (specs/ADRs) and the Rust code live in the same repo: direct traceability.
- Multi-crate workspace ready to grow by phases without restructuring.

### Negative

- The repo will contain legacy and new code together; PR diffs can mix domains if the property boundary is not respected (rule 5 of this ADR).
- `cargo` needs a Rust toolchain on the build machine (verified: cargo/rustc 1.97.0 local; edition 2024 supported).

## Not decided in this ADR

- Internal concurrency model of `game` (regions + ECS) — own ADR (pending F0 of the plan).
- Domain boundaries / data ownership — own ADR.
- Quest engine (DSL) — own ADR.
- Anti-hack, regional channels, data layer, manifest — own ADRs (list in plan §13).
