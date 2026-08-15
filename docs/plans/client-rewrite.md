---
Type: Plan
Status: Current
Audience: Contributors, maintainers, reviewers
Last verified: 2026-08-13
---

# Metin2 Client Rewrite in Rust — Canonical Plan

> **Status: Draft v0.1 (canonical).** Canonical design and migration plan for the Rust client rewrite. Mirrors the structure of [`server-rewrite.md`](server-rewrite.md). The decisions it implements are recorded in [`../decisions/0013-client-rewrite.md`](../decisions/0013-client-rewrite.md) (ADR-0013, **Accepted 2026-08-13; amended 2026-08-13 after oracle review**) — this document sequences the work, it does not decide.
> **Purpose:** a single file with the full client design for third-party review. Feedback: §11.
> **Update 2026-08-13 (oracle review, verified by the orchestrator):** amendments A–G applied — D6 slint license verification, D7 evidence correction (epk unpacker is from scratch), streaming+LOD moved to F1 (B), F4 scope cut (effects/particles/combat polish; perf merged into F5 — C, G3), blind spots F1–F9 (implementation notes), dev-time reductions G1–G6.

## Document map

| Document | Role |
|---|---|
| **This document** (`docs/plans/client-rewrite.md`) | Canonical design and migration plan for the client |
| [`../decisions/0013-client-rewrite.md`](../decisions/0013-client-rewrite.md) | ADR-0013 (Accepted 2026-08-13; amended 2026-08-13 after oracle review): the 14 decisions D1–D14 + minor technical notes |
| [`../decisions/0007-no-partial-rust-in-legacy-client.md`](../decisions/0007-no-partial-rust-in-legacy-client.md) | Boundary this plan inherits: no Rust embedded in the legacy client during F0–F6 |
| [`../decisions/0012-windows-native-runtime-wsl-on-demand.md`](../decisions/0012-windows-native-runtime-wsl-on-demand.md) | Native Windows runtime the client connects to (auth 30001 / channel 30003) |
| [`server-rewrite.md`](server-rewrite.md) | Canonical server plan (F7 "Client (after)"; F5.3 slice order this plan mirrors) |
| [`../reference/protocol/login-flow.md`](../reference/protocol/login-flow.md) | Byte-exact wire spec of the login flow (contract of the `protocol` crate) |
| `../../ROADMAP.md` | Phase tracker (root; kept in sync by the orchestrator) |
| `../../AGENTS.md` | Project rules and verified facts (root) |

---

## Table of contents

