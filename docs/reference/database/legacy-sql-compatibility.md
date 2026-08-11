---
Type: Reference
Status: Accepted
Audience: Contributors
Last verified: 2026-08-11
---

# Legacy SQL compatibility inventory (G-PG, job 2)

> **Status note:** Accepted (2026-08-10, with [ADR-0005](../../decisions/0005-postgresql-cutover-and-legacy-adapter.md)). This is the SQL-compatibility inventory for the G-PG cutover. It records how the C++ baseline (`libsql` + `db` + `game`) reaches MySQL, which MySQL-specific SQL the temporary adapter must translate, and what must be preserved so the legacy client behavior is unchanged. The open decisions of [§9](#9-open-decisions-resolved-2026-08-10) are **resolved**; the §4 translation map is the adapter's unit-test table (spec: `../../plans/server-rewrite.md` §8.2.1c).

## 1. Purpose and scope

G-PG ([ROADMAP §G-PG](../../../ROADMAP.md)) makes PostgreSQL 18 the single canonical store. The C++ baseline keeps running unchanged on that same PostgreSQL through a temporary legacy compatibility adapter that translates its MySQL-speaking `libsql` layer (ADR-0005). This document inventories, with `file:line` evidence:

1. every SQL access point and how it funnels through `libsql`;
2. MySQL-specific query constructs and their PostgreSQL conversion categories;
3. connection/pool/reconnect semantics and the `account`/`common`/`player`/`log` slots;
4. the `mysql5_password` hash and how it is preserved in PostgreSQL without a new server-side library;
5. what the temporary C++→PostgreSQL adapter changes and what disappears afterwards;
6. CP949 encoding risks and item/drop-name hazards.

Out of scope: the Rust `database` crate design (G-PG/F3), the final schema mapping (G-PG), the adapter wire boundary (G-PG) — see ADR-0005 "Not decided in this ADR".

## 2. SQL architecture map

### 2.1 libsql — the single choke point

All SQL in both binaries passes through `CAsyncSQL` (`source/server/libsql/AsyncSQL.h`, `AsyncSQL.cpp`). There is no other database path.

- Three public submission entry points: `AsyncQuery` (fire-and-forget), `ReturnQuery` (callback result), `DirectQuery` (synchronous) — `AsyncSQL.h:128-130`.
- One `MYSQL` connection per `CAsyncSQL` instance (`m_hDB`, `AsyncSQL.h:168`); a `CAsyncSQL` either runs its own child thread (`bNoThread=false`) or is used synchronously (`bNoThread=true`) — `AsyncSQL.cpp:173-209`.
- The child thread drains the query queue, executes with `mysql_real_query`, and pushes results back (`ChildLoop`, `AsyncSQL.cpp:510-650`); callers pop results (`PopResult`, `AsyncSQL.cpp:295-309`).
- `SQLMsg::Store` loops `mysql_next_result` to collect every result set of a statement batch — `AsyncSQL.h:59-80` (defined in the header, not in `AsyncSQL.cpp`). This is the multi-result contract the adapter must keep satisfying.
- `CLIENT_MULTI_STATEMENTS` is requested on connect and reconnect — `AsyncSQL.cpp:129`, `AsyncSQL.cpp:714`.
- Results carry `uiNumRows` / `uiAffectedRows` / `uiInsertID` (`AsyncSQL.h:37-39`, populated in `SQLMsg::Store` at `AsyncSQL.h:65-67`) — the adapter must reproduce these three numbers.
- Escaping goes through `mysql_real_escape_string` on the connection handle — `AsyncSQL.cpp:667-692`.
- `CStmt` (prepared statements, `Statement.cpp:40-158`) exists but has **zero call sites** in `db` or `game` (verified by grep 2026-08-10); the adapter does not need the prepared-statement API in practice.
- Connection charset is negotiated per connection via `mysql_set_character_set` / `MYSQL_SET_CHARSET_NAME` — `AsyncSQL.cpp:90-113`, `AsyncSQL.cpp:124` (see [§8](#8-encoding-risks-cp949-and-itemdrop-names)).

### 2.2 db binary: slots, three connections per slot

`CDBManager` (`source/server/db/src/DBManager.h`) owns the slots:

- Slot enum `SQL_PLAYER`, `SQL_ACCOUNT`, `SQL_COMMON`, `SQL_LOG` (under `ENABLE_DB_SQL_LOG`) — `DBManager.h:20-29`.
- Per slot, **three** `CAsyncSQL2` connections: `m_directSQL` (sync, no thread), `m_mainSQL` (async thread, return queries), `m_asyncSQL` (async thread, fire-and-forget) — `DBManager.cpp:79-112`, dispatched at `DBManager.cpp:114-143`.
- `DirectQuery` → `m_directSQL`, `ReturnQuery` → `m_mainSQL`, `AsyncQuery` → `m_asyncSQL` — `DBManager.cpp:114-143`.
- Slot wiring from `conf.txt`: `SQL_PLAYER`, `SQL_ACCOUNT`, `SQL_COMMON`, `SQL_LOG` (compile-gated) with 5 retries each — `source/server/db/src/Main.cpp:244-354`.
- Escaping uses the **direct** connection handle regardless of which connection will run the query — `DBManager.cpp:145-149` (charset must therefore be consistent across the three connections of a slot).
- Locale/`g_stLocale` is applied to all slots via `SetLocale`/`QueryLocaleSet` — `DBManager.cpp:151-172`; default locale `latin1` — `Main.cpp:42`.

Slot usage (verified in the login flow, AGENTS.md): `SQL_ACCOUNT` carries `account` schema queries (`QUERY_LOGIN` — `ClientManagerLogin.cpp:410-423`), `SQL_COMMON` carries `priv_settings`/`locale` (`ClientManager.cpp:112-115`, `ClientManager.cpp:3080`), `SQL_PLAYER` carries everything else. The default slot is `SQL_PLAYER` (`DBManager.h:46-48`).

### 2.3 game binary: three managers, three databases

The game binary does **not** use `CDBManager`; it has three independent `CAsyncSQL` pairs (config tokens `player_sql`, `common_sql`, `log_sql` — `source/server/game/src/config.cpp:368-437`, connected at `config.cpp:465-529`):

- `DBManager` — player SQL: `m_sql` (async thread) + `m_sql_direct` (sync) — `source/server/game/src/db.h:89-90`, `db.cpp:28-37`; `Query`/`DirectQuery`/`ReturnQuery` wrappers at `db.cpp:39-86`; result dispatch `Process`/`AnalyzeReturnQuery` at `db.cpp:98-132`, `db.cpp:223-444`.
- `AccountDB` — common SQL: `m_sql_direct` + `m_sql` — `db.h:153-154`, `db.cpp:475-527`; used for the `locale` table (`config.cpp:477-499`), `spam_db` (`db.cpp:575-590`), `exp_table` (`config.cpp:1389`).
- `LogManager` — log SQL: single `CAsyncSQL m_sql` — `source/server/game/src/log.h:84`, `log.cpp:20-26`; all log inserts are fire-and-forget `Query` → `m_sql.AsyncQuery` — `log.cpp:28-43`.

Note: the game **auth** binary queries the `account` table through its `DBManager` (player slot): `UPDATE account SET lang=...` — `input_auth.cpp:144-145`; `QID_AUTH_LOGIN ... FROM account WHERE login='%s'` — `input_auth.cpp:174-218`. The per-connection schema mapping of the adapter (or the migration config) must give that connection access to the account schema; this is a per-binary, config-driven concern (search_path per connection).

### 2.4 Submission counts (regex-verified 2026-08-10)

Methodology: pattern `(?<![\w:])<Method>(` over all `*.cpp` of each binary — excludes declarations/definitions whose signature is preceded by `::` (`CDBManager::DirectQuery`, `DBManager::ReturnQuery`, …) and the `Count*Query`/`CountResult` counters; includes the wrapper-internal dispatches (`DBManager.cpp:116,132,141`; `db.cpp:60,85,526,575,582`). For `game`, the fire-and-forget `Query()` column counts the 13 `DBManager::instance().Query(...)` call sites only: the `LogManager::Query` wrapper (definition at `log.cpp:28` plus 24 internal calls, all funneling to `m_sql.AsyncQuery`) and the `DBManager::Query` definition (`db.cpp:39`) are excluded.

| Binary | DirectQuery | ReturnQuery | AsyncQuery | Query() fire-and-forget | FuncQuery / FuncAfterQuery | Total sites |
|---|---|---|---|---|---|---|
| `db` (`source/server/db/src`) | 69 | 37 | 43 | — | — | 149 |
| `game` (`source/server/game/src`) | 24 | 9 | — | 13 | 6 / 3 | 55 |

Total: **204** submission sites (149 + 55).

Plus ≈30 `EscapeString` call sites in `game` (guild/messenger/input/log paths, e.g. `guild.cpp:650`, `input_auth.cpp:136-167`, `messenger_manager.cpp:52`) and 4 in `db` (`ClientManagerPlayer.cpp:171,174,886,891`).

## 3. MySQL-specific constructs inventory

Grep-derived counts over `*.cpp` of both binaries (2026-08-10; counts may include comments/macros).

| # | Category | Count | Representative sites |
|---|---|---|---|
| 1 | Backtick identifiers | 37 | `` `window` `` `ClientManagerPlayer.cpp:321-322,385-386,1116`; `Cache.cpp:56` (the `ON DUPLICATE` query at `Cache.cpp:82` reuses that `setQuery` string); `ClientManager.cpp:680,688,891,907,1425`; `` `rank` `` `ClientManagerBoot.cpp:1081,1133,1290`; `` `where` ``/`` `when` `` (chat_log) `log.cpp:327` |
| 2 | `UNIX_TIMESTAMP(x)` | 29 | `ClientManager.cpp:115`; `GuildManager.cpp:881-882`; `ClientManagerPlayer.cpp:370`; `input_auth.cpp:176-183,196-203,209-216` |
| 3 | `NOW()` as value/comparison | 41 | log inserts `log.cpp:60,90,106,119,127,145,196,204,212,222,231,240,253,260,267,281,299,313,328,339,345` (`log.cpp:320` is `FROM_UNIXTIME`, not `NOW()`; `log.cpp:240` is the `shout_log` insert); `ItemAwardManager.cpp:167`; `char_change_empire.cpp:161`; `guild.cpp:1014`; `db.cpp:378`; `ClientManager.cpp:112,3946,3970`; `ClientManagerPlayer.cpp:108,370`; `GuildManager.cpp:1131,1140` (`item_award` inserts) |
| 4 | `NOW()` arithmetic (`availDt - NOW() > 0`) | 3 | `input_auth.cpp:175,195,208` |
| 5 | `DATE_ADD(NOW(), INTERVAL n SECOND)` | 2 | `ClientManager.cpp:193`; `GuildManager.cpp:1043` |
| 6 | `REPLACE INTO` / `REPLACE tbl` | 16 | case-insensitive count: 12 uppercase sites `Monarch.cpp:230`; `ClientManagerHorseName.cpp:8`; `ClientManagerGuild.cpp:74,111`; `ClientManagerEventFlag.cpp:52`; `ClientManager.cpp:193,584`; `ClientManagerPlayer.cpp:1151`; `Cache.cpp:189`; `input_main.cpp:2541`; `guild.cpp:245`; `log.cpp:253` — plus 4 lowercase boot queries `replace into` `ClientManagerBoot.cpp:1079,1131,1195,1225` (mob/item proto reload) |
| 7 | `INSERT IGNORE` | 0 | not used in server code |
| 8 | `ON DUPLICATE KEY UPDATE` | 2 | `Cache.cpp:82`; `ClientManager.cpp:1451` (both `INSERT INTO item SET ... ON DUPLICATE KEY UPDATE ...`) |
| 9 | `INSERT ... SET` / `REPLACE ... SET` syntax | 3 | `Cache.cpp:82`; `ClientManager.cpp:1451`; `ClientManager.cpp:193` |
| 10 | `+0` enum/bit cast trick | 23 | `setFlag+0, setAffectFlag+0` `ClientManagerBoot.cpp:478`; `apply+0` `ClientManagerBoot.cpp:594,719`; `` `window`+0 `` `ClientManagerPlayer.cpp:321,385`, `ClientManager.cpp:680`; `size+0` `ClientManagerBoot.cpp:1290`; `auth+0` `guild.cpp:624`; `immuneflag+0` `ClientManagerBoot.cpp:1467` |
| 11 | Cross-database reference | 1 | `player.player_index` `ClientManagerLogin.cpp:413` |
| 12 | Multi-statement / user variables | 2 | `SET @i = (...)`, `WHERE id=@i` `log.cpp:309-313` (two queries sharing session state) |
| 13 | `TIMEDIFF` | 1 | `log.cpp:313` |
| 14 | `inet_aton` | 1 | `log.cpp:299` |
| 15 | `FROM_UNIXTIME` | 1 | `log.cpp:320` |
| 16 | `SET sql_mode = ''` | 1 | `ClientManagerBoot.cpp:39` (strict-mode off; zero dates) |
| 17 | `CAST(... AS unsigned)` | 1 | `config.cpp:576` |
| 18 | Hash functions (`mysql_hash_password` / `PASSWORD()` / SHA1-based) | 15 | `ClientManagerLogin.cpp:411,414`; `input_auth.cpp:193,195,218`; `utils.cpp:30-63`; `questlua_global.cpp:1657-1662` |
| 19 | `LIMIT` / `COUNT(*)` | 4 / 8 | `LIMIT` (SELECT, portable): `guild.cpp:1045`; `GuildManager.cpp:201` — the other 2 are the non-portable `UPDATE … LIMIT 1` (row 24). `COUNT(*)`: `ClientManagerLogin.cpp:552`; `ClientManagerPlayer.cpp:823,827`; `GuildManager.cpp:960`; `ItemIDRangeManager.cpp:121`; `guild_manager.cpp:90`; `questlua_building.cpp:92`; `questlua_pc.cpp:2128` |
| 20 | ENUM string comparisons | 6 | `` `window` IN ('INVENTORY','EQUIPMENT','DRAGON_SOUL_INVENTORY','BELT_INVENTORY') `` `ClientManagerPlayer.cpp:322,386,1116`; `` `window`='%s' `` `ClientManager.cpp:688`; `enable='YES'` `ClientManagerBoot.cpp:848` |
| 21 | BLOB / binary columns as escaped text | 4 | `questlua_global.cpp:1616-1624` (MYSQL_TYPE_BLOB branch); `guild.cpp:650` (skill blob); `ClientManagerPlayer.cpp:171-175` (`skill_level`, `quickslot` escaped) |
| 22 | Zero dates | 1 | `"00000000"` default `db.cpp:315`; allowed by `SET sql_mode=''` `ClientManagerBoot.cpp:39` |
| 23 | Raw unescaped query from Lua | 1 | `questlua_global.cpp:1588` (`DirectQuery("%s", lua_tostring(L,1))`) |
| 24 | `UPDATE … LIMIT 1` | 2 | `ClientManager.cpp:4072` (`update account set cash = cash + %d where id = %d limit 1`), `ClientManager.cpp:4074` (mileage) — **non-portable**: MySQL-only `UPDATE … LIMIT`; PostgreSQL `UPDATE` has no `LIMIT` clause (§4) |
| 25 | `COLLATE sjis_japanese_ci` | 1 | `ClientManagerPlayer.cpp:823` — character-name uniqueness check (`SELECT COUNT(*) … WHERE name='%s' collate sjis_japanese_ci`), only when `g_stLocale == "sjis"`; MySQL/Japan-specific collation with no PostgreSQL equivalent (§4, §8) |

Portable as-is (no translation needed): plain `SELECT`/`UPDATE`/`DELETE` with numeric predicates, `IN (...)`, `LEFT JOIN` (`ClientManagerBoot.cpp:253-254`), multi-row `INSERT ... VALUES (...),(...)` (`GuildManager.cpp:1327-1330,1438-1441`, `ClientManager.cpp:891-918`), `INSERT ... SELECT` (`ClientManagerGuild.cpp:111`). These are the majority of the 204 submission sites.

## 4. Translation map MySQL → PostgreSQL (adapter scope)

| Category | MySQL text | PostgreSQL translation | Evidence |
|---|---|---|---|
| Backticks | `` `ident` `` | `"ident"` — cannot be dropped blindly: `window`, `where` and `when` are reserved in PG (window functions / WHERE clause / CASE WHEN) | `Cache.cpp:56` (`window`), `log.cpp:327` (`where`, `when`, …) |
| `+0` enum cast | `setFlag+0`, `` `window`+0 ``, `size+0`, `immuneflag+0` (12 columnas del boot) | `col+0` → **índice ENUM 1-based / bitmask SET** según el catálogo estático `ENUM_COLUMNS` del proxy — `source/reforge/mysql_proxy/src/translate.rs:551-592` (fuente: `SHOW CREATE` MariaDB, 2026-08-11; regla completa: `translate.rs:516-525`). Necesario porque las columnas son **text sin CHECK** en PG (legacy-schema.md §7.2) y el C++ lee el número (`str_to_number` en el boot). `+0` de columnas **no** catalogadas → se elimina (fallback). El caso inverso (C++ escribe el índice → literal, `item.window`) lo cubre `fix_enum_value` (`translate.rs:489-513`) | `ClientManagerBoot.cpp:478,1290-1291,1467`; `ClientManager.cpp:680`; `ClientManagerPlayer.cpp:321,385` |
| `UNIX_TIMESTAMP(x)` | seconds since epoch | `EXTRACT(EPOCH FROM x)` (numeric; same text protocol value) | `ClientManager.cpp:115` |
| `UNIX_TIMESTAMP(NOW())-UNIX_TIMESTAMP(last_play)` | seconds diff | `EXTRACT(EPOCH FROM now()) - EXTRACT(EPOCH FROM last_play)` (or `EXTRACT(EPOCH FROM now() - last_play)`) | `ClientManagerPlayer.cpp:370` |
| `NOW()` | current datetime | `now()` | `log.cpp:60` |
| `availDt - NOW() > 0` | datetime - now > 0 | `availDt > now()` (rewrite; `timestamp - timestamp = interval`, cannot compare to `0`) | `input_auth.cpp:175,195,208` |
| `DATE_ADD(NOW(), INTERVAL n SECOND)` | now + n seconds | `now() + make_interval(secs => n)` | `ClientManager.cpp:193`, `GuildManager.cpp:1043` |
| `REPLACE INTO t (...)` | delete+insert (new auto-inc id) | `INSERT ... ON CONFLICT (...) DO UPDATE SET ...` — needs a table metadata map (conflict target + column list) in the adapter. Semantics differ (no delete+reinsert) but no code relies on REPLACE id churn; no triggers exist in the schema. **Proposed** | `Monarch.cpp:230`, `ClientManagerHorseName.cpp:8`, `Cache.cpp:189` |
| `INSERT INTO t SET c=v, ... ON DUPLICATE KEY UPDATE ...` | upsert | `INSERT INTO t (cols) VALUES (...) ON CONFLICT (id) DO UPDATE SET ...` — item PK is `id` | `Cache.cpp:82`, `ClientManager.cpp:1451` |
| `INSERT INTO t SET ...` / `REPLACE INTO t SET ...` | assignment syntax | expand to column list + `VALUES` | `ClientManager.cpp:193` |
| `SET sql_mode = ''` | strict mode off | no-op in PG (drop in shim); zero-date acceptance handled at schema level (§9 OD-5) | `ClientManagerBoot.cpp:39` |
| `SET @i = (...)` / `WHERE id=@i` | session user variable | pin both queries to the same PG session; emulate `@i` via temp table or `SET LOCAL` custom GUC. **Proposed** | `log.cpp:309-313` |
| `inet_aton('%s')` | IPv4 → uint | `'%s'::inet` (schema: `loginlog2.ip` migrates to `inet`) | `log.cpp:299` |
| `TIMEDIFF(logout_time, login_time)` | time diff | `(logout_time - login_time)` (interval) or `EXTRACT(EPOCH FROM ...)` | `log.cpp:313` |
| `FROM_UNIXTIME(%d)` | epoch → datetime | `to_timestamp(%d)` — TZ-sensitive; fix session `TimeZone` (see OD-7) | `log.cpp:320` |
| `CAST(mValue AS unsigned)` | unsigned cast | `mValue::bigint` | `config.cpp:576` |
| `%s` string literals with `mysql_real_escape_string` | backslash+quote escaping | keep MySQL escaping semantics: adapter sessions run with `standard_conforming_strings = off` so backslash escapes behave as MySQL (`NO_BACKSLASH_ESCAPES` is off in the current deployment). **Proposed** | `AsyncSQL.cpp:691`, `DBManager.cpp:148`, `db.cpp:457-459` |
| `mysql_insert_id` | last auto-inc id | `lastval()` / `RETURNING id`; the shim must fill `SQLResult::uiInsertID` (`AsyncSQL.h:39`) | `GuildManager.cpp:1048-1054` |
| `mysql_affected_rows` | affected rows | PG command-tag row count (UPDATE = matched rows differs from MySQL default; MySQL 5.6 default = changed rows — verify `CLIENT_FOUND_ROWS`-like parity). **Proposed** | `AsyncSQL.h:67` (populated in `SQLMsg::Store`); consumed via `uiAffectedRows` at `ClientManager.cpp:922,1966`; `ClientManagerPlayer.cpp:898,1057,1105`; `GuildManager.cpp:1048,1269`; `Marriage.cpp:106,145,190,233`; `guild.cpp:1027`; `char_change_empire.cpp:111`; `questlua_global.cpp:1592-1598` |
| `player.player_index` | cross-database join | PG schema-qualified name is valid verbatim (`player` schema + `player_index` table); per-slot `search_path` covers the unqualified side | `ClientManagerLogin.cpp:413` |
| Multi-result `Store()` loop | one or more result sets | the shim returns exactly one result set per statement (no multi-statement strings exist in the codebase; the loop is defensive) | `AsyncSQL.h:59-80` |
| BLOB column text protocol | raw bytes in row | PG text protocol renders `bytea` as `\x...` — the Lua BLOB branch (`questlua_global.cpp:1616-1624`) and the escaped-binary columns (`ClientManagerPlayer.cpp:171-175`) need a byte-preserving path (binary decode in shim). **Proposed** | `questlua_global.cpp:1616-1624` |
| `UNSIGNED` schema types | unsigned ints | `bigint`/`numeric` + `CHECK (col >= 0)` at migration (ADR-0001 negative consequences) | schema mapping (G-PG) |
| `ENUM` schema types | enum columns (`window`, `rank`, ...) | text + CHECK or PG enum; the `+0` and `IN ('...')` usages keep working with text. **Proposed** | `ClientManagerPlayer.cpp:322` |
| Zero dates | `'0000-00-00'` | PG rejects; migrate to `NULL` / `1000-01-01` / text. **Proposed** | `db.cpp:315`, `ClientManagerBoot.cpp:39` |
| `UPDATE t … LIMIT 1` | update + MySQL-only LIMIT | drop `LIMIT 1`: both sites update `account` by primary key `id` (`account.id` matches at most one row), so MySQL's `LIMIT 1` is redundant — `UPDATE account SET cash = cash + %d WHERE id = %d`. (PG `UPDATE` has no `LIMIT` clause; the `WHERE` is already unique.) | `ClientManager.cpp:4072,4074` |
| `… collate sjis_japanese_ci` | collation in predicate | no PG equivalent; drop the `COLLATE` clause (the check is an exact `name` equality feeding a `COUNT(*)` existence test — byte equality suffices, and the §8 byte round-trip rule must be respected) or map to a locale-aware PG collation only with a verified equivalence. **Proposed** | `ClientManagerPlayer.cpp:823` |

## 5. Connections, pools and reconnect semantics

- **No connection pool anywhere.** One `CAsyncSQL` = one `MYSQL` connection; the db binary runs up to 12 (4 slots × 3), the game binary 5 (DBManager 2 + AccountDB 2 + LogManager 1). The adapter preserves this 1:1 model (one PG session per MySQL session) — session-scoped state (`@i`, charset, `sql_mode`) depends on it.
- `MYSQL_OPT_RECONNECT` is enabled before connect — `AsyncSQL.cpp:126-127`. After connect the effective value is read back with `mysql_get_option` on MariaDB builds (`AsyncSQL.cpp:135-138`, `#ifdef MARIADB_BASE_VERSION`) or directly from the `MYSQL` struct field `m_hDB.reconnect` on MySQL builds (`AsyncSQL.cpp:140`) — both access paths must exist in the shim.
- Reconnect detection: the code compares `mysql_thread_id` before each batch and re-applies the charset (`QueryLocaleSet`) after reconnecting — `AsyncSQL.cpp:233-238` (DirectQuery), `AsyncSQL.cpp:534-539` (ChildLoop), plus `AsyncSQL.cpp:598-602` (flush loop) and `AsyncSQL.cpp:729-733` (after explicit reconnect). The shim must return a stable-enough session id and re-apply the charset setting on reconnect.
- Retry loop: on `CR_SERVER_LOST` and a fixed list of connection/table errors the query is retried (`ResendQuery` + `continue`) — `AsyncSQL.cpp:548-571`, `AsyncSQL.cpp:700-738`. `ResendQuery` reconnects explicitly: `mariadb_reconnect` on MariaDB builds (`AsyncSQL.cpp:702-707`) or a fresh `mysql_real_connect` with `CLIENT_MULTI_STATEMENTS` on MySQL builds (`AsyncSQL.cpp:711-726`). The shim maps libpq connection failures onto these errno constants so the retry behavior is preserved, and must expose the MariaDB-only `mariadb_reconnect` entry point under `MARIADB_BASE_VERSION`.
- Result delivery: async results are pushed to a mutex-guarded queue and drained by the caller (`PopResult`) — `AsyncSQL.cpp:286-309`; the game drains in `Process()` (`db.cpp:98-132`) and the db binary in its main loop.
- `EscapeString` is executed on the **direct** connection handle (`DBManager.cpp:145-149`) — charset consistency between the direct and threaded connections of a slot is a precondition (see §8).

## 6. Password hashing: preserving `mysql5_password` in PostgreSQL

- The format is the MySQL native password: `"*" + UPPER(hex(SHA1(binary SHA1(pw))))` — 41 chars, **the asterisk is part of the format** — `source/server/game/src/utils.cpp:30-58` (CryptoPP implementation).
- The stored value in `account.password` uses this format and is compared with `strcmp` — `db.cpp:340`, `ClientManagerLogin.cpp:288`.
- Two verification paths must both keep working:
  - game auth: the hash is computed **in C++** (`mysql_hash_password`, `input_auth.cpp:218`) and embedded as a literal; the Windows build inlines the same expression via `_MYSQL_NATIVE_PASSWORD` (`input_auth.cpp:193`).
  - db channel: the hash is computed **by the database** via the SQL function `mysql_hash_password(pw)` = `CONCAT('*', UPPER(SHA1(UNHEX(SHA1(pw)))))` (created in MariaDB 2026-08-08, AGENTS.md §11) — `ClientManagerLogin.cpp:411,414`.
- PostgreSQL preservation **without introducing a library into the server**:
  - Recreate the SQL function on the PG side using **pgcrypto** (a standard PostgreSQL contrib module, ships with PG 18, no third-party dependency):
    `'*' || upper(encode(digest(digest(pw::bytea, 'sha1'), 'sha1'), 'hex'))`
    — this is exactly the MariaDB function body expressed in PG. The C++ side (`input_auth.cpp:218`) is untouched because it never calls the DB function.
  - Alternative (also library-free): the adapter computes the hash itself and rewrites the `mysql_hash_password(...)` call — the game binary already links CryptoPP (`utils.cpp:25-28`), but the **db** binary does not, so the SQL-function option keeps both binaries symmetric. **Proposed: OD-2.**
- Constraints:
  - Never use PG `crypt()`/`md5()` — the stored format (leading `*`, 41 chars) must not change; only a string equality is performed (`strcmp`, `db.cpp:340`).
  - The `lang` column flow (`QUERY_LOGIN` 13-column layout, `ClientManagerLogin.cpp:405-414`) is orthogonal — it must keep its column order in the translated query.
  - The hash function is needed only while the C++ baseline runs; the Rust auth computes the hash in Rust (ROADMAP F2a). The stored hashes remain valid after the C++ baseline retires — the format is a property of the data, not of the adapter.

## 7. The temporary C++→PostgreSQL adapter: what changes, what disappears

### What changes (temporary, link-time only — C++ source untouched, ADR-0003/0005)

1. **Link boundary:** both binaries link `-lmariadb` (`source/server/game/src/Makefile:61-66`, `source/server/db/src/Makefile:52-57`). The adapter is a shim library exposing the mysql C API subset that libsql actually uses, backed by libpq. Headers stay `<mysql/mysql.h>`; no C++ recompilation.
2. **API subset to implement** (from `AsyncSQL.h`/`AsyncSQL.cpp`/`Statement.cpp`): `mysql_init`, `mysql_options` (`MYSQL_SET_CHARSET_NAME`, `MYSQL_OPT_RECONNECT`), `mysql_real_connect`, `mysql_real_query`, `mysql_store_result`, `mysql_fetch_row`, `mysql_num_rows`, `mysql_affected_rows`, `mysql_insert_id`, `mysql_next_result`, `mysql_errno`, `mysql_error`, `mysql_close`, `mysql_set_character_set`, `mysql_thread_id`, `mysql_real_escape_string`, `mysql_free_result`; plus `mysql_fetch_field`/`mysql_field_seek` for the Lua bridge (`questlua_global.cpp:1604-1631` — `mysql_num_fields` is **not** used there), `mysql_get_option` (`AsyncSQL.cpp:137`), and the MariaDB-only `mariadb_reconnect` (`AsyncSQL.cpp:703`); the MySQL-build branch reads the `MYSQL` struct field `reconnect` directly (`AsyncSQL.cpp:140`), so the shim's `MYSQL` struct must expose it. The `mysql_stmt_*` family is **not** needed (CStmt has zero call sites).
3. **Result fidelity:** `MYSQL_ROW` = array of `char*` (text protocol); NULL cells; field metadata (`name`, `type`, `NOT_NULL_FLAG`, `IS_NUM`, `MYSQL_TYPE_BLOB`) — `questlua_global.cpp:1604-1631` depends on them.
4. **SQL text translation** per §4 (backticks, `+0`, `UNIX_TIMESTAMP`, `DATE_ADD`, `REPLACE INTO`, `ON DUPLICATE KEY UPDATE`, `INSERT ... SET`, `@i`, `inet_aton`, `TIMEDIFF`, `FROM_UNIXTIME`, `sql_mode`, escaping).
5. **Session mapping:** one PG session per MySQL connection; charset negotiation mapped to `client_encoding`/pass-through (§8); `mysql_thread_id` → PG session id for the reconnect detection at `AsyncSQL.cpp:534-539`; retry errno mapping for `AsyncSQL.cpp:548-571`.
6. **Config surface:** the db binary's `SQL_*` entries (`Main.cpp:244-354`) and the game's `player_sql`/`common_sql`/`log_sql` (`config.cpp:368-437`) keep their format; only the resolved backend changes (host/port → PG). Per-connection `search_path` maps slot → PG schema (account/common/player/log per ADR-0005 domain schemas), including the auth binary's account access through its player connection (§2.3).

### What disappears afterwards (F6, with the C++ baseline)

- The shim library and the whole mysql C API compat surface; the `-lmariadb` linkage.
- The SQL translation layer (§4) — the PG schema uses proper types (int columns instead of ENUM, `inet`, timestamps), so `+0`, backticks, `sql_mode`, `INSERT ... SET` rewrites vanish with the last C++ caller.
- The `SET sql_mode = ''` call (`ClientManagerBoot.cpp:39`) — it is a no-op in the shim and dies with the db binary.
- The PG `mysql_hash_password` function — only the C++ `QUERY_LOGIN` calls it; the Rust auth verifies in Rust (ROADMAP F2a). The stored hashes stay (format unchanged).
- Nothing of the adapter survives into the Rust server: ADR-0001 ("no MySQL API patterns in the new server") and ADR-0005 ("thin, explicit, removed at F6") are the contract. The Rust `database` crate is written against PostgreSQL from the start (G-PG/F3).

## 8. Encoding risks: CP949 and item/drop names

- **Per-connection charset negotiation** happens in libsql: `mysql_set_character_set` (`AsyncSQL.cpp:104-112`) and `MYSQL_SET_CHARSET_NAME` (`AsyncSQL.cpp:124`), re-applied after every reconnect (`AsyncSQL.cpp:534-539`, `AsyncSQL.cpp:728-734`). The runtime locale comes from config (`LOCALE`, `Main.cpp:185-189`; game `SetLocale` at `db.cpp:494-498`, `config.cpp:503`). The adapter must answer charset negotiation without transcoding (pass-through) or with an explicit, reversible mapping — **Proposed: OD-6**.
- **The item/drop-name trap (AGENTS.md §17):** the server's boot drop files (`etc_drop_item.txt`, `common_drop_item.txt`, `drop_item_group.txt`) reference items **by name in CP949**; `ReadEtcDropItemFile` (`source/server/game/src/item_manager_read_tables.cpp:457-498`) resolves them via `GetValidVnum`/`GetVnumByOriginalName` against `item_proto.name`. If migration or the adapter transcodes `item_proto.name`, the core aborts boot with `No such an item (name: ...)`. **Rule: `item_proto.name` must round-trip byte-identically through the adapter and the migration.**
- `mob_proto.locale_name` is loaded at boot (`ClientManagerBoot.cpp:1286-1312`) and sent to clients as NPC names (`GC_CHAR_ADDITIONAL_INFO`, `char.cpp:922-948`); the client falls back to its pack, but byte-preservation is still required for the fallback path.
- **`collate sjis_japanese_ci` hazard:** the character-name uniqueness check appends the MySQL/Japan-specific collation `collate sjis_japanese_ci` when the locale is `sjis` — `ClientManagerPlayer.cpp:821-827`. PostgreSQL has no such collation; the adapter must drop the clause (the check is an exact `name` equality feeding a `COUNT(*)` existence test, so byte equality suffices) or prove an equivalent collation mapping — and must not transcode the `name` column while doing so (§4, row 25).
- PostgreSQL options: (a) database encoding **EUC_KR** (KS X 1001 — covers the bulk of CP949 Hangul/hanja, but not the full CP949 superset); (b) **bytea** columns for the byte-sensitive name columns with the adapter passing bytes through untouched (exact-match lookups run in C++ memory, not SQL, so bytea is safe); (c) UTF-8 with transcoding at the adapter — risky, violates the byte round-trip rule. **Proposed: OD-6.**
- Related but not SQL: the server's Lua lexer is EUC-KR (2 bytes/char; AGENTS.md §15) — locale lua files with Korean must stay CP949/EUC-KR; the `locale` table rows read via SQL (`config.cpp:477-499`) fall under the same byte-preservation rule if they contain Korean.

## 9. Open decisions (resolved 2026-08-10)

| # | Decision | Options | Resolution (2026-08-10, ADR-0005 Accepted) |
|---|---|---|---|
| OD-1 | Adapter mechanism | (a) link-time shim implementing the mysql C API subset over libpq; (b) protocol-level proxy (MySQL-wire server in front of PG) | **(b)** — wire-level MySQL protocol v10 proxy, `source/reforge/mysql_proxy` (Rust, tokio + tokio-postgres), `127.0.0.1:3307`; zero C++ source/linkage change (runtime conf.txt only). (a) remains documented as the fallback — no known insurmountable wire issue |
| OD-2 | Hash computation | (a) PG SQL function with pgcrypto; (b) adapter-side computation | **(a)** — `account.mysql_hash_password(text)` = `'*' || upper(encode(digest(decode(digest($1::bytea,'sha1'),'hex'),'sha1'),'hex'))`; symmetric for both binaries, zero C++ change. Stored hashes copied verbatim |
| OD-3 | `REPLACE INTO` / `ON DUPLICATE KEY UPDATE` translation | (a) table metadata map in the adapter; (b) schema-side triggers/rules | **(a)** — PK introspected from pg_catalog per table and cached; `ON CONFLICT (pk) DO UPDATE SET` with bare names = existing row (MySQL semantics). 18 affected sites (§3 rows 6, 8, 9) |
| OD-4 | `@i` user-variable emulation | (a) session temp table; (b) `SET LOCAL` custom GUC | **(a)** — `SET @name = (subquery)` → `CREATE TEMP TABLE pg_temp.m2var_<name> AS SELECT …`; `@name` references → `(SELECT v FROM m2var_<name>)`; 1 call pair (`log.cpp:309-313`, two separate queries) |
| OD-5 | Zero dates | (a) `NULL`; (b) `1000-01-01`; (c) text columns | **(a)** — `'00000000'` only appears as a defensive default (`db.cpp:315`); columns verified at migration |
| OD-6 | Charset strategy | (a) EUC_KR database; (b) UTF-8 + bytea for CP949 name columns; (c) UTF-8 + transcoding | **(b)** — PG db UTF8; `item_proto.name`/`locale_name`, `mob_proto.locale_name`, `skill_proto.szName` → `bytea`, byte-exact round-trip; adapter answers `SET NAMES` (latin1/cp949) as pass-through, no transcoding |
| OD-7 | Session TimeZone | UTC vs server-local | **server-local** (matches current MySQL session TZ; `create_time` rendered via `localtime()` at `db.cpp:330-333`); adapter sets `TimeZone` per session |
| OD-8 | `mysql_affected_rows` parity | matched-rows vs changed-rows | **PG command-tag (matched) rows**; phase-1 consumers verified not to branch on changed-vs-matched; the §4 consumer list (9 sites) is audited per phase as it becomes reachable (F3+) |

## 10. Related documents

- [ADR-0005 — PostgreSQL cutover and temporary legacy compatibility adapter](../../decisions/0005-postgresql-cutover-and-legacy-adapter.md) (**Accepted**; this inventory feeds its translation table — G-PG spec `../../plans/server-rewrite.md` §8.2.1)
- [ADR-0001 — PostgreSQL as the primary database, no TimescaleDB by default](../../decisions/0001-postgresql-without-timescaledb-by-default.md) (Accepted; "no MySQL API patterns in the new server")
- [ROADMAP — Phase G-PG](../../../ROADMAP.md) (blocking F2)
- [Reference hub](../README.md)

## Appendix A. Count summary (2026-08-10)

Query submission sites: **204** total — `db` 149 (69 DirectQuery / 37 ReturnQuery / 43 AsyncQuery), `game` 55 (24 DirectQuery / 9 ReturnQuery / 13 fire-and-forget Query / 6 FuncQuery / 3 FuncAfterQuery). Non-portable constructs by category: backticks 37, `UNIX_TIMESTAMP` 29, `NOW()` 41 (3 with arithmetic), `DATE_ADD` 2, `REPLACE INTO` 16 (incl. 4 lowercase boot queries), `ON DUPLICATE KEY UPDATE` 2, `INSERT/REPLACE ... SET` 3, `UPDATE … LIMIT 1` 2 (non-portable), `collate sjis_japanese_ci` 1, `LIMIT` (SELECT) 4, `COUNT(*)` 8, `+0` casts 23, `@i` 2, `TIMEDIFF` 1, `inet_aton` 1, `FROM_UNIXTIME` 1, `SET sql_mode` 1, `CAST(... AS unsigned)` 1, ENUM string comparisons 6, BLOB-as-text 4, zero dates 1, Lua raw query 1, cross-schema reference 1. `INSERT IGNORE`: 0. Prepared statements (`CStmt`): 0 call sites.
