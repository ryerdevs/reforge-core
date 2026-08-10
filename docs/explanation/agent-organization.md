---
Type: Explanation
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-10
---

# Agent team organization

This project runs as a small agent team. This document explains **who does what, when an agent is spawned or reused, and how work moves through gates**. The policy that governs documentation changes lives in `DOCUMENTATION.md` §10; operational lessons are in `guardrails/agent-operations.md`.

## Why a team

The rewrite touches three worlds at once: the legacy C++ server/client (the oracle), the Rust workspace (`source/reforge`), and PostgreSQL. No single agent should both write and verify its own work. The team separates **writing**, **attacking**, **researching**, **recon** and **control**, so every deliverable passes at least one adversarial review before it is accepted.

## Roster

| Role | Model (all `opencode-go/deepseek-v4-flash`, variant max unless noted) | Owns | When spawned |
|---|---|---|---|
| **Orchestrator** | v4-flash max | Planning, delegation, verification, docs, **the only one who commits** | always (this role) |
| **Fixer** | v4-flash max | Implementation of one bounded task | per task; one lane = one fixer |
| **Oracle-fixer** | v4-flash max | Adversarial review of ONE fixer's deliverable ("try to break it" + alignment with the plan) | per lane, after its fixer |
| **Oracle general** | v4-flash max | Meta-review of the whole change after all per-lane oracles passed | at each gate (phase, commit) |
| **Librarian** | v4-flash max | External research + documentation audits; never edits | research or doc audits |
| **Explorer** | v4-flash max | Fast codebase recon returning compressed context | discovery before planning |
| **Observer** | mimo-v2.5 | Visual/media analysis | images, PDFs, screenshots |
| **Designer** | v4-flash max | UI/UX polish | user-facing interfaces |

## Standard flow (one lane)

1. **Orchestrator** reads the plan (`ROADMAP.md` / `docs/CURRENT.md`), picks the next bounded task with an acceptance criterion.
2. **Fixer** implements it (write scope: one folder/crate; disjoint from every other running lane).
3. **Oracle-fixer** (fresh session) tries to break it and checks it matches the task, not just the code.
4. **Oracle general** (fresh session) supervises the whole change after the per-lane oracles.
5. **Orchestrator** verifies independently, applies corrections, updates docs, and **commits**.
6. Loop protocol (when used): write attempt result to `.opencode/loop-history/<loop>/history-NNN.md`, PASS stops, FAIL retries up to `maxAttempts`, then escalates.

## Spawn and reuse rules

- **Fresh session for every oracle review.** Resumed oracle sessions returned empty results repeatedly (2026-08-10) — see `guardrails/agent-operations.md` rule 1.
- **Reuse fixer sessions** when the new task is in the same context (fix-1 was resumed successfully several times).
- **Never reissue an unchanged task** after a rejection; adjust scope or context first.
- **Parallel lanes only with disjoint write scopes.** One lane = one fixer + one oracle-fixer.
- **Labels are informative:** give every task a distinct description; the board inherits the original objective when a session is resumed.

## Gates

- A task is "done" only with: code + tests + per-lane oracle verdict + (at phase gates) general oracle verdict + docs updated + commit with evidence.
- No phase is marked complete without its acceptance criterion verified (`AGENTS.md` rule 5).
- Architecture decisions are ADRs before code (`DOCUMENTATION.md` §9).

## Model note

All team agents share the same model (`opencode-go/deepseek-v4-flash`, variant max) including the built-in `build` agent (configured 2026-08-10), so delegation does not change model behavior — only role, skills and permissions.
