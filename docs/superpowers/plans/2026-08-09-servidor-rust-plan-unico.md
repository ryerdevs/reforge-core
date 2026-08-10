# Plan de Reescritura del Servidor de Metin2 en Rust — Documento ÚNICO

> **Estado: DRAFT v0.2 — documento de discusión unificado.** Consolida `2026-08-09-servidor-rust-draft-discusion.md` + `2026-08-09-quest-dsl-spec.md` + diseño de movimiento.
> **Fecha:** 2026-08-09 · **Proyecto:** Reescritura del servidor de Metin2 en Rust
> **Propósito:** un solo archivo con todo el diseño para revisión de terceros. Feedback: sección «Preguntas para los revisores».

---

## Índice

1. Resumen ejecutivo
2. Contexto: el servidor legacy y por qué reescribirlo
3. Principios de la reescritura
4. Arquitectura objetivo
5. Modelo anti-hack (server-authoritative)
6. Stack tecnológico (2026)
7. Estrategia de migración (strangler fig)
8. Qué NO se porta (simplificaciones deliberadas)
9. Riesgos y mitigaciones
10. Decisiones ya tomadas
11. Spec del DSL de quests
12. Preguntas para los revisores
13. Próximos pasos

---

## 1. Resumen ejecutivo

Metin2 (MMORPG de 2004, Ymir Entertainment) tiene un servidor C++ monolítico de ~120k LOC con décadas de deuda: código espagueti, lógica validada en el cliente (fuente de la mayoría de hacks), duplicación masiva, y decisiones de 2004 que hoy son lastre.

**Propuesta:** reescribir **todo el servidor** en Rust como **rediseño estructural** (no traducción): mismos contratos observables (el cliente real sigue funcionando durante la migración), arquitectura moderna, modelo **server-authoritative** (el servidor calcula todo; el cliente solo envía intenciones), y **PostgreSQL 18** como red de seguridad transaccional.

**Lema: hacer más con menos** — menos código, menos complejidad, menos dependencias; la calidad nace de lo necesario.

Reemplazo **incremental y verificable** (strangler fig): módulo por módulo, con el servidor C++ como oráculo de tests hasta el corte final. El cliente se congela como contrato (con 1-2 paquetes aditivos de datos, ver §4.6).

---

## 2. Contexto: qué es el servidor legacy y por qué reescribirlo

### 2.1 Estado actual

- Servidor C++ (MartySama 5.9): binarios `game` (~104.7k LOC) y `db` (~12.8k LOC); `auth` es un modo del binario game (`AUTH_SERVER`), no un proceso separado.
- Cliente C++ v40999 (S3llMetin2 v24) — **intacto durante el port**; es el contrato de protocolo byte-exacto.
- Base de datos actual: MariaDB, esquema heredado, datos en CP949 (coreano de 2 bytes).
- Login completo verificado y funcionando contra el cliente real (cuenta `test`/`1234`).

### 2.2 Decisiones de Ymir que NO repetimos

| # | Decisión de Ymir (2004) | Consecuencia real | Decisión de reemplazo |
|---|---|---|---|
| 1 | Cálculo y validación en el **cliente** | Speedhack, teleport, god-mode, memory hacking | **Servidor autoritativo**: el cliente envía intenciones |
| 2 | Mutaciones económicas sin transacciones atómicas | Dupe por carreras y rollbacks | Transacciones ACID + single-writer por región |
| 3 | Queries SQL por concatenación | SQL injection | Queries parametrizadas compile-time (sqlx) |
| 4 | Cifrado casi simbólico, desactivado por defecto | Sniffing, packet forging trivial | Cifrado real con el cliente nuevo (F7) |
| 5 | God object `char.cpp` (6.6k LOC) + copy-paste | Deuda infinita, bugs por divergencia | Entity mínimo + sistemas en ECS |
| 6 | Lua 5.0 con lexer EUC-KR (2 bytes/carácter) | Encoding roto, quests frágiles | **DSL de quests propio declarativo** (sin scripting; §11) |
| 7 | Event loop `fdwatch`/`select` con bugs de backpressure | Flood de WRITE, reconexiones rotas | tokio |
| 8 | Estado duplicado entre `game` y `db` | Inconsistencias, protocolo interno que mantener | Unificación game+db (ADR-0002); PostgreSQL compartido |
| 9 | Structs cuyo tamaño cambia según flags de build | Fragilidad extrema | Crate `protocol` único con golden tests byte-exactos |
| 10 | Sin tests, sin verificación reproducible | Cada fix rompía otro | Tests de paridad + harness de captura de paquetes reales |

### 2.3 Auditoría del legacy contra estándares 2026 (evidencia en código)

