---
Type: Explanation
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-12
---

# Agent team organization

This project runs as a small agent team. This document explains **who does what, who reports to whom, and how work moves through gates**. The policy that governs documentation changes lives in `DOCUMENTATION.md` §10; operational lessons are in `guardrails/agent-operations.md`.

## Hierarchy

```
Orchestrator ── calls everyone; plans, delegates, verifies, commits
   └── Oracle — TEAM LEAD (second only to the orchestrator): supervision,
        │        architecture (ADRs), roadmap priorities
        ├── Coder — the expert writer (implements; owns the skills)
        │     └── Fixer — the quality guardian (Coder's adversary AND the
        │                   owner of tests, debugging, scalability)
        ├── Librarian — documentation maintainer (audits AND edits docs; research)
        ├── Explorer — recon (read-only)
        ├── Observer — visual analysis (read-only)
        └── Designer — UI/UX (edits UI)
```

## Why a team

The rewrite touches three worlds at once: the legacy C++ server/client (the oracle of truth), the Rust workspace (`source/reforge`), and PostgreSQL. No single agent should both write and verify its own work. The team separates **writing**, **hardening/quality**, **maintaining docs**, **recon** and **control**, so every deliverable passes at least one adversarial quality gate before it is accepted.

## Roster

| Role | Model (all `opencode-go/deepseek-v4-flash`, variant max unless noted) | Owns | When spawned |
|---|---|---|---|
| **Orchestrator** | v4-flash max | Planning, delegation, verification, docs, **the only one who commits** | always (this role) |
| **Oracle** | v4-pro max | **Team lead** — meta-review of the whole change (code + docs + plan + gates); architecture decisions (ADRs before code); roadmap priorities | at every gate (phase, commit) and when priorities/architecture are at stake |
| **Coder** (replaces the old `build`; built-in `build` is disabled) | v4-flash max | **The expert writer**: implementation of bounded features with best practices (clean-code, rust-*, ponytail, verification-before-completion); owns the skills implementation | direct implementation work |
| **Fixer** | v4-flash max | **The quality guardian**: Coder's adversary (finds bugs, structural problems, bad practices) AND the owner of the test suite, debugging (root cause) and scalability/maintainability — it WRITES tests and quality-scoped refactors | after Coder delivers, per task |
| **Librarian** | v4-flash max | **Documentation maintainer** — audits AND edits docs (applies its own audit fixes); external research | doc upkeep, research, doc audits |
| **Explorer** | v4-flash max | Fast codebase recon returning compressed context | discovery before planning |
| **Observer** | mimo-v2.5 | Visual/media analysis | images, PDFs, screenshots |
| **Designer** | v4-flash max | UI/UX polish | user-facing interfaces |

## Standard flow (one lane)

1. **Orchestrator** reads the plan (`ROADMAP.md` / `docs/CURRENT.md`), picks the next bounded task with an acceptance criterion. When in doubt about what to build next, asks the **Oracle** (priorities).
2. **Coder** implements it (write scope: one folder/crate; disjoint from every other running lane).
3. **Fixer** (fresh session) attacks Coder's output AND hardens it: findings with evidence + tests written/expanded + quality verdict.
4. **Coder** fixes the findings and applies the quality work; fixer re-checks (iterate until clean or findings are accepted by the orchestrator).
5. **Oracle** (fresh session) supervises the WHOLE change after the fixer: code, docs, plan consistency, gates, cross-cutting risks, architecture alignment. His verdict is final.
6. **Librarian** maintains the docs touched by the change (if any).
7. **Orchestrator** verifies independently, applies corrections, and **commits**.
8. Loop protocol (when used): write attempt result to `.opencode/loop-history/<loop>/history-NNN.md`, PASS stops, FAIL retries up to `maxAttempts`, then escalates.

## Specialization mechanics (agents are specialists, not generalists)

Each agent is defined by a dedicated file in `.opencode/agents/<role>.md` (local, gitignored): **mission prompt + permissions + model**. The mission is strict — what it does AND what it never does; permissions enforce it (**read-only: oracle, explorer, observer** — `edit: deny`; **writers: coder, fixer (tests/quality), librarian (docs only), designer**).

