---
Type: Reference
Status: Current
Audience: All
Last verified: 2026-08-30
---

# Document authority — where each question is answered

One page, one rule: **the live state lives in exactly two files**, and every
other document derives from, summarizes, or archives them. When two documents
disagree, this precedence list decides — not seniority, not habit.

## The two live files

| Question | Answer lives in |
|---|---|
| **What is pending, who owns it, what closes it?** | [`plans/gap-registry.md`](../plans/gap-registry.md) — per-row tracker (owner, state, evidence, dependency, risk, exit criterion) |
| **Where did we leave off, what is the current snapshot?** | [`progress.md`](../progress.md) — `Current` section + `Handoff` entries (rule 19, read at start / updated at close) |

Everything else is a view of those two plus the code.

## Precedence (when documents disagree)

1. **Fresh verification** — a command run today against HEAD (`cargo test`,
   `verify.ps1`, a wire smoke, a real-client check). Recorded evidence beats
   any narrative.
2. **Gap Registry** — the per-item tracker; its `Evidence` column names the
   commit or command that proves each claim.
3. **progress.md Current/Handoff** — the session handoff; dated entries.
4. **Accepted ADRs** — decisions (why), not status (what/when). Never updated
   to reflect progress, only to record decisions or supersessions.
5. **README, documentation/roadmap.md, ROADMAP.md, CHANGELOG.md** — summaries,
   phase history, and chronological record. A changelog entry is history, not
   proof of fresh execution.
6. **`documentation/history/`, `.omo/`, `.slim/`** — archives and external
   trackers. Read-only context; nothing there is current status.

## Document kinds and states

- Kinds: `Hub` (entry points), `Plan` (gap-registry, roadmaps), `Reference`
  (contracts, authority, schema), `Guardrail` (rules.md), `ADR` (decisions),
  `Snapshot` (dated captures, e.g. progress handoff entries).
- Document states: `Current` (maintained), `Historical` (frozen, correct for
  its date), `Superseded` (replaced by a named successor).
- Item states (registry): `OPEN`, `IN PROGRESS`, `BLOCKED`, `CLOSED`.

## Hard rules

- A claim of "done" must name a commit, a command, or a real-client check. A
  changelog line alone never proves fresh execution.
- Historical documents are never edited (no-hide-history); errors get a note,
  supersession gets a successor.
- Every live document carries `Type / Status / Audience / Last verified`.
- When code changes behavior, the same slice updates: the registry row (or a
  new one), `progress.md`, and — if a decision was made — an ADR first.

## Update duties at slice close

1. Registry rows: state + evidence.
2. `progress.md`: one dated Handoff entry + refresh `Current`.
3. `CHANGELOG.md`: one dated entry summarizing the block.
4. `README.md` status matrix: only if it still agrees with the two live files.
