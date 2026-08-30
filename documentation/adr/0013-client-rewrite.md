---
Type: Decision
Status: Superseded
Audience: Contributors, maintainers
Date: 2026-08-13
Last verified: 2026-08-30
Supersedes: —
Superseded by: ADR-0015
---

# ADR-0013: Client rewrite in Rust — Bevy 0.19, slint UI, build-time legacy asset conversion

## Status

**Superseded on 2026-08-30 by [ADR-0015](0015-rust-only-public-repository.md).**
The decision record is retained for historical context. Its proposed client
workspace, client build pipeline, and pack-conversion pipeline are not active
parts of this server repository; F7 is deferred outside it. The former
sequenced plan is preserved at
[`../history/plans/client-rewrite.md`](../history/plans/client-rewrite.md).

## Context

- The server rewrite (`source/reforge`) is largely done: workspace green, live on native Windows (ADR-0012: auth `127.0.0.1:30001`, channel `127.0.0.1:30003`, PostgreSQL 18). The last legacy component is the C++ client (v40999, DX9, a 20-year-old codebase).
- The legacy client is the playable reference and the byte-exact protocol contract. ADR-0007 forbade embedding Rust inside it during F0–F6; the server plan's F7 ("Client (after)", bevy + Slint — decided 2026-08-12) is now being entered.
- Twenty years of legacy assets exist: `.epk` packs (TEA-ECB + LZO1X), `gr2` models, DDS textures, locale `.py`/`.lua`. Some formats are already reverse-engineered in the Rust codebase (`game_core/src/map.rs`, LZO1X server-attr parsing).
- The user is not a 3D modeler; the game already has the assets. The host is Windows 10 22H2, 4 GB RAM; the players are on Windows.
- 2026 engine landscape: Bevy 0.19 (wgpu 29, GPU-driven rendering, glTF native, multi-window), macroquad, and custom-wgpu projects (m2rs-style).

## Decision

The client is rewritten in Rust as a **structural redesign** (not a translation): same observable behavior against the same Rust server, modern engine, standard runtime formats. **All 14 decisions (D1–D14):**

