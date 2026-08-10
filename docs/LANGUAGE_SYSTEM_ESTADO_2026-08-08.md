# Language System 1.2.6 — Estado de integración (sesión 2026-08-08)

> **Propósito de este archivo:** registrar TODO lo hecho y TODOS los problemas pendientes de la integración del mod "Language System 1.2.6" en nuestro servidor Metin2 (MartySama 5.9) + cliente (S3llMetin2 v24), para retomarlo en una nueva sesión con contexto fresco. Leer también `AGENTS.md` (reglas del proyecto) y `CHANGELOG.md`.

---

## 1. QUÉ ES EL MOD Y DÓNDE ESTÁ

- **Carpeta:** `C:\projects\Metin2\systems\Language System 1.2.6\Language System 1.2.6\`
- Es un mod de Metin2 (base 2017) que **mezcla DOS sistemas**:
  1. **Motor de multilenguaje (LO QUE QUEREMOS):** traducción de textos del servidor por idioma del jugador.
  2. **Coliseo PVP (NO QUEREMOS):** mapa `metin2_map_colosseum`, PK_MODE_TEAM_A/B, paquete GC de score, `CLanguageSystem` (su clase `locale_server.cpp` **NO viene en el paquete** — solo se referencia en el Makefile).

### Contenido del paquete
| Ruta | Contenido |
|---|---|
| `01. Server\locale\germany\` | `locale_string.txt` + `locale_string_{cz,de,en,es,hu,it,pl,pt,ro,tr}.txt` + `translate.lua` (dofile de TODOS los idiomas) + `translate_{cz,de,en,es,hu,it,pl,pt,ro,tr}.lua` + `translate_defaults.lua` |
| `02. Client\root\*.py` | Parches de pack — **TODOS del coliseo** (uiLanguageSystem, wndLanguageSystem, AddLanguageSystemResultWindow, NAMECOLOR teams...) → NO integrar |
| `03. Source\01. Client\` | Parches — mayoría coliseo (InstanceBase, PythonPlayerModule, NAMECOLOR...) → NO integrar |
| `03. Source\02. Server\` | **LO INTERESANTE:** `locale.cpp` completo (744 líneas, motor multi-idioma), `locale_service.cpp/h`, `locale.hpp`, parches de char.cpp/char_battle/char_item/cmd_general/input_login/main/Makefile/packet.h/pvp/questlua/sectree_manager |
| `04. SQL\account.sql` | Tabla account con columna **`lang varchar(4)`** (default 'de') |
| `char.cpp` (185 KB) | char.cpp COMPLETO de la base del mod (referencia de cómo quedan los LC_TEXT convertidos) |

### Motor del mod (referencia: `03. Source\02. Server\game\locale.cpp`)
- `std::string arstLocaleStringNames[LANGUAGE_MAX_NUM+1]` — nombres de idiomas
- `LocaleStringMapType localeString[LANGUAGE_MAX_NUM]` — un map por idioma
- `locale_clear()`, `locale_add(lang, base, trad)`, `locale_init(lang, filename)` (pares `"clave" "valor"`)
- `locale_find(lang, string)` — busca en el idioma → fallback LANGUAGE_DEFAULT → fallback string
- `locale_quest_find`, `locale_find_new`, `locale_find_special` (con `[item=vnum]`, `[mob=vnum]`, `[empire=N]`)
- Macros: `LC_TEXT(lang_type, str)`, `LC_QUEST_TEXT`, `LC_TEXT_NEW`, `LCS_TEXT`
- `locale_service.cpp`: `Locale_LoadLocaleStringFile()` carga `base/<lang>/locale_string.txt` por idioma, o desde BD `common.locale_string` (columnas lang_base, lang_cz...)
- **FALTAN en el paquete:** `LANGUAGE_MAX_NUM`, `LANGUAGE_DEFAULT`, valores de `arstLocaleStringNames`, `CLanguageSystem`, `locale_server.cpp` → hay que definirlos nosotros.

---

## 2. NUESTRA BASE (verificada con recon exp-3/exp-4)

### Servidor (`source\metin2_server\Srcs\Server\`)
- `game\src\locale_service.cpp` (1355 líneas): `LocaleService_Init(serviceName)` (no existe `Locale_Init`), `g_stServiceBasePath = "locale/<name>"`, `g_stLocaleFilename` (spain → `"locale/spain/locale_string.txt"`), `__LocaleService_Init_spain()` en ~584-594 (setea `g_stLocaleFilename` línea 593), `LocaleService_GetBasePath()` ~1198.
- `game\src\locale.hpp:11`: `#define LC_TEXT(str) locale_find(str)` **dentro de un bloque `extern "C"`** (¡no se puede sobrecargar en C!).
- `game\src\locale.cpp` (212 líneas): map único `localeString`, `locale_find(str)` con `g_iUseLocale` y fallback `"@0949"+string`, `locale_init(filename)`, helpers `quote_find_end`/`locale_convert`.
- **1182 call sites de `LC_TEXT(str)` en 49 archivos** (char_item 317, cmd_general 186, char 107, input_main 93, questlua_monarch 57, arena 51, char_skill 45, cmd_gm 43, guild 32...). `LC_TEXT_NEW`/`LC_QUEST_TEXT` NO existen.
- `common\CommonDefines.h`: defines de features (service.h es wrapper). Sin rastro de language system.
- `common\tables.h`: `TAccountTable` (306-315): id, login[31], passwd[17], social_id[14], status[8], bEmpire, players[] — packed, viaja crudo db→game.
- Login activo: cliente → auth(30001) LOGIN3 65 bytes → `CInputAuth::Login` (input_auth.cpp ~66-120) → QID_AUTH_LOGIN (16 col) → db QUERY_AUTH_LOGIN (ClientManager.cpp ~1854-1898) → DG_AUTH_LOGIN. Canal: case HEADER_CG_LOGIN3 → Login → GD_LOGIN → db QUERY_LOGIN (ClientManagerLogin.cpp:373+, 12 col, LEFT JOIN player_index con empire+pid1..5) → RESULT_LOGIN → DG_LOGIN_SUCCESS + TAccountTable → `CInputDB::LoginSuccess` (input_db.cpp:104-165, BindAccountTable en 144).
- `g_bAuthServer` (config.cpp:75 default false; se pone true con token `auth_server` en CONFIG, config.cpp:692). **core1 NO tiene AUTH_SERVER en su CONFIG; auth1 SÍ.**
- Makefile game\src: locale.cpp (142) y locale_service.cpp (143) ya compilados. NO añadir .cpp nuevos.
- Runtime activo: **LOCALE=spain** (tabla `locale` de BD común). `share/locale/spain/` **YA CONTIENE** los 16 `locale_string_{AE,CZ,DE,DK,EN,ES,FR,GR,HU,IT,NL,PL,PT,RO,RU,TR}.txt` (64-72 KB) + 16 `translate_*.lua` + `settings.lua` + `quest\locale.lua` + `locale_string.txt`. ¡Los datos ya están desplegados!
- db: `g_stLocaleNameColumn` (default "name", fijado "locale_name" por locale) — precedente de columna configurable.

