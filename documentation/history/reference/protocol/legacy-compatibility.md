---
Type: Reference
Status: Current
Audience: Contributors
Last verified: 2026-08-12
---

# Legacy Wire/Pack Compatibility Boundary — PanamaPack (151) and Hybrid-Crypt (152/153)

> **Status: Current** — the boundary is ratified by ADR-0006 (**Accepted** 2026-08-10, implemented in F2a as `protocol::legacy`). The packet facts below are verified against code.
> Purpose: inventory of the legacy-client-only wire constructs that the Rust server must emit while the legacy client (v40999) is the frozen contract (F0–F6), their layouts, why the legacy client requires them, and the deletion list at F7.
> ADR: [../../decisions/0006-legacy-wire-pack-compat-boundary.md](../../decisions/0006-legacy-wire-pack-compat-boundary.md) · Login flow spec: [login-flow.md](login-flow.md) · Canonical plan: [../../plans/server-rewrite.md](../../plans/server-rewrite.md)

## 1. What PanamaPack is (and is NOT)

**PanamaPack is a server→client wire packet** (header `0x97` = 151, fixed 289 bytes). It carries the per-pack initialization vectors (IVs) that the client needs to open pack entries compressed with the "panama" scheme.

It is **NOT**:

- a library or an API;
- an EIX/EPK container format. The pack files remain the standard EIX/EPK format; `COMPRESSED_TYPE_PANAMA`, `COMPRESSED_TYPE_HYBRIDCRYPT` and `COMPRESSED_TYPE_HYBRIDCRYPT_WITHSDB` are **index-entry compression types** inside the EPK index (`EterPack/EterPack.cpp:491,543,554,656,667,771,781,869,962,972`), and packets 151/152/153 are the wire mechanism that distributes the keys/IVs to decrypt those entries.

The name collides with the old "Panama" crypto concept in some forks; in this codebase it is exactly the packet defined in `game/src/packet.h:2097-2102` plus the pack decompression schemes above.

## 2. Packet inventory

| Header | Name | Size | Layout | Sent |
|---|---|---|---|---|
| 151 | `HEADER_GC_PANAMA_PACK` (`TPacketGCPanamaPack`, packet.h:2097-2102) | 289B fixed | `BYTE bHeader; char szPackName[256]; BYTE abIV[32]` | On every successful auth, once per entry of `panama/panama.lst`, before `GC_AUTH_SUCCESS` |
| 152 | `HEADER_GC_HYBRIDCRYPT_KEYS` (`TPacketGCHybridCryptKeys`, packet.h:2104-2147) | 7 + KeyStreamLen (dynamic) | `BYTE bHeader; WORD uDynamicPacketSize; int KeyStreamLen; BYTE pDataKeyStream[KeyStreamLen]` | On successful auth (`SendClientPackageCryptKey`, desc_manager.cpp:520-541); only if the package crypt info has keys |
| 153 | `HEADER_GC_HYBRIDCRYPT_SDB` (`TPacketGCPackageSDB`, packet.h:2149+) | 7 + iStreamLen (dynamic) | `BYTE bHeader; WORD uDynamicPacketSize; int iStreamLen; BYTE m_pDataSDBStream[iStreamLen]` | On successful auth for the default map, and again on every map warp (`SendClientPackageSDBToLoadMap`, desc_manager.cpp:543-563; char.cpp:5207, 7978; input_db.cpp:437) |

"SDB" = Supplementary Data Blocks (hybrid-crypt map data).

## 3. Wire mechanics (verified)

- **151:** the server loads `panama/panama.lst` (pack name + IV file path per line) at boot (`PanamaLoad`, `game/src/panama.cpp:8-58`). Before sending, the 32 IV bytes are XOR-ed DWORD-wise: `ivs[i] ^= desc->GetPanamaKey() + i * 16777619` (`panama.cpp:83-87`). The client registers each IV verbatim (`CEterPackManager::RegisterPack(name, "*", abIV)`, AccountConnector.cpp:225-234) and only at `GC_AUTH_SUCCESS` computes `dwPanamaKey = dwLoginKey ^ g_adwEncryptKey[0..3]` and calls `DecryptPackIV(dwPanamaKey)` (AccountConnector.cpp:305-306).
- **152/153:** dynamic-size packets — the client registers both as `DYNAMIC_SIZE_PACKET` (`PythonNetworkStream.cpp:176-177`) and handles them in every phase (HandShake:56-60, Login:54-58, Select:107-111, Loading:153-157, Game:603-607; auth state: AccountConnector.cpp:236-250). The key stream feeds `RetrieveHybridCryptPackKeys`, the SDB stream feeds the hybrid-crypt map data.

