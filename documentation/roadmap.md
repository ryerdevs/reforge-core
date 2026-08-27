# Roadmap — Metin2 Reforge

Hoja de ruta viva. Lo hecho está verificado con test/log/cliente.

## Hecho

- **F0 Foundations** — workspace Rust + protocol byte-exact (30/30) + login real
- **F1 Network** — tokio + framer + handshake (25/25)
- **G-PG Cutover** — PostgreSQL 18 única DB, `mysql_proxy`, parity 30/30, login real en PG
- **F2 Auth** — `server_realms` auth 30001 + channel 30003, world entry funciona
- **F3 Data** — WAL durable + `game_core` + bevy_ecs World + spawn dinámico
- **F5 Gameplay** — 63 parts: shops, trade, quest runtime, skills, refine, party, PvP, safebox (733 tests)

## Haciendo

- Docs unificados (esta reorg) + harness `planning/progress.md` + preset OmO muse-spark
- Audit 18 todos → archivado como diagnóstico, se ejecuta just-in-time por slice

## Futuro

- **F5 tail** — stat_points, party finish, refine polish, gold pickup, numeric buffs
- **F6** — unificación game+db, Patroni, 100% Rust, WSL `unregister`
- **F7** — cliente Rust (Bevy + slint), borrar `protocol::legacy`

## Regla

Nada es “done” sin evidencia: test/log/cliente o `verify.ps1` verde.
