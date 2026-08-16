# Server-side: qué falta en la reescritura (análisis completo, 2026-08-15)

> Comparación del rewrite Rust vs el C++ legacy congelado (oracle, ADR-0012).
> Dos análisis de exploración (superficie de módulos + profundidad por dominio) +
> los fixes C29-C32 ya implementados. Estado: workspace **652 passed / 0 failed**.

## Cifras

- C++ `game/src`: **146 archivos .cpp** (~104.7k LOC). Rust `game_core` + `channel`: **46 .rs** (~23k líneas). Ratio ≈ **4.5× menos código**, cobertura global ~40%.
- NOTA de frescura: el gap-analysis-2026-08-15.md está PARCIALMENTE desactualizado (ya hay handlers que el doc lista como ausentes: whisper, quickslot, target, chat broadcast, PvP flag).

## Ya implementado este bloque (C29-C32 — commits `10d5092`/`9e55397`/`3e05d3c`/`9a0b618`)

| Bug | Fix | Commits |
| --- | --- | --- |
| **C29 cooldown de ataque del mob** | `CalculateDuration(ATT_SPEED, 2000)` — legacy ~2s, ya no 250ms (8× más rápido) | `10d5092` |
| **C30 velocidad motion-based** | `motion(300) × 10000/CalculateDuration(move_speed, 10000)` — los 308 mobs con factor 0/1 ya no congelan | `9e55397` |
| **C31 rango de ataque exacto** | mob→PC = `range×1.15` sin floor 300; RANGE/MAGIC + BOW_DISTANCE | `3e05d3c` |
| **C32 change attack position** | reposicionamiento lateral cada 10s/1s — los mobs ya no se amontonan | `9a0b618` |

## Ranking de lo que falta (impacto para un jugador real)

### CRÍTICO — bloquean la progresión

1. **Refine/upgrade de items** — CG_ITEM_USE_TO_ITEM (60) y CG_REFINE (96) con frame pero sin dispatch; `refine_proto` (405 filas PG) ocioso. El loop de progresión de Metin2 no existe: el equipo nunca mejora.
2. **Gastar stat_points** — se cargan, el level-up los incrementa (+3 hasta 90) y se mandan en GC_POINTS, pero **no hay handler** (game.rs:214-226): el jugador sube de nivel y no puede asignar ST/DX/IQ/HT.
3. **Party** — CG_PARTY_INVITE/ANSWER/REMOVE/SET_STATE/USE_SKILL/PARAMETER con frame sin dispatch; `/party_request*` → "not implemented". Sin exp/gold compartido ni bonus 10%. El multijugador cooperativo no existe.
4. **PvP/PK** — `process_attack` solo resuelve `npc_view` → atacar a otro PC no hace nada. `/pvp` → "not implemented".

### ALTO — se nota al jugar

1. **Safebox** — `SafeboxRepo` (4 fns) con **cero callers**; CG_SAFEBOX_* sin dispatch. Sin banco, el inventario de 180 celdas es el techo duro.
2. **Buffs numéricos** (MOV_SPEED/ATT_SPEED/CRITICAL/CASTING_SPEED) — solo mandan GC_AFFECT_ADD (icono); solo MAX_HP/MAX_SP/DEF_GRADE_BONUS alteran gameplay. Las pociones de velocidad/crítico son cosméticas.
3. **Familias de skills SPLASH/PARTY/HORSE + grand master + skill_power.txt** — el 90% del árbol de skills no funciona (solo 1 ataque + 1 buff por job); `k_value = level×max_level/100` desviación documentada.
4. **Oro del suelo** — el pickup (vnum 1) va al INVENTARIO, no a `row.gold` → el oro recogido no se puede gastar (bug-registry:222).
5. **Exp con level-delta + cap 10%** — `GiveExp` (char_battle.cpp:2212-2290) con `NEW_GET_LVDELTA`, `MIN(GetNextExp()/10, iExp)`; el Rust hace `mob_exp × rate` → farmear mobs bajos da exp llena.
6. **Atributos aleatorios + sockets** — todo item creado nace con `[(0,0);7]`/`[0;3]`; no existen items mágicos ni engarces (tablas `item_attr` 54/`item_attr_rare` 20 ociosas).
7. **AIFLAGs de mob** — BERSERK (×2), STONESKIN (/2), GODSPEED (ATT_SPEED 250), COWARD (huye), DEATHBLOW (×4): jefes sin fases, cobardes sin huida.
8. **Muerte sin penalización** — `__GetExpLossPerc` no portado: morir no cuesta nada.

