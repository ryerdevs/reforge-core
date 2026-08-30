---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Date: 2026-08-10
Last verified: 2026-08-30
Supersedes: —
Superseded by: —
---

# ADR-0006: Legacy wire/pack compatibility boundary

## Context

The legacy client (v40999) is frozen as the wire contract during F0–F6 (ROADMAP principle 6, ADR-0007). Its wire carries legacy constructs that the new protocol core must not absorb:

- **PanamaPack (header 151, 289B)** — deferred from F0 to F2 (CHANGELOG 2026-08-10, 1st part).
- **Hybrid-crypt (headers 152/153)** — deferred from F0 to F2.
- Framing edge cases already documented in `network` (e.g. 0x00: the framer closes, the C++ consumes it as no-op — deliberate divergence).

If these are implemented inline in `protocol`, the new wire core accumulates legacy quirks that the new client (F7) will have to strip out again — a hunt instead of a deletion.

## Decision (accepted)

1. All legacy-client compatibility code lives behind a **`protocol::legacy` module/feature boundary**: PanamaPack 151/289B, hybrid-crypt 152/153 and any other legacy-only packets are implemented there, never in the new protocol core.
2. The boundary is documented in the [historical compatibility reference](../history/reference/protocol/legacy-compatibility.md) (packet inventory, headers, and deletion list).
3. The compatibility layer is **deleted when the legacy client is retired**.
   **Amended 2026-08-30:** the retirement trigger is **F6** (server parity
   complete, side-by-side done — [ROADMAP Phase 6](../../ROADMAP.md)), not the
   F7 client: [ADR-0015](0015-rust-only-public-repository.md) deferred F7
   outside this repository, which left the original "when the new client
   ships" trigger orphaned. ADR-0015's "separate server decision" IS this
   amendment.
4. **Implemented 2026-08-10 (F2a):** `protocol/src/legacy.rs` — PanamaPack 151/289B (XOR IV parity, `panama.cpp:70-93`), hybrid-crypt 152/153 (keys + SDB streams, `ClientPackageCryptInfo.cpp`), runtime-file conditional (`panama/` + `cshybridcrypt*` loading — no files → no packets, parity with the C++ auth); used by `server_realms --role auth` before `GC_AUTH_SUCCESS` (`input_db.cpp:1710-1716`).

## Alternatives considered

### Implement PanamaPack inline in the protocol core

Rejected: pollutes the new wire with legacy quirks; F7 removal becomes a hunt instead of a deletion.

### Drop legacy wire support at F2

Rejected: the legacy client is the frozen contract until F7; dropping it breaks the verified login flow.

### Keep legacy wire forever (dual-wire server)

Rejected: F7's protocol is the only target; dual-wire doubles the attack surface and the test surface.

## Consequences

- Clear seam between the new wire and legacy compatibility; `protocol` stays zero-deps and byte-exact.
- F2 work on PanamaPack/hybrid-crypt is scoped to `protocol::legacy` (ADR-0004's deferred module split happens here).
- The F7 deletion list is explicit (this ADR + the reference doc).

## Not decided in this ADR

- The concrete PanamaPack/hybrid-crypt packet layouts (reference doc + F2 implementation).
- Whether legacy keepalives (0xfc/0xfe) also move under `protocol::legacy` (currently handled in `network`; revisit at F2).
