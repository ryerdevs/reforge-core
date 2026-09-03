---
Type: Hub
Status: Current
Audience: All
Last verified: 2026-09-02
---

# reforge-core

`reforge-core` is a server written from scratch in Rust. It reconstructs a
byte-oriented compatibility boundary from observed behavior and keeps the
implementation incremental, testable, and server-authoritative.

This repository is an **alpha contributor preview**, not a claim of complete
gameplay parity, production readiness, or a finished client. The [Gap
Registry](documentation/plans/gap-registry.md) and [progress
handoff](documentation/progress.md) are the only live status sources. The [document authority](documentation/README.md#document-authority-and-precedence) explains
how evidence and summaries relate to them.

## Repository boundary

The public checkout contains the authored Rust server, documentation, scripts,
and supporting metadata. It does not contain client source, pack source,
runtime client assets, generated client artifacts, or the frozen C++ oracle.
Real-client checks use an external, operator-provided compatible client. The
standalone Rust client is deferred outside this repository; see
[ADR-0015](documentation/adr/0015-rust-only-public-repository.md).

## Supported first path

The documented compatibility path is authentication → channel login →
character selection → world entry. Its byte-level contract is in the [login-flow
reference](documentation/reference/login-flow.md). For a local server or test
run, read the [progress handoff](documentation/progress.md) first, then run
from the repository root:

```bash
python scripts/check_docs.py
python scripts/verify.py
```

For a Rust-only build and test run:

```powershell
Set-Location source\reforge
cargo build --workspace
cargo test --workspace
Set-Location ..\..
```

The compatible client and its assets are operator prerequisites for end-to-end
checks; they are not build inputs for this repository.

## Known alpha boundaries

- The effective gold cap is `2_000_000_000`; the G0.1e cap row is closed under
  focused and property evidence. Wallet non-negative constraints are described
  in the [schema reference](documentation/schema.md). See the [Gap Registry's
  cap section](documentation/plans/gap-registry.md#p0--user-priorities-caps-and-storage)
  for status evidence.
- The native quest DSL and runtime are accepted by [ADR-0016](documentation/adr/0016-quest-system-dsl-and-runtime.md).
  Target/affect, timer, reward, letter, and quest-state actions are implemented;
  `clear_letter`, `say_item_vnum`, `notice_multiline`, and `input_number` remain
  open in G2.5. See the [gameplay gap section](documentation/plans/gap-registry.md#p2--gameplay-and-content-gaps).
- Locale bootstrap is authentication/loading-only. Auth answers
  `CG_LOCALE_REQUEST`; the channel must not send or answer `GC_LOCALE` (header
  140) after `GC_PHASE(GAME)`. The versioned manifest, delta delivery, and
  notification-driven reload remain open under G2.10; see [ADR-0009](documentation/adr/0009-server-side-locale.md)
  and the [Gap Registry](documentation/plans/gap-registry.md#p2--gameplay-and-content-gaps).
- Broader gameplay, scale, real-client, and operational gates remain tracked
  in the live sources rather than summarized here.

## Architecture and contribution

The Rust workspace and its boundaries are described in the [documentation
hub](documentation/README.md), [phase map](documentation/roadmap.md), and
[architecture decisions](documentation/adr/). Read [AGENTS.md](AGENTS.md)
before contributing; keep changes focused and attach an evidence path to each
claim.

The [changelog](CHANGELOG.md) records chronological evidence. The maintained
[phase map](documentation/roadmap.md) records milestones, while the live pair
records current state.
