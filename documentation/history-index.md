---
Type: Hub
Status: Current
Audience: All
Last verified: 2026-08-30
---

# Historical archive index

This is the current navigation page for the read-only archive. Files under
[`history/`](history/) preserve earlier plans, references, guardrails, and
snapshots; they are not sources of current project status.

[`history/README.md`](history/README.md) is an immutable pre-migration archive
snapshot. It retains its original links and metadata. Use this page for current
archive navigation instead of editing or following that old index.

The explicit G1.14b decision is to keep that snapshot byte-for-byte immutable,
including its pre-migration metadata. This exception applies only to that
historical snapshot; new and edited documents still follow
[`DOCUMENTATION.md`](DOCUMENTATION.md).

## Current sources of truth

- [Documentation hub](README.md)
- [Documentation policy](DOCUMENTATION.md)
- [Live handoff](progress.md)
- [Live roadmap](roadmap.md)
- [Gap Registry](plans/gap-registry.md)
- [Architecture decisions](adr/)
- [Master roadmap](../ROADMAP.md)
- [Changelog](../CHANGELOG.md)

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
