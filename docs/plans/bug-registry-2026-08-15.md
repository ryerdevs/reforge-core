# Registro de errores encontrados — Metin2 rewrite (2026-08-15)

> Bitácora viva de bugs confirmados y candidatos, para el siguiente bloque de trabajo.
> Estado: [ABIERTO] / [FIXED] / [VERIFICADO] / [PARCIAL] / [REVISANDO]
> Cada entrada: sintoma, causa raiz (con parity C++ citada cuando aplica), fix, estado.

---

## A. Bugs del usuario (sesion real 2026-08-15)

### A1. [FIXED-VERIFICADO] Revivir aqui no cierra la ventana de muerte

- **Sintoma**: los botones de revivir se quedan en pantalla al revivir "aqui".
- **Causa**: el C++ manda `ChatPacket(CHAT_TYPE_COMMAND, "CloseRestartWindow")` antes del revive (cmd_general.cpp:406,460); el Rust no lo enviaba. El pack lo usa para cerrar la ventana (game.py:1874).
- **Fix**: `revive()` envia el GC_CHAT tipo COMMAND "CloseRestartWindow" primero (ambos paths). CHAT_TYPE_COMMAND=5 (no 9), id=0.
- **Commit**: `c2c5ae8`. Verifier: PASS (parity byte-exacta).
- **Para probar**: morir -> revivir aqui -> la ventana debe cerrarse y el personaje reaparecer en el sitio.

### A2. [FIXED-VERIFICADO] Revivir en la ciudad warpea al sitio equivocado

- **Sintoma**: "revivir en la ciudad" teletransporta a 960640,263099 (punto de entrada) en vez del village.
- **Causa**: el Rust usaba `exit_x/exit_y`; el C++ usa EMPIRE_START_X/Y o GetRecallPositionByEmpire (cmd_general.cpp:475,489,554).
- **Fix**: warp siempre al village fijo del mapa 41 (969600,278400 UNITS).
- **Commit**: `c2c5ae8`. Verifier: PASS.

### A3. [FIXED-PEND-VERIFY] Subir de nivel no da puntos de stats

- **Sintoma**: al subir nivel, el contador de puntos de stats no aumenta.
- **Causa**: el level-up subia level/exp pero no sumaba stat_point.
- **Fix**: `level_up_step()` — +3 POINT_STAT por nivel (nivel pre-incremento <= 90). DEVIACION documentada: la spec decia +1 pero el ciclo completo del C++ da 3/nivel (char.cpp:3075-3136, ResetPoint char.cpp:5857).
- **Commit**: `875b4ae`. Verifier: en curso.
- **RIESGO ABIERTO**: si el orquestador prefiere +1 (spec), es un cambio de 1 linea.

### A4. [FIXED-PEND-VERIFY] Stats del equipo no aumentan los del personaje

- **Sintoma**: ponerse equipo no cambia el ataque/defensa visibles.
- **Causa raiz (doble)**:
  1. `weapon_min/max` leia values[0]/[1]; el dano real del arma es value3/value4 (battle.cpp:460-461, cliente __SetWeaponPower).
  2. El Rust enviaba GC_CHARACTER_ADDITIONAL_INFO (136) al equipar — paquete de ENTRADA que el cliente DESCARTA en runtime. El C++ envia UpdatePacket (GC_CHARACTER_UPDATE, 19) en EquipTo/Unequip (item.cpp:1004-1005) — el cliente calcula ATT_MIN/MAX localmente desde ese paquete (__RecvCharacterUpdatePacket).
- **Fix**: weapon de values[3]/[4] + nuevo TPacketGCCharacterUpdate (19, 51 B) enviado al equipar/desequipar.
- **Commit**: `875b4ae`. Verifier: en curso.

### A5. [FIXED] Item proto weight: int2 leido como i64 -> vuelta al login

- **Sintoma**: al entrar al mundo, el player se devuelve al login.
- **Causa**: el fix del peso (lane D) leia `weight` (smallint) con try_get::<i64> -> "error deserializing column 9".
- **Fix**: `let weight: i16 = r.try_get(9)` + i64::from (patron shop.rs:284-289).
- **Commit**: `6665941`. Desplegado.

