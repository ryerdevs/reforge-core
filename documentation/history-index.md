---
Type: Hub
Status: Current
Audience: All
Last verified: 2026-09-02
---

# Historical archive index

This is the current navigation page for the read-only archive. Files under
[`history/`](history/) preserve earlier plans, references, guardrails, and
snapshots; they are not sources of current project status.

Archived metadata, names, paths, statuses, and links describe the date on which
each record was written and may be stale. Do not repair those records in place.
For present status, use only the [Gap Registry](plans/gap-registry.md) and
[progress handoff](progress.md), as defined by the [document authority](reference/document-authority.md).

[`history/README.md`](history/README.md) is an immutable pre-migration archive
snapshot. It retains its original links and metadata. Use this page for current
archive navigation instead of editing or following that old index.

The explicit G1.14b decision is to keep that snapshot byte-for-byte immutable,
including its pre-migration metadata. This exception applies only to that
historical snapshot; new and edited documents still follow
[`DOCUMENTATION.md`](DOCUMENTATION.md).

## Canonical navigation outside the archive

- [Documentation hub](README.md)
- [Documentation policy](DOCUMENTATION.md)
- [Live handoff](progress.md)
- [Gap Registry](plans/gap-registry.md)
- [Document authority](reference/document-authority.md)
- [Phase map](roadmap.md) — dated navigation, not live status
- [Architecture decisions](adr/)
- [Historical root roadmap](../ROADMAP.md)
- [Changelog](../CHANGELOG.md) — chronological evidence, not live status

## Dated historical records

These records are indexed here because they were omitted from the original
archive navigation. Their original metadata and paths remain historical.

- [2026-08-06 agent workflow plan](history/2026-08-06-agent-workflow-plan.md) —
  current contributor rules are in [AGENTS.md](../AGENTS.md).
- [2026-08-06 client-assets design](history/2026-08-06-client-assets-design.md) —
  the public boundary is defined by [ADR-0015](adr/0015-rust-only-public-repository.md).
- [2026-08-06 client-assets implementation](history/2026-08-06-client-assets-implementation.md) —
  client material remains outside this repository under [ADR-0015](adr/0015-rust-only-public-repository.md).
- [2026-08-06 Docker Compose baseline](history/2026-08-06-docker-compose-baseline.md) —
  consult the [documentation hub](README.md) and [document authority](reference/document-authority.md)
  for the maintained repository boundary.
- [2026-08-06 project-skills addendum](history/2026-08-06-project-skills-addendum.md) —
  maintained contributor instructions are in [AGENTS.md](../AGENTS.md).
- [2026-08-06 project-skills design](history/2026-08-06-project-skills-design.md) —
  maintained contributor instructions are in [AGENTS.md](../AGENTS.md).
- [2026-08-06 project-skills revision](history/2026-08-06-project-skills-revision.md) —
  maintained contributor instructions are in [AGENTS.md](../AGENTS.md).
- [2026-08-09 server-rewrite draft](history/2026-08-09-server-rewrite-draft.md) —
  superseded by the [historical server-rewrite plan](history/plans/server-rewrite.md);
  use the [phase map](roadmap.md) and the two live status sources for present guidance.
- [2026-08-09 server-rewrite plan v0.2](history/2026-08-09-server-rewrite-plan-v0.2.md) —
  superseded by the [historical server-rewrite plan](history/plans/server-rewrite.md);
  use the [phase map](roadmap.md) and the two live status sources for present guidance.

## Historical policy and status records

- [Historical documentation policy](history/DOCUMENTATION.md) — superseded by
  [DOCUMENTATION.md](DOCUMENTATION.md).
- [Historical status snapshot](history/CURRENT.md) — frozen at its recorded
  date; present status is in the [progress handoff](progress.md) and [Gap Registry](plans/gap-registry.md).

## Historical plans

- [Consolidated master plan](history/plans/master-plan.md)
- [Server rewrite plan](history/plans/server-rewrite.md)
- [Client rewrite plan](history/plans/client-rewrite.md) — superseded by
  [ADR-0015](adr/0015-rust-only-public-repository.md)
- [Server-side gap analysis](history/plans/server-side-gap-2026-08-15.md)
- [Legacy mob behavior analysis](history/plans/mob-legacy-behavior.md)
- [Locale redesign](history/plans/locale-redesign.md)

## Historical references

- [Legacy wire and pack compatibility](history/reference/protocol/legacy-compatibility.md)
- [Quest DSL specification](history/reference/quests/quest-dsl.md)
- [Legacy language system](history/reference/legacy/language-system.md)
- [Legacy schema inventory](history/reference/database/legacy-schema.md)
- [Legacy SQL compatibility inventory](history/reference/database/legacy-sql-compatibility.md)

## Historical guardrails and snapshots

- [Guardrail index](history/guardrails/README.md)
- [Operations guardrail](history/guardrails/operations.md)
- [World-entry crash postmortem](history/guardrails/world-entry-crash.md)
- [Historical status snapshot](history/CURRENT.md)
- [Base-playable assumptions snapshot](../ASSUMPTIONS.md)
- [Agent organization explanation](history/explanation/agent-organization.md)
