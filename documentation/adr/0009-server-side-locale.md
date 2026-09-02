---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Last verified: 2026-09-01
---

# ADR-0009: Server-Side Locale (text ownership)

## Status

Accepted (2026-08-12). Design closed with the user — see the [historical locale
plan](../history/plans/locale-redesign.md).

## Context

The client renders all text from its pack locale (17 per-language folders, locale.epk ≈ 163 MB): mob/item/skill names, item descriptions, UI strings, map-name images. Consequences:

- The server cannot control any displayed text: renaming a mob or adding a new item requires editing the pack and repacking the client.
- Multilang is client-side only; the legacy Language System (16 `locale_string_XX.txt` files) only covers chat/notices with known gaps (fixed-ES quest texts, missing keys).
- New content (mobs, items, maps, languages) is spread across pack folders, server files and the DB.

Goal: the server owns **all** text per player language, stored human-friendly, so the future web panel (item browser, locale CRUD, map editing) manages content as data.

## Decision

Server-owned locale system. The term "locale" is kept; the mechanism changes entirely.

**Data — one table per text domain, schema `common`, one row per (entity, language):**

| Table | Key | Purpose |
|---|---|---|
| `mob_names` | (vnum, lang) | mob/NPC names |
| `item_names` | (vnum, lang) | item names |
| `item_descriptions` | (vnum, lang) | item descriptions |
| `skill_names` | (skill_id, lang) | skill names |
| `map_names` | (map_id, lang) | map display names |
| `ui_texts` | (key, lang) | interface strings (locale_interface.txt keys) |
| `message_texts` | (key, lang) | server chat/notice strings — **server-side only** (client receives composed chat) |
| `item_icons` | (vnum) | PNG 32×32, one per group of 10 vnums — **panel web only** (client keeps pack TGA) |

Rows per language, not columns: adding a language = INSERTs, never ALTER TABLE.

**Wire — one request/response pair, one cache:**

- `CG_LOCALE_REQUEST` (carries `lang`) / `GC_LOCALE` (typed sections: mob, item, item-desc, skill, map, ui). The auth role is the sole responder and serves the bundle during authentication/loading. For the external compatible client, this is a bootstrap-only contract: after `GC_PHASE(GAME)`, the channel must neither push nor respond with `GC_LOCALE` (header 140), because the client's Game parser rejects it. In-game hot reload is deferred until a compatible client Game-phase parser/cache and a version-gated rollout exist outside this public server repository.
- Single channel for ALL text: quest texts join later as a new section (`common.quest_texts`) when the quest engine lands — no design change.
- Chunked if the legacy wire caps packet size (verified in F1).

**Client — one cache module (CPythonLocale), per-domain maps:** mob/NPC names, item names/descriptions, skill names, uiScriptLocale, map names (image kept as style + localized text overlay). Fallback chain: cache → pack (transition) → empty. Language selector at login (default **EN**), applies during authentication/loading. In-game hot switching is deferred until the compatible client has a Game-phase parser/cache and the version-gated rollout described above.

**Server — importer CLI (one subcommand per domain):** `import-mobs`, `import-items`, `import-skills`, `import-ui`, `import-messages`, `import-icons` — re-runnable at will. Fallback language: **EN** (diverges from legacy `LANGUAGE_DEFAULT = ES`, intentional; EN is the completeness baseline of the new data).

**Refined items (+1..+9):** the legacy wire requires one vnum per refine level. The DB keeps the vnums (wire contract) but the importer derives them from a single base definition: name = base + `" +N"` (one `item_names` row per base), icon = group-of-10 PNG (one row), stats = base + refine scaling. The panel edits ONE item with a +0..+9 slider. No manual duplication.

## Alternatives considered

1. **Keep pack-bound locale (status quo)** — server cannot control text; every change requires a repack. Rejected.
2. **Single key/value table** `locale(key, lang, value)` — simplest code, but anti-human data (keys like `mob.101`; no real columns, no joins). Rejected in favor of modular per-domain tables.
3. **Pack crypto upgrade (TEA → AES)** — raises the bar for casual unpackers but the key stays in the binary; security theater against a determined attacker. Deferred (not part of this ADR).
4. **Pull-on-demand protocol** for entity names — YAGNI: the login bundle covers all text; a pull can be added later if ever needed.
5. **DX11 + Slint client** — full UI rewrite, months of work, re-port of all verified fixes; ADR-0007 boundary. Parked (project with its own ADR when the server is playable).

## Consequences

Positive:

- Text becomes SQL rows: rename a mob = `UPDATE`; new language = INSERTs; no pack, no client rebuild.
- The locale cache is the single channel for all text (UI today, quests later) during the supported authentication/loading bootstrap; in-game refresh is a separately gated client capability.
- Panel-ready: item browser (names + stats + icons), locale CRUD, refinement slider — all over the same tables.
- locale.epk shrinks to visuals only; pack staleness stops affecting text.

Negative / costs:

- Client changes: CPythonLocale + 5 touchpoints + loading screen + language selector → one rebuild per phase, each verified empirically.
- Initial data incomplete: ES complete, EN/DE partial → fallback shows gaps until imported.
- Legacy C++ baseline untouched (oracle); the EN default applies to the new system only.
- The compatible client's Game parser rejects header 140 (`GC_LOCALE`), so in-game hot reload remains deferred until an external compatible client parser/cache rollout is version-gated outside this repository.
- New wire pair (`CG_LOCALE_REQUEST`/`GC_LOCALE`) must stay byte-exact (F0 protocol crate discipline).

## References

- Plan: [historical locale redesign](../history/plans/locale-redesign.md)
- Recon 2026-08-11: client text surface (exp-1), server locale machinery (exp-2)
- Legacy context: [`AGENTS.md`](../../AGENTS.md), [historical language-system
  reference](../history/reference/legacy/language-system.md)
