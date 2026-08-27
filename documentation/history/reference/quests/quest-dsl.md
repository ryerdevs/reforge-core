---
Type: Reference
Status: Accepted (2026-08-13)
Audience: Contributors, quest designers
Last verified: 2026-08-13
---

# Quest DSL — Specification v0.1 (draft for discussion)

> **Status: ACCEPTED (2026-08-13, 41st part)** — the `quest_dsl` crate implements this spec (`source/reforge/quest_dsl`: ast/parser/family/render; 44 tests green, clippy clean) and the qc→DSL converter proves the grammar against the full legacy corpus (**194/194 files convert, 0 failed, ~2 s**; 6 family proposals covering 112/194 files). The former draft decisions (§11) are resolved and recorded below; the draft text that follows is kept as history.
> **Date:** 2026-08-09 · **Project:** Metin2 server rewrite in Rust
> **Context:** replaces the Lua runtime (mlua 5.1 + shim) previously considered. Decision: **no scripting language in the Rust server; quests as declarative data in an own DSL.**
> Canonical plan: [../../plans/server-rewrite.md](../../plans/server-rewrite.md) (§12).

## 1. Context and goals

The legacy server runs quests in Lua 5.0 (lexer patched for EUC-KR) compiled from Metin2's own DSL (`qc`). The real content is 194 `.quest` files (~2,500+ duplicated lines in the `collect_quest_lv30..lv96` family alone).

**Decision:** eliminate Lua entirely. Design an own declarative DSL, typed and verified by a Rust parser, with composition (families + blocks + imports) to remove the repetition.

**Format goals:**
1. Readable and elegant for quest designers (not programmers).
2. Short: a long quest is composed, not repeated.
3. Zero arbitrary logic: only composition of known typed actions.
4. Load-time validation with file:line:column errors.
5. Automatic, verifiable migration from legacy content.
6. The same parser serves runtime, CLI validator and editor (a single source of truth).

**What it is NOT:** a scripting language. No arbitrary flow variables, no free loops, no general functions. Cases that require them (event managers like `oxevent`) are written in Rust as server modules.

## 2. Basic syntax

- Suggested extension: `.quest` (same as legacy — eases diff during migration).
- **Significant indentation** (2 spaces). No `begin/end`, no `(`, no commas.
- `#` for comments.
- A quest = `quest` blocks, inside them `state`, inside them `on`, inside them `->` actions.

```quest
# quests/biology/collect_quest_lv30.quest
quest collect_quest_lv30
  state start
    on login, levelup with pc.level >= 30
      -> set_state(information)

  state information
    on letter
      -> send_letter(gameforge.collect_quest_lv30._10_sendLetter)

    on button, info
      -> say_title(gameforge.collect_quest_lv30._10_sendLetter)
      -> say(gameforge.collect_quest_lv30._20_say)

    on 20084.chat
      -> say_title(gameforge.collect_herb_lv10._50_sayTitle)
      -> say(gameforge.collect_quest_lv30._40_say)
      -> wait()
      -> set_qf(duration, 0)
      -> set_qf(collect_count, 0)
      -> set_state(go_to_disciple)

    on 601.kill with number(1, 100) <= 5
      -> give_item2(30006, 1)
```

### 2.1 Grammar rules

| Construction | Syntax | Notes |
|---|---|---|
| Quest | `quest <name>` | One per file (optional `import` on top) |
| State | `state <name>` | `start` mandatory; `__complete`, `__giveup__` convention |
| Event | `on <trigger>[ , <trigger>...]` | Multi-trigger with comma; implicit `or` |
| Condition | `with <expression>` | Optional; typed expression (see §4) |
| Action | `-> <action>(<args>)` | One per line; `()` optional when no args |
| Block | `block <name>[ (<param>: <type>)]` | Reusable (§7) |
| Block use | `use <name>[ (<args>)]` | Inside an event |
| Import | `import <file>` | No extension, relative to `quests/` |
| Family | `quest <name> = <base>(<param>: <value>)` | Parameterized instance (§6) |
| Comment | `# ...` | Whole line |

**Validation rule:** every action, trigger and condition is **known to the parser** (typed catalog). An unknown name = load error with file:line. There is no escape into free code.

## 3. Triggers (events) — inventory of the real corpus

Extracted from the 194 deployed `.quest` files (germany). This is the catalog; the converter completes and audits it.

