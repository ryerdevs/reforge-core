# Spec — Wire Protocol del flujo de login (contrato del crate Rust `protocol`)

> Extraído del servidor legacy C++ (`source\metin2_server`) el 2026-08-08 usando el grafo graphify (coordenadas abajo).
> Este documento es el contrato byte-exacto para el crate `protocol` de la reescritura Rust (ROADMAP Fase 0).
> Verificado contra: `Srcs\Server\game\src\packet.h`, `packet_info.cpp`, `input_auth.cpp`, `input_login.cpp`, `input_db.cpp`, `desc.cpp`, `Srcs\Server\common\tables.h`, `length.h`, `Srcs\Server\db\src\ClientManagerLogin.cpp`, `ClientManager.cpp`, `QID.h`.

## 1. Constantes

| Constante | Valor | Impacto en wire |
|---|---|---|
| `LOGIN_MAX_LEN` | 30 | buffers `[31]` |
| `PASSWD_MAX_LEN` | 16 | buffers `[17]` |
| `CHARACTER_NAME_MAX_LEN` | 24 | buffers `[25]` |
| `HANDSHAKE_RETRY_LIMIT` | 32 | `desc.h:17` |
| `PLAYER_PER_ACCOUNT` | 5 | `ENABLE_PLAYER_PER_ACCOUNT5` |
| `SOCIAL_ID_MAX_LEN` | 18 | `[19]` |
| `ACCOUNT_STATUS_MAX_LEN` | 8 | `[9]` |

Flags de build que cambian tamaños: `ENABLE_ACCE_COSTUME_SYSTEM` (+4B en TSimplePlayer), `ENABLE_QUIVER_SYSTEM` (+dwArrow), `ENABLE_SEQUENCE_SYSTEM` OFF (sin byte de secuencia), `USE_NO_PACKET_ENCRYPTION` ON (plaintext), `__LANGUAGE_SYSTEM__` (LOGIN3 en auth +3 bytes).

## 2. Reglas de framing

- **No hay prefijo de longitud.** Un paquete = header BYTE + payload fijo. Los tamaños vienen de tablas conocidas en cada lado.
- Cliente→servidor: tabla `CPacketInfoCG` (`packet_info.cpp:136-236`). Entradas clave (sequence OFF):
  - `0xff` Handshake = `sizeof(TPacketCGHandshake)` = 13
  - `0xfe` Pong = 1
  - `1` Login = 49; `109` Login2 = 52; **`111` Login3 = `sizeof(TPacketCGLogin3) + (auth ? 3 : 0)` = 65 en canal, 68 en auth** (sufijo `"es\0"`)
  - `6` PlayerSelect = 2; `4` Create = 34; `5` Delete = 10; `0xfc` TimeSync = 13
  - Header desconocido → `CPacketInfo::Get` false → conexión cerrada (`input.cpp:77-84`). Los paquetes de tamaño variable devuelven `iExtraLen` desde `Analyze` (`input.cpp:96-101`).
- Servidor→cliente: sin tabla; el server envía `desc->Packet(&struct, sizeof(struct))` — structs crudos.
- **Endianness:** little-endian nativo (x86).
- **Alineación:** `packet.h:304..2305` envuelve TODOS los structs de paquetes en `#pragma pack(1)` → byte-exactos, sin padding. **ERRATA 2026-08-10:** `TSimplePlayer`/`TAccountTable` (`tables.h:285-316`) NO se declaran antes del pack(1) — `tables.h:271` abre `#pragma pack(1)` y `tables.h:1333` lo cierra, así que ambos están DENTRO del pack → **TSimplePlayer = 71B packed, TPacketGCLoginSuccess = 449B, TAccountTable = 444B** (el cliente coincide: `UserInterface/Packet.h:355-356` + `TSimplePlayerInformation:1063` → `sizeof(TPacketGCLoginSuccess4)` = 449, verificado en producción). Ver sección «Erratas 2026-08-10».

## 3. Structs byte-exactos (todos packed, LE)

