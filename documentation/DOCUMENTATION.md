---
Type: Hub
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-30
---

# Documentation policy

This is the mandatory policy for documents in `documentation/`. It keeps the
project's technical writing clear, navigable, and consistent with the current
server repository boundary.

## 1. Language

- Write new and edited documentation in clear technical English.
- Keep historical documents read-only and do not translate or rewrite them.

## 2. Document kinds

Every document has exactly one kind:

| Kind | Directory or location | Purpose |
|---|---|---|
| Tutorial | `tutorials/` | Learning-oriented, step-by-step instruction |
| How-to | `how-to/` | A focused recipe for a real task |
| Reference | `reference/` | Exact, structured technical information |
| Explanation | `explanation/` | Context and rationale without a procedure |
| Plan | `plans/` or a root plan such as `ROADMAP.md` | Active sequencing and acceptance criteria |
| Decision | `adr/` | One architectural decision per ADR |
| Guardrail | `guardrails/` | Rules that prevent repeat failures |
| History | `history/` | Read-only superseded plans, records, and snapshots |
| Hub | Navigation or documentation metadata | Indexes and explains the documentation set |
| Snapshot | A dated status document | Point-in-time project status |

Do not create an empty category merely to reserve a directory. Create a
category when its first document is needed.

## 3. Metadata

Every new or edited document starts with this YAML block:

```yaml
---
Type: Tutorial | How-to | Reference | Explanation | Plan | Decision | Guardrail | History | Hub | Snapshot
Status: Current | Proposed | Accepted | Superseded | Historical
Audience: Contributors | Maintainers | Operators | All
Last verified: YYYY-MM-DD
---
```

Use `Type: Decision` for ADRs. Use `Status: Historical` for documents under
`history/`. Update `Last verified` when the documented claims are checked, not
merely because a file was touched.

### Guardrail structure

Each rule in a guardrail document has exactly these fields:

- **Rule** — the imperative constraint.
- **Why** — the reason the constraint exists.
- **Evidence** — a verified link to an ADR, rule, changelog entry, source path,
  or reference document.
- **Consequence** — what happens when the rule is violated.
- **Status** — `Active`, `Closed`, or `Deprecated`.

## 4. Canonical documents

- Keep one canonical document per concept. Link to it instead of creating a
  duplicate or a `v2` file.
- Extend the existing canonical document when updating a concept.
- Keep plans, decisions, references, and snapshots separate; a plan does not
  replace its ADR.

## 5. History is read-only

- Do not edit or delete documents under `documentation/history/`.
- When a decision changes, add a new ADR and record the change in
  [`CHANGELOG.md`](../CHANGELOG.md); do not rewrite the old record.
- If a historical link is stale, link to the historical document from a live
  hub or canonical successor rather than changing the historical body.

## 6. Current sources of truth

- [`documentation/progress.md`](progress.md) is the current status and handoff:
  what is done, blocked, and next.
- [`ROADMAP.md`](../ROADMAP.md) is the master plan and sequence.
- [`CHANGELOG.md`](../CHANGELOG.md) is the chronological evidence record.
- [`documentation/README.md`](README.md) is the documentation hub.

When these sources disagree, reconcile them in the same session and record the
verified change in the changelog.

## 7. Phase documentation

Each active roadmap phase or gate entry identifies, at minimum:

1. the code paths involved;
2. the acceptance criterion;
3. the exact verification command;
4. evidence that proves completion; and
5. the single next action or its blocking gate.

## 8. Links

- Use relative links between repository documents.
- Verify every link points to an existing file or a non-empty documented
  directory.
- Use current paths and names: `documentation/`, `adr/`, `plans/`,
  `reference/`, `protocol`, `network`, `database`, `game_core`,
  `quest_dsl`, and `server_realms`.
- Do not use superseded `docs/`, `decisions/`, `superpowers/`, or old crate
  names in new links or current guidance. Historical records may retain their
  original wording.

## 9. ADRs before architecture code

Record decisions about boundaries, data ownership, protocols, concurrency,
failures, migrations, or other expensive-to-reverse architecture in an ADR
before implementing them. Use the lifecycle `Proposed` → `Accepted` or
`Superseded`, and name the successor in a superseded ADR.

## 10. Documentation workflow

- The librarian audits and maintains documentation.
- The fixer reviews code and documentation adversarially but does not own doc
  edits.
- The oracle reviews architecture and the complete change.
- The orchestrator coordinates lanes and commits the reconciled result.
- Every session updates the canonical docs, `ROADMAP.md`,
  [`CHANGELOG.md`](../CHANGELOG.md), and [`progress.md`](progress.md) when
  project knowledge changes.

## Review checklist

- [ ] Clear technical English; historical files remain unchanged.
- [ ] Exactly one document kind and an accurate metadata block.
- [ ] Guardrails contain Rule, Why, Evidence, Consequence, and Status.
- [ ] The canonical document was updated instead of creating a duplicate.
- [ ] Historical documents were not edited or deleted.
- [ ] Status claims agree with `progress.md` and `ROADMAP.md`.
- [ ] Active phase entries include code, acceptance, command, evidence, and next
      action.
- [ ] Relative links resolve to existing paths.
- [ ] Architecture decisions have an ADR with the correct lifecycle status.