Auditoría directa de `source\metin2_server\Srcs\Server` + grafo graphify (god node #1: `CHARACTER`, 815 edges) + investigación de la industria (agosto 2026). **Cada decisión del legacy que NO se copia está listada con archivo:línea.**

**Lo que el legacy hace BIEN y se conserva:** sectree como interest management, AI de mobs mínima sin pathfinding, patrón anti-hack parcial en `SyncPosition` (input_main.cpp:1758), save asíncrono fuera del world loop, catálogo QID tipado, `MoneyLog`, y el single-thread sin locks (la propiedad «single-writer» se hereda, elevada a **single-writer por región**).

| Prio | Decisión legacy (evidencia) | Estándar moderno | Ganancia |
|---|---|---|---|
| **P0** | Movimiento sin validación de distancia (`ENABLE_TP_SPEED_CHECK` comentado, input_main.cpp:1455) | Envelope de velocidad por entidad, reloj servidor | Elimina speedhack/teleport |
| **P0** | Cooldowns de skill sin chequeo server (`ENABLE_SKILL_COOLDOWN_CHECK` ausente, char_skill.cpp:107) | Cooldowns en servidor | Elimina skill-spam/attack-rate |
| **P0** | Persistencia write-behind 30 min sin atomicidad (Cache.cpp:21,101) | WAL local + batch ≤100ms + replay idempotente | Elimina dupe por rollback |
| **P0** | Config por compilación (CommonDefines.h ~80 flags) | Config runtime + feature flags | Fin del re-build cruzado |
| **P0** | Observabilidad `sys_log`+`fflush` por llamada (log.c:171,218) | tracing estructurado + métricas | Debug de producción en minutos |
| **P1** | `select()` O(fds) (fdwatch.c:400) | tokio/epoll | Escalabilidad; elimina flood WRITE |
| **P1** | Tick global con allocs, O(todas las entidades) (char_manager.cpp:641) | Regiones + ECS en paralelo | Techo 1.000+ jugadores/instancia |
| **P1** | Broadcast serializado por receptor (entity_view.cpp:36-239) — 40k serializaciones/seg | Serializar una vez + referencias | ~40x menos serialización |
| **P1** | God object `CHARACTER` (815 edges) | Entity mínimo + sistemas | Testeable y extensible |
| **P1** | Código muerto (tea.s 121KB, liblua/5.2) | No portar nada sin llamadores | Menos superficie |
| **P2** | Build de deploy con AddressSanitizer activo (Makefile) | Release limpio; sanitizers en CI | 2-3x CPU recuperados |
| **P2** | Copy-paste de contenido (11+ `collect_quest_lvXX`) | Familias parametrizadas en el DSL | Contenido sin duplicar |
| **P2** | Sistemas paralelos (BlueDragon vs DragonLair; shop vs shopEx) | Un sistema por concepto, datos config | Menos divergencia |
| **P2** | Cero tests en todo el repo | Golden tests + harness de paridad | Condición de avance |

**Ganancia estimada:** 2-5x CPU disponible en el tick (misma implementación mejor) → techo 300-500 → **1.000+ jugadores/instancia** con el modelo nuevo.

**Validación por la industria (2026):** TCP es correcto (WoW, FFXIV, EVE, GW2, ESO usan TCP; Veloren abandonó UDP; Ember usa Boost.Asio). **ECS SÍ** para esta visión (Veloren: millones de entidades con ECS + regiones; bevy_ecs standalone). Sin CQRS/event sourcing/outbox (sobre-ingeniería en proceso único; solo el log económico append-only). Tick de mundo 10-20 Hz (TrinityCore: 20 Hz). Referencia de techo: EVE (2.670-6.557 CCU por sistema con nodos).

---

## 3. Principios de la reescritura

1. **Hacer más con menos**: YAGNI, stdlib antes que dependencias, una línea antes que cincuenta.
2. **Rediseño estructural, no traducción**: límites de dominio, propiedad de datos, protocolos, concurrencia y fallos se deciden por escrito (ADRs) antes de implementar.
3. **Servidor autoritativo**: el cliente es una vista, nunca una fuente de verdad.
4. **La BD no calcula, garantiza**: la lógica vive en Rust; PostgreSQL impone integridad.
5. **Reemplazo incremental verificable**: cada módulo preserva el comportamiento observable y pasa verificación antes de avanzar.
6. **Cliente congelado como contrato** (F0–F6), con 1-2 paquetes aditivos de datos (§4.6).
7. **Paridad solo donde importa**: el comportamiento observable se conserva; el código interno no tiene por qué parecerse.
8. **Un proceso por región**: `game` + `db` unificados (ADR-0002); `auth` como modo del mismo binario.

---

## 4. Arquitectura objetivo

### 4.1 Visión general

```
Cliente C++ (congelado + 2 paquetes aditivos)   Cliente Rust futuro (F7)
        │                                                │
        ▼                                                ▼
┌──────────────────────────────────────────────────────────────────┐
│            Servidor Rust — mundo por región (proceso)            │
│  ┌──────────┐                                                    │
│  │   net    │      ┌───────────┐   ┌───────────┐                 │
│  │ (tokio)  │─▶    │ región 1  │   │ región N  │  …              │
│  └──────────┘      │ (task +   │   │ (task +   │                 │
│                    │  ECS)     │◀─▶│  ECS)     │  mpsc eventos   │
│  ┌──────────┐      │ mobs,     │   │ mobs,     │  inter-región   │
│  │  auth    │      │ jugadores │   │ jugadores │                 │
│  │  (modo)  │      └─────┬─────┘   └─────┬─────┘                 │
│  └──────────┘            │               │                       │
│  ┌───────────────────────▼───────────────▼───────────────────┐  │
│  │  db crate (sqlx, async): queries → resultados por mpsc    │  │
│  │  (las regiones NUNCA await SQL inline)                    │  │
│  └───────────────────────┬───────────────────────────────────┘  │
└──────────────────────────┼──────────────────────────────────────┘
                           ▼
               ┌───────────────────────┐
               │   PostgreSQL 18.4     │  ← central (una para todas las regiones)
               │  (estado + integridad)│
               └───────────────────────┘
```

Nota: cada entidad (y su inventario/oro) pertenece a UNA región en cada momento — single-writer por región, anti-dupe intacto.

### 4.2 Modelo de concurrencia

- **tokio, task-per-connection** para la red. **Mundo por región**: una región = agrupación de sectrees del mapa, con su task de simulación. Un servidor = colección de regiones en paralelo.
- **Una task tokio por región, con ECS dentro (bevy_ecs standalone)**: queries paralelizables (par_iter/rayon), cero locks compartidos. Cada entidad (y su inventario/oro) pertenece a UNA región → **single-writer por región, anti-dupe intacto**; migración de entidad entre regiones con handshake.
- **Regla de oro: el task de región NUNCA hace `.await` de SQL inline.** Patrón ReturnQuery del legacy: emite la query, el crate db la ejecuta en el runtime multihilo, el resultado vuelve por mpsc. Un query lento jamás detiene el mundo.
- Comunicación inter-región por `mpsc` de eventos (chat global, gremios, migraciones). Broadcast con política ante saturación (drop-to-newest para posición, encolar para eventos).
- Timers con binary heap (patrón `event_queue` legacy).
- **Escalado**: más jugadores/mobs → región más fina o más regiones, sin multiplicar procesos. Multi-proceso futuro (por grupos de regiones) no se construye hasta que el benchmark lo pida (gate F5: N bots × N regiones).

### 4.3 Descomposición en dominios

El monolito se porta **por sistemas sobre un núcleo Entity mínimo** (VID, posición, sectree, estado de vida) **en un ECS (bevy_ecs standalone)** — nunca como clase única. Es la decisión de arquitectura más importante del proyecto.

| Dominio | Archivos legacy | Dificultad | Notas |
|---|---|---|---|
| Protocolo/wire | `packet.h`, `packet_info.cpp`, `tables.h` | 1 | Contrato byte-exacto; ya specced |
| Net/transporte | `libthecore` | 1-2 | Reimplementar semántica, no portar C |
| Auth/login | `input_auth`, `input_login`, path auth de `db.cpp` | 1 | Modo AUTH_SERVER; primer slice vertical |
| Capa de datos | `db/src/*` (~12.8k) | 2 | Crate interno; por dominios (§4.5) |
| Mundo/espacio | `sectree_manager`, `char_manager`, `dungeon`, `building` | 3 | Mundo entero en RAM |
| Entidades | `char.cpp` (6.6k), `char.h`, `char_state` | 3 | **Portar por sistemas, no como god object** |
| Movimiento | Sync en `char.cpp`, `entity_view` | 2-3 | Anti-speedhack (§4.7) |
| Items/inventario | `char_item` (6.6k), `item`, `refine`, `blend`, `cube`, `safebox`, `shop` | 3 | Máximo riesgo dupe; transacciones |
| Combate | `char_battle`, `battle`, `pvp`, `skill_power` | 3 | Daño 100% servidor |
| Skills | `char_skill`, `skill`, buffs | 3 | Cooldowns en servidor |
| NPC AI + spawn | `mob_manager`, `regen`, FSM | 2 | FSM trivial; sin A* |
| Quests | `questmanager`, `questlua_*` (~10k bindings) | 3 | DSL propio (§11) |
| Social | `party`, `guild`, `guild_war`, `messenger`, `marriage` | 2-3 | SQL-heavy; cross-región |
| Economía | `exchange`, `safebox`, `shop`, `fishing`, `mining` | 2-3 | Anti-dupe crítico |
| Admin/GM | `cmd_gm` (3.8k), `cmd_general`, `gm.cpp` | 2 | Permisos re-chequeados en BD |
| Config/locale/encoding | `config`, `locale_service` | 2 | Trampas de encoding conocidas |
| Eventos/raids | `BlueDragon`, `DragonLair`, `OXEvent`, `wedding`, `arena`, `war_map` | 2-3 | Portar todo, diferido (§8) |

### 4.4 Canales regionales (EUW/LAN/LAS) — el modelo Metin2 distribuido

**Requisito del usuario:** un solo servidor para todos — una cuenta, un personaje con absolutamente todo (inventario, oro, quests, gremio) en cualquier región. Servidores-canal por región por el ping; cambiar de región = elegir servidor y loguear, sin transferencias.

**Modelo:** el sistema de canales de Metin2 (los canales comparten la BD → el personaje está en todos), distribuido geográficamente. A los jugadores se les llama «Servidor 1: Europa», «Servidor 2: EUW», «Servidor 3: América» — el término interno es canal/región.

- **Una BD central (PostgreSQL)** con TODO el estado durable: cuentas, personajes, inventario, oro, quests, gremios.
- **Un proceso por canal-región** (EUW, LAN, LAS…): cada uno corre su copia del mundo en memoria (mobs, mercado vivo, PvP son por canal — comportamiento Metin2).
- **Login en cualquier región**: eliges servidor y logueas; el personaje carga desde la BD central con todo. La BD es la fuente de verdad.
- **Anti-doble-login cross-región**: row lock/advisory lock sobre la fila del personaje al entrar; al salir se libera. Nunca dos regiones con el mismo personaje (eso sería el clon/dupe).
- **Durable = write-through** (§4.5): la BD siempre está al día → cambiar de región no pierde NADA. Posición/HP (volátil) son locales.
- **Coordinación cross-canal en PG**: guilds y cachés vía LISTEN/NOTIFY.
- **Comercio unificado**: el mercado (subasta) vive en la BD (`economy`) — todos los canales comparten la misma subasta con advisory locks; cierre de pujas con el reloj de la BD (sin trampas de reloj por región).

**Latencia — matiz honesto:** la BD central no está en el hot path; el ping del jugador depende solo de su canal regional. El lag de persistencia (100-200ms LAS↔EUW) es irrelevante para el gameplay. Punto único de fallo (BD central) → mitigado con failover (§4.5).

**Regiones cross-server (permanentes y temporales):** mapas que no pertenecen a ningún servidor-región, accesibles desde todos — p.ej. una isla de guerra o un continente PvP permanente donde se ven y pelean jugadores de todas las regiones. Técnicamente es una **región especial** con su propio proceso (típicamente junto a la BD central), a la que los personajes migran temporalmente (mismo mecanismo de migración, single-writer intacto) y de la que vuelven a su servidor al salir. Ping ~150-200ms para los lejanos (jugable — el legacy ya se juega así desde LAT contra servidores EU). Es un caso más de la frontera de regiones, no un sistema nuevo.

**Lo que NO es:** un mundo vivo único tipo EVE (mismo instante para todos los continentes, migración de estado en caliente entre nodos) — consistencia transcontinental no resuelta por ningún MMO, y la física limita: la luz tarda ~150ms en cruzar el Atlántico; alguien siempre come el ping. Los canales regionales con BD compartida dan el 95% del sueño con el 100% de la física cumplida.

### 4.5 Capa de datos — diseño final

**Base:** PostgreSQL 18 central + sqlx 0.9 (ADR-0001). El crate `db` se organiza **por dominios**, cada uno con esquema propio, migraciones versionadas y repositorios:

```
db crate
├── account/  → esquema account (cuentas, login, bans)
├── player/   → esquema world   (personajes, items, quests)
├── social/   → esquema social  (guilds, parties, messenger)
├── economy/  → esquema economy (auction, money log, trade history)
└── log/      → esquema log    (auditoría append-only)
```

Permisos PG por esquema (log no escribe en economy — defensa en profundidad). Contrato: **mundo en memoria = autoridad viva (cero SQL en hot path); BD = persistencia** — writes durables en batch transaccional, reads solo en boot/cambio de región. Todo el estado duradero (items, oro, quests, personajes, gremios) es persistente por requisito.

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

- **Cada mutación durable lleva `mutation_id` (uuid)**: aplica-una-sola-vez garantizado (replay idempotente) — la columna vertebral que une WAL, PG y auditoría.
- **Sin ventana de dupe**: crash = pérdida máxima del batch en vuelo (~100ms), nunca items confirmados; el replay restaura el resto.
- **RLS (row-level security) en world/economy**: cada query lleva `current_setting('app.pid')`; PG rechaza filas de otro jugador — red de seguridad contra bugs.
- **Failover**: hot standby replicando, promoción automática ~2 min; las regiones siguen el mundo en memoria con writes encolados.
- **Log de auditoría particionado por fecha + retención + `pg_stat_statements`** desde el día uno.
- **Migración**: backend MaríaDB mínimo solo lo que el game C++ necesita (F2–F3); migración a PG en F3 (los datos ya existen), no en F6 — la dualidad de esquemas muere antes. Sin backend peer-legacy.
- **Contrato fijado en ADR**: durable = WAL local + batch ≤100ms; volátil = save cada 30s + logout; failover ≤2 min.

**Por qué PostgreSQL y no redb/SQLite/SurrealDB/etc.:** redb es una librería **embebida** (archivo local de UN proceso) — no sirve para N regiones compartiendo la misma BD, rompe los canales regionales. SQLite idem (single-writer). SurrealDB es documental (el modelo de personajes/items/gremios es relacional puro), inmaduro, y sin ecosistema de ops. CockroachDB: licencia propietaria + multi-nodo que no existe aquí. TiDB/ScyllaDB: idem + Scylla ni es relacional. libSQL/Turso: en pausa. **PG es la única que cumple el contrato completo: ACID multi-fila, constraints, RLS, LISTEN/NOTIFY, failover, 25 años de battle-testing.**

### 4.6 Datos servidor→cliente (manifest versionado + delta download)

**Problema legacy:** el cliente es una fuente de datos (nombres/descripciones de items y NPCs en su pack `.epk`, textos de quest en locale.lua por idioma) separada del servidor — cambiar un nombre = editar BD + repaquear cliente + patcher.

**Solución (cliente C++ con modificaciones aditivas — solo 1-2 paquetes nuevos, sin tocar render/gameplay):** el servidor es la única fuente de datos; el cliente solo renderiza.

- **Items/NPCs**: el servidor envía los datos (nombres, descripciones) al login vía **manifest versionado + delta**: el cliente pide «¿versión 42?», el servidor responde «hay 43» y envía SOLO lo que cambió (KB). El pack queda solo para visuales (iconos, modelos). Adiós a la trampa CP949, al repaqueo y al patcher por boludeces.
- **Textos de quest**: viven en la BD por locale (`account.lang` decide el idioma); el servidor los envía localizados — mata las 181 claves faltantes y la mezcla ES/EN. Cero archivos por idioma.
- **Patcher** queda solo para binario del cliente y visuales; los datos viajan por delta.
- El manifest se genera de la BD (única fuente de verdad) — sin copias manuales.
- El archivo que descarga el cliente es **ultra ligero** (delta, KB); la base se descarga una vez.

**Hot reload (recarga en caliente, sin reiniciar nada):**

```
BD editada → LISTEN/NOTIFY de PG → servidor recarga la tabla/quest en memoria
          → bump de versión del manifest → cliente pide delta → aplica
```

- **Sí recarga en caliente:** textos/idiomas, items/equipo/accesorios (stats, nombres, descripciones), quests del DSL (la próxima instancia usa la versión nueva), rates/config vía manifest.
- **No recarga:** el estado del mundo en curso (una quest a medias conserva su instancia hasta completarse; un item en inventario conserva su instancia). Solo se recargan los datos que definen el comportamiento futuro.
- El cliente viejo aplica el delta al login (seguro); el cliente recompilado puede refrescar textos en vivo.
- Es el patrón `PROTO_FROM_DB` del legacy completado: el legacy cargaba de la BD al boot; nosotros en runtime con NOTIFY + manifest.

### 4.7 Movimiento y anti-speedhack — el diseño

**El problema legacy:** `ENABLE_TP_SPEED_CHECK` estaba comentado — el servidor aceptaba posiciones del cliente sin validar. Speedhack y teleport triviales.

**El diseño — el servidor es el dueño de la posición:**

```
cliente envía: "me muevo hacia (x,y), modo correr"
     │
     ▼
región valida (todo con reloj del servidor):
  1. ¿Está vivo y no aturdido/paralizado?       → no → ignora
  2. ¿Cooldown de movimiento respetado?         → no → ignora
  3. ¿Distancia máx. según modo? (andar/correr/
     montura = envelopes distintos)             → no → corrige
  4. ¿El destino es caminable? (datos del mapa
     del SERVIDOR, no del cliente)              → no → corrige
  5. ¿Ruta en línea recta sin atravesar muros?  → no → corrige
     │
     ▼
  OK → posición autoritativa → re-broadcast a visores
       (el cliente recibe su posición corregida si se desvió)
```

**Reglas clave:**
- **Envelope por entidad**: distancia máxima = velocidad(entidad) × tiempo desde el último movimiento aceptado. Reloj del servidor — el cliente no "gana tiempo" editando su reloj local.
- **Corrección, no ban**: si se excede, se descarta y se envía la posición real (mini-teleport atrás). Ban automático solo tras N violaciones en T segundos (patrón legacy `SyncPosition` + HackLog, completado).
- **Interpolación client-side**: el resto de entidades se ven suaves entre snapshots (10-20Hz).
- **Lag tolerance** (para que 300ms no rompan la validación): margen explícito (+20% distancia o +100ms) — justo con ping alto.
- **Los mobs/NPCs se mueven 100% en servidor**: el cliente nunca "mueve" a nadie que no sea su propio personaje.
- **Latencia (ya en Metin2)**: el combate tick-based tolera 150-300ms; se optimiza con predicción client-side (animación inmediata, valor del servidor), interpolación, tick 10-20Hz, lag compensation estilo WoW (validar el golpe contra la posición que el atacante veía). Todo lo percibido se optimiza para que 300ms se sientan como 20; todo lo validado usa el reloj del servidor.

### 4.8 Modificaciones al cliente que liberan al serverside (F0–F6)

Auditoría de `source\metin2_client\Srcs\Client\` (evidencia archivo:línea). El cliente se recompila (ya probado 3 veces con éxito); la regla de oro para F0–F6: **cambios solo en (1) tabla de headers + cases de fase, (2) overrides de datos en memoria, (3) python del pack. NADA de render, formatos de pack ni structs existentes.**

**El contrato inamovible (el server Rust se construye dentro de estas paredes):** headers/structs existentes (LOGIN3 65/68B, TSimplePlayer con flags de build), handshake, máquina de fases, límites (24 chars nombre, 5 personajes/cuenta, stack 200, inventario). Cambiarlos = proyecto de protocolo post-F6.

**CRÍTICO — los paquetes aditivos deben ser PULL-based:** el cliente viejo, ante un header desconocido, descarta el buffer de recepción completo (`PythonNetworkStream.cpp:571-578, 654-662`) → un servidor que mande push a un cliente sin recompilar desincroniza la sesión. Pull = el cliente pregunta (solo el recompilado pregunta) y el servidor responde. El cliente viejo nunca se desincroniza.

**Headers libres verificados en ambos lados:** 139-149 (11), 154-160 (7), **162-207 (46, recomendado)**, 211-214 + 216-255 (44). Mecánica de registro: `Packet.h` + `Set(HEADER_GC_X, ...)` en `PythonNetworkStream.cpp:60-184` + `case` en la fase.

| # | Modificación cliente | Qué desbloquea | Effort | Riesgo | Cuándo |
|---|---|---|---|---|---|
| 1 | **Paquetes aditivos pull-based** (CG_QUERY → GC_RESPONSE, headers 162+) | El canal §4.6 completo: manifest versionado + delta de items/NPCs/textos | Bajo | Bajo | **F3-F4** |
| 2 | **Overrides en memoria**: `SetLocaleName`/`SetItemLocaleName` tras `LoadLocaleData` (`PythonApplication.cpp:867-911`) | El server Rust manda nombres UTF-8 desde la BD; mata el mojibake; el pack deja de ser fuente de verdad de textos | Bajo | Bajo | **F4-F5** |
| 3 | **Lista de canales desde el auth** (override de `serverinfo.py`) | IP/puertos de canales fuera del pack (queda solo la IP del auth); reconfiguración en runtime sin repack | Medio | Medio-bajo | **F5-F6** |
| 4 | **(Sin tocar cliente) validar `dwLoginKey` (LOGIN_BY_KEY) en el canal** | El password deja de reenviarse en claro en reconexiones (fix #14 sin password); base para sesiones tokenizadas | Bajo | Bajo | **F2-F4** |
| 5 | Post-F6: caché en disco del manifest, eliminar `serverinfo.py`, revisar límites con cliente nuevo | — | — | — | **F7** |

**Otros hallazgos:** el render de texto del cliente es **UTF-8 nativo** (`GrpTextInstance.cpp:124` `CP_UTF8`) → el server puede mandar UTF-8 directo sin conversión. Los cooldowns de skill el cliente solo los muestra (no valida) → el server valida. Riesgo alto: parseo de tablas (`PythonSkill.cpp` — el crash 0xC0000374), render DX9, formatos de pack (TEA/LZO/MMPT0 — nunca cambiar el formato, solo el contenido). El cliente ya tiene fallback de NPCs al nombre del servidor (§2.2 del AGENTS).

### 4.9 Libertades del serverside gracias al cliente modificable

Regla: **toca el cliente solo si (a) ≤1 semana de trabajo y (b) desbloquea algo en el serverside que no puede conseguir solo.** Nada está prohibido; todo es coste/beneficio. Con eso, el serverside gana:

| Cliente (cambio) | Libertad en el servidor Rust | Cuándo |
|---|---|---|
| Paquetes aditivos pull-based (headers 162+) | Canal de datos dinámicos: items/NPCs/textos nuevos **sin tocar el pack** — el servidor es la única fuente de contenido | F3-F4 |
| Overrides de nombres UTF-8 | El servidor controla **todo texto visible** (idiomas, correcciones) desde la BD; adiós mojibake | F4-F5 |
| Lista de canales desde el auth | Canales/servidores **configurables en runtime** (IP, puertos, abrir/cerrar sin repack); adiós a la IP horneada | F5-F6 |
| Version check al conectar | El servidor **gatea la evolución del protocolo**: rechaza versiones viejas con mensaje claro; puede añadir paquetes nuevos sin romper a quien sí recompila | F2 |
| Hardware ID en LOGIN3 | **Bans por hardware, anti-multibox** — sin kernel driver | F2 |
| Server time al login | Timers/eventos/cooldowns **consistentes con el reloj del servidor**; mata trampas de reloj local | F2 |
| Config vía manifest (rates, límites visibles) | Ajustar el juego **sin recompilar ni repaquear** | F5 |
| `dwLoginKey` (LOGIN_BY_KEY, sin tocar cliente) | **Sesiones tokenizadas**: el password no se reenvía en claro en reconexiones | F2-F4 |

**Preparación para F7 (no refactor de C++ — artefactos reutilizables):** spec de las paredes del cliente (límites/structs/headers), build del cliente reproducible (script/CI), cliente como visor fino (datos del servidor), diseño UI en Slint standalone (los `.slint` sobreviven). Refactorizar módulos C++ "bien hechos" = trabajo tirado, no hacer.
**Principio rector: el cliente envía intenciones, nunca hechos.** Todo dato que el cliente podría haber editado en memoria es no-confiable; el servidor recomputa desde su propio estado.

| Hack | Contramedida |
|---|---|
| Speedhack / teleport | Envelope de velocidad por entidad + walkability del mapa (§4.7) |
| God-mode / one-shot / attack-speed | Daño íntegro en servidor; cooldowns por reloj del servidor; rango y LoS vía sectree |
| Dupe (la clase reina) | (1) single-writer por región; (2) transacciones atómicas + WAL local + mutation_id (§4.5); (3) uuidv7; (4) política de save explícita |
| Memory hacking del cliente | El cliente es solo una vista; nada de lo que muestra se lee del cliente |
| Paquetes falsos / commands GM | Máquina de estados de fase estricta; permisos re-chequeados en BD; rate limits |
| SQL injection | Queries parametrizadas compile-time (sqlx) |
| Packet floods / spam | Rate limiting por conexión: paq/seg, bytes/seg, por acción (chat, trade, skill, loot) — el legacy NO lo tiene |
| Bots de farm | Telemetría de comportamiento server-side (rutas/ritmos de farm + flags para revisión humana) — diferenciador de venta |

**Persistencia en dos clases explícitas:**
- **Durable** (items, oro, quest flags, guilds, safebox): pipeline WAL → PG (§4.5). Un crash NUNCA pierde ni duplica items.
- **Volátil** (posición, HP, cooldowns): save cada 30s + logout. Perder segundos de posición es aceptable; perder items no.

---

## 6. Stack tecnológico (investigado contra fuentes primarias, agosto 2026)

| Capa | Elección | Justificación |
|---|---|---|
| Lenguaje | **Rust** (edition 2024) | Seguridad de memoria, cero-cost abstractions |
| Runtime async | **tokio 1.49** | Estándar; tareas, mpsc, timers |
| ECS | **bevy_ecs standalone** | Queries paralelizables sin arrastrar el motor gráfico |
| Base de datos | **PostgreSQL 18.4** | ACID, uuidv7, OLD/NEW en RETURNING, LISTEN/NOTIFY, advisory locks, RLS, backups incrementales, failover |
| Acceso BD | **sqlx 0.9** | Queries compile-time checked, migraciones, pool propio |
| Quests | **DSL propio declarativo** (§11) | Cero runtime de scripting |
| Config | config-rs + clap 4.6 | — |
| Observabilidad | tracing + métricas (Prometheus/Grafana) | — |
| Tests | cargo test + proptest + golden tests + harness de paridad | — |

**Descartadas con justificación:** CockroachDB (licencia propietaria), TiDB/ScyllaDB (multi-nodo innecesario; Scylla no relacional), SurrealDB (documental + inmaduro), libSQL/Turso (en pausa), SQLite/redb (embebidas, rompen la BD compartida entre regiones), TimescaleDB (ADR-0001: solo si los logs lo demuestran).

**Nota sobre «que la BD calcule todo»:** descartada. La lógica en triggers/procedures SQL es anti-patrón (intestable, latencia, cuello único) y no elimina hacks: el dupe es una carrera de guardado, no de cálculo. La BD **garantiza**; el servidor **calcula**.

---

## 7. Estrategia de migración (strangler fig)

### 7.1 Forma general

Slices verticales (cliente→auth→db→cliente) con el cliente congelado. El db legacy permanece como oráculo hasta el corte. El crate `db` se construye **tras un trait de backend**: `direct-sql` (MariaDB, mínimo) → `postgres` (destino, en F3). El shim legacy del ADR-0002 se degrada a artefacto de test (golden tests), no de deploy.

### 7.2 Fases

| Fase | Objetivo | Hito verificable |
|---|---|---|
| **F0** Fundaciones | Workspace Cargo, ADRs, crate `protocol` byte-exacto (login flow), harness de captura de paquetes | Un LOGIN3 real capturado se parsea y re-serializa idéntico |
| **F1** Red/transporte | Listener tokio con la semántica verificada (`result > 0`/EAGAIN), framing, handshake con retries | El auth C++ se conecta a un peer Rust y viceversa sin floods |
| **F2** Auth + cliente (semana 1) | Slice auth (modo AUTH_SERVER): LOGIN3, hash `"*"+UPPER(SHA1(UNHEX(SHA1(pw))))`; **cliente**: version check, hardware ID, server time, `dwLoginKey` | Login contra auth Rust + db C++; el cliente recompilado pasa el version check |
| **F3** Capa de datos + canal de datos | Crate `db` por dominios tras trait de backend; portar por QID; migración a PG; **cliente**: paquetes pull-based 162+ (CG_QUERY/GC_RESPONSE) | El game C++ corre contra db Rust sin cambios; el cliente recibe datos aditivos sin desincronizarse |
| **F4** Entrada al mundo + nombres | CG_PLAYER_SELECT, spawn, mapa, stats; **cliente**: overrides de nombres UTF-8 | El cliente real entra al mundo contra el core Rust con nombres correctos |
| **F5** Gameplay | Movimiento, combate, drops, items, NPCs, quests, chat, shops, trade, GM — por dominios, side-by-side; **cliente**: lista de canales desde auth, config vía manifest; **Slint standalone** arranca en paralelo | Sesión completa sin divergencias + benchmark de escala (N bots × N regiones) |
| **F6** Paridad total | Side-by-side automatizado (misma entrada → diff), corte instancia por instancia | El servidor Rust reemplaza al C++ sin cambios en el cliente |
| **F7** Cliente (después) | Cliente Rust (wgpu), UI Slint (los `.slint` del F5 se integran), protocolo nuevo, cifrado real | — |

### 7.3 Feature set

**Portar todo** (los eventos NO se descartan). **Orden:** core jugable primero (movimiento, combate, skills, items, drops, NPCs, quests, chat, tiendas, safebox, trade, GM), **eventos/raids/social masivo diferidos** (OXEvent, bodas, BlueDragon, DragonLair, 3 imperios, arena, guerra de gremios) a portar después — YAGNI: sin este recorte de orden, F5 es infinito.

---

## 8. Qué NO se porta (simplificaciones deliberadas)

- **Código repetido**: eventos raid casi idénticos (BlueDragon, DragonLair, xmas…) → **un solo framework de encounter configurable por datos**.
- **Cifrado legacy**: plaintext se hereda SOLO mientras el cliente C++ viva (contrato). El servidor interno y el cliente futuro (F7) usan cifrado real.
- **Encoding CP949**: se revierte a UTF-8 para contenido quest/locale. Las tablas de datos del boot (nombres CP949 referenciados por `etc_drop_item.txt`) se conservan byte-compatibles o se migran atómicamente con `item_proto`.
- **Lua y todo scripting**: eliminados. Quests = DSL declarativo propio (§11). El servidor Rust no tiene ningún runtime de scripting.
- **Duplicación de estado game↔db**: eliminada por diseño (ADR-0002).
- **Orden de arranque**: desaparece (solo PostgreSQL primero).

---

## 9. Riesgos y mitigaciones

| # | Riesgo | Mitigación |
|---|---|---|
| 1 | Paridad byte-exacta del protocolo (flags de build cambian tamaños) | Golden tests con capturas tcpdump reales en cada fase; crate protocol única fuente; side-by-side F6 automatizado |
| 2 | Traducción del god object `char.cpp` | ADR de límites de dominio ANTES de F4; portar por sistemas sobre Entity mínimo; validación side-by-side |
| 3 | Conversión del corpus de quests (194 `.quest`) | DSL propio + conversor automático + harness de paridad; quests que no quepan → Rust directo |
| 4 | Alcance infinito del monolito | Feature set acordado (core primero, eventos diferidos) |
| 5 | Verificación frágil/manual (entorno 4GB/WSL inestable) | Verificación por script desde F0 (smoke test login→mundo→combate); cross-región diferido |
| 6 | Coordinación cross-canal en PG (latencia vs caché) | Contratos explícitos + benchmark antes de portar GuildManager/LoginData |

---

## 10. Decisiones ya tomadas (para contexto de los revisores)

- ADR-0001: PostgreSQL como base principal — **ratificado** por la investigación 2026.
- ADR-0002: unificar `game` + `db` en un proceso Rust; db como crate interno.
- Stack Rust: tokio 1.49 + sqlx 0.9 + bevy_ecs + config-rs + clap + tracing + proptest. Sin scripting (DSL propio).
- Modelo: servidor autoritativo + BD como red de seguridad atómica.
- Estrategia: strangler por slices verticales; cliente congelado (+2 paquetes aditivos); corte en F6.
- Auditoría §2.3: 14 decisiones P0/P1/P2 del legacy NO se arrastran; las 7 cosas buenas se conservan.
- Escala: mundo por región con ECS (single-writer por región, anti-dupe intacto); TCP; tick 10-20Hz; sin CQRS/event sourcing.
- Canales regionales (§4.4): BD central + proceso por región; cambiar de región = logout→login; comercio unificado; regiones cross-server permanentes y temporales. IDs uuidv7 globales.
- Capa de datos (§4.5): módulos por dominio, WAL local + mutation_id, RLS, failover, log particionado.
- Datos servidor→cliente (§4.6): manifest versionado + delta; patcher solo para binario/visuales.
- Adopción comunitaria: documentación de protocolo/quests pública desde F0, Docker + CI + API REST + metrics como features de primera clase, licencia MPL-2.0 (AGPL repele a operadores de pservers), anti-bot como diferenciador, y F6 con el cliente real funcionando como argumento único (no existe servidor Metin2 en Rust con tracción).

---

## 11. Spec del DSL de quests

### 11.1 Contexto y objetivos

El legacy ejecuta las quests en Lua 5.0 (lexer EUC-KR) compiladas desde un DSL propio de Metin2 (`qc`). Contenido real: 194 archivos `.quest` (~2.500+ líneas duplicadas solo en la familia `collect_quest_lv30..lv96`).

**Decisión:** eliminar Lua por completo. DSL declarativo propio, tipado y verificado por un parser Rust, con composición (familias + bloques + imports).

**Objetivos:** (1) legible y elegante para diseñadores (no programadores); (2) corto — una quest larga se compone, no se repite; (3) cero lógica arbitraria — solo acciones tipadas conocidas; (4) validación en load-time con errores archivo:línea:columna; (5) migración automática verificable; (6) el mismo parser sirve a runtime, validador CLI y editor.

**Lo que NO es:** lenguaje de scripting. Los casos que lo requieran (gestores de eventos como `oxevent`) se escriben en Rust como módulos del servidor.

### 11.2 Sintaxis básica

- Extensión `.quest`. **Indentación significativa** (2 espacios). Sin `begin/end`, sin comas. `#` comentarios.
- Una quest = `quest`, dentro estados `state`, dentro eventos `on`, dentro acciones `->`.

```quest
# quests/biology/collect_quest_lv30.quest
quest collect_quest_lv30
  state start
    on login, levelup with pc.level >= 30
      -> set_state(information)

  state information
    on letter
      -> send_letter(gameforge.collect_quest_lv30._10_sendLetter)

    on button, info
      -> say_title(gameforge.collect_quest_lv30._10_sendLetter)
      -> say(gameforge.collect_quest_lv30._20_say)

    on 20084.chat
      -> say_title(gameforge.collect_herb_lv10._50_sayTitle)
      -> say(gameforge.collect_quest_lv30._40_say)
      -> wait()
      -> set_qf(duration, 0)
      -> set_qf(collect_count, 0)
      -> set_state(go_to_disciple)

    on 601.kill with number(1, 100) <= 5
      -> give_item2(30006, 1)
```

**Reglas de la gramática:**

| Construcción | Sintaxis | Notas |
|---|---|---|
| Quest | `quest <nombre>` | Único por archivo (opcional: `import` arriba) |
| Estado | `state <nombre>` | `start` obligatorio; `__complete`, `__giveup__` convención |
| Evento | `on <trigger>[ , <trigger>...]` | Multi-trigger con coma; `or` implícito |
| Condición | `with <expresión>` | Opcional; expresión tipada |
| Acción | `-> <acción>(<args>)` | Una por línea; `()` opcional sin args |
| Bloque | `block <nombre>[ (<param>: <tipo>)]` | Reutilizable |
| Uso de bloque | `use <nombre>[ (<args>)]` | Dentro de un evento |
| Import | `import <archivo>` | Relativo a `quests/` |
| Familia | `quest <nombre> = <base>(<param>: <valor>)` | Instancia parametrizada |
| Comentario | `# ...` | Línea completa |

**Regla de validación:** toda acción, trigger y condición es **conocida por el parser** (catálogo tipado). Nombre desconocido = error de carga con archivo:línea. No hay escape hacia código libre.

### 11.3 Triggers — inventario del corpus real

| Trigger | Sintaxis | Semántica |
|---|---|---|
| Login | `login` | Entra al mundo |
| LevelUp | `levelup` | Sube de nivel |
| Carta | `letter` | Abre el diario de quests |
| Botón/Info | `button`, `info` | Pulsa botón/info de la quest |
| Chat NPC | `<vnum>.chat` | Habla con el NPC |
| Matar | `<vnum>.kill` | Mata al mob |
| Usar item | `<vnum>.use` | Usa el item |
| Click target | `__TARGET__.target.click` | Click en objetivo marcado |
| Entrar | `enter` | Entra en un mapa |
| Salir | `logout` | Cierra sesión |
| Temporizador | `timer` (a definir con `set_qf` cooldown) | Legacy: patrón `get_time()` |

**Triggers especiales (diferidos o Rust):** `arena.*`, `oxevent.*`, `d.*` (dungeon), `wedding.*` — auditados en la conversión; los que no quepan → módulos Rust.

### 11.4 Condiciones (expresiones tipadas)

Mini-lenguaje parseado y tipado: comparaciones (`==`, `!=`, `<`, `>`, `<=`, `>=`), aritmética, `and`/`or`/`not`, paréntesis, literales.

| Función | Sintaxis | Legacy |
|---|---|---|
| Nivel | `pc.level >= 30` | `pc.level` |
| Contar item | `count_item(30006) > 0` | `pc.count_item(vnum)` |
| Flag de quest | `get_qf(duration) != 0` | `pc.getqf("duration")` |
| Probabilidad | `number(1, 100) <= 5` | `number(min, max)` |
| Tiempo | `get_time() >= get_qf(duration)` | `get_time()` |
| Mapa | `get_map_index() == 113` | `pc.get_map_index()` |
| GM | `get_gm_level() == 5` | `pc.get_gm_level()` |
| Mascota | `pet.is_summon(34003)` | `pet.is_summon(vnum)` |
| Servidor test | `is_test_server()` | — |
| Rango de nivel | `pc.level between 15, 39` | — (nueva) |

### 11.5 Acciones — inventario del corpus real

| Acción | Sintaxis | Legacy |
|---|---|---|
| Diálogo título | `say_title(clave)` | `say_title(...)` |
| Diálogo | `say(clave)` | `say(...)` |
| Recompensa mostrada | `say_reward(clave)` | `say_reward(...)` |
| Item mostrado | `say_item_vnum(30006)` | `say_item_vnum(vnum)` |
| Enviar carta | `send_letter(clave)` | `send_letter(...)` |
| Limpiar carta | `clear_letter()` | `clear_letter()` |
| Esperar | `wait()` | `wait()` (yield → evento) |
| Cambiar estado | `set_state(nombre)` | `set_state(...)` |
| Quest externa | `set_quest_state(quest, estado)` | `set_quest_state(...)` |
| Flag de quest | `set_qf(nombre, valor)` | `pc.setqf("k", v)` |
| Dar item | `give_item2(vnum[, count])` | `pc.give_item2(...)` |
| Quitar item | `remove_item(vnum, count)` | `pc.remove_item(...)` |
| Marcar target | `target_vid(nombre, npc_vnum, clave)` | `target.vid(...)` |
| Borrar target | `target_delete(nombre)` | `target.delete(...)` |
| Teletransporte | `warp(x, y)` | `pc.warp(...)` |
| Aviso global | `notice(clave)` | `notice(...)` |
| Aviso multilínea | `notice_multiline(clave, notice_all)` | — |
| Afecto/buff | `affect_add(apply.MOV_SPEED, 10, segundos)` | `affect.add_collect(...)` |
| Quitar afecto | `affect_remove(...)` | — |
| Menú | `select(clave1, clave2...)` | `select(...)` (devuelve índice → ramas) |
| Input | `input_number(clave)` | `input_number(...)` |

**Catálogo completo:** las 982 entradas de `quest_functions` se auditan en la conversión; solo las usadas por el corpus real se portan al DSL. El resto muere o pasa a Rust.

### 11.6 Familias de quest con parámetros

Elimina la repetición de quests casi-idénticas (caso real: 11 archivos `collect_quest_lv30..lv96`).

```quest
# quests/biology/collect_quest.family.quest
quest collect_quest family (level, mob, herb, drug)
  state start
    on login, levelup with pc.level >= (level)
      -> set_state(information)

  state information
    on letter
      -> send_letter(@100_sendLetter)

    on (mob).kill with number(1, 100) <= 5
      -> give_item2((herb), 1)

    on (drug).use with get_qf(duration) == 0
      -> remove_item((drug), 1)
      -> set_qf(duration, get_time() + 60 * 60 * 22)

# instancias (quests reales)
quest collect_quest_lv30 = collect_quest(level: 30, mob: 601, herb: 30006, drug: 71035)
quest collect_quest_lv40 = collect_quest(level: 40, mob: 602, herb: 30007, drug: 71036)
quest collect_quest_lv50 = collect_quest(level: 50, mob: 603, herb: 30008, drug: 71037)
```

- Parámetro: `(nombre)` en condiciones/acciones. Claves de texto con parámetro: `@100_sendLetter` (clave de locale; el conversor la genera por nivel).
- **El conversor automático detecta quests diff-casi-idénticas y las agrupa en familias** (heurística + confirmación humana).

### 11.7 Bloques reutilizables e imports

```quest
# quests/common/helpers.quest
block npc_target(npc: vnum, clave: key)
  -> target_vid(__TARGET__, (npc), (clave))

block reward_sequence(title, text, next_state)
  -> say_title((title))
  -> say((text))
  -> wait()
  -> set_state((next_state))
```

```quest
# quests/biology/collect_quest_lv30.quest
import helpers

state information
  on letter
    use npc_target(20084, @150_sayTitle)
    -> send_letter(@10_sendLetter)

  on 20084.chat
    use reward_sequence(@50_sayTitle, @40_say, go_to_disciple)
    -> set_qf(duration, 0)
```

- `block`/`use` solo componen acciones/condiciones tipadas. `import` comparte bloques entre quests. Ciclo de import = error.

### 11.8 Casos especiales → Rust (no DSL)

Gestores de eventos con lógica real de coordinación (`oxevent.quest`, `christmas_*`, `game.set_event_flag`): se reimplementan como **módulos Rust del servidor** (mismo API de triggers/acciones vía bindings nativos). El DSL no crece hacia lenguaje general.

### 11.9 Conversión automática del legacy (qc → DSL)

1. Parsear los 194 `.quest` con el parser de qc (AST real del DSL legacy).
2. Traducir AST → DSL v2 (tablas de equivalencia de §11.3-11.5).
3. Detección de familias (diff de ASTs) + agrupación propuesta.
4. Extracción de bloques repetidos (subárboles comunes).
5. **Harness de paridad**: ejecutar la misma quest en legacy (oráculo) y en el motor Rust con los mismos inputs → mismo estado final y salida de diálogos.
6. Salida: `quests/` en DSL + informe de discrepancias + quests que requieren revisión manual (→ Rust).

**Regla:** ninguna quest migrada se da por convertida sin pasar el harness de paridad.

### 11.10 Ramas y flujo dentro de un evento

```quest
on 20011.chat
  -> select(@_20_say, @_30_say) as choice
  if choice == 1
    -> warp(896500, 24600)
  else
    -> return

on 20011.chat with get_gm_level() == 5
  -> input_number(@_160_say) as amount
  if amount > 200
    -> say(@_250_say)
```

- `as <nombre>` captura el resultado; `if/else` ramifica solo sobre resultados capturados y condiciones simples. Sin bucles. Sin variables mutables fuera del evento (contadores → `set_qf`/`get_qf`).

### 11.11 Decisiones abiertas del DSL

1. `between a, b`: ¿sintaxis nativa o solo comparaciones?
2. `if` dentro de eventos: ¿1 nivel + else (propuesto) o ilimitado?
3. ¿`select` con captura `as` cubre todos los usos del corpus?
4. ¿Claves de locale: `@clave` o literal directo?
5. Naming: `.quest` vs `.qdsl` vs `.mq`.
6. ¿`wait()` requiere trigger `timer` explícito o basta `with get_time() >= get_qf(...)`?

### 11.12 Fuera de alcance del spec

- Motor de ejecución Rust (máquina de estados + scheduler de `wait()`) — F5.
- Harness de paridad — F0/F5.
- Validador CLI `quest-validate` y schema de editor — tras cerrar el spec.
- Editor visual GUI — **excluido** (YAGNI; validador + schema cubren el 90%).

---

## 12. Preguntas para los revisores

1. **Modelo de autoridad**: ¿algún contraargumento a «cliente envía intenciones, servidor calcula, BD garantiza»? ¿Hay hacks de Metin2 conocidos que este modelo no cubra?
2. **Stack**: ¿algo mejor que PostgreSQL 18 + sqlx para un MMO single-node en 2026? ¿Alguna feature de PG 19 (GA oct-2026) que justifique esperar?
3. **Concurrencia**: ¿mundo por región con ECS (bevy_ecs standalone) es la elección correcta para escalar (más mobs, más jugadores)? ¿O actores desde el inicio? (Postura: regiones + ECS, YAGNI en multi-proceso hasta benchmark.)
4. **Quests**: el DSL propio — ¿la gramática es elegante y completa para el corpus? ¿faltan triggers/condiciones/acciones? (decisiones abiertas en §11.11)
5. **Migración**: ¿el orden F0→F6 es el correcto? ¿Faltaría un paso de validación entre fases?
6. **Alcance**: ¿es correcto diferir eventos/raids/social masivo al final?
7. **Auditoría §2.3**: ¿falta alguna decisión del legacy en la tabla P0/P1/P2?
8. **Adopción**: ¿MPL-2.0 es la licencia correcta? ¿API web + metrics + Docker desde F5 o después del corte?
9. **Canales regionales (§4.4)**: ¿el modelo «BD central + proceso por región, cambiar de región = logout→login» es el correcto? ¿Mundo vivo compartido tipo EVE queda descartado por diseño?
10. **Persistencia (§4.5)**: ¿el pipeline WAL local + mutation_id + batch ≤100ms es correcto? ¿El failover con Patroni es suficiente para un pserver?
11. **Datos servidor→cliente (§4.6)**: ¿el manifest versionado + delta es el mecanismo adecuado? ¿Los 2 paquetes aditivos al cliente son aceptables antes de F7?
12. **Lo que no veo**: ¿qué estamos pasando por alto?

---

## 13. Próximos pasos

1. Recoger feedback de los revisores sobre este documento.
2. Escribir los ADRs pendientes: límites de dominio (char.cpp por sistemas), concurrencia (regiones + ECS), motor de quests (DSL propio), modelo anti-hack, canales regionales (BD central + proceso por región), capa de datos (WAL + mutation_id + RLS + failover), datos servidor→cliente (manifest + delta), revisión de la migración del ADR-0002.
3. Actualizar `ROADMAP.md` con las correcciones (F2 auth como modo, F3 backend trait + migración PG en F3, F0 ADRs).
4. Escribir el plan de implementación formal con tareas granularizadas (por dominio, TDD, verificación por script).