| Struct | Size | Layout |
|---|---|---|
| `TPacketCGHandshake` | 13 | `BYTE bHeader; DWORD dwHandshake; DWORD dwTime; long lDelta` |
| `TPacketCGLogin` | 49 | `BYTE header; char login[31]; char passwd[17]` |
| `TPacketCGLogin2` | 52 | `BYTE header; char login[31]; DWORD dwLoginKey; DWORD adwClientKey[4]` |
| `TPacketCGLogin3` | 65/68 | `BYTE header; char login[31]; char passwd[17]; DWORD adwClientKey[4]` + en auth `char szLanguage[3]` |
| `TPacketGCLoginKey` | 5 | `BYTE bHeader; DWORD dwLoginKey` |
| `TPacketCGPlayerSelect` | 2 | `BYTE header; BYTE index` |
| `TPacketCGPlayerDelete` | 10 | `BYTE header; BYTE index; char private_code[8]` |
| `TPacketCGPlayerCreate` | 34 | `BYTE header; BYTE index; char name[25]; WORD job; BYTE shape; BYTE Con; BYTE Int; BYTE Str; BYTE Dex` |
| `TPacketGCPhase` | 2 | `BYTE header; BYTE phase` (1 HANDSHAKE, 2 LOGIN, 3 SELECT, 4 LOADING, 5 GAME, 10 AUTH) |
| `TPacketGCHandshake` | 13 | igual que CG handshake |
| `TPacketGCAuthSuccess` | 6 | `BYTE bHeader; DWORD dwLoginKey; BYTE bResult` |
| `TPacketGCLoginFailure` | 10 | `BYTE header; char szStatus[9]` ("NOID","WRONGPWD","ALREADY","NOTAVAIL","BLKLOGIN","VERSION","FULL","SHUTDOWN") |
| `TPacketGCEmpire` | 2 | `BYTE bHeader; BYTE bEmpire` (1..3) |
| `TPacketGCLoginSuccess` | **449** (ERRATA: no 474) | `BYTE bHeader(=0x20=32)`; `TSimplePlayer players[5]` packed (offset 1, 355B); `DWORD guild_id[5]` (356, 20B); `char guild_name[5][13]` (376, 65B); `DWORD handle` (**441**); `DWORD random_key` (**445**) |
| `TPacketGCCharacterAdd` | 37 | `BYTE header; DWORD dwVID; float angle; long x,y,z; BYTE bType; DWORD wRaceNum; BYTE bMovingSpeed; BYTE bAttackSpeed; BYTE bStateFlag; DWORD dwAffectFlag[2]` |
| `TPacketGCCharacterAdditionalInfo` | 70 | `BYTE header; DWORD dwVID; char name[25]; DWORD awPart[5]; BYTE bEmpire; DWORD dwGuildID; DWORD dwLevel; short sAlignment; BYTE bPKMode; DWORD dwMountVnum; DWORD dwArrow` |

`TSimplePlayer` (**71B packed, ERRATA: no 76B**, `tables.h:285` DENTRO del pack(1) de `tables.h:271`, ACEE+QUIVER ON): `DWORD dwID`(0); `char szName[25]`(4); `BYTE byJob`(29); `BYTE byLevel`(30); `DWORD dwPlayMinutes`(31); `BYTE byST,byHT,byDX,byIQ`(35,36,37,38); `DWORD wMainPart`(39); `BYTE bChangeName`(43); `DWORD wHairPart`(44); `DWORD wAccePart`(48); `BYTE bDummy[4]`(52); `long x,y`(56,60); `long lAddr`(64); `WORD wPort`(68); `BYTE skill_group`(70). Suma: 4+25+1+1+4+4+1+4+1+4+4+4+4+4+4+2+1 = 71. Los offsets del layout original (47,51,55,59,63,67,71,73,75) eran erróneos.

## 4. Máquina de estados del login

