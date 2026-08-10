# Reescritura del servidor de Metin2 en Rust — Propuesta para discusión

> **Metadata**
> - Type: History
> - Status: Historical
> - Audience: Project agents and maintainers (historical context only)
> - Last verified: 2026-08-10
> - Original location: `docs/superpowers/plans/2026-08-09-servidor-rust-draft-discusion.md`
> - **Historical record.** Archived for context. This document is NOT current normative guidance: it was the v0.1 discussion draft superseded by `docs/plans/server-rewrite.md` (the active rewrite plan). Architecture decisions live in `docs/decisions/` (ADRs). Statements, phase dates and stack choices in this file must not be treated as binding today.

> **Estado: DRAFT v0.1 — documento de discusión.** No es el plan final.
> **Propósito:** presentar la visión, las decisiones y la estrategia a revisores externos para que aporten antes de fijar el plan definitivo.
> **Fecha:** 2026-08-09 · **Autor:** equipo del proyecto (orchestrator + revisión de arquitectura)
> **Feedback solicitado:** sección «Preguntas para los revisores» al final.

---

## 1. Resumen ejecutivo

Metin2 (MMORPG de 2004, creado por Ymir Entertainment) tiene un servidor C++ monolítico de ~120k LOC con décadas de deuda: código espagueti, lógica de juego validada en el cliente (fuente de la mayoría de hacks), duplicación masiva, y decisiones de arquitectura que fueron razonables en 2004 y hoy son lastre.

**Propuesta:** reescribir **todo el servidor** en Rust, no como traducción línea por línea, sino como **rediseño estructural**: mismos contratos observables (el cliente real sigue funcionando durante toda la migración), pero con arquitectura moderna, modelo **server-authoritative** (el servidor calcula todo; el cliente solo envía intenciones), y una capa de datos sobre **PostgreSQL 18** como red de seguridad transaccional.

**Lema rector: hacer más con menos** — menos código, menos complejidad, menos dependencias; la calidad nace de lo necesario, no de lo recortado.

El reemplazo es **incremental y verificable** (patrón strangler fig): módulo por módulo, con el servidor C++ como oráculo de tests hasta el corte final. El cliente se congela como contrato durante todo el port del servidor.

---

## 2. Contexto: qué es el servidor legacy y por qué reescribirlo

### 2.1 Estado actual

- Servidor C++ (MartySama 5.9): binarios `game` (monolito, ~104.7k LOC) y `db` (~12.8k LOC); `auth` es un modo del binario game (`AUTH_SERVER`), no un proceso separado.
- Cliente C++ v40999 (S3llMetin2 v24) — **intacto durante el port**; es el contrato de protocolo byte-exacto.
- Base de datos actual: MariaDB con esquema heredado y datos en CP949 (encoding coreano de 2 bytes).
- El login completo (auth + canal + selección de personaje) está verificado y funcionando contra el cliente real.

### 2.2 Decisiones de Ymir que NO repetimos

| # | Decisión de Ymir (2004) | Consecuencia real | Decisión de reemplazo |
|---|---|---|---|
| 1 | Cálculo y validación en el **cliente** (posición, velocidad, daño, stats) | Speedhack, teleport, god-mode, memory hacking | **Servidor autoritativo**: el cliente envía intenciones; el servidor calcula hechos |
| 2 | Mutaciones económicas sin transacciones atómicas (drop/trade/refine) | Dupe por carreras y rollbacks | Transacciones ACID por mutación + single-writer del mundo |
| 3 | Queries SQL construidas por concatenación de strings | SQL injection | Queries parametrizadas (sqlx compile-time checked) |
| 4 | Cifrado de paquetes casi simbólico, opcional y desactivado por defecto | Sniffing, packet forging trivial | Cifrado moderno real (a decidir: se hereda plaintext solo mientras el cliente legacy viva) |
| 5 | God object `char.cpp` (6.6k LOC) + copy-paste masivo | Deuda infinita, bugs por divergencia | Núcleo `Entity` mínimo + sistemas por dominio |
| 6 | Lua 5.0 con lexer modificado para EUC-KR (2 bytes/carácter) | Encoding roto, quests frágiles, scripting general donde solo hace falta estructura | **DSL de quests propio y declarativo** (sin lenguaje de scripting; spec `2026-08-09-quest-dsl-spec.md`) |
| 7 | Event loop custom `fdwatch`/`select` con bugs de backpressure | Flood de WRITE, reconexiones rotas | tokio (gestión de interés de escritura interna) |
| 8 | Estado duplicado entre `game` y `db` (cachés por ambos lados) | Inconsistencias, protocolo interno entero que mantener | Unificación game+db en un proceso Rust por canal; PostgreSQL como estado compartido |
| 9 | Protocolo binario con structs cuyo tamaño cambia según flags de compilación | Fragilidad extrema, rompe compatibilidad | Crate `protocol` único con golden tests byte-exactos |
| 10 | Sin tests, sin verificación reproducible | Cada fix rompía otro | Tests de paridad contra el legacy + harness de captura de paquetes reales |

