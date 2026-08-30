---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Date: 2026-08-30
Last verified: 2026-08-30
Supersedes: —
Superseded by: —
---

# ADR-0016: Quest system — native DSL plus converter and a runtime engine

## Status

Accepted (2026-08-30). Records the architecture that already shipped in
2026-08-13 (crate `quest_dsl`) and 2026-08-27–28 (runtime engine
`game_core/src/quest/engine.rs`), closing the "ADR: quest engine" debt that
[ADR-0003](0003-reforge-workspace-rust-layout.md) left open (policy: ADRs
before architecture code).

## Context

- The legacy server runs quests as Lua 5.0 scripts interpreted by a modified
  EUC-KR lexer (`liblua/5.0`); the .qc files compile to Lua at build time.
  Embedding a Lua VM in Rust was rejected early (YAGNI, dependency surface,
  the server lexer is already a divergence trap — the CP949 locale bug).
- The corpus is ~194 real quest files with a repetitive structure (collect,
  kill, deliver, main/sub quest families).
- The server owns quest STATE (`player.quest` table; `__status` 1-based —
  parity `questpc.cpp:115-118`) and decides all conditions server-side
  (server-authoritative invariant, ADR-0011).

## Decision

1. **Native DSL crate (`quest_dsl`)** — typed AST/parser/renderer for quest
   logic, with a typed catalog per spec §3–§5 (actions, triggers, conditions)
   and family templates (main, sub, collect, herb, new, flame) that expand.
   §11 resolved: `between` native, `if` 1-level + else, `select` as-capture,
   `@key` refs, `.quest` files, timer alias.
2. **qc→DSL converter** — `quest_dsl/src/convert/` parses the real qc grammar
   (Lua 5.0 dialect with begin/end, multi-line when heads, inline ifs,
   while/repeat) and maps 22 actions / 10 triggers / 10 conditions. Corpus
   conversion: 194/194 files, 0 failed; ~5,513 unmapped items are
   Rust-module territory (per spec §8), tracked in the registry.
3. **Runtime engine (`game_core/src/quest/engine.rs`)** — interprets the DSL:
   state machine `{quest}.__status`, `wait()`/`select()` suspension with
   re-entry via `CG_SCRIPT_ANSWER` (29 B), conditions + actions subset,
   `GC_SCRIPT` (45 B) rendering, `player.quest` persistence (value 0 = DELETE),
   background load at boot.
4. **No Lua VM.** Quest text/keys go through the locale/data channel
   ([ADR-0009](0009-server-side-locale.md), G2.10); quest rewards/actions are
   server-side typed actions, not sandboxed scripts.

## Alternatives considered

- **Embed Lua 5.0 (parity purist)** — rejected: drags the modified EUC-KR
  lexer, a C VM, and 2000-era API design into the Rust server; the corpus's
  repetitive structure does not need a full scripting language.
- **Hand-code every quest in Rust** — rejected: 194 files of content would
  swamp the core; the DSL keeps content as data, reviewable and diffable.
- **Keep qc + a thin Rust interpreter** — rejected: the qc grammar is the
  worst part (Korean-keyed, lua-dialect); normalizing to a typed DSL once is
  cheaper than living with the grammar forever.

## Consequences

- **Shipped:** `quest_dsl` (44+ tests), converter (corpus 194/194), engine
  (states, suspension, persistence, wire) — verified in the registry and the
  handoff (2026-08-27–28 slices).
- **Open rows:** G2.5 (`input_number`), G2.10 (quest text delivery via the
  data channel), G3.1c (stale coverage table — fixed 2026-08-30), and the
  family-proposal expansion (~112/194 files) when content work resumes.
- **Additive path:** new quest actions land as typed catalog entries + engine
  handlers; the DSL/catalog is the only place that grows for content.
- **Quest text keys** (`gameforge.*`) render raw until G2.10 serves locale
  chunks for them (locale push already exists).