**(a) Auth :30001 (CInputAuth, `input_auth.cpp:223-257`):**
1. S→C: `GC_PHASE=0xfd` (PHASE_HANDSHAKE) + `GC_HANDSHAKE=0xff` (retries hasta 32)
2. C→S: `CG_HANDSHAKE=0xff` (13B) → phase PHASE_AUTH
3. C→S: `CG_LOGIN3=111` (68B con lang) → `CInputAuth::Login` (`input_auth.cpp:66`) → `ReturnQuery(QID_AUTH_LOGIN, ...)` — SQL **15 columnas** (ERRATA: no 13, `input_auth.cpp:207-218`): `mysql_hash_password('%s'), password, securitycode, social_id, id, status, availDt-NOW()>0, 7×UNIX_TIMESTAMP(...), create_time` (col0 = hash con `*`, strcmp contra almacenado — `db.cpp:340`)
4. Resultado en `DBManager::AnalyzeReturnQuery` (`db.cpp:229-396`): 0 filas → `GC_LOGIN_FAILURE` "NOID"; hash mismatch → "WRONGPWD"; si no → `SendAuthLogin` (`db.cpp:179-202`, `HEADER_GD_AUTH_LOGIN=100`, `TPacketGDAuthLogin` **110B** (ERRATA: no 100 — 4+4+31+19+16+36=110, `tables.h:987-995` con `iPremiumTimes[9]`): `DWORD dwID; DWORD dwLoginKey; char szLogin[31]; char szSocialID[19]; DWORD adwClientKey[4]; int iPremiumTimes[9]`)
5. db `QUERY_AUTH_LOGIN` (`ClientManager.cpp:1854-1901`) registra CLoginData → `HEADER_DG_AUTH_LOGIN` (1 BYTE result)
6. game `CInputDB::AuthLogin` (`input_db.cpp:1697-1728`) → C→S: **`GC_AUTH_SUCCESS=150`** (6B) → el cliente cierra auth y conecta al canal

**(b) Canal :30003 (CInputLogin, `input_login.cpp:1023-1119`):**
1. S→C: `GC_PHASE=0xfd` (PHASE_LOGIN)
2. C→S: `CG_LOGIN3=111` (65B, sin lang) → `CInputLogin::Login` (`input_login.cpp:97-147`) → `HEADER_GD_LOGIN=1` `TLoginPacket` (48B: `char login[31]; char passwd[17]`) — plaintext, sin chequeo de key
3. db `QUERY_LOGIN` (`ClientManagerLogin.cpp:395-426`): 13 columnas en `SQL_ACCOUNT`: `SELECT mysql_hash_password('%s'), a.id, a.login, a.password, a.social_id, pi.empire, pi.pid1..pid5, a.status, a.lang FROM account a LEFT JOIN player.player_index pi ...` → `QID_LOGIN=4`
4. db `RESULT_LOGIN` (`ClientManagerLogin.cpp:428-510`): `HEADER_DG_LOGIN_NOT_EXIST=31` (0 filas), `HEADER_DG_LOGIN_WRONG_PASSWD=33`, `HEADER_DG_LOGIN_ALREADY=34`, o **`HEADER_DG_LOGIN_SUCCESS=30` + TAccountTable (472B)**; luego QID_LOGIN de nuevo para filas de personajes → merge en TAccountTable
5. game `login_success` (`input_db.cpp:140-186`): S→C **`GC_EMPIRE=90`** (2B, random 1..3 si no hay location) → PHASE_SELECT → **`GC_LOGIN_SUCCESS_NEWSLOT=0x20`** (474B, `desc.cpp:955-988`)
6. C→S: `CG_CHARACTER_SELECT=6` (2B) → `HEADER_GD_PLAYER_LOAD=3` → entrada al mundo