- Specialization comes from **scope restriction, not context volume**: each agent receives only its lane (files, acceptance criterion, evidence sources), never the whole project.
- Global skills exist for everyone (opencode mechanics), but each agent's mission limits it to its lane's skills (fixer → adversarial + rust-* + diagnose + improve-codebase-architecture; coder → rust-* + clean-code; librarian → documentation-*; explorer → graphify/codemap; designer → impeccable + brainstorming). MCP: **graphify** (code knowledge graph, rule 13) for orchestrator/coder/librarian — the only registered MCP; `context7`/`gh_grep` were declared on librarian but never registered (removed 2026-08-12, ponytail). GitHub stays on the `gh` CLI (repo not on GitHub yet — no GitHub MCP until F6/F7 PR workflow exists).
- More context does not make an agent smarter — it dilutes focus. A specialist gets **less, targeted** context.

## Value contract (what each agent uniquely contributes)

| Agent | Unique contribution | Must NOT |
|---|---|---|
| **Orchestrator** | Plan, delegate, reconcile, verify, docs, commit | Implement non-trivial code; review its own work |
| **Oracle** | Team lead: meta-review of the whole change, architecture (ADR) decisions, roadmap priorities, gate verdict | Write code, edit |
| **Coder** | Write code for one bounded task, with best practices | Decide architecture; review its own output as final; write the quality suite alone |
| **Fixer** | Attack Coder's output (bugs, structure, bad practices) AND own the test suite / debugging / scalability | Implement NEW features; silently change observable behavior; edit without evidence |
| **Librarian** | Maintain the documentation (audit + edit), research | Edit code; decide policy alone |
| **Explorer** | Recon: find files/patterns, return compressed context | Implement |
| **Observer** | Read images/PDFs/screenshots without polluting context | Implement |
| **Designer** | UI/UX visual and interaction quality | Copywriting, backend |

Delegation discipline (the antidote to "loose agents"):

- The orchestrator delegates **by lane**: recon → explorer, implementation → coder, hardening + tests → fixer, supervision/architecture/priorities → oracle, docs → librarian, visuals → designer/observer.
- The orchestrator handles directly **only**: coordination, verification commands, git/commit, and edits smaller than ~15 lines that are isolated and low-risk.
- **Never** implement a non-trivial feature directly while coder exists; **never** self-review own docs while an oracle exists.
- Every task dispatch names: the lane, the write scope, the acceptance criterion, and (for reviewers) the mandatory final report.
- **Every task ends with its documentation updated** (canonical docs, doc-comments, `Last verified` — policy `docs/DOCUMENTATION.md`); the librarian owns doc upkeep, coder/fixer list required doc updates when docs are outside their lane, the oracle verifies docs as part of supervision.

## Spawn and reuse rules

- **Fresh session for every reviewer** (oracle and fixer-as-adversary). Resumed reviewer sessions returned empty results repeatedly (2026-08-10) — see `guardrails/agent-operations.md` rule 1.
- **Reuse coder (implementer) sessions** when the new task is in the same context (resumed successfully several times as `build`).
- **Fixer quality work** (tests/refactors it writes itself) may resume its own session when the context matches; as adversary it stays fresh (rule 1).
- **Never reissue an unchanged task** after a rejection; adjust scope or context first.
- **Parallel lanes only with disjoint write scopes.**
- **Labels are informative:** give every task a distinct description; the board inherits the original objective when a session is resumed.

## Gates

- A task is "done" only with: code + tests + fixer verdict (adversarial + quality) + oracle verdict + docs updated + commit with evidence.
- No phase is marked complete without its acceptance criterion verified (`AGENTS.md` rule 5).
- Architecture decisions are ADRs before code (`DOCUMENTATION.md` §9); the oracle owns them.

## Model note

All team agents run `opencode-go/deepseek-v4-flash` variant max **except**: **Oracle → `opencode-go/deepseek-v4-pro`** (upgraded 2026-08-12, user decision — architecture reviews and designs pass through the stronger model; requires an opencode restart to load) and **Observer → `mimo-v2.5`** (native image analysis). `coder` (configured 2026-08-12, replacing the disabled built-in `build`). Delegation changes role, skills and permissions — and for the oracle, model strength.
