# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
The project uses semantic versioning ([SemVer](https://semver.org/spec/v2.0.0.html)) once releases exist; until then, entries are grouped by date.

> **Language note:** entries before the 2026-08-10 (4th part) docs reorganization were written in Spanish and are preserved verbatim (history is never rewritten) — this includes the 2026-08-10 1st–3rd parts and all earlier sessions. Only the 4th part and the new English documentation follow the "docs are written in English" rule (AGENTS.md).

## [2026-08-10] (4th part) — Documentation reorganization (docs hubs, plan reorder, ADR-0005/0006/0007)

### Added

- **Documentation hub restructured** (final layout; the hub files are owned by the documentation lanes, the reorg is coordinated):
  - `docs/README.md` — documentation index (entry point to all docs).
  - `docs/CURRENT.md` — current verified state of the project.
  - `docs/DOCUMENTATION.md` — documentation rules and workflow (Keep a Changelog format, ADR template, graph workflow).
  - `docs/plans/server-rewrite.md` — canonical design reference (replaces `docs/history/2026-08-09-server-rewrite-draft.md`, preserved as historical).
  - `docs/reference/protocol/login-flow.md` — byte-exact login wire spec (moved from `docs/superpowers/specs/2026-08-08-wire-protocol-login-flow.md`).
  - `docs/reference/protocol/legacy-compatibility.md` — legacy wire/pack compatibility boundary (ADR-0006).
  - `docs/reference/quests/quest-dsl.md` — quest DSL spec (moved from `docs/superpowers/specs/2026-08-09-quest-dsl-spec.md`).
  - `docs/how-to/`, `docs/tutorials/`, `docs/explanation/`, `docs/decisions/`, `docs/history/`.
- **ADR-0005 (Proposed)** — `docs/decisions/0005-postgresql-cutover-and-legacy-adapter.md`: PostgreSQL cutover (phase G-PG) + temporary legacy compatibility adapter; **F2 is gated by it**. Not accepted yet — needs confirmation.
- **ADR-0006 (Proposed)** — `docs/decisions/0006-legacy-wire-pack-compat-boundary.md`: legacy wire/pack compatibility boundary — PanamaPack (151, 289B) and hybrid-crypt (152/153) isolated in `protocol::legacy`, never in the new wire core; boundary documented in `docs/reference/protocol/legacy-compatibility.md`; deleted at the new client (F7).
- **ADR-0007 (Accepted — only the already-agreed boundary)** — `docs/decisions/0007-no-partial-rust-in-legacy-client.md`: no partial Rust embedded in the legacy client during F0–F6; the Rust client ships standalone (Slint standalone in F5, wgpu client in F7). Everything else about the new client remains open (own ADRs at F7).

### Changed

- **Root `README.md`:** translated to English; concise current status linking to `docs/README.md` and `docs/CURRENT.md`; final workspace names (`source/reforge`: `protocol`, `network`, `database`, `realm` + binary `server_realms` with `auth|channel` roles); architecture section trimmed (no duplicated design — points to `docs/plans/server-rewrite.md`).
- **`ROADMAP.md`:** translated to English; **plan reorder — G-PG (PostgreSQL cutover) before F2**; F2 split into **F2a** (server-side auth) / **F2b** (client batch 1) and **blocked until the PostgreSQL cutover + ADR-0005**; compatibility packets isolated in `protocol::legacy` (ADR-0006); no partial Rust embedded client (ADR-0007); dependency deferrals documented (clap/config-rs → F2, sqlx → G-PG/F3, bevy_ecs → F4, no mlua ever); links updated to `docs/plans/server-rewrite.md`, `docs/reference/protocol/login-flow.md`, `docs/reference/protocol/legacy-compatibility.md`, `docs/reference/quests/quest-dsl.md`; graph counts updated to **server 13,200/33,251, client 17,501/39,258, merged 30,701/72,509**. F0/F1 actual evidence preserved; **G-PG and F2 NOT marked done**.
- **`AGENTS.md`:** translated to English; repository layout and documentation workflow updated to the new docs structure; all safety/build rules, protocol facts, runbook, crash history and the graph workflow preserved; documentation rules now point to `docs/README.md`, `docs/CURRENT.md`, `docs/DOCUMENTATION.md`.
- **ADRs 0001–0004:** metadata headers added (`Status`/`Date`/`Supersedes`/`Superseded by`); ADR-0003 links updated to the new plan/spec paths (old ones noted as historical); decisions unchanged.
- **Documentation policy:** docs are written in English going forward; old decisions/plans marked historical/superseded, never deleted (no-hide-history rule).

### Verified

- All links in the five owned files (README, ROADMAP, CHANGELOG, AGENTS.md, `docs/decisions/*`) point to the final docs target paths.
- ROADMAP F0/F1 checkbox evidence untouched (56/56 tests, F1.1–F1.5 acceptance criteria); G-PG/F2 left unchecked; ADR statuses explicit (0001–0004 Accepted, 0005/0006 Proposed, 0007 Accepted for the already-agreed boundary).
- No source code touched on this lane — documentation-only change; the other docs lanes' files (hubs, renames) are separate work in the same worktree.

## [2026-08-10] (6th part) — Docs audit: guardrails, metadata normalization, hub sections

### Added

- **`docs/guardrails/`** — new section with 6 files, each rule structured as Rule / Why / Evidence / Consequence / Status (policy `docs/DOCUMENTATION.md` §3.1):
  - `README.md` (Hub index), `rust-rewrite.md` (property boundary, two source copies, ADR-before-code, tests/evidence, minimal deps, no partial Rust in client), `legacy-compatibility.md` (PanamaPack is a wire packet not a library/EIX/EPK, `protocol::legacy` temporary, single canonical PostgreSQL, legacy client contract), `data-and-encoding.md` (CP949, `PROTO_FROM_DB`, `item_proto` names, PostgreSQL encoding, units vs cells), `operations.md` (WSL memory, boot order, `sync` after deploy, IP check, no artifacts in git), `world-entry-crash.md` (0xC0000374 postmortem, closed 2/2, diagnostic lessons).

### Changed

- **`docs/DOCUMENTATION.md`** — `Type: Hub`; metadata scheme extended: `Type: Tutorial | How-to | Reference | Explanation | Plan | Decision | Guardrail | History | Hub | Snapshot`; `Status: Current | Proposed | Accepted | Superseded | Historical`; document-kinds table (Plans/Decisions/Guardrails/History/Hub/Snapshot); guardrail rule structure §3.1; no-empty-Diátaxis-dirs rule; documentation workflow §10 (librarian audits → fixer applies → oracle reviews → orchestrator commits).
- **`docs/README.md`** — hub rewritten with visible **Plans / Decisions / Reference / Guardrails / History** sections; empty `tutorials/`/`how-to/`/`explanation/` links removed (documented as on-demand only); reader directed to CURRENT/ROADMAP/CHANGELOG.
- **ADRs 0001–0007 normalized** — consistent YAML frontmatter (`Type: Decision`, `Status`, `Audience`, `Date`, `Last verified`, `Supersedes`, `Superseded by`); **active ADRs translated to English without changing decisions** (0001, 0002, 0003, 0004 were Spanish); 0001 note on ADR-0005 refinement kept; 0002 note on ADR-0004 process-topology refinement added; 0003 folder list corrected (`source/tools/pack`, not `source/pack`).
- **`docs/CURRENT.md`** — `Type: Snapshot`; docs-structure line updated (no empty Diátaxis dirs listed).
- **`docs/plans/server-rewrite.md`** — `Type: Plan`.
- **`AGENTS.md`** — `docs\` layout row and methodology updated: `guardrails/` added, empty Diátaxis dirs marked on-demand, no empty-dir policy.
- **`ROADMAP.md`** — "How the count is kept" adds `docs/guardrails/`.
- **`README.md` (root)** — docs tree line updated (no empty Diátaxis dirs listed).

### Verified

- **Relative-link scan over `docs/**` + root markdown: 0 broken links.**
- Backtick-path scan: 53 flagged, all explained — policy mode names (`docs/tutorials/` etc., on-demand), brace expansion (`source/{client,server,tools,deploy}`), explicit provenance (historical "Original location" paths under `docs/superpowers/`, the reverted `source/realms` rename note), and read-only historical content references. **No active path is broken.**
- Every guardrail file has complete metadata (`Type: Guardrail`, `Status`, `Audience`, `Last verified`) and linked evidence.
- No link points to empty/missing categories; `docs/superpowers`, `docs/tutorials`, `docs/how-to`, `docs/explanation` directories do not exist and contain no files (nothing to remove).
- Content consistency preserved: single canonical PostgreSQL + adapter (ADR-0005), G-PG before F2, F2 blocked, `protocol::legacy` isolated, no partial Rust in legacy client; 56/56 tests, graph counts 13,200/33,251 + 17,501/39,258 + 30,701/72,509; crates `protocol`/`network`/`database`/`realm`/`server_realms`; F1.6 pending; G-PG/F2 not marked done.

## [2026-08-10] (5th part) — Documentation reconciliation (oracle findings)

### Added

- **`docs/history/2026-08-09-server-rewrite-plan-v0.2.md`**: the original Spanish plan v0.2 **body** restored byte-identically from `HEAD:docs/superpowers/plans/2026-08-09-servidor-rust-plan-unico.md` (verified via the original blob hash `7a108e754229ef...`); migration metadata/provenance was added separately and the body remains non-normative. `docs/plans/server-rewrite.md` links to it.
- **`docs/history/README.md`**: index of all historical documents (`Type: Hub`, `Status: Historical`).
- Historical documents received provenance metadata and limited link corrections; their technical history remains preserved and non-normative.

### Changed

- **Single canonical PostgreSQL (user decision 2026-08-10):** `ADR-0005`, `ROADMAP.md` (G-PG + F3) and `docs/plans/server-rewrite.md` now state: **one PostgreSQL**; the C++ baseline operates on the **same PG** through a temporary compatibility adapter (its MySQL `libsql` is bridged); MariaDB is used only as the **migration/export source**; no dual-store, no `direct-sql` backend, no "C++ stays on MariaDB during F2–F6".
- **`docs/plans/server-rewrite.md` + `ROADMAP.md` F4:** removed the invented `SetLocaleName`/`SetItemLocaleName` API (does not exist in the legacy client) → "new in-memory override API to be added around `CPythonNonPlayer`/`CItemData` after `LoadLocaleData`".
- **`docs/reference/protocol/login-flow.md`:** `GC_AUTH_SUCCESS` corrected to **S→C** (was C→S); `TSimplePlayer` size sum fixed to `4+25+1+1+4+4+4+1+4+4+4+4+4+4+2+1 = 71`.
- **`source/reforge/protocol/src/lib.rs` doc-comment:** contract path → `docs/reference/protocol/login-flow.md` (canonical); obsolete 76B/474B deviation narrative removed.
- **Paths:** `source\pack` → `source\tools\pack` in AGENTS.md, README.md, ROADMAP.md and active docs; `Mysql2Proto` added to the tools list (exists in `source/tools`).
- **QUERY_LOGIN:** columns 12 → **13** in AGENTS.md and ROADMAP (verified `ClientManagerLogin.cpp:395-426` + `CreateAccountTableFromRes:259-297`; `lang` column from the Language System ALTER).
- **World-entry crash state:** AGENTS.md/ROADMAP aligned with CHANGELOG — closed, field test 2/2 (2026-08-09).
- **Plan section refs:** ROADMAP §4.6→§5.6, §2.3→§3.3, §11.11→§11.
- **`docs/CURRENT.md`:** removed the claim that the docs snapshot is in commit `b85a019` → "documentation reorganization pending commit".
- **`docs/DOCUMENTATION.md`:** metadata scheme now allows `Type: Hub | Snapshot` and `Status: Historical` (used by the hubs and history index).
- **`README.md`:** license badge/link → "License: pending decision" (no `LICENSE` file exists; MPL-2.0 still Proposed).
- **`CHANGELOG.md` language note:** corrected — the 2026-08-10 1st–3rd parts are also Spanish; only the 4th part and the new English docs follow the English rule.

### Verified

- Relative-link scan over `docs/**` + root markdown: **0 broken links**.
- Grep: no active `source\pack`; no `SetLocaleName` as an existing API (only negative mentions); no active dual-store/`direct-sql (MariaDB)`; no `12 columns` in active docs (only the historical 2026-08-08 changelog entry, preserved verbatim); `docs/superpowers` appears only as provenance/history policy.
- v0.2 body restoration verified byte-identical against the original blob (`7a108e754229ef378605a2fe7216f7c2b185035d`); the current historical file additionally contains the approved migration metadata.

## [2026-08-10] (3ª parte) — F1.5 handshake + binario `server_realms` + config TOML

### Añadido

- **F1.5 — Handshake** (`network/src/handshake.rs`, 597 líneas, 11 tests nuevos): `perform`/`perform_with` sobre `Connection`+`Framer` — envía `GC_PHASE(HANDSHAKE)`+`GC_HANDSHAKE` (nonce u32 nunca 0, now32 con wrap 2^32, l_delta=0), valida el eco `CG_HANDSHAKE` (nonce, l_delta≥0 parity desc.cpp:693-697, bias |±80ms| simétrico vs [0,50] unilateral del legacy), retries ≤32 con timeout 500ms/intento y respiro 50ms, filtra keepalives (0xfc/0xfe) y descarta paquetes fuera de orden (parity input.cpp:625-626). **Cancelación por timeout demostrada segura** (solo descarta reads Pending; los bytes parciales quedan en `framer.buf`). **56/56 tests** (network 23/23, protocol 30/30, server_realms 3/3), 0 warnings de build (clippy: 4 warnings pre-existentes de F0 en `protocol` — indentación de doc-list, args de función, identity_op en test; no de esta sesión).
- **Binario `server_realms`** (antes `server`; nombre provisional del usuario): `git mv` + package renombrado, members/README actualizados, smoke verificado (roles auth/channel exit 0; rol inválido exit 2).
- **Configs: TOML** (decisión del usuario 2026-08-10): configs de `server_realms` en TOML vía config-rs (F2; clap para args). Registrado en ADR-0004.

### Corregido

- **Carpeta legacy**: el renombre intermedio a `source/realms` se REVIRTIÓ por corrección del usuario — `source/deploy` conserva su nombre; `realms` queda solo como sufijo del binario (`server_realms`).
- `.gitignore`: vuelve a `source/deploy/` (el runtime renombrado quedaba sin ignorar — 50 MB).
- Artefactos residuales del binario viejo (`target/debug/server*.exe/pdb`) eliminados.

### Verificado (reviews adversariales del equipo: 3 fixers → 3 oracles-fixer)

- Ora-7 (rename server_realms): ✓ 7/7, sin hallazgos ≥ baja.
- Ora-8 (docs deploy/server_realms): ✓ 8/8, consistencia disco↔docs verificada; 0 residuales de `source/realms` como ruta activa.
- Ora-10 (handshake F1.5): **✓ LISTO para F2** — byte-parity del wire verificada contra el cliente real; cancelación por timeout demostrada por análisis del código. 7 hallazgos de deuda conocida (abajo).

### Pendiente (deuda conocida de F1.5, no bloqueante — para el kickoff de F2)

- **Racional del retry-on-wrong-nonce incoherente** (handshake.rs:64-67, 254-256): el nonce es fijo entre intentos → un eco duplicado lleva el MISMO nonce y se acepta; el camino solo lo dispara corrupción/malicia, donde el close instantáneo del C++ (input.cpp:179-183) es mejor que 32×(500+50)ms ≈ 17.6s de conexión zombie. Corregir doc o revertir a parity en F2.
- **`Handshake.delta ≈ 0` SIEMPRE con el cliente legacy** (el cliente hace eco de `dwTime + 2·lDelta` con lDelta=0 → bias=0): la doc que dice "el bias real es ~la latencia" es incorrecta; y el mecanismo bias/retry NO converge para peers no-legacy con reloj desviado (el C++ converge con lNewDelta; el Rust reenvía now32/lDelta=0). F2 y el futuro cliente Rust no deben heredar esa suposición.
- **Gap de test**: eco parcial (5 de 13 B) a través del timeout — la propiedad más delicada, analizada correcta pero sin test que la fije.
- Off-by-one vs C++: 32 intentos Rust vs 33 envíos legacy (1 inicial + 32 retries); `Handshake.server_time` es el now_ms del inicio (stale tras retries, el C++ fija el del éxito); `unreachable!()` en ruta de red (demostrablemente seguro hoy); 17.6s de vida de conexión muda (el auth público debe considerar timeout global en F2).

## [2026-08-10] (2ª parte) — Estructura y nombres profesionales (ADR-0004) + layout plano

### Añadido

- **ADR-0004** (`docs/decisions/0004-reforge-structure-and-names.md`): estructura y nombres definitivos del workspace — layout PLANO en `source/reforge` (el subdirectorio `crates/` se propuso y el usuario lo descartó), renombres de dominio (sin prefijo de marca), binario único `server_realms` con roles por config, convenciones de workspace, runtime legacy `source/deploy` (sin cambio).
- **Crate binario `server_realms`** (`source/reforge/server_realms/`, nombre provisional del usuario): UN binario con roles por config — `--role auth` (F2) | `--role channel` (F5); main mínimo con std (sin clap), `parse_role` puro con 3 tests. Resuelve la inconsistencia ADR-0002 ("auth proceso propio") vs plan único ("auth modo del binario"): auth es un ROL del mismo binario.
- **Convenciones de workspace**: `[workspace.dependencies]` (tokio 1.49 → resuelve 1.53.1, features centralizadas), `[workspace.lints.rust] unsafe_code = "forbid"` heredado en los 5 crates (`[lints] workspace = true`), `rust-toolchain.toml` (1.97.0), `README.md` del workspace (estructura + glosario).

### Cambiado

- **Renombres de crates** (git mv, historial preservado): `net` → `network`, `db` → `database`, `game` → `realm`, `protocol` sin cambio; crate `auth` eliminado → módulo `network::auth` (stub F2).
- **Framer de network**: tabla C→S ampliada con `CG_ENTERGAME` (10 → 1B) y `CG_STATE_CHECKER` (206 → 1B, constante añadida a `protocol::header`); doc-comment con los matices (0x00 divergencia deliberada vs no-op del C++ `input.cpp:75-76`; EnterGame/StateChecker para F2/F4; sin idle timeout hasta F2). Test nuevo `entergame_and_state_checker_are_1_byte_packets`.
- **Runtime legacy: conserva `source/deploy`** (copia Windows del runtime, gitignored; el árbol WSL `metin2_svfiles` NO se toca — los scripts dependen de esa ruta). El renombre intermedio a `source/realms` se revirtió por corrección del usuario; `realms` queda solo como sufijo del binario de la reescritura (`server_realms`).

### Corregido

- **`.gitignore`**: `source/realms/` → `source/deploy/` (revertido tras corrección del usuario — el runtime legacy conserva su nombre; la regla vuelve a cubrir `source/deploy/`).

### Verificado

- **45/45 tests** (`cargo test` workspace): protocol 30/30, network 12/12, server_realms 3/3, database/realm 0. Build 0 warnings (debug + release fresco). Smoke del binario: roles auth/channel exit 0, rol inválido/flag desconocido/valor faltante exit 2.
- Review adversarial (oracle-fixer A): 0 críticos; 2 MEDIA corregidos (lints heredados en database/realm — falso positivo, ya estaban — y gitignore); lista para commit.
- **Corrección del usuario (misma fecha):** el runtime legacy conserva `source/deploy` (el renombre a `realms` se revirtió) y el binario de la reescritura pasa a llamarse `server_realms` (nombre provisional; carpeta `source/reforge/server_realms` albergará el binario compilado + configs desde F2). Docs, ROADMAP, ADR-0004 y `.gitignore` actualizados en consecuencia.

## [2026-08-10] — REESCRITURA RUST ARRANCADA: ADR-0003 + workspace `source/reforge` + crate `protocol` (F0)

### Añadido

- **ADR-0003** (`docs/decisions/0003-reforge-workspace-rust-layout.md`): el servidor Rust vive en `source/reforge` (carpeta nueva, mismo repo `reforge-core`), workspace con crates `protocol`/`net`/`db`/`game`/`auth`, edition 2024, `protocol` zero-deps, límite de propiedad: nadie toca la línea base C++ desde esta línea de trabajo.
- **Workspace Cargo** en `source/reforge` (5 crates, `cargo build` OK) + `**/target/` en `.gitignore`.
- **Crate `protocol` implementado** (`source/reforge/protocol/src/lib.rs`, ~1.7k líneas, zero-deps): 17 paquetes del flujo de login del spec §3 (handshake, login/login2/login3 65/68B, phase, auth success/failure, login key, empire, login success 449B + TSimplePlayer 71B, character add 37B, additional info 70B, player select/delete/create) con parseo sin panic (longitud incorrecta → `ProtocolError::BadLength`, también slices largos), LE manual, helpers C-string strlcpy (fix `saturating_sub` anti-panic), constantes de headers verificadas contra `packet.h`.
- **30/30 tests** (`cargo test -p protocol`): golden byte-vectores manuales (login3 65B/68B, login success 449B con offsets críticos handle@441/random_key@445/skill_group@70, character add, additional info, handshake, phase, auth success, login failure), roundtrips de todos los paquetes, `wire_sizes`, `bad_lengths_are_errors`.
- **Review adversarial de 2 vueltas (oracle)**: contrato antes de escribir + código después — sin fallos críticos; 2 huecos MEDIO diferidos a fase (keepalive TIME_SYNC/PING → F1; PanamaPack 151/289B + hybrid-crypt 152/153 → F2).

### Corregido (spec del wire protocol — errores que habrían roto la paridad)

- **`TSimplePlayer` es 71B packed, no 76B natural** (`tables.h:271` abre pack(1) antes del struct; evidencia dual-toolchain gcc -m32 y MSVC x86, ambos 71B) → **`TPacketGCLoginSuccess` = 449B (handle@441, random_key@445), no 474B**; `TAccountTable` = 444B. El cliente real ya coincide (449B en producción).
- `TPacketGDAuthLogin` = 110B (no 100B); SQL del auth = 15 columnas (no 13); `HEADER_GC_LOGIN_FAILURE=7`/`HEADER_GC_LOGIN_KEY=118` añadidos; bug conocido del cliente (registra LOGIN_FAILURE con 6B) documentado.
- Todas las correcciones aplicadas en el cuerpo del spec + sección «Erratas 2026-08-10» (§7) con pendientes por fase.

### Pendiente

- Harness de captura real (tcpdump contra server C++ en WSL) para cerrar el hito F0 con evidencia de red — requiere stack arriba.
- F1 (net): listener tokio + keepalive; F2 (auth): PanamaPack + constructores + validación de header por dispatch.

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

- **Servidor: 11/11 archivos del doc `docs/reference/legacy/language-system.md` §4 verificados en el código actual.** Motor vivo (`g_iUseLocale=TRUE`), runtime 16 idiomas desplegado, `account.lang='en'` en BD (el cliente lo sobrescribió al loguear en EN — comportamiento por diseño).
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
- **Spec byte-exacto del wire protocol de login** (`docs/reference/protocol/login-flow.md`, antes `docs/superpowers/specs/2026-08-08-wire-protocol-login-flow.md`): constantes (LOGIN_MAX_LEN=30, PASSWD_MAX_LEN=16), framing sin prefijo de longitud (tabla `CPacketInfoCG`), 16 structs packed con offsets (TPacketCGLogin3 65/68B, TPacketGCLoginSuccess 474B, TPacketGCCharacterAdd 37B...), máquina de estados auth→canal completa y protocolo peer GD/DG/QID. Extraído con el grafo graphify + lectura de fuentes.
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
