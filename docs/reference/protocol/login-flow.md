---
Type: Reference
Status: Current
Audience: Contributors
Last verified: 2026-08-10
---

# Wire Protocol — Login Flow (byte-exact contract of the Rust `protocol` crate)

> Extracted from the legacy C++ server on 2026-08-08 using the graphify graph (coordinates in §6).
> This document is the byte-exact contract for the `protocol` crate of the Rust rewrite (ROADMAP Phase 0).
> Verified against: `packet.h`, `packet_info.cpp`, `input_auth.cpp`, `input_login.cpp`, `input_db.cpp`, `desc.cpp`, `common/tables.h`, `length.h`, `db/src/ClientManagerLogin.cpp`, `ClientManager.cpp`, `QID.h`.
> Related: legacy-only packets 151/152/153 live under the compatibility boundary — see [legacy-compatibility.md](legacy-compatibility.md). Canonical plan: [../../plans/server-rewrite.md](../../plans/server-rewrite.md).

## 1. Constants

| Constant | Value | Wire impact |
|---|---|---|
| `LOGIN_MAX_LEN` | 30 | buffers `[31]` |
| `PASSWD_MAX_LEN` | 16 | buffers `[17]` |
| `CHARACTER_NAME_MAX_LEN` | 24 | buffers `[25]` |
| `HANDSHAKE_RETRY_LIMIT` | 32 | `desc.h:17` |
| `PLAYER_PER_ACCOUNT` | 5 | `ENABLE_PLAYER_PER_ACCOUNT5` |
| `SOCIAL_ID_MAX_LEN` | 18 | `[19]` |
| `ACCOUNT_STATUS_MAX_LEN` | 8 | `[9]` |

Build flags that change sizes: `ENABLE_ACCE_COSTUME_SYSTEM` (+4B in TSimplePlayer), `ENABLE_QUIVER_SYSTEM` (+dwArrow), `ENABLE_SEQUENCE_SYSTEM` OFF (no sequence byte), `USE_NO_PACKET_ENCRYPTION` ON (plaintext), `__LANGUAGE_SYSTEM__` (LOGIN3 at auth +3 bytes).

## 2. Framing rules

- **There is no length prefix.** A packet = header BYTE + fixed payload. Sizes come from tables known on each side.
- Client→server: table `CPacketInfoCG` (`packet_info.cpp:136-236`). Key entries (sequence OFF):
  - `0xff` Handshake = `sizeof(TPacketCGHandshake)` = 13
  - `0xfe` Pong = 1
  - `1` Login = 49; `109` Login2 = 52; **`111` Login3 = `sizeof(TPacketCGLogin3) + (auth ? 3 : 0)` = 65 at the channel, 68 at auth** (`"es\0"` suffix)
  - `6` PlayerSelect = 2; `4` Create = 34; `5` Delete = 10; `0xfc` TimeSync = 13
  - Unknown header → `CPacketInfo::Get` false → connection closed (`input.cpp:77-84`). Variable-size packets return `iExtraLen` from `Analyze` (`input.cpp:96-101`).
- Server→client: no table; the server sends `desc->Packet(&struct, sizeof(struct))` — raw structs.
- **Endianness:** native little-endian (x86).
- **Alignment:** `packet.h:304..2305` wraps ALL packet structs in `#pragma pack(1)` → byte-exact, no padding. **ERRATA 2026-08-10:** `TSimplePlayer`/`TAccountTable` (`tables.h:285-316`) are NOT declared before the pack(1) — `tables.h:271` opens `#pragma pack(1)` and `tables.h:1333` closes it, so both are INSIDE the pack → **TSimplePlayer = 71B packed, TPacketGCLoginSuccess = 449B, TAccountTable = 444B** (the client agrees: `UserInterface/Packet.h:355-356` + `TSimplePlayerInformation:1063` → `sizeof(TPacketGCLoginSuccess4)` = 449, verified in production). See §7.

## 3. Byte-exact structs (all packed, LE)