### Cliente (`source\metin2_client\Srcs\Client\`)
- `TPacketCGLogin3` (Packet.h:479-485): header 0x6F + name[31] + pwd[17] + adwClientKey[16] = **65 bytes**, packed. Registrado con sizeof en packet_info.cpp:152.
- El LOGIN3 se manda en 2 sitios: **AUTH** (AccountConnector.cpp:157-178) y **CANAL** (PythonNetworkStreamPhaseLogin.cpp:234-257 SendLoginPacket y :259-293 SendLoginPacketNew).
- `LocaleService_GetLocaleName()` → "es" (Locale.cpp). El cliente conoce su idioma.
- Headers GC **139-149 libres** en ambos lados (cliente Packet.h:277-282 y servidor packet.h:245-255).
- **DIVERGENCIA CRÍTICA detectada:** la copia Windows de `input_login.cpp` NO tenía el `case HEADER_CG_LOGIN3` que WSL SÍ tiene (1045-1047). Se sincronizó WSL→Windows durante esta sesión (ver §4).

---

## 3. DECISIONES DE DISEÑO TOMADAS (adaptación, NO copia literal)

1. **Definir el enum nosotros** (16 idiomas del runtime): `LANGUAGE_AE=0..LANGUAGE_TR=15`, `LANGUAGE_MAX_NUM`, `LANGUAGE_DEFAULT=LANGUAGE_ES`.
2. **Nombres con sufijo `_lang`** (`locale_find_lang`, `locale_init_lang`, `locale_add_lang`, `locale_find_new_lang`) FUERA del bloque `extern "C"` — evita conflicto con las funciones C single-arg existentes.
3. **`LC_TEXT(str)` intacto (1182 call sites sin tocar):** `locale_find(const char*)` es ahora un wrapper → `locale_find_lang(g_iCurrentLang, string)`. Nuevas macros `LC_TEXT_LANG(lang, str)` y `LC_TEXT_NEW_LANG`.
4. **Contexto de idioma por jugador (el truco clave):** global `BYTE g_iCurrentLang = LANGUAGE_DEFAULT`; se setea en `CInputMain::Analyze` (input_main.cpp ~2959) al inicio de cada paquete: `g_iCurrentLang = d->GetLang()`. Así TODO el procesamiento de un paquete traduce al idioma de ese jugador. Limitación documentada: mensajes de timers/broadcast usan el último idioma procesado (aceptable; usar LC_TEXT_LANG explícito si importa).
5. **`account.lang`** como fuente del idioma (varchar(4), default 'es'), propagado auth→db→game vía `TAccountTable.lang[8]`.
6. **Cliente manda su idioma al AUTH** con 3 bytes extra al final del LOGIN3 (`szLanguage[3]` = "es"), total 68 bytes. **El CANAL sigue mandando 65 bytes** (el servidor registra `sizeof(TPacketCGLogin3) + (g_bAuthServer ? 3 : 0)`).
7. **Carga por archivo** (no BD): `LocaleService_GetBasePath() + "/locale_string_" + NOMBRE_MAYÚSCULAS + ".txt"` — coincide con el runtime (`locale_string_ES.txt`).
8. **NO integrar el coliseo PVP** (PK_MODE_TEAM_A/B, mapa colosseum, CLanguageSystem, paquetes GC nuevos, parches del pack del cliente).