- **D1 — Engine: Bevy 0.19** (wgpu 29, GPU-driven rendering, glTF native, multi-window). Chosen over custom wgpu (m2rs-style) and macroquad.
- **D2 — Legacy assets = build-time conversion.** The runtime loads ONLY standard formats (glTF, PNG/KTX2, JSON). No proprietary formats in the shipped client.
- **D3 — Legacy C++ client stays alive during the rewrite** (coexistence until F5 parity) as playable reference. Do NOT freeze it.
- **D4 — F1 milestone = map 41 (Venter) + mobs + minimal UI**, against the real Rust server (not a mock).
- **D5 — Organization: `source/client_rust`, a SEPARATE Cargo workspace inside the same repo**, with the `protocol` crate via path dependency (`../reforge/...`). Rationale: the server build/test stays fast (bevy's huge dependency tree does not pollute the server workspace), the protocol is always in sync, one repo/backup. NOT same-workspace, NOT separate repo.
- **D6 — UI: slint** (declarative, fast prototyping, prettier than the egui default). Risk acknowledged: bevy integration is less mature. Mitigation: F0 spike (slint panel inside a bevy window) BEFORE any real UI; replacement (egui) is contained because UI is a leaf layer. Research note: egui is the proven MMO-HUD pattern; slint is chosen by user preference, with a re-evaluation point at the F0 spike. **License (user decision 2026-08-13): slint RETAINED** — user's understanding: free usage with logo credit in the credits + some restrictions. The exact terms are VERIFIED at the F0 spike (the oracle flagged GPLv3-or-commercial; if the free/logo-credit terms do not hold, switch to the egui fallback or a commercial license at distribution). egui (MIT/Apache) remains the documented fallback.
- **D7 — asset_tools pipeline = 100% Rust:** epk unpacker (TEA-ECB + LZO1X — LZO1X is already reverse-engineered in `game_core/src/map.rs`; the epk/TEA-ECB/MMPT/MIPX unpacker is a **from-scratch effort** — no epk unpacking exists in `source/reforge`; TEA-ECB is a small well-known cipher, estimate days, not weeks), gr2→glTF via opengr2-rs + the `gltf` crate (NOT GrannyConverterLibrary/Blender; a one-off Blender conversion is acceptable only as an escape hatch — implementation note 3), textures DDS→PNG/KTX2 (KTX2/meshopt deferred until after parity — amendment G6), locale `.py`/`.lua`→JSON.
- **D8 — Converted assets live INSIDE the repo** (`source/client_rust/assets`), converted PER PHASE (F1: map 41 + player chars + 2–3 mobs + minimal icons), not all at once. Git LFS only if it hurts later.
- **D9 — Platform: Windows first** (players are on Windows); Linux comes nearly free via bevy/slint/winit.
- **D10 — Crate layout: 4 crates** — `client` (binary), `client_net`, `client_ui`, `asset_tools`.
- **D11 — F1 content:** map 41 (Venter), chars `lkjsnlfknlsk`/`ninja`, mobs #101 Perro Salvaje + #2101 Zorro del Desierto, minimal UI (chat + HP bar + hotbar).
- **D12 — F0 order:** slint-in-bevy spike FIRST (1–2 days), before any real UI work.
- **D13 — gr2 strategy:** opengr2-rs as a build-time CONVERTER producing glTF. Synthesis after debate: runtime gr2 rejected because of skinning/animation translation risk; opengr2-rs maturity 4★, dormant since 2023; DDS/epk/map still need conversion anyway; the glTF ecosystem enables LOD/meshopt/KTX2 for the scale vision; future content is authored natively in glTF.
- **D14 — Map = 3 LAYERS** (industry standard, validated by WoW ADT/MCNK + MDDF/MODF and UE5 World Partition): (1) terrain = heightmap (PNG/EXR, chunked, LOD, streaming 5×5 around the player, collision from the blocked/water attribute grid LZO1X); (2) objects = placement layer (JSON: model ref + x/y/z/rot/scale, instanced, 1 draw call for many trees); (3) models = glTF from asset_tools. Round-trip to Blender: heightmap displacement + placements addon (~100–150 lines, import/export empties as JSON). WYSIWYG in-game editor **deferred past F5** (amendment G4: Blender round-trip + the legacy client as reference cover F3 content needs). AI-assisted map generation (procedural heightmaps, text-generated placements, Blender MCP for tweaks). Rationale: the user is not a 3D modeler; the game already has 20 years of assets to convert; no 3D modeling required.

**Minor technical notes (agreed, part of this ADR):**

- **Network:** tokio dedicated thread + the `protocol` crate; dev connects to `127.0.0.1:30001`/`30003`. `client_net` scope: handshake with clock-bias retries, the phase machine, TimeSync, reconnect using LOGIN_BY_KEY (the server supports it — avoids resending the password), and both LOGIN3 variants (88 B auth with version+hwid vs 65 B channel).
- **Audio:** kira.
- **Input/camera:** winit + gilrs via bevy; third-person legacy-style camera.
- **Locale:** the client translates (existing architecture, AGENTS.md §17); data converted to JSON in asset_tools.

## Alternatives considered

- **Engine — custom wgpu (m2rs-style)**: rejected (D1) — engine work (scene graph, asset pipeline, input, camera) would consume the rewrite; Bevy 0.19 provides glTF native, GPU-driven rendering and multi-window out of the box.
- **Engine — macroquad**: rejected (D1) — minimal immediate-mode framework; lacks the asset/glTF/GPU-driven ecosystem the scale vision needs.
- **Organization — same Cargo workspace as the server**: rejected (D5) — bevy's dependency tree would slow every server build/test.
- **Organization — separate repository**: rejected (D5) — protocol sync would be manual; two backups; the `protocol` crate must always match the server.
- **UI — egui (MIT/Apache)**: researched (D6) — the proven MMO-HUD pattern; not chosen (user preference for slint); remains the contained fallback after the F0 spike re-evaluation point, including as the license fallback (D6).
- **gr2 — runtime parsing**: rejected (D13) — skinning/animation translation risk at runtime with no escape hatch; a failed parse would block the shipped client.
- **gr2 — GrannyConverterLibrary/Blender**: rejected (D7, D13) — non-Rust, not reproducible in the build; conversion must be 100% Rust (D7).
- **Map — single-layer engine-native scene**: rejected (D14) — no round-trip to Blender, no AI-assisted generation, no streaming/LOD discipline; the 3-layer model is the industry standard (WoW, UE5 World Partition).

## Consequences

- Server workspace stays fast; the `protocol` crate is shared and always in sync (D5).
- The shipped client has zero proprietary formats: every legacy format is consumed at build time (D2).
- Dual-client period until F5 parity: the legacy client remains playable and maintainable (D3); A/B reference available.
- Converted assets grow in-repo per phase; Git LFS deferred until storage actually hurts (D8).
- slint×bevy integration risk is bounded by the F0 spike; the fallback (egui) is a leaf-layer swap (D6). **slint license terms are verified at the F0 spike**; if the free/logo-credit terms do not hold, the egui (MIT/Apache) fallback or a commercial license at distribution applies (D6).
- The epk/MMPT/MIPX unpacker is a from-scratch effort (amendment A): only LZO1X parsing existed in `game_core/src/map.rs`; TEA-ECB is a small well-known cipher — days, not weeks.
- opengr2-rs is build-time only: file-version gaps fail the build, never the runtime; F1 scope limits exposure to a small known asset set (D13). If a 2005-era Granny file defeats opengr2-rs, a one-off Blender conversion committed as glTF is acceptable (implementation note 3).
- Bevy 0.19 pins the render stack (wgpu 29); upgrades happen between milestones, never mid-phase.
- Windows first; Linux "nearly free" via bevy/slint/winit (D9).
- F0 is independent of the server (spike + asset_tools run offline); F1 requires the running Rust server (`scripts/start_win.ps1`, auth 30001 / channel 30003).

## Amendments (2026-08-13, oracle review)

Applied after the architecture oracle review (verified by the orchestrator). In-place edits above are marked; the deltas not visible in the decision text:

- **A — D7 evidence correction (HIGH):** removed the claim that TEA-ECB/epk are reverse-engineered in `game_core/src/map.rs`. Verified: `map.rs` contains ONLY LZO1X (server_attr tiles); no TEA-ECB/MMPT/MIPX/epk unpacking exists in `source/reforge` (only PanamaPack tests in `protocol/legacy.rs` and historical Python tooling). The epk unpacker is a from-scratch effort — TEA-ECB is a small well-known cipher (days, not weeks). The 100% Rust decision stands.
- **B — Streaming to F1 (MEDIUM):** 5×5 chunk streaming + chunked LOD are F1 deliverables (D14 core design), not F4. Map 41 is huge (2.17 M blocked cells).
- **C — F4 scope cut (MEDIUM):** multi-window and KTX2/meshopt removed from F4 (YAGNI — no value before parity; multi-window is not in the legacy client, slint popups suffice). F4 rebalanced to effects, particles, combat polish; the remaining perf work merges into F5 (G3).
- **D — slint license (D6):** slint RETAINED (user decision 2026-08-13); license terms verified at the F0 spike; if the free/logo-credit terms do not hold → egui fallback (MIT/Apache) or a commercial license at distribution.
- **E — client_net scope:** network note expanded (handshake with clock-bias retries, phase machine, TimeSync, LOGIN_BY_KEY reconnect, LOGIN3 variants).
- **F — Blind spots:** see "Implementation notes" below (items 1–9).
- **G — Dev-time reductions:** applied to `../history/plans/client-rewrite.md` — prior-art reuse (G1), F1 cut (G2), F4→F5 merge (G3), WYSIWYG deferral (G4), golden screenshots (G5), PNG-only until parity (G6).

## Implementation notes (oracle review 2026-08-13)

Blind spots from the review, carried into the plan (risks/sections there):

1. **Heightmap source:** the legacy `.map` file is the source — the terrain heightmap must be GENERATED from it. The server collision grid is 2D attributes only; height is cosmetic/physics.
2. **Placement layer source:** the legacy `.map` file IS the placement data — parse it directly into placements JSON for map 41 (no manual authoring).
3. **gr2 escape hatch:** if opengr2-rs cannot read 2005-era Granny files or the skinning/animation mapping fails, a one-off Blender conversion committed as glTF is acceptable (D8 keeps outputs in the repo). The Rust tool remains the target, not the blocker.
4. **Missing converters:** `icon.epk` (TGA/BMP — the F1 hotbar needs it), fonts (`.fnt` bitmap + CJK glyph coverage), EFT particle effects (skills), MTR materials, `sound.epk` (audio for kira) — added to the plan's asset_tools table.
5. **CP949:** server-originated text (e.g. `GC_CHAR_ADDITIONAL_INFO` names) arrives as CP949 bytes — the client needs CP949→UTF-8 transcoding (`encoding_rs` crate).
6. **Prediction/interpolation:** the server validates movement with an envelope (correction-not-ban) — a naive client will rubber-band. The client owns its half in F2 (the server plan §5.7 commits its half).
7. **IME:** Korean/Chinese chat input — spike in F2 (winit/slint IME support is spotty).
8. **4 GB dev host:** use `dynamic_linking` for dev builds; the F0 spike must confirm the DX12 backend exists on the dev GPU or fall back to GL.
9. **Bevy churn:** budget a dedicated bevy-upgrade task between milestones (not just a risk row).

## Not decided in this ADR

- Exact F2–F4 content ordering beyond the F0/F1 milestones — sequenced in `../history/plans/client-rewrite.md`, mirroring the server's F5.3 slice order (movement → combat → items → NPCs → quests → shops/trade).
- Server-side F7 items (real encryption, "new protocol" beyond the current `protocol` crate contract) — coordinated with the server lane when they land.
- Git LFS activation, Linux release packaging, audio asset format strategy, and the legacy-client retirement date (a post-F5-parity decision point).
