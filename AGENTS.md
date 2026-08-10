# Metin2 Agent Instructions

## Mission

**Metin2 será reescrito por completo en Rust con tecnologías modernas.** Esta es la meta principal del proyecto.

- **Lema: hacer más con menos.** Menos código, menos complejidad, menos dependencias — más claridad, más robustez, más rendimiento.
- **Estrategia: reemplazo incremental módulo por módulo.** El servidor C++ legacy se sustituye pieza a pieza por módulos Rust verificables. Cada módulo debe preservar el comportamiento observable y pasar verificación antes de avanzar.
- La reescritura es un rediseño estructural, no una traducción línea por línea. Las decisiones de arquitectura (límites de dominio, propiedad de datos, protocolos, concurrencia, fallos, migración) se registran como ADRs antes de implementar.

## Fase actual (agosto 2026) — LOGIN FUNCIONANDO

**El login completo (auth + canal + selección de personaje) está ARREGLADO y verificado el 2026-08-08.** Cuenta de prueba: `test` / `1234`.

Cadena de fixes aplicados en esta sesión (cada uno verificado empíricamente):

**Servidor — binario game (auth + cores):**
1. `desc.cpp` `ProcessOutput`: consumir `result > 0` bytes (socket_write retorna length en éxito completo; 0 = EAGAIN; -1 = error). El modelo anterior rompió esto y el buffer de salida nunca drenaba → el cliente no recibía respuestas.
2. `socket.c` (libthecore): el modelo había cambiado la semántica de retorno de `socket_write` (éxito = length); `desc.cpp` debe coincidir con esa semántica (par consistente). **No revertir uno sin el otro.**
3. `main.cpp`: revertido el hack del io_loop (fallthrough READ→WRITE + sys_err debug). Se conservó `optreset→optind` (necesario para glibc/Linux).
4. Cifrado: `_IMPROVED_PACKET_ENCRYPTION_` OFF en ambos lados (cliente y servidor = plaintext). Sequence OFF en ambos.
5. `utils.cpp` `mysql5_password`: **EL FORMATO REAL INCLUYE EL ASTERISCO** (`"*" + encoded2` en mayúsculas — líneas 51-57). El hash almacenado en `account.password` DEBE tener el `*` (formato MySQL nativo de `PASSWORD()`). No "corregirlo" quitando el asterisco — eso rompe el strcmp del auth.

**Servidor — binario db:**
6. `PeerBase.cpp` `Send()`: consumir `result > 0` (mismo problema que el game).
7. `ClientManagerLogin.cpp` `QUERY_LOGIN`: la query DEBE devolver 12 columnas en el orden que espera `CreateAccountTableFromRes`: `SELECT mysql_hash_password('%s'), id, login, password, social_id, 0,0,0,0,0,0, status FROM account WHERE login='%s' AND password=mysql_hash_password('%s')`. El modelo anterior puso 6 columnas en otro orden → WRONGPWD siempre.
8. Ruteo SQL: las queries de cuenta deben usar `iSlot = SQL_ACCOUNT` (el default es `SQL_PLAYER` → `player.account` no existe).
9. `ClientHandleInfo` (ClientManager.h): el constructor NO inicializaba `account_index`/`account_id` → basura → RESULT_LOGIN tomaba el branch equivocado. Ya inicializado a 0.
10. Flood del main loop: el interés WRITE de fdwatch es persistente (el flag `oneshot` se ignora en `fdwatch_add_fd`); tras drenar el buffer del peer, re-registrar con solo READ (`fdwatch_del_fd` + `add_fd(FDW_READ, false)` en el case FDW_WRITE) — si no, el db gira en `AUTH_PEER_WRITE: size 0` y nunca procesa los resultados async.
11. Función SQL creada en MariaDB: `account.mysql_hash_password(pw)` = `CONCAT('*', UPPER(SHA1(UNHEX(SHA1(pw)))))` (coincide con el C++).