---

## 4. LO QUE YA ESTÁ IMPLEMENTADO (estado al cierre de sesión)

### Cliente — COMPLETADO y COMPILADO (fix-1, sesión ses_01d88c6b1ffeSBqcNx1FVQDBAm)
- `UserInterface\Packet.h:485`: `char szLanguage[3];` al final de `TPacketCGLogin3` (68 bytes) con comentario "Language System".
- `UserInterface\AccountConnector.cpp:174-180` (`__AuthState_RecvPhase`): rellena `szLanguage` con `LocaleService_GetLocaleName()` (fallback `'e','s','\0'`), envía `sizeof(LoginPacket)` = 68 bytes.
- `UserInterface\PythonNetworkStreamPhaseLogin.cpp:250-251 y 275-277` (CANAL): envía `sizeof(LoginPacket) - sizeof(LoginPacket.szLanguage)` = 65 bytes (conserva protocolo del canal; comentario explicativo).
- **Build Release|Win32 OK** (solo warnings C4267 preexistentes). Exe: 5.115.392 bytes, 18:36:47. **COPIA a `client\metin2client.exe` HECHA** (5115392 bytes, 18:36:47).

### Servidor — COMPLETADO y COMPILADO (fix-2, sesión ses_01cb5e047ffeQIcUy4zecdyoOb)
12 archivos modificados en `source\metin2_server\Srcs\Server\` (sincronizados a WSL vía `\\wsl$`):
1. `common\CommonDefines.h:63`: `#define __LANGUAGE_SYSTEM__`
2. `common\tables.h:313`: `TAccountTable` + `char lang[8];` (entre status y bEmpire)
3. `game\src\locale.hpp`: enum LANGUAGE_AE..TR + LANGUAGE_MAX_NUM + `#define LANGUAGE_DEFAULT LANGUAGE_ES`, `extern BYTE g_iCurrentLang`, `extern std::string arstLocaleStringNames[LANGUAGE_MAX_NUM+1]`, decls `locale_clear/locale_add_lang/locale_init_lang/locale_find_lang/locale_find_new_lang` FUERA de extern "C", macros `LC_TEXT_LANG`/`LC_TEXT_NEW_LANG`. `LC_TEXT(str)` y funciones C intactas.
4. `game\src\locale.cpp`: motor — `g_iCurrentLang` (17), `arstLocaleStringNames[17]` = {AE,CZ,DE,DK,EN,ES,FR,GR,HU,IT,NL,PL,PT,RO,RU,TR,""} (20-24), `localeString_lang[16]` (26), `locale_clear` (30), `locale_add_lang` (42-45, inserta si no existe, SIN queries BD), `locale_find_lang` (48+, preserva early-return `!g_iUseLocale||LC_IsKorea()||LC_IsWE_Korea()`; fallback idioma→default→"@0949"+string con sys_err), `locale_find_new_lang` (vsnprintf), `locale_init_lang` (parser compartido reutilizando quote_find_end/locale_convert). `locale_find(const char*)` = wrapper → `locale_find_lang(g_iCurrentLang, string)` (106-110). Legacy `localeString`/`locale_add`/`locale_init` intactos.
5. `game\src\locale_service.cpp:400-424`: `LocaleService_LoadLocaleStringFile()` — bajo `#ifdef __LANGUAGE_SYSTEM__` bucle 0..15 cargando `LocaleService_GetBasePath() + "/locale_string_" + arstLocaleStringNames[i] + ".txt"` con `sys_log(0, "Load LocaleString %s", ...)` por archivo; legacy en `#else`. **OJO: la función tiene early returns `if (g_stLocaleFilename.empty()) return;` (402) y `if (g_bAuthServer) return;` (405).**
6. `game\src\packet_info.cpp`: registro de `HEADER_CG_LOGIN3` → `sizeof(TPacketCGLogin3) + (g_bAuthServer ? 3 : 0)` (auth consume 68, canal 65).
7. `game\src\input_auth.cpp` `CInputAuth::Login`: lee 3 bytes extra del buffer (`c_pData + sizeof(TPacketCGLogin3)`), valida 2 letras+'\0', normaliza minúsculas, `DBManager::instance().DirectQuery("UPDATE account SET lang='%s' WHERE login='%s'")` con EscapeString (síncrono para evitar race con el QUERY_LOGIN del canal; error solo logueado = degradación suave).
8. `db\src\ClientManagerLogin.cpp`: `QUERY_LOGIN` con `a.lang` (13ª columna) en el SELECT; `CreateAccountTableFromRes` parsea `pkTab->lang`; `QUERY_LOGIN_BY_KEY` con `SELECT lang FROM account` (camino alternativo consistente).
9. `game\src\desc.h` / `desc.cpp`: `BYTE m_bLang` bajo `#ifdef __LANGUAGE_SYSTEM__` + `GetLang()`/`SetLang(BYTE)`, inicializado a `LANGUAGE_DEFAULT` en `DESC::Initialize()`.
10. `game\src\input_db.cpp` `CInputDB::LoginSuccess`: tras BindAccountTable (~144), mapea `pTab->lang` a índice del enum (strcasecmp contra arstLocaleStringNames; no encontrado → LANGUAGE_DEFAULT), `d->SetLang(bLang)`, `g_iCurrentLang = d->GetLang()` (162).
11. `game\src\input_main.cpp` `CInputMain::Analyze` (~2959): al inicio de cada paquete `g_iCurrentLang = d->GetLang();` con comentario de limitación.
12. `source\metin2_svfiles\sql\account.sql`: comentario final con el ALTER preparado.