---

## B. Hallazgos del Gap loop (verifiers adversariales, 2026-08-15)

### B1. [FIXED] Lane B: `/skillup 0` creaba skill fantasma + gastaba punto

- C++: CanUseSkill(char_skill.cpp:3572) `if (0 == dwSkillVnum) return false` (no-op). Rust mutaba el blob.
- Fix: `skillup_apply` devuelve None para vnum 0. Commit `c24fe70`.

### B2. [FIXED] Lane B: bMasterType nunca escrito (grado MASTER/GRAND/PERFECT)

- C++: SetSkillLevel (char_skill.cpp:207-217) setea bMasterType en cada subida (20/30/40 -> 1/2/3).
- Fix: `master_type_for_level` escribe blob[off]. Commit `c24fe70`.
- Layout del bytea confirmado: bMasterType@+0, bLevel@+1, tNextRead u32@+2 (tables.h:351-356) — el "vnum u32 en off" era incorrecto.

### B3. [FIXED] Lane A: entry.rs:457 no compilaba (points_packet firma 3 args)

- El commit e6ca209 cambio la firma pero dejo 1 call site con 2 args. Ya resuelto en 3bcaf26 (lane E). Sin commit nuevo (correcto).

### B4. [FIXED] Lane C: NUL de cola rompia los comandos '/' (CRITICO — "No such command" para todo)

- El cliente manda el texto con NUL; parse_command con split_whitespace no matcheaba "comando\0".
- Fix: `chat_text()` recorta en el primer NUL ANTES del hook. Commit `70379f1`.

### B5. [FIXED] Lane C: sin caps de longitud -> DoS del cliente legacy

- GC_CHAT/WHISPER sin cap -> el cliente legacy hacia Recv sobre buf[1025]/[513] sin bound-check (stack overflow).
- Fix: caps 485 (chat) / 512 (whisper), CHATBUF_MAX 512. Commit `70379f1`.

### B6. [FIXED] Lane C: rango TALKING 1000 no parity (C++: todo el mapa, ~5500 efectivo)

- Fix: cull server-side por VIEW_RANGE 5000+500=5500. Commit `70379f1`.

### B7. [FIXED] Lane C: SHOUT con alcance/formato equivocados

- C++: canal completo, id=0, payload "Name : msg", cooldown 15s. Fix: rama propia + last_shout. Commit `70379f1`.

### B8. [FIXED] Lane C: GC_CHAT sin prefijo "Name : "

- El cliente pinta el payload verbatim (AppendChat); sin nombre en la ventana.
- Fix: `chat_payload()` arma "Name : msg". Commit `70379f1`.

### B9. [FIXED] Lane C: whisper case-sensitive vs FindPC case-insensitive

- Fix: eq_ignore_ascii_case. Commit `70379f1`.

### B10. [FIXED] Lane D: GC_CHARACTER_POSITION con header 28 (debe ser 43)

- 28 es GC_QUICKSLOT_ADD (4 B estaticos) -> desync del stream del cliente.
- Fix: header 43 (packet.h:159). Commit `18dc92f`.

### B11. [FIXED] Lane D: posturas corridas (SITTING_CHAIR=3/4, no 1/2)

- Fix: length.h:288-296. Commit `18dc92f`.

### B12. [FIXED] Lane D: quickslot types corridos (ITEM=1/SKILL=2/COMMAND=3)

- Fix: length.h:238-245; clear_item_refs con type 1. Commit `18dc92f`.

### B13. [FIXED] Lane E: CG_EMPIRE(90)/CG_CHANGE_NAME(106) sin framer -> rename desconecta

- Fix: 90->2, 106->27 en packet_size. Commit `073ee93`.

### B14. [FIXED] Lane E: rollback del create inefectivo (fila huerfana + nombre quemado)

- Fix: `delete_row` sin gate del indice (parity ClientManagerPlayer.cpp:901-907). Commit `073ee93`.

### B15. [FIXED] Lane E: player.rs delete() usaba dw_pid pero el esquema es dwPID (sqlstate 42703)

