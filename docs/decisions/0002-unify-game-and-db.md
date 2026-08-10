# ADR-0002: Unificar `game` y `db` en un solo proceso Rust (db como crate)

## Estado

Aceptado

## Fecha

2026-08-08

## Contexto

El servidor legacy (C++) separa la lógica en tres binarios: `auth` (30001/30002), `db` (30000) y `game`/cores (30003+). El binario `db` no es una base de datos: es un **broker SQL + coordinador cross-canal** que:

- concentra las conexiones SQL por slot (`SQL_PLAYER`/`SQL_ACCOUNT`/`SQL_COMMON`/`SQL_LOG`, 3 conexiones por slot — `DBManager.cpp`);
- mantiene estado compartido entre cores: `LoginData`, `GuildManager`, `ItemIDRangeManager`, `PrivManager`, `Monarch`, `ItemAwardManager`, `Marriage`;
- sirve las tablas de proto en boot (`PROTO_FROM_DB`);
- habla con cada core un protocolo de socket propio con request/response correlacionado por `ident` (`QID_*` + `ReturnQuery`), y **duplica estado** (caché de login en ambos lados: `game/src/db.cpp:134` vs `db/src/LoginData`).

Costo del split observado empíricamente (fixes 2026-08-08):

- un protocolo entero entre game↔db que hay que mantener;
- máquina de reconexión con bugs propios (flood de WRITE del fdwatch — fix #10: el interés WRITE persiste porque `oneshot` se ignora; hay que re-registrar READ tras drenar);
- orden de arranque obligatorio (mariadb → db → auth → cores);
- doble deploy y coordinación de versiones;
- el db como cuello de botella único y punto de fallo.

Lo que el split SÍ compra: (1) contención de crash (un core muere sin matar la capa SQL), (2) N cores contra 1 proceso db, (3) aislamiento de la capa SQL del event loop single-thread de `libthecore` (MySQL síncrono bloquearía).

En Rust con tokio, los tres beneficios se recomponen sin el protocolo:

- la contención de crash se recrea con panics por tarea (`catch_unwind`/`JoinHandle`) y topología proceso-por-canal;
- "un db para N cores" es exactamente lo que ya hace PostgreSQL (ADR-0001);
- el aislamiento SQL lo da un pool async (`sqlx`/`PgPool`) sin event loop bloqueante.

El seam game↔db es limpio: se comunica exclusivamente por un paquete tipado pequeño (`HEADER_GD_*`/`HEADER_DG_*` en `common/tables.h`, `QID.h`) y el binario db son solo ~12.8k LOC. La decisión es de bajo riesgo en ambos sentidos; el costo de separar es duplicar el contrato de paquetes, y el de unificar es perder la frontera de proceso.

## Decisión

**Unificar `game` y `db` en un solo proceso Rust por canal, con `db` como crate interno** (límite de librería, no de proceso).

- Un binario por canal + `auth` como proceso propio (proxy fino). `db` es un crate (`metin2-db`) embebido que expone la misma funcionalidad del legacy db como API interna; no hay protocolo game↔db en Rust.
- La coordinación cross-canal que hoy vive en el proceso db se mueve a PostgreSQL: sequences con batches para `ItemIDRangeManager`, row locks/advisory locks para guilds/parties, `LISTEN/NOTIFY` para invalidación de caché, y el login registry como tabla.
- **Durante la migración (Fases 0–5):** el crate se compila también como daemon independiente que habla el protocolo legacy del peer (`HEADER_GD_*`/`HEADER_DG_*`), de modo que el game C++ sigue funcionando contra un db Rust (hito F3) y el cliente real nunca se entera. El shim debe ser fino: solo framing + dispatch, sin lógica de negocio.
- **La unificación final ocurre en F6**, cuando el último core se porta a Rust y el shim legacy se elimina.

## Alternativas consideradas

### Mantener la separación de procesos (db como servicio Rust separado)

Rechazada. Conserva el protocolo, la reconexión, la correlación por ident, el estado duplicado y el orden de arranque — toda la deuda que la reescritura quiere eliminar. El único beneficio extra real (aislamiento duro de crash) se recompone con proceso-por-canal.

### Unificar todo en un único proceso para los 9 canales

Rechazada. Un panic fatal o OOM en código compartido mataría todos los canales. El proyecto quiere "hacer más con menos", pero no a costa de un único punto de fallo global; la topología proceso-por-canal da el aislamiento correcto.

## Consecuencias

### Positivas

- Desaparecen el protocolo game↔db, la reconexión, el flood de WRITE (tokio gestiona el interés de escritura internamente) y el estado duplicado.
- `db`-como-crate es directamente testeable con golden tests, sin sockets.
- Un solo binario+config por canal; deploy más simple.
- La frontera de proceso se mantiene donde importa: cliente ↔ canal, canal ↔ Postgres.

### Negativas

- La coordinación cross-canal en Postgres cambia latencia y semántica de consistencia (p.ej. doble login entre canales) respecto a la caché en memoria del legacy — requiere contratos explícitos y benchmark antes de portar `GuildManager`/`LoginData`.
- Pérdida de la contención dura del proceso db (un abort fatal en código compartido tumba el canal; mitigar con `catch_unwind` en fronteras de tarea y supervisión de restart).
- El shim legacy (F3–F5) es código de transición que debe mantenerse fino o se convierte en deuda.

## Puntos de decisión que el ADR fija ahora

1. **Propiedad de estado:** qué vive en Postgres vs en memoria por canal; destino de cada gestor del db legacy (LoginData → tabla + caché; ItemIDRangeManager → sequence con batches; GuildManager/Marriage/Monarch → tablas + row locks + NOTIFY); dueño y cadencia del save write-behind.
2. **Recuperación de crash:** qué pierde un canal al reiniciar; política de fsync/save; arranque sin orden entre procesos (solo Postgres primero).
3. **Topología de deploy:** proceso por canal, `auth` como proceso propio; un solo binario+config.
4. **Migración:** contrato del shim (qué headers legacy se mantienen, cuáles mueren) y punto de corte en F6.

## No decidido en este ADR

- Crate de acceso a PostgreSQL concreto (recomendación pendiente: `sqlx` 0.9, ver ADR de stack).
- Modelo de concurrencia interno (tokio task-per-connection vs actores) — ADR propio.
- Runtime lua para quests (mlua vs migración de datos UTF-8) — ADR propio.
- Esquema Postgres definitivo.
- Todo lo relativo al cliente (Fase 7).