### SQL — EJECUTADO (verificado)
```sql
ALTER TABLE account ADD COLUMN lang varchar(4) NOT NULL DEFAULT 'es' AFTER status;
-- Verificado: SHOW COLUMNS → lang varchar(4) NO es; cuenta test = 'es'
```

### Builds y deploy — HECHOS (WSL, 19:14-19:18)
- `make -C game/src` → **OK 0 errores** → `game_r41023` (54.7 MB, md5 `98788379e17b331015858a42760ccf8b`)
- `make -C db/src` → **OK 0 errores** → `db_r41023` (10.6 MB, md5 `e3ca154cd2f8cf3725ebc7f89d06f7b7`)
- Deploy a `main\srv1\share\bin\{game,db}` + `sync` + md5 verificado (los binarios desplegados coinciden).
- Stack reiniciado 19:18: db (22135), auth1 (22146), core1 (22152); puertos 30000-30004 OK; mariadb corriendo.
- **Verificado que el core1 corre el binario nuevo:** `readlink /proc/22152/exe → share/bin/game`, md5 coincide, `strings` del binario contiene "Load LocaleString %s", `nm` muestra `locale_find_lang T`, `locale_init_lang T`, `g_iCurrentLang D`.

---

## 5. ✅ PROBLEMA PRINCIPAL RESUELTO (sesión 2, 2026-08-08): EL MOTOR SÍ CARGABA — ERA UN FALSO NEGATIVO DEL LOG