- Detectado por el oracle director (realm_pg 5/6). Fix: DELETE quest/affect con dwPID. Commit `4a6130c`.

### B16. [FIXED] Oracle: realm_pg nombre de 25 chars > varchar(24)

- Fix: 'e2e_rok_' (18 chars). Commit `4a6130c`.

### B17. [FIXED] Lane F: vendedores del pueblo 20002/20006/20023 sin shop (regresion)

- El fix legacy dejo sin shop a los vendedores visibles. Fix: 20002->1002, 20006->1003, 20023->1004. Commit `83f2481`.

---

## C. Errores/candidatos ABIERTOS (proximo bloque)

### C1. [ABIERTO] Quest actions pendientes

- `say_reward`, `send_letter`, `set_quest_state`, `target_vid`, `affect_*`, `input_number` — el engine los tiene como TODO. Fuente: CURRENT.md.

### C2. [ABIERTO] Safebox + Messenger repos dormidos

- `database/src/safebox.rs` (4 fns) + `messenger.rs` (3 fns) con CERO callers — sin handlers/framer/dispatch. Fuente: gap-analysis §6.

### C3. [ABIERTO] Skills GAPs

- SPLASH/PARTY/HORSE families, `skill_power.txt`, buffs numericos (MOV_SPEED/ATT_SPEED/CRITICAL no aplicados de verdad), quest-granted skills. Fuente: CURRENT.md.

### C4. [ABIERTO] GM mob spawn (`/mob`)

- Necesita intent nuevo del mundo (crear entidad + Spawned). Fuente: gap-analysis §1.

### C5. [ABIERTO] Party/Guild gameplay

- Headers en framer pero handlers "not implemented" (lane B los reconoce pero no los implementa). Fuente: gap-analysis §1/§2.

### C6. [ABIERTO] Data channel 162/163 inactivo

- Wire definido (protocol::datachannel) pero sin activacion; PROTO_FROM_DB pendiente. Fuente: ROADMAP:156.

### C7. [ABIERTO] Locale wire (ADR-0009)

- Importer listo, wire GC_LOCALE no (server no envia texto por idioma todavia). Fuente: CURRENT.md.

### C8. [ABIERTO] Benchmark ladder incompleto

- Solo paso 1 (100 bots). Faltan 250/500/1000 + per-tick CPU. Fuente: CURRENT.md.

### C9. [ABIERTO] Auction / trade unificado

- Trade entre jugadores basico existe; auction no. Fuente: CURRENT.md.

### C10. [ABIERTO] Client UTF-8 name overrides: API hecha, wire pendiente

- Fuente: ROADMAP F4 tail.

### C11. [ABIERTO] Subir stats: sin paquete del cliente identificable

- El level-up ahora da stat_point (A3) pero no hay handler para asignarlos (ST/DX/IQ/HT). Fuente: gap-analysis §6.

### C12. [ABIERTO] Envelope follow-ups

- N-violations auto-ban, path-line sampling, buffs/mounts recompute speed, warp re-anchor. Fuente: CURRENT.md.

### C13. [ABIERTO] Manifest/hot-reload

- NOTIFY -> reload -> manifest bump -> delta. Fuente: ROADMAP:184.

### C14. [ABIERTO] Regional channels (ADR sin escribir)

- El plan §4.4 exige procesos por region con anti-double-login; el ADR no existe. Fuente: ROADMAP:83.

### C15. [ABIERTO] Converter quests (~88 restantes)

- Corpus 194/194 convertido, pero ~88 con acciones sin mapear (say_pc_name, give_exp2, change_money...). Fuente: CURRENT.md.

### C16. [CANDIDATO] pesos del inventario no se muestran / max_weight formula

- El peso se implemento con (30+level+2*ST)*10 — el oracle noto que el C++ de esta variante NO tiene GetMaxWeight (parity clasica, no de este repo). REVISAR si el cliente lo muestra o si la formula debe ser otra.

### C17. [CANDIDATO] shim pi.exe en pnpm/bin (infra)

