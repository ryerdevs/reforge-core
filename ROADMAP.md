# ROADMAP — Reescritura de Metin2 en Rust

> **Plan vivo.** Este documento es el plan maestro del proyecto y se actualiza en cada sesión.
> Metodología de seguimiento: `AGENTS.md` (reglas + estado verificado) + `CHANGELOG.md` (registro cronológico de cambios) + `ROADMAP.md` (este plan) + `docs/decisions/` (ADRs).
> **Regla de avance: ninguna fase se da por completa sin evidencia de verificación** (regla 5 de AGENTS.md).
> **Documento de referencia del diseño completo:** `docs/superpowers/plans/2026-08-09-servidor-rust-plan-unico.md` (único archivo, 13 secciones).

## Estado actual (2026-08-09)

- **Línea base C++ verificada:** login completo funcionando (auth + canal + selección de personaje) con el cliente real. Cuenta `test` / `1234`.
- **CRASH DE ENTRADA AL MUNDO — RESUELTO (2026-08-09):** causa raíz en el cliente — over-read de heap en `string_replace_word` (PythonSkill.cpp:62). Fix de 2 líneas desplegado (`metin2client.exe` 5.115.904 B, 14:12, hash C7EAD7CC). **Pendiente: verificación final del usuario (entrar 2-3 veces).** Detalle en AGENTS.md y CHANGELOG.
- **Language System 1.2.6:** integrado y cargando (16 idiomas, 764-775 entradas c/u). Huecos A+B+C del servidor y 181 claves pendientes — **los huecos de textos del servidor quedan superados por el diseño nuevo** (textos servidor→cliente por manifest, §4.6 del plan único).
- **PLAN DE REESCRITURA UNIFICADO ESCRITO (2026-08-09):** `docs/superpowers/plans/2026-08-09-servidor-rust-plan-unico.md` — consolida arquitectura, anti-hack, DB, DSL de quests, migración, canales regionales, cliente modificable. 12 preguntas abiertas para revisores externos.
- **Auditoría legacy vs estándares 2026 completada:** 14 decisiones P0/P1/P2 que NO se arrastran (con evidencia archivo:línea) + 7 cosas que hace bien y se conservan. Ganancia estimada: 2-5x CPU, techo 1.000+ jugadores/instancia.
- **Quests: DSL propio DECIDIDO (sin Lua):** spec en `docs/superpowers/specs/2026-08-09-quest-dsl-spec.md` (integrado en el plan único §11). Familias + bloques + imports eliminan las ~2.500 líneas duplicadas del corpus de 194 quests.
- **Cliente: regla de esfuerzo ≤1 semana por cambio** (nada prohibido; coste/beneficio). 7 modificaciones aditivas identificadas con evidencia (version check, hardware ID, server time, dwLoginKey, paquetes pull 162+, overrides UTF-8, lista de canales desde auth + config vía manifest).
- Grafos graphify: server **13.190 nodos / 33.233 edges**, client **17.951 nodos / 40.116 edges** (refrescar tras cambios).
- Pendientes de la línea base C++: verificar fix del crash, revisar 17 SYSERR de boot preexistentes.

## Principios de la reescritura

1. **Hacer más con menos**: menos código, menos complejidad, menos dependencias; la calidad nace de lo necesario.
2. **Rediseño estructural, no traducción línea por línea** (ADR-0001).
3. **Servidor autoritativo**: el cliente envía intenciones, el servidor calcula hechos; el cliente es una vista, nunca una fuente de verdad.
4. **La BD no calcula, garantiza**: la lógica de juego vive en Rust; PostgreSQL impone integridad (constraints, transacciones, locks, RLS, auditoría).
5. **Reemplazo incremental verificable** (strangler fig): cada módulo Rust preserva el comportamiento observable de su homólogo C++ y pasa verificación (harness de paridad) antes de avanzar.
6. **Cliente congelado como contrato durante F0–F6**, con excepción: cambios aditivos ≤1 semana que liberen al serverside (regla de coste/beneficio, no de prohibición).
7. **Paridad solo donde importa**: el comportamiento observable se conserva; el código interno no tiene por qué parecerse.
8. **ADR antes de implementar**: límites de dominio, propiedad de datos, protocolos, concurrencia, fallos y migración se deciden por escrito primero.
9. **Hot reload por diseño**: textos, items, quests y config se editan en la BD y se recargan en runtime (NOTIFY + manifest) — sin reiniciar ni recompilar.