1. [Executive summary](#1-executive-summary)
2. [Relationship to the server rewrite](#2-relationship-to-the-server-rewrite)
3. [Context: the legacy client and why rewrite it](#3-context-the-legacy-client-and-why-rewrite-it)
4. [Rewrite principles](#4-rewrite-principles)
5. [Decisions (ADR-0013)](#5-decisions-adr-0013)
6. [Target architecture](#6-target-architecture)
7. [Asset pipeline (asset_tools)](#7-asset-pipeline-asset_tools)
8. [3-layer map architecture](#8-3-layer-map-architecture)
9. [Phases F0–F6](#9-phases-f0f6)
10. [Risks and mitigations](#10-risks-and-mitigations)
11. [Next steps](#11-next-steps)

---

## 1. Executive summary

The server rewrite is largely done: the Rust stack (`source/reforge`) runs natively on Windows and the real client plays against it. The **last legacy component is the C++ client** (v40999, DX9, 20 years old) — it is the protocol contract and the playable reference, but its codebase is a maintenance dead end.

**Proposal:** rewrite **the entire client** in Rust as a **structural redesign** (not a translation): the same observable behavior against the same Rust server, a modern engine (Bevy 0.19), a declarative UI (slint), and a **build-time asset pipeline** that converts the 20-year-old proprietary formats (epk/gr2/DDS) into standard runtime formats (glTF/PNG-KTX2/JSON). The legacy client stays alive as the playable reference until **F5 parity**, then the cutover.

**Motto: do more with less** — same as the server: less code, less complexity, fewer dependencies. The client is a **view**: it sends intentions and renders server truth (the server is authoritative, ADR-0010/0011); it never validates gameplay.

**Incremental, verifiable replacement:** F0 spike first (1–2 days), then per-phase milestones with concrete verification, mirroring the server's phase discipline. The map is built on a **3-layer architecture** (heightmap + placements + glTF models, §8) that needs **no 3D modeling** — the game already has 20 years of assets to convert, and the 3-layer model enables Blender round-trip and AI-assisted map generation.

## 2. Relationship to the server rewrite

- **Same repo, separate workspace (D5):** the client lives in `source/client_rust` as its own Cargo workspace; the `protocol` crate is a **path dependency** (`../reforge/protocol`) — the wire contract is always in sync, the server build/test stays fast (bevy's dependency tree never pollutes it), one repo/backup.
- **F0 is independent of the server:** the slint-in-bevy spike and the asset_tools skeleton run offline; no server connection is required (D12, D4).
- **F1 requires the running Rust server (D4):** `scripts/start_win.ps1` (auth `127.0.0.1:30001`, channel `127.0.0.1:30003`), account `test`/`1234` (AGENTS.md runbook). The F1 milestone is measured **against the real server, not a mock**.
- **The server plan's F7 is this plan:** `server-rewrite.md` §8.2 F7 ("Client (after)", bevy + Slint) is the entry point this plan executes. Server-side F7 items (real encryption, new protocol, deleting `protocol::legacy` 151–153, ADR-0006) are unlocked when the new client lands and are coordinated with the server lane.
- **Frozen asymmetry:** the C++ **server** is frozen (ADR-0012, never rebuilt); the C++ **client** is NOT frozen (D3) — it stays alive and maintainable as the playable reference until F5 parity.
- **Locale:** the client translates (existing architecture, AGENTS.md §17); its locale data is converted to JSON by asset_tools. Server-owned text (ADR-0009) continues to arrive from the server; the two sources do not conflict.

## 3. Context: the legacy client and why rewrite it

- **Legacy state:** C++ client v40999 (S3llMetin2 v24, DX9), compiled from `source\client` (MSBuild Release|Win32). Proprietary pack formats (`root.epk`/`locale.epk`: TEA-ECB + LZO1X, MMPT0/MIPX), CP949 encoding traps, a crash history (0xC0000374 heap corruption from `string_replace_word`, resolved 2026-08-09 — see AGENTS.md), and client-validated gameplay logic that the server rewrite has made obsolete.
- **Why rewrite:** (1) the server-authoritative model (ADR-0010/0011) makes the client a thin view — the legacy one carries 20 years of validation debt that must not be ported; (2) DX9 is end-of-life; (3) the scale vision (LOD, streaming, instancing, KTX2/meshopt — D13/D14) needs a modern render stack; (4) memory safety and a codebase that future agents can maintain.
- **What we do NOT repeat** (same as the server's list, applied to the client): client-side validation of gameplay facts, near-symbolic encryption, CP949 text, pack formats as a runtime dependency, and the 6.6k-LOC god-object style (the legacy client's `PythonApplication`/`CPythonNetworkStream` monoliths).

## 4. Rewrite principles

1. **Do more with less**: YAGNI; stdlib/native before dependencies; one line before fifty (project rule 14).
2. **Structural redesign, not translation**: the decisions are recorded first (ADR-0013); this plan only sequences them.
3. **The client is a view**: it sends intentions, renders server truth; nothing it shows is read back as a fact (server-authoritative, ADR-0010/0011).
4. **Standard formats at runtime only** (D2): proprietary formats die at build time in asset_tools.
5. **Incremental, verifiable**: per-phase milestones with concrete verification (§9), mirroring the server's phase discipline.
6. **Legacy client alive until F5 parity** (D3) as playable reference — do not freeze it.
7. **Windows first** (D9); Linux comes nearly free via bevy/slint/winit.

## 5. Decisions (ADR-0013)

All 14 decisions are recorded in [`../decisions/0013-client-rewrite.md`](../decisions/0013-client-rewrite.md) (Accepted 2026-08-13; amended 2026-08-13 after oracle review). Summary:

| # | Decision | Detail |
|---|---|---|
| **D1** | Engine | **Bevy 0.19** (wgpu 29, GPU-driven rendering, glTF native, multi-window) — not custom wgpu (m2rs-style), not macroquad |
| **D2** | Legacy assets | Build-time conversion; runtime loads ONLY glTF, PNG/KTX2, JSON — no proprietary formats in the shipped client |
| **D3** | Legacy client | Stays alive during the rewrite (coexistence until F5 parity) as playable reference; **do NOT freeze it** |
| **D4** | F1 milestone | Map 41 (Venter) + mobs + minimal UI, against the **real Rust server** (not a mock) |
| **D5** | Organization | `source/client_rust`, **separate Cargo workspace in the same repo**; `protocol` via path dependency (`../reforge/...`) — not same-workspace, not separate repo |
| **D6** | UI | **slint** (declarative, fast prototyping); risk acknowledged (bevy integration less mature); mitigation: F0 spike first; **license terms VERIFIED at the F0 spike** (oracle flagged GPLv3-or-commercial — if the free/logo-credit terms do not hold: egui fallback or commercial license at distribution); egui (MIT/Apache) is the contained fallback (leaf layer) |
| **D7** | Asset pipeline | 100% Rust: epk unpacker (TEA-ECB + LZO1X — only LZO1X is already in `game_core/src/map.rs`; the epk/TEA-ECB part is **from scratch**, days not weeks), gr2→glTF (opengr2-rs + `gltf` crate, NOT GrannyConverterLibrary/Blender; one-off Blender only as escape hatch), DDS→PNG (KTX2/meshopt post-parity, G6), locale `.py`/`.lua`→JSON (reuse `locale_import`, G1) |
| **D8** | Assets location | Inside the repo (`source/client_rust/assets`), converted **per phase** (F1: map 41 + player chars + 2–3 mobs + minimal icons); Git LFS only if it hurts later |
| **D9** | Platform | Windows first (players are on Windows); Linux nearly free via bevy/slint/winit |
| **D10** | Crates | 4 crates: `client` (binary), `client_net`, `client_ui`, `asset_tools` |
| **D11** | F1 content | Map 41 (Venter), chars `lkjsnlfknlsk`/`ninja`, mobs #101 Perro Salvaje + #2101 Zorro del Desierto, minimal UI (chat + HP bar; hotbar optional — G2) |
| **D12** | F0 order | slint-in-bevy spike FIRST (1–2 days), before any real UI work |
| **D13** | gr2 strategy | opengr2-rs as **build-time converter** → glTF (runtime gr2 rejected: skinning/animation translation risk; opengr2-rs 4★ dormant 2023; DDS/epk/map need conversion anyway; glTF enables LOD/meshopt/KTX2; future content native glTF) |
| **D14** | Map | **3 layers** (WoW ADT/MCNK + MDDF/MODF, UE5 World Partition): heightmap + placements JSON + glTF models; streaming 5×5; Blender round-trip; WYSIWYG editor **past F5** (G4); AI-assisted generation (§8) |

> **Amended 2026-08-13 (oracle review):** D6 license verification, D7 evidence correction, D11 hotbar optional (G2), D14 WYSIWYG deferral (G4); phase-level changes (B, C, G3, G6) are in §9.

**Minor technical notes (from ADR-0013):**

- **Network:** tokio dedicated thread + the `protocol` crate; dev connects to `127.0.0.1:30001`/`30003`. `client_net` scope: handshake with clock-bias retries, the phase machine, TimeSync, reconnect using LOGIN_BY_KEY (avoids resending the password), and both LOGIN3 variants (88 B auth with version+hwid vs 65 B channel).
- **Audio:** kira.
- **Input/camera:** winit + gilrs via bevy; third-person legacy-style camera.
- **Locale:** the client translates (existing architecture); data converted to JSON in asset_tools.

## 6. Target architecture

```
source/client_rust/                    (separate Cargo workspace — D5)
├── client/        binary: app shell, bevy plugins, network-thread wiring
├── client_net/    tokio dedicated thread + protocol crate (../reforge/protocol)
├── client_ui/     slint UI (chat, HP bar, hotbar → full HUD) — leaf layer
└── asset_tools/   build-time converters: epk unpacker, gr2→glTF,
                   DDS→PNG, locale .py/.lua→JSON, icon.epk, fonts,
                   EFT, MTR, sound.epk  (D7)
        │  converted per phase (D8)
        ▼
source/client_rust/assets/             glTF, PNG/KTX2, JSON — the ONLY
                                       formats the runtime loads (D2)
```

- **Network:** a dedicated tokio thread owns the connection; the `protocol` crate (path dependency) encodes/decodes the wire. `client_net` scope: handshake with clock-bias retries, the phase machine, TimeSync, reconnect using LOGIN_BY_KEY (avoids resending the password), and both LOGIN3 variants (88 B auth with version+hwid vs 65 B channel). Dev connects to `127.0.0.1:30001` (auth) / `127.0.0.1:30003` (channel) — the same endpoints the legacy client uses on the native Windows stack (ADR-0012).
- **Text encoding:** server-originated text (e.g. `GC_CHAR_ADDITIONAL_INFO` names) arrives as **CP949 bytes** — transcode CP949→UTF-8 at the wire boundary (`encoding_rs` crate) before any UI render (implementation note 5).
- **UI:** slint, a leaf layer (D6): it receives view-state and emits intentions; bevy never depends on it, so a swap to egui after the F0 spike stays contained. **License terms are verified at the F0 spike** (oracle flagged GPLv3-or-commercial; if free/logo-credit terms do not hold → egui (MIT/Apache) fallback or a commercial license at distribution).
- **Input/camera:** winit + gilrs via bevy; third-person legacy-style camera (the Metin2 camera behavior, not a new scheme).
- **Prediction/interpolation:** the server validates movement with a per-entity envelope (correction-not-ban, server plan §5.7) — a naive client rubber-bands. The client owns its half (other-entity interpolation + own-movement prediction) in F2 (implementation note 6).
- **Audio:** kira (SFX/music), integrated from F3; source assets come from `sound.epk` via asset_tools.
- **Locale:** the client translates (existing architecture, AGENTS.md §17); asset_tools converts locale data to JSON at build time (reusing `locale_import`, G1).

## 7. Asset pipeline (asset_tools)

The pipeline is **100% Rust** (D7) and runs at **build time** (D2) — the runtime never sees a proprietary format:

| Legacy input | Converter | Output (runtime format) |
|---|---|---|
| `.epk` packs (TEA-ECB + LZO1X) | epk unpacker — **from scratch** (only LZO1X is already reverse-engineered in `game_core/src/map.rs`; TEA-ECB is a small well-known cipher: days, not weeks — amendment A) | raw files (models, textures, scripts) |
| `gr2` models (Granny) | opengr2-rs + `gltf` crate — build-time only (D13); one-off Blender conversion allowed as escape hatch (implementation note 3) | glTF (skinning/animation translated) |
| `icon.epk` (item/UI icons, TGA/BMP) | icon converter — F1 hotbar needs it (implementation note 4) | PNG |
| Fonts (`.fnt` bitmap) | font converter + CJK glyph coverage check (implementation note 4) | JSON + PNG atlas |
| EFT (particle effects, skills) | EFT converter (implementation note 4) | glTF/JSON particles |
| MTR (materials) | MTR converter (implementation note 4) | JSON materials |
| `sound.epk` | sound unpacker — feeds kira (implementation note 4) | OGG/WAV |
| DDS textures | DDS decoder | **PNG only until parity** — KTX2/meshopt deferred (G6) |
| locale `.py`/`.lua` | locale converter — reuse the existing `locale_import` crate (`../reforge/locale_import`) instead of writing from scratch (G1) | JSON |

- **Prior art first (G1):** evaluate the m2rs/lyketo Rust parsers for the epk/gr2/texture side of asset_tools (license check before reuse); reuse `locale_import` for locale→JSON.
- Converted assets live **inside the repo** (`source/client_rust/assets`) and are converted **per phase** (D8) — F1 converts only map 41 + player chars + 2–3 mobs + minimal icons, not the whole 20-year catalog. Git LFS is activated only if storage hurts later.
- Because gr2 conversion is build-time, an opengr2-rs file-version gap fails the **build**, never the shipped client (D13, risk §10.3); the escape hatch (implementation note 3) keeps the build unblocked.
- **PNG only until parity (G6):** no KTX2/meshopt before F5 — they add value only at the scale stage (F6).

## 8. 3-layer map architecture

The map is **three independent layers** (D14) — the industry-standard decomposition (WoW ADT/MCNK + MDDF/MODF, UE5 World Partition):

| Layer | Format | Content | Notes |
|---|---|---|---|
| **1. Terrain** | Heightmap (PNG/EXR) | Elevation, chunked, LOD | **Generated from the legacy `.map` file** — the server collision grid is 2D attributes only; height is cosmetic/physics (implementation note 1). Streamed 5×5 around the player (F1 deliverable — amendment B); collision from the blocked/water attribute grid (LZO1X, server-attr parsing in `game_core/src/map.rs`) |
| **2. Objects** | JSON placement layer | model ref + x/y/z/rot/scale | **The legacy `.map` file IS the placement data** — parse it directly into placements JSON for map 41; no manual authoring (implementation note 2). Instanced rendering: 1 draw call for many trees |
| **3. Models** | glTF | Meshes, skins, animations | Produced by asset_tools from `gr2` (D7, D13) |

- **Streaming:** terrain chunks load/unload in a 5×5 window around the player, with LOD — **an F1 deliverable** (D14 core design; map 41 is huge — 2.17 M blocked cells), not deferred polish (amendment B).
- **Blender round-trip:** a small addon (~100–150 lines) imports the heightmap (displacement) and imports/exports placements as JSON empties — the user can edit maps without any 3D modeling skill.
- **WYSIWYG in-game editor:** deferred past F5 (G4) — the Blender round-trip + the legacy client as reference cover F3 content needs.
- **AI-assisted map generation:** procedural heightmaps, text-generated placements, Blender MCP for tweaks — the user is not a 3D modeler, and the game already has 20 years of assets; **no 3D modeling is required** (D14 rationale).

## 9. Phases F0–F6

| Phase | Goal | Deliverables | Verification |
|---|---|---|---|
| **F0** Foundations + spike | `client_rust` workspace (4 crates), `protocol` path dependency, tokio network-thread scaffold, asset_tools skeleton; **slint-in-bevy spike FIRST (1–2 days)** before any real UI (D12) | Spike demo (slint panel inside a bevy window); epk unpacker (from scratch — amendment A) + first DDS→PNG + first gr2→glTF | Spike runs on the 4 GB host; one legacy model + one map chunk converted by asset_tools; `cargo test` green in `client_rust`; **slint license terms verified (D6)**; **DX12 backend confirmed on the dev GPU or GL fallback (implementation note 8)**; dev builds use `dynamic_linking` (implementation note 8). **Independent of the server** — runs offline. Re-evaluation point for D6 (slint vs egui) |
| **F1** World entry | Map 41 (Venter) 3-layer rendering **including chunked terrain streaming 5×5 + LOD (amendment B)**, chars `lkjsnlfknlsk`/`ninja`, mobs #101 + #2101, minimal UI (chat + HP bar; hotbar optional — G2), real network (D4, D11) | F1 asset set converted (map 41 + player chars + 2–3 mobs + minimal icons, D8); **terrain heightmap + placements generated from the legacy `.map` file (implementation notes 1–2)**; tokio thread + `protocol` crate connected (handshake, phase machine, TimeSync — E) | Login → select → world → move **against the real Rust server** (`scripts/start_win.ps1`; `127.0.0.1:30001`/`30003`; `test`/`1234`); **move across map 41 with streaming — no load hitches**; mobs visible; chat + HP bar functional; **golden screenshots A/B vs the legacy client** (same account/actions — G5) |
| **F2** Core gameplay | Movement + third-person legacy-style camera (winit+gilrs), combat + skills (server-authoritative), items/inventory, hotbar actions, NPC interaction, shops/trade; **client-side prediction/interpolation (implementation note 6)**; **IME spike for Korean/Chinese chat (implementation note 7)** | Gameplay systems over the F1 shell; prediction/interpolation (own-movement + other-entity); IME spike result | Full session vs the Rust server: kill mobs, loot, equip, shop, 2-player trade — no divergences (mirrors server F5.3 slices); no rubber-banding under the movement envelope |
| **F3** Content + UI depth | Quest log (DSL-driven), character sheet, inventory windows, minimap, settings — all in slint; kira audio; locale JSON active | Full HUD in slint; audio integrated (from `sound.epk` via asset_tools); locale data as JSON | Quests from the DSL corpus render and complete; audio plays; UI screens functional (mirrors server F5.3 quest/social slices) |
| **F4** Effects + combat polish | Effects, particles, combat polish, settings persistence (amendment C) — **multi-window and KTX2/meshopt removed** (YAGNI before parity; multi-window is not in the legacy client — slint popups suffice) | Particle effects (EFT converted), combat-feel polish, settings persistence | Combat feels correct vs the legacy client (golden screenshots G5); no perf work before parity (G3) |
| **F5** Parity | Feature/gameplay parity with the legacy client (D3 end point) **+ remaining performance work** (G3: perf pays only at parity) | Coexistence review: legacy client retired or kept as reference | Scripted A/B: same account/character/actions in both clients → same observable results; golden screenshots (G5) |
| **F6** Cutover + scale | Legacy client retired/frozen after F5 parity + stabilization; scale features (GPU-driven rendering payoff, **KTX2/meshopt post-parity — G6**, larger content; Git LFS if it hurts) | The Rust client is the default client | Default client = `client` binary; legacy binary archived; asset pipeline complete |

> **Amendments 2026-08-13 (oracle review):** streaming+LOD moved F4→F1 (B); F4 scope cut to effects/particles/combat polish with perf merged into F5 (C, G3); hotbar optional in F1 (G2); multi-window/KTX2/meshopt removed pre-parity (C, G6); WYSIWYG editor deferred past F5 (G4); golden screenshots from F1 (G5).

Phase discipline mirrors the server plan: each phase ends with its documentation updated and its `Last verified` refreshed (policy `../DOCUMENTATION.md`).

## 10. Risks and mitigations

| # | Risk | Mitigation |
|---|---|---|
| 1 | **slint×bevy integration is less mature** (D6) | F0 spike FIRST (1–2 days) gates any real UI; re-evaluation point at the spike; egui fallback is contained because UI is a leaf layer (§6) |
| 2 | **slint license terms (oracle flagged GPLv3-or-commercial)** | License VERIFIED at the F0 spike (D6, user decision 2026-08-13: slint retained, free-with-logo-credit understanding); if the free/logo-credit terms do not hold → egui (MIT/Apache) fallback or a commercial license at distribution |
| 3 | **opengr2-rs file-version gaps** (4★, dormant since 2023, D13) | Build-time only: gaps fail the build, never the shipped client; F1 scope is small (2–3 mobs + chars); glTF validation in asset_tools; per-file fallback path if a specific `gr2` does not convert; **one-off Blender conversion committed as glTF is acceptable** (implementation note 3 — D8 keeps outputs in the repo; the Rust tool remains the target, not the blocker) |
| 4 | **GPU-driven rendering maturity (wgpu 29)** | Bevy 0.19 ships it; standard pipeline fallback; Windows-first backend (DX12) per D9 — **confirmed on the dev GPU at the F0 spike or GL fallback** (implementation note 8) |
| 5 | **Bevy version churn** | Pin bevy 0.19; **dedicated bevy-upgrade task budgeted between milestones** (implementation note 9), never mid-phase; `client_net`/`protocol` have no bevy dependency — the wire and the render stack cannot couple |
| 6 | **4 GB host constraint** | Per-phase asset scope (D8), streaming + LOD in F1 (amendment B), release builds; **dev builds use `dynamic_linking`** (implementation note 8); the legacy client remains playable meanwhile (D3) |
| 7 | **20 years of legacy asset scale** | Per-phase conversion only (D8); Git LFS deferred until storage hurts; AI-assisted generation for new content (D14) |
| 8 | **Terrain/placement generation from the legacy `.map`** | The source is identified (implementation notes 1–2): the heightmap is GENERATED from `.map` (the server collision grid is 2D attributes only) and the placements ARE the `.map` data (parsed directly, no manual authoring); generation is F1 work, not discovery |
| 9 | **CP949 from the server** (e.g. `GC_CHAR_ADDITIONAL_INFO` names) | CP949→UTF-8 transcoding at the wire boundary (`encoding_rs` crate) before any UI render (implementation note 5) |
| 10 | **Rubber-banding without prediction** (server envelope = correction-not-ban) | Client owns its half in F2: other-entity interpolation + own-movement prediction (implementation note 6; server plan §5.7 commits its half) |
| 11 | **IME input for Korean/Chinese chat** (winit/slint IME support is spotty) | IME spike in F2 (implementation note 7); scope decision from the spike result |

## 11. Next steps

1. **F0 spike (D12):** slint panel inside a bevy window, 1–2 days — the D6 re-evaluation point **and the slint license verification point (D6)**. This is the first task; it is independent of the server.
2. **Prior-art evaluation (G1):** m2rs/lyketo Rust parsers for the epk/gr2/texture side of asset_tools (license check first); reuse `locale_import` (`../reforge/locale_import`) for locale→JSON instead of writing from scratch.
3. **asset_tools skeleton (D7):** epk unpacker (from scratch — TEA-ECB is small, days not weeks; amendment A), first DDS→PNG, first gr2→glTF (escape hatch documented — implementation note 3).
4. **Workspace (D5/D10):** `source/client_rust` with the 4 crates, `protocol` path dependency, tokio network-thread scaffold.
5. **After spike green:** F1 asset conversion (map 41 + chars + mobs + icons, D8), terrain/placements from the legacy `.map` (implementation notes 1–2), and the F1 world-entry milestone against the running Rust server (D4, D11) with **golden screenshots A/B vs the legacy client (G5)**.
6. **Bevy-upgrade task:** budgeted between milestones, never mid-phase (implementation note 9).
7. **Orchestrator:** `../../ROADMAP.md` entries for the client-rewrite phases (F0–F6); `../../CHANGELOG.md` at session close.
