# Progress — Metin2 Reforge

## Goal
Corre todo lo que falta para que el servidor sea mejor que el original, sin 1:1, con menos líneas y sin decisiones del 2000. Preset OmO muse-spark-1.2-contributor.

## Current

- HEAD: `95bf3e0` (feat(refine): full roll with scroll verifier)
- Tests: **822** (cargo test --workspace -- --list, 2026-08-27 — conteo real de items, sin run)
- Docs: `documentation/` (20 vivos — 5 hub + 14 ADR + reference/login-flow), `scripts/status.ps1|verify.ps1|handoff.ps1|clean.ps1`, CI `.github/workflows/docs.yml` ✓
- Handoff: último commit y slices recientes (stats 5/nivel, items attrs, exp delta, guild x4, pvp, splash, horse, aiflags, weight, locale, dungeon, event, land, belt, dragon soul, refine, etc.)

## Todos (restante)

Checklist vs `documentation/history/plans/server-side-gap-2026-08-15.md` + estado real del código (HEAD `95bf3e0`):

- [x] stats 5/nivel infinito (ADR-0014)
- [x] items attrs (roll_attrs + sockets/magic — database::attr, verifier apply_index)
- [x] exp level-delta (tabla parity + low-mob nunca full — combat)
- [x] guild basic/wire/grade/comment/ranking/PG persist (war = stub; member_flow_roundtrip + live PG)
- [x] pvp can_attack (gates guild/party/safe-zone/protect; PK mode parse 10b/2b + daño)
- [x] skills splash/horse/skill_power (splash radius/max_hit/pvp gate; horse mounted; k_value por tabla)
- [x] aiflags berserk/coward/godspeed/stoneskin (ecs combat)
- [x] weight (gate fail-open hasta importar item_proto.weight — ver pendiente)
- [x] locale push (GC_LOCALE chunked al conectar; pull pendiente)
- [x] dungeon/event/land/belt/dragon soul/refine stubs (d_soul upgrade stage/grade ya)
- [x] refine full (roll 1..100 ≤ refine_proto.prob real, scroll destroy/degrade, fee, ventana — items.rs:1664; REFINE_SUCCESS_PCT 70 queda solo en game_core::refine)

- [ ] party full — core ya (invite/answer/remove/parameter/set_state 75/use_skill 76 con verifier party_role_heal_summon_wired + exp NON_PARITY/PARITY con bonus tabla CHN); falta GC_PARTY_LINK/UNLINK (la ventana funciona sin él) y +30% item del líder (requiere trackear equipo, party.rs doc GAPs)
- [ ] safebox full — grid de size celdas (strip 5 ancho, item_proto.size real = 2679 items 2×2), gates EXPAND/antiflag SAFEBOX (667 vnums)/STACKABLE y `/safebox_change_password <old> <new>` DONE (2026-08-27, verifier checkin_gate_* + old_password_matches); falta checkout a DS/belt (safebox.rs:25)
- [ ] messenger full — core ya (2026-08-21, persist live PG); falta OTHER_SEX_ONLY (sin player.sex), matrimonio, block-mode/observer, textos INFO en EN
- [ ] guild full — persist/ranking ya; falta war real (hoy stub, score verifier ya en game_core)
- [ ] pvp full — PK mode ya; falta penalización PK (exp/drop), war-PK
- [ ] skills full — falta familia PARTY, grand master, buffs ATT_SPEED/CASTING al gameplay
- [ ] weight con datos reales — item_proto.weight = 0 (11 002 filas); importar pesos para activar el gate
- [ ] locale pull — CG_LOCALE_REQUEST (hoy solo push)
- [ ] events/raids/dungeons full — hoy stubs (dungeon: ids + owning party ya)
- [ ] quests full — falta say_reward, send_letter, set_quest_state, target_vid, affect_*, timers/scheduler (corpus ~88 incompletas)
- [ ] GM commands restantes — ~9 reales de 174 (mob, kill, purge, goto, set, makeguild, setskill, polymorph, priv_empire...)

## Handoff

- 2026-08-27 | slice safebox full (coder, lane paralela del party): grid de `size` celdas (strip vertical paso 5, parity `IsEmpty(pos,1,size)` safebox.cpp:130-136) en checkin/checkout/item_move con item_proto.size real; gates EXPAND (71009) + antiflag SAFEBOX (1<<17) + STACKABLE/ANTIFLAG_STACK del stack; `/safebox_change_password` completo (gm parse + SafeboxRepo::change_password, strcasecmp del C++); verifiers: checkin_gate_rejects_antiflag_safebox (mutation PROBADA: roja al quitar el gate), _2x2_over_strip_cell, _2x2_out_of_grid, checkout_strip_respects_inventory_page_and_occupancy, stack_requires_stackable_dst_without_antiflag, old_password_matches_is_case_insensitive; smoke live PG del batch ANY($1) (9510: size 2 + antiflag) — workspace 784 passed / 0 failed; clippy sin nuevos.

- 2026-08-27 | slice party (coder): CG_PARTY_SET_STATE (75) + CG_PARTY_USE_SKILL (76) cableados en game.rs (antes caían a `other`) — handle_set_state parity input_main.cpp:2184-2239 (gates líder/miembro/rol, SetRole cupo 1, broadcast GC_PARTY_UPDATE) + handle_use_skill parity 2388-2415 (HealParty real con cooltime 60 min + SummonToLeader con anillo SUMMON_RING vía GC_WARP); protocol: TPacketCGPartySetState/UseSkill/GCLink/GCUnlink + consts 91/92; verifier party_role_heal_summon_wired. VE: protocol 100/100, server_realms 117/117. GAPs que quedan: GC_PARTY_LINK/UNLINK sin emitir, +30% item líder (sin equipo). OJO: lane safebox paralela tocó safebox.rs/gm.rs/database — su test checkout_strip... FAILED (safebox.rs:1082, suyo, no mío).
- 2026-08-27 16:55 | HEAD 4245dc6 | M source/reforge/game_core/src/gm.rs; M source/reforge/server_realms/src/channel/gm.rs; M source/reforge/server_realms/src/channel/session.rs; M source/reforge/server_realms/src/config.rs; ?? documentation/adr/0014-infinite-stats-five-per-level.md
- ADR-0014 + stats 5/nivel infinito implementado (gm.rs/session.rs/config.rs) — verifier 2 passed
- 2026-08-27 | slice weight (coder): game_core/src/weight.rs (weight_for_item/can_carry/max_weight (30+level*3+ST*2)*10) + gate pickup events.rs con GC_CHAT INFO + verifier weight_limit_rejects_pickup; game_core 214/214, server_realms check OK. DB item_proto.weight = 0 (11 002 filas): el gate es fail-open hasta importar pesos (pendiente: columna weight). VE: fórmula clásica GetMaxWeight (el C++ de la variante no tiene peso).
- 2026-08-27 | Goal started: corre todo lo que falta