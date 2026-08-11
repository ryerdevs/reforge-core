---
Type: Hub
Status: Current
Audience: Contributors, maintainers, agents
Last verified: 2026-08-11
---

# Guardrails — lessons and rules not to repeat

Guardrails are the project's **do-not-repeat rules**: hard-won lessons (crashes, data traps, environment limits) distilled into short, actionable rules with evidence. They are the operational counterpart of ADRs: ADRs decide **what** to build; guardrails constrain **how** we work and what we must not break.

Each guardrail file uses a fixed structure: **Rule** (the constraint), **Why** (context), **Evidence** (verified source, linked), **Consequence** (what happens if violated), **Status**.

> These files extract the essentials from `../../AGENTS.md` (the authoritative source of verified facts and rules) and from `../../CHANGELOG.md` (the evidence trail). When in doubt, read those two first.

## Index

| Guardrail | Scope |
|---|---|
| [`rust-rewrite.md`](rust-rewrite.md) | Property boundary legacy↔reforge, two source copies, ADR-before-code, tests/evidence, minimal dependencies, no partial Rust in the legacy client |
| [`legacy-compatibility.md`](legacy-compatibility.md) | PanamaPack is a wire packet (not a library/EIX/EPK), `protocol::legacy` is temporary, single canonical PostgreSQL (no dual DB), legacy client contract |
| [`data-and-encoding.md`](data-and-encoding.md) | CP949/EUC-KR server locale files, `PROTO_FROM_DB`, never change server `item_proto` names, PostgreSQL encoding, units vs cells, byte-exactness via `od`/hex |
| [`operations.md`](operations.md) | WSL memory limits, minimal start, boot order, `sync` after deploy, IP check after restart, no runtime/build artifacts in git, WSL memory pressure kills processes silently, never `cp` over a running binary, `wsl.exe` mangles quoted args, E2E residue sweep |
| [`agent-operations.md`](agent-operations.md) | Agent-team lessons: fresh sessions for reviewers, explicit report instructions, session reuse/reconcile, task labels, disjoint write scopes, config restart, exact-text verification before `edit` |
| [`world-entry-crash.md`](world-entry-crash.md) | Postmortem of the 0xC0000374 heap-corruption crash (closed 2/2) and the diagnostic lessons |

Related: architecture decisions live in [`../decisions/`](../decisions/); the current state in [`../CURRENT.md`](../CURRENT.md); the plan in [`../../ROADMAP.md`](../../ROADMAP.md).
