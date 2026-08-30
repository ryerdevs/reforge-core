---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Date: 2026-08-30
Last verified: 2026-08-30
Supersedes: —
Superseded by: —
---

# ADR-0017: Regional channels — deferred (one shared World per channel process)

## Status

Accepted (2026-08-30). Closes the "regional channels — own ADR" checkbox that
[ADR-0003](0003-reforge-workspace-rust-layout.md) left open. Decision: do NOT
build regional/multi-world channels now; the single binary with per-role
configs and one shared bevy World stays.

## Context

- ADR-0003 anticipated "regional channels / multi-world — own ADR"; the
  ROADMAP Phase 0 checklist carried the promise without a decision.
- Current architecture: `server_realms` runs one process per role (auth,
  channel); each channel process owns one shared bevy World with dynamic
  spawn materialization (`SPAWN_VIEW`/`DESPAWN_RADIUS`, G0.1c) and per-player
  event routing.
- Measured evidence (wave-44 bench, 2026-08-13): 100 bots × 60 s OK with
  sub-linear latency scaling (world_ms median 2742 ms at 100 bots) and
  AI tick < 1 ms; the 250/500/1000 ladder is pending (F5).
- The client config (`serverinfo.py`) already lists 4 channel ports (30003,
  30007, 30011, 30015) — multiple channel PROCESSES are a config/ops matter,
  not a code matter, today.

## Decision

1. **Defer regional channels** (per-map or per-region World splitting) until
   a measured need exists: sustained >1000 concurrent players on one channel,
   or an isolation requirement (per-region rulesets, maintenance blast radius).
2. **Horizontal scale stays process-level:** more channels = more
   `server_realms --role channel` processes with distinct ports and world
   configs; no shared cross-channel state is planned (the C++ never had it
   either — channels were independent cores).
3. **The data channel (ARQ-C/G2.10) is the only cross-cutting server service**
   planned; it serves content (locale/tables), not world state, so it does not
   reintroduce shared-state coordination.

## Alternatives considered

- **Design regional channels now** — rejected (YAGNI): no measured load demand;
  the ADR-0010 multi_threaded decision and the F5 ladder come first; splitting
  the World without a load driver would freeze a wrong boundary.
- **Per-map World inside one process** — rejected for now: dungeon instances
  (G2.11c) may need scoped Worlds, but that is a dungeon-instance decision,
  tracked there, not a region topology decision.

## Consequences

- The ROADMAP Phase 0 checkbox "ADR: regional channels" is CLOSED by this
  document (defer decision recorded).
- If the F5 ladder shows a single channel saturating before 1000 bots, the
  follow-up decision is documented as a new ADR superseding this one.
- Multi-process channels imply per-process PG access (already true — pool per
  process) and no cross-channel features (whisper across channels stays
  out-of-scope, same as the C++).
