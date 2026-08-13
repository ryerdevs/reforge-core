---
Type: Plan
Status: F1 importer DONE (2026-08-12 — tables live, data imported, counts in CHANGELOG); wire slice next (client cache integration DONE 2026-08-12, 9th part); ADR-0009 **Accepted (2026-08-12)**
Audience: Contributors, maintainers
Last verified: 2026-08-12
---

# Server-Side Locale — Redesign Plan

## Summary

Replace the client pack-bound locale with a **server-owned text system**. The name "locale" stays (project familiarity); everything else changes.

**One table per text domain, one request/response pair, one lookup.**

- PostgreSQL tables by domain — one per text type (`mob_names`, `item_names`, `item_descriptions`, `ui_texts`, `skill_names`, `map_names`, `message_texts`). Human-readable: real columns, one row per (entity, language).
- One request/response pair `CG_LOCALE_REQUEST` / `GC_LOCALE` (pattern of F5 manifests): the client asks for a language at any time and the server answers with that language's data for every domain (merged with the **EN** default for missing keys). Used at connect, at login (language change), and for **hot reload** (language/content change mid-session, no restart).
- The client keeps one cache module with per-domain maps and answers every name/string request from it, falling back to the pack during transition, then to nothing.
- A **loading screen at game start** (same pattern as the map-loading screen) covers the locale fetch and doubles as the future content updater (no launcher needed).

## Why

Today the client renders text from its pack: `locale/es/mob_proto`, `item_proto`, `itemdesc.txt`, `SkillDesc.txt`, `locale_interface.txt`, map-name images — 17 per-language folders (locale.epk ≈ 163 MB). Consequences:

- The server cannot control any displayed text. Renaming a mob or adding a new item requires editing the pack and repacking the client.
- Multilang is client-side only; the server's Language System (16 `locale_string_XX.txt` files) only covers chat/notices, with known gaps (fixed-ES quest texts, missing keys).
- New content (mobs, items, maps, languages) is spread across pack folders, server files and the DB.

## Goals / Non-goals

Goals:

- All displayed text comes from the server, per player language.
- Adding/renaming text = SQL rows. No pack, no client rebuild.
- **Language change without client restart**: the player picks a language in the login screen (default **EN** on a fresh client); it applies at login and can be switched live afterwards (hot reload).
- One data source for client text AND server chat text.
- Keep the term "locale".
- **Human-friendly data**: tables grouped by domain, real columns, readable names — not a key/value bag.

Non-goals (for now):

- Removing the visual part of the pack protos (models, icons) — the client still needs them.
- Removing the map-name image entirely (see decision: image kept as style, localized text overlaid).

## Data model — one table per domain (modular)

All in schema `common` (same schema as the legacy `locale` table). One row per (entity, language).

| Table | Columns | Example |
|---|---|---|
| `common.mob_names` | `vnum bigint`, `lang varchar(4)`, `name text` | `(101, 'es', 'Perro Salvaje')`, `(101, 'en', 'Wild Dog')` |
| `common.item_names` | `vnum bigint`, `lang varchar(4)`, `name text` | `(173217, 'es', 'Espada Corta')` |
| `common.item_descriptions` | `vnum bigint`, `lang varchar(4)`, `text text` | `(173217, 'es', 'Un arma ligera...')` |
| `common.skill_names` | `skill_id int`, `lang varchar(4)`, `name text` | `(12, 'es', 'Golpe de Fuerza')` |
| `common.map_names` | `map_id int`, `lang varchar(4)`, `name text` | `(41, 'es', 'Aldea del Este')` |
| `common.ui_texts` | `key text`, `lang varchar(4)`, `value text` | `('INVENTORY', 'es', 'Inventario')` — keys of locale_interface.txt |
| `common.message_texts` | `key text`, `lang varchar(4)`, `value text` | `('WELCOME', 'es', '¡Bienvenido, {name}!')` — server-side only |
| `common.item_icons` | `vnum bigint PK`, `png bytea` | icono PNG 32×32 del pack (TGA→PNG, 1 por grupo de 10 vnums, ~4MB total) — solo para el panel web, el cliente nunca lo lee |

Design choices:

