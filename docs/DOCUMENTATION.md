---
Type: Hub
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-10
---

# DOCUMENTATION — Mandatory documentation policy

This is the policy for every document under `docs/`. If you write or edit documentation here, you must follow it. The review checklist at the end is part of every documentation review.

## 1. Language

- All documentation is written in **clear technical English**.
- Existing historical documents written in other languages are **read-only** and are not translated (see §5).

## 2. Diátaxis modes

Every technical document belongs to exactly one of the four Diátaxis modes. The mode decides its structure and audience:

| Mode | Directory | Orientation | Answers | Shape |
|---|---|---|---|---|
| **Tutorial** | `docs/tutorials/` | Learning | "How do I learn this?" | Step-by-step lesson with a working result; no shortcuts |
| **How-to** | `docs/how-to/` | Goal | "How do I solve this task?" | Steps for a specific, real task; assumes background |
| **Reference** | `docs/reference/` | Information | "What is the exact definition?" | Exhaustive, precise, structured (tables, lists); no narrative |
| **Explanation** | `docs/explanation/` | Understanding | "Why is it like this?" | Prose that explains context and rationale; no steps |

- A document must not mix modes. If a topic needs two modes, split it into two documents and cross-link them.
- **Empty Diátaxis directories are not created.** A mode without content does not appear in the hub as a link. When the first real document of a mode is written, create its directory and add the link then (under demand, not in advance).

### Document kinds beyond Diátaxis

Project documents that are not technical content use their own kinds and directories:

| Kind | Directory | `Type` | Purpose |
|---|---|---|---|
| **Plan** | `docs/plans/` | `Plan` | Active design/migration plans (e.g. `server-rewrite.md`) |
| **Decision (ADR)** | `docs/decisions/` | `Decision` | Architecture decision records; one decision per ADR |
| **Guardrail** | `docs/guardrails/` | `Guardrail` | Lessons/rules not to repeat, with evidence (see §3.1) |
| **History** | `docs/history/` | `History` | Read-only superseded plans, specs and snapshots (see §5) |
| **Hub** | any | `Hub` | Navigation/meta documents (`docs/README.md`, `docs/DOCUMENTATION.md`, `docs/history/README.md`, `docs/guardrails/README.md`) |
| **Snapshot** | any | `Snapshot` | Point-in-time status documents (e.g. `docs/CURRENT.md`) |

Plans never live inside `decisions/`: ADRs record **decisions**, plans record **how the work is sequenced**. They appear as separate sections in the hub.

## 3. Metadata block

Every new or edited document starts with a YAML metadata block:

```yaml
---
Type: Tutorial | How-to | Reference | Explanation | Plan | Decision | Guardrail | History | Hub | Snapshot
Status: Current | Proposed | Accepted | Superseded | Historical
Audience: Contributors | Maintainers | Operators | All
Last verified: YYYY-MM-DD
---
```

- `Last verified` is updated whenever the document's claims are re-checked against the code (not merely when the file is touched).
- `Type` and `Status` are written with capital initial letter exactly as above.
- **`Type: Hub`** — navigation/meta documents; they index other documents instead of containing technical content.
- **`Type: Snapshot`** — point-in-time status documents; they describe the state at a specific date, not standing rules.
- **`Type: Decision`** — ADRs. Status: `Accepted` (decided), `Proposed` (direction drafted, pending acceptance), or `Superseded` (with a note naming the successor, as ADR-0003 does for ADR-0004).
- **`Type: Guardrail`** — each guardrail file is a set of rules with the structure **Rule / Why / Evidence / Consequence / Status** (see §3.1).
- **`Type: History` / `Status: Historical`** — read-only records (everything under `history/`, and any document whose claims describe a past state, e.g. `reference/legacy/language-system.md`). See §5.

### 3.1 Guardrail structure

Every rule inside a guardrail file has exactly five fields:

- **Rule** — the constraint, stated as an imperative.
- **Why** — the context that makes the rule necessary.
- **Evidence** — verified source, linked (AGENTS.md section, CHANGELOG entry, ADR, reference doc). No invented evidence.
- **Consequence** — what happens if the rule is violated.
- **Status** — Active / Closed / Deprecated.

Guardrails extract the essentials from `../AGENTS.md` (the authoritative facts) and `../CHANGELOG.md` (the evidence trail) — they **link, never duplicate** (AGENTS.md stays the source of truth).

## 4. One canonical doc per concept

- Each concept (protocol, phase, gate, crate, tool) has **exactly one canonical document**. Everything else links to it.
- To update a concept, **edit its canonical document**. Do not create "v2" files next to it.
- A new canonical document requires the orchestrator's go-ahead; otherwise extend or link to the existing one.

## 5. Historical documents are read-only

- Documents that record a past state (e.g. `reference/legacy/language-system.md`, everything in `history/`) are **read-only**: no edits, no deletions.
- During the one-time documentation migration, a provenance/metadata header may be added without changing the historical body. Once that migration lands, the file is frozen and further corrections belong in its canonical successor or in the changelog.
- If a historical document's claims are no longer true, do **not** fix it in place — record the change in `../CHANGELOG.md` and, if the topic is still live, update its canonical successor.
- Reading order: historical docs inform; `CURRENT.md` and `../ROADMAP.md` are the truth.
- When a plan or spec is superseded, move it to `history/` (with a pointer in the changelog); keep the canonical document in its Diátaxis directory.