## Fases

### Fase 0 — Fundaciones (workspace, ADRs, protocolo)

Objetivo: esqueleto del workspace Rust + decisiones de arquitectura cerradas por ADR + crate de protocolo con el flujo de login verificado.

- [x] **ADR-0002: unificación `game` + `db`** (ACEPTADO: un proceso por región, db como crate; shim legacy durante F3–F5, unificación en F6)
- [x] Stack Rust investigado y fijado: **tokio 1.49 + sqlx 0.9 (PgPool) + bevy_ecs standalone + config-rs + clap 4.6 + tracing + proptest** (sin mlua — quests en DSL propio; regiones + ECS, no actores)
- [x] Crate `protocol`: **spec byte-exacto del flujo de login completado** (`docs/superpowers/specs/2026-08-08-wire-protocol-login-flow.md`)
- [x] **Plan unificado escrito** (`docs/superpowers/plans/2026-08-09-servidor-rust-plan-unico.md`)
- [x] Auditoría legacy completa (§2.3 del plan único)
- [ ] ADR: límites de dominio y propiedad de datos (char.cpp por sistemas sobre Entity mínimo en ECS)
- [ ] ADR: concurrencia (regiones + ECS; world task nunca await SQL inline)
- [ ] ADR: motor de quests (DSL propio, sin scripting)
- [ ] ADR: modelo anti-hack (server-authoritative + envelope + transacciones)
- [ ] ADR: canales regionales (BD central + proceso por región; anti-doble-login con row locks)
- [ ] ADR: capa de datos (WAL local + mutation_id + RLS + failover; contrato durable/volátil)
- [ ] ADR: datos servidor→cliente (manifest versionado + delta + hot reload)
- [ ] ADR: migración MySQL → PostgreSQL (en F3, no F6)
- [ ] Workspace Cargo con crates: `protocol`, `net`, `db`, `game`, `auth`
- [ ] Implementar crate `protocol` (login flow) con golden tests de los structs del spec
- [ ] Harness de verificación: captura de paquetes reales (tcpdump/Wireshark contra el server C++) como golden tests
- [ ] **Repositorio GitHub**: solo fuentes (~150-200 MB); binarios/pack/backups a Releases o storage externo; `.gitignore` de artefactos de build, clientes instalados, graphify-out, .opencode

**Hito F0:** un LOGIN3 real capturado se parsea y re-serializa byte a byte idéntico.

### Fase 1 — Red y transporte

Objetivo: reemplazar `libthecore` + fdwatch por tokio, con paridad de comportamiento.

