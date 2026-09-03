---
Type: Hub
Status: Current
Audience: All
Last verified: 2026-09-03
---

# Documentation — reforge-core

Central navigation hub, document authority precedence, and documentation guidelines.

## Quick Index

| Topic / Question | Canonical Source |
|---|---|
| What is pending, who owns it, and what closes it? | [Gap Registry](plans/gap-registry.md) |
| Where did the last verified session stop? | [Progress handoff](progress.md) |
| What do phases F0–F7 mean? | [Phase map](roadmap.md) |
| What is the alpha scope and execution plan? | [Collaborative alpha readiness](plans/alpha-collaborative-readiness.md) |
| What is the database shape? | [Schema reference](schema.md) |
| What is the supported login wire? | [Login-flow reference](reference/login-flow.md) |
| What public tools and external prerequisites apply? | [Public tools and external prerequisites](reference/public-tooling-boundary.md) |
| How do backup and restore operations work? | [Backup & restore runbook](reference/backup-restore.md) |
| Which rules prevent repeat failures? | [Rules](rules.md) |
| Why were architecture choices made? | [Architecture decisions](adr/) |
| What changed over time? | [Changelog](../CHANGELOG.md) |
| Where is the read-only historical archive? | [Progress history index](progress.md#historical-archive-index) |

## Document Authority and Precedence

Live project state lives in exactly two files:
1. [`plans/gap-registry.md`](plans/gap-registry.md) — owned work, state, evidence, dependency, risk, exit criteria.
2. [`progress.md`](progress.md) — current verified snapshot, session handoff, and history index.

When documents disagree, precedence is:
1. **Fresh verification:** A command run today against HEAD (`cargo test`, `python scripts/verify.py`, `manage.py db check`). Recorded command evidence beats any narrative.
2. **Gap Registry:** [`plans/gap-registry.md`](plans/gap-registry.md) per-item tracker.
3. **Progress Handoff:** [`progress.md`](progress.md) dated entries.
4. **Accepted ADRs:** [`adr/`](adr/) architectural decisions.
5. **Changelog & Phase Map:** [`../CHANGELOG.md`](../CHANGELOG.md) and [`roadmap.md`](roadmap.md).
6. **Archive:** [`history/`](history/) (read-only context; nothing there is current status).

## Documentation Policy

- Every live document carries a YAML frontmatter block (`Type`, `Status`, `Audience`, `Last verified`).
- Maintained in clear technical English, UTF-8.
- Proportional verification: never claim completion without reproducible command evidence.
- When code in `source/reforge` is touched, CI enforces updating `progress.md`, `plans/gap-registry.md`, and `../CHANGELOG.md`.

## Standard Verification Path

```bash
# Standard test suite
cargo test --workspace

# Database health check
python scripts/manage.py db check

# Public boundary & documentation checks
python scripts/check_boundary.py
python scripts/check_docs.py
python scripts/verify.py
```