| Struct | Size | Layout |
|---|---|---|
| `TPacketCGHandshake` | 13 | `BYTE bHeader; DWORD dwHandshake; DWORD dwTime; long lDelta` |
| `TPacketCGLogin` | 49 | `BYTE header; char login[31]; char passwd[17]` |
| `TPacketCGLogin2` | 52 | `BYTE header; char login[31]; DWORD dwLoginKey; DWORD adwClientKey[4]` |
| `TPacketCGLogin3` | 65/68 | `BYTE header; char login[31]; char passwd[17]; DWORD adwClientKey[4]` + at auth `char szLanguage[3]` |
| `TPacketGCLoginKey` | 5 | `BYTE bHeader; DWORD dwLoginKey` |
| `TPacketCGPlayerSelect` | 2 | `BYTE header; BYTE index` |
| `TPacketCGPlayerDelete` | 10 | `BYTE header; BYTE index; char private_code[8]` |
| `TPacketCGPlayerCreate` | 34 | `BYTE header; BYTE index; char name[25]; WORD job; BYTE shape; BYTE Con; BYTE Int; BYTE Str; BYTE Dex` |
| `TPacketGCPhase` | 2 | `BYTE header; BYTE phase` (1 HANDSHAKE, 2 LOGIN, 3 SELECT, 4 LOADING, 5 GAME, 10 AUTH) |
| `TPacketGCHandshake` | 13 | same as CG handshake |
| `TPacketGCAuthSuccess` | 6 | `BYTE bHeader; DWORD dwLoginKey; BYTE bResult` |
| `TPacketGCLoginFailure` | 10 | `BYTE header; char szStatus[9]` ("NOID","WRONGPWD","ALREADY","NOTAVAIL","BLKLOGIN","VERSION","FULL","SHUTDOWN") |
| `TPacketGCEmpire` | 2 | `BYTE bHeader; BYTE bEmpire` (1..3) |
| `TPacketGCLoginSuccess` | **449** (ERRATA: not 474) | `BYTE bHeader(=0x20=32)`; `TSimplePlayer players[5]` packed (offset 1, 355B); `DWORD guild_id[5]` (356, 20B); `char guild_name[5][13]` (376, 65B); `DWORD handle` (**441**); `DWORD random_key` (**445**) |
| `TPacketGCCharacterAdd` | 37 | `BYTE header; DWORD dwVID; float angle; long x,y,z; BYTE bType; DWORD wRaceNum; BYTE bMovingSpeed; BYTE bAttackSpeed; BYTE bStateFlag; DWORD dwAffectFlag[2]` |
| `TPacketGCCharacterAdditionalInfo` | 70 | `BYTE header; DWORD dwVID; char name[25]; DWORD awPart[5]; BYTE bEmpire; DWORD dwGuildID; DWORD dwLevel; short sAlignment; BYTE bPKMode; DWORD dwMountVnum; DWORD dwArrow` |

`TSimplePlayer` (**71B packed, ERRATA: not 76B**, `tables.h:285` INSIDE the pack(1) opened at `tables.h:271`, ACEE+QUIVER ON): `DWORD dwID`(0); `char szName[25]`(4); `BYTE byJob`(29); `BYTE byLevel`(30); `DWORD dwPlayMinutes`(31); `BYTE byST,byHT,byDX,byIQ`(35,36,37,38); `DWORD wMainPart`(39); `BYTE bChangeName`(43); `DWORD wHairPart`(44); `DWORD wAccePart`(48); `BYTE bDummy[4]`(52); `long x,y`(56,60); `long lAddr`(64); `WORD wPort`(68); `BYTE skill_group`(70). Sum: 4+25+1+1+4+4+4+1+4+4+4+4+4+4+2+1 = 71. The original layout offsets (47,51,55,59,63,67,71,73,75) were wrong.

## 4. Login state machine

