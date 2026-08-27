# Progress — Metin2 Reforge

## Current

- Fecha: 2026-08-27
- HEAD: `59b6be9` (63rd part messenger/emotions, 733 tests)
- Árbol sucio: `gm.rs`/`social.rs` (+137/+34)
- Preset OmO `muse-spark-1.2-contributor` activo
- Plan audit: 18 todos en `.omo` (SHA `5649F62B`, APPROVE local, pendiente `$start-work` o archivado)
- `docs/CURRENT.md` stale (50th `9a0b618`)

## Handoff

- 2026-08-27 16:55 | HEAD 4245dc6 |  M source/reforge/game_core/src/gm.rs;  M source/reforge/server_realms/src/channel/gm.rs;  M source/reforge/server_realms/src/channel/session.rs;  M source/reforge/server_realms/src/config.rs; ?? documentation/adr/0014-infinite-stats-five-per-level.md
- ADR-0014 + stats 5/nivel infinito implementado (gm.rs/session.rs/config.rs) — verifier 2 passed

## Next

1. `docs/GUIDE.md` + `docs/plans/audit-2026-08-27.md`
2. Decidir si archivar el audit y volver a slices
3. T1 baseline si se ejecuta el audit

Last update: 2026-08-27 16:55 - handoff.ps1
- 2026-08-27 | slice weight (coder): game_core/src/weight.rs (weight_for_item/can_carry/max_weight (30+level*3+ST*2)*10) + gate pickup events.rs con GC_CHAT INFO + verifier weight_limit_rejects_pickup; game_core 214/214, server_realms check OK. DB item_proto.weight = 0 (11 002 filas): el gate es fail-open hasta importar pesos (pendiente: columna weight). VE: formula cl�sica GetMaxWeight (el C++ de la variante no tiene peso).