- Necesario para que pi-agents funcione (spawn pi ENOENT en Windows). Documentar en caso de reinstalar pnpm.

### C18. [CANDIDATO] modo RPC de pi-agents roto en Windows

- pi --mode rpc se cuelga sin salida. Limita workflow_create. Usar pi-subagents (Agent tool) en su lugar.

### C19. [ABIERTO — NUEVO, alto impacto] Envelope rechaza movimientos SIN ack al cliente

- **Sintoma (CONFIRMADO EN VIVO 2026-08-15, log 144800 — 74 rechazos)**: el jugador manda MOVEs que el server rechaza "fuera del envelope" 4-6 veces seguidas; el jugador queda CLAVADO.
- **Causa**: (1) el rechazo (movement.rs:199) solo hace log + Continue — el C++ hace Show()+Stop() (sincroniza, input_main.cpp:1463-1482); (2) la tolerancia permite 2925 u (500×3.25×1.8) pero los MOVEs en rafaga del cliente exceden o el anchor se acumula mal.
- **Fix propuesto**: enviar sincronizacion de posicion al rechazar + revisar el calculo del anchor/envelope para el patron real de MOVEs.

### C20. [CANDIDATO — NUEVO] Quickslot clippy error (WIP de otro proceso)

- channel/quickslot.rs:139 clippy::absurd_extreme_comparisons — pre-existente, sin commitear de otro proceso. Confirmado por 3 verifiers que no es de los lanes.

### C21. [ABIERTO — NUEVO] Level-up sin cap de nivel 99

- char.cpp:2976: `case POINT_LEVEL: if ((GetLevel() + amount) > gPlayerMaxLevel) return` — el C++ bloquea subir de 99. El rewrite (`level_up_step` session.rs:310) usa `saturating_add(1)` SIN tope.
- Fix: cap en `level_up_step` — si prev_level >= 99, no subir.

### C22. [CERRADO — fix `93b9290`] Pickup de ORO del suelo no suma al monedero

- El oro en el suelo (item vnum 1, del kill o drop) va por el path NORMAL de items en `PickupResult` (events.rs:205+): se le busca proto, se pesa, y se mete al INVENTARIO como item — NO suma a `row.gold`.
- C++ parity: `PickupItem` con vnum 1 (oro) → `PointChange(POINT_GOLD, count)` + GC_POINTS (el oro no entra al inventario).
- Fix: en `PickupResult`, si `gi.vnum == 1` → `row.gold += count` + GC_POINTS + save + quitar del suelo.

### C23. [ABIERTO — NUEVO, alto impacto] Mobs muertos NO respawnean en el sitio

- El Rust lee `regen.txt`/`npc.txt` (npc.rs:186) pero solo materializa/desmaterializa por DISTANCIA del jugador (spawn.rs `respawns_with_new_vid_on_approach`). El C++ respawnea por TIEMPO (regen con intervalos, char_manager.cpp:230-464). Un mob que matas queda muerto para siempre (hasta que el jugador se aleje y vuelva).
- Impacto: el farming no funciona — no hay mobs que respawneen en el sitio.
- Fix: timer de respawn por entrada (el regen.txt ya tiene el intervalo — parse_regen_record npc.rs:500).

---

## D. Diferido / no aplica (documentado, no bug)

- Peso: formula clasica no presente en este C++ — gate fail-open aditivo (B16 del verifier D).
- `f16_peer_smoke` inexistente en el arbol actual.
- Tests PG/WSL gated por diseno (map41_spawns, channel_pg).

### C26. [ABIERTO — NUEVO, CRÍTICO] Revivir en la ciudad no teletransporta

- El path answer==1 manda GC_WARP al village (969600,278400) pero NO actualiza `row.x/y` antes del `save()`. La reconexion (DirectEnter) hace player_load con la posicion guardada = la del death -> el jugador aparece donde murio, no en el village.
- Fix: en revive() answer==1, setear row.x/y al village + save ANTES del GC_WARP.

### C27. [ABIERTO — NUEVO] Botas no agregan velocidad de movimiento

