---
Type: Guardrail
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-13
---

# Guardrails — agent operations

Operational lessons about running the agent team. Each rule follows the standard guardrail structure. The team model is described in `explanation/agent-organization.md`.

## Rule 1 — Use a fresh session for every reviewer (oracle, fixer-as-adversary)

- **Why:** resumed reviewer sessions repeatedly returned empty results (observed 3+ times on 2026-08-10: the contract review, the net-crate review and the structure design all came back empty when resumed; fresh sessions delivered full reports).
- **Evidence:** board history of `ses_0160574aeffe...` (empty on resume, full report on fresh spawn); `ora-20`/`ora-6` errored. Under the current model the fixer is a reviewer too — same rule applies.
  - **2026-08-11 (writer lanes):** resumed `fix-2` (gate re-run) and `fix-4` (live fixes) returned EMPTY results. fix-2's work was done but unreported (verified in the environment); fix-4's was NOT done (3 tasks unapplied — verified DB/files/logs, redone directly by the orchestrator). **Rule for writers: an empty terminal result means UNVERIFIED — never trust the "completed" board state; verify the actual artifacts (DB tables, files, logs, processes) before reconciling or re-dispatching.**
  - **2026-08-12 (team reorg):** fixer is now the quality guardian (writes/expands tests, quality-scoped refactors) — the fresh-session rule applies to its ADVERSARIAL reviews; its own quality WRITING (tests/refactors) may resume when the context matches (same rule as coder).
- **Consequence:** resuming a reviewer wastes a round trip; retry with a fresh session.
- **Status:** Active.

## Rule 2 — End reviewer prompts with an explicit report instruction

- **Why:** the failures above were silent: the session "completed" without producing its final message.
- **Evidence:** prompts containing "escribe SIEMPRE tu informe completo en tu último mensaje" consistently produced full reports.
- **Consequence:** prompts without it risk a completed-but-empty review.
- **Status:** Active.

## Rule 3 — Reuse coder (implementer) sessions when the context matches

- **Why:** implementers keep the file context they read; resuming them avoids re-reading and re-deciding.
- **Evidence:** the implementer session chain (fix-1: net crate → hardening → retoques → flat layout → server_realms rename) resumed successfully multiple times with full reports. Under the current model the implementer role is `coder` (formerly `build`).
- **Consequence:** reusing an implementer across unrelated scopes bloats context; keep reuse within the same crate/folder.
- **Status:** Active.

## Rule 4 — Reconcile sessions after every lane

- **Why:** the board accumulated 20+ "completed, unreconciled" sessions during the docs reorg; unreconciled sessions cannot be resumed and pollute the board.
- **Evidence:** background-job-board listing after the documentation lanes (2026-08-10).
- **Consequence:** the orchestrator reconciles (reads the result, marks the lane closed) before dispatching the next dependent lane.
- **Status:** Active.

## Rule 5 — Use distinct, descriptive task labels

- **Why:** several unrelated audits all surfaced as "Oracle A: workspace reestructurado" because the label is inherited when a session is resumed; the board becomes misleading.
- **Evidence:** board entries `ora-12`..`ora-25` share the same stale objective.
- **Consequence:** the orchestrator gives every task a unique short description of its actual objective.
- **Status:** Active.

## Rule 6 — Parallel lanes must have disjoint write scopes

- **Why:** overlapping writers silently conflict (the docs hub was written before the moves finished and had stale links).
- **Evidence:** `docs/README.md` initially linked to `superpowers/...` paths that parallel lanes had already moved; fixed by a reconciliation lane.
- **Consequence:** before dispatching parallel fixers, declare each lane's file scope and check for overlap.
- **Status:** Active.

## Rule 7 — Configuration changes need an opencode restart

- **Why:** opencode loads config once at startup; it is not hot-reloaded.
- **Evidence:** `customize-opencode` skill (config loaded at start).
- **Consequence:** after editing `~/.config/opencode/*`, the user must restart opencode before the new models/skills apply.
- **Status:** Active.

## Rule 8 — Verify exact text with Select-String before `edit`

- **Why:** the edit tool fails on `oldString not found` when the target text contains characters that render differently from how they are stored (em-dash `—`, section sign `§`, arrows `→`, non-ASCII) — 2 failed edits on 2026-08-11. The rendered view and the stored bytes differ; blind oldString matching fails.
- **Evidence:** 2026-08-11 session: `edit` failed twice on strings containing `§`/`—`; matching the exact text from `Select-String`/`read` output (ASCII-only anchors) succeeded.
- **Consequence:** failed edits, wasted rounds, or editing the wrong occurrence after guessing.
- **Status:** Active.

## Rule 9 — Read-only loops that deliver nothing → the orchestrator implements directly

- **Why:** 2026-08-13 the quest DSL core went through 3 agent attempts that returned EMPTY reports without writing code (read-only loops); the orchestrator implemented it directly and it landed in one pass.
- **Evidence:** `quest_dsl` crate (ast/parser/family/render, 13 tests green, clippy clean — implemented directly by the orchestrator after 3 empty agent attempts; CHANGELOG 2026-08-13, 40th part).
- **Consequence:** an agent that loops read-only without producing artifacts wastes sessions and blocks the lane; the orchestrator cuts the loop, implements directly, and re-scopes or retrains the lane.
- **Status:** Active.
