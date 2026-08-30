---
Type: Reference
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-08-30
---

# reforge-core Agent Instructions

## Mission

**The compatible server will be completely rewritten in Rust with modern technologies.** This is the project's main goal.

- **Motto: do more with less.** Less code, less complexity, fewer dependencies — more clarity, more robustness, more performance.
- **Strategy: incremental module-by-module replacement.** The legacy C++ server is replaced piece by piece with verifiable Rust modules. Each module must preserve observable behavior and pass verification before moving on.
- The rewrite is a structural redesign, not a line-by-line translation. Architecture decisions (domain boundaries, data ownership, protocols, concurrency, failures, migration) are recorded as ADRs before implementation.

## Public repository boundary (ADR-0015)

- The public repository contains the authored Rust server in `source/reforge`
  and the documentation, scripts, and supporting metadata needed to develop and
  verify it.
- Client source, pack source or assets, generated client binaries, and frozen
  C++ oracle source are not public repository contents. Do not add them back to
  the index or describe them as build inputs for this checkout.
- Real-client checks use an external, operator-provided compatible client. Its
  installation and assets are local prerequisites and are not distributed here.
- F7âthe standalone Rust clientâis deferred outside this repository. ADR-0015
  supersedes ADR-0013; do not create a `source/client_rust` workspace in this
  repository.

## Current phase (2026-08-30)

The current state of the project lives in two files, read at the start of every
session and updated at the close:

- `documentation/progress.md` — live handoff (HEAD, deploy, gate result, open rows).
- `documentation/plans/gap-registry.md` — per-row tracker (owner, evidence, state).

The dated wave notes and the `Historical` section below are kept for context only;
do not edit them as if they were current. New work lives in the registry, the
changelog, and the ADRs. If the handoff or the registry does not match your
understanding, that is a bug — update one or the other before you write code.

**Runtime hosts (ADR-0012, ADR-0015):**

- **Primary (daily):** native Windows, PostgreSQL 18.4 on `127.0.0.1:5432`
  (service `postgresql-metin2`, role `mt2`), Rust auth on `127.0.0.1:30001`,
  Rust channel on `127.0.0.1:30003`. Start/stop: `scripts/start_win.ps1` /
  `scripts/stop_win.ps1`. Status: `scripts/status.ps1`.
- **Parity (on-demand only):** WSL `Debian-M2` box, cap 1 GB, off when unused.
  Holds the frozen C++ oracle binaries and `mysql_proxy` for parity_boot A/B
  and golden captures. Used only for parity sessions (F6 side-by-side, golden
  captures). Full delete at F6.

**Definition of done:** the gate is `scripts/verify.ps1` (fmt + test --workspace
+ ignored informative leg + clippy -D warnings + git diff --check). The slice is
not done until the local run prints `OK: verificacion completa`. The GitHub
workflow `.github/workflows/docs.yml` runs the same script on every push and
every PR, plus a documentation metadata gate and a handoff check (source
touched => `progress.md` / `CHANGELOG.md` / `gap-registry.md` updated).

## Current phase (2026-08-30) — NATIVE WINDOWS RUNTIME (ADR-0012), world entry + ECS World working

The execution history is preserved in the wave notes below and in `CHANGELOG.md`
(per-date entries). For current state, runtime status, and open rows, see
`documentation/progress.md` and the gap registry. For the protocol/data rules
that the next session still needs (the login fix chain, encryption, encoding
quirks), jump to the `## Historical` section below.

**World entry WORKS on the all-Windows stack (verified with an external
compatible client on 2026-08-12: login → select → world → movement).** The
key fixes that made this work are recorded in CHANGELOG.md 2026-08-12
(`replay_once` async via `tokio::sync::OnceCell`, dynamic spawn materialization
in `game_core/src/ecs/systems/spawn.rs` with `SPAWN_VIEW = 300000` and
`DESPAWN_RADIUS = 310000`, character ADD `b_moving_speed`/`b_attack_speed`
100/100, `TPacketGCMainCharacter` HEADER 113 → 15 fix). The current
predicates are covered by the G0.1c checks in the gap registry.

**Backup cadence (oracle H.2) live:** nightly `scripts\backup_win.ps1`
(native `pg_dump -Fc` of `metin2` → `C:\projects\metin2-extra\backups\metin2_<yyyy-MM-dd>.dump`,
retention 7). The 53-commit backlog is PUSHED to `origin/main`; the current
HEAD is in the live handoff.

## Historical — login fix chain (2026-08-08, verified facts; still authoritative for protocol/data rules)

**The full login (auth + channel + character select) is FIXED and verified on 2026-08-08.** Test account: `test` / `1234`.

Chain of fixes applied in that session (each verified empirically):

**Server — game binary (auth + cores):**

1. `desc.cpp` `ProcessOutput`: consume `result > 0` bytes (socket_write returns length on full success; 0 = EAGAIN; -1 = error). The previous model broke this and the output buffer never drained †â the client received no responses.
2. `socket.c` (libthecore): the model had changed the return semantics of `socket_write` (success = length); `desc.cpp` must match that semantics (consistent pair). **Do not revert one without the other.**
3. `main.cpp`: reverted the io_loop hack (fallthrough READ†âWRITE + sys_err debug). `optreset†âoptind` was kept (needed for glibc/Linux).
4. Encryption: `_IMPROVED_PACKET_ENCRYPTION_` OFF on both sides (client and server = plaintext). Sequence OFF on both.
5. `utils.cpp` `mysql5_password`: **THE REAL FORMAT INCLUDES THE ASTERISK** (`"*" + encoded2` uppercase — lines 51-57). The hash stored in `account.password` MUST have the `*` (native MySQL `PASSWORD()` format). Do not "fix" it by removing the asterisk — that breaks the auth strcmp.