**Cliente:**
12. `AccountConnector.cpp` `__AuthState_RecvPhase`: se eliminó `rkNetStream.ClearLoginInfo()` — borraba `m_stPassword` del CPythonNetworkStream DURANTE el auth → la conexión al canal enviaba la contraseña vacía → "contraseña incorrecta" en el canal. (El `ClearLoginInfo` del cliente solo limpia el password, no el ID — por eso el canal recibía el login correcto con password vacío.) Cliente RECOMPILADO con este fix (Release|Win32, metin2client.exe ~5.1MB).
13. Pack `root.epk` (intrologin.py): re-SetLoginInfo con los valores de los edit lines antes de conectar al canal (líneas ~1082 y ~1320). Repack con `PackMakerLite.exe` (en `source\pack\` — usa `PackMakerLite.json` con las claves del pack).
14. **ENTRADA AL MUNDO (nuevo, 2026-08-08):** `PythonNetworkStreamPhaseLogin.cpp` `SetLoginPhase`: se ELIMINÓ `ClearLoginInfo()` (borraba `m_stPassword`) en AMBAS ramas (normal y DirectEnter). El flujo de entrada al mundo reconecta varias veces al canal y REAUTENTICA con LOGIN3 en cada reconexión: (a) DirectEnter del select (`ConnectGameServer`, `introselect.py` → `net.DirectEnter`), (b) warp a la partida (`RecvWarpPacket`). El password vacío → db `RESULT_LOGIN: no account` → `GC_LOGIN_FAILURE` que el cliente en DirectEnter-mode se traga → cliente colgado en el select. Verificado: el server acepta el LOGIN3 con password correcto y responde al `CG_PLAYER_SELECT` (header 6) con la creación del personaje en el mundo (mapa `Venter_the_east.mp3`, stats). Cliente RECOMPILADO (Release|Win32, 14:55, ~5.1MB) y desplegado a `client\metin2client.exe`. La copia del password permanece en memoria durante la sesión (intencional).
15. **SPAM DEL CHAT / monster_chat (nuevo, 2026-08-08, fix de DATOS — sin rebuild):** el chat spameaba `SYSERR: LUA ScriptRunError (code:1 src:[(locale.monster_chat[vnum] ...)])` por cada monstruo en combate. Doble causa, ambas en el runtime `share/locale/spain`:
    - `translate.lua` estaba desplegado VACÍO (0 bytes) → `gameforge` nunca se definía. Fix: `translate.lua` ← `translate_ES.lua` (1.1MB, el contenido real en español). Mismo para `germany` ← `translate_DE.lua`.
    - `quest/locale.lua` tenía error de SINTAXIS para el lua 5.0 del servidor: el lexer está MODIFICADO para coreano EUC-KR (2 bytes por carácter — `read_string` en `liblua/5.0/src/llex.c` consume 2 bytes cuando `b_current & 0x80`). El archivo español era UTF-8 con 349 líneas de coreano de **3 bytes** → desalineación de paridad → la comilla de cierre se "come" → `unfinished string` (LUA_ERRSYNTAX=3) → `LoadQuestLocale returns 3` → `locale.monster_chat` NUNCA se definía. Fix: convertir SOLO las líneas coreanas a CP949 (script `fix_locale_enc.py`: 349 líneas, 0 fallos); el español (acentos UTF-8 de 2 bytes) queda intacto. Verificado: `/home/m2/luaparse` (harness lua 5.0 real del servidor) → `SYNTAX_OK`; tras reiniciar el core: `LoadQuestLocale(...) returns 0` y 0 ScriptRunError nuevos.
    - **REGLA: los archivos lua de locale del servidor que contengan coreano DEBEN usar CP949/EUC-KR (2 bytes/carácter), NO UTF-8 — el lexer del servidor se rompe con UTF-8.**
16. **NOMBRES DE MOBS EN COREANO (nuevo, 2026-08-08, fix de DATOS — sin rebuild):** los mobs mostraban nombres coreanos/mojibake en el juego. Doble causa:
    - El binario db desplegado tiene `ENABLE_PROTO_FROM_DB` + config `PROTO_FROM_DB` activado (evidencia: syserr `InitializeItemTableFromDB`) → el db lee `mob_proto`/`item_proto` desde **MySQL** (`SELECT vnum, name, locale_name, ... FROM mob_proto ORDER BY vnum` en `ClientManagerBoot.cpp:1290-1309`), NO desde los txt del dir `db/`. Editar `mob_proto.txt`/`mob_names.txt` NO surte efecto mientras `PROTO_FROM_DB=1` (el `mob_names.txt` desplegado además estaba VACÍO, 0 líneas).
    - Los datos en MySQL eran coreano CP949 doble-codificado: el txt original `mob_proto.txt` tiene nombres coreanos CP949 (ej. `b5 e9 b0 b3` = 들개) y al importarlos quedaron como `C2B5C3A9...` (cada byte CP949 → latin-1 → UTF-8) en columnas `name`/`locale_name` utf8mb4.
    - **Fix FINAL (multilenguaje):** los nombres se tomaron del PACK DEL CLIENTE (`locale.epk` → `locale/es/mob_proto`, decodificado con `DumpProto.exe` — `Srcs\Tools\DumpProto`, claves mob {4813894,18955,552631,6822045}, item {173217,72619434,408587239,27973291}; formato MMPT0/MIPX + TEA-ECB 32 rounds + LZO1X). El pack español S3llMetin2 v24 ya trae los 2864 nombres en español (Perro Salvaje, Jabalí, Zorro del Desierto...). `UPDATE mob_proto SET name=..., locale_name=...` desde `mob_names.txt` del dump (script `gen_pack_sql.py`), truncando a 24 bytes (`varbinary(24)` — un nombre más largo falla con `ERROR 1406 Data too long`). 14 mobs sin traducir en el pack (coreano) se dejaron intactos. Verificado: `MOB: #101 Perro Salvaje`, `#2101 Zorro del Desierto`, `#20001 Alquimista` en el boot del core.
17. **ARQUITECTURA MULTILENGUAJE (verificada 2026-08-08) — el cliente traduce, el servidor NO:**
    - **Mobs:** el paquete de spawn `TPacketGCCharacterAdd` NO lleva nombre → el cliente resuelve el nombre del mob desde SU pack (`CPythonNonPlayer::LoadNonPlayerData("locale/es/mob_proto")` → `GetName(race)` → `szLocaleName`). El servidor no pinta nombres de mobs.
    - **Items:** el cliente traduce desde `locale/es/item_proto` (MIPX + TEA + LZO → `CItemData::szLocaleName`) + `itemdesc.txt` (solo descripciones). El servidor NO pinta nombres de items.
    - **NPCs:** desde 2026-08-09 el cliente también los resuelve desde SU pack. El servidor SÍ envía el nombre (`GC_CHAR_ADDITIONAL_INFO` → `char.cpp:922-948` → `GetName()` → `szLocaleName` de MySQL) pero el cliente lo IGNORA para `TYPE_NPC` y usa `CPythonNonPlayer::GetName(race)` del pack (fallback al nombre del servidor si el pack no tiene la entrada — `PythonNetworkStreamPhaseGameActor.cpp` `RecvCharacterAdditionalInfo`). Antes (08-08) dependían de la BD → no cambiaban de idioma con el cliente.
    - **TRAMPA (causó crash del core):** los txt de drops del servidor (`etc_drop_item.txt`, y presumiblemente `common_drop_item.txt`, `drop_item_group.txt`, etc.) referencian items **POR NOMBRE en CP949** (`ReadEtcDropItemFile` en `item_manager_read_tables.cpp:457-498` → `GetValidVnum`/`GetVnumByOriginalName`). Si `item_proto` en MySQL no tiene los nombres CP949 originales, el core aborta el boot con `No such an item (name: ...)` → `Boot: cannot load ETCDropItem`. **REGLA: NO tocar los nombres de `item_proto` en el servidor — deben quedar en CP949 original; los nombres visibles de items los pone el cliente.**
    - Los nombres de `mob_proto` SÍ pueden cambiarse en MySQL (mobs no se referencian por nombre en los txt de boot); los NPCs mostrarán el `locale_name` de MySQL.
    - El dbmanager de `source\tools\DBManager` (suite PHP/bash) existe pero es solo import/export txt↔mysql — no traduce.

## Layout del repositorio

> **Estructura reorganizada 2026-08-09:** todo el código vive bajo `source/`, organizado por componente (sin prefijos `metin2_`, sin `Srcs` intermedios). Las rutas viejas (`source\metin2_client`, `source\metin2_server`, `source\metin2_pack`, `source\metin2_svfiles`) NO existen.

| Ruta | Qué es |
|---|---|
| `client\` | Cliente instalado (metin2client.exe v1.0.40999.1 — RECOMPILADO 2026-08-08 con el fix; pack\*.epk, config, mark) |
| `client-om2\` | Cliente fuente de referencia ("Old Metin2 Project", corresponde a tmp4-server) |
| `source\client\` | FUENTE del cliente (S3llMetin2 v24 / MartySama). Build: MSBuild `Metin2Client.sln` (en `source\client\`) Release\|Win32 (VS Build Tools 18). Dependencias de build en `source\client\Extern\` (gitignored) |
| `source\server\` | FUENTE del servidor (MartySama 5.9). `{common,db,game,libgame,liblua,libpoly,libsql,libthecore}`, Makefile portado a Debian/gcc |
| `source\reforge\` | **REESCRITURA RUST (nueva, 2026-08-10)** — workspace Cargo: crates `protocol` (F0: login flow byte-exacto, 30/30 tests), `net` (F1), `db` (F3), `game` (F4+), `auth` (F2). ADR-0003. **NO tocar la línea base C++ desde aquí; la línea base es el oráculo** |
| `source\deploy\` | Runtime desplegado: `main\srv1\{db,auth1,chan\chX\coreY}` instancias (gitignored) |
| `source\pack\` | Fuente del pack del cliente (`root\serverinfo.py` = lista de servidores; `PackMakerLite.exe` + `.json` = herramienta de repack) |
| `source\tools\` | Herramientas: `DBManager` (suite PHP/bash import/export txt↔mysql), `DumpProto`, `switch_compiler.py` |
| `source\tools\proto\` | Metadatos de protocolo |
| `archive\`, `.commandcode\` | Backups, skills |
| `docs\` | ADRs (`docs\decisions\`) y planes/specs (`docs\superpowers\`) |
| `ROADMAP.md`, `CHANGELOG.md` | Plan maestro de la reescritura Rust y registro cronológico de cambios |
| `scripts\` | Scripts de arranque/recuperación (`start_m2_min.sh`, `start_m2_full.sh`, `mem_audit.sh`, `watch_*.sh`) |

**WSL (Debian-M2):**
- `/home/m2/source` — copia de build + deploy (LA fuente de verdad para compilar el servidor).
- `/home/m2/source/metin2_svfiles/main/srv1` — instancias en ejecución (binarios vía symlinks a `share/bin/{game,db}`).
- `/home/m2/tmp4-server` — repo git upstream old-metin2.com (referencia, NO es la fuente activa).
- Los backups de binarios en `/root/m2_backup_bins` se ELIMINARON en la limpieza de disco (2026-08-08).

## REGLA CRÍTICA: dos copias de source (servidor)

- `/home/m2/source` (WSL) es **la copia que compila el servidor** (el `VERSION.txt` hornea esa ruta).
- `C:\projects\Metin2\source` (Windows) es una copia de referencia — NO se usa para compilar el servidor.
- El cliente SÍ se compila desde la copia Windows (`source\client`).
- El desastre del modelo anterior vino de editar ambas copias inconsistentemente y de cambios de defines/protocolo opuestos entre cliente y servidor. **Después de cualquier cambio, sincronizar ambas copias** (diff/md5sum) y **verificar que los defines de protocolo coincidan en ambos lados**.
- CUIDADO: los crashes de WSL pueden PERDER escrituras sin flushear (ext4) — tras desplegar binarios, ejecutar `sync` en WSL y verificar con md5sum.

## Hechos de protocolo (verificados 2026-08-08)

- Cliente v40999 (`source\client`), servidor `__GAME_VERSION__` 41023 (`source\server`).
- Tablas de headers coherentes entre servidor y ambos clientes (handshake 0xff/0xfe, LOGIN3=111, GC_AUTH_SUCCESS=150, GC_LOGIN_SUCCESS3=6, GC_EMPIRE=90...).
- Flujo del login: cliente → auth(30001): GC_PHASE + GC_HANDSHAKE (con retries de bias de reloj ~40-80ms) → CG_HANDSHAKE echo → LOGIN3(65 bytes: 0x6F + name[31] + pwd[17] + keys[16]) → QID_AUTH_LOGIN (SQL en db) → strcmp(hash con *, hash almacenado) → GC_AUTH_SUCCESS(0x96+key+result) → cliente cierra auth → conecta al canal(30003) → LOGIN3 → GD_LOGIN → QUERY_LOGIN (12 columnas) → RESULT_LOGIN → GC_EMPIRE(0x5a+empire) + SendLoginSuccessPacket → selección de personaje.
- **Cifrado: `_IMPROVED_PACKET_ENCRYPTION_` OFF en ambos lados; `USE_NO_PACKET_ENCRYPTION` ON (plaintext).** Sequence OFF en ambos. SI se cambia un lado, cambiar el otro.
- `serverinfo.py`: host `172.25.104.175` (IP eth0 de WSL — **REVISAR tras cada reinicio de WSL**), auth 30001, ch1 30003, ch2 30007, ch3 30011, ch4 30015.
- Runtime: MariaDB 127.0.0.1:3306 (dbs `account`,`common`,`player`,`log`, user/pass mt2/mt2), srv1-db 30000, auth1 30001/30002, cores 30003+.
- Cuenta de prueba: `test` / `1234` (hash `*A4B6157319038724E3560894F7F932C8886EBFCF` en `account.account`).

## Runbook: levantar y probar el servidor

```powershell
# 1. Arranque mínimo (db + auth + ch1-core1) — suficiente para login
wsl -d Debian-M2 -- bash /mnt/c/projects/Metin2/scripts/start_m2_min.sh
# 2. Verificar: puertos 30000-30004; Test-NetConnection 172.25.104.175 -Port 30001
# 3. Probar login en el cliente (C:\projects\Metin2\client\metin2client.exe, test/1234)
# 4. Logs: auth1 = auth/syslog; core1 = chan/ch1/core1/syslog ("LoginSuccess" = OK);
#    db = db/syslog
```

- **El stack completo (9 cores) revienta la memoria de esta máquina** (4GB host, WSL cap 2GB). Usar `start_m2_min.sh` salvo que haya más RAM.
- **WSL inestable:** crashes `Wsl/Service/E_UNEXPECTED` en momentos de I/O pesado (builds, syncs, restarts) — máquina con 4GB RAM, Windows 10 22H2, WSL 2.7.3, errores WHEA PCIe. El SSD estuvo LLENO (5GB libres) — se limpió (npm-cache, TEMP, logs WSL, artefactos de build, backups) → 23GB libres en host, 3.2G usados en WSL. Config en `C:\Users\Ricardo Casamayor\.wslconfig` (memory=2GB, swap=8GB). Tras cada crash: `wsl --shutdown` → `start_m2_min.sh`.
- **Recompilar el servidor:** en WSL: las librerías primero (`cd /home/m2/source/metin2_server/Srcs/Server && make -C liblua/5.0 && make -C libsql && make -C libgame/src && make -C libpoly && make -C libthecore/src`), luego `make -C game/src` → `game_r41023` y `make -C db/src` → `db_r41023`; desplegar a `main/srv1/share/bin/{game,db}` y reiniciar auth+cores. **Siempre `sync` tras el deploy.**
- **Recompilar el cliente:** MSBuild `source\client\Metin2Client.sln` /p:Configuration=Release /p:Platform=Win32 → `source\client\bin\Release\metin2client.exe` → copiar a `client\metin2client.exe`.
- **Reempaquetar el pack:** editar `source\pack\root\*.py` → `cd source\pack && PackMakerLite.exe --nolog --parallel -p root` → copiar `root.epk`/`root.eix` a `client\pack\`.
- **Orden de arranque obligatorio:** mariadb → srv1-db → srv1-auth1 → cores.

## Pendientes conocidos (2026-08-08)

- ~~El db binario desplegado incluye logs de depuración temporales (`DBG_AQR`, `DBG_RESULT_LOGIN`, `DBG_PARSE` en ClientManager.cpp / ClientManagerLogin.cpp)~~ — **LIMPIADOS (sesión 2):** 0 líneas DBG en el boot nuevo del db; el item award loguea limpio.
- **Language System (motor multi-idioma del mod 1.2.6):** integrado y cargando — 16 idiomas, 764-775 entradas c/u, evidencia en `core1/stdout` (NO en syslog: `sys_log` es silencioso en `config_init` porque el logfile no está abierto y `DB_LOG_LEVEL: 1` → `log_level_bits=1` bloquea el stdout de `sys_log`; por eso `LocaleService_LoadLocaleStringFile` usa `fprintf(stdout, "Load LocaleString %s (%d entries)")`). Detalle completo en `docs/LANGUAGE_SYSTEM_ESTADO_2026-08-08.md` §5. **Prueba end-to-end PARCIAL (sesión 2):** el usuario jugó en el mundo y los textos del servidor salen en ESPAÑOL ✓ (incluido `monster_chat`); `login_success: lang 'es' -> 5` confirma la propagación account.lang → g_iCurrentLang. El cliente (build 18:36, 5.115.392 bytes) envía su idioma en el LOGIN3 del auth (68 bytes: 65 + `szLanguage[3]`) → auth hace `UPDATE account SET lang=...` → **el cliente SOBRESCRIBE account.lang en cada login** (para probar otro idioma del servidor hay que cambiar el locale del cliente o el envío).
- **CRASH INTERMITENTE DE ENTRADA AL MUNDO — DIAGNÓSTICO EN CURSO (2026-08-09, sesión 3):** el over-read de `string_replace_word` (PythonSkill.cpp:62) fue un corruptor REAL y está arreglado (bounds check, build 14:12, hash C7EAD7CC), PERO NO es el único: cdb capturó `0xC0000374` en la sesión del debugger (15:25) detectado en `granny2.dll` (alocando 0x552 B) y en los dumps WER de 14:45 en `igc32.dll`/`igdumdim32.dll` (Intel shader compiler) — **distintos detectores, mismo heap dañado** → overflow determinista del cliente durante la carga del mundo cuya detección depende del layout del heap (ASLR). El usuario logró 5/5 entradas seguidas sin instrumentación (estado: sin AppVerifier/PageHeap/cdb) pero NO está declarado resuelto. Herramientas instaladas: Debugging Tools (cdb/WinDbg x86 en `C:\Program Files (x86)\Windows Kits\10\Debuggers\`) + LocalDumps (C:\dumps) + PageHeap vía gflags. **Próximos pasos si reaparece:** `!heap -p -a <bloque>` sobre el dump (stack de asignación del bloque corrupto) y prueba de campo personaje nuevo en mapa inicial (particionar mapa 41/GM vs bug global). Detalle en CHANGELOG.
- **CONVENCIÓN DE COORDENADAS (CRÍTICA para crear/mover personajes):** `player.x/y` = **UNIDADES** (aldea c1 del mapa 41 = `969600, 278400`). Los valores de `AddGotoInfo` del boot (p.ej. c1 `(9696, 2784)`) son **células (÷100)** — NO usarlos directamente en la BD. `GetValidLocation` valida la posición contra el sectree del mapa; si falla → fallback a `EMPIRE_START_*(empire)` (para empire 0 devuelve 0,0). Un personaje guardado con coordenadas basura crashea el CLIENTE con `0xc0000374` (heap corruption) en la carga del mapa.
- Pendientes del LS (auditoría completa 2026-08-09, ver CHANGELOG): selector de idioma en el login (columna de banderas — petición del usuario; el diálogo nativo `IDD_SELECT_LOCALE` está compilado pero muerto: `LOCALE_SERVICE_GLOBAL` no definido). ~~NPCs multilenguaje~~ — **RESUELTO (2026-08-09):** el cliente resuelve NPCs desde su pack (fallback servidor); rebuild cliente 5.115.904 bytes (ver §17 y CHANGELOG). **Huecos reales del servidor:** (A) broadcasts/notices usan el idioma del último paquete (`LC_TEXT_LANG` definida pero nunca usada — 26 `SendNotice` afectados); (B) quest/monster_chat NO traducen (lua fijo español al boot — `locale_quest_find`/`LC_QUEST_TEXT` del mod no integrados; **causa real de la mezcla ES/EN vista por el usuario**); (C) ~437 `ChatPacket` sin `LC_TEXT` (mayoría comandos de protocolo, algunos visibles: arena, battle, char.cpp:3045); (D) nombres NPC del servidor fijos desde `mob_proto.locale_name` sin rama por GetLang; (E) ES no tiene 11 claves que EN sí (10 usadas: exchange won → `@0949`+inglés para jugadores ES); (F) copia Windows de svfiles desincronizada (16 locale_string solo en WSL). **Compatibilidad locale_string del mod (verificada):** formato 100% compatible con el parser; 11 idiomas base + AE/EN/GR cubren ~75% de las 769 claves del código; **181 claves (23.5%) faltan en TODOS los archivos → `@0949`+clave para todos** (52 inglesas: exchange won/dados/fishing; 129 coreanas: chat bans, monarch); **PT (44%) y RU (19%) son de otra base del mod — no sirven**. Corrección: el EN SÍ cubre el 100% de las claves ES (el dato de "732 faltantes" de esta sesión fue un error de parseo — formato pares, no líneas).
- La cuenta `test` tiene 2 personajes (slot 0 = `lkjsnlfknlsk`, slot 4 = `ninja`, ambos en el mapa 41). **OJO:** la "Entrada al mundo VERIFICADA" que constaba antes era evidencia del lado SERVIDOR (paquete de creación de personaje) — el CLIENTE crasheaba en la carga del mapa (ver "Crash de entrada al mundo"). El spam del chat (monster_chat) quedó arreglado con el fix de datos del item 15 (`LoadQuestLocale returns 0` tras reiniciar el core). Pendiente de probar: combate completo (parcial: el usuario combatió y mató mobs ✓), NPCs, items, drops.
- El pack `intrologin.py` tiene los fixes de restauración de contraseña (líneas 1082/1320) que resultaron redundantes tras el rebuild del cliente — se pueden dejar o revertir.
- Evaluar la estabilidad del entorno (más RAM, WSL update/downgrade, o Docker Desktop) antes de depender de sesiones largas.

## Crash de entrada al mundo (0xc0000374 heap corruption) — RESUELTO (2026-08-09)

**Síntoma original:** el cliente crasheaba con `STATUS_HEAP_CORRUPTION` (0xc0000374, ntdll) ~8-17s después del `player_load`, durante la carga del mapa, ~75% de las entradas (intermitente). Firma IDÉNTICA en WER desde las 15:00 del 08-08 (build cliente viejo, ANTES de los cambios LS) → no lo causó el Language System.

**Parte DETERMINISTA — RESUELTA (sesión 2):** los 2 personajes estaban en la BD con coordenadas basura `(960155, 269313)` / `(960970, 271421)` en el mapa 41 (aldea real = `(969600, 278400)` en UNIDADES). Fix: `UPDATE player SET x=969600, y=278400`.

**Parte INTERMITENTE — CAUSA RAÍZ ENCONTRADA Y FIX (2026-08-09, sesión 3):**
1. **Evidencia decisiva: los minidumps del propio cliente** (`client\logs\metin2client_*.dmp`, escritos por `EterExceptionFilter` de `EterBase\error.cpp` — siempre han estado ahí, nadie los había leído). Dos dumps del crash de hoy (13:15:00, 13:15:25) idénticos: excepción `0xC0000005` en `string_replace_word` (`PythonSkill.cpp:62`), instrucción `mov eax,[ecx]` en RVA 0x95110 (`disasm` con dumpbin + PDB), con ECX=0x96510FFD — puntero basura.
2. **Causa:** `string_replace_word` hace `memcmp(base + cur, src, src_len)` SIN comprobar `cur + src_len <= base_len` → over-read del final del string `base` (un `std::string` de `TokenVector[POINT_POLY]` del parseo de `SkillTable.txt`, cargado en la fase de selección de personaje). La basura leída podía "coincidir" espuriamente con "number"/"atk"/"mwep" → se generaban fórmulas de skill corruptas guardadas en `m_SkillDataMap` → al entrar al mundo, evaluar esas fórmulas corrompía el heap → 0xc0000374. Con AppVerifier (guard pages) el over-read se detectaba al instante en el login (por eso cambió el timing: "ahora apenas logueo se cierra").
3. **Fix (2 líneas):** bounds check `cur + src_len <= base_len` antes del `memcmp` (`PythonSkill.cpp:72-90`). Rebuild Release|Win32 → `client\metin2client.exe` 5.115.904 B, 14:12, hash `C7EAD7CC...` desplegado y verificado. **PENDIENTE: verificación final del usuario (entrar 2-3 veces seguidas).**
4. Lecciones: (a) el syserr del servidor NUNCA verá crashes del cliente (memoria local; el servidor solo ve cerrarse el socket) — los errores de cierre del cliente están en `client\logs\*.dmp` (binario, parseable con el script `parse_dump3.py` de la sesión o dumpbin/cdb); (b) App Verifier Heaps cambia el timing de detección (guard pages detectan el over-read en el write) — útil para aislar, no para reproducir el síntoma original.

## Reglas de trabajo

1. Leer este archivo y cualquier `AGENTS.md` cercano antes de trabajar.
2. Inspeccionar el código fuente, build y runtime relevantes antes de tocar nada.
3. Declarar el alcance; preservar cambios del usuario no relacionados.
4. Cambio mínimo y localizado, con justificación. No ocultar warnings sin documentar.
5. Verificación proporcional: inspección → chequeo enfocado → build/run. Reportar con salida real de comandos; no afirmar que algo funciona sin evidencia.
6. Sincronizar siempre las dos copias de source (WSL + Windows).
7. Confirmar antes de operaciones destructivas (borrar volúmenes, bases, caches de build).
8. Actualizar docs/ADRs cuando cambie el conocimiento del proyecto.
9. **Anotar los cambios (bitácora del orchestrator):** al final de cada sesión de trabajo, registrar en `CHANGELOG.md` (estilo Keep a Changelog, agrupado por fecha) qué cambió y con qué evidencia; marcar progreso en `ROADMAP.md`; escribir ADR antes de decidir arquitectura. No terminar una sesión con cambios sin anotar.
10. **Trabajar en paralelo (velocidad):** cuando haya tareas independientes, desplegar agentes especializados en background simultáneamente (@explorer/@librarian para descubrimiento, @fixer para implementación acotada, @oracle para decisiones/review). No serializar trabajo que pueda correr en paralelo; reconciliar resultados al volver.
11. **Modo plan por defecto:** para toda tarea de arquitectura o reescritura, PRIMERO planificar y discutir con el usuario (alternativas, riesgos, ADR antes de implementar). No escribir código de la reescritura sin confirmación explícita del plan.
12. **Pushback permanente (devil's advocate):** el usuario lo pidió explícitamente: antes de aceptar cualquier plan suyo, evaluarlo críticamente y, si existe una opción significativamente mejor, proponerla con argumentos concretos (hechos del repo, medidas, riesgos). Si el plan es sólido, validarlo con evidencia en vez de inventar un pushback falso. Nunca aceptar un plan sin análisis.
13. **Grafos primero (regla permanente del usuario):** ante CUALQUIER tarea de buscar/explorar/modificar/refactorizar código, consultar SIEMPRE los grafos de graphify ANTES de grep/glob/lectura a ciegas: `graphify query "..." --graph <merged>` para preguntas enfocadas, `graphify explain/path/god-nodes` para nodos específicos, o `GRAPH_REPORT.md` para contexto amplio. El usuario no debe tener que pedirlo: es automático en cada tarea de código. Grafos disponibles: `graphify-out/graph.json` (raíz = merge server+client), `source\server\graphify-out\graph.json`, `source\client\graphify-out\graph.json`.
14. **Personalidad ponytail (permanente):** el orchestrator opera SIEMPRE con la filosofía ponytail: YAGNI, solución más mínima que funciona, stdlib/nativo antes que dependencias, una línea antes que cincuenta, no escribir código que no haga falta, no sobre-construir. Aplicar a todo código de la reescritura y de la línea base. Nunca cortar validación, seguridad ni accesibilidad — lo pequeño es consecuencia de lo necesario, no de recortar.

## Metodología de documentación (cómo se lleva la cuenta)

El proyecto sigue el patrón estándar de proyectos con agentes IA (AGENTS.md + CHANGELOG.md + ROADMAP.md + docs/), que es la metodología que el usuario pidió adoptar (referencia: "purely", de gestorify — patrón idéntico a Keep a Changelog + ROADMAP + AGENTS.md).

- **`CHANGELOG.md`** — registro cronológico de todo cambio verificado: fecha, qué cambió, evidencia. Lo mantiene el orchestrator al cierre de cada sesión (regla 9).
- **`ROADMAP.md`** — plan maestro de la reescritura Rust: fases F0–F7 con checkboxes e hitos verificables. Actualizar los checkboxes cuando una fase avanza; mover a "Estado actual" lo verificado.
- **`docs/decisions/`** — ADRs (formato ADR-0001 como plantilla: Estado/Fecha/Contexto/Decisión/Alternativas/Consecuencias). Toda decisión de arquitectura se escribe ANTES de implementar.
- **Grafos** — tras cambios de código relevantes, refrescar con `graphify update` sobre `source\server` y `source\client`, y re-mergear a la raíz (`graphify merge-graphs` server client --out `graphify-out\graph.json`). El MCP `graphify` (config global de opencode) sirve el grafo mergeado en `C:\projects\Metin2\graphify-out\graph.json` — visible como conectado en la TUI de omo-slim. Ver regla 13.

## Guardarraíles para la reescritura Rust (futuro)

- No mezclar cambios de modernización en trabajo de la línea base C++.
- Unificar `game` y `db` es una decisión de arquitectura abierta; documentarla en ADR antes de implementar.
- Mantener la línea base C++ estable y reproducible mientras un módulo se porta; verificar paridad de comportamiento por módulo.
- Los adaptadores de compatibilidad entre el servidor Rust y el legacy deben ser explícitos.