### Síntoma original
El boot del core1 (19:18:17) no mostraba las 16 líneas "Load LocaleString" en `syslog` ni en `stdout`.

### Causa raíz (diagnóstico definitivo)
1. **`sys_log` es invisible en el boot temprano:** `LocaleService_LoadLocaleStringFile()` se llama desde `config_init` (config.cpp:1492), que corre ANTES de `thecore_init` (main.cpp:530) → el logfile `syslog` aún no está abierto.
2. **`sys_log` a stdout está condicionado a `log_level_bits > 1`** (libthecore/src/log.c:209) — y el CONFIG del core tiene `DB_LOG_LEVEL: 1` → `log_set_level(1)` → `log_level_bits = 1` → el `fprintf(stdout)` interno de `sys_log` NUNCA se ejecuta en todo el boot.
3. El motor cargaba los 16 idiomas desde el 19:18; solo las líneas de evidencia se perdían en el vacío. El diagnóstico de la sesión anterior buscaba en los sitios equivocados.

### Fix aplicado (3 archivos × 2 copias WSL/Windows, md5 sincronizados)
- `locale.cpp`: `locale_init_file` y `locale_init_lang` ahora devuelven `int` = nº de entradas cargadas (0 = archivo ausente/vacío).
- `locale.hpp`: firma de `locale_init_lang` → `int`.
- `locale_service.cpp`: el bucle imprime `fprintf(stdout, "Load LocaleString %s (%d entries)\n", ...)` — incondicional y visible en boot (estilo de los demás mensajes de `config_init`). El `sys_log` se eliminó porque era silencioso en ese punto.

### Evidencia (boot 20:31, pid 23792, binario game_r41023 20:29)
```
Load LocaleString locale/spain/locale_string_AE.txt (774 entries)
Load LocaleString locale/spain/locale_string_CZ.txt (764 entries)
Load LocaleString locale/spain/locale_string_DE.txt (764 entries)
... EN (775), ES (764), ... TR (764)  → 16 líneas, 16/16 con conteos > 0
```
- `LOCALE_ERROR` = 0 en el boot nuevo (sin fallos de resolución de claves).
- `LoadSettings/LoadTranslate/LoadQuestLocale` = 0 (los fixes CP949 del día siguen vigentes).
- Las líneas van a `core1/stdout`, NO a `syslog` (por diseño — ver causa raíz). El harness `m2_check_lang.sh` quedó obsoleto: buscar en `stdout`, no `syslog`.

### Hipótesis descartadas de la sesión anterior
- No es orden de inicialización (la llamada está en `config_init` sin condicionales; `LocaleService_Init` corre antes, línea 492, y setea `g_stLocaleFilename`).
- No es `g_bAuthServer` (core1: false, confirmado por "LogSQL connected").
- No es falta de archivos (16 presentes).

---

## 6. OTROS PENDIENTES CONOCIDOS