| Trigger | Syntax | Semantics (legacy) |
|---|---|---|
| Login | `login` | Enters the world |
| LevelUp | `levelup` | Gains a level |
| Letter | `letter` | Opens the quest journal |
| Button/Info | `button`, `info` | Presses the quest button or info |
| NPC chat | `<vnum>.chat` | Talks to the NPC |
| Kill | `<vnum>.kill` | Kills the mob |
| Item use | `<vnum>.use` | Uses the item |
| Target click | `__TARGET__.target.click` | Clicks the marked target |
| Enter | `enter` | Enters a map |
| Logout | `logout` | Logs out |
| Timer | `timer` (to be defined with `pc.setqf` cooldown, see §4) | Legacy: `get_time()` pattern |
| Select | `select` (action) | Option menu (action, not trigger) |

**Special event triggers (deferred or Rust):** `arena.*`, `oxevent.*`, `d.*` (dungeon), `wedding.*` — the events corpus is audited in the conversion phase; those that do not fit the DSL → Rust modules.

## 4. Conditions (typed expressions)

Mini expression language, parsed and typed by the parser. Supports: comparison (`==`, `!=`, `<`, `>`, `<=`, `>=`), basic arithmetic (`+`, `-`, `*`, `/`), `and`, `or`, `not`, parentheses, numeric/string literals.

| Function | Syntax | Legacy |
|---|---|---|
| Level | `pc.level >= 30` | `pc.level` |
| Count item | `count_item(30006) > 0` | `pc.count_item(vnum)` |
| Quest flag | `get_qf(duration) != 0` | `pc.getqf("duration")` |
| Probability | `number(1, 100) <= 5` | `number(min, max)` (random integer) |
| Time | `get_time() >= get_qf(duration)` | `get_time()` |
| Map | `get_map_index() == 113` | `pc.get_map_index()` |
| GM | `get_gm_level() == 5` | `pc.get_gm_level()` |
| Pet | `pet.is_summon(34003)` | `pet.is_summon(vnum)` |
| Test server | `is_test_server()` | — |
| Level range | `pc.level between 15, 39` | — (new, friendly syntax) |

**Open decisions (§11):** native `between` or only composed comparisons? Do we need `get_qf(...) between a, b`?

## 5. Actions — inventory of the real corpus

| Action | Syntax | Legacy |
|---|---|---|
| Title dialog | `say_title(key)` | `say_title(...)` |
| Dialog | `say(key)` | `say(...)` |
| Shown reward | `say_reward(key)` | `say_reward(...)` |
| Shown item | `say_item_vnum(30006)` | `say_item_vnum(vnum)` |
| Send letter | `send_letter(key)` | `send_letter(...)` |
| Clear letter | `clear_letter()` | `clear_letter()` |
| Wait | `wait()` | `wait()` (coroutine yield → event) |
| Change state | `set_state(name)` | `set_state(...)` |
| External quest | `set_quest_state(quest, state)` | `set_quest_state(...)` |
| Quest flag | `set_qf(name, value)` | `pc.setqf("k", v)` |
| Give item | `give_item2(vnum[, count])` | `pc.give_item2(...)` |
| Remove item | `remove_item(vnum, count)` | `pc.remove_item(...)` |
| Mark target | `target_vid(name, npc_vnum, key)` | `target.vid(...)` |
| Delete target | `target_delete(name)` | `target.delete(...)` |
| Teleport | `warp(x, y)` | `pc.warp(...)` |
| Global notice | `notice(key)` | `notice(...)` |
| Multi-line notice | `notice_multiline(key, notice_all)` | — |
| Affect/buff | `affect_add(apply.MOV_SPEED, 10, seconds)` | `affect.add_collect(...)` |
| Remove affect | `affect_remove(...)` | — |
| Menu | `select(key1, key2...)` | `select(...)` (returns index → needs branches: §10) |
| Input | `input_number(key)` | `input_number(...)` |

**Full catalog:** the 982 entries of `quest_functions` (inventory of the legacy API) are audited in the conversion phase; only those used by the real corpus are ported to the DSL. The rest dies or moves to Rust.

## 6. Parameterized quest families

Removes the repetition of near-identical quests (real case: 11 files `collect_quest_lv30..lv96` = the same quest with different numbers).

```quest
# quests/biology/collect_quest.family.quest
quest collect_quest family (level, mob, herb, drug)
  state start
    on login, levelup with pc.level >= (level)
      -> set_state(information)

  state information
    on letter
      -> send_letter(@100_sendLetter)

    on (mob).kill with number(1, 100) <= 5
      -> give_item2((herb), 1)

    on (drug).use with get_qf(duration) == 0
      -> remove_item((drug), 1)
      -> set_qf(duration, get_time() + 60 * 60 * 22)

# instances (real quests)
quest collect_quest_lv30 = collect_quest(level: 30, mob: 601, herb: 30006, drug: 71035)
quest collect_quest_lv40 = collect_quest(level: 40, mob: 602, herb: 30007, drug: 71036)
quest collect_quest_lv50 = collect_quest(level: 50, mob: 603, herb: 30008, drug: 71037)
```

