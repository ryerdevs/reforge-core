---
Type: Reference
Status: Current
Audience: All
Last verified: 2026-09-02
---

# Document authority — where each question is answered

One page, one rule: **the live state lives in exactly two files**, and every
other document derives from, summarizes, or archives them. When two documents
disagree, this precedence list decides — not seniority, not habit.

## The two live files

The only live status sources are:

1. `documentation/plans/gap-registry.md` — owned work, state, evidence,
   dependency, risk, and exit.
2. `documentation/progress.md` — the current verified snapshot and session
   handoff.

| Question | Answer lives in |
|---|---|
| **What is pending, who owns it, what closes it?** | [`plans/gap-registry.md`](../plans/gap-registry.md) — per-row tracker (owner, state, evidence, dependency, risk, exit criterion) |
| **Where did we leave off, what is the current snapshot?** | [`progress.md`](../progress.md) — `Current` section + `Handoff` entries (rule 19, read at start / updated at close) |

Everything else is a view of those two plus the code. A fresh verification
command is evidence that may outrank written status; it is not a third live
status document and its durable result belongs in the registry or handoff.

## Precedence (when documents disagree)

1. **Fresh verification** — a command run today against HEAD (`cargo test`,
   `verify.ps1`, a wire smoke, a real-client check). Recorded evidence beats
   any narrative.
2. **Gap Registry** — the per-item tracker; its `Evidence` column names the
   commit or command that proves each claim.
3. **progress.md Current/Handoff** — the session handoff; dated entries.
4. **Accepted ADRs** — decisions (why), not status (what/when). Never updated
   to reflect progress, only to record decisions or supersessions.
5. **README, documentation/roadmap.md, CHANGELOG.md** — navigation,
   phase map, historical narrative, and chronological record. A changelog entry
   is history, not proof of fresh execution.
6. **`documentation/history/`, `.omo/`, `.slim/`** — archives and external
   trackers. Read-only context; nothing there is current status.

## Document kinds and states

- Kinds: `Hub` (entry points), `Plan` (gap-registry and active plans),
  `Reference` (contracts, authority, schema, and the phase map), `Guardrail`
  (rules.md), `ADR` (decisions), `History` (frozen historical narratives),
  and `Snapshot` (dated captures, e.g. progress handoff entries).
- Document states: `Current` (maintained), `Historical` (frozen, correct for
  its date), `Superseded` (replaced by a named successor).
- Item states (registry): `OPEN`, `IN PROGRESS`, `BLOCKED`, `CLOSED`.

The maintained phase map, [`documentation/roadmap.md`](../roadmap.md), keeps
`Type: Reference` as a navigation exception. It is a dated phase-level view,
not a per-item plan or a live status source.

## Hard rules

- A claim of "done" must name a commit, a command, or a real-client check. A
  changelog line alone never proves fresh execution.
- Historical documents are never edited (no-hide-history); errors get a note,
  supersession gets a successor.
- Every live document carries `Type / Status / Audience / Last verified`.
- README and reference/plan summaries must link to the two live files instead of
  restating volatile status.
- The fragment check is deliberately narrow: relative Markdown targets on the
  active documentation surface are checked against GitHub-compatible heading
  fragments; external URLs, `mailto:` links, generated artifacts, and archived
  document sources are outside its scope.
- When code changes behavior, the same slice updates: the registry row (or a
  new one), `progress.md`, and — if a decision was made — an ADR first.

## Update duties at slice close

1. Registry rows: state + evidence.
2. `progress.md`: one dated Handoff entry + refresh `Current`.
3. `CHANGELOG.md`: one dated entry summarizing the block.
4. `README.md`: update only stable scope and navigation; never make it a third
   live status source.