**Contrato de columnas** (`CreateAccountTableFromRes`, `ClientManagerLogin.cpp:259-297`): hash, id, login, password, social_id, bEmpire, pid1..pid5, status, lang — **el orden de columnas es load-bearing** (fix #7).

## 5. Protocolo peer db↔game (subconjunto login)

- Framing: header BYTE + DWORD handle + payload (`EncodeHeader`/`Encode`/`EncodeReturn`; `DBPacket` en game).
- GD (game→db, `tables.h:10-137`): `HEADER_GD_LOGIN=1` (TLoginPacket), `HEADER_GD_PLAYER_LOAD=3`, `HEADER_GD_LOGIN_KEY=7`, `HEADER_GD_AUTH_LOGIN=100`, `HEADER_GD_LOGIN_BY_KEY=101`.
- DG (db→game, `tables.h:140-170`): `HEADER_DG_LOGIN_SUCCESS=30` (+TAccountTable), `HEADER_DG_LOGIN_NOT_EXIST=31`, `HEADER_DG_LOGIN_WRONG_PASSWD=33`, `HEADER_DG_LOGIN_ALREADY=34`, `HEADER_DG_EMPIRE_SELECT=49`, `HEADER_DG_DIRECT_ENTER=55`, `HEADER_DG_AUTH_LOGIN` (1 BYTE).
- QID (`db/src/QID.h`): QID_PLAYER=0, QID_ITEM=1, QID_LOGIN=4, QID_PLAYER_SAVE=13, QID_LOGIN_BY_KEY=16, QID_PLAYER_INDEX_CREATE=17; game-local (`db.h:15-20`): QID_AUTH_LOGIN=1.
- Nota: el auth moderno bypasea el handshake `TPacketGDAuthLogin` clásico — el flujo real es SQL directo vía el DBManager del game con QID_AUTH_LOGIN; `QUERY_AUTH_LOGIN` solo re-registra LoginData para LOGIN_BY_KEY.

## 6. Coordenadas del grafo graphify (para re-verificación)

`command_login3` → `game/src/packet.h:507`; `CInputLogin::Analyze()` → `input_login.cpp:1023`; `CPacketInfo` → `packet_info.h:17`; `QUERY_LOGIN`/`RESULT_LOGIN` → `db/src/ClientManager.h:222-225`; `CInputDB::Boot()` → `input_db.cpp:474`; `SendLoginSuccessPacket` → `desc.cpp:955`; `TEA_Encrypt` → `libthecore/src/tea.c:282` (sin uso actual: plaintext).

---

## 7. Erratas 2026-08-10 (verificadas contra código y toolchains)

> Revisión adversarial del crate `protocol` (oracle + fixer, 2026-08-10). Corregido en el cuerpo del spec; este resumen queda como registro. Evidencia empírica: `gcc -m32` (toolchain server) y MSVC 14.51 x86 (toolchain cliente) compilan `TSimplePlayer` = 71B; el cliente registra `sizeof(TPacketGCLoginSuccess4)` = 449 y el login funciona en producción.

| # | Error del spec original | Realidad (verificada) | Estado |
|---|---|---|---|
| 1 | `TSimplePlayer` 76B natural (antes del pack) | **71B packed** (`tables.h:271` abre pack(1); struct en 285; cierra en 1333) | CORREGIDO §2/§3 |
| 2 | `TPacketGCLoginSuccess` 474B (handle@466) | **449B** (players@1, guild_id@356, guild_name@376, handle@441, random_key@445) | CORREGIDO §3 |
| 3 | `TAccountTable` 472B | **444B** packed (4+31+17+19+9+8+1+5×71) | CORREGIDO §2 |
| 4 | `TPacketGDAuthLogin` 100B | **110B** (suma de sus propios campos: 4+4+31+19+16+36) | CORREGIDO §4a |
| 5 | SQL del auth "13 columnas" | **15 columnas** (`input_auth.cpp:207-218`, +7 premium + create_time) | CORREGIDO §4a |
| 6 | — | `HEADER_GC_LOGIN_FAILURE=7`, `HEADER_GC_LOGIN_KEY=118` faltaban en la tabla §3 | AÑADIDO (crate) |
| 7 | — | Bug del cliente `AccountConnector.cpp:113`: registra `GC_LOGIN_FAILURE` con `sizeof(TPacketGCAuthSuccess)` (6B) en vez de 10B — el wire ES 10B; no "corregir" el crate para emitir 6B | DOCUMENTADO |

**Fuera de alcance F0 (pendientes para fases):**
- **F1 (net):** keepalive/framing — `CG_TIME_SYNC` (0xfc, 13B = sizeof(TPacketCGHandshake)), `CG_PONG` (0xfe), `GC_PING` (44). El canal y el auth los intercalan; el parseo de Login3 fallaría si no se filtran.
- **F2 (auth):** `TPacketGCPanamaPack` (151, 289B: header + szPackName[256] + abIV[32] XOR-eado con la panama key — `panama.cpp:83-87`) y hybrid-crypt 152/153 se envían en TODO auth exitoso ANTES de `GC_AUTH_SUCCESS` (`input_db.cpp:1710-1717`); el cliente los espera incondicionalmente (`AccountConnector.cpp:119-126`). El crate `protocol` debe añadirlos en F2.
- **F2 (API):** `TPacketGCLoginSuccess` sin constructor (25 campos × 5 jugadores), `TPacketCGPlayerCreate::new` fija `index=0`; `from_bytes` no valida `data[0]` (decisión: el dispatch lo hace net, F1).