## 6. Current status source of truth

- `docs/CURRENT.md` is the **source of truth for current status**: what is done, what is blocked, what is next.
- `../ROADMAP.md` is the source of truth for the **plan**: phases, tasks, acceptance criteria, milestones.
- `../CHANGELOG.md` is the source of truth for **history**: every verified change with evidence.
- When these disagree, resolve the disagreement in the same session and record it in the changelog. Never leave two documents asserting different current states.

## 7. Phase documentation requirement

Every ROADMAP phase has a documentation entry that lists, at minimum:

1. **Code paths** — the crates/modules that implement it (e.g. `source/reforge/protocol`).
2. **Acceptance** — the task's acceptance criteria (from the ROADMAP).
3. **Command** — the exact verification command (e.g. `cargo test -p protocol`).
4. **Evidence** — what output proves completion (test counts, captured packets, diff).
5. **Next action** — the single next step, or "blocked on X" with the blocking gate.

Template for a phase status entry (used in `CURRENT.md` and phase docs):

```markdown
| Phase | Status | Code | Acceptance | Command | Evidence | Next action |
|---|---|---|---|---|---|---|
```

## 8. Links

- Use **relative links** between documents (`../ROADMAP.md`, `reference/protocol/login-flow.md`); never absolute filesystem paths.
- Links must point to **existing paths**; never link to empty or missing categories. Verify before writing.
- Never link to a stale name: the workspace crates are `protocol`, `network`, `database`, `game_core`, `server_realms` (ADR-0004; `realm` → `game_core` 2026-08-13). Old names (`net`, `db`, `game`, `realm`, `auth` crate) must not appear in new links or prose describing the current workspace. The same rule applies to moved documents: canonical paths are `plans/server-rewrite.md`, `reference/protocol/login-flow.md`, `reference/quests/quest-dsl.md` — never the superseded `superpowers/...` paths.

## 9. ADRs before architecture code

- Any architectural decision (domain boundaries, data ownership, protocols, concurrency, failure, migration) is written as an **ADR in `decisions/` before** the corresponding code is written.
- ADR statuses: `Proposed` → `Accepted` | `Superseded`. When an ADR is superseded, add a status note at the top naming the successor, as ADR-0003 does for ADR-0004.
- A decision that is agreed but not yet written down is **Proposed**, not Accepted — record it in `CURRENT.md` as a working decision with its pending gate (e.g. G-PG) until the ADR lands.

## 10. Documentation workflow (librarian / fixer / oracle / orchestrator)

Documentation changes follow the same role split as code changes:

- **Librarian** — **maintains** the docs: audits them AND edits them (applies its own audit fixes), keeping them consistent with this policy; proposes policy improvements. Owner of documentation upkeep.
- **Fixer** — build's adversary: reviews code changes adversarially (stale docs are findings); never edits.
- **Oracle** — supreme supervisor: reviews the whole change (code + docs + plan) after the per-lane reviewers; never edits.
- **Orchestrator** — accepts/rejects the audit, coordinates lanes, and **commits** at session close. Only the orchestrator commits.

Every session ends with the changes logged in `../CHANGELOG.md` (rule 9 of AGENTS.md).

### Agent team organization

The full agent roster (hierarchy, models, skills, spawn/reuse rules, session discipline, gates and the loop protocol) is documented in [`explanation/agent-organization.md`](explanation/agent-organization.md). Operational lessons agents must not repeat are in [`guardrails/agent-operations.md`](../guardrails/agent-operations.md).

Session discipline (mandatory): fresh session for every reviewer (oracle, fixer); end reviewer prompts with an explicit "report in your final message" instruction; reuse build (implementer) sessions within the same scope; reconcile every lane before dispatching the next; disjoint write scopes for parallel lanes.

## 11. Review checklist

Every documentation change is reviewed against this checklist:

- [ ] Language: clear technical English; no untranslated leftovers in new docs (historical files stay verbatim).
- [ ] Kind/mode: one type matching the directory (`tutorials/`, `how-to/`, `reference/`, `explanation/`, `plans/`, `decisions/`, `guardrails/`, `history/`).
- [ ] Metadata block present and accurate (`Type`, `Status`, `Audience`, `Last verified`).
- [ ] Guardrails: every rule has Rule / Why / Evidence / Consequence / Status; evidence linked, not invented.
- [ ] Canonical: did not duplicate an existing concept; links to its canonical document.
- [ ] No edits to read-only historical documents; changes recorded in `../CHANGELOG.md`.
- [ ] Status claims match `CURRENT.md` / `../ROADMAP.md`; no "done" for planned gates.
- [ ] Every phase entry has code paths, acceptance, command, evidence, next action.
- [ ] All links relative, verified to exist, and not pointing to empty/missing categories.
- [ ] ADRs updated before architecture code; statuses Proposed/Accepted/Superseded correct.
- [ ] `Last verified` updated with the date of the latest fact check.
