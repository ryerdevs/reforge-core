---
Type: Hub
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-09-02
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
| Plan | `plans/` | Active sequencing and acceptance criteria |
| Decision | `adr/` | One architectural decision per ADR |
| Guardrail | `guardrails/` | Rules that prevent repeat failures |
| History | `history/` | Read-only superseded plans, records, and snapshots |
| Hub | Navigation or documentation metadata | Indexes and explains the documentation set |
| Snapshot | A dated status document | Point-in-time project status |

The maintained phase map, [`roadmap.md`](roadmap.md), intentionally carries
`Type: Reference` as a navigation exception. It is a dated phase-level view,
not a per-item plan or a live status source.

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

## 6. Current status authority

Exactly two documents carry live project status:

1. [`plans/gap-registry.md`](plans/gap-registry.md) — owned work, state,
   evidence, dependency, risk, and exit criterion.
2. [`progress.md`](progress.md) — the current verified snapshot and session
   handoff.

Fresh verification commands are evidence and can outrank written status, but a
command result is not a third status document. Record durable status evidence in
the registry or handoff, as appropriate.

The other public documents have narrower jobs: [`README.md`](README.md) is a
navigation hub, [`roadmap.md`](roadmap.md) is the phase map, the root
[`ROADMAP.md`](../ROADMAP.md) is historical, [`CHANGELOG.md`](../CHANGELOG.md)
is chronological evidence, ADRs record decisions, and `history/` preserves
read-only context. None of them is a competing live status source.

## 7. Phase documentation

Each active phase or gate entry in the maintained plan identifies, at minimum:

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
- `scripts/check_docs.ps1` validates GitHub-compatible fragments only for
  relative Markdown targets on the active documentation surface. It does not
  inspect external URLs, `mailto:` links, generated artifacts, or archived
  document sources.

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
- Every session updates the two live status documents and records durable
  changes in [`CHANGELOG.md`](../CHANGELOG.md). The maintained phase map is
  [`roadmap.md`](roadmap.md).

## Review checklist

- [ ] Clear technical English; historical files remain unchanged.
- [ ] Exactly one document kind and an accurate metadata block.
- [ ] Guardrails contain Rule, Why, Evidence, Consequence, and Status.
- [ ] The canonical document was updated instead of creating a duplicate.
- [ ] Historical documents were not edited or deleted.
- [ ] Status claims agree with `progress.md` and `plans/gap-registry.md`.
- [ ] Active phase entries include code, acceptance, command, evidence, and next
      action.
- [ ] Relative links resolve to existing paths.
- [ ] Architecture decisions have an ADR with the correct lifecycle status.
