# ADR-0003: Workspace Rust en `source/reforge` — layout y política del nuevo servidor

## Estado

Aceptado — **parcialmente superado por ADR-0004 (2026-08-10):** el layout y los nombres del workspace cambian (layout plano, sin `crates/`; `network`/`database`/`realm`; binario `server_realms` con roles). Se mantienen: carpeta `source/reforge`, límite de propiedad sobre la línea base C++, políticas de verificación.

## Fecha

2026-08-10

## Contexto

El proyecto reescribe el servidor de Metin2 en Rust (ROADMAP F0–F7, plan único `docs/superpowers/plans/2026-08-09-servidor-rust-plan-unico.md`). El código legacy C++ vive en `source/{client,server,tools,pack,deploy}` y debe permanecer **intacto y estable** durante toda la migración (regla de AGENTS.md: dos copias de source, la de WSL compila el servidor; la línea base C++ es el oráculo de tests).

El usuario decidió (2026-08-10): el nuevo servidor Rust va en una **carpeta nueva** `source/reforge`, dentro del mismo repositorio, para no modificar nada de la línea base C++.

El plan F0 exige: "Workspace Cargo con crates: `protocol`, `net`, `db`, `game`, `auth`" + "Implementar crate `protocol` (login flow) con golden tests de los structs del spec". El contrato byte-exacto ya está especificado (`docs/superpowers/specs/2026-08-08-wire-protocol-login-flow.md`).

## Decisión

1. **Ubicación:** workspace Cargo en `source/reforge`, en el repo actual (`origin → github.com/ryerdevs/reforge-core.git`). No se crea un repo ni una branch separada; el repo existente ES el repo del proyecto.
2. **Layout del workspace** (un crate por capa, según plan F0):
   - `protocol` — paquetes byte-exactos del wire (cliente↔servidor y peer legacy), sin dependencias (std only). Primera implementación: el flujo de login completo del spec §3.
   - `net` — transporte tokio (F1): framing, listener, semántica `result > 0`/EAGAIN.
   - `db` — capa de datos por dominios (F3), crate interno (ADR-0002).
   - `game` — lógica de juego por regiones/ECS (F4+).
   - `auth` — modo auth del binario (F2).
3. **Política de crates:** edition 2024, `resolver = "3"`, dependencias SOLO cuando la fase las exige (ponytail: YAGNI). `protocol` arranca **zero-deps**: serialización manual LE byte-exacta, sin serde/bincode.
4. **Política de verificación:** cada crate compila con `cargo build` y pasa `cargo test` desde el primer commit; el crate `protocol` incluye golden tests byte a byte construidos desde el spec (y más adelante desde capturas tcpdump reales — harness F0).
5. **Límite de propiedad:** `source/reforge/**` es propiedad exclusiva del workspace Rust. Nadie edita `source/server`, `source/client` ni `source/deploy` en esta línea de trabajo.

## Alternativas consideradas

### Repo separado para el server Rust

Rechazada: el repo actual ya se llama `reforge-core`; un segundo repo duplica la gestión de issues/CI y complica el referenciado del contrato (specs, ADRs) que viven en este repo. El plan §"Repositorio GitHub" ya define qué sube al repo (solo fuentes); `source/reforge` cumple ese criterio.

### Branch separada `reforge` en el mismo repo

Rechazada por ahora: la línea base C++ está estable y es el oráculo; trabajar en `main` con la carpeta nueva mantiene el flujo commit+push simple del usuario. Si el trabajo Rust empieza a entorpecer la base, se evalúa branch en su momento (YAGNI).

## Consecuencias

### Positivas

- La línea base C++ queda físicamente separada: cero riesgo de edición cruzada (la lección del desastre de las dos copias de source).
- El contrato (specs/ADRs) y el código Rust conviven en el mismo repo: trazabilidad directa.
- Workspace multi-crate listo para crecer por fases sin reestructurar.

### Negativas

- El repo contendrá el código legacy y el nuevo juntos; el diff de PRs puede mezclar dominios si no se respeta el límite de propiedad (regla 5 de este ADR).
- `cargo` necesita toolchain Rust en la máquina de build (verificado: cargo/rustc 1.97.0 local; edition 2024 soportada).

## No decidido en este ADR

- Modelo de concurrencia interno de `game` (regiones + ECS) — ADR propio (pendiente F0 del plan).
- Límites de dominio / propiedad de datos — ADR propio.
- Engine de quests (DSL) — ADR propio.
- Anti-hack, canales regionales, capa de datos, manifest — ADRs propios (lista del plan §13.2).
