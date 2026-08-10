# Changelog

Todos los cambios notables del proyecto se documentan en este archivo.

El formato se basa en [Keep a Changelog](https://keepachangelog.com/es/1.1.0/).
El proyecto usa versionado semántico ([SemVer](https://semver.org/spec/v2.0.0.html)) cuando existan releases; mientras tanto, las entradas se agrupan por fecha.

## [2026-08-09] (3ª sesión, 4ª parte) — Selector de banderas FUNCIONANDO + personajes viejos recuperados (2/2) + stack rearmado

### Resuelto

- **CRASH DE ENTRADA AL MUNDO — CERRADO (prueba de campo 2/2):** los personajes viejos del mapa 41 (lkjsnlfknlsk, ninja) tenían coordenadas basura `(957500,258241)`/`(959878,242236)` (fueron escritas por harness de sesiones anteriores). `UPDATE player SET x=969600, y=278400` (aldea c1, unidades) → **entradas 2/2 seguidas** con el cliente. Los 3 dumps WER de 18:49-18:50 (0xC0000374, confirmado con cdb) eran SIEMPRE con lkjsnlfknlsk — **no el idioma TR** (el servidor aceptó `lang 'tr' -> 15` + `LoginSuccess` correctamente).
- **Selector de idioma con banderas — FUNCIONANDO end-to-end** (login → `locale.cfg` → reinicio → LOGIN3 con el idioma → servidor): 
  - Fix del header TGA generado (struct.pack con 6 H's en vez de 4 → width/height=0 → `Cannot GetImageInfo from texture` en syserr.txt del cliente). Header corregido a `20 00 18 00 20 08` (32×24, bpp32, desc 0x08) idéntico al `choise_close.tga` del pack.
  - Fix pantalla negra: `ui.__mem_func__` sobre closure rompía `LoginWindow.Open()` → SetEvent directo (como las lambdas del VK) + try/except blindado.
  - Posición final: anclada al **SaveAccountBoard** (`y = saveAccountBoard.y - 30`), no al LoginBoard — el SAB está más arriba.
  - TR probado por el usuario (entra con el fix de coordenadas).
- **Stack caído y rearmado** (22:23): mariadb + db + auth + core1 levantados con `start_m2_min.sh` (puertos 30000-30004 OK, RAM 617MB/2GB). La BD cayó por el socket pero el demonio seguía en TCP — `mysql -h 127.0.0.1` lo confirmó.

### Pendiente

- Vigilar estabilidad (2/2 es buena señal pero el usuario quiere más muestras).
- Evaluar si Debug build del cliente aporta algo (respuesta: no — ver abajo).

## [2026-08-09] (3ª sesión, 3ª parte) — Selector de idioma con banderas en el login + partición del crash (4/4 personaje nuevo)

### Añadido

- **Selector de idioma con banderas en el login** (pack root, SIN rebuild del cliente):
  - 16 idiomas (los que soporta el servidor, `LANGUAGE_AE..TR` de locale.hpp:20-36): ae, cz, de, dk, en, es, fr, gr, hu, it, nl, pl, pt, ro, ru, tr.
  - 32 imágenes TGA generadas (16 × normal + hover `_over`, 32×24, type-2 32-bit BGRA bottom-left, formato idéntico al `choise_close.tga` del pack) descargadas de flagcdn (w40, `en`→`gb` porque flagcdn no tiene ISO "en") → `pack/root/flag/`.
  - `intrologin.py`: `__CreateLanguageSelector()` (fila de `ui.MakeButton` centrada abajo del todo, `y = SCREEN_HEIGHT-45`, tooltip con el idioma en inglés) + `__OnClickLanguageFlag(lang, codepage, name)` → escribe `client\locale.cfg` (`"10002 <codepage> <lang>"`) y pide reiniciar el juego (el C++ solo lee locale.cfg al arranque; no hay reinicio in-process).
  - Codepages tomados de la tabla nativa `gs_stLocaleData` (Locale.cpp:235-263): ae 1256, cz 1252, de 1252, dk 1252, en 1252, es 1252, fr 1252, gr 1253, hu 1250, it 1252, nl 1252, pl 1250, pt 1252, ro 1250, ru 1251, tr 1254.
  - Repack `PackMakerLite.exe -p root` (538368 B, 18:04) desplegado a `client\pack\` y VERIFICADO desempaquetando: 32 banderas presentes + código del selector en intrologin.py.
  - **PENDIENTE de probar por el usuario:** abrir el cliente → fila de banderas abajo → click → reiniciar → textos del cliente en el idioma elegido.

### Resuelto (partición del crash de entrada al mundo)

- **Prueba de campo 4/4 entradas seguidas con personaje NUEVO** (mapa 0, "Chaman", id 3) → el crash `0xC0000374` NO es global ni del cliente: es de los DATOS de los 2 personajes viejos del mapa 41 (lkjsnlfknlsk id 1, ninja id 2). Inspección BD: items normales (vnums válidos, sin vnum 0 con count, sin counts >200), sin quests, sin affects — el estado del personaje en BD se ve limpio; la causa más probable restante es la posición `(957500,258241)`/`(959878,242236)` en el mapa 41 (fuera de la aldea c1 `969600,278400`) o un dato no inspeccionado. Próximo paso si el usuario quiere recuperar esos personajes: `UPDATE player SET x=969600, y=278400` en ambos (posicionarlos en la aldea) y reintentar; si sigue, borrar y recrear.

### Arreglado (mismo día, 18:12 — pantalla negra en el login)

- **El selector de banderas causó pantalla NEGRA al abrir el login** (primera versión 18:04): `btn.SetEvent(ui.__mem_func__(self.__OnClickLanguageFlag(...)))` envolvía una **closure** con `__mem_func__` (wrapper pensado para métodos bound estilo `self.__OnClickLoginButton`) → excepción en `__CreateLanguageSelector` durante `LoginWindow.Open()` → el login no se construye → negro. **Fix:** `SetEvent` directo con la closure (igual que las lambdas del teclado virtual, `key_space.SetEvent(lambda ...)`) + **try/except blindado** en `__CreateLanguageSelector` (`print` del error, el login se muestra igual aunque el selector falle). Repack 538368 B 18:12, desplegado a `client\pack` y verificado por desempaquetado (línea 379 sin `__mem_func__`, 32 banderas dentro del epk).
- **Verificado el `.rar` del sistema completo** (`systems\Language System 1.2.6.rar`, UnRAR l): contenido idéntico a la carpeta extraída, **sin ninguna imagen de bandera de país** y sin lógica de selector de login. Los 8 `02. Client\root\*.py` del mod son parches del coliseo PVP (dependen de `__LANGUAGE_SYSTEM__` en el C++ del cliente, no integrado) — **copiarlos rompería el login** (ImportError `uiLanguageSystem`, AttributeError `app.LANGUAGE_SYSTEM`, `player.IsLanguageSystem()` inexistente). Confirmada la decisión #8 del doc de estado (no integrar ese root).


## [2026-08-09] (3ª sesión, 2ª parte) — Crash de entrada al mundo: diagnóstico en curso + auditoría del Language System

### Arreglado

- **`string_replace_word` over-read — CORRUPTOR REAL confirmado y arreglado** (pero NO es el único; ver "En curso"): el over-read de `memcmp(base+cur, src, src_len)` (PythonSkill.cpp:62) fue confirmado por los minidumps del cliente (13:15, con AppVerifier: AV `0xC0000005` en 0x495110, ECX=0x96510FFD) y arreglado con bounds check `cur+src_len <= base_len` (PythonSkill.cpp:72-90, build 14:12, hash C7EAD7CC desplegado).
- **Diagnóstico del crash CON herramientas definitivas (cdb instalado):**
  - Dumps WER completos de 14:45-14:46 (466MB c/u, LocalDumps): `0xC0000374` (heap corruption) en ntdll, stack del hilo principal: `metin2client!CPythonMiniMap::Render → CStateManager::DrawIndexedPrimitive → d3d9!CreatePixelShader → igdumdim32!GTPIN_IGC_Instrument → ntdll!RtlAllocateHeap`; hilo 0:015 (pool del driver Intel): `igc32!OpenCompiler9` compilando shaders → detecta el heap ya corrupto.
  - cdb en vivo (15:25): capturó `0xC0000374` detectado en `granny2.dll` (alocando 0x552 B, heap 0x00cc0000, bloque 0x1a722638) — **distinto detector, mismo heap dañado**.
  - **Conclusión:** overflow determinista del cliente durante la carga del mundo (entre login y entrada); la DETECCIÓN depende del layout del heap (ASLR) → intermitente (~75%). Los detectores (igc32, granny2) son víctimas, no culpables.
  - **Estado: NO RESUELTO.** El usuario logró 5/5 entradas seguidas sin instrumentación (buena señal, posible reducción de frecuencia con el fix de string_replace_word, pero la corrupción subyacente sigue: cdb la detectó en la misma ventana).
  - Herramientas ahora instaladas y configuradas: **Debugging Tools (cdb/WinDbg x86)** en `C:\Program Files (x86)\Windows Kits\10\Debuggers\`, LocalDumps full → `C:\dumps`, PageHeap vía gflags. **Próximo paso si reaparece:** `!heap -p -a <bloque>` sobre el dump nuevo (stack de asignación del bloque corrupto) + prueba de campo personaje nuevo en mapa inicial (particionar mapa 41/GM vs bug global).
  - Lección registrada: el syserr del servidor NUNCA verá crashes del cliente (memoria local); los errores del cliente están en `client\logs\*.dmp` (EterExceptionFilter) o `C:\dumps` (WER LocalDumps).

### Auditoría completa del Language System (cliente + servidor + pack)

- **Servidor: 11/11 archivos del doc `LANGUAGE_SYSTEM_ESTADO_2026-08-08.md` §4 verificados en el código actual.** Motor vivo (`g_iUseLocale=TRUE`), runtime 16 idiomas desplegado, `account.lang='en'` en BD (el cliente lo sobrescribió al loguear en EN — comportamiento por diseño).
- **CORRECCIÓN DE DATO ERRÓNEO de esta misma sesión:** el EN del runtime **cubre el 100% de las claves de ES (0 faltantes, 11 extra)**. El análisis previo de "732 claves ES sin cubrir por EN" fue un **error de parseo** (contaba líneas con comillas, no pares clave→valor). El EN estaba completo; la mezcla ES/EN que vio el usuario tiene otras causas (ver huecos B y C abajo).
- **Huecos reales del servidor** (lo que falta para "todos los textos del servidor en el idioma del jugador"):
  - **A. Broadcasts/notices/timers usan el idioma del ÚLTIMO paquete procesado** — `LC_TEXT_LANG`/`LC_TEXT_NEW_LANG` están definidas (locale.hpp:57-58) pero nunca se usan (1 match = comentario). Los 26 `SendNotice` salen en el idioma del jugador anterior.
  - **B. Textos de quest y monster_chat NO traducen** — cargan lua fijo al boot en español (`translate.lua`, `quest/locale.lua`, `MonsterChat` → `locale.monster_chat` sin pasar por el motor). **Es la causa real de los "NPCs/mensajes en español" con cliente EN.** El mod traía `LC_QUEST_TEXT`/`locale_quest_find` (mod `locale.cpp:333-374`) que NO se integró.
  - **C. ~437 `ChatPacket` sin `LC_TEXT`** (de 1424) — la mayoría son comandos de protocolo (no requieren traducción), pero hay visibles: arena (marcadores), battle (avisos hack), `char.cpp:3045` "You have gained %d exp." hardcodeado en inglés, etc.
  - **D. Nombres de NPC fijos** desde `mob_proto.locale_name` (español) sin rama por `GetLang()` — mitigado client-side hoy (el cliente resuelve NPCs desde su pack), pero el servidor no manda nombre por idioma.
  - **E. ES no tiene 11 claves que EN sí** (10 usadas por el código: exchange de won) → los jugadores ES ven `@0949`+inglés en esos 10 textos.
  - **F. Copia Windows de `svfiles` desincronizada** (los 16 `locale_string_*.txt` solo están en WSL).
- **Selector de idioma en el login (banderas): NO existe.** El diálogo nativo `IDD_SELECT_LOCALE` está **compilado pero muerto** (`LOCALE_SERVICE_GLOBAL` no definido → `LocaleService_LoadGlobal` devuelve siempre false, UserInterface.cpp:759). Hoy el idioma se elige con `config.exe`/`locale.cfg`. Pendiente de implementar (aprobado por el usuario).
- **"El root que faltó" = correctamente NO integrado:** los 8 archivos `02. Client\root\*.py` del mod son parches del **coliseo PVP** (`app.LANGUAGE_SYSTEM`, `IsTournamentMap`, `NAME_COLOR_LANGUAGE_SYSTEM`, `LanguageSystem_ITEM_BOX_REWARD`) — cero lógica de localización. Excluirlos fue la decisión correcta (decisión #8 del doc de estado).

### Compatibilidad de los locale_string del mod con nuestro código (verificado con conteos)

- **Formato: 100% compatible** con el parser (`locale.cpp:222-307`); 11 idiomas base perfectos; solo 4 líneas con comillas embebidas (GR:1409/1415, PT:1409, RU:488 — cosmético, trunca el valor) y 24 claves duplicadas en RU (inocuo).
- **Contenido: parcial.** 769 claves únicas `LC_TEXT` en el código; los 11 idiomas base + AE/EN/GR cubren ~75% (576-587 claves, sets idénticos entre sí). **181 claves (23.5%) no existen en NINGÚN archivo → `@0949`+clave para TODOS los jugadores** (52 inglesas de features MartySama 5.9: exchange de won, dados, fishing; 129 coreanas: chat bans ×4, monarch, char_battle...).
- **PT (43.7%) y RU (19.1%) NO sirven — son de OTRA base/versión del mod** (aportan claves que ES no tiene). Habría que regenerarlos.
- **Fallback confirmado** (locale.cpp:48-80): idioma del jugador → ES (default) → `@0949`+clave. Jugador EN con clave solo en ES ve ESPAÑOL (no `@0949`).

### En curso (actualizado)

- **Verificación del fix del crash**: entrar 2-3 veces seguidas con el cliente nuevo (14:12).
- Reescritura Rust del servidor (ver `ROADMAP.md` — Fase 0 en preparación).
- **Pendientes del Language System por orden**: (1) verificar crash, (2) huecos del servidor A+B+C (broadcasts por idioma, quest/monster_chat multilenguaje, ChatPacket sin LC_TEXT), (3) selector de banderas en el login (pack + imágenes), (4) 181 claves faltantes en los 16 archivos + regenerar PT/RU + 11 claves ES, (5) limpiar 4 líneas con comillas embebidas.
- Selector de idioma en el login (columna de banderas — pendiente de diseño, no confundir con el coliseo del mod).

## [2026-08-09] (3ª sesión) — Multilenguaje: NPCs resuelven nombre desde el pack del cliente

### Arreglado

- **NPCs ahora traducen con el idioma del cliente.** El nombre de los NPCs (guardias, tenderos, Alquimista…) venía del servidor (`GC_CHAR_ADDITIONAL_INFO` → `GetName()` → `szLocaleName` de MySQL, en español) y no pasaba por el Language System → no cambiaba de idioma aunque el cliente estuviera en inglés. Items y mobs sí cambiaban porque el cliente los resuelve desde su pack (`locale/<lang>/item_proto` y `mob_proto` — misma ruta dinámica, `PythonApplication.cpp:878-880`).
  - Fix en el cliente (`PythonNetworkStreamPhaseGameActor.cpp` `RecvCharacterAdditionalInfo`): para `CActorInstance::TYPE_NPC` se usa `CPythonNonPlayer::GetName(race)` del pack del cliente (idioma actual), con fallback al nombre del servidor si el pack no tiene la entrada. `TYPE_PC` (jugadores) intacto — sus nombres son del servidor por diseño.
  - Rebuild Release|Win32 OK (0 errores, `metin2client.exe` 5.115.904 bytes, 12:35). **Despliegue a `client\metin2client.exe` pendiente** — el cliente estaba abierto (deploy falló con IOException). Verificar hash tras copiar.
- Diagnóstico completo del multilenguaje (evidencia por código, no solo teoría):
  - Items: pack cliente → cambian ✓ (verificado por el usuario).
  - Mobs reales: pack cliente, misma ruta que items → ya cambiaban (el usuario veía criaturas tipo NPC — guardias/tenderos — que son las que no cambiaban).
  - NPCs: servidor MySQL (español) → NO cambiaban ← este es el fix de hoy.
  - El pack `locale.epk` (S3llMetin2 v24) tiene los 17 locales con `mob_proto` real por idioma (es `#101=Perro Salvaje`, en `#101=Wild Dog`); el cliente nunca tuvo hardcodeado `locale/es` — AGENTS.md §17 quedó desactualizado en ese punto (la ruta es dinámica desde `locale.cfg` → `MULTI_LOCALE_PATH`).

### En curso (actualizado)

- Reescritura Rust del servidor (ver `ROADMAP.md` — Fase 0 en preparación).
- Prueba end-to-end del Language System: textos ES verificados; **probar ahora con cliente EN**: server texts vía `account.lang` (el cliente sobrescribe con su locale), NPCs en inglés (fix de hoy), mobs/items en inglés (pack). monster_chat/quest strings siguen en español (data `quest/locale.lua` — ver pendientes).
- Selector de idioma en el login (columna de banderas — pendiente de diseño, no confundir con el coliseo del mod).

## [2026-08-08] (2ª sesión) — Language System: motor cargando + limpieza DBG

### Arreglado

- **Language System — motor cargando (falso negativo de log):** el boot del core no mostraba las 16 líneas "Load LocaleString" porque `sys_log` es invisible en `config_init` (el logfile no está abierto aún y `sys_log` a stdout requiere `log_level_bits > 1`, pero el CONFIG fija `DB_LOG_LEVEL: 1`). El motor cargaba desde el inicio; solo la evidencia se perdía.
  - `locale_init_file`/`locale_init_lang` ahora devuelven el nº de entradas cargadas (`int`); el bucle de `LocaleService_LoadLocaleStringFile()` imprime con `fprintf(stdout, "Load LocaleString %s (%d entries)")` — visible en boot.
  - Evidencia (boot 20:31, `core1/stdout`): 16/16 líneas con 764-775 entradas por idioma (`AE 774, CZ/DE/DK/ES/FR/GR/HU/IT/NL/PL/PT/RO/RU/TR 764, EN 775`); `LOCALE_ERROR` = 0.
- **Logs de debug del db eliminados:** `DBG_AQR` (ClientManager.cpp), `DBG_PARSE` y `DBG_RESULT_LOGIN` (ClientManagerLogin.cpp) — rebuild `db_r41023`, deploy y verificación: 0 líneas DBG en el boot nuevo; el item award refresh loguea limpio.
- Ambos cambios aplicados en las dos copias de source (WSL `/home/m2/source` + Windows `source\metin2_server`), md5 sincronizados, binarios desplegados y stack reiniciado (db/auth1/core1, puertos 30000-30004 OK).
- **Crash de entrada al mundo — parte determinista RESUELTA:** los 2 personajes de `test` estaban en la BD con coordenadas basura `(960155, 269313)` en el mapa 41 (~100x fuera; la aldea es `(969600, 278400)`); el cliente crasheaba con `0xc0000374` (heap corruption) al calcular tiles fuera de rango. Fix: `UPDATE player SET x=969600, y=278400` para ambos. El usuario entró al mundo y jugó (combate ✓, mobs con nombre en español ✓, textos del servidor en español ✓).
- **Language System — prueba end-to-end parcial:** los textos del servidor salen en ESPAÑOL en el cliente real (motor traduciendo con la tabla ES); propagación `account.lang` → `g_iCurrentLang` verificada (`login_success: lang 'es' -> 5`). Nota: el cliente sobrescribe `account.lang` con su idioma en cada login (diseño actual).

### Pendiente / conocido

- **Crash INTERMITENTE de entrada al mundo (~75% de entradas, NO RESUELTO):** con coordenadas válidas el cliente crashea aleatoriamente ~8-17s tras `player_load`, misma firma `0xc0000374` que desde las 15:00 (no relacionado con el Language System). Hipótesis principales: overflow del cliente base S3llMetin2 v24 durante la carga del mundo (layout del heap), mismatch de algún paquete de entrada no auditado, o race de hilos del cliente. Detalle completo en AGENTS.md "Crash de entrada al mundo". Captura `/home/m2/cap_entry.pcap` (1 entrada con éxito) para comparar.
- Prueba end-to-end del Language System (ver "En curso").

## [2026-08-08] — Línea base de login verificada + metodología de docs

### Añadido

- **Graphify como MCP conectado en la TUI (omo-slim/opencode):** servidor MCP `graphify` (stdio, `python -m graphify.serve`) registrado en la config global `C:\Users\Ricardo Casamayor\.config\opencode\opencode.jsonc`; dependencia `mcp` instalada en Python; grafo mergeado raíz creado (`graphify merge-graphs` server+client → `graphify-out\graph.json`, 31.141 nodos / 73.349 edges). Handshake MCP verificado (`serverInfo: graphify 0.9.35`). El MCP y el skill ponytail se añadieron al preset del orchestrator (`oh-my-opencode-slim.json`).
- **Regla 13 (permanente):** consultar SIEMPRE los grafos de graphify primero (query/explain/path/GRAPH_REPORT) antes de grep/glob/lectura a ciegas en cualquier tarea de buscar/modificar/refactorizar código.
- **Regla 14 (permanente):** personalidad ponytail — YAGNI, mínima solución que funciona, stdlib/nativo antes que dependencias, una línea antes que cincuenta; sin recortar validación/seguridad/accesibilidad.
- **Skills de ponytail instalados** (github.com/DietrichGebert/ponytail, MIT): `ponytail`, `ponytail-review`, `ponytail-audit`, `ponytail-debt`, `ponytail-gain`, `ponytail-help` en `.agents/skills/`; plugin OpenCode vendeado en `.opencode/ponytail/` y activado en `opencode.json`. Filosofía YAGNI ("la mejor línea es la que nunca se escribe") — alineada con el lema del proyecto "hacer más con menos" (benchmark del autor: -54% LOC, -20% coste, 100% safe).
- **ROADMAP.md**: plan maestro de la reescritura Rust (servidor primero, cliente después) con fases F0–F7, hitos verificables y decisiones abiertas para ADRs.
- **CHANGELOG.md**: este registro cronológico de cambios (metodología "Keep a Changelog").
- **AGENTS.md**: sección de metodología de documentación — el orchestrator anota los cambios de cada sesión en el CHANGELOG y actualiza ROADMAP/ADRs.
- **Grafos actualizados**: `graphify update` sobre `source\metin2_server` (13.190 nodos / 33.233 edges) y `source\metin2_client` (17.951 nodos / 40.116 edges).

### Avance Fase 0 (reescritura Rust)

- **ADR-0002 aceptado** (`docs/decisions/0002-unify-game-and-db.md`): unificar `game`+`db` en un proceso por canal con db como crate; shim legacy del protocolo GD/DG durante F3–F5; unificación final en F6. Recomendación de @oracle con verificación en el código (el db legacy es un broker SQL + coordinador cross-canal, no una BD).
- **Spec byte-exacto del wire protocol de login** (`docs/superpowers/specs/2026-08-08-wire-protocol-login-flow.md`): constantes (LOGIN_MAX_LEN=30, PASSWD_MAX_LEN=16), framing sin prefijo de longitud (tabla `CPacketInfoCG`), 16 structs packed con offsets (TPacketCGLogin3 65/68B, TPacketGCLoginSuccess 474B, TPacketGCCharacterAdd 37B...), máquina de estados auth→canal completa y protocolo peer GD/DG/QID. Extraído con el grafo graphify + lectura de fuentes.
- **Stack Rust investigado y fijado**: tokio 1.49 + sqlx 0.9 + mlua 0.12 + config-rs + clap 4.6 + tracing + proptest (reporte de @librarian; sin actores: task-per-connection, mundo por canal tras `mpsc`).
- **Mapa de módulos del servidor** (reporte de @explorer): los 3 binarios, propiedad de datos, capa de red libthecore/fdwatch y 15 fronteras naturales de port (char.cpp 6.5k LOC, input_main, quest engine, db ClientManager*...).

### Arreglado (línea base C++ — ver AGENTS.md "Fase actual" para detalle)

- **Login completo funcional** (auth + canal + selección de personaje), verificado con el cliente real y la cuenta `test`/`1234`:
  - Semántica de `socket_write` (consumir `result > 0`) en game (`desc.cpp`) y db (`PeerBase.cpp`) — el buffer de salida drenaba.
  - Cifrado plaintext en ambos lados (`_IMPROVED_PACKET_ENCRYPTION_` OFF, `USE_NO_PACKET_ENCRYPTION` ON).
  - `mysql5_password` con asterisco incluido (`"*" + UPPER(SHA1(UNHEX(SHA1(pw))))`), coincidiendo con la función SQL `account.mysql_hash_password`.
  - `QUERY_LOGIN` con las 12 columnas en el orden que espera `CreateAccountTableFromRes`.
  - Ruteo SQL con `iSlot = SQL_ACCOUNT`.
  - `ClientHandleInfo` con `account_index`/`account_id` inicializados.
  - Re-registro del peer con solo READ tras drenar el buffer (evita el flood `AUTH_PEER_WRITE: size 0`).
  - Cliente: eliminados los `ClearLoginInfo()` que borraban `m_stPassword` durante el auth y en `SetLoginPhase` (entrada al mundo vía DirectEnter/warp).
- **Entrada al mundo** verificada (mapa `Venter_the_east.mp3`, stats) con el cliente recompilado.
- **Spam del chat / monster_chat**: `translate.lua` desplegado vacío → restaurado desde `translate_ES.lua`; `quest/locale.lua` con sintaxis rota por coreano UTF-8 (el lexer lua 5.0 es EUC-KR 2 bytes) → convertido a CP949. `LoadQuestLocale returns 0`.
- **Nombres de mobs**: reescritos en MySQL desde el pack del cliente (locale_epk, DumpProto) con los 2864 nombres en español; `item_proto` se dejó en CP949 original (los drops referencian items por nombre — no traducir).

### Reglas nuevas (documentadas en AGENTS.md)

- Los `.lua` de locale del servidor con coreano deben usar **CP949/EUC-KR**, no UTF-8.
- No traducir `item_proto` en el servidor (los txt de drops referencian items por nombre CP949).
- El cliente traduce, el servidor no (contrato de multilenguaje).

## [2026-08-06] — Fundaciones

### Añadido

- **ADR-0001**: PostgreSQL como base principal del futuro servidor Rust, sin TimescaleDB por defecto (en `docs/decisions/0001-postgresql-without-timescaledb-by-default.md`).
- Skills de proyecto (`.agents/skills/`) y planes en `docs/superpowers/`.
- Compatibilidad de la línea base C++ con Alpine/Docker (planes en `docs/superpowers/plans/`).
