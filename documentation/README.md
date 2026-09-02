---
Type: Hub
Status: Current
Audience: All
Last verified: 2026-09-02
---

# Documentation — reforge-core

This page is the human navigation hub. It describes where to look; it does not
duplicate volatile project status.

## Start here

| Question | Canonical document |
|---|---|
| What is pending, who owns it, and what closes it? | [Gap Registry](plans/gap-registry.md) |
| Where did the last verified session stop? | [Progress handoff](progress.md) |
| Which source wins when documents disagree? | [Document authority](reference/document-authority.md) |
| What do phases F0–F7 mean? | [Phase map](roadmap.md) |
| What is the alpha scope and execution plan? | [Collaborative alpha readiness](plans/alpha-collaborative-readiness.md) and [A0 plan](plans/alpha-a0-truthful-baseline.md) |
| What is the database shape? | [Schema reference](schema.md) |
| What is the supported login wire? | [Login-flow reference](reference/login-flow.md) |
| Which rules prevent repeat failures? | [Rules](rules.md) |
| Why were architecture choices made? | [Architecture decisions](adr/) |
| What changed over time? | [Changelog](../CHANGELOG.md) |
| Where is the immutable archive? | [Historical archive index](history-index.md) |

The [Gap Registry](plans/gap-registry.md) and [progress handoff](progress.md) are
the only live status sources. The root [ROADMAP](../ROADMAP.md) is a historical
compatibility narrative; the [phase map](roadmap.md) is a dated navigation view.

## Quick verification path

Run commands from the repository root:

```powershell
powershell -File scripts/status.ps1
powershell -File scripts/check_docs.ps1
powershell -File scripts/verify.ps1
```

For a Rust-only build and test run:

```powershell
Set-Location source\reforge
cargo build --workspace
cargo test --workspace
Set-Location ..\..
```

Read the [progress handoff](progress.md) first for environmental prerequisites
and the result of the latest gate. A command result is evidence, not a new
status source.

## Repository map

```text
documentation/
  README.md            → this navigation hub
  DOCUMENTATION.md     → mandatory documentation policy
  progress.md          → live snapshot and handoff
  plans/               → live plans and the Gap Registry
  adr/                 → architecture decisions
  reference/           → technical references and runbooks
  roadmap.md           → dated phase map
  schema.md            → PostgreSQL schema reference
  history-index.md     → navigation for the read-only archive
  history/             → archived, read-only documents
source/reforge/        → authored Rust server
scripts/               → verification and runtime operations
```