- El C++ da velocidad por AddAffect(POINT_MOV_SPEED, item->GetValue(2)) al equipar botas (char_item.cpp:4337 y variantes). El Rust hardcodea b_moving_speed=100 (packets.rs:282) sin procesar affects de items equipados.
- Impacto: las botas no dan velocidad; tampoco pociones de velocidad.
- Fix: calcular b_moving_speed desde los affects de items equipados.

### C28. [ABIERTO — NUEVO] Mobs se amontonan (sin separación)

- No hay collision/separacion entre mobs — todos persiguen al jugador en linea recta y se superponen. El C++ tiene distancia minima entre mobs.
- Fix: no perseguir si hay otro mob en el radio (~50-100 u) + separacion en el patrol.

### C29. [CERRADO — fix `10d5092`] Mobs atacan ~8x mas rapido que el legacy (sin cooldown)

- El legacy golpea cada CalculateDuration(POINT_ATT_SPEED, 2000) ~= 2 s (char_state.cpp:1005-1012). El rewrite ataca cada tick de AI (250 ms) sin cooldown (combat.rs:118-148). Los mobs pegan ~8x mas rapido.
- Referencia completa: docs/plans/mob-legacy-behavior.md §8.1.

### C30. [CERRADO — fix `9e55397`] Velocidad de mob motion-based (se mueven raro)

- El legacy deriva la velocidad de la ANIMACION (GetMoveMotionSpeed char.cpp:2726-2749, ~300 u/s) no de la columna; el rewrite usa move_speed del mob_proto como u/s reales (ai.rs:16-58) → mobs ~3x mas lentos + dw_duration del GC_MOVE ~3x mayor → "se mueven raro".
- Referencia: mob-legacy-behavior.md §7.

### C31. [CERRADO — fix `3e05d3c`] Rango de ataque del mob equivocado

- Legacy: persigue a range*1.15, para en range*0.9/0.8 (char_state.cpp:991, 694-714). Rewrite: MAX(300, range*1.15) solo MELEE (combat.rs:290-298) → RANGE/MAGIC atacan a 300 y mobs con range<261 golpean desde 300 en vez de ~157-201.
- Referencia: mob-legacy-behavior.md §8.3.

### C32. [CERRADO — fix `9a0b618`] Mobs se juntan: falta change attack position

- El legacy reparte a los mobs alrededor del jugador con reposicionamiento aleatorio cada 10 s/1 s (char.cpp:5436-5462, 5869-5881). El rewrite tiene separate_landing (parche) pero no el change attack position → convergen al rango y se quedan pegados.
- Referencia: mob-legacy-behavior.md §8.12.

### C33. [ABIERTO → CERRADO — fix en curso] Exp sin level-delta ni cap 10%

- El rewrite daba `mob_exp × rate` sin importar la diferencia de nivel (farmear mobs bajos daba exp llena). El C++ usa `NEW_GET_LVDELTA` (`aiPercentByDeltaLev`, constants.cpp:235-266) + cap `MIN(GetNextExp()/10, iExp)` (char_battle.cpp:2210-2267).
- Fix: `exp_level_delta_factor` (tabla 31 valores, índice clamp `(mob+15)−player`) + cap 10% en `apply_kill`. mob_level en KillInfo.

### C34. [CERRADO — fix `c8f3bd5`/`9d6f101`] Buffs numéricos (CRITICAL/MOV_SPEED/ATT_SPEED) solo cosméticos

- CRITICAL_PCT → Affects::critical_pct (parity char_battle.cpp:1661-1675) + melee_damage ×2 + flag CRITICAL del wire.
- MOV_SPEED → recalcula la velocidad real del jugador (GetMoveSpeed = motion × 10000/CalculateDuration, char.cpp:2751-2754).
- ATT_SPEED → suma al denominador de GET_ATTACK_SPEED (battle.cpp:757-782).

### C35. [CERRADO — fix `21b25f0`] Morir no cuesta nada (death penalty)

- MIN(800000, next_exp × aiExpLossPercents% / 100) al morir en RestartAtSamePos (char_battle.cpp:310-337, constants.cpp:768); revive en ciudad exento.

