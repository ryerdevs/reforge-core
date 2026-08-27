# Progress — Metin2 Reforge

## Current

- Fecha: 2026-08-27
- HEAD: `59b6be9` (63rd part messenger/emotions, 733 tests)
- Árbol sucio: `gm.rs`/`social.rs` (+137/+34)
- Preset OmO `muse-spark-1.2-contributor` activo
- Plan audit: 18 todos en `.omo` (SHA `5649F62B`, APPROVE local, pendiente `$start-work` o archivado)
- `docs/CURRENT.md` stale (50th `9a0b618`)

## Handoff

- 2026-08-27 15:54 | HEAD 59b6be9 |  M AGENTS.md;  D docs/CURRENT.md;  D docs/DOCUMENTATION.md;  D docs/README.md;  D docs/decisions/0001-postgresql-without-timescaledb-by-default.md;  D docs/decisions/0002-unify-game-and-db.md;  D docs/decisions/0003-reforge-workspace-rust-layout.md;  D docs/decisions/0004-reforge-structure-and-names.md;  D docs/decisions/0005-postgresql-cutover-and-legacy-adapter.md;  D docs/decisions/0006-legacy-wire-pack-compat-boundary.md;  D docs/decisions/0007-no-partial-rust-in-legacy-client.md;  D docs/decisions/0008-data-layer.md;  D docs/decisions/0009-server-side-locale.md;  D docs/decisions/0010-domain-boundaries-and-data-ownership.md;  D docs/decisions/0011-anti-hack-model.md;  D docs/decisions/0012-windows-native-runtime-wsl-on-demand.md;  D docs/decisions/0013-client-rewrite.md;  D docs/explanation/agent-organization.md;  D docs/guardrails/README.md;  D docs/guardrails/agent-operations.md;  D docs/guardrails/data-and-encoding.md;  D docs/guardrails/legacy-compatibility.md;  D docs/guardrails/operations.md;  D docs/guardrails/rust-rewrite.md;  D docs/guardrails/world-entry-crash.md;  D docs/history/2026-08-06-agent-workflow-plan.md;  D docs/history/2026-08-06-client-assets-design.md;  D docs/history/2026-08-06-client-assets-implementation.md;  D docs/history/2026-08-06-docker-compose-baseline.md;  D docs/history/2026-08-06-project-skills-addendum.md;  D docs/history/2026-08-06-project-skills-design.md;  D docs/history/2026-08-06-project-skills-revision.md;  D docs/history/2026-08-09-server-rewrite-draft.md;  D docs/history/2026-08-09-server-rewrite-plan-v0.2.md;  D docs/history/README.md;  D docs/plans/bug-registry-2026-08-15.md;  D docs/plans/client-rewrite.md;  D docs/plans/gap-analysis-2026-08-15.md;  D docs/plans/locale-redesign.md;  D docs/plans/master-plan.md;  D docs/plans/mob-legacy-behavior.md;  D docs/plans/next-block-plan-2026-08-15.md;  D docs/plans/server-rewrite.md;  D docs/plans/server-side-gap-2026-08-15.md;  D docs/reference/README.md;  D docs/reference/database/legacy-schema.md;  D docs/reference/database/legacy-sql-compatibility.md;  D docs/reference/legacy/language-system.md;  D docs/reference/protocol/legacy-compatibility.md;  D docs/reference/protocol/login-flow.md;  D docs/reference/quests/quest-dsl.md;  M source/reforge/game_core/src/gm.rs;  M source/reforge/protocol/src/social.rs; ?? .github/; ?? documentation/; ?? scripts/handoff.ps1; ?? scripts/status.ps1; ?? scripts/verify.ps1; ?? source/reforge/game_core/tests/synthetic_verifier.rs
- fix handoff path -> documentation/progress.md

## Next

1. `docs/GUIDE.md` + `docs/plans/audit-2026-08-27.md`
2. Decidir si archivar el audit y volver a slices
3. T1 baseline si se ejecuta el audit

Last update: 2026-08-27 15:54 - handoff.ps1