- **Rows per language, not columns**: adding a new language = INSERT rows, never ALTER TABLE. A future panel presents the same data as a grid (entities × languages) for human editing.
- **Keys stay readable**: UI/message domains have text keys (the legacy file keys, e.g. `INVENTORY`), numeric domains have the real ids.
- **No id-name collision**: `mob_names.vnum` joins cleanly with `mob_proto`; `map_names.map_id` with the maps table.
- **Fallback language: EN** — when a key is missing in the player's language, the server merges the EN value; if neither exists, the row is omitted (client shows empty / pack fallback during transition).
- **Refined items (+1..+9)**: the legacy wire requires one vnum per refine level (base+1..+9, e.g. 173217..173226). The DB keeps the vnums (wire contract) but the importer **derives** them from a single base definition: name = base + `" +N"` (one `item_names` row per base), icon = group-of-10 PNG (one row, shared), stats = base + refine table scaling. The panel edits ONE item with a +0..+9 slider; the server generates the refined vnums on the fly. No manual duplication.

## Maps & spawns in PostgreSQL (same pattern, F1)

Same architecture as the locale tables: the runtime map files become import-only; the server reads PG.

| Table | Columns | Example |
|---|---|---|
| `world.maps` | `map_id int PK`, `name text`, `base_x int`, `base_y int`, `spawn_x int`, `spawn_y int` | `(41, 'metin2_map_c1', 921600, 204800, 969600, 278400)` — from index + Setting.txt (BasePosition) + Town.txt |
| `world.spawns` | `map_id int`, `vnum bigint`, `x int`, `y int`, `count int`, `kind text` | `(41, 101, 957600, 247300, 3, 'mob')` — expanded spawns from regen/npc/boss/stone.txt (groups already resolved), in UNITS |

- The importer reuses the verified parity parser (`game_core::npc::load_map_spawns`) — no reimplementation; output `SpawnEntry` list is byte-identical, so the 6 channel tests keep passing plus a new parity test (PG vs files for map 41).
- The channel loads per map **at each world entry** (`SELECT * FROM world.spawns WHERE map_id=$1`): edits are visible on the next entry without restart (hot reload for new entries). Live refresh for players already in the map arrives with the respawn runtime (NOTIFY-based, like the locale hot reload).
- Files stay as import-only source (like the pack for locale); runtime reads never touch them.

## Wire — one request/response pair

`CG_LOCALE_REQUEST` (client → auth, carries `lang`) and `GC_LOCALE` (auth → client, typed sections: mob, item, item-desc, skill, map, ui — message_texts stays server-side). Both roles can serve it (it is stateless), which is what makes hot reload work: the client can re-request at any time and swap its cache. **This is the single channel for ALL text** — quest texts will be a future section (same packet, same cache, same tables, e.g. `common.quest_texts`) when the quest engine lands; no design change required.

**Startup flow (with loading screen):**

```
client starts → loading screen ("Conectando...")
  → connect auth → CG_LOCALE_REQUEST(lang)
  → GC_LOCALE (chunked if the legacy wire caps packet size — to verify empirically in the F1 wire slice)
  → login screen rendered with server strings → LOGIN3 (login, lang)
```

**Language change / hot reload:** at any point (login screen or in game) the client re-sends `CG_LOCALE_REQUEST(new_lang)` → `GC_LOCALE` → cache swapped → UI re-renders live. Same mechanism serves content hot reload (a rename in PG is visible after the next request).

## Client — one cache module, per-domain maps

New module CPythonLocale: receives `GC_LOCALE`, fills per-domain maps; `SetLanguage(lang)` swaps them and triggers a UI refresh.

| Today (pack loads) | After (lookup) |
|---|---|
| `LoadLocaleData("locale/es/mob_proto")` | `mob_names[vnum]` |
| `LoadItemTable` / `LoadItemDesc` | `item_names[vnum]` / `item_descriptions[vnum]` |
| `RegisterSkillDesc` | `skill_names[skill_id]` |
| `uiScriptLocale.<KEY>` from locale_interface.txt | `ui_texts[KEY]` |
| map-name images (`c1.tga` …) | image kept (single set) + `map_names[map_id]` text overlay |

Fallback chain: locale cache → EN (server-merged) → pack (transition) → empty.

Touchpoints (recon 2026-08-11):

