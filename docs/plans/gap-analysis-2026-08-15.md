# Gap Analysis — Legacy C++ vs Rust rewrite (2026-08-15)

> Comparación cuantitativa contra el oráculo congelado (`source/server`, ADR-0012).
> Metodología: conteos reales de código fuente + logs de runtime + dump legacy.

## 1. Comandos de chat (`/cmd`) — brecha 174 vs 9

El C++ registra **174 comandos** (`cmd_info[]`, cmd.cpp:276-500). El Rust implementa **9**
(`game_core/src/gm.rs`: warp, item, notice, level, restart_here, restart_town, logout,
quit, phase_select). **El 95% de los comandos no existen.**

El usuario confirmó el impacto real: **los botones del cliente mandan comandos por el chat**
(`PythonNetworkStream.cpp:203-240` — ExitApplication → `/quit`, ExitGame → `/phase_select`,
LogOutGame → `/logout`). Los comandos GM_PLAYER (nivel 0, sin gmlist) del bug 4 ya cubren
los de cierre/revive, pero faltan los de juego:

| Comando GM_PLAYER (nivel 0) | Estado | Uso |
| --- | --- | --- |
| `safebox`, `safebox_close` | ❌ | Caja del banco |
| `mount` / `horse_*` | ❌ | Monturas |
| `party_request*` | ❌ | Grupos |
| `pvp` | ❌ | Modo PvP |
| `view_equip` | ❌ | Ver equipo ajeno |
| `observer` | ❌ | Modo espectador |
| `emotion_allow` / `kiss` / `slap` / `dance*` | ❌ | Emociones |
| `set_walk_mode` / `set_run_mode` | ❌ | Andar/correr |
| `skillup`, `gskillup` | ❌ | Subir skills |

Comandos GM (nivel 1+): `mob`, `kill`, `goto`, `set`, `reset`, `makeguild`, `stat`,
`notice_map`, `purge`, `transfer`... — todos pendientes (subset actual: warp/item/notice/level).

## 2. Headers de juego C→S — 27 en el framer, 16 con dispatch

De los 27 headers de fase de juego en la tabla del framer (`network/src/framer.rs`),
el game loop (`server_realms/src/channel/game.rs`) maneja **16**; **11 caen en `other`
y se ignoran en silencio**:

| Header ignorado | Función C++ | Impacto |
| --- | --- | --- |
| CG_ITEM_DROP (12) | Soltar item | ❌ |
| CG_ITEM_DROP2 (20) | Soltar oro/cantidad | ❌ |
| CG_ITEM_USE_TO_ITEM (60) | Item sobre item (refinar) | ❌ |
| CG_QUICKSLOT_ADD/DEL/SWAP (16/17/18) | Barras rápidas | ❌ |
| CG_SCRIPT_BUTTON (66) | Botones de diálogo | ⚠️ parcial |
| CG_QUEST_INPUT_STRING (30) / CG_QUEST_CONFIRM (31) | Input/confirm quest | ❌ |
| CG_PVP (41) | PvP | ❌ |
| CG_MYSHOP (55) | Tienda de jugador | ❌ |
| CG_FLY_TARGETING (51) | Skill área | ❌ |
| CG_SHOOT (54) | Disparo arco | ❌ |
| CG_WHISPER (19) | Mensaje privado | ❌ |

## 3. Tiendas NPC — BUG DE DATOS (no de código)

**Síntoma**: el click en vendedor no abre. El diag (añadido 2026-08-15) reveló:
`world: shop Open vid X — NPC vnum 9003 SIN shop en la tabla`.

**Causa raíz**: el legacy (`player.shop`, dump 2026-08-12) tenía los shops 1-8 con
npc_vnum = 9001-9009 (los vendedores del mapa c1). En el wave 45-46 los npc_vnum 1-3 se
re-asignaron a los vendedores del pueblo (20002/20006/20023) y **los vnums legacy
(9001, 9009, 9003, 9002...) se perdieron** — el NPC 9003 del pueblo ya no tiene shop.

| NPC clicado | Shop legacy | Shop PG hoy | ¿Abre? |
| --- | --- | --- | --- |
| 9003 | shop 3 (armaduras) | — | ❌ |
| 20016 | — | — | ❌ |
| 20002 | — | shop 1 (23 items) | ✅ |
| 20006 | — | shop 2 (7 items) | ✅ |
| 20023 | — | shop 3 (24 items) | ✅ |

**Fix propuesto** (requiere confirmación del usuario — regla "no game-data edits without
explicit confirmation"): re-asignar npc_vnum de los 33 shops para cubrir los vendedores
visibles del mapa c1 (9001-9009 + 20002/20006/20023/20042...). Los 15 shops con
npc_vnum=0 (all_*) no tienen vendedor asignado en el legacy tampoco.

## 4. Cobertura por dominio (estimación)

| Dominio | Estado | % |
| --- | --- | --- |
| Login/auth/world-entry | F2a/F2b/F4 — verificado con cliente real | ~90% |
| Movimiento + envelope + walkability | DONE (map.rs, movement.rs) | ~70% |
| Combate básico + AI (aggro/chase/patrol) | DONE (ecs systems) | ~50% |
| Skills | CG_USE_SKILL + buffs básicos | ~40% |
| Items (use/move/stack/equip/toggle) | DONE esta sesión | ~60% |
| Tiendas/trade | ShopTable + CG_SHOP + trade | ~55% (bug datos) |
| Quests | engine + corpus 194 | ~55% |
| Comandos | **9/174** | **5%** |
| Party/Guild | ❌ | 0% |
| Safebox/Messenger | ❌ | 0% |
| PvP/eventos/raids | ❌ | 0% |
| Refinar/blend/cube/DS/belt | ❌ | 0% |
| Data channel F3 (162/163) | ❌ | 0% |

**Estimación global: ~35-40% de la base jugable completa** (no el 90%).
El plan "Base jugable" de 5 bugs fue un PRIMER BLOQUE, no la totalidad.

## 5. Prioridades propuestas (siguiente loop)

1. **Tiendas**: re-asignar npc_vnum en PG (datos) + confirmar apertura (bug activo).
2. **Comandos GM_PLAYER**: safebox, mount, party, pvp, emociones — cierran los botones del cliente.
3. **Headers ignorados**: CG_ITEM_DROP, CG_QUICKSLOT_*, CG_WHISPER (fáciles, alto impacto).
4. **Safebox + Messenger** (bancos del jugador — muy usados).
5. **Refinar** (CG_ITEM_USE_TO_ITEM + refine_proto).