**Server — db binary:**
6. `PeerBase.cpp` `Send()`: consume `result > 0` (same problem as the game).
7. `ClientManagerLogin.cpp` `QUERY_LOGIN`: the query MUST return 13 columns in the order `CreateAccountTableFromRes` expects — hash, id, login, password, social_id, bEmpire, pid1..pid5, status, lang: `SELECT mysql_hash_password('%s'), a.id, a.login, a.password, a.social_id, pi.empire, pi.pid1, pi.pid2, pi.pid3, pi.pid4, pi.pid5, a.status, a.lang FROM account a LEFT JOIN player.player_index pi ON pi.id = a.id WHERE a.login='%s' AND a.password=mysql_hash_password('%s')` (verified 2026-08-10, `ClientManagerLogin.cpp:395-426` + `CreateAccountTableFromRes:259-297` — the `lang` column comes from the Language System ALTER). The previous model used 6 columns in another order †â WRONGPWD always.
8. SQL routing: account queries must use `iSlot = SQL_ACCOUNT` (the default is `SQL_PLAYER` †â `player.account` does not exist).
9. `ClientHandleInfo` (ClientManager.h): the constructor did NOT initialize `account_index`/`account_id` †â garbage †â RESULT_LOGIN took the wrong branch. Now initialized to 0.
10. Main-loop flood: the WRITE interest of fdwatch is persistent (the `oneshot` flag is ignored in `fdwatch_add_fd`); after draining the peer buffer, re-register with READ only (`fdwatch_del_fd` + `add_fd(FDW_READ, false)` in the FDW_WRITE case) — otherwise the db spins on `AUTH_PEER_WRITE: size 0` and never processes the async results.
11. SQL function created in MariaDB: `account.mysql_hash_password(pw)` = `CONCAT('*', UPPER(SHA1(UNHEX(SHA1(pw)))))` (matches the C++).

