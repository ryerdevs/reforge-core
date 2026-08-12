# reforge — reescritura del servidor Metin2 en Rust

Reescritura incremental (strangler fig) del servidor C++ legacy de Metin2.
La línea base C++ es el oráculo de comportamiento; cada crate se verifica
contra ella antes de avanzar (ROADMAP.md, ADR-0003, ADR-0004).

## Estructura

| Ruta | Crate | Rol |
|---|---|---|
| `protocol/` | protocol | Paquetes byte-exactos del wire (F0): flujo de login — 81 tests |
| `network/` | network | Transporte tokio (F1): listener, connection, framer + auth (F2) — 28 tests |
| `database/` | database | Capa de datos por dominios (F3): account/world/social/economy/log + WAL durable (ADR-0008) — 70 tests |
| `realm/` | realm | Lógica de juego por regiones (F4+): funciones puras (combate, ai, packets) + estado por conexión + `WorldStore` — 64 tests. ECS diferido a benchmark (ver ADR de domain boundaries pendiente) |
| `server_realms/` | server_realms | Binario único con roles: `--role auth` / `--role channel` — 42 tests |
| `mysql_proxy/` | mysql_proxy | Adaptador temporal MySQL→PostgreSQL para el C++ legacy (G-PG, ADR-0005): wire v10 + translate + session — 67 tests; se elimina en F6 |
| `locale_import/` | locale_import | Importer de locales a PG (F1, ADR-0009): un subcomando por dominio — 19 tests |

## Glosario de nombres (ADR-0004)

- **network** — antes `net`: transporte TCP y framing.
- **database** — antes `db`: capa de datos.
- **realm** — antes `game`: la región de juego (un proceso por región, ADR-0002).
- **auth** — no es un crate: es un módulo de `network` y un rol del binario `server_realms`.

## Build / test

```bash
cargo build   # workspace completo
cargo test    # 371 tests (conteo de atributos #[test]/#[tokio::test] por crate, 2026-08-12:
              #   protocol 81, network 28, database 70, realm 64, server_realms 42, mysql_proxy 67, locale_import 19)
```
