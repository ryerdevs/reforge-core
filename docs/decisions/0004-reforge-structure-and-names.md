# ADR-0004: Estructura y nombres del workspace `reforge`

## Estado

Aceptado

## Fecha

2026-08-10

## Contexto

ADR-0003 definió el workspace Rust en `source/reforge` con layout plano y nombres de capa genéricos (`protocol`, `net`, `db`, `game`, `auth`). El usuario pidió una estructura más profesional y nombres con identidad, sin prefijo de marca (rechazó `m2-*`). Tras evaluar la propuesta de subdirectorio `crates/` (convención de tokio/serde/bevy), el usuario la **descartó**: prefiere el layout plano en la raíz del workspace.

Además decidió **UN solo binario** (no varios): la unificación `game`+`db` (ADR-0002) elimina el broker db como proceso, y el aislamiento entre canales que exige ADR-0002 se logra con **N procesos del MISMO binario** con config distinta (rol `auth` | `channel`), no con N binarios. Esto resuelve la inconsistencia entre ADR-0002 ("auth como proceso propio") y el plan único ("auth como modo del mismo binario"): auth es un rol del mismo binario, y el proceso que corre ese rol es el auth server.

## Decisión

1. **Layout plano** en `source/reforge` (sin subdirectorios): `protocol/`, `network/`, `database/`, `realm/`, `server_realms/`.
2. **Nombres de crates** (renombres sobre ADR-0003):

| Antes (ADR-0003) | Ahora | Justificación |
|---|---|---|
| `protocol` | `protocol` | Sin cambio — en contexto del workspace no es ambiguo |
| `net` | `network` | Comunica la capa completa; "transport" descartado por el usuario |
| `db` | `database` | Inequívoco; "db" era ambiguo |
| `game` | `realm` | Nombra el dominio (la simulación del mundo por regiones); "server" queda reservado para el binario |
| `auth` (crate) | módulo `network::auth` | El auth es capa de red pura (handshake, LOGIN3, keys, panama pack) — vive dentro de network (F2) |

3. **Un solo binario `server_realms`** (nombre provisional del usuario) con roles por config: `--role auth` (puerto 30001) | `--role channel` (región, puerto 30003). Un artefacto, N procesos aislados. Escala por config: regiones cross-server = rol nuevo, sin binario nuevo.
4. **Convenciones de workspace**: `[workspace.dependencies]` centralizado (tokio 1.49: rt-multi-thread/net/io-util/time/sync/macros), `[workspace.lints.rust] unsafe_code = "forbid"` (no hay unsafe; el lint lo garantiza), `rust-toolchain.toml` (1.97.0), `README.md` con arquitectura + glosario de nombres.
5. **Runtime**: el runtime legacy conserva `source/deploy` (copia Windows del árbol de instancias, gitignored; el árbol WSL `metin2_svfiles` NO se toca — los scripts de arranque dependen de esa ruta). 2026-08-10: el renombre intermedio a `source/realms` se **revirtió por corrección del usuario**. El nombre **`server_realms`** es el crate binario de la reescritura en `source/reforge/server_realms`, que albergará el binario compilado + configs desde F2 (nombre provisional del usuario).

## Alternativas consideradas

- **Subdirectorio `crates/` + `servers/`**: propuesto (convención de industria), rechazado por el usuario — prefiere el plano. Se revisita solo si el workspace crece a >8-10 crates (YAGNI).
- **Prefijo `m2-`/`metin2-`**: rechazado por el usuario — nombres sin prefijo.
- **`transport` para net**: rechazado por el usuario — `network`.
- **Varios binarios (`auth-server`, `channel-server`)**: rechazado — duplican main()/config/deps y pueden divergir; el binario único escala por config.
- **Nombres del runtime**: `deploy` (se queda — el renombre a `realms` se revirtió por corrección del usuario), `runtime`/`production` (no evocan el caso), `release` (colisiona con `cargo build --release` y GitHub Releases) → `realms` pasó al nombre del binario de la reescritura (`server_realms`, provisional).

## Consecuencias

### Positivas

- Identidad del proyecto (nombres de dominio, no de implementación).
- Raíz del workspace limpia y legible; un solo artefacto de build.
- Escalado por config (roles), no por binarios; regiones nuevas = config nueva.
- Límites claros: `server_realms` (ejecutable fino) vs crates (librerías).

### Negativas

- Los renombres tocan referencias en docs/specs/ADRs (se actualizan en la misma sesión).
- La división de `protocol/src/lib.rs` (~1700 líneas) en módulos queda **diferida a F2** (cuando entre PanamaPack y los paquetes del mundo) — YAGNI hoy.
- Si el workspace crece mucho, el layout plano se llenará — se revisita `crates/` en ese momento.

## No decidido en este ADR

- Estructura interna de módulos de `protocol` (F2).
- **Config del binario: TOML — DECIDIDO (2026-08-10).** Los configs de `server_realms` (rol, región, puertos, rates...) se escriben en **TOML** (comentarios, tablas anidadas, Rust-nativo; vía config-rs en F2; clap para los args `--role`). `server_realms/` albergará el binario compilado + configs desde F2.
- Esquema de instancias del runtime (`source/deploy`) y de la carpeta `server_realms` (binario + configs) — F2/F5.
