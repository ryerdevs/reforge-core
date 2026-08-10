---
Type: Guardrail
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-10
---

# Guardrails — agent operations

Operational lessons about running the agent team. Each rule follows the standard guardrail structure. The team model is described in `explanation/agent-organization.md`.

## Rule 1 — Use a fresh session for every oracle review

- **Why:** resumed oracle sessions repeatedly returned empty results (observed 3+ times on 2026-08-10: the contract review, the net-crate review and the structure design all came back empty when resumed; fresh sessions delivered full reports).
- **Evidence:** board history of `ses_0160574aeffe...` (empty on resume, full report on fresh spawn); `ora-20`/`ora-6` errored.
- **Consequence:** resuming an oracle wastes a round trip; retry with a fresh session.
- **Status:** Active.

## Rule 2 — End oracle prompts with an explicit report instruction

- **Why:** the failures above were silent: the session "completed" without producing its final message.
- **Evidence:** prompts containing "escribe SIEMPRE tu informe completo en tu último mensaje" consistently produced full reports.
- **Consequence:** prompts without it risk a completed-but-empty review.
- **Status:** Active.

## Rule 3 — Reuse fixer sessions when the context matches

- **Why:** fixers keep the file context they read; resuming them avoids re-reading and re-deciding.
- **Evidence:** `fix-1` was resumed successfully multiple times (net crate → hardening → retoques → flat layout → server_realms rename) with full reports each time.
- **Consequence:** reusing a fixer across unrelated scopes bloats context; keep reuse within the same crate/folder.
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