1. ~~**Logs de depuración del db (ruidosos):** `DBG_AQR` (~ClientManager.cpp:2637), `DBG_RESULT_LOGIN` (~ClientManagerLogin.cpp:408), `DBG_PARSE` (~ClientManagerLogin.cpp:266) — "uno cada 5s por el item award refresh". AGENTS.md los marca para limpiar: quitar esos `sys_log`, rebuild db, deploy.~~ **✅ HECHO (sesión 2):** eliminados los 3 `sys_log` (ClientManager.cpp y ClientManagerLogin.cpp, ambas copias sincronizadas), rebuild db (`db_r41023` 20:31), deploy + restart. Verificado: boot nuevo del db con **0** líneas DBG (grep en sección `Start of pid: 23772`), el item award ahora loguea limpio (`return 0/0/0 async 0/0/0`), `strings` del binario desplegado ya no contiene `DBG_AQR`.
2. **Verificación end-to-end del idioma:** una vez el motor cargue, probar en el cliente real: cuenta `test`/`1234` con `lang='es'` (ya está) → login → ver textos en español; luego `UPDATE account SET lang='en'` y verificar que el servidor emite textos en inglés (los del cliente los traduce el pack). El cliente manda su idioma en cada LOGIN3 al auth → sobrescribirá `account.lang` con el idioma del pack del cliente (locale.cfg).
3. **Selector de idioma en el login (petición del usuario del turno anterior):** "columna de banderas arriba del login" — pendiente de diseño/implementación (pack: intrologin.py + imágenes de banderas; ver conversación previa sobre `locale.cfg`/selector Win32 `IDD_SELECT_LOCALE` con `LOCALE_SERVICE_GLOBAL`). **NO confundir con el coliseo del mod.**
4. ~~**NPCs multilenguaje:** el servidor pinta nombres de NPCs desde MySQL (`szLocaleName`) — con `DB_NAME_COLUMN=locale_name` es una sola columna (español actualmente). Para multilenguaje real de NPCs haría falta override client-side o columnas por idioma (ver AGENTS.md §17 — el cliente ya traduce mobs/items desde su pack; los NPCs usan el nombre del servidor).~~ **✅ RESUELTO (2026-08-09):** fix client-side — `RecvCharacterAdditionalInfo` (`PythonNetworkStreamPhaseGameActor.cpp`) usa `CPythonNonPlayer::GetName(race)` del pack del cliente para `TYPE_NPC` (fallback al nombre del servidor). Rebuild Release|Win32 5.115.904 bytes (12:35). Los NPCs ahora cambian de idioma con el cliente, igual que mobs/items. `TYPE_PC` intacto (nombres de jugadores = servidor).
5. **Sincronización de copias:** la copia Windows de `input_login.cpp` estaba desactualizada (sin case LOGIN3); fix-3 sincronizó WSL→Windows (21 archivos, 8/8). **Regla AGENTS.md #6: tras cualquier cambio de source, sincronizar AMBAS copias y verificar defines de protocolo coincidentes.**
6. **WSL inestable** (4GB RAM, crashes E_UNEXPECTED): tras crashes → `wsl --shutdown` → `start_m2_min.sh`. Revisar IP `172.25.104.175` (eth0) tras reinicios. Verificar árbol real del runtime en WSL antes de diseñar deploys (la copia Windows de svfiles puede estar desincronizada).

---

## 7. CÓMO RETOMAR (checklist de la próxima sesión)

