# reforge-core

> **Un MMORPG clásico de 2004 reimaginado desde cero: servidor en Rust, PostgreSQL 18, arquitectura server-authoritative y un diseño pensado para crecer.**
>
> Un servidor alternativo e independiente para un juego clásico de hack & slash asiático, recreado por **ingeniería inversa** desde el binario original — sin afiliación con el desarrollador o publicador original.

[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)
[![PostgreSQL](https://img.shields.io/badge/postgresql-18-blue.svg)](https://www.postgresql.org)
[![License: MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](LICENSE)

---

## ¿Qué es esto?

**reforge-core** es la reescritura completa del servidor de un **MMORPG clásico de 2004** (género hack & slash con mundo persistente, gremios y PvP) en **Rust**, con las tecnologías de 2026 y un objetivo claro: **hacer más con menos**.

No es una traducción línea por línea del C++ original — es un **rediseño estructural** que:

- 🛡️ **Elimina los hacks de raíz**: servidor autoritativo — el cliente envía *intenciones*, el servidor calcula *hechos*. Speedhack, god-mode, dupe y memory hacking dejan de existir por diseño.
- 🚀 **Rinde al máximo**: tokio + ECS por región, sin locks compartidos, sin SQL en el hot path. Techo de **1.000+ jugadores por instancia** (el original: ~300-500).
- 🌍 **Un solo servidor para todos**: canales regionales (EUW/LAN/NA) que comparten la misma base de datos — juega con ping local, cambia de región solo con loguear, mercado unificado.
- 🔄 **Hot reload**: textos, items y quests se editan en la BD y se recargan en caliente — sin reiniciar, sin recompilar, sin repaquear.
- 📦 **Sin scripting**: las quests pasan de Lua a un **DSL declarativo propio** — elegante, tipado, con familias y bloques reutilizables.
- 🗄️ **PostgreSQL 18** como red de seguridad transaccional: WAL local, `mutation_id` idempotente, RLS, failover — el dupe es imposible por construcción.

> ⚠️ **Nota legal:** este proyecto es un servidor alternativo independiente, construido por ingeniería inversa. No tiene afiliación con el desarrollador ni el publicador del juego original, y no incluye sus assets ni su contenido protegido — solo código original.

## Estado

| Fase | Estado |
|---|---|
| Línea base del binario original verificada (login completo contra el cliente) | ✅ |
| Auditoría del legacy vs estándares 2026 | ✅ |
| Plan de reescritura unificado | ✅ [ver plan](docs/superpowers/plans/2026-08-09-servidor-rust-plan-unico.md) |
| Spec del DSL de quests | ✅ [ver spec](docs/superpowers/specs/2026-08-09-quest-dsl-spec.md) |
| F0–F6 (servidor Rust) | ⏳ en diseño |
| F7 (cliente nuevo) | ⏳ después del servidor |

## Arquitectura en 30 segundos

```
Cliente (binario original congelado + 2 paquetes aditivos) ──► Servidor Rust
                                                                    │  net (tokio)
                                                                    │  regiones en paralelo (ECS)
                                                                    │  db crate (sqlx, nunca inline)
                                                                    ▼
                                                            PostgreSQL 18 central
```

- **Un proceso por región** (agrupaciones del mapa) con ECS (`bevy_ecs`): simulación paralela, single-writer por entidad.
- **BD central compartida** entre canales-región: personaje, oro, mercado y gremios unificados.
- **Persistencia en dos clases**: durable (items/oro → WAL + transacción) vs volátil (posición → periódico).
- Detalle completo en el [plan unificado](docs/superpowers/plans/2026-08-09-servidor-rust-plan-unico.md).

## Repositorio — qué contiene

```
source/
├── client/     # Código C++ del cliente v40999 (contrato de protocolo)
├── server/     # Código C++ del servidor legacy (la referencia a portar)
├── tools/      # Herramientas: DBManager, DumpProto, switch_compiler + proto/
│   └── proto/  #   Metadatos de protocolo
├── pack/       # Fuentes del pack (python, uiscript, PackMakerLite)
└── svfiles/    # Runtime desplegado (local, no va a git)
docs/           # Plan de reescritura, specs, ADRs
scripts/        # Scripts de arranque del servidor (WSL/Linux)
ROADMAP.md      # Plan maestro por fases
CHANGELOG.md    # Registro cronológico de cambios
```

> **Binarios y packs no están en git.** El cliente instalado, los `.epk`, las dependencias de build (`source/client/Extern/`) y el runtime (`source/svfiles/`) se quedan en local o se distribuyen como Releases.

## Roadmap

| Fase | Contenido |
|---|---|
| **F0** | Workspace Rust, ADRs, crate `protocol` byte-exacto, harness de captura |
| **F1** | Red/transporte con tokio |
| **F2** | Auth Rust + primeras modificaciones del cliente |
| **F3** | Capa de datos (PostgreSQL) + canal de datos servidor→cliente |
| **F4** | Entrada al mundo + nombres UTF-8 |
| **F5** | Gameplay completo, benchmark de escala, hot reload, API + metrics |
| **F6** | Paridad total y reemplazo del C++ |
| **F7** | Cliente nuevo (wgpu + UI Slint) |

## ¿Quieres participar?

Este proyecto quiere **revivir el género clásico con la comunidad**: documentación pública de protocolo, anti-bot efectivo, y un servidor moderno que los operadores puedan adoptar. Issues, PRs y opiniones sobre el plan son bienvenidos — las decisiones de arquitectura se discuten en los documentos de `docs/` antes de implementarse.

## Licencia

**MPL-2.0** — permisiva para operadores de servidores privados (pueden correr y modificar sin abrir su trabajo completo). Ver [LICENSE](LICENSE).