**(a) Auth :30001 (CInputAuth, `input_auth.cpp:223-257`):**
1. S→C: `GC_PHASE=0xfd` (PHASE_HANDSHAKE) + `GC_HANDSHAKE=0xff` (retries up to 32)
2. C→S: `CG_HANDSHAKE=0xff` (13B) → phase PHASE_AUTH; S→C then sends `GC_PHASE=0xfd` (PHASE_AUTH=10) — **the client sends LOGIN3 ONLY on receiving it** (`AccountConnector.cpp` `__AuthState_RecvPhase`; verified 2026-08-10 in the Rust auth — a missing PHASE_AUTH hangs the real client at "connecting" until timeout)
3. C→S: `CG_LOGIN3=111` (68B with lang) → `CInputAuth::Login` (`input_auth.cpp:66`) → `ReturnQuery(QID_AUTH_LOGIN, ...)` — SQL **15 columns** (ERRATA: not 13, `input_auth.cpp:207-218`): `mysql_hash_password('%s'), password, securitycode, social_id, id, status, availDt-NOW()>0, 7×UNIX_TIMESTAMP(...), create_time` (col0 = hash with `*`, strcmp against the stored one — `db.cpp:340`)
4. Result in `DBManager::AnalyzeReturnQuery` (`db.cpp:229-396`): 0 rows → `GC_LOGIN_FAILURE` "NOID"; hash mismatch → "WRONGPWD"; otherwise → `SendAuthLogin` (`db.cpp:179-202`, `HEADER_GD_AUTH_LOGIN=100`, `TPacketGDAuthLogin` **110B** (ERRATA: not 100 — 4+4+31+19+16+36=110, `tables.h:987-995` with `iPremiumTimes[9]`): `DWORD dwID; DWORD dwLoginKey; char szLogin[31]; char szSocialID[19]; DWORD adwClientKey[4]; int iPremiumTimes[9]`)
5. db `QUERY_AUTH_LOGIN` (`ClientManager.cpp:1854-1901`) registers CLoginData → `HEADER_DG_AUTH_LOGIN` (1 BYTE result)
6. game `CInputDB::AuthLogin` (`input_db.cpp:1697-1728`) → on success first sends PanamaPack 151 + hybrid-crypt 152/153 — **conditional on runtime files** (`panama/panama.lst` + `cshybridcrypt*`; the current srv1 runtime has none → the C++ auth sends none; parity implemented in `protocol::legacy`, ADR-0006) — then **S→C: `GC_AUTH_SUCCESS=150`** (6B, game → client) → the client closes auth and connects to the channel

**(b) Channel :30003 (CInputLogin, `input_login.cpp:1023-1119`):**
1. S→C: `GC_PHASE=0xfd` (PHASE_LOGIN)
2. C→S: `CG_LOGIN3=111` (65B, no lang) → `CInputLogin::Login` (`input_login.cpp:97-147`) → `HEADER_GD_LOGIN=1` `TLoginPacket` (48B: `char login[31]; char passwd[17]`) — plaintext, no key check
3. db `QUERY_LOGIN` (`ClientManagerLogin.cpp:395-426`): 13 columns in `SQL_ACCOUNT`: `SELECT mysql_hash_password('%s'), a.id, a.login, a.password, a.social_id, pi.empire, pi.pid1..pid5, a.status, a.lang FROM account a LEFT JOIN player.player_index pi ...` → `QID_LOGIN=4`
4. db `RESULT_LOGIN` (`ClientManagerLogin.cpp:428-510`): `HEADER_DG_LOGIN_NOT_EXIST=31` (0 rows), `HEADER_DG_LOGIN_WRONG_PASSWD=33`, `HEADER_DG_LOGIN_ALREADY=34`, or **`HEADER_DG_LOGIN_SUCCESS=30` + TAccountTable (444B)**; then QID_LOGIN again for character rows → merged into TAccountTable
5. game `login_success` (`input_db.cpp:140-186`): S→C **`GC_EMPIRE=90`** (2B, random 1..3 if no location) → PHASE_SELECT → **`GC_LOGIN_SUCCESS_NEWSLOT=0x20`** (449B, `desc.cpp:955-988`)
6. C→S: `CG_CHARACTER_SELECT=6` (2B) → `HEADER_GD_PLAYER_LOAD=3` → world entry