1. Leer este archivo + `AGENTS.md` + `CHANGELOG.md`.
2. Verificar estado del stack: `wsl -d Debian-M2 -- bash /home/m2/check_lang_boot.sh` (o `start_m2_min.sh` si está caído).
3. ~~Resolver el problema del §5~~ — **RESUELTO (sesión 2)**: el motor carga 16/16 idiomas (evidencia en `core1/stdout`, NO en syslog).
4. **Prueba end-to-end PARCIAL (sesión 2):** textos del servidor en español ✓ en el cliente real (monster_chat ✓, sistema ✓). Falta: probar otro idioma (cambiar locale del cliente o el envío — el cliente sobrescribe account.lang) y verificar a fondo NPCs/quests/items. **OJO: crash intermitente de entrada al mundo (~75%) NO resuelto — ver §8.**
5. ~~Limpiar logs DBG_* del db~~ — **HECHO (sesión 2)**, ver §6.1.
6. Pendientes abiertos: selector de idioma en el login (columna de banderas, pack intrologin.py), NPCs multilenguaje (necesita columnas por idioma o override client-side), revisar los 17 SYSERR del boot (dragon_soul_table.txt ausente, motions yamachun — preexistentes, no relacionados).
7. Registrar todo en `CHANGELOG.md` + actualizar `ROADMAP.md` (regla AGENTS.md #9) al cerrar.

---

## 8. RESULTADO END-TO-END (sesión 2, 2026-08-08) + INVESTIGACIÓN DEL CRASH DE ENTRADA

### Language System — verificado en el cliente real (PARCIAL)
- El usuario jugó en el mundo (mapa 41, Venter_the_east): **los textos del servidor salen en español** (chat de monstruos, mensajes de sistema) — el motor traduce con la tabla ES cargada (764 entradas).
- `InputDB::login_success: lang 'es' -> 5 for test` — propagación account.lang → índice de idioma → `g_iCurrentLang` funcionando.
- El cliente (build 18:36, 5.115.392 bytes) envía su idioma en el LOGIN3 del auth (68 bytes: 65 + `szLanguage[3]`="es") → auth: `InputAuth::Login : lang es for test` → `UPDATE account SET lang='es'`. **El cliente sobrescribe account.lang en cada login** — para probar otro idioma del servidor hay que cambiar el locale del cliente (locale.cfg) o el envío, no solo la BD.
- `monster_chat` en español ✓ (el fix CP949 del quest/locale.lua sigue vigente).

### Crash de entrada al mundo (0xc0000374) — lo que pasó y lo que se cree
- **Firma:** `STATUS_HEAP_CORRUPTION` en ntdll (offset 0x000e6dc3), idéntica en WER desde las 15:00 (build cliente de las ~14:47, ANTES de los cambios LS) → el crash NO lo causó el Language System; el "entrada al mundo verificada" anterior era evidencia del lado servidor.
- **Causa determinista (RESUELTA):** los personajes estaban en BD con coordenadas basura `(960155, 269313)` en el mapa 41 (un harness anterior escribió valores ~100x fuera; aldea real = `(969600, 278400)` en UNIDADES). El cliente crasheaba calculando tiles fuera de rango. Fix: `UPDATE player SET x=969600, y=278400`. Tras el fix: entradas exitosas y juego normal.
- **Causa intermitente (NO RESUELTA):** con coordenadas válidas, ~75% de las entradas crashean ~8-17s después de `player_load` (la misma posición funcionó a las 20:50 y crasheó a las 21:05; entradas OK a las 21:14). El servidor está limpio (sin errores de protocolo); el cliente muere localmente cargando el mundo. Defines de paquetes MATCH (PLAYER_PER_ACCOUNT5, QUIVER, ACCE, CHEQUE); structs de entrada idénticos (TPacketGCCharacterAdd, TPacketGCCharacterAdditionalInfo).
- **Hipótesis (por orden):** (1) overflow de buffer del cliente base S3llMetin2 v24 durante la carga del mundo (intermitente por layout del heap; sospechosos: mapa 41 en maps.epk, sonido/bgm, entidades al spawn, paths GM), (2) mismatch de tamaño en algún paquete de entrada no auditado (DYNAMIC_SIZE, fase de carga), (3) race entre hilo de carga y hilo de red del cliente.
- **Próximos pasos:** analizar `/home/m2/cap_entry.pcap` (1 entrada con éxito; falta capturar una con crash para comparar), audit de tamaños de TODOS los paquetes GC de entrada (dump de sizeof con los mismos defines en cliente y servidor), probar personaje nuevo no-GM y/o mapa 1, App Verifier si se dispone.

---

## 10. AUDITORÍA COMPLETA + COMPATIBILIDAD (sesión 3, 2026-08-09) — leer antes de continuar

### Estado verificado (código + runtime + BD)
- **Los 11 archivos del §4 coinciden EXACTAMENTE** con lo documentado (incluidos los fixes de la sesión 2: retornos `int`, `fprintf(stdout)`). Motor vivo: `g_iUseLocale=TRUE` en todos los inits de locale (locale_service.cpp:465-1017).
- Runtime WSL: 16 `locale_string_*.txt` OK (ES=764, EN=775, AE=774, resto 764). BD: `account.lang` = 'en' para `test` (el cliente lo sobrescribió al loguear en EN — diseño actual).
- **CORRECCIÓN de dato erróneo (sesión 3):** el EN del runtime **cubre el 100% de las claves de ES** (0 faltantes, 11 extra). El "732 claves ES sin cubrir por EN" reportado a mitad de la sesión fue un **error de parseo** (se contaban líneas con comillas, no pares clave→valor). El formato real es `"clave";` ⏎ `"valor";` ⏎ (vacía).

### Huecos reales del servidor (por orden)
- **(A) Broadcasts/notices/timers:** `LC_TEXT_LANG`/`LC_TEXT_NEW_LANG` definidas (locale.hpp:57-58) pero **nunca usadas** (1 match = comentario en input_main.cpp:2958). Los 26 `SendNotice` usan el idioma del último paquete procesado. Fix: `LC_TEXT_LANG(d->GetLang(), str)` en los destinos.
- **(B) Quest/monster_chat NO traducen (causa de la mezcla ES/EN vista por el usuario):** `LoadTranslate` + `LoadQuestLocale` (questlua.cpp:718-746) cargan lua fijo ES al boot; `MonsterChat` (char.cpp:5957-6004) usa `locale.monster_chat[...]` sin pasar por el motor. El mod traía `locale_quest_find`/`LC_QUEST_TEXT` (mod locale.cpp:333-374) — **NO integrado**. Integrarlo es el trabajo más grande pendiente.
- **(C) ~437 `ChatPacket` sin `LC_TEXT`** (de 1424): mayoría comandos de protocolo (ok), visibles: arena.cpp:142/718-747, battle.cpp:826/855, char.cpp:3045 "You have gained %d exp." (inglés hardcodeado).
- **(D) Nombres NPC** del servidor fijos desde `mob_proto.locale_name` — mitigado client-side (2026-08-09): el cliente resuelve NPCs desde su pack.
- **(E)** ES no tiene 11 claves que EN sí (10 usadas por código → `@0949`+inglés para jugadores ES).
- **(F)** Copia Windows de `svfiles` desincronizada (16 locale_string solo en WSL).

### Compatibilidad de los locale_string del mod con nuestro código (verificada con conteos)
- **Formato: 100% compatible** con `locale_init_lang` (pares, quote_find_end, locale_convert). Anomalías menores: 4 líneas con comillas embebidas (GR:1409/1415, PT:1409, RU:488 — truncan el valor, cosmético), 24 claves duplicadas en RU (inocuo).
- **Contenido: parcial.** 769 claves únicas `LC_TEXT` en el código; 11 idiomas base + AE/EN/GR cubren ~75% (576-587; los 11 base son sets IDÉNTICOS entre sí). **181 claves (23.5%) faltan en TODOS los archivos → `@0949`+clave para todos los jugadores** (52 inglesas: exchange de won, dados, fishing; 129 coreanas: chat bans ×4, monarch, char_battle...).
- **PT (43.7%) y RU (19.1%) son de OTRA base/versión del mod — no sirven** (aportan claves que ES no tiene; regenerarlos desde la base correcta).
- **Fallback confirmado** (locale.cpp:48-80): idioma jugador → LANGUAGE_DEFAULT(ES) → `@0949`+clave (sys_err LOCALE_ERROR). Jugador EN con clave solo en ES ve ESPAÑOL.

### Cliente (auditoría exp-1)
- Selección de idioma HOY: `locale.cfg` (id codepage nombre; lo escribe `config.exe`, el juego solo lo lee). `locale_list.txt` = 16 idiomas.
- Diálogo nativo `IDD_SELECT_LOCALE` (Locale.cpp:288-359, UserInterface.rc:79-86): **compilado pero muerto** — `LOCALE_SERVICE_GLOBAL` no definido → `LocaleService_LoadGlobal` devuelve false siempre (Locale.cpp:361-364). Activable con la define (listbox feo sin imágenes, aparece cada arranque).
- "Root que faltó" = **correctamente NO integrado**: los 8 archivos `02. Client\root\*.py` del mod son del **coliseo PVP** (`app.LANGUAGE_SYSTEM`, `IsTournamentMap`, `NAME_COLOR_LANGUAGE_SYSTEM`) — cero lógica de localización. Ver decisión #8.
- Selector de banderas en el login: **no existe** (intrologin.py sin idioma; pack sin imágenes de banderas de idioma; plantilla de carga con alphas en introempire.py:53-61). Implementación: imágenes `flag_<lang>.tga` + fila de banderas en intrologin.py + escribir locale.cfg + relanzar (los textos Python no se recargan en caliente).

### Pendiente operativo
- **Verificación del fix del crash** (bounds check `string_replace_word`, cliente 14:12, hash C7EAD7CC): entrar 2-3 veces seguidas.

---

## 9. REFERENCIAS DE ARCHIVOS CLAVE

- Mod: `C:\projects\Metin2\systems\Language System 1.2.6\Language System 1.2.6\03. Source\02. Server\game\{locale.cpp, locale_service.cpp, locale_service.h, locale.hpp}`
- Motor implementado: `source\metin2_server\Srcs\Server\game\src\{locale.cpp, locale.hpp, locale_service.cpp, input_auth.cpp, input_db.cpp, input_main.cpp, desc.h, desc.cpp, packet_info.cpp}`
- Cliente: `source\metin2_client\Srcs\Client\UserInterface\{Packet.h, AccountConnector.cpp, PythonNetworkStreamPhaseLogin.cpp}`
- Runtime: `/home/m2/source/metin2_svfiles/main/srv1/share/locale/spain/locale_string_*.txt` (16 archivos, ya presentes)
- BD: `account.lang` (ya existe, default 'es')
- Binarios: `share/bin/game` (md5 98788379...), `share/bin/db` (md5 e3ca154c...)
