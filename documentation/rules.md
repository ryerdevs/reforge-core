---
Type: Guardrail
Status: Current
Audience: Contributors, maintainers, operators
Last verified: 2026-09-02
---

# Guardrails — rules not to repeat

The authoritative project instructions remain in [`AGENTS.md`](../AGENTS.md).
This page keeps the short operational rules discoverable and links to their
evidence instead of duplicating the full history.

## 1. Keep the C++ server frozen

**Rule:** Do not modify or rebuild `source/server`; use it only as the local
parity oracle.

**Why:** The Linux binaries are the frozen reference, and rebuilding or mixing
copies can invalidate parity evidence.

**Evidence:** [ADR-0012](adr/0012-windows-native-runtime-wsl-on-demand.md) and
[`AGENTS.md`](../AGENTS.md).

**Consequence:** A violation makes the oracle and its evidence untrustworthy;
the change must be discarded or explicitly reconciled before parity work.

**Status:** Active

## 2. Preserve CP949 in server locale Lua

**Rule:** Keep Korean text in server locale Lua files encoded as CP949/EUC-KR,
not UTF-8.

**Why:** The legacy Lua 5.0 lexer consumes two bytes for a high-bit character;
UTF-8 Korean text misaligns the lexer and can invalidate the file.

**Evidence:** [`AGENTS.md`](../AGENTS.md) and the historical
[encoding guardrail](history/guardrails/data-and-encoding.md).

**Consequence:** The locale file can fail to load, leaving quest or monster
text undefined and producing runtime script errors.

**Status:** Active

## 3. Keep original `item_proto` names

**Rule:** Do not translate the server's `item_proto.name`; preserve the original
CP949 value used by drop-table lookup.

**Why:** Legacy drop files resolve items by their original name during boot,
while the external client's pack supplies visible item names.

**Evidence:** [`AGENTS.md`](../AGENTS.md) and the historical
[legacy-compatibility guardrail](history/guardrails/legacy-compatibility.md).

**Consequence:** Boot can fail with `No such an item`, and item drops may not
load.

**Status:** Active

## 4. Write durable mutations through WAL and the batcher

**Rule:** Route world mutations through `WAL → Batcher → PostgreSQL`; do not
await SQL inline in the world task.

**Why:** Durable-first writes, one transaction, and idempotent replay protect
items, gold, and other state across crashes.

**Evidence:** [ADR-0008](adr/0008-data-layer.md) and the current
[schema reference](schema.md).

**Consequence:** Inline or non-idempotent writes can lose state or duplicate a
mutation during replay.

**Status:** Active

## 5. Isolate test database operations

**Rule:** Use the test database and temporary prefixes defined by the test
runbook; never point disposable tests at the production `metin2` database or
port 5432.

**Why:** Integration tests create and remove temporary data and must not damage
the native development database.

**Evidence:** [`AGENTS.md`](../AGENTS.md),
[the operations history](history/guardrails/operations.md), and
[`scripts/verify.ps1`](../scripts/verify.ps1).

**Consequence:** A test can destroy live data or make the local runtime
unusable; stop and restore the affected database before continuing.

**Status:** Active

## 6. Close every session with current documentation

**Rule:** Update [`progress.md`](progress.md), [`plans/gap-registry.md`](plans/gap-registry.md),
and [`CHANGELOG.md`](../CHANGELOG.md) when verified project knowledge changes.

**Why:** The handoff, plan, and evidence record must agree so the next session
does not repeat closed work or treat an unverified item as complete.

**Evidence:** [`AGENTS.md`](../AGENTS.md) rules 8, 9, and 19, and the
[documentation policy](DOCUMENTATION.md).

**Consequence:** The change is incomplete and its status must remain open until
the canonical documents are reconciled.

**Status:** Active

## 7. Keep client material outside this repository

**Rule:** Do not add client source, pack source, generated client artifacts, or
an F7 client workspace to this repository.

**Why:** This checkout distributes the authored Rust server; real-client checks
use an independently supplied compatible client.

**Evidence:** [ADR-0015](adr/0015-rust-only-public-repository.md) and the
[repository boundary](../README.md#repository-boundary).

**Consequence:** The public boundary and ordinary server verification become
ambiguous; remove the material and record the reconciliation.

**Status:** Active

## 8. Coordinate OpenCode 2 subagents through their lifecycle

**Rule:** Start one bounded child lane at a time through the OpenCode subagent
mechanism. Do not resume, re-prompt, or duplicate a child while its scheduler
state is live or unconfirmed. Interrupt a background child only through
`POST /api/session/<session-id>/interrupt`, then verify the active-session list
and terminal session outcome.

**Why:** Agent instructions are context, not a session-control mechanism.
Treating an uncertain child as completed or creating a replacement can duplicate
work, lose a cancellation, and leave the project state unclear.

**Evidence:** [`AGENTS.md`](../AGENTS.md#opencode-2-subagent-lifecycle), the
OpenCode V2 [agents](https://opencode.ai/v2/docs/agents/) and
[API](https://opencode.ai/v2/docs/api/) references, and the OpenCode service
health check verified on 2026-09-02.

**Consequence:** Stop the affected lane, inspect the OpenCode service and
session state, reconcile the result, and only then schedule a distinct next
task. Do not work around the uncertainty with another child.

**Status:** Active