**Column contract** (`CreateAccountTableFromRes`, `ClientManagerLogin.cpp:259-297`): hash, id, login, password, social_id, bEmpire, pid1..pid5, status, lang — **the column order is load-bearing** (fix #7).

## 5. db↔game peer protocol (login subset)

- Framing: header BYTE + DWORD handle + payload (`EncodeHeader`/`Encode`/`EncodeReturn`; `DBPacket` in game).
- GD (game→db, `tables.h:10-137`): `HEADER_GD_LOGIN=1` (TLoginPacket), `HEADER_GD_PLAYER_LOAD=3`, `HEADER_GD_LOGIN_KEY=7`, `HEADER_GD_AUTH_LOGIN=100`, `HEADER_GD_LOGIN_BY_KEY=101`.
- DG (db→game, `tables.h:140-170`): `HEADER_DG_LOGIN_SUCCESS=30` (+TAccountTable), `HEADER_DG_LOGIN_NOT_EXIST=31`, `HEADER_DG_LOGIN_WRONG_PASSWD=33`, `HEADER_DG_LOGIN_ALREADY=34`, `HEADER_DG_EMPIRE_SELECT=49`, `HEADER_DG_DIRECT_ENTER=55`, `HEADER_DG_AUTH_LOGIN` (1 BYTE).
- QID (`db/src/QID.h`): QID_PLAYER=0, QID_ITEM=1, QID_LOGIN=4, QID_PLAYER_SAVE=13, QID_LOGIN_BY_KEY=16, QID_PLAYER_INDEX_CREATE=17; game-local (`db.h:15-20`): QID_AUTH_LOGIN=1.
- Note: the modern auth bypasses the classic `TPacketGDAuthLogin` handshake — the real flow is direct SQL via the game's DBManager with QID_AUTH_LOGIN; `QUERY_AUTH_LOGIN` only re-registers LoginData for LOGIN_BY_KEY.

## 6. graphify graph coordinates (for re-verification)

`command_login3` → `game/src/packet.h:507`; `CInputLogin::Analyze()` → `input_login.cpp:1023`; `CPacketInfo` → `packet_info.h:17`; `QUERY_LOGIN`/`RESULT_LOGIN` → `db/src/ClientManager.h:222-225`; `CInputDB::Boot()` → `input_db.cpp:474`; `SendLoginSuccessPacket` → `desc.cpp:955`; `TEA_Encrypt` → `libthecore/src/tea.c:282` (currently unused: plaintext).

## 7. Errata 2026-08-10 (verified against code and toolchains)

> Adversarial review of the `protocol` crate (oracle + fixer, 2026-08-10). Corrected in the body of this spec; this summary remains as the record. Empirical evidence: `gcc -m32` (server toolchain) and MSVC 14.51 x86 (client toolchain) compile `TSimplePlayer` = 71B; the client registers `sizeof(TPacketGCLoginSuccess4)` = 449 and login works in production.

| # | Original spec error | Reality (verified) | State |
|---|---|---|---|
| 1 | `TSimplePlayer` 76B natural (before pack) | **71B packed** (`tables.h:271` opens pack(1); struct at 285; closes at 1333) | FIXED §2/§3 |
| 2 | `TPacketGCLoginSuccess` 474B (handle@466) | **449B** (players@1, guild_id@356, guild_name@376, handle@441, random_key@445) | FIXED §3 |
| 3 | `TAccountTable` 472B | **444B** packed (4+31+17+19+9+8+1+5×71) | FIXED §2 |
| 4 | `TPacketGDAuthLogin` 100B | **110B** (sum of its own fields: 4+4+31+19+16+36) | FIXED §4a |
| 5 | Auth SQL "13 columns" | **15 columns** (`input_auth.cpp:207-218`, +7 premium + create_time) | FIXED §4a |
| 6 | — | `HEADER_GC_LOGIN_FAILURE=7`, `HEADER_GC_LOGIN_KEY=118` were missing from the §3 table | ADDED (crate) |
| 7 | — | Client bug `AccountConnector.cpp:113`: registers `GC_LOGIN_FAILURE` with `sizeof(TPacketGCAuthSuccess)` (6B) instead of 10B — the wire IS 10B; do NOT "fix" the crate to emit 6B | DOCUMENTED |

## 8. Legacy compatibility packets (151/152/153)

PanamaPack (151) and hybrid-crypt (152/153) are legacy-client-only constructs: the server sends them on every successful auth before `GC_AUTH_SUCCESS`, and the legacy client needs them to decrypt its pack entries. They are deliberately isolated in `protocol::legacy` and will be **deleted at F7** — see [legacy-compatibility.md](legacy-compatibility.md) for the packet inventory, layouts, why the legacy client requires them, and the deletion list.

**Out of F0 scope (pending for later phases):**
- **F1 (net):** keepalive/framing — `CG_TIME_SYNC` (0xfc, 13B = sizeof(TPacketCGHandshake)), `CG_PONG` (0xfe), `GC_PING` (44). The channel and the auth interleave them; Login3 parsing would fail if they were not filtered.
- **F2 (auth):** the 151/152/153 emission sequence (see §4a step 6 and the legacy reference). The `protocol` crate adds them under `protocol::legacy` in F2 (ADR-0006).
- **F2 (API):** `TPacketGCLoginSuccess` without a constructor (25 fields × 5 players), `TPacketCGPlayerCreate::new` fixes `index=0`; `from_bytes` does not validate `data[0]` (decision: the dispatch is done by `network`, F1).
