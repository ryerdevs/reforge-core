# reforge — reescritura del servidor Metin2 en Rust

Reescritura incremental (strangler fig) del servidor C++ legacy de Metin2.
La línea base C++ es el oráculo de comportamiento; cada crate se verifica
contra ella antes de avanzar (ROADMAP.md, ADR-0003, ADR-0004).

## Estructura

| Ruta | Crate | Rol |
|---|---|---|
| `protocol/` | protocol | Paquetes byte-exactos del wire (F0): flujo de login, 30/30 tests |
| `network/` | network | Transporte tokio (F1): listener, connection, framer + auth (F2, esqueleto) |
| `database/` | database | Capa de datos por dominios (F3): account/world/social/economy/log |
| `realm/` | realm | Lógica de juego por regiones/ECS (F4+): entidades, combate, items |
| `server_realms/` | server_realms | Binario único con roles: `--role auth` / `--role channel` |

## Glosario de nombres (ADR-0004)

- **network** — antes `net`: transporte TCP y framing.
- **database** — antes `db`: capa de datos.
- **realm** — antes `game`: la región de juego (un proceso por región, ADR-0002).
- **auth** — no es un crate: es un módulo de `network` y un rol del binario `server_realms`.

## Build / test

```bash
cargo build   # workspace completo
cargo test    # protocol 30/30 + network 23/23 (framing+handshake) + server_realms 3/3 (roles)
```