## 4. Why the legacy client requires them

- The EIX/EPK pack may contain entries whose compression type is panama or hybrid-crypt (see §1). Without the IVs (151) or the key stream/SDB (152/153) the client **cannot decompress those entries** — the pack would be unreadable.
- The server sends the sequence on **every successful auth, before `GC_AUTH_SUCCESS`** (`CInputDB::AuthLogin`, `input_db.cpp:1697-1728`), and the client's auth state machine expects the 151 packet in the stream (`AccountConnector.cpp:119-120`).
- A Rust server replacing the auth/channel must emit the same sequence, or the legacy client fails to open its packs and the session breaks. This is part of the frozen wire contract (ADR-0007), not optional behavior.

## 5. Temporary isolation and deletion

- All legacy-client compatibility code lives behind a **`protocol::legacy` module/feature boundary** (proposed, ADR-0006) — PanamaPack 151, hybrid-crypt 152/153 and any other legacy-only packets are implemented there, **never in the new protocol core**.
- The compatibility layer is **deleted when the new client ships (F7)** — nothing legacy survives in the new wire. Deletion list: 151, 152, 153 (and revisit keepalives 0xfc/0xfe, currently handled in `network` — ADR-0006, "Not decided").
- **Never extend** these packets; they are frozen legacy artifacts.

## 6. Pack formats (EIX/EPK)

- The EIX/EPK container formats are part of the frozen client contract during F0–F6: TEA-ECB 32 rounds + LZO1X + MMPT0/MIPX index formats stay as-is (see the pack tooling in `source/tools`).
- **A redesign of EIX/EPK is deferred to the new client (F7)** — the pack stops being a data source of truth then (server→client manifest + delta, plan §5.6); until then only the content, never the formats, may change.

## 7. Deliberate wire divergences (hardened core — ADR-0011)

The Rust core enforces anti-hack controls **always-on** where the legacy C++ shipped them disabled or absent. These are deliberate, documented divergences (ADR-0011 §2/§3): the hardened core needs no client cooperation, and the wire contract stays byte-exact in the translator (`protocol` + `protocol::legacy`).

| # | Divergence | Legacy C++ behavior | Rust behavior | Evidence |
|---|---|---|---|---|
| V1 | Header `0x00` | consumed as a 1-byte no-op (input.cpp:75-76) | **connection close** | framer.rs:44-47; ADR-0011 §2 |
| V2 | Timer speedhack check | **OFF by default** (`gHackCheckEnable=false`, config.cpp:127; input_main.cpp:1308) | **always on** — `iDelta` vs server delta; SlowTimer/FastTimer → kick | movement.rs:94-104; ADR-0011 §2 |
| V3 | Anti-teleport | `ENABLE_TP_SPEED_CHECK` commented out (input_main.cpp:1463-1464) | per-MOVE max distance (2500/6000 units), reject without updating position | movement.rs:106-114; ADR-0011 §2 |
| V4 | Signed clock wrap | `(int)(dwCurTime - dwTime)` cast (u32→i32) — spurious SlowTimer/FastTimer kick beyond ±2³¹ | **modular difference with tolerance**; kick = explicit anti-cheat policy, not a cast artifact (regression test for clock bias ±2³¹) | movement.rs:95-98; ADR-0011 §3 (decided) |
| V5 | DB down | silent hang | deterministic fail-fast at `WorldStore::new` | world.rs:39; ADR-0011 §2 |
| V6 | Idle timeout | no global timeout | per-read, reset on ANY packet incl. keepalives | channel.rs:97-105,119-129; ADR-0011 §2 |

> The wire-debt inventory D1–D6 (1-based slots, `180+wear` cells, `ITEM_ID_RANGE`, `safebox size==1`, `quest lValue==0`, default IP) is the F7 deletion checklist — ADR-0010 §5; mirroring it in this document is pending follow-up.