- [ ] Listener TCP (tokio/mio) con backpressure equivalente a la semántica verificada: **consumir `result > 0` bytes; `0` = EAGAIN; `-1` = error** (fix #1/#2/#6)
- [ ] Framing de paquetes (header + body)
- [ ] Handshake con retries de bias de reloj (~40-80ms) como el cliente real (no se elimina: pasa una vez en login, beneficio nulo, riesgo alto)
- [ ] Reconexión y reautenticación del flujo de entrada al mundo — resuelto server-side con sesiones por key (no password)

**Hito F1:** el auth binario C++ se conecta a un peer Rust y viceversa, sin timeouts ni floods de WRITE.

### Fase 2 — Auth + primer lote de cliente (semana 1)

Objetivo: puerto del slice auth (modo AUTH_SERVER del binario game) + las 4 modificaciones de cliente más baratas.

- [ ] Flujo: `GC_PHASE` + `GC_HANDSHAKE` → `CG_HANDSHAKE` echo → `LOGIN3` (65 bytes: `0x6F` + name[31] + pwd[17] + keys[16])
- [ ] Verificación de hash: **`mysql5_password` = `"*" + UPPER(SHA1(UNHEX(SHA1(pw))))`** — el asterisco es parte del formato (fix #5/#11)
- [ ] `GC_AUTH_SUCCESS` (0x96 + key + result)
- [ ] **Cliente (≤1 semana): version check al conectar** (rechazo limpio por versión; gatea evolución de protocolo)
- [ ] **Cliente: hardware ID en LOGIN3** (bans por hardware, anti-multibox)
- [ ] **Cliente: server time** (timers consistentes con reloj del servidor)
- [ ] **Serverside (sin tocar cliente): validar `dwLoginKey` (LOGIN_BY_KEY)** — el password no se reenvía en claro en reconexiones (sesiones tokenizadas)

**Hito F2:** login contra auth Rust + db C++; el cliente recompilado pasa el version check.

### Fase 3 — Capa de datos + canal de datos

Objetivo: crate `db` por dominios tras trait de backend + migración a PostgreSQL + paquetes pull-based en el cliente.

- [ ] Crate `db` organizado por módulos de dominio: account/world/social/economy/log (esquemas PG separados, permisos por esquema, RLS)
- [ ] Backend trait: `direct-sql` (MariaDB, mínimo, solo lo que el game C++ necesita) → `postgres` (destino en esta fase, no F6)
- [ ] Portar por QID: login → player load/save → items → social
- [ ] Pipeline durable: **WAL local por región + mutation_id + batch ≤100ms + replay idempotente** (`ON CONFLICT DO NOTHING`)
- [ ] Ruteo SQL: `SQL_ACCOUNT` vs `SQL_PLAYER` (fix #8); `QUERY_LOGIN` 12 columnas (fix #7)
- [ ] Migrador MySQL → PostgreSQL + harness de comparación de datos
- [ ] **Cliente: paquetes aditivos pull-based** (headers 162+: CG_QUERY/GC_RESPONSE; registro tabla + case en PhaseLogin) — el canal de datos §4.6
- [ ] `PROTO_FROM_DB` mantenido

**Hito F3:** el game C++ corre contra db Rust sin cambios de comportamiento; el cliente recompilado recibe datos aditivos sin desincronizarse.

### Fase 4 — Entrada al mundo + nombres

Objetivo: selección de personaje + spawn con paridad + overrides de nombres UTF-8.

- [ ] `CG_PLAYER_SELECT` (header 6) → `GC_LOGIN_SUCCESS3`
- [ ] Spawn de personaje, mapa (`Venter_the_east.mp3`), stats
- [ ] **Cliente: overrides en memoria** (`SetLocaleName`/`SetItemLocaleName` tras `LoadLocaleData`) — el servidor manda nombres UTF-8 desde la BD; adiós mojibake y trampa CP949
- [ ] Entidades: núcleo Entity mínimo + sistemas en ECS (bevy_ecs standalone) — NUNCA portar char.cpp como clase única

**Hito F4:** el cliente real entra al mundo contra el core Rust con nombres correctos.

### Fase 5 — Gameplay básico + escala

Objetivo: core jugable por dominios, side-by-side, benchmark de escala, y el resto de cliente.

- [ ] Movimiento: envelope de velocidad por entidad + walkability del mapa + corrección (anti-speedhack) + lag tolerance
- [ ] Combate: daño íntegro servidor + cooldowns por reloj del servidor + rango/LoS
- [ ] Drops, items, inventario: transacciones atómicas (mats → resultado → oro en un commit)
- [ ] NPCs, quests (motor DSL + conversor automático del corpus + harness de paridad), chat, tiendas, safebox, trade, GM
- [ ] **Cliente: lista de canales desde el auth** (override de serverinfo.py — adiós IP horneada)
- [ ] **Cliente: config vía manifest** (rates, límites visibles — ajuste sin recompilar)
- [ ] Hot reload operativo: NOTIFY → recarga → bump manifest → delta
- [ ] **Slint standalone** (UI de login/select/HUD contra el servidor real, en paralelo — se reutiliza en F7)
- [ ] Benchmark de escala: N bots × N regiones (gate antes de considerar multi-proceso)
- [ ] API REST + metrics (Prometheus/Grafana) + Docker (features de primera clase)

**Hito F5:** sesión de juego completa sin divergencias observables + benchmark superado.

### Fase 6 — Paridad total e integración

- [ ] Side-by-side automatizado: misma entrada de paquetes → diff de respuestas Rust vs C++
- [ ] Suite de golden tests ampliada a todo el tráfico de una sesión real
- [ ] Migración de datos completa y verificada (backup/restore; failover con Patroni)
- [ ] Reemplazo final: instancias `srv1` corriendo 100% Rust

**Hito F6:** el servidor Rust reemplaza al C++ en producción de pruebas sin cambios en el cliente.

### Fase 7 — Cliente (después del servidor)

> Decisiones abiertas que se resolverán con ADRs propios. La UI se diseña en Slint (app standalone desde F5, se integra por textura en el cliente nuevo). El cliente se rehace con wgpu; la UI Slint existente se reutiliza (los `.slint` sobreviven).

- [ ] Cliente Rust (wgpu), protocolo nuevo, cifrado real
- [ ] UI Slint integrada (login → select → HUD — lo hecho en F5 standalone se conserva)
- [ ] Límites del cliente legacy (24 chars, 5 personajes, stack 200) revisables con cliente nuevo
- [ ] Formatos de pack: solo se preservan las herramientas (PackMakerLite, TEA/LZO, DumpProto) si se reutilizan

## Decisiones abiertas (para ADRs y revisores)

1. **DSL de quests** (spec §11.11): `between` nativo; `if` 1 nivel + else; `select` con captura `as`; claves `@clave` vs literal; naming `.quest`/`.qdsl`/`.mq`; trigger `timer` explícito.
2. **Cliente F7**: motor (wgpu), protocolo nuevo, cifrado — sin detalle hasta F6.
3. **Regiones cross-server**: ubicación física de los procesos de regiones especiales (junto a la BD central propuesto).
4. **Comercio unificado**: cierre de pujas con reloj de la BD (decidido el principio; detalles de la subasta en F5).
5. **Licencia**: MPL-2.0 propuesta (AGPL repele a operadores de pservers) — confirmar con la comunidad.
6. **Timing API web + metrics**: desde F5 (propuesto) vs después del corte.

## Repositorio GitHub (preparación)

- **Solo fuentes al repo** (~150-200 MB): `source\server`, `source\client` (sin artefactos de build), `source\pack` (sin .epk), `source\tools` (incluye `proto\`), `scripts\`, `docs\`, `AGENTS.md`, `ROADMAP.md`, `CHANGELOG.md`.
- **NO van al repo**: `client\` (cliente instalado, pack 2.1 GB), `client-om2\` (referencia descargada), `archive\` (backups), `Extern\` (dependencias), artefactos de build (obj/bin/Debug/Release ~2,4 GB), `graphify-out\`, `.opencode\`, `systems\`.
- **Binarios** (cliente instalado, .epk, builds) → GitHub Releases (no cuenta contra el límite del repo) o storage externo; se generan con los scripts de build.
- `.gitignore` raíz con todos los patrones anteriores antes del primer push.

## Cómo se lleva la cuenta

- **`CHANGELOG.md`** — registro cronológico: cada cambio verificado se anota (fecha, qué cambió, evidencia). Lo mantiene el orchestrator al final de cada sesión.
- **`AGENTS.md`** — estado actual, hechos verificados y reglas. Se actualiza cuando cambia el conocimiento del proyecto.
- **`docs/decisions/`** — ADRs. Toda decisión de arquitectura se escribe ANTES de implementar.
- **`docs/superpowers/plans/2026-08-09-servidor-rust-plan-unico.md`** — el diseño completo de referencia (único archivo).
- **Grafos** — `graphify update` sobre `source\server` y `source\client` tras cambios de código relevantes.