- Parameter: `(name)` in conditions/actions; `(name)` without spaces.
- Parameterized text keys: `@100_sendLetter` → the `@` prefix marks a locale key; the converter generates it per level (the family carries its own key index).
- **The automatic converter detects diff-near-identical quests and groups them into families** (similarity heuristic + human confirmation).

## 7. Reusable blocks and imports

```quest
# quests/common/helpers.quest
block npc_target(npc: vnum, key: key)
  -> target_vid(__TARGET__, (npc), (key))

block reward_sequence(title, text, next_state)
  -> say_title((title))
  -> say((text))
  -> wait()
  -> set_state((next_state))
```

```quest
# quests/biology/collect_quest_lv30.quest
import helpers

state information
  on letter
    use npc_target(20084, @150_sayTitle)
    -> send_letter(@10_sendLetter)

  on 20084.chat
    use reward_sequence(@50_sayTitle, @40_say, go_to_disciple)
    -> set_qf(duration, 0)
```

- `block` and `use` only compose typed actions/conditions — no free logic.
- `import` shares blocks between quests and serves as a base library (`helpers.quest`).
- Validation: the parser resolves blocks and imports at load time; an import cycle = error.

## 8. Special cases → Rust (not DSL)

Event managers with real coordination logic (GM, global flags, sequences) — the corpus confirms it with `oxevent.quest`, `christmas_*`, `oxevent_manager`, `game.set_event_flag`:

**Decision:** these are reimplemented as **Rust server modules** (with the same trigger/action API available via native bindings). The DSL does not grow into a general language to accommodate them.

## 9. Automatic legacy conversion (qc → DSL)

1. Parse the 194 `.quest` files with the qc parser (extract the real AST of the legacy DSL).
2. Translate AST → DSL v2: triggers/conditions/actions mapped by the equivalence tables (the same ones from §3–5!).
3. Family detection (AST diff) + proposed grouping.
4. Extraction of repeated blocks (common-subtree analysis).
5. **Parity harness:** run the same quest in the legacy server (oracle) and in the Rust engine with the same simulated inputs → same final state and same dialog output. It is the same harness that validates the engine in F5.
6. Output: `quests/` in DSL + discrepancy report + list of quests requiring manual review (those that do not fit the DSL → Rust).

**Rule:** no migrated quest is considered converted without passing the parity harness.

## 10. Branches and flow inside an event

The legacy uses `if/else` and `select(...)` with branches in the body. The DSL declares branches per event:

```quest
on 20011.chat
  -> select(@_20_say, @_30_say) as choice
  if choice == 1
    -> warp(896500, 24600)
  else
    -> return

on 20011.chat with get_gm_level() == 5
  -> input_number(@_160_say) as amount
  if amount > 200
    -> say(@_250_say)
```

- `as <name>` captures the result; `if/else` branches only over captured results and simple conditions.
- No loops. No mutable variables outside the event scope. (The legacy uses them for counters → in the DSL they are `set_qf`/`get_qf`, persisted.)

**Open decision (§11):** nested `if` allowed (1 level) or `elif`? We propose 1 level + `else` to keep readability.

## 11. Open decisions (for reviewers) — RESOLVED 2026-08-13

All six decisions were resolved by the `quest_dsl` core implementation (41st part; the draft questions below are kept as history):

1. `between a, b` in conditions — **RESOLVED: native `between`** (parsed as a first-class comparison, not composed).
2. `if` inside events — **RESOLVED: 1 level + `else`** (readability; no `elif`).
3. `select` with `as` capture — **RESOLVED: `as`-capture covers the corpus** (no nested-menu restructure forced).
4. Locale keys — **RESOLVED: `@key`** (per-family key table).
5. Naming — **RESOLVED: `.quest`** (continuity with legacy; eases diff during migration).
6. `wait()` / timers — **RESOLVED: explicit `timer` trigger** (alias; not only composed `on login ... with get_time()` conditions).

Original draft questions (historical):

## 12. Out of scope of this spec

- **Rust runtime engine** (state machine + `wait()` scheduler) — designed in phase F5 (future; not implemented yet).
- Parity harness (dual execution) — designed in phase F0/F5.
- CLI validator `quest-validate` and editor schema — designed after this spec is closed.
- Visual GUI editor — **excluded by decision** (YAGNI; validator + schema cover 90% of the benefit).