### 2.3 Auditoría del legacy contra estándares 2026 (evidencia en código)

Auditoría directa de `source\metin2_server\Srcs\Server` (104.9k LOC game + 12.8k db + 3.3k common) + grafo graphify (god node #1 del servidor: `CHARACTER`, 815 edges) + investigación de la industria (agosto 2026). **Esto es la respuesta al riesgo «arrastrar la estructura vieja»: cada decisión del legacy que NO se copia está listada con archivo:línea.**

**Lo que el legacy hace BIEN y se conserva:** sectree como interest management (no reinventar), AI de mobs mínima sin pathfinding (decisión correcta), patrón anti-hack parcial en `SyncPosition` (input_main.cpp:1758 — la semilla del anti-speedhack), save asíncrono fuera del world loop, catálogo QID tipado (contrato de persistencia), `MoneyLog` (semilla del anti-dupe), y el single-thread sin locks globales (la propiedad «world single-writer» se hereda, elevada a **single-writer por región** en el modelo Rust).

| Prio | Decisión legacy (evidencia) | Estándar moderno | Ganancia |
|---|---|---|---|
| **P0** | Movimiento propio sin validación de distancia (`ENABLE_TP_SPEED_CHECK` comentado, input_main.cpp:1455) | Envelope de velocidad por entidad con reloj servidor | Elimina speedhack/teleport |
| **P0** | Cooldowns de skill sin chequeo server (`ENABLE_SKILL_COOLDOWN_CHECK` ausente, char_skill.cpp:107) | Cooldowns validados en servidor | Elimina skill-spam/attack-rate |
| **P0** | Persistencia write-behind 30 min sin atomicidad (Cache.cpp:21,101; char.cpp:1109) | Clases de persistencia: durable = tx por mutación; volátil = periódico | Elimina dupe por rollback; pérdida acotada |
| **P0** | Config por compilación compartida cliente/servidor (CommonDefines.h ~80 flags) | Config runtime + feature flags por instancia | Fin del re-build cruzado |
| **P0** | Observabilidad `sys_log` + `fflush(stdout)` por llamada (log.c:171,218) | tracing estructurado + métricas + niveles | Debug de producción en minutos |
| **P1** | `select()` O(fds) (fdwatch.c:400) | tokio/epoll + backpressure por conexión | Escalabilidad; elimina flood WRITE |
| **P1** | Tick global con allocs por tick, O(todas las entidades) (char_manager.cpp:641-666) | Tick particionado por región, presupuesto de tiempo medido | Techo 1.000+ jugadores/core |
| **P1** | Broadcast serializado por receptor (entity_view.cpp:36-239) — 40k serializaciones/seg en movimiento | Serializar una vez + referencias; batching | ~40x menos serialización |
| **P1** | God object `CHARACTER` (815 edges, char.cpp 6.6k LOC) | Entity mínimo + sistemas | Testeable y extensible |
| **P1** | Código muerto: cifrado TEA/DES/GOST desactivado (tea.s 121KB), liblua/5.2 completo sin usar | No portar nada sin llamadores; `dead_code` como error | Menos superficie |
| **P2** | Build de deploy con AddressSanitizer activo (Makefile) | Release limpio; sanitizers solo en CI | 2-3x CPU recuperados |
| **P2** | Copy-paste de contenido (11+ archivos `collect_quest_lvXX`) | Datos parametrizados (familias en el DSL de quests) | Contenido sin duplicar |
| **P2** | Sistemas paralelos incompatibles (BlueDragon vs DragonLair; shop vs shopEx) | Un sistema por concepto, datos config | Menos código, menos divergencia |
| **P2** | Cero tests en todo el repo server | Golden tests + harness de paridad (ROADMAP F0/F6) | Condición de avance |

**Ganancia estimada del Rust (mismo modelo, mejor implementación): 2-5x CPU disponible en el tick** (select→epoll, sys_log→tracing, allocs→sin temporales, ASAN→release). Con modelo distinto (interest management explícito + batching + save por clase): techo de ~300-500 → **1.000+ jugadores/instancia**.

**Validación por la industria (2026):** TCP es la elección correcta (WoW, FFXIV, EVE, GW2 y ESO usan TCP; Veloren abandonó UDP por TCP; el rewrite más moderno, Ember, usa Boost.Asio/TCP). **ECS SÍ para esta visión** (decisión del usuario: mundo único sin canales, más mobs, más jugadores): Veloren (Rust, ECS specs) escala mundos de millones de entidades precisamente con ECS + regiones en paralelo; bevy_ecs standalone aporta queries paralelizables sin arrastrar el motor gráfico. Sin CQRS/event sourcing/outbox para el estado del juego (sobre-ingeniería en proceso único; solo el log de eventos económicos append-only, ya en el plan). Tick de mundo 10-20 Hz (TrinityCore: 20 Hz). Mundo único con regiones en paralelo: el patrón de TrinityCore `MapUpdater` (multihilo por mapa) y EVE (nodos por sistema solar, récords de 2.670-6.557 CCU en un sistema) como referencia de techo.

---

## 3. Principios de la reescritura

1. **Hacer más con menos**: YAGNI, stdlib/nativo antes que dependencias, una línea antes que cincuenta.
2. **Rediseño estructural, no traducción**: los límites de dominio, propiedad de datos, protocolos, concurrencia y fallos se deciden por escrito (ADRs) antes de implementar.
3. **Servidor autoritativo**: el cliente es una vista; nunca una fuente de verdad. Todo input del cliente se valida contra el estado del servidor.
4. **La BD no calcula, garantiza**: la lógica de juego vive en Rust; PostgreSQL impone integridad (constraints, transacciones, locks, auditoría).
5. **Reemplazo incremental verificable**: cada módulo Rust preserva el comportamiento observable de su homólogo C++ y pasa verificación antes de avanzar.
6. **Cliente congelado como contrato**: durante F0–F6 el cliente C++ real no cambia; el servidor Rust habla su protocolo byte-exacto.
7. **Paridad solo donde importa**: el comportamiento observable se conserva; el código interno no tiene por qué parecerse.
8. **Un solo proceso por canal**: `game` + `db` unificados en Rust (ADR-0002), db como crate interno; `auth` como modo del mismo binario.

---

## 4. Arquitectura objetivo

### 4.1 Visión general

```
Cliente C++ (congelado)          Cliente Rust futuro (F7)
        │                                │
        ▼                                ▼
┌──────────────────────────────────────────────────────────────┐
│            Servidor Rust — MUNDO ÚNICO (sin canales)         │
│  ┌──────────┐                                                │
│  │   net    │      ┌───────────┐   ┌───────────┐             │
│  │ (tokio)  │─▶    │ región 1  │   │ región N  │  …          │
│  └──────────┘      │ (task +   │   │ (task +   │             │
│                    │  ECS)     │◀─▶│  ECS)     │  mpsc de    │
│  ┌──────────┐      │ mobs,     │   │ mobs,     │  eventos    │
│  │  auth    │      │ jugadores │   │ jugadores │  inter-     │
│  │  (modo)  │      └─────┬─────┘   └─────┬─────┘  región     │
│  └──────────┘            │               │                   │
│  ┌───────────────────────▼───────────────▼───────────────┐  │
│  │  db crate (sqlx, async): queries → resultados por     │  │
│  │  mpsc (las regiones NUNCA await SQL inline)           │  │
│  └───────────────────────┬───────────────────────────────┘  │
└──────────────────────────┼──────────────────────────────────┘
                           ▼
               ┌───────────────────────┐
               │   PostgreSQL 18.4     │
               │  (estado + integridad)│
               └───────────────────────┘
```

Nota: cada entidad (y su inventario/oro) pertenece a UNA región en cada momento — single-writer por región, anti-dupe intacto.

### 4.2 Modelo de concurrencia

- **tokio, task-per-connection** para la red (no actores). **Mundo único por servidor — sin canales**: un canal legacy es una copia del mundo por puerto; el servidor Rust es **una colección de regiones** (agrupaciones de sectrees) que simulan en paralelo.
- **Una task tokio por región, con ECS dentro (bevy_ecs standalone)**: queries paralelizables (par_iter/rayon) dentro de la región, cero locks compartidos. Cada entidad (y su inventario/oro) pertenece a UNA región en cada momento → **single-writer por región, anti-dupe intacto**; migración de entidad entre regiones con handshake.
- **Regla de oro: el task de región NUNCA hace `.await` de SQL inline.** Reproduce el patrón `ReturnQuery` del legacy: emite la query, el crate db la ejecuta en el runtime multihilo, el resultado vuelve como evento por mpsc. Un query lento jamás detiene el mundo.
- Comunicación inter-región por `mpsc` de eventos (chat global, gremios, migraciones). Broadcast por región con política explícita ante saturación (drop-to-newest para posición, encolar para eventos).
- Timers con binary heap (mismo patrón que el `event_queue` legacy).
- **Escalado**: más jugadores/mobs → región más fina o más regiones, sin multiplicar procesos. Si un proceso no basta (escala futura), el siguiente paso es multi-proceso por grupos de regiones — la frontera de eventos ya existe. No se construye hasta que el benchmark lo pida (gate F5: N bots por región + N regiones).

### 4.3 Descomposición en dominios

El monolito se porta **por sistemas sobre un núcleo Entity mínimo** (VID, posición, sectree, estado de vida) **en un ECS (bevy_ecs standalone)** — nunca como clase única. Esta es la decisión de arquitectura más importante del proyecto.

| Dominio | Archivos legacy | Dificultad | Notas |
|---|---|---|---|
| Protocolo/wire | `packet.h`, `packet_info.cpp`, `tables.h` | 1 | Contrato byte-exacto; ya specced |
| Net/transporte | `libthecore` (fdwatch, socket, buffer, tea/des) | 1-2 | Reimplementar semántica, no portar C |
| Auth/login | `input_auth`, `input_login`, path auth de `db.cpp` | 1 | Modo AUTH_SERVER; primer slice vertical |
| Capa de datos | `db/src/*` (~12.8k) | 2 | Crate interno; portar por QID |
| Mundo/espacio | `sectree_manager`, `char_manager`, `dungeon`, `building` | 3 | Mundo entero en RAM |
| Entidades | `char.cpp` (6.6k), `char.h`, `char_state`, affects | 3 | **Portar por sistemas, no como god object** |
| Movimiento | Sync en `char.cpp`, `entity_view` | 2-3 | Aquí vive el anti-speedhack |
| Items/inventario | `char_item` (6.6k), `item`, `refine`, `blend`, `cube`, `safebox`, `shop` | 3 | Máximo riesgo dupe; transacciones obligatorias |
| Combate | `char_battle`, `battle`, `pvp`, `skill_power` | 3 | Daño 100% servidor (ya lo hace el legacy) |
| Skills | `char_skill`, `skill`, buffs | 3 | Sin SQL |
| NPC AI + spawn | `mob_manager`, `regen`, FSM | 2 | FSM trivial; sin pathfinding A* en el server |
| Quests | `questmanager`, `questlua_*` (~10k bindings) | 3 | DSL propio declarativo + motor de estados Rust (spec `2026-08-09-quest-dsl-spec.md`) |
| Social | `party`, `guild`, `guild_war`, `messenger`, `marriage` | 2-3 | SQL-heavy; cross-canal |
| Economía | `exchange`, `safebox`, `shop`, `fishing`, `mining` | 2-3 | Anti-dupe crítico |
| Admin/GM | `cmd_gm` (3.8k), `cmd_general`, `gm.cpp` | 2 | Permisos re-chequeados en BD |
| Config/locale/encoding | `config`, `locale_service` | 2 | Trampas de encoding conocidas |
| Eventos/raids | `BlueDragon`, `DragonLair`, `OXEvent`, `wedding`, `arena`, `war_map` | 2-3 | **Portar todo, pero diferido** (ver §8) |

### 4.4 Mundo único con canales regionales (EUW/LAN/LAS) — el modelo Metin2 distribuido

**Requisito del usuario:** un solo servidor para todos — una cuenta, un personaje con absolutamente todo (inventario, oro, quests, gremio) en cualquier región. Canales por región (EUW, LAN, LAS…) por el ping; cambiar de región = elegir canal y loguear, sin transferencias ni trámites.

**Modelo:** el sistema de canales de Metin2 (los canales comparten la BD → el personaje está en todos), distribuido geográficamente:

- **Una BD central (PostgreSQL)** con TODO el estado durable del juego: cuentas, personajes, inventario, oro, quests, gremios.
- **Un proceso por canal-región** (EUW, LAN, LAS…): cada uno corre su copia del mundo en memoria (mobs, mercado, PvP son por canal — comportamiento Metin2).
- **Login en cualquier región**: eliges canal y logueas; el personaje carga desde la BD central con todo. Cambiar de región = logout → login en la otra. No hay transferencia: la BD es la fuente de verdad.
- **Anti-doble-login cross-región**: row lock/advisory lock sobre la fila del personaje al entrar (la `LoginData` del legacy, ahora en PG — decisión de ADR-0002); al salir se libera. Nunca dos canales con el mismo personaje a la vez (eso sería el clon/dupe).
- **Durable = write-through** (ya decidido en §5): inventario/oro/quests se escriben en la BD central por mutación, en batch asíncrono que no bloquea la región. Por eso cambiar de canal-región no pierde NADA: la BD siempre está al día. Posición/HP (volátil) son locales del canal.
- **Coordinación cross-canal en PG**: guilds y cachés vía LISTEN/NOTIFY para invalidación (ADR-0002).

**Latencia — matiz honesto:** la BD central no está en el hot path (regla «nunca await SQL inline»), el ping del jugador depende solo de su canal regional. La BD central recibe escrituras batch asíncronas; si la BD vive en un continente, el lag de persistencia (100-200ms para LAS↔EUW) es irrelevante para el gameplay — nadie espera a la BD en vivo.

**Lo que NO es:** un mundo vivo único tipo EVE (ver en vivo al jugador de otro continente, mismo boss en el mismo instante, migración de estado en caliente entre nodos) — problema de investigación de consistencia transcontinental que ningún MMO ha resuelto; además NO es lo que describe el requisito («un canal en euw, un canal en lan» = mundo vivo por canal, datos compartidos).

**Regiones cross-server (permanentes y temporales):** mapas que no pertenecen a ningún servidor-región, accesibles desde todos — p.ej. una isla de guerra o un continente PvP permanente donde se ven y pelean jugadores de Europa, EUW y América. Técnicamente es una **región especial** con su propio proceso (ubicación a decidir, típicamente junto a la BD central), a la que los personajes migran temporalmente (mismo mecanismo de migración de §4.4, single-writer intacto) y de la que vuelven a su servidor de origen al salir. Ping de ~150-200ms para los lejanos (jugable; el legacy ya se juega así desde LAT contra servidores EU). Es un caso más de la frontera de regiones ya diseñada, no un sistema nuevo.

### 4.5 Capa de datos — organización por módulos de dominio

PostgreSQL 18 + sqlx 0.9 (ADR-0001). El crate `db` se organiza **por dominios**, cada uno con esquema propio, migraciones versionadas y repositorios:

```
db crate
├── account/  → esquema account (cuentas, login, bans)
├── player/   → esquema world   (personajes, items, quests)
├── social/   → esquema social  (guilds, parties, messenger)
├── economy/  → esquema economy (auction, money log, trade history)
└── log/      → esquema log    (auditoría append-only)
```

Permisos PG por esquema (log no escribe en economy — defensa en profundidad). Contrato estricto: **mundo en memoria = autoridad viva (cero SQL en hot path); BD = persistencia** — writes durables en batch transaccional, reads solo en boot/cambio de canal. Todo el estado duradero (items, oro, quests, personajes, gremios) es persistente por requisito.

**Pipeline de persistencia durable (el diseño final):**

```
región (mundo en memoria)
  │  mutación durable (mutation_id uuid)
  ▼
WAL local por región (append-only, fsync batch ~ms)
  │  (si crash → replay al arrancar con ON CONFLICT DO NOTHING)
  ▼
PostgreSQL central (tx batch ≤100ms, uuidv7, CHECK gold>=0)
  │
  ├─ log de auditoría append-only (misma tx, OLD/NEW en RETURNING)
  └─ replicación: hot standby + failover automático (Patroni)
```

- **Cada mutación durable lleva `mutation_id` (uuid)**: aplica-una-sola-vez garantizado (replay idempotente), y es la columna vertebral que une WAL, PG y auditoría.
- **Sin ventana de dupe**: crash = pérdida máxima del batch en vuelo (~100ms), nunca items ya confirmados; el replay restaura el resto.
- **RLS (row-level security) en world/economy**: cada query lleva `current_setting('app.pid')`; PG rechaza filas de otro jugador — red de seguridad contra bugs.
- **Failover**: hot standby replicando, promoción automática ~2 min; las regiones siguen el mundo en memoria con writes encolados.
- **Log de auditoría particionado por fecha + retención + `pg_stat_statements`** desde el día uno.
- **Migración**: backend MaríaDB mínimo solo lo que el game C++ necesita (F2–F3); migración a PG en F3 (los datos ya existen, el migrador se escribe igual), no en F6 — la dualidad de esquemas muere antes. Sin backend peer-legacy.
- **Contrato fijado en ADR**: durable = WAL local + batch ≤100ms; volátil = save cada 30s + logout; failover ≤2 min.

**Decisiones que se fijan YA:**
- IDs globales `uuidv7()` (sin colisiones entre canales-región; ya en el stack con PG 18).
- Cuenta central: el módulo auth sirve a todas las regiones contra la BD de cuentas compartida.
- El crate `db` y PG son la frontera cross-canal (ADR-0002); el snapshot versionado de personaje queda como contrato para backups/import-export/migraciones, no como mecanismo de cambio de región.

### 4.6 Datos servidor→cliente (manifest versionado + delta download)

**Problema legacy:** el cliente es una fuente de datos (nombres/descripciones de items y NPCs en su pack `.epk`, textos de quest en locale.lua por idioma) separada del servidor — cambiar un nombre = editar BD + repaquear cliente + patcher. Dolor de miles de archivos duplicados.

**Solución (cliente C++ con modificaciones aditivas — solo 1-2 paquetes nuevos, sin tocar render/gameplay):** el servidor es la única fuente de datos; el cliente solo renderiza.

- **Items/NPCs**: el servidor envía los datos (nombres, descripciones) al login vía **manifest versionado + delta**: el cliente pide "¿versión 42?", el servidor responde "hay 43" y envía SOLO lo que cambió (KB). El pack queda solo para visuales (iconos, modelos). Adiós a la trampa CP949, al repaqueo y al patcher por boludeces.
- **Textos de quest**: viven en la BD por locale (`account.lang` decide el idioma); el servidor los envía localizados — mata las 181 claves faltantes y la mezcla ES/EN. Cero archivos por idioma.
- **Patcher** queda solo para binario del cliente y visuales; los datos viajan por delta.
- El manifest se genera de la BD (única fuente de verdad) — sin copias manuales.
- Seguridad: el server manda los datos (tú eres el server) — sin riesgo adicional.

---

## 5. Modelo anti-hack (server-authoritative)

**Principio rector: el cliente envía intenciones, nunca hechos.** Todo dato que el cliente podría haber editado en memoria es no-confiable; el servidor recomputa desde su propio estado.

| Hack | Contramedida |
|---|---|
| Speedhack / teleport | Envelope de velocidad por entidad (distancia máx. / tiempo, por modo andar/correr/montura) con reloj del servidor + walkability del mapa; posición re-broadcasteada por el servidor |
| God-mode / one-shot / attack-speed | Daño íntegro en servidor con stats de servidor; cooldowns por reloj del servidor; rango y LoS vía sectree |
| Dupe (la clase reina) | (1) **Single-writer del mundo**: un task serializa TODAS las mutaciones de items; (2) transacciones atómicas de alto valor: trade/refine/cube/shop/drop = state machines de servidor con un único commit; (3) IDs de item desde sequence de PostgreSQL con batches; (4) política de save explícita |
| Memory hacking del cliente | El cliente es solo una vista; nada de lo que muestra se lee del cliente |
| Paquetes falsos / commands GM | Máquina de estados de fase estricta; permisos re-chequeados en BD; rate limits de chat/acciones |
| SQL injection | Queries parametrizadas compile-time (sqlx) |
| Packet floods / spam | Rate limiting por conexión: paquetes/seg, bytes/seg, límites por acción (chat, trade, skill, loot) — el legacy NO lo tiene |
| Bots de farm (la plaga de 2026: la mitad del top-10 de repos Metin2 son bots) | Telemetría de comportamiento server-side: análisis de rutas/ritmos de farm + flags para revisión humana; PoW anti-bot (mCaptcha, Rust) para la capa web de cuentas |

**Persistencia en dos clases explícitas:**
- **Durable** (items, oro, quest flags, guilds, safebox): write-through transaccional. Un crash NUNCA pierde ni duplica — el rollback a medias ES el dupe.
- **Volátil** (posición, HP, cooldowns): save periódico + logout. Perder segundos de posición es aceptable; perder items no.

---

## 6. Stack tecnológico (investigado contra fuentes primarias, agosto 2026)

| Capa | Elección | Justificación |
|---|---|---|
| Lenguaje | **Rust** (edition 2024) | Seguridad de memoria, cero-cost abstractions, ecosistema async maduro |
| Runtime async | **tokio 1.49** | Estándar de facto; tareas, mpsc, timers |
| Base de datos | **PostgreSQL 18.4** | ACID, MVCC, features 17/18: async I/O, `uuidv7()`, `OLD/NEW` en RETURNING (auditoría gratis), generated columns, LISTEN/NOTIFY, advisory locks, RLS, checksums por defecto, backups incrementales |
| Acceso BD | **sqlx 0.9** | Queries compile-time checked, migraciones integradas, pool propio |
| Quests | **DSL propio declarativo + motor de estados Rust** (parser pest/chumsky) | Cero runtime de scripting: quests = datos tipados (familias con parámetros, bloques reutilizables, imports) validados en load-time con errores archivo:línea. Contenido legacy (194 `.quest`) convertido automáticamente con harness de paridad. Casos raros (oxevent) → módulos Rust. Spec: `docs/reference/quests/quest-dsl.md` (original: `docs/superpowers/specs/2026-08-09-quest-dsl-spec.md`) |
| Config | config-rs + clap 4.6 | — |
| Observabilidad | tracing | — |
| Tests | cargo test + proptest + golden tests de paquetes | — |

**Descartadas con justificación** (para revisores): CockroachDB (licencia propietaria desde 24.3, resuelve escala multi-nodo que no existe aquí), TiDB/ScyllaDB (idem + modelo no relacional), SurrealDB (inmaduro), libSQL/Turso (proyecto en pausa), SQLite (single-writer global, insuficiente a esta escala), TimescaleDB (ADR-0001: solo si los logs lo demuestran).

**Nota sobre la idea «que la BD calcule todo»:** descartada deliberadamente. La lógica de juego en triggers/procedures SQL es un anti-patrón (imposible de testear, latencia, cuello de botella único) y no elimina hacks: el dupe es una carrera de guardado, no un problema de cálculo. La BD **garantiza** (constraints, transacciones, locks); el servidor **calcula**. Esa separación es la que mata los dupes.

---

## 7. Estrategia de migración (strangler fig)

### 7.1 Forma general

Slices verticales (cliente→auth→db→cliente) que ejercitan la pila completa, con el cliente congelado. El db legacy permanece como oráculo hasta el corte. El crate `db` se construye **tras un trait de backend**:

1. `direct-sql` (MariaDB) — default durante la migración;
2. `peer-legacy` (client-side del protocolo GD/DG) — solo para QIDs cross-canal (LoginData, ItemIDRange) mientras el deploy sea multi-instancia;
3. `postgres` — destino final (F6), con migración de datos verificada.

El shim legacy del ADR-0002 se degrada a **artefacto de test** (golden tests del catálogo de queries), no de deploy.

### 7.2 Fases

| Fase | Objetivo | Hito verificable |
|---|---|---|
| **F0** Fundaciones | Workspace Cargo, ADRs, crate `protocol` byte-exacto (login flow), harness de captura de paquetes | Un LOGIN3 real capturado se parsea y re-serializa idéntico |
| **F1** Red/transporte | Listener tokio con la semántica verificada (`result > 0` / EAGAIN), framing, handshake con retries | El auth C++ se conecta a un peer Rust y viceversa sin floods |
| **F2** Auth | Slice auth del binario game (modo AUTH_SERVER): LOGIN3, hash `"*"+UPPER(SHA1(UNHEX(SHA1(pw))))`, GC_AUTH_SUCCESS | Login contra auth Rust + db C++ con la cuenta de test |
| **F3** Capa de datos | Crate `db` tras trait de backend; portar **por QID**: login → player load/save → items → social | El game C++ corre contra db Rust sin cambios de comportamiento |
| **F4** Entrada al mundo | CG_PLAYER_SELECT, spawn, mapa, stats | El cliente real entra al mundo contra el core Rust |
| **F5** Gameplay | Movimiento, combate, drops, items, NPCs, quests, chat, shops, trade, GM — **por dominios, con side-by-side por dominio** | Sesión de juego completa sin divergencias observables + benchmark de escala (N bots × N regiones) |
| **F6** Paridad total | Side-by-side automatizado (misma entrada → diff de salidas), migración a PostgreSQL, corte instancia por instancia | El servidor Rust reemplaza al C++ sin cambios en el cliente |
| **F7** Cliente (después) | Diseño del cliente Rust (wgpu/Bevy), protocolo nuevo, cifrado real | — |

### 7.3 Feature set

**Portar todo** (el usuario lo confirmó; los eventos NO se descartan). **Orden:** core jugable primero (movimiento, combate, skills, items, drops, NPCs, quests, chat, tiendas, safebox, trade, GM), **eventos/raids/social masivo diferidos** (OXEvent, bodas, BlueDragon, DragonLair, 3 imperios, arena, guerra de gremios) a portar después, solo si se juegan — YAGNI: sin este recorte de orden, F5 es infinito.

---

## 8. Qué NO se porta (decisiones de simplificación deliberadas)

- **Código repetido**: los eventos raid del legacy son copias casi idénticas (BlueDragon, DragonLair, xmas…) → **un solo framework de encounter configurable por datos**. Esto es «hacer más con menos» en la práctica.
- **Cifrado legacy**: `_IMPROVED_PACKET_ENCRYPTION_` OFF + plaintext se hereda SOLO mientras el cliente C++ viva (contrato). El servidor Rust interno y el cliente Rust futuro (F7) usan cifrado real.
- **Encoding CP949 en el servidor**: se revierte a UTF-8 para contenido quest/locale (regla #15 del proyecto NO se transfiere al Rust). Las tablas de datos del boot (nombres CP949 referenciados por `etc_drop_item.txt`) se conservan byte-compatibles o se migran atómicamente con `item_proto`.
- **Lua y todo scripting**: eliminados. Quests = DSL declarativo propio (familias, bloques, imports; spec `2026-08-09-quest-dsl-spec.md`). El servidor Rust no tiene ningún runtime de scripting.
- **Duplicación de estado game↔db**: eliminada por diseño (ADR-0002).
- **Orden de arranque**: desaparece (solo PostgreSQL primero).

---

## 9. Riesgos y mitigaciones

| # | Riesgo | Mitigación |
|---|---|---|
| 1 | Paridad byte-exacta del protocolo (flags de build cambian tamaños: `ENABLE_ACCE_COSTUME_SYSTEM` +4B, `__LANGUAGE_SYSTEM__` +3B…) | Golden tests con capturas tcpdump reales en cada fase; crate protocol como única fuente; side-by-side de F6 automatizado |
| 2 | Traducción del god object `char.cpp` | ADR de límites de dominio ANTES de F4; portar por sistemas sobre Entity mínimo; cada sistema validado side-by-side |
| 3 | Conversión del corpus de quests (194 `.quest` con lógica imperativa) | DSL propio declarativo (spec `2026-08-09-quest-dsl-spec.md`) + conversor automático + harness de paridad (misma quest en legacy y Rust → mismo estado final y salida de diálogos); quests que no quepan en el DSL → Rust directo |
| 4 | Alcance infinito del monolito | Feature set acordado explícitamente (core primero, eventos diferidos) |
| 5 | Verificación frágil/manual (entorno 4GB/WSL inestable) | Verificación por script desde F0 (smoke test login→mundo→combate + captura de tráfico), no juego manual; cross-canal diferido (no verificable en 1 core) |
| 6 | Coordinación cross-canal en PostgreSQL (latencia vs caché en memoria del legacy) | Contratos explícitos + benchmark antes de portar GuildManager/LoginData |

---

## 10. Decisiones ya tomadas (para contexto de los revisores)

- ADR-0001: PostgreSQL como base principal (sin TimescaleDB por defecto) — **ratificado** por la investigación 2026.
- ADR-0002: unificar `game` + `db` en un proceso Rust por canal; db como crate interno.
- Stack Rust: tokio 1.49 + sqlx 0.9 + config-rs + clap + tracing + proptest. **Sin scripting**: quests en DSL propio declarativo (spec `2026-08-09-quest-dsl-spec.md`).
- Modelo: servidor autoritativo + BD como red de seguridad atómica.
- Estrategia: strangler por slices verticales; cliente congelado; corte en F6.
- **Auditoría legacy §2.3**: las 14 decisiones P0/P1/P2 del legacy NO se arrastran; las 7 cosas que hace bien se conservan.
- **Validado por la industria + requisito de escala (usuario)**: TCP (no QUIC/UDP hasta F7), **ECS (bevy_ecs standalone) con mundo único por servidor particionado en regiones en paralelo** (sin canales; cada entidad en una región → single-writer, anti-dupe intacto), sin CQRS/event sourcing, tick 10-20 Hz, regla «nunca await SQL inline» por región, benchmark N bots × N regiones como gate.
- **Canales regionales (§4.4)**: un solo servidor para todos — BD central con TODO el estado durable, un proceso por canal-región (EUW/LAN/LAS) corriendo el mundo, cambiar de región = elegir canal y loguear (la BD es la fuente de verdad; anti-doble-login con row locks en PG). IDs globales `uuidv7()` desde el inicio; snapshot versionado como contrato para backups/import-export. Feature de múltiples canales-región operativa en F6+, pero la frontera cross-canal en PG (ADR-0002) y los IDs nacen en F0/F3.
- **Adopción comunitaria**: documentación de protocolo/quests pública desde F0, Docker + CI + API REST + metrics (Prometheus/Grafana) como features de primera clase, licencia permisiva tipo MPL-2.0 (AGPL repele a operadores de pservers), anti-bot como diferenciador de venta, y F6 con el cliente real funcionando como argumento que ningún otro proyecto Rust tiene (no existe servidor Metin2 en Rust con tracción — hay hueco real).

## 11. Preguntas para los revisores

1. **Modelo de autoridad**: ¿algún contraargumento a «cliente envía intenciones, servidor calcula, BD garantiza»? ¿Hay hacks de Metin2 conocidos que este modelo no cubra?
2. **Stack**: ¿algo mejor que PostgreSQL 18 + sqlx para un MMO single-node en 2026? ¿Alguna feature de PG 19 (GA oct-2026) que justifique esperar?
3. **Concurrencia**: ¿mundo único particionado en regiones con ECS (bevy_ecs standalone) por región es la elección correcta para escalar (más mobs, más jugadores, sin canales)? ¿O habría que plantearse actores desde el inicio? (Nuestra postura: regiones + ECS, YAGNI en multi-proceso hasta benchmark.)
4. **Quests**: el DSL propio (spec `2026-08-09-quest-dsl-spec.md`) — ¿la gramática es elegante y completa para el corpus? ¿faltan triggers/condiciones/acciones? (decisiones abiertas en §11 del spec)
5. **Migración**: ¿el orden F0→F6 (protocolo → red → auth → db → mundo → gameplay → corte) es el correcto? ¿Faltaría un paso de validación entre fases?
6. **Alcance**: ¿es correcto diferir eventos/raids/social masivo al final (portar todo, pero en ese orden)?
7. **Auditoría §2.3**: ¿falta alguna decisión del legacy en la tabla P0/P1/P2? ¿alguna de las «7 cosas que hace bien» debería reevaluarse?
8. **Adopción**: ¿es MPL-2.0 la licencia correcta para la comunidad de pservers? ¿API web + metrics + Docker desde F5 o después del corte?
9. **Canales regionales (§4.4)**: ¿el modelo «BD central + canal-región por proceso, cambiar de región = logout→login» es el correcto para EUW/LAN/LAS con ping óptimo? ¿O se contemplaría un mundo vivo compartido tipo EVE a futuro (descartado por diseño — consistencia transcontinental no resuelta por ningún MMO)?
10. **Lo que no veo**: ¿qué estamos pasando por alto?

## 12. Próximos pasos

1. Recoger feedback de los revisores sobre este draft.
2. Escribir los ADRs pendientes: límites de dominio (char.cpp por sistemas), concurrencia (regiones + ECS), motor de quests (DSL propio), modelo anti-hack, canales regionales (BD central + proceso por región), revisión de la migración del ADR-0002.
3. Actualizar `ROADMAP.md` con las correcciones (F2 auth como modo, F3 backend trait, F0 ADRs).
4. Escribir el plan de implementación formal con tareas granularizadas (por dominio, TDD, verificación por script).