### C36. [CERRADO — fix `7aac132`] Sin banco del jugador (safebox)

- /safebox_password abre (password, cooldown 10s, GC_SAFEBOX_WRONG_PASSWORD), checkin/checkout/item_move/gold con parity; SafeboxRepo wired (era 4 fns sin callers). Gaps: grid 2×2, antiflag, change_password, mall.

### C37. [CERRADO — fix `625380f`] Refine/upgrade de items inexistente (loop de progresión)

- CG_ITEM_USE_TO_ITEM (60) + CG_REFINE (96) con dispatch; refine_proto (405 filas) wired: NORMAL (fee cost×5, FAIL destruye) + SCROLL (consume scroll, FAIL baja al vnum previo). Gaps: specials, MONEY_ONLY, gates.

### C38. [CERRADO — fix `28f4395`] PvP/PK: atacar a otro PC no hace nada

- battle_is_attackable gate (parity battle.cpp:107-139): muerto/misma-party → false; PK ON (víctima o atacante PK_MODE_FREE) → true. Daño al Hp del PC + 2 eventos (atacante/víctima, parity SendDamagePacket). Muerte → GC_DEAD + revive. Gaps: ATTR_BANPK, guilds/duelos/arena, alignment, broadcast.

### C39. [CERRADO — fix `79ae59e`] Items sin atributos mágicos ni sockets

- attribute_set_index + alter_to_magic_item + add_rare_attribute + roll_creation_bonus (parity item_attribute.cpp); tablas item_attr/item_attr_rare wired; drops/GM/quest pueblan attrs+sockets. Gaps: engarce de gems (USE), addon, iRarePct.

### C40. [CERRADO — fix `11a2f69`] Skills de área (SPLASH) sin efecto

- process_skill → splash_damage (parity ComputeSkillAtPosition/FuncSplashDamage): centro = target/caster, radio dwsplashrange, lMaxHit, daño per-víctima + szSplashAroundDamageAdjustPoly, gate atacabilidad. SP/cooldown 1 vez. +FIX kill single-target: vista ANTES de remove_npc (exp/gold del kill por skill).

### C41. [CERRADO — fix `c8f3bd5`+`9d6f101`] Buffs numéricos

- CRITICAL_PCT/MOV_SPEED/ATT_SPEED alteran gameplay (no solo icono); melee crit ×2 + flag DAMAGE_CRITICAL del wire.

### C42. [CERRADO — fix `0d33d86`] Daño de skills con tabla aproximada

- skill_power real de PG (common.locale SKILL_POWER_BY_LEVEL + TYPE0..8): k = power(job, skillgroup, level) × max_level / 100 (parity GetSkillPowerByLevelFromType). Fail-open → aproximación previa.

### C43. [CERRADO — fix `3ea8f49`] GM commands faltantes (95% del cmd_info[])

- mob/kill/purge/goto/stat implementados con parity (GmSpawn intent nuevo; target_vid; FuncPurge radio/mapa; find_player; stat cap 90 + stat- floor). Gaps: mob por nombre, kill-vs-PC, goto coords.

### C44. [CERRADO — fix `6d26167`] Pociones de buff cosméticas + bolsas de oro no-op

- USE_ABILITY_UP (7): switch APPLY_*literal (MOV/ATT_SPEED con AFF_*_POTION, STR/DEX/CON/INT, CAST_SPEED, ATT/DEF_GRADE) → affects reales + sync al mundo (combat los lee). AUTOUSE_GOLD (3): oro al usarlo (cap 2e9). Wire subtipos = orden del enum (4/7/3, no 5/3/8). Gaps: treasure box llave+grupo, ITEM_ELK 50026, buffs post-relog.

### C45. [CERRADO — fix `841dd0a`] Party sin bonus de exp
- bonus por tamaño (tabla CHN [0,0,12,18,26,40,53,70,100], cap 8) + 5% party veterana (>60 min); aplicado al pool antes del reparto, solo si el kill cerca del líder. Gold: el C++ no reparte (drop al suelo del killer). Gaps: +30% item del líder, centralización 5%.