- `PythonApplication.cpp:859-948` — LoadLocaleData → CPythonLocale
- `PythonNetworkStreamPhaseGameActor.cpp:132-140` — mob names (server fallback is commented today)
- `PythonNetworkStreamPhaseGameActor.cpp:164-172` — NPC names (pack first today → locale first)
- `uiscriptlocale.py:62-63` — uiScriptLocale from pack → from cache
- `uimapnameshower.py:158-166` — map name image + text overlay

## Server — two pieces

1. **Importer** (CLI, one-time, re-runnable; one subcommand per domain, independent):
   - `import-mobs` — mob names from the pack ES dump (DumpProto)
   - `import-items` — item names + descriptions from item_proto/itemdesc
   - `import-skills` — SkillDesc.txt
   - `import-ui` — 17 × `locale_interface.txt`
   - `import-messages` — 16 × `locale_string_XX.txt`
2. **Auth** (`server_realms`): answers `CG_LOCALE_REQUEST` with the player language's rows per domain, merged with EN for missing keys, sends `GC_LOCALE` (chunked if needed).

## Future workflows (how it stays easy)

| Task | What you do |
|---|---|
| Add a new language | `INSERT` rows in each table (or re-run the importers with the new language files). Nothing else: the client requests its language at login and gets it. |
| Change player language | The player picks it in the client; it applies at login and can be switched live (hot reload). No restart. |
| Rename a mob | `UPDATE common.mob_names SET name = ... WHERE vnum = 101 AND lang = 'es'` — visible after the next locale refresh. |
| Add a new item | `item_proto` row (visuals+stats) + rows in `item_names` / `item_descriptions` → in-game immediately. |
| Add a new map | Map files to client (visual, one-time content) + maps/spawn_points rows + `map_names` row. |
| Add a UI string | One row in `common.ui_texts`. |
| New chat text | One row in `common.message_texts`; F3 server resolves per player. |
| New mob | `mob_proto` row + `mob_names` rows. |

## File / data organization

| Artifact | Where |
|---|---|
| Locale tables | PostgreSQL, schema `common` (7 tables) |
| `CG_LOCALE_REQUEST` / `GC_LOCALE` encoders | `protocol` crate (byte-exact, typed sections) |
| Importer | new tool under reforge (e.g. `source/reforge/tools/locale_import`, one subcommand per domain) |
| Client cache | `source/client` (new CPythonLocale + 5 touchpoints + loading screen) |
| Pack locale | stays as fallback during transition; deletable later |

## Phases

- **F0 — Foundation**: locale tables + importer + ES/EN/DE data + verify the pending spawn-resolve perf fix.
- **F1 — Fetch + names**: `CG_LOCALE_REQUEST`/`GC_LOCALE` at connect + loading screen + client cache + mob/NPC names from cache. Verification: rename a mob in PG → see it in game, no rebuild.
- **F2 — UI + maps + selector**: UI strings from cache, map names (image + text overlay), language selector at login (default EN), language applies at login without restart.
- **F3 — Chat + hot reload**: Rust chat/notices resolve from `message_texts` per player (retires the 16 `locale_string_XX.txt` files and fixed-ES quest texts); live language/content hot reload through re-request.
- **F4 — Content delivery (updater)**: file transfer over the game connection (chunks + hashes) reusing the loading screen; the client downloads missing maps/packs — no launcher. (Separate slice; the loading screen from F1 is its foundation.)

## Decisions (closed 2026-08-11)

- **Map names**: image kept (single set, original style — the 17 per-language image sets die) + server-provided localized text overlaid.
- **Fetch timing**: at connect, under a loading screen (map-loading pattern), before the login screen renders.
- **Chunking**: chunked if the legacy wire caps packet size (to verify empirically in the F1 wire slice).
- **Language selector**: yes — client-side choice, default EN on fresh install, applies at login, hot-switchable live.
- **Fallback language**: EN (differs from the C++ `LANGUAGE_DEFAULT = ES` — intentional; the new system's data is imported per language and EN is the completeness baseline).

## References

- Recon 2026-08-11: client text surface (explorer session exp-1); server locale machinery (explorer session exp-2).
- Legacy context: `AGENTS.md` §17 (multilang architecture), `docs/reference/legacy/language-system.md`.