### MEDIO

 1. **Quests pendientes** — say_reward, send_letter, set_quest_state, target_vid, affect_*, input_number, timers (sin scheduler), CG_SCRIPT_BUTTON no-op (menú de NPC), quests de party/guild/marriage (TriggerKind::Rust sin implementar). ~88 quests incompletas.
 2. **Refinar/blend/cube** — recetas de crafteo ausentes.
 3. **Messenger + emociones** — CG_MESSENGER sin dispatch; `/kiss`/`/dance*` → "not implemented".
 4. **Exp/gold compartidos en party, bonus de guild** — distribución 0%.
 5. **Belt y Dragon Soul** — ventanas cargadas pero `CG_ITEM_MOVE` las rechaza ("belt/DS pendiente"); CG_DRAGON_SOUL_REFINE sin dispatch.
 6. **Comandos GM restantes** — 9 reales de 174; el 95% del cmd_info[] falta (mob, kill, purge, goto, set, makeguild, setskill, polymorph, priv_empire...).
 7. **Items USE_* variados** — pociones de buff (USE_ABILITY_UP), bolsas de oro (AUTOUSE_GOLD), cofres (TREASURE_BOX), bombas: sin efecto.
 8. **death penalty y rates de exp** — nivel muerto sin riesgo.
 9. **Damage flags del wire** — DAMAGE_BLOCK/POISON/CRITICAL/PENETRATE: el rewrite manda siempre NORMAL (visual).

### BAJO (contenido)

 1. **Eventos** — OXEvent, threeway war, arena, wedding/marriage, monarch: 0%.
 2. **Raids** — Blue Dragon, Dragon Lair: 0%.
 3. **Horse/Pet/Polymorph** — solo flag `riding`; `pet.is_summon=0` hardcodeado.
 4. **Dungeon/instancias** — 0%.
 5. **Land/construcción/privilegios** — solo GC_LAND_LIST en entry, sin construcción ni priv.
 6. **Skills de mob (UseMobSkill)** — jefes = tanques con auto-ataque.
 7. **Mob skills/FUNC_MOB_SKILL** — motions SPECIAL_1.. no emitidos.

## Estructura (deuda)

- **Headers GC ad-hoc por módulo** — 34 centralizados en `protocol::header`, pero GC_GUILD*/GC_SAFEBOX*/GC_MYSHOP/etc. se definen sueltos (sin fuente única del wire S→C).
- **Headers C→S sin frame** — guild/fishing/acce/mall/item-give/fly-targeting **cierran la conexión del cliente** (UnknownHeader).
- **Repos dormidos** — safebox.rs/messenger.rs/social.rs GuildRepo: capa de datos escrita, cero callers.

## Próximo bloque sugerido

1. **Stat points** (handler real — desbloquea la progresión sin UI nueva: el C++ de esta variante NO tiene header de stat-up, hay que decidir wire).
2. **Party** (CG_PARTY_* + exp/gold compartido).
3. **Refine** (CG_ITEM_USE_TO_ITEM + refine_proto).
4. **Oro del suelo** (fix del pickup vnum 1).
5. **Buffs numéricos** (MOV_SPEED/ATT_SPEED/CRITICAL al gameplay).
6. **Safebox** (CG_SAFEBOX_* + repos).
7. **Exp level-delta + cap 10%** (curva de progresión).
8. **AIFLAGs BERSERK/COWARD/GODSPEED/STONESKIN** (mobs, del análisis legacy).

## Actualización 2026-08-15 (2ª tanda)

- **AIFLAGs de mob (BERSERK ×2, STONESKIN /2, GODSPEED ATT_SPEED 250)** — DONE
  (feat(combat), columnas sp_berserk/sp_stoneskin/sp_godspeed del mob_proto, 3 tests;
  item #11 del ranking).
- **C22 oro del suelo → monedero** — DONE (fix(items), vnum 1 → row.gold + GC_POINTS).
- **Party (invite/answer/remove/parameter + exp repartida)** — IN PROGRESS (Fixer lane).
- **Verifier de mobs (C29-C32 + AIFLAGs + C22)** — IN PROGRESS (independiente).
- **stat_points** — SIN wire en esta variante del cliente (Packet.h:32-37 comentado;
  solo `/con+`/`/str+` como GM). Documentado: la progresión de stats requiere una
  decisión de wire (modificar el cliente) — bloqueado, no un bug del server.