**Client:**
12. `AccountConnector.cpp` `__AuthState_RecvPhase`: removed `rkNetStream.ClearLoginInfo()` — it deleted `m_stPassword` from CPythonNetworkStream DURING auth †â the channel connection sent an empty password †â "incorrect password" at the channel. (The client's `ClearLoginInfo` only clears the password, not the ID — that is why the channel received the correct login with an empty password.) Client RECOMPILED with this fix (Release|Win32, metin2client.exe ~5.1MB).
13. Pack `root.epk` (intrologin.py): re-SetLoginInfo with the edit-line values before connecting to the channel (lines ~1082 and ~1320). Repacked with `PackMakerLite.exe` (in `source\tools\pack\` — uses `PackMakerLite.json` with the pack keys).
14. **WORLD ENTRY (new, 2026-08-08):** `PythonNetworkStreamPhaseLogin.cpp` `SetLoginPhase`: `ClearLoginInfo()` (which deleted `m_stPassword`) was REMOVED in BOTH branches (normal and DirectEnter). The world-entry flow reconnects to the channel several times and RE-AUTHENTICATES with LOGIN3 on each reconnect: (a) DirectEnter from the select (`ConnectGameServer`, `introselect.py` †â `net.DirectEnter`), (b) warp to the game (`RecvWarpPacket`). Empty password †â db `RESULT_LOGIN: no account` †â `GC_LOGIN_FAILURE` which the client in DirectEnter-mode swallows †â client stuck at the select. Verified: the server accepts the LOGIN3 with the correct password and responds to `CG_PLAYER_SELECT` (header 6) with character creation in the world (map `Venter_the_east.mp3`, stats). Client RECOMPILED (Release|Win32, 14:55, ~5.1MB) and deployed to `client\metin2client.exe`. The password copy stays in memory for the session (intentional).
15. **CHAT SPAM / monster_chat (new, 2026-08-08, DATA fix — no rebuild):** the chat spammed `SYSERR: LUA ScriptRunError (code:1 src:[(locale.monster_chat[vnum] ...)])` for every monster in combat. Double cause, both in the `share/locale/spain` runtime:
    - `translate.lua` was deployed EMPTY (0 bytes) †â `gameforge` was never defined. Fix: `translate.lua` †Â `translate_ES.lua` (1.1MB, the real Spanish content). Same for `germany` †Â `translate_DE.lua`.
    - `quest/locale.lua` had a SYNTAX error for the server's lua 5.0: the lexer is MODIFIED for Korean EUC-KR (2 bytes per char — `read_string` in `liblua/5.0/src/llex.c` consumes 2 bytes when `b_current & 0x80`). The Spanish file was UTF-8 with 349 lines of Korean of **3 bytes** †â parity misalignment †â the closing quote gets "eaten" †â `unfinished string` (LUA_ERRSYNTAX=3) †â `LoadQuestLocale returns 3` †â `locale.monster_chat` NEVER defined. Fix: convert ONLY the Korean lines to CP949 (script `fix_locale_enc.py`: 349 lines, 0 failures); the Spanish (2-byte UTF-8 accents) stays intact. Verified: `/home/m2/luaparse` (the server's real lua 5.0 harness) †â `SYNTAX_OK`; after core restart: `LoadQuestLocale(...) returns 0` and 0 new ScriptRunErrors.
    - **RULE: server locale lua files containing Korean MUST use CP949/EUC-KR (2 bytes/char), NOT UTF-8 — the server lexer breaks on UTF-8.**
16. **MOB NAMES IN KOREAN (new, 2026-08-08, DATA fix — no rebuild):** mobs showed Korean/mojibake names in game. Double cause:
    - The deployed db binary has `ENABLE_PROTO_FROM_DB` + config `PROTO_FROM_DB` active (evidence: syserr `InitializeItemTableFromDB`) †â the db reads `mob_proto`/`item_proto` from **MySQL** (`SELECT vnum, name, locale_name, ... FROM mob_proto ORDER BY vnum` in `ClientManagerBoot.cpp:1290-1309`), NOT from the `db/` txt files. Editing `mob_proto.txt`/`mob_names.txt` has no effect while `PROTO_FROM_DB=1` (the deployed `mob_names.txt` was also EMPTY, 0 lines).
    - The MySQL data was Korean CP949 double-encoded: the original `mob_proto.txt` has Korean CP949 names (e.g. `b5 e9 b0 b3` = Ã«âÂ¤ÃªÂ°Å) and importing them produced `C2B5C3A9...` (each CP949 byte †â latin-1 †â UTF-8) in the utf8mb4 `name`/`locale_name` columns.
    - **FINAL FIX (multilanguage):** names were taken from the CLIENT PACK (`locale.epk` †â `locale/es/mob_proto`, decoded with `DumpProto.exe` — `Srcs\Tools\DumpProto`, mob keys {4813894,18955,552631,6822045}, item keys {173217,72619434,408587239,27973291}; MMPT0/MIPX format + TEA-ECB 32 rounds + LZO1X). The S3llMetin2 v24 Spanish pack already ships the 2864 Spanish names (Perro Salvaje, JabalÃÂ­, Zorro del Desierto...). `UPDATE mob_proto SET name=..., locale_name=...` from the dump's `mob_names.txt` (script `gen_pack_sql.py`), truncated to 24 bytes (`varbinary(24)` — a longer name fails with `ERROR 1406 Data too long`). 14 mobs untranslated in the pack (Korean) were left intact. Verified: `MOB: #101 Perro Salvaje`, `#2101 Zorro del Desierto`, `#20001 Alquimista` in the core boot.
17. **MULTILANGUAGE ARCHITECTURE (verified 2026-08-08) — the client translates, the server does NOT:**
    - **Mobs:** the spawn packet `TPacketGCCharacterAdd` carries NO name †â the client resolves the mob name from ITS pack (`CPythonNonPlayer::LoadNonPlayerData("locale/es/mob_proto")` †â `GetName(race)` †â `szLocaleName`). The server does not render mob names.
    - **Items:** the client translates from `locale/es/item_proto` (MIPX + TEA + LZO †â `CItemData::szLocaleName`) + `itemdesc.txt` (descriptions only). The server does not render item names.
    - **NPCs:** since 2026-08-09 the client also resolves them from ITS pack. The server DOES send the name (`GC_CHAR_ADDITIONAL_INFO` †â `char.cpp:922-948` †â `GetName()` †â `szLocaleName` from MySQL) but the client IGNORES it for `TYPE_NPC` and uses `CPythonNonPlayer::GetName(race)` from the pack (fallback to the server name if the pack has no entry — `PythonNetworkStreamPhaseGameActor.cpp` `RecvCharacterAdditionalInfo`). Before (08-08) they depended on the DB †â did not change language with the client.
    - **TRAP (caused a core crash):** the server's drop txt files (`etc_drop_item.txt`, and presumably `common_drop_item.txt`, `drop_item_group.txt`, etc.) reference items **BY NAME in CP949** (`ReadEtcDropItemFile` in `item_manager_read_tables.cpp:457-498` †â `GetValidVnum`/`GetVnumByOriginalName`). If `item_proto` in MySQL lacks the original CP949 names, the core aborts boot with `No such an item (name: ...)` †â `Boot: cannot load ETCDropItem`. **RULE: do NOT touch the `item_proto` names on the server — they must stay original CP949; visible item names come from the client.**
    - The `mob_proto` names CAN be changed in MySQL (mobs are not referenced by name in the boot txts); NPCs will show the MySQL `locale_name`.
    - The dbmanager in `source\tools\DBManager` (PHP/bash suite) exists but is only txt†âmysql import/export — it does not translate.

## Repository layout

> **Structure reorganized 2026-08-09:** all code lives under `source/`, organized by component (no `metin2_` prefixes, no intermediate `Srcs`). The old paths (`source\metin2_client`, `source\metin2_server`, `source\metin2_pack`, `source\metin2_svfiles`) DO NOT exist.

| Path | What it is |
| --- | --- |
| `C:\projects\metin2-extra\client` | External compatible client installation (local verification only; not distributed or built by this repository) |
| `client-om2\` | Reference client source ("Old Metin2 Project", corresponds to tmp4-server) — **DELETED** (cleanup 2026-08-11) |
| `source\server\` | SERVER SOURCE (MartySama 5.9) — **FROZEN ORACLE (user decision 2026-08-12, ADR-0012): NEVER rebuilt**. `{common,db,game,libgame,liblua,libpoly,libsql,libthecore}`, Makefile ported to Debian/gcc — the Linux ELF binaries run only in the on-demand WSL box |
| `source\reforge\` | **RUST SERVER REWRITE (new, 2026-08-10, flat layout — ADR-0004)** — Cargo workspace: `protocol` (F0: byte-exact wire, 30/30), `network` (F1: tokio+framer+handshake, 23/23; includes `auth` module for F2), `database` (F3), `game_core` (F4+ — renamed from `realm` 2026-08-13, 42nd part), `server_realms` (single binary, roles `auth\|channel` by config — provisional user name). ADR-0003/0004. **Do not touch the C++ baseline from here; the baseline is the oracle** |
| `source\deploy\` | Deployed runtime (gitignored): `win\` = **native Windows Rust runtime** (`server_realms.exe`, `auth.toml`/`channel.toml`, `logs\` — ADR-0012); `main\srv1\{db,auth1,chan\chX\coreY}` = frozen C++ WSL instances (parity oracle, never rebuilt) |
| `source\tools\` | Supporting data and protocol tools that remain in the server repository; client and pack sources are excluded by ADR-0015 |
| `source\tools\proto\` | Protocol metadata |
| `C:\projects\metin2-extra\pg18` | Native PostgreSQL 18.4 (ADR-0012): binaries `pg18\pgsql\bin`, data `pg18\data`, Windows service `postgresql-metin2` (NETWORK SERVICE) |
| `C:\projects\metin2-extra\backups` | PG dumps — `metin2_pg_2026-08-12.dump` (migration, restore verified) + nightly `metin2_<yyyy-MM-dd>.dump` from `scripts\backup_win.ps1` (retention 7) |
| `C:\projects\metin2-extra\archive` | Backups (client + source + `mariadb_full_2026-08-12.sql` 5.7 MB) — **MOVED OUTSIDE THE REPO 2026-08-11** (cleanup) |
| `.commandcode\` | Skills |
| `C:\projects\metin2-extra\` | OUTSIDE THE REPO (cleanup 2026-08-11): installed `client\` (2.2 GB), `archive\` backups (1.6 GB) |
| `documentation/` | Lean documentation hub: `README.md` (index + cheat sheet), `roadmap.md`, `schema.md`, `rules.md`, `adr/` (15 ADRs, with ADR-0013 superseded), `reference/login-flow.md`, `history/` (archived, read-only). `documentation/progress.md` is the live handoff |
| `ROADMAP.md`, `CHANGELOG.md` | Master plan of the Rust rewrite and chronological change record |
| `scripts\` | Windows runtime + ops (ADR-0012 primary): `start_win.ps1`, `stop_win.ps1`, `backup_win.ps1` (nightly pg_dump, retention 7); WSL parity/recovery (historical path): `start_m2_min.sh`, `start_m2_full.sh`, `mem_audit.sh`, `watch_*.sh` |

**WSL (Debian-M2) — ON-DEMAND ORACLE BOX ONLY (ADR-0012, 2026-08-12):**

- Kept ONLY to run the frozen C++ binaries (`game_r41023`/`db_r41023`, Linux ELF, never rebuilt) + `mysql_proxy` for parity sessions (parity_boot A/B, golden captures, F6 side-by-side). Memory cap 1 GB (`.wslconfig`); `wsl --shutdown` when not in use.
- `/home/m2/source` — was the build + deploy copy (THE source of truth for compiling the server until 2026-08-12); archived to `metin2-extra\archive`, full delete at F6 (`wsl --unregister Debian-M2`).
- `/home/m2/source/metin2_svfiles/main/srv1` — the frozen C++ instances (binaries via symlinks to `share/bin/{game,db}`), used only in parity sessions.
- `/home/m2/tmp4-server` — upstream old-metin2.com git repo (reference, NOT the active source).
- The binary backups in `/root/m2_backup_bins` were DELETED in the disk cleanup (2026-08-08).
- The proxy STAYS in WSL so the frozen C++ `conf.txt` (SQL slots †â `127.0.0.1:3307`) is never touched; its PG target is the Windows host (WSL gateway IP, one TOML line patched per boot; Windows Firewall allows 5432 from the WSL subnet).

## CRITICAL RULE: the C++ server is FROZEN (supersedes the "two copies" rule, 2026-08-12)

- **Since ADR-0012 (2026-08-12) the C++ server is NEVER rebuilt** (user decision: the Linux ELF binaries are the parity oracle and run only in the on-demand WSL box). There are no more WSL builds — `C:\projects\Metin2\source` (Windows) is now the only local source copy; the authored Rust server is the public implementation.
- The compatible client is external to this repository and is used only for local verification. This repository does not compile or package it.
- Historical rule (until 2026-08-12): `/home/m2/source` (WSL) was the copy that compiled the server (`VERSION.txt` baked that path); the previous model's disaster came from editing both copies inconsistently and from opposing protocol defines between client and server — after any change both copies had to be synced (diff/md5sum) and the protocol defines verified on both sides.
- CAUTION (still true for WSL parity sessions): WSL crashes can LOSE writes without flushing (ext4) — after deploying binaries in WSL, run `sync` and verify with md5sum.

## Protocol facts (verified 2026-08-08)

- External compatible client v40999, server `__GAME_VERSION__` 41023 from the frozen local oracle (`source\server`).
- Header tables coherent between server and both clients (handshake 0xff/0xfe, LOGIN3=111, GC_AUTH_SUCCESS=150, GC_LOGIN_SUCCESS=6, GC_LOGIN_SUCCESS_NEWSLOT=0x20, GC_EMPIRE=90...).
- Login flow: client †â auth(30001): GC_PHASE + GC_HANDSHAKE (with clock-bias retries ~40-80ms) †â CG_HANDSHAKE echo †â LOGIN3(65 bytes: 0x6F + name[31] + pwd[17] + keys[16]) †â QID_AUTH_LOGIN (SQL in db) †â strcmp(hash with *, stored hash) †â GC_AUTH_SUCCESS(0x96+key+result) †â client closes auth †â connects to the channel(30003) †â LOGIN3 †â GD_LOGIN †â QUERY_LOGIN (13 columns) †â RESULT_LOGIN †â GC_EMPIRE(0x5a+empire) + SendLoginSuccessPacket †â character select.
- **Encryption: `_IMPROVED_PACKET_ENCRYPTION_` OFF on both sides; `USE_NO_PACKET_ENCRYPTION` ON (plaintext).** Sequence OFF on both. If one side changes, change the other.
- `serverinfo.py`: host `127.0.0.1` (repacked 2026-08-12, ADR-0012 — client and servers share the Windows host), auth 30001, ch1 30003, ch2 30007, ch3 30011, ch4 30015.
- Runtime (native Windows, ADR-0012): PostgreSQL 18.4 service `postgresql-metin2` on 127.0.0.1:5432 (db `metin2`, role mt2/mt2), Rust auth :30001 + Rust channel :30003 from `source\deploy\win`. Historical WSL runtime (until 2026-08-12, archived): MariaDB 127.0.0.1:3306 (dbs `account`,`common`,`player`,`log`, user/pass mt2/mt2), srv1-db 30000, auth1 30001/30002, cores 30003+.
- Parity sessions (frozen C++ in the WSL oracle box): the old `serverinfo.py` host `172.25.104.175` (WSL eth0 IP — **CHECK after every WSL restart**).
- Test account: `test` / `1234` (hash `*A4B6157319038724E3560894F7F932C8886EBFCF` in `account.account`).

## Runbook

### Primary path (native Windows, ADR-0012) — the daily stack

```powershell
# 1. Start: PG service postgresql-metin2 -> Rust auth :30001 -> Rust channel :30003
powershell -ExecutionPolicy Bypass -File scripts\start_win.ps1
# 2. Verify: ports 5432, 30001, 30003 (the script prints OK/FALTA per port)
# 3. Test login in an externally supplied compatible client (test/1234)
# 4. Logs: source\deploy\win\logs\{auth,channel}.{out,err}.log
# 5. Stop: powershell -ExecutionPolicy Bypass -File scripts\stop_win.ps1
# 6. Nightly backup: powershell -ExecutionPolicy Bypass -File scripts\backup_win.ps1
```

### Parity path (on-demand WSL oracle box, ADR-0012) — frozen C++ only

```powershell
# 1. Start the frozen C++ oracle (db + auth + ch1-core1) — parity sessions only
wsl -d Debian-M2 -- bash /mnt/c/projects/Metin2/scripts/start_m2_min.sh
# 2. Verify: ports 30000-30004; Test-NetConnection 172.25.104.175 -Port 30001
# 3. Logs: auth1 = auth/syslog; core1 = chan/ch1/core1/syslog ("LoginSuccess" = OK);
#    db = db/syslog
# 4. After the session: wsl --shutdown (the box stays off ~95% of the time)
```

- **Memory:** the full C++ stack (9 cores) blows this machine's memory (4 GB host; WSL cap 1 GB since ADR-0012). Use `start_m2_min.sh` for parity sessions; the daily stack is the Windows one.
- **WSL unstable (parity sessions):** `Wsl/Service/E_UNEXPECTED` crashes during heavy I/O — 4 GB RAM machine, Windows 10 22H2, WSL 2.7.3, WHEA PCIe errors. Config in `C:\Users\Ricardo Casamayor\.wslconfig` (memory=1GB, swap=8GB). After each crash: `wsl --shutdown` †â `start_m2_min.sh`.
- **Compile the C++ server: NEVER (frozen, ADR-0012).** Historical instruction (until 2026-08-12, kept for the record): in WSL — libraries first (`cd /home/m2/source/metin2_server/Srcs/Server && make -C liblua/5.0 && make -C libsql && make -C libgame/src && make -C libpoly && make -C libthecore/src`), then `make -C game/src` †â `game_r41023` and `make -C db/src` †â `db_r41023`; deploy to `main/srv1/share/bin/{game,db}` and restart auth+cores. **Always `sync` after deploy.**
- **Client verification:** use an externally supplied, properly licensed compatible client. Client compilation, pack conversion, and client asset distribution are outside this repository (ADR-0015).
- **Mandatory boot order (Windows):** PG service †â Rust auth †â Rust channel (`start_win.ps1` does it). Parity boot order (WSL): mariadb(archived — proxy only) †â srv1-db †â srv1-auth1 †â cores.

## Known pending items (2026-08-08, updated)

- ~~The deployed db binary included temporary debug logs (`DBG_AQR`, `DBG_RESULT_LOGIN`, `DBG_PARSE` in ClientManager.cpp / ClientManagerLogin.cpp)~~ — **CLEANED (session 2):** 0 DBG lines in the new db boot; item award logs clean.
- **Language System (1.2.6 mod multilingual engine):** integrated and loading — 16 languages, 764-775 entries each, evidence in `core1/stdout` (NOT syslog: `sys_log` is silent in `config_init` because the logfile is not open yet and `DB_LOG_LEVEL: 1` †â `log_level_bits=1` blocks `sys_log` stdout; that is why `LocaleService_LoadLocaleStringFile` uses `fprintf(stdout, "Load LocaleString %s (%d entries)")`). Full detail in `documentation/reference/legacy/language-system.md` ÃÂ§5 (historical). **Partial end-to-end test (session 2):** the user played in the world and server texts come out in SPANISH Ã¢Åâ (including `monster_chat`); `login_success: lang 'es' -> 5` confirms account.lang †â g_iCurrentLang propagation. The client (build 18:36, 5,115,392 bytes) sends its language in the auth LOGIN3 (68 bytes: 65 + `szLanguage[3]`) †â auth does `UPDATE account SET lang=...` †â **the client OVERWRITES account.lang on every login** (to test another server language, change the client locale or the send).
- ~~**INTERMITTENT WORLD-ENTRY CRASH — DIAGNOSIS IN PROGRESS (2026-08-09, session 3):** the `string_replace_word` over-read (PythonSkill.cpp:62) was a REAL corruptor and is fixed (bounds check, build 14:12, hash C7EAD7CC)...~~ — **CLOSED (2026-08-09):** see the "World-entry crash" section below and CHANGELOG. Final state: string_replace_word bounds check (build C7EAD7CC) + coordinate fix (`UPDATE player SET x=969600, y=278400` for both old characters) †â **field test 2/2 consecutive entries** (2026-08-09, session 3 4th part). The cdb/WER findings (granny2.dll, igc32.dll detectors) were symptoms of the same corrupted heap, not independent causes. Tools remain installed for future diagnostics (Debugging Tools + LocalDumps C:\dumps + PageHeap via gflags).
- **COORDINATE CONVENTION (CRITICAL for creating/moving characters):** `player.x/y` = **UNITS** (village c1 of map 41 = `969600, 278400`). The `AddGotoInfo` boot values (e.g. c1 `(9696, 2784)`) are **cells (ÃÂ·100)** — do NOT use them directly in the DB. `GetValidLocation` validates the position against the map sectree; on failure †â fallback to `EMPIRE_START_*(empire)` (0,0 for empire 0). A character saved with garbage coordinates crashes the CLIENT with `0xc0000374` (heap corruption) while loading the map.
- LS pending items (full audit 2026-08-09, see CHANGELOG): language selector at login (flag column — user request; the native `IDD_SELECT_LOCALE` dialog is compiled but dead: `LOCALE_SERVICE_GLOBAL` not defined). ~~Multilanguage NPCs~~ — **RESOLVED (2026-08-09):** the client resolves NPCs from its pack (server fallback); client rebuild 5,115,904 bytes (see ÃÂ§17 and CHANGELOG). **Real server gaps:** (A) broadcasts/notices use the last packet's language (`LC_TEXT_LANG` defined but never used — 26 `SendNotice` affected); (B) quest/monster_chat do NOT translate (lua fixed Spanish at boot — `locale_quest_find`/`LC_QUEST_TEXT` from the mod not integrated; **the real cause of the ES/EN mix the user saw**); (C) ~437 `ChatPacket` without `LC_TEXT` (mostly protocol commands, some visible: arena, battle, char.cpp:3045); (D) server NPC names fixed from `mob_proto.locale_name` without a GetLang branch; (E) ES lacks 11 keys EN has (10 used: exchange won †â `@0949`+English for ES players); (F) Windows svfiles copy out of sync (16 locale_string only in WSL). **mod locale_string compatibility (verified):** format 100% compatible with the parser; 11 base languages + AE/EN/GR cover ~75% of the code's 769 keys; **181 keys (23.5%) missing in ALL files †â `@0949`+key for everyone** (52 English: exchange won/dados/fishing; 129 Korean: chat bans, monarch); **PT (44%) and RU (19%) are from another mod base — useless**. Correction: EN covers 100% of the ES keys (the "732 missing" figure from that session was a parse error — pair format, not lines).
- The `test` account has 2 characters (slot 0 = `lkjsnlfknlsk`, slot 4 = `ninja`, both on map 41). **WATCH OUT:** the earlier "World entry VERIFIED" note was SERVER-side evidence (character-creation packet) — the CLIENT crashed during map load until the coordinate fix (`UPDATE player SET x=969600, y=278400`, 2026-08-09) and the `string_replace_word` bounds check; after both, **2/2 consecutive entries** (see "World-entry crash"). The chat spam (monster_chat) was fixed by the data fix of item 15 (`LoadQuestLocale returns 0` after core restart). Pending to test: full combat (partial: the user fought and killed mobs Ã¢Åâ), NPCs, items, drops.
- The `intrologin.py` pack has the password-restoration fixes (lines 1082/1320) that became redundant after the client rebuild — can be kept or reverted.
- ~~Evaluate environment stability (more RAM, WSL update/downgrade, or Docker Desktop) before depending on long sessions~~ — **RESOLVED by ADR-0012 (2026-08-12):** the runtime is native Windows; WSL is on-demand only (cap 1 GB, off when unused). The 4 GB host constraint remains — nightly `scripts\backup_win.ps1` covers the data-loss risk (oracle H.2).

## World-entry crash (0xc0000374 heap corruption) — RESOLVED (2026-08-09)

**Original symptom:** the client crashed with `STATUS_HEAP_CORRUPTION` (0xc0000374, ntdll) ~8-17s after `player_load`, during map load, ~75% of entries (intermittent). IDENTICAL signature in WER since 15:00 on 08-08 (old client build, BEFORE the LS changes) †â not caused by the Language System.

**Deterministic part — RESOLVED (session 2):** the 2 characters were in the DB with garbage coordinates `(960155, 269313)` / `(960970, 271421)` on map 41 (real village = `(969600, 278400)` in UNITS). Fix: `UPDATE player SET x=969600, y=278400`.

**Intermittent part — ROOT CAUSE FOUND AND FIXED (2026-08-09, session 3):**

1. **Decisive evidence: the client's own minidumps** (`client\logs\metin2client_*.dmp`, written by `EterExceptionFilter` in `EterBase\error.cpp` — they were always there, nobody had read them). Two dumps of today's crash (13:15:00, 13:15:25) identical: exception `0xC0000005` in `string_replace_word` (`PythonSkill.cpp:62`), instruction `mov eax,[ecx]` at RVA 0x95110 (`disasm` with dumpbin + PDB), with ECX=0x96510FFD — garbage pointer.
2. **Cause:** `string_replace_word` does `memcmp(base + cur, src, src_len)` WITHOUT checking `cur + src_len <= base_len` †â over-read past the end of the string `base` (a `std::string` in `TokenVector[POINT_POLY]` from parsing `SkillTable.txt`, loaded in the character-select phase). The garbage read could spuriously "match" "number"/"atk"/"mwep" †â corrupted skill formulas stored in `m_SkillDataMap` †â on world entry, evaluating those formulas corrupted the heap †â 0xc0000374. With AppVerifier (guard pages) the over-read was detected instantly at login (that is why the timing changed: "now it closes right after login").
3. **Fix (2 lines):** bounds check `cur + src_len <= base_len` before the `memcmp` (`PythonSkill.cpp:72-90`). Rebuild Release|Win32 †â `client\metin2client.exe` 5,115,904 B, 14:12, hash `C7EAD7CC...` deployed and verified. **CLOSED (2026-08-09, field test 2/2):** after the coordinate fix (`UPDATE player SET x=969600, y=278400`) the user entered the world twice in a row with the recovered characters (CHANGELOG 2026-08-09 3rd session, 4th part).
4. Lessons: (a) the server syserr will NEVER see client crashes (local memory; the server only sees the socket close) — client close errors are in `C:\projects\metin2-extra\client\logs\*.dmp` (binary, parseable with the session's `parse_dump3.py` script or dumpbin/cdb); (b) App Verifier Heaps changes the detection timing (guard pages detect the over-read at the write) — useful to isolate, not to reproduce the original symptom.

## Work rules

> **Agent team:** preset OmO (`openai/gpt-5.6-luna`, variant `max`) — coder/fixer/oracle/explorer/librarian/designer/observer, with per-function skills.

1. Read this file and any nearby `AGENTS.md` before working.
2. Inspect the relevant source, build and runtime before touching anything.
3. Declare the scope; preserve unrelated user changes.
4. Minimal, localized change with justification. Do not hide warnings without documenting them.
5. Proportional verification: inspection †â focused check †â build/run. Report with real command output; do not claim something works without evidence.
6. **The C++ server is frozen — never rebuilt (ADR-0012).** The old "sync the two source copies" rule is dead for the server: `C:\projects\Metin2\source` (Windows) is the only source copy; Rust + client + docs live only on Windows. (WSL parity sessions still require `sync` + md5sum after touching the frozen oracle copy.)
7. Confirm before destructive operations (deleting volumes, databases, build caches).
8. **Docs after every change:** every task ends with its documentation updated — canonical docs, doc-comments and metadata `Last verified` (policy: `documentation/DOCUMENTATION.md`; hub: `documentation/README.md`). Never leave docs describing the old behavior; if a doc is outside the lane's write scope, the agent lists the exact required updates in its report. Update documentation/ADRs when project knowledge changes (see "Documentation methodology" below).
9. **Log the changes (orchestrator log):** at the end of each work session, record in `CHANGELOG.md` (Keep a Changelog style, grouped by date) what changed and with what evidence; mark progress in `ROADMAP.md`; write an ADR before deciding architecture. Never end a session with unlogged changes.
10. **Work in parallel (speed):** when there are independent tasks, deploy specialized agents in the background simultaneously (@explorer for discovery, @librarian for documentation/research, @coder for implementation, @fixer (quality guardian: attacks + writes/expands tests) for review, @oracle for supervision). Do not serialize work that can run in parallel; reconcile results when returning.
11. **Plan mode by default:** for any architecture or rewrite task, FIRST plan and discuss with the user (alternatives, risks, ADR before implementing). Do not write rewrite code without explicit plan confirmation.
12. **Permanent pushback (devil's advocate):** the user explicitly asked: before accepting any plan of theirs, evaluate it critically and, if a significantly better option exists, propose it with concrete arguments (repo facts, measurements, risks). If the plan is solid, validate it with evidence instead of inventing fake pushback. Never accept a plan without analysis.
13. **Graphs first (permanent user rule):** for ANY code search/explore/modify/refactor task, ALWAYS consult the graphify graphs BEFORE grep/glob/blind reading: `graphify query "..." --graph <merged>` for focused questions, `graphify explain/path/god-nodes` for specific nodes, or `GRAPH_REPORT.md` for broad context. The user should not have to ask: it is automatic in every code task. Use only graphs that exist for the checked-out source; the public server graph is the relevant graph for this repository, while old client graphs are external/historical inputs.
14. **Ponytail personality (permanent):** the orchestrator always operates with the ponytail philosophy: YAGNI, the minimal solution that works, stdlib/native before dependencies, one line before fifty, do not write code that is not needed, do not over-build. Apply to all rewrite and baseline code. Never cut validation, security or accessibility — small is a consequence of necessary, not of trimming.
15. **NEVER block the chat with long-running commands (permanent):** any command that takes >15 s (builds, server start/restart, deployments, dumps, restores) MUST run **detached/background** (`Start-Process ... -RedirectStandardOutput <log>` or a background task) and the orchestrator **ends the turn immediately** after launching — the user must be able to keep writing while it runs. Verify on the NEXT turn with quick read-only checks (<10 s each: log tails, `netstat`, `Get-Process`). NEVER chain build†âcopy†ârestart†âverify in one synchronous call. Single quick commands (<15 s) are fine. Guardrail: `documentation/history/guardrails/operations.md` ÃÂ§11 (historical evidence).
16. **NEVER chain server operations in one command (permanent, refinement of rule 15):** stopping a process, copying a binary, starting a server and verifying ports are **separate single-purpose commands**, each <10 s, NEVER concatenated with `;` in one call (2026-08-13: the stop+copy+start+verify one-liner hung 4+ times and blocked the chat every time — the output truncated mid-command). If a launch is needed: run ONLY the `Start-Process` line(s), then **end the turn**; check `netstat`/logs on the next turn. Do not "finish the job" in one shot at the cost of blocking the user.
17. **Conventional Commits in English (permanent, 2026-08-15; trailer-free, 2026-08-30):** every commit message follows [Conventional Commits](https://www.conventionalcommits.org/) — `type(scope): description` in English, imperative present tense, no period at the end. Types: `feat` (new behavior), `fix` (bug fix), `docs` (documentation only), `refactor` (no behavior change), `perf` (performance), `test` (tests only), `chore` (build/tooling/backlog, e.g. committing work that was never committed), `revert`. One logical change = one commit (**atomicity** — no "fix A + fix B" mixes: enables `git bisect` and clean reverts). The scope is optional but recommended (`fix(movement)`, `feat(channel)`). **Do NOT add a `Co-authored-by:` trailer to commit messages** (user decision 2026-08-30: the public repository has a single author identity, `ryer <82473243+ryerdevs@users.noreply.github.com>`, and no co-author trailers). Historical commits using the old styles (`[fix]`, Spanish, multi-purpose) are preserved verbatim — never rewritten. Do not rewrite pushed history.
18. **Automate in scripts, don't improvise:** anything repeatable goes to `scripts/*.ps1` (`status.ps1` snapshot, `verify.ps1` definition of done, `start_win.ps1`). The model runs the script, it does not invent commands.
19. **Single handoff:** `documentation/progress.md` is read at the start and updated at the close of every session/slice. Never guess where we left off.
20. **Synthetic tests that catch bugs:** every fix must have a mutation test that fails if the fix is reverted (verifier pattern), plus property tests (`proptest`) wherever invariants exist. `passed >= N` is not a gate.

> **Ops note (2026-08-13, 43rd part):** `OPENCODE_EXPERIMENTAL_BACKGROUND_SUBAGENTS=true` is set in `~/.config/opencode/opencode.jsonc` — agent tasks now run in the BACKGROUND and the conversation never blocks while one works (root fix for the recurring "agent stuck / chat frozen" problem). Takes effect after an opencode restart.

## Documentation methodology (how the count is kept)

The project follows the standard pattern for AI-agent projects (AGENTS.md + CHANGELOG.md + ROADMAP.md + documentation/), the methodology the user asked to adopt (reference: "purely" from gestorify — identical pattern to Keep a Changelog + ROADMAP + AGENTS.md).

**Documentation is written in English** (technical terms, code identifiers and commands stay as-is). Historical/superseded content is marked and kept — never deleted (no-hide-history rule).

- **`documentation/README.md` â index + one-page cheat sheet (quick start, team, cycle).
- **`documentation/DOCUMENTATION.md`** â mandatory documentation policy and review checklist.
- **`documentation/roadmap.md`** — one-page roadmap.
- **`documentation/schema.md`** — database schema.
- **`documentation/rules.md`** — rules never to repeat (consolidates guardrails).
- **`documentation/adr/`** — ADRs (renamed from `decisions/`; template: Status/Date/Context/Decision/Alternatives/Consequences). Every architecture decision is written BEFORE implementation.
- **`documentation/reference/login-flow.md`** — wire contract.
- **`documentation/history/`** — archived docs (read-only); old planning and specification material is preserved and indexed there.
- **`documentation/progress.md`** — live handoff, source of truth between sessions (read at start, updated at close — rule 19).
- **`CHANGELOG.md`**, **`ROADMAP.md`** (root) — kept as historical/master record.
- **Graphs** — after relevant Rust-server code changes, refresh the graph for the checked-out server source and re-merge only existing graph inputs. Do not treat an external client graph as a repository dependency. See rule 13.

## Guardrails for the Rust rewrite

- Do not mix modernization changes into C++ baseline work.
- Unifying `game` and `db` is DECIDED in ADR-0002 (accepted: one process per region, db as crate); the legacy shim must stay thin; document before diverging from it.
- Keep the C++ baseline stable and reproducible while a module is ported; verify behavior parity per module.
- Compatibility adapters between the Rust server and the legacy must be explicit: legacy peer protocol (ADR-0002 shim), PostgreSQL cutover adapter (ADR-0005 — Accepted 2026-08-10, gate 4/4; `mysql_proxy` live), legacy wire/pack boundary `protocol::legacy` (ADR-0006 — Accepted 2026-08-10, implemented in F2a).
- No partial Rust embedded in the legacy client during F0€âF6 (ADR-0007 — accepted for the already-agreed boundary).
- Newer ADRs: **ADR-0008** (Accepted 2026-08-11 — data layer: tokio-postgres 0.7, save-by-event + WAL durable, RLS post-WAL, Patroni F5/F6), **ADR-0009** (Accepted 2026-08-12 — server-side locale: server owns all text per language), **ADR-0010** (Accepted 2026-08-12 — domain boundaries: pure-function modules + **bevy_ecs World adopted** (user decision: mob-farming density is the core requirement) + per-connection state + WorldStore; F5 benchmark validates), **ADR-0011** (Accepted 2026-08-12 — anti-hack model: always-on controls, signed clock wrap decided; server-authoritative invariant), **ADR-0012** (Accepted + executed 2026-08-12 — runtime hosting: native Windows + WSL on-demand oracle box until F6; the C++ binaries are frozen and NEVER rebuilt), **ADR-0013** (Superseded 2026-08-30 by ADR-0015 — former client rewrite plan), **ADR-0014** (Accepted 2026-08-27 — five stat points per level), and **ADR-0015** (Accepted 2026-08-30 — Rust-only public repository boundary).
- Current Rust workspace: `source/reforge` (ADR-0003/0004) — flat layout, `unsafe_code = "forbid"`, `server_realms` single binary with roles by config.


