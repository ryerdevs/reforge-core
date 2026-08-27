---
Type: Guardrail
Status: Current
Audience: Contributors, agents, operators
Last verified: 2026-08-10
---

# Guardrail: data and encoding

Rules about character encodings, proto tables, and coordinate conventions. These traps caused real bugs (mojibake, boot aborts, client crashes). Source of truth: `../../AGENTS.md` (fixes 15–17, coordinate convention), `../../CHANGELOG.md`.

## 1. Server locale Lua with Korean MUST be CP949/EUC-KR

- **Rule:** server locale lua files containing Korean use **CP949/EUC-KR (2 bytes/char), NOT UTF-8**. The server's lua 5.0 lexer is modified for Korean EUC-KR (`read_string` in `liblua/5.0/src/llex.c` consumes 2 bytes when `b_current & 0x80`); UTF-8 Korean (3 bytes) misaligns parity → `unfinished string` → `LoadQuestLocale` fails → `locale.monster_chat` undefined.
- **Why:** a deployed UTF-8 `quest/locale.lua` broke quest locale loading (chat spam of ScriptRunErrors until fixed).
- **Evidence:** AGENTS.md fix #15 (conversion script `fix_locale_enc.py`, 349 lines, 0 failures; verified `SYNTAX_OK` + `LoadQuestLocale(...) returns 0`); `../../CHANGELOG.md` 2026-08-08.
- **Consequence:** broken quest text loading; monster_chat spam; players see `@0949`-style placeholders.
- **Status:** Active.

## 2. `PROTO_FROM_DB`: proto tables come from MySQL, not from txt files

- **Rule:** while `ENABLE_PROTO_FROM_DB` + config `PROTO_FROM_DB` are active, the db reads `mob_proto`/`item_proto` from **MySQL** (`ClientManagerBoot.cpp:1290-1309`), NOT from the `db/` txt files. Editing `mob_proto.txt`/`mob_names.txt` has no effect.
- **Why:** a deployed empty `mob_names.txt` produced Korean/mojibake mob names; the txt path silently does nothing.
- **Evidence:** AGENTS.md fix #16 (`MOB: #101 Perro Salvaje` verified in core boot after MySQL update).
- **Consequence:** wasted time editing dead txt files; names stay wrong in game.
- **Status:** Active (baseline behavior; the Rust rewrite replaces this with DB-at-runtime + manifest, plan §5.6).

## 3. Never change server `item_proto` names (CP949 trap)

- **Rule:** the server's drop txt files (`etc_drop_item.txt`, `common_drop_item.txt`, `drop_item_group.txt`, …) reference items **BY NAME in CP949** (`ReadEtcDropItemFile` in `item_manager_read_tables.cpp:457-498` → `GetValidVnum`/`GetVnumByOriginalName`). `item_proto` in MySQL must keep the original CP949 names; visible item names are the client's job.
- **Why:** translating `item_proto` aborts the core boot: `No such an item (name: ...)` → `Boot: cannot load ETCDropItem` (caused a real core crash).
- **Evidence:** AGENTS.md fix #17 "TRAP"; `../../CHANGELOG.md` 2026-08-08.
- **Consequence:** core boot abort; server down until names are restored.
- **Status:** Active. (`mob_proto` names CAN be changed — mobs are not referenced by name in boot txts.)

## 4. PostgreSQL bytes/encoding and migration types

- **Rule:** the Rust side is UTF-8 end-to-end; the client renders text natively in UTF-8 (`GrpTextInstance.cpp:124` `CP_UTF8`). The CP949 constraint applies only to the legacy server boot data. The MySQL→PostgreSQL migration must adapt types/defaults/`ENUM`/`SET`/`UNSIGNED`/invalid dates per ADR-0001 negative consequences.
- **Why:** the legacy stack is CP949 double-encoded in places (the mob-name mojibake came from CP949→latin-1→UTF-8 double encoding); the rewrite must not inherit it.
- **Evidence:** [ADR-0001](../decisions/0001-postgresql-without-timescaledb-by-default.md) (negative consequences); `../plans/server-rewrite.md` §9 (CP949 reverted to UTF-8); AGENTS.md fix #16.
- **Consequence:** mojibake in the new server; migration tooling that silently mangles data.
- **Status:** Active (G-PG deliverable).

## 5. Coordinates: units vs cells

- **Rule:** `player.x/y` in the DB are **UNITS** (village c1 of map 41 = `969600, 278400`). The `AddGotoInfo` boot values (e.g. `(9696, 2784)`) are **cells (÷100)** — never write them directly into the DB.
- **Why:** characters saved with garbage coordinates crashed the CLIENT with `0xc0000374` (heap corruption) during map load; `GetValidLocation` falls back to `EMPIRE_START_*(empire)` on failure.
- **Evidence:** AGENTS.md "COORDINATE CONVENTION (CRITICAL)"; [`world-entry-crash.md`](world-entry-crash.md); `../../CHANGELOG.md` 2026-08-09 (fix `UPDATE player SET x=969600, y=278400`).
- **Consequence:** client heap-corruption crash on world entry; lost characters.
- **Status:** Active.

## 6. Verify byte-exactness with `od`/hex, not text diff

- **Rule:** when a layer consumes BINARY bytes (blobs, bytea, wire payloads), verify byte-exactness with `od -An -tx1` (or a hex dump), NOT with a text diff. PG's text output (`\x…` for bytea) and the MySQL wire can re-encode; a text diff can pass while the bytes differ (and vice versa). Never feed PG text forms into binary layers.
- **Why:** the world-entry crash (2026-08-10, 0xC0000374 heap corruption) came from binary data corrupted in a text layer; the E2E catches this class with raw byte comparison.
- **Evidence:** `e2e_db.sh` Q2/Q4/Q5 (`skill_level bytea raw identical` — `od -An -tx1` straight from the wire); [`world-entry-crash.md`](world-entry-crash.md); AGENTS.md "bytea raw" convention.
- **Consequence:** silently corrupted blobs (skill_level/quickslot) that crash the client at map load.
- **Status:** Active.

Related: [`world-entry-crash.md`](world-entry-crash.md), [`legacy-compatibility.md`](legacy-compatibility.md).
