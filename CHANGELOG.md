# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
The project uses semantic versioning ([SemVer](https://semver.org/spec/v2.0.0.html)) once releases exist; until then, entries are grouped by date.

> **Language note:** entries before the 2026-08-10 (4th part) docs reorganization were written in Spanish and are preserved verbatim (history is never rewritten) — this includes the 2026-08-10 1st–3rd parts and all earlier sessions. Only the 4th part and the new English documentation follow the "docs are written in English" rule (AGENTS.md).

## [2026-08-15] (50th part) — Mob combat parity: attack cooldown/speed/range/reposition (C29-C32)

> The 4 CRITICAL mob behaviors from the legacy analysis (mob-legacy-behavior.md) —
> "mobs don't feel like the original". All committed, workspace **652 passed / 0 failed**,
> deployed (binary 5,297,664 B, hash 357d8620; stack up auth :30001 / channel :30003).

- **C29 — Mob attack cooldown (CRITICAL)** (`10d5092`): the mob attacked EVERY AI tick
  (250 ms); the legacy hits every `CalculateDuration(POINT_ATT_SPEED, 2000)` ~= 2 s
  (char_state.cpp:1005-1012) — mobs hit ~8× faster. NEW `ai::mob_attack_cooldown_ms`
  (parity utils.cpp:201-210) + `LastAttack` component (last hit instant, updated only
  on a successful hit — parity `m_dwLastAttackTime`) + `attack_speed` column selected
  (mob_proto, position 18) + `b_attack_speed` in GC_CHARACTER_ADD wired (was hardcoded 0).
- **C30 — Mob speed is motion-based (CRITICAL)** (`9e55397`): the rewrite used the
  mob_proto `move_speed` column directly as u/s — the legacy derives speed from the RUN
  animation (~300 u/s) with the column as the POINT_MOV_SPEED factor
  (`GetMoveMotionSpeed × 10000/CalculateDuration(factor, 10000)`, char.cpp:2726-2754).
  NEW `ai::mob_move_speed` — factor 100 → 300 u/s; the 308 mobs with factor 0/1 now
  move at ~150 u/s instead of being FROZEN. Chase and patrol both use it.
- **C31 — Exact mob attack range (HIGH)** (`3e05d3c`): mob→PC range is ONLY
  `GetMobAttackRange() × 1.15` with NO 300 floor (battle.cpp:147-152); RANGE/MAGIC add
  POINT_BOW_DISTANCE (char.cpp:2010-2020). NEW `combat::mob_attack_max_range` — mobs
  with range < 261 no longer hit from 300; `melee_max_range` stays for the PC→mob path.
- **C32 — Change attack position (MEDIUM-HIGH)** (`9a0b618`): pursuing mobs converged
  and clumped — the legacy repositions each mob randomly around the victim every
  `AI_CHANGE_ATTACK_POISITION_TIME_NEAR` (10 s) near / `TIME_FAR` (1 s) far
  (char.cpp:5436-5462, 5869-5881). NEW `ai::change_attack_dest` (fMinDistance = range×
  0.9/0.8, angle = direction ± 2×number(-90,90) near / number(0,359) far) + `AttackPos`
  component + `Mob.rank` (MOB_RANK_BOSS = 4 excluded).
- **Full server-side gap analysis** (`server-side-gap-2026-08-15.md`): 2 exploration
  lanes (module surface + per-domain depth) — 146 C++ .cpp (~104.7k LOC) vs 46 Rust
  .rs (~23k) = 4.5× less code, ~40% coverage; ranked 28 missing behaviors (CRITICAL:
  refine, stat_points, party, PvP; HIGH: safebox, numeric buffs, skill families, gold
  pickup, exp level-delta, item attributes, mob AIFLAGs, death penalty). **NEXT block:**
  stat points → party → refine → gold pickup → numeric buffs → safebox → exp curve
  → AIFLAGs (BERSERK/COWARD/GODSPEED/STONESKIN).
- **Commit convention adopted** (`1d0f28c`): Conventional Commits in English as
  AGENTS.md rule 17 (atomicity + no rewriting published history).

## [2026-08-15] (49th part) — Base jugable fixes: revive city/boots/mobs + full legacy-mob analysis

> Sl session continuation (loop "Base jugable"). The 5 original plan bugs (drag-equip
> INVENTORY, GC_ITEM_DEL vnum=0, CG_SHOP header 50, player commands + revive, CG_TARGET
> 61→GC_TARGET 63) were fixed and verified in earlier loop iterations (see ASSUMPTIONS.md).
> This part: the 3 real-client-feel fixes (revive city, boots speed, mobs separation/
> respawn) + the complete C++ legacy-mob behavior analysis that explains "mobs don't
> work like the original".

### Fixes (all committed, all verified)

- **Revive city (C26)** — city-revive now restores HP/MP + teleports to the village
  (969600, 278400) instead of reviving in place; `RestartTown` parity. Verified by
  verifier (sólido). (`4f00706`)
- **Boots speed (C27)** — `APPLY_MOV_SPEED` (boots) now actually raises movement speed
  (was applied but not wired to the envelope); capped at 200 (`game_core/src/movement.rs`).
  Verified byte-exact against the C++ apply model. (`41a244f`)
- **Mobs separation (C28)** — SEP_MOBS distance (60 u) + flank ±90° landing, per-map
  snapshot (mobs on different maps never block each other), spawn jitter so copies
  don't stack. (`41a244f` + `261d0c7`)
- **Mobs respawn (C23)** — `time==0` entries never respawn (sentinel `u64::MAX`),
  top-up = `max_count - alive` on deadline (multi-copy entries respawn while sisters
  alive), despawn-by-distance no longer cancels pending respawns. (`41a244f` + `261d0c7`)
- **Verifier mobs v2: PASS** — 3 mutation tests (each fix reverted → its test fails),
  3 full `cargo test --workspace` runs (647 passed / 0 failed), scoping clean
  (3 files touched).
- **Redeploy**: release build + copy + `start_win.ps1` — server running (auth :30001 +
  channel :30003), hashes identical (0953602509f8809c06bf718c681fc4d7).

### Full legacy-mob analysis (new doc: `docs/plans/mob-legacy-behavior.md`, `841101a`)

Explorer lane captured the COMPLETE C++ mob AI (char_state.cpp, char_battle.cpp,
char.cpp, trigger.cpp, regen.cpp, length.h) with file:line evidence. Findings:

- **CRITICAL: mob attack cooldown** — legacy hits every `CalculateDuration(ATT_SPEED, 2000)`
  ≈ 2 s (char_state.cpp:1005-1012); the rewrite attacks every 250 ms tick → ~8× faster.
  (bug-registry C29)
- **CRITICAL: mob speed is motion-based** — legacy derives speed from the RUN animation
  (GetMoveMotionSpeed, char.cpp:2726-2749, ~300 u/s), not the mob_proto column; the
  rewrite treats the column as real u/s → mobs ~3× slower + wrong GC_MOVE duration
  ("mobs move weird"). (C30)
- **HIGH: attack range exact** — legacy chases at `range*1.15`, stops at `range*0.9/0.8`;
  rewrite uses `MAX(300, range*1.15)` only for MELEE → RANGE/MAGIC mobs attack at 300. (C31)
- **MEDIUM: change attack position** — legacy repositions mobs randomly around the
  victim every 10 s/1 s (char.cpp:5436-5462) — the real reason legacy mobs "don't clump";
  the rewrite's `separate_landing` is a patch. (C32)
- **The "knockdown" does NOT exist server-side** — `AIFLAG_FALL` (length.h:545) is dead
  code (0 uses in source/server); no stagger/interruption on damage. The hit reaction
  is 100% client-side animation — the server only needs correct FUNC_ATTACK
  dw_time/dwDuration sync.
- 21 missing behaviors total, prioritized (C29–C32 registered in bug-registry).

### Pending (next block, documented in bug-registry + next-block-plan)

- C29 cooldown de ataque, C30 velocidad motion, C31 rango exacto, C32 change attack
  position (mobs juntos). Then: berserk, aggro completo, leash, KILL_AND_GO, COWARD,
  STONESKIN, GODSPEED, interceptación, walkability, REVIVE party, mob skills.
- Committed the 44–46 backlog (pool/handshake/client C++ code, 59 files) that was
  documented but never committed — `d2c0831`. Workspace clean, 647 passed / 0 failed.

## [2026-08-15] (48th part) — Real-client session fixes + full gap analysis

> Sl session (continuation). Despliegue del wave de los 5 bugs (build release 04:10,
> auth.041013/channel.041013) + sesión de juego del usuario que reveló 3 problemas
> reales. Análisis completo de brechas vs el legacy (docs/plans/gap-analysis-2026-08-15.md).

### Fixes de la sesión real del cliente

- **Doble-click en item equipado → desequipa (toggle parity)**: `handle_use` solo
  buscaba en INVENTORY — el doble-click en un item EQUIPADO daba "uso de celda 182 sin
  item". Fix: buscar en INVENTORY o EQUIPMENT + toggle (parity UseItemEx
  char_item.cpp:1874-1938 — equipado → UnequipItem). (items.rs, `46517a8`)
- **Diag de tiendas**: el silencio parity impedía saber qué NPC fallaba al click. Se
  añade log con el vnum (`world: shop Open vid X — NPC vnum Y SIN shop`). (social.rs,
  `46517a8`)
- **Tick del mundo 500→250 ms**: los mobs se veían "a saltos rápidos" (pasos de
  speed×0.5s con pausas entre ellos; el C++ mueve cada ~100ms). Con 250ms los pasos son
  speed×0.25s — 2× más suave, misma velocidad (step_toward usa tick.dt_ms). (mod.rs +
  world.rs, `58cad3a`)

### Despliegue

- Build release 04:09, deploy 04:10 (binario 5,045,760 B, auth.041013/channel.041013),
  boot limpio (0 panics).

### Hallazgos del gap analysis (documento nuevo)

- **Comandos**: 174 en el C++ vs 9 en el Rust (5%). Los botones del cliente mandan
  comandos por el chat (PythonNetworkStream.cpp:203-240).
- **Headers de juego**: 27 en el framer, 16 con dispatch, 11 ignorados en silencio
  (CG_ITEM_DROP, CG_QUICKSLOT_*, CG_WHISPER, CG_PVP, CG_MYSHOP...).
- **Tiendas — BUG DE DATOS**: los shops 1-8 legacy apuntaban a npc_vnum 9001-9009; el
  wave 45-46 re-asignó 1-3 a 20002/20006/20023 y los vnums legacy se perdieron. El NPC
  9003 del pueblo (con shop en el legacy) ya no tiene. Fix propuesto: re-asignar
  npc_vnum (requiere confirmación del usuario — no game-data edits sin OK).
- **Cobertura global ~35-40%** (no 90%): party/guild/safebox/messenger/PvP/refinar/
  data-channel en 0%.

### Commits

- `46517a8` fix(items+shop): doble-click en equipado desequipa (toggle) + diag shop
- `58cad3a` fix(movement): tick del mundo 500->250ms — mobs a saltos rapidos

## [2026-08-15] (49th part) — Gap loop: 6 lanes paralelos + verifier adversarial + oracle director

> Sl session multi-agente (pi-subagents, background). Los 6 lanes del gap analysis
> se implementaron EN PARALELO (2 concurrentes), cada uno con un VERIFICADOR
> ADVERSARIO (que intenta probar que está malo) y reparación de hallazgos. Todos los
> verifiers dieron FAIL con bugs reales — todos reparados. El oracle director hizo la
> revision global final. Workspace **634 passed / 0 failed**.

### Lanes (6, archivos disjuntos)

- **A — Framer 21 headers CG + GC centralizado** (`e6ca209`): 21 headers C->S añadidos
  con tamaños exactos (packet_info.cpp); GC_SHOP=38/GC_EXCHANGE=42/GC_AFFECT_ADD=126
  centralizados en protocol::header. Verifier: FAIL (entry.rs:457 no compilaba con la
  firma nueva de points_packet — ya resuelto en `3bcaf26`).
- **B — Comandos GM_PLAYER** (`5e20dcf` + `c24fe70`): 30 comandos parseados
  (safebox/mount/party/pvp/emociones/walk/skillup); set_walk_mode + skillup reales con
  persistencia. Verifier: FAIL -> 2 fixes parity: `/skillup 0` no-op (char_skill.cpp:3572)
  - bMasterType escrito (char_skill.cpp:207-217, thresholds 20/30/40).
- **C — Chat broadcast + whisper** (`088c8c1` + `70379f1`): broadcast a todo el mapa
  (rango view 5500), SHOUT canal completo id=0 + cooldown 15s, whisper case-insensitive,
  payload "Name : msg". Verifier: FAIL -> 6 fixes: **NUL de cola rompia los comandos '/'
  (CRITICO — "No such command" para todo)**, caps de longitud (DoS del cliente legacy),
  rango TALKING, SHOUT, prefijo, case.
- **D — Dispatch gameplay** (`2064c65` + `18dc92f`): drop item/oro, quickslots
  persistentes (bytea 72B), script/quest intents, PvP, Standup/Sitdown, peso.
  Verifier: FAIL -> 3 fixes wire: header GC_CHARACTER_POSITION 28->43 (desync),
  posturas 3/4 (length.h), quickslot ITEM=1/SKILL=2/COMMAND=3.
- **E — Crear/borrar personaje + empire + rename** (`3bcaf26` + `073ee93`):
  CG_CHARACTER_CREATE/DELETE/EMPIRE/CHANGE_NAME con parity completa (JobInitialPoints,
  social_id últimos-7, UNITS 969600/278400). Verifier: FAIL -> 2 fixes: CG_EMPIRE(90)/
  CG_CHANGE_NAME(106) al framer (el rename desconectaba), rollback del create sin gate.
- **F — Tiendas datos PG** (SQL + `83f2481`): 17 filas restauradas al legacy EXACTO
  (9001-9009) + los 3 vendedores del pueblo (20002/20006/20023) reasignados a las filas
  all_* libres. Verifier: FAIL (regresion: los vendedores del pueblo quedaron sin shop).
  Cobertura final mapa c1: 13 vendedores con shop. Backups: shops_backup_antes.sql +
  shops_backup_fix2.csv.

### Oracle director (revision global) — FAIL -> 3 fixes

- realm_pg.rs: nombre de 25 chars > varchar(24) -> 'e2e_rok_' (18 chars).
- player.rs delete(): DELETE de quest/affect usaba dw_pid pero el esquema es dwPID
  (sqlstate 42703) — bug real del borrado de personaje (realm_pg 6/6 con PG real).
- pool.rs (untracked) commiteado — los tests PG lo requieren (HEAD autocontenido).

### Resultado

- Workspace **634 passed / 0 failed** (33 bins).
- **10+ commits** de la fase (6 lanes + 6 reparaciones + 1 fix oracle).
- El ciclo completo: 6 verifiers -> 6 FAIL con bugs reales -> todos reparados con parity
  del C++ verificada. El diseño adversarial funciono como se pidio.

### Pendiente del operador

- Reiniciar el canal (redeploy del binario) para que load_shops recargue la ShopTable
  con los vendedores 20002/20006/20023 (cache unica al boot, mod.rs:226).
- El WIP de otro proceso (client-rewrite/movement, ~1185 inserciones) queda en el arbol.

## [2026-08-15] (47th part) — "Base jugable": 5 bugs gameplay fixeados (equip drag, GC_ITEM_DEL, CG_SHOP framer, comandos GM_PLAYER, barra de vida del mob)

> Sl session (pi-loop-mode `/loop`). Los 5 bugs del plan "Base jugable" con causa raíz confirmada se arreglaron, cada uno con tests + `cargo test --workspace` + commit. El servidor C++ quedó FROZEN (solo parity, nunca se reconstruyó). Workspace **584 passed / 0 failed** (+20 desde los 564 del último wave).

### Bug 1 — Equipar por arrastre (drag) rechazado

- El drag-equip del cliente llega como `CG_ITEM_MOVE` con `window=INVENTORY` y cell destino `180+wear`; el handler solo reconocía `EQUIPMENT` → el drag se rechazaba en silencio (solo funcionaba el doble-click).
- **FIX**: `is_equip_position()` replicando `SItemPos::IsEquipPosition` (length.h:825-830): window ∈ {INVENTORY, EQUIPMENT} Y cell ∈ [180, 212). Extraída a nivel de módulo + test. (items.rs)

### Bug 2 — Items duplicados en el inventario (GC_ITEM_DEL con vnum real)

- `GC_ITEM_DEL` (20) se mandaba con el vnum real en los 8 call sites (use/equip/desequip/stack/move + shop venta + trade entrega + quest remover); el cliente `RecvItemSetPacket` interpreta vnum≠0 como "set" y PINTA el item en vez de borrarlo → duplicados.
- **FIX**: vnum=0 y count=0 en todos los call sites (parity char_item.cpp:433-443). (items/shop/trade/quest.rs)

### Bug 3 — Tiendas NPC: cierre de conexión en el primer BUY/SELL/END

- El header `CG_SHOP` (50) no estaba en la tabla del framer → UnknownHeader → cierre. `TPacketCGShop` (2 B) es de tamaño VARIABLE según subheader (END 2, BUY 4, SELL 3, SELL2 4) — el framer lo resuelve por subheader como CG_CHAT (parity input_main.cpp:1054-1088 + iExtraLen).
- **FIX**: `header::CG_SHOP = 50` + caso variable en el framer + test (fragmentado/concatenado/rol Auth rechaza). (protocol/lib.rs, network/framer.rs)

### Bug 4 — Comandos del diálogo de muerte y del menú → "No such command"

- `/restart_here`, `/restart_town` (uirestart.py) y `/logout`, `/phase_select`, `/quit` (menú) se parseaban como desconocidos (solo existían warp/item/notice/level).
- **FIX**: variantes GM_PLAYER (nivel 0 — SIN gmlist, parity cmd.cpp:340-347) en `GmCommand` + parseo (argumentos extra ignorados, parity do_cmd). `handle_player_command`: revive vía `script::revive` extraída (RestartHere=mismo punto, RestartTown=ciudad vía GC_WARP), Logout/Quit=cierre de conexión, PhaseSelect=GC_PHASE(SELECT)+cierre. (game_core/gm.rs, channel/gm.rs, channel/script.rs)

### Bug 5 — Sin barra de vida del mob

- El cliente dibuja la barra vía `GC_TARGET` (63); el dispatch no tenía arm del `CG_TARGET` (61) y el daño al mob no se difundía (solo GC_DAMAGE_INFO — número sin barra).
- **FIX**: `TPacketGCTarget` (6 B: header+vid+bHPPercent) + `CombatIntent::Target` → `process_target` (npc_view) → `TargetResult` → GC_TARGET; y en `AttackResult` (KillInfo ya traía hp/max_hp) se emite GC_TARGET con el pct nuevo tras el golpe (broadcast parity BroadcastTargetPacket, char.cpp:5048-5143). (protocol/world.rs, ecs/events.rs, ecs/world.rs, ecs/systems/combat.rs, channel/events.rs, channel/game.rs)

### Commits (5, uno por bug)

- `a761b79` fix(items): equipar por arrastre acepta INVENTORY con cell 180+wear
- `11c6093` fix(items): GC_ITEM_DEL con vnum=0 en los 8 call sites
- `0e3e52a` fix(shop): CG_SHOP (50) en el framer — tamaño variable por subheader
- `8fb962c` fix(commands): comandos GM_PLAYER /restart_here /restart_town /logout /phase_select /quit
- `8d7c59c` fix(combat): barra de vida del mob — CG_TARGET(61)->GC_TARGET(63) + broadcast

### Siguiente (fuera del loop actual)

- Redeploy del binario a `source\deploy\win` (los 5 fixes están en source, el runtime aún corre el binario del wave 46) + verificación con cliente real.
- Pendiente para el 100%: party/guild, safebox+messenger, SPLASH/PARTY/HORSE, refine/blend/cube/DS/belt, quests restantes, GM expandido, PvP/eventos/raids, data channel F3.

## [2026-08-14] (46th part) — Wave 46 (dawn): quest-boot A/B fix (background corpus), handshake leniency, close panic fix, channel.inf, shop/quest diagnosis

> Dawn session (orchestrator + coder lane). All verified with real-client sessions and real outputs. Deploys: 01:50 `941F1B54` (boot async), 02:05 `2E9CC162` (leniency), **02:11 `015FB634` (panic fix) — current stack auth.021225/channel.021225**.

### Root cause of the post-quests "no connect" ISOLATED with A/B

- The quest wiring loaded the corpus (194 files, ~2 s) **synchronously before the accept loop** → the client connected into the kernel backlog, exceeded its timeout and closed. A/B: with `quest_path` empty the client connects; with the synchronous corpus load it does not.
- **FIX: corpus load in BACKGROUND** (`tokio::spawn`, channel/mod.rs:132-147) — accept starts instantly; the quests arrive ~2 s later (fail-open without them).

### Handshake leniency

- After 32 retries (~17.6 s) the channel NO LONGER closes — it sends `GC_PHASE(LOGIN)` and waits for the LOGIN3 up to **45 s** (slow client — its phase machine prepares late). Test: `channel_slow_client_login3_after_retries_exhausted` (channel_smoke.rs:174).

### Close panic fix

- `session.rs save()` accessed `motion()` (expect) on connections closed BEFORE ENTERGAME → worker panic ("motion: seteado en la fase select"). **FIX**: position sync only when the motion is set (session.rs:449-457).

### channel.inf (client)

- The saved channel index "1" (from the 4-channel era) vs the current 1-channel dict → KeyError → no channel connection; **corrected to 0**. Finding documented: the client UI needs a fallback (pack).

### Shops + quest texts — data linked, runtime IN DIAGNOSIS

- Shop data: `player.shop` 1–6 → visible NPCs (20002/20006/20023/20025 blacksmiths, 20029/20030 ponies), 9 → 20042 (Vendedor Ambulante), 1002 → 20341 (blacksmith), 1003 → 20343 (archer). **Shop opening: IN DIAGNOSIS (coder lane)**.
- Quest dialog: the engine sends raw keys ("gameforge.map_warp._20_sayTitle") — **text resolution: IN DIAGNOSIS (coder lane)**.

### Verified live (stack 02:11)

- Stable entry (the 3 handshake fixes), position persisted (the player crossed the map and reconnected at his position), movements accepted (331 lines — map crossing), quests 194 loaded in background, **0 panics** after the fix.
- Workspace **573 tests** (with the leniency test) → **573 after the panic fix** (616 test attributes).

### Ops

- dot-source (`. scripts\start_win.ps1`) — no nested powershell — stops/starts return instantly (previously hung the tool 120 s). `-HsDebug` switch added to start_win.ps1.

### Pending

- Shop opening (coder, in diagnosis); quest text resolution (coder, in diagnosis); channel.inf client fallback (pack); skills GAPs (SPLASH/PARTY/HORSE, buffs); GM mob spawn; quest converter completion (~88); ladder 250/500/1000; per-tick CPU; ECS parallelism decision (ADR-0010 §1).

## [2026-08-13] (45th part) — Wave 45 (night): real-client session COMPLETE — subtype/walkability/envelope fixes + content gaps

> Night session (orchestrator direct + coder lane). All verified with real-client sessions and real outputs. **Deploys: 19:38 `1B4420E2` (subtype), 20:38 `271EDEBE` (walkability), 21:40 `19DD1724` (envelope client-Δt), 22:06 `2A71E57E` (tuning) — current stack auth.220608/channel.220608.**

### Real-client session now COMPLETE (entry → world → cross-map movement → dynamic spawns)

- **`subtype` column fix** — `database/src/item.rs:179`: the item query used the legacy `sub_type` vs PG `subtype` → 42703 ejected players entering with equipped items. Deploy 19:38 (`1B4420E2…`).
- **Walkability re-scope** — the `server_attr` parse was CORRECT; the village source cells are legitimate ATTR_BLOCK, so the pre-move gate froze the player. The gate moved to **anti-teleport reinforcement**: normal steps accepted, jumps onto blocked terrain rejected (channel movement). Deploy 20:38 (`271EDEBE…`).
- **Envelope fix (client Δt)** — server-only Δt rejected legitimate walking: the client sends MOVEs in bursts → **Δt = max(client clock, server clock)** (movement.rs). Plus tuning: tolerance 1.2→**1.5** (`ENVELOPE_TOLERANCE` movement.rs:95), lag 100→**250 ms** (`ENVELOPE_LAG_MS` movement.rs:96-97) — the real client pattern fits with margin; **anti-cheat intact** (2500 cap + C++ timers; sustained speedhack >450 u/s still bounded). Deploys 21:40 (`19DD1724…`) + 22:06 (**`2A71E57E…`** — current).
- **Verified live**: the player enters, CROSSES the map (~9k units), mobs/NPCs materialize around (dynamic spawn with the real client: tanacas in the village, Uriel/Capitán at the second plaza). Wave-44 benchmarks re-confirmed in vivo (20 bots: login_ms median 797 vs 4,894, sel 203 vs 2,905, world_ms 1,052 vs 7,829, spawns 11/bot on ALL; 100 bots: 100/100 OK, world_ms 2,742, tick <1 ms — `logs/bench-run-2026-08-13-wave44.md`).

### Spawns restored to the ORIGINAL data (user directive)

- The village population (dogs/wolves etc. inserted in `world.spawns`) was **REVERTED** — the village area is identical again (5001×1 + 5004×10 + 20354).
- **NEW RULE: no game-data modifications without explicit user confirmation.**

### Content findings (real gaps, NOT regressions)

- **Shops inaccessible in practice**: `player.shop`/`player.shop_item` exist and the wave-7 shop code works, but the vendors (npc_vnum 9001–9009) are on the map with `folder=''` (INVISIBLE client-side — only 9006/9008 blacksmiths visible) and the visible city NPCs (20000 series) have no shop linked. Fix future: link shops to visible NPCs (20002/20006/20023/20025) or add models.
- **Quests not wired**: the quest corpus (qc → DSL) is NOT hooked server-side (converter IN PROGRESS — roadmap) → NPCs have no quests.
- **Intermittent silent handshake**: the client sends LOGIN3 before the echo; the server discarded it — **FIX IN PROGRESS (coder lane: `HandshakeError::LoginEarly`)**; `MT2_HS_DEBUG=1` instrumentation added in `network/src/handshake.rs` (env-gated, logs handshake packets, :306-309).
- Workspace **566 passed** (2026-08-13 night; 609 test attributes).

### Night follow-up (deploys 23:31 `FE6BDE06` + 00:15 `8C335FAA` — current) — quests wired, shops linked, vision/speed/mob tuning

- **Spawns — tanacas removed (user directive)**: the village tanacas (5001/5004 at 969600/278400 — "they're annoying, shouldn't appear in city 1") deleted from `world.spawns`; the village area now holds only 20354 (psql-verified). Confirms the wave-45 rule: no game-data edits without explicit user confirmation.
- **Vision enlarged (user: "make it even bigger")**: `SPAWN_VIEW` 5000→**8000**, `DESPAWN_RADIUS` 7000→**10000** (`game_core/src/ecs/systems/spawn.rs:24,28`); tests adjusted (spawn.rs:212).
- **Movement**: `DEFAULT_MOVE_SPEED` 300→**500** (envelope base — the client's run; 1.8× margin = 900 u/s), `MAX_DIST_NO_RIDING` 4000→**6000** (`game_core/src/movement.rs:81,94`).
- **Shops linked to VISIBLE NPCs**: `player.shop` 1–6 → **20002/20006/20023/20025/20029/20030** (the 9001–9009 vendors are invisible `folder=''` in the pack); 7/8 (gold_bar/firework) left unlinked (still point at invisible 9005/9004). Data-only — the wave-7 shop code already existed.
- **Quests WIRED (coder lane)**: the corpus loads at channel boot (default `germany/quest` — the runtime's `spain/quest` is empty) via `quest_dsl::convert::convert_corpus` (channel/mod.rs:403-416); **194/194 convert, 106 with Chat triggers (346 triggers)**; `QuestIntent::NpcClick` (channel/shop.rs:67 — CG_ON_CLICK) → `QuestTrigger::Chat(vnum)` → GC_SCRIPT dialog; `quest_path` in config.rs (:59,109,170); `QuestRepo::load` + `QuestIntent::Init` in entry.rs. Tests: `npc_click_offers_quest_dialog`, `quest_corpus_loads_into_engine`. Workspace **572 passed / 0 failed** (615 test attributes).
- **Mob movement duration**: `move_duration_ms(dx, dy, move_speed)` (`game_core/src/ai.rs:41` — parity `CHARACTER::CalculateMoveDuration` char.cpp:2765-2768; C++ truncation `(int) fDist/motionSpeed×1000`, tests :298-302); mobs animate with real duration (was fixed 500 ms/tick → "they move too fast").
- **Ops**: the nested `powershell -File scripts\start_win.ps1` pattern hung the tool up to 120 s → replaced with direct dot-source (`. scripts\start_win.ps1`) — no nesting, no waits.
- **Deploys**: 23:31 (vision 8000 / speed 500 — SHA256 `FE6BDE0637615EFDA5F395B6DA32FBEB1E727DC5CB408E95E42A87610C8EB435`), 00:15 of the 14th (quests + shops + mob speed — SHA256 `8C335FAACDF591C65F0FA667719CA958DB715924A7228113ED19D3650C30E806`, 4,941,312 B — channel 001531, current).

### Pending

- Handshake `LoginEarly` fix (in progress); quest converter completion for the remaining ~88 quests without Chat dialogs + `pc.setqf` unblocking; shops 7/8 relink or models; user validation of quests/shops/mobs; skills GAPs (SPLASH/PARTY/HORSE, buff numeric application); GM mob spawn; ladder 250/500/1000; per-tick CPU; ECS parallelism decision (ADR-0010 §1).

## [2026-08-13] (44th part) — Wave 44: connection pool + shared Batcher + metrics + benchmark ladder step 1

> Pool lane (coder) + deploy + bench runs (orchestrator). All verified by the orchestrator with real outputs. **Deploy DONE: the waves-6/7 + pool binary is live since 18:01:39** (the "redeploy pending" state of parts 42–43 is closed).

### Connection pool EXECUTED (ADR-0008 clause)

- **`deadpool-postgres 0.14.1`** added (pairing verified: depends on tokio-postgres 0.7.9 = the same workspace driver — no driver change, exactly what ADR-0008 Decision 3 promised).
- **NEW `database/src/pool.rs`** (`PgPool`); ~13 repos of `database/src` moved from per-call `pg_conn` + `connect()` to `pool.get()`.
- **`PgMutationSink::new(pool)`** (wal.rs) — no more reconnect-per-batch; **`WorldStore::new(pool, Arc<Batcher>)`** (game_core/src/world.rs; channel/entry.rs:153) — **ONE `Batcher` per channel** (Arc, flush 100 ms), was one per player.
- `config.rs pool_max_size` default 10 (config.rs:110); direct SQL in `channel/shop.rs` absorbed by `ItemRepo::load_sell_proto` (ADR-0008 §2 "no direct-sql backend" restored — shop.rs:407-411).
- `WorldMetrics.last_tick_ms` + `record_metrics` → `tick_ms.csv` via bench_capture (bench_capture.rs:155-170); `replay_wal` fail-open at boot (documented decision).
- **Verified**: workspace **565 passed / 0 failed / 35 ignored**; clippy identical to baseline; release built 17:57.

### Deploy DONE + verified (18:01:39)

- Binary **4,509,184 B**, SHA256 `77D8ACD292EA52A4D4396E04E1DF07F3065976D47C015D61B50E87E16323C732`; stack up 18:01:39 (auth.180138/channel.180138 logs).
- `start_win.ps1` gains **`-BenchCapture <rel>`** (passes `--bench-capture` to the channel; daily behavior unchanged without the flag).

### Benchmark — spawn-concurrency fix confirmed + ladder step 1 (report `logs/bench-run-2026-08-13-wave44.md`)

- **20 bots** (vs the 06:11 baseline in parentheses): 20/20 OK; channel_login_ms median **797** (4,894); sel_ms **203** (2,905); world_ms **1,052** (7,829); **spawns 11/11 on ALL 20 bots** (baseline 19/20 with 0); 0 panics — oracle criteria PASS. The 41st-part spawn-concurrency finding is **RESOLVED**.
- **100 bots (ladder step 1)**: **100/100 OK**; auth 857 / sel 1,008 / world_ms **2,742** medians; **spawns 1,100 = 11/bot**; AI-tick window <1 ms (0 at CSV integer resolution) — <500 ms criterion PASS with huge margin; 0 panics (10 × 10053 teardown expected); `--cleanup-accounts` → 0.

### Pending

- Ladder steps 250/500/1000 bots; per-tick CPU measurement; real-client smoke pending; ECS parallelism decision open (ADR-0010 §1).
- Unchanged chain: quest pending actions, auction/unified trade, GM extensions, data channel 162/163, locale wire slice, manifest/hot reload, auto-ban.

## [2026-08-13] (43rd part) — Wave 7: NPC shops + player trade · quest runtime engine · GM commands · dw_arrow (quiver gate)

> Wave 7 complete (social lane cod-3, cod-1, fix-4, orchestrator direct). All verified by the orchestrator with real outputs. **Deploy note: the servers still run the pre-wave-7 binary (3,935,232 B, 07:23 build) — redeploy pending the orchestrator.**

### NPC shops + player trade DONE (social lane, cod-3)

- **`game_core/src/shop.rs` (406 lines, pure)**: buy price = `item_proto.gold × count` (parity `shop.cpp:166-180`); sell = `shop_buy_price × count / 5 − 3% tax` (parity `shop_manager.cpp:297-319`); rejects SoldOut / NotEnoughMoney / InventoryFull / NotSellable / Equipped / GoldOverflow.
- **`game_core/src/trade.rs` (370 lines, pure)**: `TradeSession` state machine — 12-item cap, gold once, 2-phase accept, **commit via `ItemExchange::exchange_mutated` + `Batcher::flush()` — ONE tx per unit, new ids 100M–200M, dupe-protected**.
- **`ecs/systems/social.rs`**: `ShopTable` resource + `handle_social` arms; **`channel/shop.rs` + `channel/trade.rs`** wire byte-exact (GC_SHOP START 1888 B, GC_EXCHANGE 47 B, CG_SHOP 50 B, CG_EXCHANGE 27 B — Packet.h sizes; cheque field 0 for NPC shops). `WorldStore` gained the `exchange(&ItemExchange)` facade.
- Workspace **548 passed / 0 failed** at that point.

### Quest runtime engine DONE (cod-1)

- **`game_core/src/quest/engine.rs` (859 lines)**: state machine (flag `{quest}.__status` 1-based — parity `questpc.cpp:115-118`; 0 = not started → start-state events start it); **`wait()`/`select()` suspension scheduler** (event SUSPENDS, saved path, client answers `CG_SCRIPT_ANSWER` → re-enter); conditions (pc.level/count_item/get_qf/number/get_time/get_map_index/get_gm_level/pet.is_summon/is_test_server + arith/compare/between); actions (say/say_title/wait/select/set_state/set_qf/give_item2/remove_item/warp/notice/return — **pending: say_reward, send_letter, set_quest_state, target_vid, affect_*, input_number**).
- Persistence via `player.quest` (`QuestRepo`; value 0 = DELETE). Wire: `GC_SCRIPT` (45) 6 B header + markup [ENTER]/[NEXT]/[QUESTION], `CG_SCRIPT_ANSWER` (29, 2 B). `QuestIntent {Load, Init, Event, Answer}` + `QuestEvent::Run`.
- Workspace **564 passed / 0 failed** at that point.

### GM commands DONE (fix-4, subset)

- **`game_core/src/gm.rs` (221 lines) + `channel/gm.rs`**: chat `/` prefix → `interpret_command` parity (`input_main.cpp:661-665`); permissions re-checked in DB per command (`common.gmlist`, `gm.cpp:50-105` isGM parity, ADR-0011); subset: warp/item/notice/level (**mob spawn deferred** — needs a new intent); unknown/rejected → EN message (locale pending, documented divergence). Wired in `channel/mod.rs` + `chat.rs`.

### dw_arrow DONE (orchestrator direct)

- **Arrow gate in `channel/skills.rs`**: `USE_ARROW_DAMAGE` skills require equipped arrows ≥1 (parity `GetArrowAndBow` `char_battle.cpp:2919-2941`).
- `equipped_arrow_index` + `consume_arrow` (parity `UseArrow` `char_battle.cpp:2770-2789`; count-1, `GC_ITEM_UPDATE` 38 B) in `channel/mod.rs`; `pending_arrow_shot` flag in `session.rs` (:237,280) consumed at SkillResult (events.rs:360-370).
- **`dw_arrow` field of `GC_CHARACTER_ADDITIONAL_INFO` now carries the real equipped arrow count** (was hardcoded 0; client `ENABLE_QUIVER_SYSTEM` `Packet.h:1229`) — `game_core/src/packets.rs` `character_additional_info_with_parts(row, empire, parts, arrow_count)` (:311-333) + channel call sites (items.rs ×2 equip/unequip, script.rs revive, entry.rs enter_packets).
- Verified: **cargo check 4.52 s; game_core 154 + server_realms 42 passed, 0 failed**.

### Ops: background subagents ENABLED (user-facing)

- `OPENCODE_EXPERIMENTAL_BACKGROUND_SUBAGENTS=true` added to `~/.config/opencode/opencode.jsonc` (verified) — tasks now run in background; the conversation never blocks while an agent works (root fix for the recurring "agent stuck / chat frozen" problem). **The user must restart opencode for it to take effect.**

### Evidence

- My attribute scan 2026-08-13 (post-wave-7): protocol 81, network 28, database 96, game_core **161**, server_realms **55**, mysql_proxy 67, locale_import 19, bench_bot 34, quest_dsl 66 = **607 attributes**; orchestrator runs: 548 (shops/trade) → 564 (quest engine) → after dw_arrow: game_core 154 + server_realms 42 passed 0 failed (cargo check 4.52 s).
- File:line: `game_core/src/{shop,trade,gm}.rs` (406/370/221), `game_core/src/quest/engine.rs` (859), `channel/{shop,trade,gm}.rs`, `packets.rs:311-333`, `session.rs:237,280`, `skills.rs` (arrow gate), `opencode.jsonc` (background flag).

### Pending

- **Redeploy** the wave-7 binary (servers still on the 07:23 build).
- Quest pending actions (say_reward, send_letter, set_quest_state, target_vid, affect_*, input_number); GM mob spawn (new intent) + GM locale; auction/unified trade.
- Benchmark full ladder (WorldSim::metrics, sharded-region, 100→1000 bots); spawn-concurrency fix; family extraction; skill GAPs (SPLASH/PARTY/HORSE, `skill_power.txt`, buff numeric application); data channel 162/163; locale wire slice; manifest/hot reload; auto-ban.

### Docs audit (librarian, 2026-08-13)

- `docs/CURRENT.md` refreshed: snapshot date/commit `9f2e82e` (HEAD = origin/main, 0 ahead/behind — 37th–43rd parts pushed), waves 6–7 (42nd/43rd parts) added to the snapshot, deploy of the 42nd+43rd-part binary noted IN PROGRESS (3,964,928 B staged, restart ~11:37, verification pending — no asserted deployment), ECS executor correction, stale `ecs.rs:449`/`ecs.rs:337-340` citations → `ecs/systems/spawn.rs:22,26` / `ecs/events.rs:303-307`.
- `docs/decisions/0008-data-layer.md`: the pool deferral (Decision 3 clause) marked EXECUTING 2026-08-13 — deadpool-postgres, channel-level `PgPool`, one `Batcher` per channel (trigger: the ADR's own "If a pool is later measured to be needed..." clause; code lane in flight — not yet in Cargo.lock at audit time).
- `docs/decisions/0010-domain-boundaries-and-data-ownership.md`: `multi_threaded` "one-line toggle" claim corrected — verified against bevy_ecs 0.19.1 source: `["std"]` only (Cargo.toml:18, no `multi_threaded` feature) → `Schedule::default()` = `SingleThreadedExecutor` (bevy_ecs `schedule.rs:410`, `executor/mod.rs:49-64`); 5 systems `.chain()`ed (game_core/src/ecs/world.rs:83-93); parallel ECS = pending decision; "World per-connection" deviation updated to the channel-level shared World (spawn dinámico, `Intent::Join` mpsc — ecs/events.rs:303-307, channel/mod.rs:97).
- `ROADMAP.md`: the two "redeploy pending" notes (waves 6+7, parts 42+43) consolidated into one; NEXT lines updated (redeploy → pool → benchmark ladder).
- Deploy-state observation (read-only, not asserted as verified): `deploy\win\server_realms.exe` = 3,964,928 B (08:01 build); processes restarted 11:37:52 from `deploy\win` (ports 30001/30003 listening, PG Running); an 08:02 session with the new binary logged in `logs/chan8.err.log` (login → world entry → moves).

## [2026-08-13] (42nd part) — Structural refactor: `realm` → `game_core` rename + channel/ecs splits + PG migrations + quest similarity engine

> Wave 6 (rename lane). User decision: the crate `realm` is renamed `game_core`. Workspace 512 passed / 0 failed, clippy no new, release green. **Deploy note: the servers still run the pre-refactor binary (3,935,232 B from 07:23, auth8/chan8) — redeploy pending the orchestrator.**

### `realm` → `game_core` crate rename (user decision)

- Directory `source/reforge/realm/` → `source/reforge/game_core/`; **78 code refs updated** (Cargo.toml members, imports, `-p realm` invocations, docs paths). `server_realms`, `quest_dsl`, `bench_bot`, `mysql_proxy`, `locale_import` crate names unchanged.
- Workspace **512 passed / 0 failed** (attribute scan 2026-08-13: protocol 81, network 28, database 95, game_core 115, server_realms 50, mysql_proxy 67, locale_import 19, bench_bot 34, quest_dsl 66 = **555 attributes**); clippy no new; release green.
- **N1 trap guard (oracle)** landed with the rename: `game_core/src/ecs/systems/{social,quest}.rs` + `server_realms/src/channel/{social,quest}.rs` — empty-sub-enum delegates, so the first social/quest lane gets compiler-enforced wiring (no silent no-ops).

### Channel + ecs structural splits

- **channel.rs split** → `server_realms/src/channel/` (13 files incl. `mod.rs`: chat, combat, entry, events, game, items, movement, quest, script, session, skills, social) with `Session` struct + `Outcome` + dispatch one-liners; the rename lane reported the split as 2,565 → 11 files.
- **ecs.rs split** → `game_core/src/ecs/` (components.rs, events.rs, resources.rs, world.rs, test_util.rs + `systems/` 8 files, incl. the N1 guards) with `Intent`/`NpcEvent` wrapper sub-enums; reported 2,694 → 12 files.
- **LoginGuard lifetime bug found during extraction** (fixed in the split — the guard was held across an await).

### PG migrations (scripts/gpg/)

- **`alter_gold_check.sql`**: `CHECK (gold >= 0)` added on the 3 wallet tables — `money_log` deliberately excluded (append-only, negative deltas legit).
- **`migrate_guild_tables.sql`**: `player.guild_member`/`grade`/`comment` migrated to PG (schema-only) — closes the documented F3-tail gap (41st part).

### Quest similarity engine (spec §9.3)

- `QuestSimilarity` + group detection in `quest_dsl` (family proposals automated beyond the 6 manual ones); **quest_dsl 66 tests** (was 44).

### Evidence

- Workspace **512 passed / 0 failed** (orchestrator run); my attribute scan: 555 attributes (counts per crate above).
- File:line: `source/reforge/game_core/` (realm dir gone — verified), `game_core/src/ecs/systems/{social,quest}.rs`, `server_realms/src/channel/{social,quest}.rs`, `scripts/gpg/{alter_gold_check,migrate_guild_tables}.sql`.

### Pending

- **Redeploy** the refactored binary (servers still on the 07:23 build).
- Ongoing lanes: spawn-concurrency fix, family parameter extraction, benchmark ladder; skill GAPs (SPLASH/PARTY/HORSE, `skill_power.txt`, buff numeric application).

## [2026-08-13] (41st part) — Skills + server-timed buffs live · qc→DSL converter 194/194 · benchmark FIRST LOAD SIGNAL

> Wave 3 of 2026-08-13 (three lanes active: channel.rs/skills-fix, quest_dsl, bench_bot). All verified by the orchestrator with real outputs; skills binary deployed and live.

### Skills + server-timed buffs DONE (40th-part wave, completed 41st)

- **NEW `realm/src/skill.rs` (732 lines)**: `SkillRepo` reading `player.skill_proto` from PG, poly evaluator, `skill_damage` full chain — **15 tests**.
- **ecs.rs**: components `Mp` (:190), `SkillLevels` (:198), `SkillCooldowns` (:203), `Affects` (:209) + `SkillTable` resource (:377) + `process_skill` + `affects_system` (:870) — **19 tests**.
- **channel.rs**: `CG_USE_SKILL` handler (52, 9 B — client `Packet.h:854`; channel.rs:800-810); **`GC_AFFECT_ADD` (126)** emitted (channel.rs:1958-1963), **`GC_AFFECT_REMOVE` (127)** (channel.rs:2002).
- **Verified**: workspace **481 passed / 0 failed** (realm 106 per run); clippy no new; **deployed 07:23 (binary 3,935,232 B — verified on disk), live (auth8/chan8 logs 07:26)**.
- **GAPs documented** (pending, not hidden): SPLASH/PARTY/HORSE skill families; `skill_power.txt` table (k = level×max/100 subset); quest-granted/passive skills; MOV_SPEED/ATT_SPEED/CRITICAL buffs stored+shown but **numeric application pending**; test chars have all-zero `skill_level` blobs (skills reject until granted — DB-side note: `UPDATE player.player SET skill_level = overlay(skill_level placing '\x01' from 8 for 1) WHERE id = 2` grants skill 1 lvl 1).

### qc→DSL converter DONE — corpus 194/194 (40th-part wave, completed 41st)

- **`quest_dsl/src/convert/`** (qc.rs, map.rs, mod.rs): `qc.rs` = real-grammar parser (Lua 5.0 dialect with begin/end, multi-line when heads, inline ifs, while/repeat first-class AST nodes); `map.rs` = equivalence tables (**22 actions, 10 triggers, 10 conditions mapped**); `mod.rs` + `convert_corpus` CLI.
- **44 tests / 0 failed** (was 13), clippy 0.
- **Corpus: 194/194 files convert, 0 failed (~2 s)**; **5,513 unmapped items** (journal `q.*` ~944, UI setskin/makequestbutton ~625, target.pos, say_pc_name, pc.give_exp2/change_money — Rust-module territory per spec §8); **6 family proposals** (collect_quest 11, main_quest 32, subquest 44, collect_herb 6, new_quest 12, main_quest_flame 7 = 112/194 files).
- Next slice IN PROGRESS: family parameter extraction + Value-as-Expr (unblocks ~160 `pc.setqf` + affect calls).

### Benchmark — FIRST LOAD SIGNAL (report `logs/bench-run-2026-08-13.md`, bench lane; run against the 06:11 binary on auth7/chan7)

- **Smoke 1 bot**: world_ms **1524**, 12 mobs, no kick. **5 bots × 30 s**: 5/5 OK, 0 failures (world_ms median **2254**). **20 bots × 60 s**: **20/20 OK, 0 failures, 0 panics** (world_ms median **7829**, p95 8214, ~13 % spread); `--cleanup-accounts` → 0 rows (psql-verified).
- **FINDING — spawn concurrency**: dynamic materialization reaches only FIRST-COMERS under concurrency (**19/20 bots got 0 entries**; the 3 "2 entries" players counted 12 ADDs) — interpretation + **fix IN PROGRESS by the channel lane**.
- **Latency**: world_ms 1.5 s → 7.8 s (1 → 20 bots) — per-connection entry bottleneck candidate (login_ms 0.9 → 4.9 s), bounded and stable, no cascades.
- **Envelope interaction**: the harness walk (300 u/s) is rejected by the envelope after the first move (NOT kicked, session healthy) — harness fix in progress (walk ≤ ~250 u/s or config knob).
- Full ladder pending: WorldSim::metrics wiring, spawn-visibility interpretation, harness walk, sharded-region case, then 100/250/500/1000 bots.

### Deploy state

- Binary **3,935,232 B (07:23)** live (auth8/chan8 07:26) — the skills binary on the native-Windows stack; client unchanged (5,130,752 B).

### Evidence

- My attribute scan 2026-08-13: protocol 81, network 28, database 95, realm **113**, server_realms 50, mysql_proxy 67, locale_import 19, bench_bot 27, quest_dsl **44** → **524 attributes**; orchestrator-verified run: **481 passed / 0 failed** (realm 106 per run; quest_dsl 44/44).
- File:line: skill.rs (732 lines, 15 tests), ecs.rs:190-209/377/870, channel.rs:800-810/1958-1963/2002, `quest_dsl/src/convert/{qc,map,mod}.rs`, `logs/bench-run-2026-08-13.md` (120 lines).

### Pending

- **Spawn-concurrency fix** (channel lane, IN PROGRESS — first-comer visibility interpretation).
- **Family parameter extraction + Value-as-Expr** (converter lane, IN PROGRESS); harness walk fix.
- Benchmark full ladder (WorldSim::metrics, sharded-region case, 100→1000 bots).
- Skill GAPs: SPLASH/PARTY/HORSE families, `skill_power.txt`, buff numeric application (MOV_SPEED/ATT_SPEED/CRITICAL), quest-granted/passive skills.

## [2026-08-13] (40th part) — spawn dinámico live + walkability/speed envelope + quest DSL core + F3 tail ACID

> Wave 2 of 2026-08-13 (three code lanes active: channel.rs/skills, quest converter, benchmark run). All items verified by the orchestrator with real outputs; deployed binary live.

### Spawn dinámico DONE + deployed

- **Channel-level shared bevy World**: `spawn_despawn_system` in `realm/src/ecs.rs` — SPAWN_VIEW 2500 materialize, **DESPAWN_RADIUS 4000** (ecs.rs:449, hysteresis margin), combat mobs never despawn; the static SPAWN_VIEW filter in channel.rs is gone.
- **Entry via `Intent::Join` mpsc** (ecs.rs:337-340 — Veloren pattern, ADR-0010 §1); connections send intents over the mpsc and receive `NpcEvent`s back.
- **bench_capture wired — all 4 call sites** (open_conn / capture_conn in+out / close_conn): raw byte captures now land for the golden fixtures.

### Walkability + speed envelope DONE (fix-2 lane)

- **NEW `realm/src/map.rs` (553 lines)**: `server_attr` LZO1X parsing — **real map 41 file decoded: 16×20 tiles, 2,176,848 blocked + 1,076,378 water cells**; exact C++ cell arithmetic (`SECTREE_SIZE 6400`, `CELL_SIZE 50` — units→cell = `(x % 6400) / 50`); `MapStore` lazy + cached failures.
- **`realm/src/movement.rs` (296 lines)**: `PlayerMotion.speed` (default **300 u/s**), `MoveError::ExceedsEnvelope` (movement.rs:53-67), envelope = `speed × (dt + 100 ms) / 1000 × 1.20`, inert without an anchor.
- **channel.rs CG_MOVE validates walkability BEFORE `process_move`** — reject → position stands, no ban.
- **Verified**: workspace **418 passed / 0 failed**, clippy no new warnings, release built; **deployed 06:11 (binary 3,818,496 B — verified on disk)**; live stack runs it (auth7/chan7 logs 06:33).
- Follow-ups documented by the implementer: N-violations auto-ban (config knobs), path-line sampling (diagonal corner-cut), buffs/mounts recompute speed, warp re-anchor.

### Quest DSL core DONE (orchestrator direct — after 3 failed agent attempts)

- **NEW crate `quest_dsl/`**: modules `ast`/`parser`/`family`/`render`; typed catalog (triggers/conditions/actions per spec §3–§5); families expand; **13 tests green, clippy clean**; workspace member (Cargo.toml members list verified).
- **§11 decisions resolved**: `between` native, `if` 1-level + else, `select` as-capture, `@key`, `.quest` extension, `timer` alias.
- Next: **qc→DSL converter IN PROGRESS** (separate lane).

### F3 tail DONE (fix-2 lane, previous wave)

- **ItemRepo QID parity audit**; **`Batcher::flush()`** (ACID unit support — wal.rs:398); **`ItemExchange::exchange_mutated()`** (materials→result→gold in ONE tx — proven against real PG: 4 audit rows with the same `applied_at`; item.rs:233,600-603).
- **SocialRepo** (`GuildRepo` load/ranking — social.rs:58; note: `player.guild_member`/`grade`/`comment` NOT migrated to PG — documented gap).
- **EconomyRepo** (money_log append-only + `checked_gold_mutation` — economy.rs:97,154-163; note: `CHECK (gold >= 0)` does NOT exist in PG — the Rust guard is the enforcement; a G-PG ALTER is a follow-up).
- database **95 test attributes**.

### Deploy state

- Binary **3,818,496 B** (08-13 06:11) deployed and running (native Windows; auth7/chan7 logs 06:33); client remains 5,130,752 B (39th part).
- **Full benchmark run IN PROGRESS** against the live stack — no `logs/bench-run-2026-08-13.md` report yet (checked 2026-08-13).

### Evidence

- My attribute scan 2026-08-13: protocol 81, network 28, database **95**, realm **94**, server_realms 50, mysql_proxy 67, locale_import 19, bench_bot 27, quest_dsl **13** → **474 attributes**; orchestrator-verified run: **418 passed / 0 failed** (realm 87, server_realms 37 per run; quest_dsl 13/13).
- File:line: ecs.rs:337-340 (`Intent::Join`), ecs.rs:449 (`DESPAWN_RADIUS`), movement.rs:53-67 (`MoveError::ExceedsEnvelope`), wal.rs:398 (`Batcher::flush`), item.rs:233 (`exchange_mutated`), economy.rs:97 (`checked_gold_mutation`), social.rs:58 (`GuildRepo`).

### Pending

- **Skills IN PROGRESS** (channel.rs lane — server-timed buffs after), **qc→DSL converter IN PROGRESS**, **full benchmark run IN PROGRESS**.
- After: shops/safebox/trade wiring (exchange_mutated/ACID units ready), N-violations auto-ban (config knobs), path-line sampling, warp re-anchor, guild_member/grade/comment migration + `CHECK gold>=0` ALTER (G-PG).

## [2026-08-13] (39th part) — 5-front parallel wave: ECS migration (ADR-0010) + WAL idempotency + dwLoginKey real flow + client UTF-8 overrides + benchmark harness

> Parallel wave (coder ×3 + fixer ×3 + librarian) executed and verified by the orchestrator with real outputs. All deployed and running on the native-Windows stack.

### ECS migration DONE (cod-2) — bevy_ecs World replaces `MobCache`

- **bevy_ecs 0.19** in workspace deps (`default-features=false, features=["std"]`).
- **`realm/src/ecs.rs` NEW (986 lines)**: components Vid/Position/Hp/Aggro/Mob/Item/Player; resources Tick (ecs.rs:171), Rand (:179), NpcOutbox (:205), SpawnCache (`Arc<Mutex<MobCache>>`, :212); systems `chase_attack_system` (:271) / `aggro_detect_system` (:338) / `patrol_system` (:362) — chained in parity order; `WorldSim` wrapper (:463) — `resolve_spawns`/`spawn_npcs`/`damage_npc`/`spawn_item`/`update`.
- **channel.rs refactored**: `live_npcs`/`live_items` → World; AI tick → `world.update`; `CG_ATTACK`/`CG_MOVE`/potions/equip sync player state into the World.
- **Verified**: workspace **359 passed / 0 failed**; clippy no new warnings; release build green; deployed + running (native Windows, ports 30001/30003).
- **Accepted deviations** (documented by the implementer, accepted by the orchestrator): `multi_threaded` NOT enabled yet (one-line toggle at the F5 benchmark); SpawnCache stays `Arc<Mutex<>>` as a World resource (cross-connection PG-row cache, not world state); World is **per-connection** for now (channel-level shared World = the spawn-dinámico slice, IN PROGRESS); armor computed at entry/equip/unequip instead of per-tick (same values, zero per-tick PG round-trips). mpsc player-intent channel explicitly deferred to the spawn-dinámico slice (ecs.rs:34).

### WAL idempotency DONE (fix-2) — the 2 non-idempotent paths closed

- `database/src/safebox.rs` + `messenger.rs`: both INSERT paths now **`ON CONFLICT DO NOTHING`** on natural PKs (safebox.rs:10,88,122,128,166-180,197; messenger.rs:9,35,40,84,146,153) — legacy quirks preserved (`safebox size==1 → INSERT` parity, wire debt D4).
- New `SafeboxRepo::set_size_mutated` / `MessengerRepo::add_mutated` Batcher-wired.
- **`replay_wal` PG test UN-GATED and green against live PG (2/2 passed, 2.34 s)** — the anti-dupe crash path is now covered BEFORE trade/safebox.
- database **79 test attributes**; clippy 0 new.

### dwLoginKey real flow DONE (cod-3) — F2a debt closed at the auth

- `auth.rs` +325/−46: real **`LoginKeyStore`** (per-process `Mutex<HashMap>`, keys die with the process — C++ parity `ClientManagerLogin.cpp:81-178`, `input_auth.cpp:133-152`).
- **LOGIN_BY_KEY validated via the existing LOGIN3 `passwd[17]` field — no wire change** (68/88 B intact); wrong key → rejection; password path byte-for-byte unchanged.
- **18 auth unit tests** (auth.rs test attributes, verified 2026-08-13). Channel-side LOGIN2 consumption stays for the channel lane.

### Client UTF-8 overrides DONE (fix-1) — override API, inert until the data channel

- `CPythonNonPlayer::SetLocaleName(vnum, name)` (PythonNonPlayer.cpp:116), `CItemData::SetLocaleName` static map (ItemData.cpp:10-15), `CPythonLocale::Utf8ToDisplay` public; Python hooks `netSetLocaleName`/`netSetItemLocaleName` + registration (PythonNetworkStreamModule.cpp:1668-1708).
- MSBuild 0 errors; **exe 5,130,752 B** (verified: `metin2-extra\client\metin2client.exe`, 08-13 01:43) deployed.
- Lookup order: **override → bundle cache → pack**. The server does NOT send overrides yet (data channel future) — API inert until then.

### Benchmark harness DONE (fix-3)

- **NEW crate `source/reforge/bench_bot/`** (accounts.rs, bot.rs, main.rs, report.rs, splitter.rs; **27 test attributes** = 26 tests + 1 ignored; live-PG green). Smoke verified: 1 bot login→world **1321 ms**, 11 mobs, 5 moves with **no speedhack kick**; 2/2 concurrent bots; `--cleanup-accounts` leaves 0 rows.
- `server_realms/src/bench_capture.rs` (**201 lines**): raw byte capture module + **`--bench-capture <dir>`** flag (main.rs:34-35,59-61,85-89); 4 call sites for the channel lane to wire: open_conn / capture_conn in+out / close_conn. server_realms 50 test attributes.

### Ops + deploy state

- **`scripts/start_win.ps1` rewritten LAUNCH-ONLY**: prints OK, timestamped per-launch logs, no port verification inside (AGENTS.md rule 16 + operations.md §13).
- Deploy state: binaries live and running (native Windows — auth :30001, channel :30003, PG service `postgresql-metin2`); client 5,130,752 B.

### Evidence

- My own attribute scan 2026-08-13: protocol 81, network 28, database 79, realm 78, server_realms 50, mysql_proxy 67, locale_import 19, bench_bot 27 → **429 attributes**; orchestrator-verified run: workspace **359 passed / 0 failed**, `replay_wal` PG **2/2 (2.34 s)**.
- File:line evidence listed per item above (ecs.rs, auth.rs, safebox.rs, messenger.rs, PythonNetworkStreamModule.cpp, PythonNonPlayer.cpp, ItemData.cpp, bench_capture.rs, main.rs).

### Pending

- **Spawn dinámico IN PROGRESS** (channel-level shared World — the next slice; mpsc intents land with it, ecs.rs:34).
- Full N-bot benchmark run (F5 milestone) + `multi_threaded` toggle validation; `--bench-capture` channel hooks wiring (4 call sites).
- Walkability (`IsMovablePosition`) + speed envelope queued (last P0 anti-hack hole, ADR-0011); then `dw_arrow`, skills, shops, quests (DSL), safebox, trade, GM.

## [2026-08-12] (38th part) — WSL retirement executed (ADR-0012) + world-entry fixes verified on the all-Windows stack + backup script

> Executes the 37th part's plan (ADR-0012, logged in ROADMAP): the runtime is now NATIVE WINDOWS and the real client reaches the world with movement on the all-Windows stack.

### WSL retirement — Phase 1 EXECUTED (ADR-0012)

- **PostgreSQL 18.4 native on Windows** — Windows service `postgresql-metin2` (NETWORK SERVICE account), binaries `C:\projects\metin2-extra\pg18\pgsql\bin`, data `C:\projects\metin2-extra\pg18\data`, db `metin2`, role `mt2`/`mt2`, `LC_COLLATE='C'`, pgcrypto in schema `account` — `mysql_hash_password` works with `search_path account,public`. Restore from the WSL dump verified with matching counts: spawns 145,876, mob_names 8,628, item_names 34,281.
- **MariaDB archived + stopped**: `C:\projects\metin2-extra\archive\mariadb_full_2026-08-12.sql` (5.7 MB); no Windows MariaDB ever needed.
- **Migration dump kept**: `C:\projects\metin2-extra\backups\metin2_pg_2026-08-12.dump` (restore verified — counts above).
- **Rust auth + channel run native** from `source\deploy\win\` (`auth.toml`/`channel.toml`: listen `127.0.0.1:30001`/`30003`, PG `127.0.0.1:5432`, `timeout_ms 120000`, `map_path = source\deploy\main\srv1\share\locale\spain\map`).
- **Client**: `serverinfo.py` host → `127.0.0.1` + repack (verified in `source\tools\pack\root\serverinfo.py`); the client and the servers now share the Windows host.
- **Scripts**: `scripts/start_win.ps1` / `stop_win.ps1` (PG service → Rust auth → Rust channel; prints OK/FALTA per port 5432/30001/30003).
- **WSL = on-demand oracle box only** (frozen C++ binaries + `mysql_proxy`, cap 1 GB, off when unused; `/home/m2/source` archived later; full delete at F6). The proxy stays in WSL so the frozen C++ `conf.txt` is never touched.

### World-entry fixes — real-client verified (login → select → world → movement WORKS)

- (a) `replay_once` async via `tokio::sync::OnceCell` (`realm/src/world.rs:40-79`) — fixed the nested-runtime panic that killed the channel.
- (b) `SPAWN_VIEW` 2500-entry filter (`server_realms/src/channel.rs:390-393`) — fixed the 23,032-mob spawn flood that froze the client (11 visible now).
- (c) character ADD `b_moving_speed`/`b_attack_speed` 100/100 (`realm/src/packets.rs:265`, parity `char.cpp:2245-2246`; were 0 → client `SetMoveSpeed(0)` → player frozen + buried under terrain = invisible). Root cause found by the fixer with client-side evidence (`InstanceBase.cpp:824`, `ActorInstance.cpp:191-219`).
- (d) mob spawn ADDs now carry `move_speed` (`realm/src/npc.rs:760`, parity `char.cpp:2257`).
- (e) `TPacketGCMainCharacter` HEADER 113→15 (`protocol/src/world.rs:861-875`) — the client maps 15 = 47 B plain, 113 = 48 B EMPIRE variant; 113/47 B desynced the stream by 1 byte (latent).
- **"0 items" observation is NOT a bug** — verified: `player.item` holds 22 rows, all `owner_id 2` (other characters); the test chars have none. The item query contract is correct.

### Backup cadence + repo

- **`scripts/backup_win.ps1` (new)**: nightly backup of the native PG — `pg_dump.exe -h 127.0.0.1 -U mt2 -d metin2 -Fc` → `C:\projects\metin2-extra\backups\metin2_<yyyy-MM-dd>.dump`, retention = last 7 dumps, credentials via PGUSER/PGPASSWORD env, `ErrorAction Stop` + exit-code check, `-WhatIf` dry-run mode. Syntax-checked (`[scriptblock]::Create` parse OK). Scheduled by the orchestrator, not executed in this session.
- **53-commit backlog PUSHED**: `origin/main = 294edb1` = local HEAD (0 ahead / 0 behind, verified 2026-08-12).

### Evidence

- Real-client E2E on the Windows-native stack: login → select → world → movement; mobs spawn (11 visible after the SPAWN_VIEW filter); PG service `postgresql-metin2` Running; psql `player.item` count = 22 (owner_id 2).
- Code refs: `realm/src/world.rs:40-79`, `server_realms/src/channel.rs:390-393`, `realm/src/packets.rs:265` (+test :796), `realm/src/npc.rs:760`, `protocol/src/world.rs:861-875`; parity `char.cpp:2245-2246/2257`, client `InstanceBase.cpp:824`, `ActorInstance.cpp:191-219`.

### Pending

- WSL disk cleanup: `/home/m2/source` archive + PG/MariaDB data + toolchains out of WSL (deferred; full delete at F6).
- Next work slice: **ECS migration** (`MobCache` → bevy World, ADR-0010) + provisional N-bot benchmark; then walkability + speed envelope, spawn dinámico.

## [2026-08-12] (36th part) — Consolidated master plan + oracle review applied (H.1–H.5)

> User asked to join the current documentation and plan into ONE big plan and pass it through the oracle for improvement proposals. Done and applied.

### Consolidated plan

- **`docs/plans/master-plan.md` (new, Draft v0.2):** one document joining ROADMAP.md + `docs/plans/server-rewrite.md` (canonical design v0.3) + `docs/plans/locale-redesign.md` + ADRs 0001–0011 + `docs/CURRENT.md` snapshot: mission/principles, verified state, ADR index, target architecture (concurrency, domains, regional channels, data layer, manifest+locale, anti-speedhack), anti-hack table, stack, phases F0–F7 with milestones, F5.3 slices + tail, quest DSL, non-ported items, risks, open decisions, deferrals, ops.

### Oracle review (ora-1) — verdict: architecture sound; not ready to act as-is (H.1–H.5); no code changes required

- **H.1 — ECS migration is the next slice + provisional benchmark:** ADR-0010 mandates `MobCache → bevy World` next (ADR-0010:148-150) but the F5.3 tail omitted it; every slice built on `Arc<Mutex<MobCache>>` is rework. Applied to master-plan §8 + ROADMAP tail + CURRENT gates: **(1) ECS migration slice, (2) provisional N-bot benchmark** (wire-level bot simulator with sharded-region case, mob-density dimension, defined failure path), then walkability + speed envelope, the 2 non-idempotent WAL paths, `dw_arrow`, skills, shops, quests, safebox, trade, GM. **User confirmed ECS implementation** (mob-farming density is the core requirement — "con ECS lo que ganamos de rendimiento es una locura").
- **H.2 — Push + backup cadence:** local is **53 commits ahead** of origin/main (verified `git rev-list`, HEAD `d6d80d3`), the 4 GB host holds the ONLY copy of history + PG + WAL + client. Documented as data-loss risk (master-plan §13/§15, ROADMAP GitHub section); push still pending user confirmation.
- **H.3 — YAGNI cuts:** **Slint standalone deferred F5 → F7** (double protocol work — legacy wire now, new wire at F7; ADR-0007 amended) and **REST/metrics deferred post-cutover** (no F5 consumer); benchmark instrumentation stays in F5. Applied to master-plan, ROADMAP (lines kept as history), server-rewrite.md §8.2.
- **H.4 — Milestones redefined + E2E gate:** **F3 milestone rewritten** ("C++ game runs against the Rust database" is unreachable post-G-PG — the C++ runs on PG via `mysql_proxy`; now: ported QIDs identical on PG via the Rust crate + active pull channel + PROTO_FROM_DB); **F5 milestone = defined real-client session script** (login → kill → loot → stack → equip → potion → death → revive → warp); real-client E2E smoke gate every N slices (zero real-client evidence for slices 2–17 since F4).
- **H.5 — Staleness sweep:** CURRENT.md (ADR-0006/0009 statuses fixed to Accepted; ADR-0010/0011 added to the decisions list; commit → `d6d80d3`); ROADMAP Phase-0 ADR checkboxes (boundaries/concurrency/anti-hack → done, ADR-0010/0011); sqlx "deferred to the WAL phase" remnants removed (WAL phase DONE, decision stands); docs/README hub (ADRs 0001–0011, stale snapshot warning, master-plan link); server-rewrite.md document map 0001–0011; per-schema-permissions claim corrected (`mt2` owns all four schemas today; separation by repo discipline until RLS); **CP949 hard rules restated verbatim** in master-plan §11 (locale lua MUST be CP949/EUC-KR; `item_proto` names MUST stay original CP949).
- **Runner-ups applied:** migration tooling DECIDED (plain SQL files + small runner, no sqlx::migrate); `replay_wal` gated PG test to un-gate BEFORE trade/safebox (untested crash path of the anti-dupe guarantee); single-region double-login semantics added as a small decision (§13); locale wire header numbers to be pinned before the slice (162/163 taken by datachannel).

### Evidence

- Oracle review read the plan + all sources; `git rev-list origin/main..HEAD` = 53; HEAD `d6d80d3`; bevy_ecs absent from `source/reforge/Cargo.toml` (ECS decision vs implementation gap confirmed).
- Docs: 8 files updated (master-plan.md, ROADMAP.md, CURRENT.md, docs/README.md, server-rewrite.md, ADR-0007, locale-redesign.md, CHANGELOG.md); no code changed.

### Pending

- User confirmation to **push the 53-commit backlog** (H.2) and start the nightly `pg_dump` backup cadence.
- Next work slice: **ECS migration** (`MobCache` → bevy World) + provisional benchmark spec.

## [2026-08-12] (35th part) — Skill adjustments per agent (approved inventory)

> User asked for the full skill inventory per agent (current / candidates / unnecessary). Analysis delivered from verified config + skill descriptions (config skills dir, `~/.agents/skills`, project `.agents/skills`). Applied the 3 approved changes; the rest of the roster is already correct.

### Inventory highlights (verified 2026-08-12)

- **Unnecessary in the pool (never assigned, from other projects)**: playwright-*, webapp-testing, shadcn, svelte-*, tailored-resume-generator — web/resume skills irrelevant to a Rust game server + Slint/bevy client. Left installed (no harm), never assigned.
- **Observer 0 skills**: correct — its skill is the model itself (mimo-v2.5 native vision).
- **Librarian 4 / Explorer 3 / Designer 2 / Oracle 12**: already correct for their lanes (no changes).

### Changed — config (local/gitignored, requires restart)

- **Orchestrator 8 → 9**: removed `docker-development` (Docker out of the plan since 2026-08-11 — dead weight); added `verification-planning` + `deepwork` (the bundled scheduler skills it lacked).
- **Fixer 16 → 17**: added `zeroize-audit` (missing-zeroization detection for sensitive data — passwords/keys in memory; aligns with ADR-0011 anti-hack).
- **Coder 9 → 7**: removed `cpp-pro` + `make` (they were for the G-PG legacy adapter — already DONE, deleted at F6; note recorded in coder.md toolkit line).
- **Docs**: `coder.md` toolkit line updated (cpp-pro/make note); `agent-organization.md` specialization line updated (fixer +zeroize-audit, orchestrator skills, docker removed).
- **Verified**: JSON parses, UTF-8 without BOM, counts confirmed (orchestrator 9, fixer 17, coder 7).

### Pending

- **Restart opencode** to load everything accumulated: coder "The Reforger" + routing, MCPs for all agents, skill adjustments, oracle v4-pro.
- After restart: slice 18 spec review with the oracle on v4-pro, then the bevy_ecs adoption.

> User: "solo crea la del coder que todavía no tiene personalidad — ¿omo-slim trae un .md prompt ya enriquecido del cual podamos usar?" Answer: NO — coder does not exist in the harness pantheon (it is our custom agent replacing `build`); the harness ships agent prompts as `.ts` template strings (functional Role/Behavior/Constraints, no narrative — "The Last Builder" etc. is README marketing only, verified in `fixer.ts`); the `.md` mechanism is for USER overrides, not pre-made prompts. So the personality was created modeled on the harness style + our project rules.

### Changed — coder personality and config (local/gitignored, requires restart)

- **`.opencode/agents/coder.md` rewritten**: identity **"Coder — The Reforger"** (the legacy C++ is the broken blade; you forge it anew in Rust — clean, sharp, minimal) + mission, lane, method (senior not typist), never/always, team relationships, and **"The Reforger's creed"** (the legacy is the oracle of behavior, never of design; parity is a contract at the wire, a suggestion inside the formula; less code is a feature; evidence beats claims).
- **Empirical probe (cod-1)**: the active subagent prompt is the `.md` file content (quoted verbatim), NOT the config inline prompt — and it loads at startup (restart needed for the new personality). Also proves `.md` wins over inline prompt for custom agents.
- **Config `agents.coder` cleaned**: the inline `prompt` was CORRUPT mojibake (`coder ǟ�'...`) from an earlier BOM-corrupted write — removed (dead weight; the `.md` wins). Added `description` ("The Reforger — the project's expert writer...") + `orchestratorPrompt` (routing block for the orchestrator: DELEGATE bounded implementation with scope/acceptance/evidence/verification; REUSE sessions on same context; DO NOT delegate architecture/review/docs/recon/UI). JSON verified, no BOM.
- **Docs**: `agent-organization.md` roster row for coder (The Reforger + routing defined).

### Pending

- **Restart opencode** to load: coder personality (The Reforger), coder routing, MCPs for all agents, explorer/fixer skills, oracle v4-pro.
- After restart: slice 18 spec review with the oracle on v4-pro, then the bevy_ecs adoption.

> User direction: "las skills que ya tienen están perfecto — lo que quiero es que cada agente tenga habilidades enfocadas en su laburo: los MCPs que tenemos para todos, explorer con skills para explorar código a gran escala, librarian con skills de documentación, fixer con skills de debug/arreglar, coder con clean code". Deep analysis of the harness done first (lib-2: README completo + 7 docs + schema + los 8 prompts fuente de src/agents/*.ts).

### Analysed — the oh-my-opencode-slim harness (v2.2.13, github.com/alvinunreal/oh-my-opencode-slim)

- **Plugin de orquestación multi-agente** para OpenCode: registra los 7 agentes del panteón + council + observer + custom agents, tools propias (ast_grep, webfetch smart, cancel_task, wait_for_user), MCPs built-in (context7/gh_grep — provisionados programáticamente en v1, por eso NO están en opencode.jsonc), slash commands (/deepwork /reflect /preset), Background Job Board y wake scheduler.
- **Skills = permission grants**: un agente solo activa las skills asignadas (lista, "*", "!x"). 8 bundled: codemap, deepwork, verification-planning, simplify, worktrees, clonedeps, reflect, oh-my-opencode-slim.
- **MCPs = acceso por agente** (misma sintaxis; deny gana). Los propios se declaran en opencode.json y se otorgan por nombre.
- **3 capas de personalización**: routing (description + orchestratorPrompt → se inyecta al prompt del orquestador), permission grants (skills/mcps), prompt layering ({agent}_append.md por proyecto en .opencode/oh-my-opencode-slim/). Temperatures por defecto: designer 0.7 (única alta), resto 0.1-0.2.
- **Hallazgos**: @coder sin description/orchestratorPrompt (el orquestador no sabe rutar); Council sin configurar; 6/8 skills bundled sin conceder al orquestador; prompt layering por proyecto sin usar; variants todos "max" vs recomendación del preset (orchestrator thinking, lanes high).

### Changed — config (local/gitignored, requiere reinicio)

- **MCPs (graphify + context7 + gh_grep) concedidos a TODOS los agentes**: orchestrator, oracle, librarian, explorer, designer, fixer, observer, coder (antes solo orchestrator/librarian/coder tenían alguno). Verificado `opencode mcp list`: 3 servers connected; JSON sin BOM.
- **Explorer + `clonedeps`** (ahora graphify, codemap, clonedeps — explorar fuentes de dependencias a gran escala).
- **Fixer + `gdb`, `sanitizers`, `strace-ltrace`** (ahora 16: adversarial ×4, diagnose, clean-code ×2, verification, cpp-pro, rust-* ×3, improve-codebase-architecture, y las 3 de debug C++ legacy — para atacar el baseline C++ con herramientas reales).
- **Docs**: `agent-organization.md` línea de especialización actualizada (skills por lane + MCPs para todos).
- **Prompts documentados al usuario**: tabla con la esencia del prompt actual de cada agente (del harness + proyecto).

### Pending

- **Restart opencode** para cargar: MCPs de todos los agentes + skills nuevas de explorer/fixer (oráculo v4-pro sigue pendiente también).
- Después: revisión de la spec del slice 18 (World compartido por canal) y el desarrollo de personalidades restante (routing de @coder, Council, prompt layering por proyecto) si el usuario lo aprueba.

> The 31st part claimed (a) the GitHub repo "is not on GitHub yet" and (b) `context7`/`gh_grep` were "never registered — dead refs". **Both claims were WRONG** (user correction, verified empirically). This entry corrects the record; the 31st part stays as history.

### Corrected facts (verified 2026-08-12)

- **The repo IS on GitHub**: `github.com/ryerdevs/reforge-core` (PUBLIC, remote `origin`, authenticated `gh` as `ryerdevs`). The old "repo GitHub sin montar" F0 debt is superseded. ⚠️ **Local is 49 commits AHEAD of `origin/main`** (last pushed: `352b850` G-PG inventories) — everything since (ADR-0008→0011, bevy_ecs decision, parts 29-31) is local-only, NOT pushed.
- **`context7` and `gh_grep` ARE active**: they are **provisioned by the oh-my-opencode-slim harness** (plugin v2.2.13, `github.com/alvinunreal/oh-my-opencode-slim`), not by `opencode.jsonc`. Verified live with `opencode mcp list`: **3 servers connected** — `graphify` (local `python -m graphify.serve`), `context7` (`https://mcp.context7.com/mcp`), `gh_grep` (`https://mcp.grep.app`). The `mcps` field in `oh-my-opencode-slim.json` is the per-agent access list, not the server registration.
- **BOM bug introduced by me (fixed)**: my PowerShell `Set-Content -Encoding UTF8` wrote a **UTF-8 BOM** into `oh-my-opencode-slim.json`, breaking the plugin's JSON parser (`Invalid JSON ... Unrecognized token ''`). Fixed by rewriting with `UTF8Encoding($false)` (no BOM). Verified: `opencode mcp list` clean, JSON parses, all config changes preserved.
- **Librarian `mcps` restored**: `context7` + `gh_grep` were wrongly removed in the 31st part → back to `graphify, context7, gh_grep`.
- **GitHub CLI vs MCP, corrected answer**: the repo IS on GitHub, so a GitHub MCP is not premature on those grounds — but the `gh` CLI is already authenticated and sufficient for the current workflow (no PR culture yet); the harness does not bundle a GitHub MCP by default. Decision stands: keep `gh` CLI; add a GitHub MCP only when PR/issue workflow starts.

### Changed — config (local/gitignored, requires restart)

- `oh-my-opencode-slim.json` rewritten **UTF-8 without BOM** (was BOM-corrupted by my earlier PowerShell write).
- `librarian.mcps` → `graphify, context7, gh_grep` (restored).
- No other config change from the 31st part was lost (verified: oracle v4-pro, fixer 13 skills, designer 2 skills, coder graphify MCP).

### Pending

- **Push the 49 local commits** (user decision — the repo on the phone is 49 commits behind).
- **Restart opencode** to load all config changes (oracle v4-pro, fixer 13 skills, coder/librarian graphify MCP, designer brainstorming, restored MCPs).
- After restart: **slice 18 spec review with the oracle on v4-pro** before Coder starts the bevy_ecs adoption.

> User questions: "¿para qué sirven los MCPs? ¿no es mejor un GitHub MCP que el CLI? ¿no deberíamos añadir skills a los agentes sin skills?" — answered with verified facts, applied the fixes.

### Answered (verified)

- **MCPs**: only **graphify** is actually registered (`opencode.jsonc`, `python -m graphify.serve --graph graphify-out/graph.json` — the merged code graph, rule 13). `context7` and `gh_grep` were declared in the librarian's `mcps` in `oh-my-opencode-slim.json` but **never registered** → dead references.
- **GitHub CLI vs MCP**: keep the `gh` CLI (ponytail). The repo is not even on GitHub yet (F0 debt "repo GitHub sin montar") — a GitHub MCP server would be infrastructure for a workflow that doesn't exist. Revisit only when the repo is hosted and PR workflow starts (F6/F7).
- **Skills audit**: fixer 13 / oracle 12 / coder 9 / orchestrator 8 / librarian 4 / explorer 2 (correct for recon) — only **designer (1)** and **observer (0)** were thin. Observer's "skill" is its model itself (mimo-v2.5 = native vision; no visual-analysis skills exist in the pool).

### Changed — config (local/gitignored, requires restart)

- **Designer + `brainstorming` skill** (now 2: impeccable + brainstorming — the latter brings the visual companion, right for UI design).
- **Librarian `mcps` cleaned**: `context7`, `gh_grep` removed (dead refs) → only `graphify`.
- **Docs**: `agent-organization.md` specialization line updated (designer skills, only-graphify MCP, gh CLI note).

### Pending

- **Restart opencode** to load all config changes (oracle v4-pro, fixer 13 skills, coder/librarian graphify MCP, designer brainstorming, MCP cleanup).
- After restart: **slice 18 spec review with the oracle on v4-pro** before Coder starts the bevy_ecs adoption.

> Config: `~/.config/opencode/oh-my-opencode-slim.json` (preset `opencode-go` + `agents.coder`) + `.opencode/agents/*.md` (local, gitignored). Docs: `docs/explanation/agent-organization.md`. All changes require an opencode restart (guardrail rule 7 — verified empirically with a fresh-oracle probe: the new model does NOT load hot).

### Changed — agent team config

- **Oracle → `opencode-go/deepseek-v4-pro`** (user decision: architecture reviews/designs pass through the stronger model). Applied in BOTH places — the local `oracle.md` and the preset in the global config (the preset still said v4-flash and would have won).
- **Fixer + 2 skills** (now 13): `rust-async-patterns` (will attack the slice-18 tokio↔bevy mpsc bridge) + `improve-codebase-architecture` (structural anti-patterns / bad-code detection — user request).
- **Librarian + graphify MCP** (now context7 + gh_grep + graphify) — docs work needs the code graphs (rule 13).
- **Coder + graphify MCP** (was missing the `mcps` property entirely) — the writer must see the graphs before touching code (rule 13).
- **Docs**: `agent-organization.md` roster (oracle v4-pro), specialization line (fixer skills + MCPs), model note rewritten (oracle v4-pro, observer mimo-v2.5, rest v4-flash).
- **Verified**: config JSON parses; probe confirmed model changes load only after restart.

### Pending

- **Restart opencode** to load: oracle v4-pro + fixer/librarian/coder skill-MCP changes.
- After restart: **slice 18 spec review with the oracle on v4-pro** (fresh session) before Coder starts the bevy_ecs adoption (World compartido por canal, 5 pasos — spec de ora-1 aprobada).

> User decision after the strategic review: "Metin2 es un juego de farmeo — el lag con muchos mobs es el problema core; con ECS mejoraría muchísimo el rendimiento. Y para el cliente futuro, bevy (no wgpu desde cero)." Documented in ADR-0010 §2 (amended) + ADR-0007 (amended) + plan/ROADMAP.

### Decided — ECS and client stack

- **ADR-0010 amended + Accepted**: §1 realm architecture now has FOUR layers — pure domain modules + **bevy_ecs World** (components Position/Hp/Aggro/Mob/Item; systems AI 500 ms/movement/combat/drops; `default-features = false`, no bevy_reflect) + per-connection tokio tasks (intents via mpsc, Veloren pattern) + WorldStore/Batcher+WAL. §2 replaces the benchmark gate with **adoption NOW**: mob-farming density is the core requirement (145,876 spawns, map 41 = 10,026), solo-dev maintenance (ecosystem maintains archetypes/queries), one paradigm with the future client; the F5 benchmark validates the choice instead of gating entry. Alternatives/Consequences rewritten (hand-rolled SoA rejected — would grow into a mini-ECS; actor model rejected).
- **Client F7 = bevy + Slint (user decision)**: replaces the wgpu-from-scratch plan (months of work reinventing what bevy provides: render, assets, ECS, input). ADR-0007 amended accordingly. Same ecosystem on server and client.
- **ADR-0011 → Accepted** (anti-hack model: server-authoritative invariant, always-on controls ratified with file:line, signed clock wrap decided → modular difference with tolerance).
- **ADR-0009 → Accepted** (server-side locale, implementation live since 2026-08-11).

### Changed — docs (aligned with the decision)

- `ROADMAP.md`: ECS line 158 superseded by ADR-0010 **Accepted** (bevy adopted); F7 note/client/engine (213/215/234) → bevy + Slint; deferral 227 → bevy_ecs ADOPTED 2026-08-12.
- `docs/plans/server-rewrite.md`: §2 item 9 (defer list — bevy_ecs adopted, WAL DONE), §5.2 (182) ECS adopted, §5.5 (262-263) WAL DONE + contract fixed in ADR-0008, §7 table (385) Entities → bevy_ecs adopted, deferred list (393), F7 row (418), §11 ADR table (0009/0010/0011 → Accepted).
- `AGENTS.md` guardrails: ADR-0009/0010/0011 → Accepted with the bevy_ecs decision summarized.
- `docs/decisions/0007` amended: new client = bevy + Slint (decided 2026-08-12).
- `source/reforge/README.md`: realm row → bevy_ecs World adoptado (ADR-0010 §2).
- **Next slice**: ECS adoption implementation — `MobCache` → World components/systems with the 371 existing tests staying green (ADR-0010 Consequences).

> User-requested full-project review ("are we only transcribing, not innovating?"). Three recon lanes (explorer inventory, librarian plan digest, oracle verdict) + two doc lanes (librarian staleness sweep x2). No code changed.

### Verified — inventory & plan digest

- **Workspace real**: `source/reforge` = 7 crates, 49 files, **371 tests** (protocol 81, network 28, database 70, realm 64, server_realms 42, mysql_proxy 67, locale_import 19), 0 `todo!()`/`unimplemented!()`, 11 TODO. 396 "parity" + 946 C++ mentions in comments.
- **Plan written**: F0/F1/G-PG/F2a/F2b/F4 milestone MET; F3 + F5 in progress; F6/F7 not started. 12 internal doc contradictions found (CURRENT stale, ADR-0005/0006 mislabeled Proposed, sqlx vs tokio-postgres, F2b/F4 checkboxes, F5.3 taxonomy missing, ADR-0008 §5 contract lying).

### Decided — verdict (oracle, verified with file:line)

- **Deviation**: execution deviation small and healthy; doc staleness large (fixable, now fixed); method deviation = "innovate first, document later" (ADR-0008/0009 written after implementation) — to cut.
- **Transcribing vs innovating**: critique is **fair in the business-logic layer** (combat.rs/ai.rs/movement.rs replicate 25-year quirks: SPEEDHACK_LIMIT_BONUS=80 hardcoded, DISTANCE_APPROX `>>8` approximation, `number(1,5)` min-damage dice, f64→int truncations, signed clock wrap) and **unfair in infrastructure** (mysql_proxy wire codec, WAL pipeline, GC_CHANNEL_LIST, always-on anti-teleport/speedhack, server-side locale, DB fail-fast — all new, none in C++).
- **Key distinction codified**: parity of CONTRACT (wire byte-exact, required by the frozen client — the translator) vs parity of DESIGN (internal path — must be new code). Contract parity = the product; design parity = the mistake.
- **Innovation now (risk≈0)**: I1 constants → config TOML/game_config with same defaults; I2 signed clock wrap → modular difference with tolerance (ADR-0011 §3). **Frozen to F6/F7**: balance formulas, data-model quirks (observable output).

### Changed — docs (librarian, 12 items + second pass)

- `docs/CURRENT.md` → 2026-08-12, commit `c0954c5`, 371 tests by crate, F5.3 (17 slices), ADR-0008/0009, locale_import, next gates rewritten.
- `ROADMAP.md`: open-decisions → ADR-0005/0006 **Accepted**; sqlx → **tokio-postgres 0.7 (ADR-0008)**; F2b pending → DONE; F4/F1 checkboxes → [x] with milestone; **F5.3 block added** (17 slices, operational taxonomy, not a plan sub-phase); ECS line superseded by ADR-0010.
- `docs/decisions/0006`: body (proposed) → (accepted), aligned with frontmatter. `0008` §5: AMEND — save-by-event via Batcher+WAL (30s+logout = history). `0009`: metadata only, status still Proposed.
- `docs/plans/server-rewrite.md`: §2/§8.2 phase tables → real states; §11 ADR table (0005/0006 Accepted, 0008/0009/0010/0011 rows); §13 questions Q2/Q3/Q5/Q8/Q10 marked resolved (none deleted); §14 next-steps → done/partial; sqlx → tokio-postgres everywhere; Last verified 2026-08-12.
- `AGENTS.md` guardrails: ADR-0005/0006 → Accepted; ADR-0008/0009/0010/0011 rows added.
- `docs/reference/protocol/legacy-compatibility.md`: Current + §7 "Deliberate wire divergences" V1–V6 (0x00 close, speedhack always-on, anti-teleport, **signed clock wrap → ADR-0011**, DB fail-fast, idle timeout).
- `docs/reference/protocol/login-flow.md`: LOGIN3 **88B** variant (version+hwid) added. `source/reforge/README.md`: 7 crates, real counts, ECS claim removed.

### Added — new ADRs (both Proposed, pending user approval)

- **ADR-0010 — Domain boundaries and data ownership**: ratifies the real realm architecture (pure functions + per-connection state + WorldStore, NOT the plan's ECS); ECS entry criterion = F5 benchmark failing 1,000+/instance with ≥2–5x CPU headroom or AI-tick >500ms; data ownership volatile/durable/derived; **translator-vs-core governing boundary** (user principle codified); wire debt inventory D1–D6 with F7 removal plan.
- **ADR-0011 — Anti-hack model**: invariant server-authoritative zero client trust; ratifies implemented controls (timer speedhack always-on, anti-teleport, 0x00→close, DB fail-fast, idle timeout, server-clock cooldowns); **decides signed clock wrap** → modular difference with tolerance (kick stays as policy); pending controls with phase (speed envelope, walkability from PG, floods, god-mode, dupe completion, farm bots); attack-class table.

> Team model change (user-directed, defined before implementation). Config: `~/.config/opencode/oh-my-opencode-slim.json` + `.opencode/agents/*.md` (local, gitignored). Docs: this repo.

### Changed — the agent team

- **`build` → `coder`**: the expert writer (implementation of bounded features, owns the skills implementation). Built-in `build` disabled in the global config; new `coder` agent registered with its prompt/skills (clean-code, rust-*, ponytail, verification-before-completion).
- **`fixer` redefined as the quality guardian**: NOT read-only anymore — besides being Coder's adversary (bugs, structure, bad practices), it now **writes/expands the test suite, debugs to root cause, and guards scalability/maintainability** (quality-scoped refactors allowed; `edit: allow`). Never implements new features.
- **`oracle` redefined as the team lead**: supervision (unchanged) + **architecture decisions (ADRs before code)** + **ROADMAP priorities**. `edit: deny` (unchanged).
- **Docs updated**: `docs/explanation/agent-organization.md` (hierarchy, roster, standard flow, value contract, gates — new team), `docs/guardrails/agent-operations.md` (rules 1/3 updated for the new fixer role and coder name), `AGENTS.md` (roster note + work rule 10).
- **Operational nuance recorded**: the fresh-session rule for reviewers applies to the fixer's ADVERSARIAL reviews; its own quality WRITING (tests/refactors) may resume like coder when the context matches.
- **Action required**: restart opencode so the config changes apply (guardrail rule 7).

## [2026-08-12] (26th part) — F5.3 attack_speed del arma (GET_ATTACK_SPEED parity)

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — weapon attack speed (Rust, commit `0c3f995`)

- **`realm::combat::attack_speed_for_weapon(weapon) -> u32`** (pure, parity `GET_ATTACK_SPEED` battle.cpp:757-782): `real_speed = 1000*100/(80+0+0)` = **1250 ms** (el `ani_speed` default del constructor ANI, ani.cpp:121; la tabla `.msa` real del pack por raza/arma es GAP documentado); `DAGGER`/`CLAW` (subtipos 1/8 — ani.cpp:37-49) → `/2` = 625 ms (battle.cpp:774-779); `None`/no-weapon → 1250.
- **Channel `CG_ATTACK`**: resuelve el arma equipada ANTES de construir el `PlayerState` y ajusta `player.attack_speed_ms` — el cooldown del combate usa el intervalo del arma (antes siempre 1250).
- **Tests**: manos desnudas 1250, espada 1250 (ANI default), daga 625, garra 625, item no-weapon sin `/2`.
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: tabla `.msa` real (data-driven del pack), `dw_arrow` (quiver), skills, walkability (`IsMovablePosition`), NPCs interactivos/tiendas.

## [2026-08-12] (25th part) — F5.3 FindEquipCell: validación de tipo al equipar

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — equip type validation (Rust, commit `75ea629`)

- **`database/src/item.rs`**: `ProtoItem` + `wear_flag` (la columna `item_proto.wearflag` — los bits `WEARABLE_*`, item_length.h:379-392) añadida al SQL del proto.
- **`realm::packets`**: `mod wearable` (los bits) + `find_equip_cell(proto) -> Option<u16>` — el slot del equip según el orden EXACTO de los `else-if` del C++ (`item.cpp:568-592`): BODY=0, HEAD=1, FOOTS=2, WRIST=3, WEAPON=4, SHIELD=10, NECK=5, EAR=6, ARROW=9, UNIQUE=7, ABILITY=11 (length.h:99-119); `None` = no equipable (wearflag 0 — item.cpp:511-519 — o solo HAIR/PENDANT/GLOVE, GAP documentado).
- **`CG_ITEM_MOVE` equipar (channel)**: carga el proto del item y valida que el slot candidato == `find_equip_cell` (parity `EquipItem` → `FindEquipCell(item, iCandidateCell)`, char_item.cpp:6139) — rechaza items no equipables y slots equivocados.
- **Tests**: `find_equip_cell` (bits individuales → slot, varios bits → gana el primero del orden C++, wearflag 0 / solo-HAIR → None).
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: UNIQUE/ABILITY con múltiples slots (el C++ busca el primero libre — el subset usa el slot 1), `attack_speed` del arma, `dw_arrow` (quiver), skills, walkability.

## [2026-08-12] (24th part) — F5.3 ComputeParts: el personaje muestra el arma/armadura equipada

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — visual parts of equipped items (Rust, commit `d391482`)

- **`realm::packets::equipped_parts(row, inventory) -> [u32; 5]`** (pure): part = **vnum del item** (parity `item.cpp:793,833` — `SetPart(PART_WEAPON/MAIN, GetVnum())`); el slot del equip se deduce del cell wire (`EQUIP_CELL_BASE = 180 + wear`, length.h:827): `WEAR_BODY=0`→ARMOR, `WEAR_HEAD=1`→HEAD, `WEAR_WEAPON=4`→WEAPON (length.h:101-111); HAIR del row persistido (char.cpp:1710); ACCE=0 (GAP) + test (BODY/HEAD/WEAPON vnums, HAIR intacto, INVENTORY no afecta).
- **`character_additional_info_with_parts(row, empire, parts)`**: la variante con los 5 parts computados; `character_additional_info` delega con `equipped_parts(row, &[])` — semántica C++ corregida: sin items `PART_MAIN` = 0 (se setea al equipar; el `part_base` del row es la apariencia base `bBasePart`, char.cpp:1709 — antes se mapeaba `row.part_main` que no se carga del DB, GAP heredado).
- **Channel**: equipar/desequipar/revive/entry reenvían el `GC_CHARACTER_ADDITIONAL_INFO` con los parts computados de los items EQUIPMENT (`enter_packets` ahora recibe `parts`).
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: validación de tipo al equipar (`IsEquipable`/`CanEquipNow`), `attack_speed` del arma, `dw_arrow` (quiver), skills, walkability.

## [2026-08-12] (23rd part) — F5.3 items equipados afectan el combate (arma + armadura)

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — equipped items affect combat (Rust, commit `1a596b7`)

- **`database/src/item.rs`**: `load_proto_use_values` → `Option<ProtoItem>` (nuevo struct: `b_type`/`b_sub_type` del item_proto + `values[6]` value0..5 — el SQL ampliado a `type, sub_type, value0..5`). El handler de pociones usa `proto.values`.
- **`realm::combat::melee_damage`**: con arma equipada (`Option<&ProtoItem>`) → `iDam = number(value3, value4) × 2` (`Item_GetDamage`, battle.cpp:460-461,533) + `iAtk += value5 × 2` (battle.cpp:548); sin arma sigue `roll(0,1) × 2`. `handle_attack` acepta `weapon`.
- **`realm::combat::player_def_grade(level, ht, i_armor)`**: + `iArmor` de los items ARMOR equipados (char.cpp:2124-2125: `value1 + 2×value5`); el test ampliado con casos de armadura.
- **Channel**: `CG_ATTACK` resuelve el arma equipada (cell `INVENTORY_MAX_NUM + WEAR_WEAPON(4)` = 184) y la pasa a `handle_attack`; el AI tick (mob→jugador) suma el `iArmor` de los items `EQUIPMENT` con `b_type == ITEM_TYPE_ARMOR` y subtipo BODY/HEAD/SHIELD/FOOTS (`matches!(0|1|2|4)`) a la DEF del jugador.
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: parts visuales de items equipados (ComputeParts — el ADDITIONAL_INFO aún usa parts del row), validación de tipo al equipar (`IsEquipable`), attack_speed del arma, skills.

## [2026-08-12] (22nd part) — F5.3 equipar/desequipar items (CG_ITEM_MOVE + EQUIPMENT window)

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — item equip/unequip (Rust, commit `b4cad81`)

- **`INVENTORY_MAX_NUM = 180`** (length.h:29 con `ENABLE_EXTEND_INVEN_SYSTEM` — CommonDefines.h:32; 5×9×4) + `WEAR_MAX_NUM = 32` (length.h:77). **Fix**: el pickup buscaba slot libre en 0..90 — corregido a 180 (el runtime real tiene 4 páginas).
- **`CG_ITEM_MOVE` EQUIPAR** (INVENTORY→EQUIPMENT, parity `EquipItem` char_item.cpp:6128): el cell del window EQUIPMENT = `INVENTORY_MAX_NUM + wear` (length.h:827 `IsEquipPosition`); slot vacío obligatorio (parity `GetItem(DestCell)` → false, char_item.cpp:5675-5680); `num` debe ser 0 (el split al equipar es pendiente); wire: `GC_ITEM_DEL` deprecated (origen) + `GC_ITEM_SET` (destino EQUIPMENT) + `GC_CHARACTER_ADDITIONAL_INFO` (parts del row — los parts de items equipados son pendiente: ComputeParts con items) + upsert.
- **DESEQUIPAR** (EQUIPMENT→INVENTORY): destino INVENTORY vacío obligatorio; mismo wire inverso.
- **Belt/DS siguen fuera** (documentado).
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: parts visuales de items equipados (ComputeParts), validación de tipo del item al equipar (`IsEquipable`/`CanEquipNow`), split al equipar, ATT/DEF de items equipados en combate.

## [2026-08-12] (21st part) — F5.3 CG_ITEM_MOVE — mover/stack/split de items del inventario

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — inventory item move (Rust, commit `c1f5407`)

- **`protocol/src/world.rs`**: `TPacketCGItemMove` (8 B, header 13 — `Packet.h:593-599`: header + TItemPos origen + TItemPos destino + BYTE num) + roundtrip test (incl. num 0 = todo el stack). El framer ya lo conocía como 8 B.
- **`CG_ITEM_MOVE` handler (channel)**: parity `MoveItem` (`char_item.cpp:5609-5767`):
  - misma posición / sin item en la celda / `num > count` → ignorado (parity `@fixme196`, `GetItem`, `GetCount < count`).
  - destino ocupado con MISMO vnum + sockets iguales → **STACK** (límite 200, `GC_ITEM_UPDATE` en ambos; `GC_ITEM_DEL` deprecated + delete PG si el origen se agota).
  - destino vacío + `0 < num < count` → **SPLIT** (`GC_ITEM_UPDATE` origen + `GC_ITEM_SET` destino con id del rango `ITEM_ID_RANGE` + upsert).
  - destino vacío → **MOVEr todo** (`GC_ITEM_DEL` deprecated origen + `GC_ITEM_SET` destino + upsert).
  - fuera del subset (documentado): equipar (`EQUIPMENT`), belt, dragon-soul.
- **`ITEM_COUNT_LIMIT`** movido a nivel de módulo (lo comparten pickup/move/use).
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: equip (`EquipItem` — window EQUIPMENT + parts en el ADDITIONAL_INFO), belt, dragon-soul, walkability.

## [2026-08-12] (20th part) — F5.3 usar pociones del inventario (CG_ITEM_USE) + fix bug latente del framer

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — potion use (Rust, commit `e40c289`)

- **Fix BUG LATENTE del framer**: `CG_ITEM_USE` estaba registrado como 16 B (el tamaño del GC S→C) pero el struct C→S real es `header + TItemPos` = **4 B** (`Packet.h:559-563` + `packet.h:618-622`) — usar un item del inventario habría desincronizado el stream. Corregido a 4 B + test actualizado.
- **`protocol/src/world.rs`**: `TPacketCGItemUse` (4 B, header 11 — el uso de item C→S) + `TPacketGCItemDelDeprecated` (42 B packed, header 20 — el borrado de item del inventario; el cliente lo registra con `sizeof(TPacketGCItemDelDeprecated)`, PythonNetworkStream.cpp:71) + roundtrip tests; `header::GC_ITEM_DEL = 20` (lib.rs).
- **`database/src/item.rs`**: `ItemRepo::load_proto_use_values(vnum)` — `player.item_proto.value0..4` (el efecto de uso del item).
- **`CG_ITEM_USE` handler (channel)**: busca el item en el inventario por `(window INVENTORY, cell)` → carga los values del item_proto → aplica el efecto de la poción (parity `UseItemEx`, char_item.cpp:4172-4204): `value0` = HP flat, `value1` = SP flat, `value3` = HP % del máximo, `value4` = SP % del máximo (clamp a máximos; NO consume si HP/MP están llenos — parity `used`) → `GC_POINTS` (hp/mp) + `GC_ITEM_UPDATE` (38 B) si queda count o `GC_ITEM_DEL` deprecated (42 B) + `ItemRepo::delete` si se agota → upsert/delete + `save_character`.
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: `GC_ITEM_USE` S→C (22, la animación de uso — el C++ la manda con `UsePacketEncode` item.cpp:188-198; omitida en el subset), demás subtypes de `ITEM_USE` (ability up, affects), `ITEM_FLAG_STACKABLE` from item_proto.

## [2026-08-12] (19th part) — F5.3 aggro proactivo + aggressive_sight data-driven

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — proactive aggro (Rust, commit `7886e68`)

- **`database/src/npc.rs`**: `MobRow.aggressive_sight` (smallint — `wAggressiveSight` del mob_proto; el rango en UNITS en el que un mob AGRESIVO detecta al jugador) + SQL/mapper/column-order test.
- **`realm::ai::is_aggressive(ai_flag)`**: parity `AIFLAG_AGGRESSIVE` (`char_state.cpp:224-226`) — el `ai_flag` del PG es el SET legacy como TEXTO ("AGGR,COWARD"): contiene "AGGR" en cualquier posición + test (posiciones del SET, None, vacío).
- **AI tick — AGGRO PROACTIVO**: un mob `AGGR` detecta al jugador DENTRO de su `aggressive_sight` y empieza a perseguirlo por iniciativa propia (parity `FindVictim(wAggressiveSight)`, `char_state.cpp:893`); sight 0 = nunca proactivo. Antes solo se volvía hostil al recibir daño.
- **De-aggro DATA-DRIVEN**: el umbral fijo de 5 000 units se reemplaza por `aggressive_sight.max(2 000)` — el mob abandona la persecución cuando el jugador sale de su rango real (floor 2 000: un mob con sight 0 pero GOLPEADO sigue persiguiendo un mínimo).
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: walkability (`IsMovablePosition`), `ITEM_FLAG_STACKABLE` from item_proto, armor (`iArmor`) in player DEF, multicast.

## [2026-08-12] (18th part) — F5.3 DEF del jugador en el daño del mob

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — player DEF in mob damage (Rust, commit `12cf334`)

- **`realm::combat::player_def_grade(level, ht)`**: `level + (int)(ht / 1.25)` — parity `char.cpp:2114` (PC as victim; subset WITHOUT armor — the `iArmor` from equipped items `char.cpp:2115-2140` and DEF_GRADE_BONUS are pending) + truncation test (ht=6 → 4.8 → 4).
- **`realm::ai::attack_damage(min, max, victim_def, roll)`**: `iAtk = number(min,max)` (the mob_proto damage IS its attack) → `iDam = MAX(0, atk − def)` → floor `number(1,5)` if `< 3` (parity `CalcBattleDamage`, `battle.cpp:199-206` — a nearly-blocked hit still lands 1..5) + tests (subtraction, floor, degenerate range, defensive min>max).
- **Channel AI tick**: passes `player_def_grade(row.level, row.ht)` to the mob attack.
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: armor (`iArmor`) in player DEF, `ITEM_FLAG_STACKABLE` from item_proto, walkability (`IsMovablePosition`), `aggressive_sight` data-driven, multicast.

## [2026-08-12] (17th part) — F5.3 item stacking al recoger (AutoStackItem parity)

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — pickup stacking (Rust, commit `206f97d`)

- **`protocol/src/world.rs`**: `TPacketGCItemUpdate` (38 B packed, header 25 — `packet.h:1078-1085` + `Packet.h:1715-1722`: header + `TItemPos` + count BYTE + sockets 3×long + attrs 7×3 B) + roundtrip test; `header::GC_ITEM_UPDATE = 25` (lib.rs).
- **`CG_ITEM_PICKUP` stacking** (parity `AutoStackItemProto`, `char_item.cpp:6722-6755`): before creating a new slot, the pickup searches the inventory for an existing item with the SAME vnum, empty sockets (`FN_check_item_socket`) and `count < 200` (`g_bItemCountLimit`, `config.cpp:39`) → adds the picked count to the stack, sends `GC_ITEM_UPDATE` (38 B) and persists with `ItemRepo::upsert`; any remainder goes to a new slot (the existing `GC_ITEM_SET` path). The `ITEM_FLAG_STACKABLE` check of the item_proto is NOT consulted (documented subset).
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: `ITEM_FLAG_STACKABLE` from item_proto, walkability (`IsMovablePosition`), `aggressive_sight` data-driven, player-DEF in mob damage (`char.cpp:2113-2114`), multicast.

## [2026-08-12] (16th part) — F5.3 patrullaje de mobs idle (world alive)

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — mob idle patrol (Rust, commit `547733d`)

- **`realm::ai::patrol_step`** (pure, parity `UpdateState` IDLE — `char_state.cpp:668-688`): probability 1/7 per tick (`!number(0, 6)`), random direction 0..359°, step 300-700 UNITS, destination CLAMPED to the spawn radius (the C++ doesn't clamp but the IDLE state keeps the mob near its spawn — documented; walkability pending, partial parity) + 4 tests (probability, radius clamp, border no-op, nearby target kept).
- **`LiveNpc`** + `home_x/home_y` (spawn position) + `nomove` (`ai_flag` contains "NOMOVE" → parity `AIFLAG_NOMOVE`, the mob never patrols).
- **AI tick patrol branch**: idle mobs (no aggro, no NOMOVE) VISIBLE to the player (≤ 2 500 units — the C++ only updates the player's sectree) walk toward the patrol target with their `move_speed`; max 20 `GC_MOVE` per tick (no flood — the map has 23k spawns).
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: walkability (`IsMovablePosition`), `aggressive_sight` data-driven, player-DEF in mob damage (`char.cpp:2113-2114`), multicast.

## [2026-08-12] (15th part) — F5.3 warp a la ciudad (revive answer 1) + de-aggro por distancia

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — city warp + de-aggro (Rust, commit `d687f77`)

- **`protocol/src/world.rs`**: `TPacketGCWarp` (15 B, header 65 — `packet.h:1381-1388` + `Packet.h:199`: header + lX + lY + lAddr inet_addr LE + wPort) + roundtrip test; `header::GC_WARP = 65` (lib.rs).
- **Revive en la ciudad (`CG_SCRIPT_ANSWER` answer == 1)**: hp/mp to subset maxima + save + `GC_WARP` (destino = `exit_x/y` del personaje o el village 969600/278400 del mapa 41; addr/port = el canal via `parse_listen` + `ip_to_inet_addr`). El cliente hace `__DirectEnterMode_Set` + `Connect(addr, port)` (`RecvWarpPacket`, PythonNetworkStreamPhaseGame.cpp:942-954) → reconecta con el flujo DirectEnter completo que el canal Rust ya sirve (F4). Answer 0 sigue `RestartAtSamePos`.
- **De-aggro por distancia (AI tick)**: un mob hostil abandona la persecución si el jugador se aleja más de 5 000 units (50 m) — parity del C++ (el mob pierde el aggro fuera de su rango); el data-driven con `aggressive_sight` del mob_proto queda pendiente.
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending**: `aggressive_sight` data-driven, player-DEF in mob damage (`char.cpp:2113-2114`), patrol/states (`ai_flag`), multicast.

## [2026-08-12] (14th part) — F5.3 PC death + revive (GC_DEAD + RestartAtSamePos)

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — player death and revive (Rust, commit `b89bdd8`)

- **Death (AI tick)**: the mob attack now subtracts from `row.hp` with NO floor; `hp <= 0` → `GC_DEAD` (14) of the player + `GC_POINTS` with hp 0 + durable save (the client shows the death screen). The previous floor-1 clamp is gone.
- **Revive (`CG_SCRIPT_ANSWER`, 29, 2 B — `Packet.h:679`)**: `RestartAtSamePos` parity (`cmd_general.cpp:534` + `char.cpp:838-873`) — restores hp/mp to the subset maxima (`compute_max_points`) and sends `GC_CHARACTER_DEL` + `GC_CHARACTER_ADD` + `GC_CHARACTER_ADDITIONAL_INFO` + `GC_POINTS` + save (the client resets the instance in place). Script answer while alive is ignored with log (quests F5.x).
- **Verified**: `cargo test --workspace` green (excluding the pre-existing cold-start-flaky `f16_peer_smoke`), clippy no new warnings.
- **Pending (documented)**: warp-to-city revive (answer 1 → `WarpSet(EMPIRE_START)`), player-DEF in mob damage (`char.cpp:2113-2114`), patrol/states (`ai_flag`), de-aggro by distance, multicast.

## [2026-08-12] (13th part) — F5.3 NPC AI slice 2: el mob ataca en rango

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — mob attack in range (Rust, commit `ee75301`)

- **`database/src/npc.rs`**: `MobRow.damage_min`/`damage_max` (smallint, `mob_proto`) + SQL/mapper/column-order test.
- **`realm/src/ai.rs`**: `attack_damage(damage_min, damage_max, roll)` — `number(min,max)` inclusive puro (min==max → fijo sin sorteo; min>max → defensivo devuelve min; NO resta la DEF del jugador — la fórmula del PC como víctima es pendiente, `char.cpp:2113-2114`) + tests.
- **`server_realms/src/channel.rs`** (AI tick): el mob aggro **EN RANGO** (parity `melee_max_range` — 300 UNITS o el rango del mob) ahora ATACA: `GC_MOVE(FUNC_ATTACK)` (x/y = posición actual, dwDuration 0 — parity `char_state.cpp:386`) + `GC_DAMAGE_INFO` (135, el número de daño) + daño al jugador (`row.hp`, roll `number(min,max)` con rand32) + `GC_POINTS` (la barra) + `store.save_character`. Fuera de rango sigue persiguiendo (slice 1).
- **Muerte del PC pendiente (documentado):** `row.hp` se fija en floor 1 (sin `GC_DEAD`/respawn del jugador — F5.x). El mob sí muere (slice anterior: `GC_DEAD` + `GC_CHARACTER_DEL`).
- **Verified**: `cargo test --workspace` green excluyendo `f16_peer_smoke` — **flaky de cold-start pre-existente (F1.6)**: primera ejecución del binario tras compilar → timeout de 10 s; segunda corrida del mismo binario → 2/2 en 0.04 s (race de timing del fake-auth; no toca nada de esta ronda). Clippy sin warnings nuevos.
- **Pending**: ataque del mob con la DEF del jugador, muerte/respawn del PC, patrullaje/estados (`ai_flag`), de-aggro por distancia, multicast.

## [2026-08-12] (12th part) — F5.3 NPC AI: aggro + persecución + broadcast GC_MOVE

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests, unit tests only).

### Added — NPC AI slice 1 (Rust, commit `255228a`)

- **`protocol/src/movement.rs`**: `TPacketGCMove` (24 B, header 3 — S→C, `Packet.h:1912-1923` + `EncodeMovePacket` `char.cpp:825-836`): header + bFunc + bArg + bRot + dwVID + x + y + dwTime + dwDuration; consts `FUNC_WAIT/MOVE/ATTACK` (packet.h:565-572); roundtrip + bad-length tests.
- **`database/src/npc.rs`**: `MobRow.move_speed` (smallint default 100 — `bSpeed` del mob_proto → `m_dwMoveSpeed` del C++) + SQL/mapper/column-order test.
- **`realm/src/ai.rs`** (nuevo módulo): `step_toward` (paso normalizado hacia el objetivo, clamp al destino; **speed 0 = sin movimiento — el test unit pilló el bug del "salto al destino"**: un mob inmóvil no se teletransporta) + `rotation_5deg` (bRot en pasos de 5°, cardinales verificados 0/18/36/54) + 5 tests.
- **`server_realms/src/channel.rs`**: `LiveNpc` + `move_speed`/`aggro`; el mob se vuelve hostil al recibir daño (`npc.aggro = true` en CG_ATTACK con damage > 0, parity `OnDamage`); **AI tick de 500 ms** en el game loop: los mobs aggro persiguen al jugador (paso por tick) y su `GC_MOVE` (FUNC_MOVE + destino + dwTime/dwDuration para la interpolación del cliente) se difunde por la conexión.
- **Verified**: `cargo test --workspace` green (realm 47/47 incl. 5 nuevos tests de ai; protocol 73/73 incl. 2 de GC_MOVE); clippy sin warnings nuevos.
- **Pending (documentado en `realm::ai`)**: el ataque del mob en rango (FUNC_ATTACK), patrullaje/estados (`ai_flag` COWARD/BERSERK), de-aggro por distancia, multicast a observadores.

## [2026-08-12] (11th part) — F3 phase 2: WAL local a disco + replay (ADR-0008)

> Implemented directly by the orchestrator (no delegation — user directive). Follows the 8-point review spec of fix-3 (baseline gate: 25/25 gated PG tests passed 2026-08-11; no gated tests run this session, unit tests only).

### Added — durable-first WAL (`source/reforge/database/src/wal.rs`, commit `13d928e`)

- **`WalSink<S: MutationSink>`** (envelope durable-first): persists the batch to `{wal_dir}/{uuidv7}.wal` (JSONL, one `payload_json` per line, `sync_all`) BEFORE touching PG; deletes the file ONLY post-COMMIT; on error the file STAYS on disk for the next-boot replay. The batcher error path now has a real recovery promise (was "the WAL will re-apply" with no WAL).
- **`replay_wal(dir, pg_conn)`**: pure function — re-applies each `*.wal` (sorted by uuidv7 = chronological) as ONE batch (one tx + audit via `PgMutationSink`) and deletes the file post-commit. Returns how many files were re-applied. Invocable from tests; wired once per process in production.
- **`parse_payload_json`**: inverse parser of the closed payload format (no serde — std only): strings with `\"`/`\\` escapes, UTF-8 pass-through, `\xHEX` → `Param::Bytes`, numbers → `Param::Int`, `null` → `Param::Null`; unknown keys are skipped (forward-compatible). `Param::Bytes` Display now emits `\\x` (escaped — valid JSON; was raw `\x` which the parser rejected).
- **Idempotency audit documented** in the module (fix-3 point 2): the 5 wired paths are idempotent in result (player UPDATE by PK — `last_play=NOW()` re-written, harmless; item/quest/affect `ON CONFLICT`; item_award UPDATE); the 2 plain-INSERT paths (`safebox.set_size` size==1, `messenger.add`) are NOT wired and documented as pending (replay would violate PK).
- **Concurrency** (fix-3 point 1): `replay_wal` runs ONCE per process via `OnceLock` in `WorldStore` (multiple `WorldStore` per login connection — concurrent replays against live appenders would corrupt).
- **`realm/src/world.rs`**: `WorldStore::new` + `with_audit_table` rebuild the SAME WAL→Batcher→PG wiring (fix-3 point 4 — the WAL is never silently disabled in tests); wal dir = env `REALM_WAL_DIR` or `./wal` (documented dual-CWD caveat, fix-3 point 6).
- **Tests (unit, no PG)**: payload round-trip with all param types (Text with quotes/backslash/UTF-8, negative Int, non-ASCII Bytes, Null), empty params, `persist_batch` writes a parseable JSONL file (cleanup always), WalSink keeps the file on error and removes post-commit (two isolated scenarios), bad-uuid rejection. database 48/48, workspace green, clippy no new warnings.

### Pending

- Gated PG test for `replay_wal` against the real PG (pattern `e2e_wal_*` + `DATABASE_TEST_PG` + cleanup always) — written in the spec but NOT run this session (user directive: no gated tests); the existing `wal_pg.rs` gated suite (2 tests) still passes as baseline.
- Pre-existing clippy in `wal.rs` (`enclosing Ok + ?` / `async fn syntax` in `MutationSink::apply`) — left as-is (improvement, not requirement per fix-3 point 7).
- `social`/`economy`/`log` repos: stubs remain (fix-3 point 8 — zero usage in login/select/enter; documented as the correct answer).

## [2026-08-12] (10th part) — F5.3 item drops on kill + pickup (direct implementation)

> Implemented directly by the orchestrator (no delegation — user directive; no gated PG tests were run, unit tests only).

### Added — drops (Rust, `source/reforge`, commit `9424efd`)

- **`protocol/src/world.rs`**: `TPacketGCItemGroundAdd` (58 B packed, header 26 — `ENABLE_ITEM_GROUND_EX` active on BOTH sides, `packet.h:1087-1098` + `Packet.h:1724-1738`), `TPacketGCItemGroundDel` (5 B, header 27), `TPacketGCItemOwnership` (30 B, header 31) + roundtrip tests; headers `GC_ITEM_GROUND_ADD=26`, `GC_ITEM_GROUND_DEL=27`, `GC_ITEM_OWNERSHIP=31` in `protocol::header`.
- **`database/src/npc.rs`**: `MobRow.drop_item` (i64, `player.mob_proto.drop_item` → `item_proto.vnum`) + SQL/mapper/column-order test.
- **`server_realms/src/channel.rs`**:
  - On mob death: primary drop (`mob_proto.drop_item`) with `drop_rate` from config, at the mob position → `GC_ITEM_GROUND_ADD` + `GC_ITEM_OWNERSHIP` (player name, parity `item.cpp:145-162`); ground items tracked in `live_items` (VIDs 50 000+, no collision with NPCs 10 000+).
  - `CG_ITEM_PICKUP` (15, 5 B): distance ≤ 600 (`CItem::DistanceValid`, `item.cpp:461-472`) → first free inventory cell 0..90 (`INVENTORY_MAX_NUM`, `length.h:29`) → `GC_ITEM_SET` (item enters inventory) + `GC_ITEM_GROUND_DEL` → id from `ITEM_ID_RANGE` (100M-200M, parity `ItemIDRangeManager.cpp:93,121`) + `ItemRepo::upsert` durable → inventory kept mutable in the loop.
- **Verified**: `cargo test --workspace` green (protocol 71/71 incl. 3 new ground-item tests; realm 42/42; network 26/26); clippy no new warnings in touched files (fixed the `to_vec()` unnecessary-use warnings from my own code; doc-comment list-item warning fixed).
- **Pending**: drops are primary-only (`drop_item` column) — the C++ also uses `etc_drop_item.txt`/`common_drop_item.txt` by CP949 name (TRAP AGENTS.md §17) — not ported; item stacking (`AutoStackItem`), ownership expiry, and drop items for other players (single-player world for now).

## [2026-08-12] (9th part) — F5.3 kill rewards + chat + client locale cache (implemented directly by the orchestrator, no delegation)

> **Workflow note:** the three fixer lanes dispatched earlier that session returned review reports instead of implementations (and the gameplay lane errored). The user asked to stop waiting on stalled delegated tests and solve it directly — this entry is the direct implementation, verified with pure unit tests (no gated PG tests were run).

### Added — F5.3 kill rewards (Rust, `source/reforge`)

- **`database/src/npc.rs`**: `MobRow` + `exp` (bigint), `gold_min`/`gold_max` (integer) from `player.mob_proto` (types per `legacy-schema.md` §4.6); both SQLs (single/batch) + mapper + column-order test updated.
- **`realm/src/combat.rs`**: `kill_reward(mob_exp, gold_min, gold_max, exp_rate, gold_rate, roll)` — pure function (parity: exp del mob × rate; gold = `number(min,max)` × rate); 5 unit tests (rates, fixed gold, zero mob, channel contract).
- **`server_realms/src/channel.rs`**: on mob death (hp ≤ 0): reward → `row.exp/gold` (saturating) → level-up loop (`next_exp` recargado por nivel vía `CommonRepo`, parity `exp_table`) → re-send `GC_POINTS` updated → `store.save_character` (Batcher durable). `row` and `next_exp` now mutable.

### Added — chat (CG_CHAT/GC_CHAT, Rust)

- **`network/src/framer.rs`**: `CG_CHAT` (3) variable-size packet (WORD LE `length` at [1..3] = total size incl. 4 B header, `Packet.h:534-539` + `input_main.cpp:641-655`); `length < 4` → close (parity PHASE_CLOSE); only in Channel role; test: full/fragmented/concatenated/invalid/auth-rejects.
- **`protocol/src/lib.rs`**: `header::GC_CHAT = 4`.
- **`server_realms/src/channel.rs`**: game-loop handler — echoes `GC_CHAT` (header + size incl. 9 B + type + dwVID + bEmpire + msg) to the player (single-player world; multicast later).

### Added — client locale cache integration (C++, `source/client` — the 4 patches from fix-2's audit)

- **`GameLib/ItemData.h/.cpp`**: `TLocaleNameProvider` typedef + `SetLocaleNameProvider` + static `ms_pfnLocaleName`; `GetName()` → provider first, pack (`szLocaleName`) fallback. Gamelib does NOT include PythonLocale.h (dependency direction preserved).
- **`UserInterface/PythonLocale.cpp`**: ctor registers the item-name provider; `Utf8ToDisplay()` (UTF-8 → `GetDefaultCodePage()` via `MultiByteToWideChar(CP_UTF8)`/`WideCharToMultiByte`, fallback raw — fixes the guaranteed mojibake "JabalÃ"); applied in `ParseKeyValue` (all domains, at store time); `ParseBundle`: empty bundle (`iSize < 1`) → `Clear() + true` (was `false` → login disconnect on the defensive branch).
- **`UserInterface/PythonNonPlayer.cpp`**: `GetName()` → `CPythonLocale::GetMobName` first, pack fallback; `#include "PythonLocale.h"`.
- **Rebuilt + deployed**: MSBuild Release|Win32 0 errors → `metin2client.exe` **5,128,192 B**, SHA256 `26DC9FDD...` → `C:\projects\metin2-extra\client\metin2client.exe` (previous `.bak` kept).

### Fixed

- `server_realms/tests/auth_locale.rs`: removed unused `decode_chunks` import (clippy).
- `realm/src/npc.rs` test fixture: `MobRow` constructor updated with the new reward fields.

### Verification (all real output, no gated PG tests)

- `cargo test --workspace` — all green (realm 42/42 incl. 5 new kill_reward; network 26/26 incl. CG_CHAT variable test; others unchanged).
- `cargo clippy -p protocol -p network -p database -p realm -p server_realms --all-targets` — no new warnings (remaining protocol warnings are pre-existing F1 WIP; mysql_proxy warnings pre-existing, untouched).
- MSBuild Release|Win32: 0 errors.

### Pending

- Gated PG tests (`channel_pg`/`auth_locale`/`mob_pg` etc.) NOT run this session (user directive — they stall in this environment; PG was down; the fix-3 gated baseline 25/25 passed 2026-08-11).
- F5.3 next: drops/items on kill, NPC AI (aggro/movement), movement broadcast (`GC_CHARACTER_MOVE` to observers), exp stat/skill points on level-up (currently only level+exp+gold via GC_POINTS).
- F1 client still pending: UI/map/skill rendering integration + language selector; `common.item_icons`/`map_names` sources.
- WAL local-to-disk + replay (fix-3's 8-point review) still unimplemented — next data-layer slice.

## [2026-08-12] (8th part) — F0 closed + F1 importer (locale + maps/spawns in PG)

### Fixed

- **channel_pg test bug** (`server_realms/tests/channel_pg.rs:887`): the "dead vid no damage" check had inverted logic — the 2 s timeout (the CORRECT behavior) was treated as an error via `?`. Now the timeout is the success case. Root cause of the repeated red test, not a server bug.
- **F5 perf fix VERIFIED end-to-end** (the contract that was cancelled twice): from WSL, `cargo test -p server_realms --test channel_pg -- --ignored` → **6/6 pass**; `entry + 23033 spawns leídos en 12.2 s` (previous stall: 3–4 min via 10,026 sequential PG connections; now batch/cache). Also `cargo test -p protocol -p realm`: 100/100 unit tests pass.

### Changed

- **F0 cleanup (user-approved)**: `DROP SCHEMA world CASCADE` in PG (aborted-redo debris: `world.map` 1 row + `world.map_spawn` 10,026 rows, 0 code references) + deleted `scripts/gpg/migrate_spawns.py`. The proper spawns-in-PG work is F1 (below), not the redo.
- **ADR-0009 written** (`docs/decisions/0009-server-side-locale.md`, Proposed): server-owned locale — one table per text domain (`common.*`), `GC_LOCALE` request/response, EN fallback, refined items derived from base, DX11/Slint client parked.
- **Design closed** in `docs/plans/locale-redesign.md`: 8 locale tables + `world.maps`/`world.spawns`, hot reload via per-entry read + NOTIFY later, map names = image + text overlay, language selector (default EN).

### Added — F1 importer (`source/reforge/locale_import/`, new workspace member)

- CLI binary, one subcommand per domain, idempotent (per-lang DELETE / TRUNCATE), `--pg`/`--proto-dir`/`--pack-locale`/`--locale-strings`/`--map-path` flags with WSL UNC defaults. Reuses the verified `realm::npc::load_map_spawns` parser for spawns (no reimplementation); reuses DumpProto artifacts (`source/tools/proto/<lang>/{mob,item}_names.txt`) — no MMPT0/MIPX binary parsing needed.
- Schema applied to PG via `scripts/gpg/f1-locale-world-schema.sql` (idempotent): `common.{mob_names,item_names,item_descriptions,skill_names,map_names,ui_texts,message_texts,item_icons}` + `world.{maps,spawns}` (+ spawns index).
- **Live data (verified by orchestrator with psql)**: mob_names **8,628** (es/en/de × 2,876), item_names **34,281**, item_descriptions **22,674**, skill_names **402**, ui_texts **3,903**, message_texts **12,489** (16 langs), `world.maps` **65**, `world.spawns` **145,876** (Σ count 279,603; map 41 = 10,026 entries, Σ 23,033 — matches `map41_spawns.rs` parity, incl. the design row `(41, 101, 957600, 247300, 3, mob)` and `5004 Anywhere@(969600,278400)`).
- Tests: `locale_import` 17 passed + 2 gated (live PG: mobs es = 2,876, map-41 Σ = 23,033); `cargo test --workspace` all green, clippy clean.
- **Gaps documented**: `common.map_names` empty (map names are TGA images, no text source yet); `common.item_icons` empty (import-icons skipped — needs EPK extraction + TGA→PNG, parked for the panel slice); `\"` kept as backslash+quote (exact C++ parity); Korean CP949 leftovers byte-faithful (client renders identically).

### Pending

- F1 wire slice: `CG_LOCALE_REQUEST`/`GC_LOCALE` + client cache (CPythonLocale) + loading screen — next.
- `common.item_icons` / `common.map_names` sources.
- F5 quest engine (quest DSL designed, not started).

## [2026-08-11] (7th part) — Repo cleanup: heavy assets moved outside the repo

### Changed

- **`client\` (2.2 GB installed client) and `archive\` (1.6 GB backups) MOVED OUT of the repo** → `C:\projects\metin2-extra\` (user decision). `archive` needed `robocopy /MOVE` (a directory-handle lock blocked `Move-Item`; 6 files, 1.643 GB, 0 errors). `client-om2\` was already deleted; stray `tsize.obj` removed.
- **Rust build artifacts cleaned**: `cargo clean` freed 9.1 GiB (31,572 files). A temporary CARGO_TARGET_DIR redirect to `C:\projects\metin2-extra\target` was reverted the same session (user decision — the target stays in the workspace at `source\reforge\target`).
- **Docs updated**: AGENTS.md (layout table, runbook, recompile/repack instructions, crash-dump paths), ROADMAP.md (GitHub preparation section), `docs/guardrails/world-entry-crash.md` (dump paths).

### Pending (pre-existing, NOT caused by this change)

- **F5 WIP (uncommitted, external session)**: mid-session `cargo check -p protocol` failed (E0425: `GC_ATTACK`/`GC_DAMAGE_INFO` missing from `crate::header`); after edits to `protocol/src/lib.rs` a later check **finished clean** (`Finished dev profile in 6.51s`). Full workspace not re-checked (realm/server_realms unverified).

## [2026-08-11] (6th part) — F4 MILESTONE MET: the real client enters the world against the Rust core and stays

### Fixed (the world-entry saga — 7 server-side iterations, all matching the CLIENT contract)

The client reached the select screen early, but world entry failed silently (clean close, no dump = Python-exception pattern; the instrumented client proved NO exceptions — each failure was a SERVER mismatch with the client's contract):

1. **Entry queue** (C++ order verified `input_db.cpp:428-459`/`input_login.cpp:611-656`): LOADING → MainCharacter → 36× quickslots → Points → Skills → items → affects → [CG_ENTERGAME] → ADD+INFO+GAME+LandList(435B)+TIME+CHANNEL.
2. **MainCharacter 47B** — the CLIENT's layout has NO `empire` (48B server layout desynced the whole stream → the client closed on an invalid header after the loading).
3. **lAddr/wPort in the 449B** — the DirectEnter reconnect uses them (`introselect.py` → `ConnectGameServer`); 0/0 → silent `OnConnectFailure` → back to login. Now `inet_addr` format + port (bytes at offsets 64/68 verified).
4. **0xf1 CG_CLIENT_VERSION2 (67B)** — the client sends its version at the end of the loading; the C++ ignores it (`input.cpp:205-213`).
5. **Game-phase C→S table** — 24 packets (CG_MOVE 16B verified, ATTACK, ITEM_*, etc.) accepted + ignored (gameplay is F5); unknown/variable headers still close (parity).
6. **Inactivity timeout** — the absolute 15s killed sessions; now per-read (reset on ANY client packet).
7. **Heartbeat** — the client is SILENT at idle; the C++ pings (`GC_PING` 44, 1B, `desc.cpp:179-214`); the channel sends it every 10s (`ping_interval_ms`, < idle 15s) and the client's `CG_PONG` resets the idle.

### Added

- **Client instrumentation** (`python_error.log` — Python exceptions + RecvErrorPacket content; kept — the definitive diagnostic tool; the client was rebuilt + deployed, md5 `2D94E5EE...`).
- **Guardrails**: the client's `Packet.h` is the wire contract for game-phase packets (field-by-field; the 47B lesson); the heartbeat is server-side; timeout semantics (absolute vs inactivity).

### Verified

- **F4 MILESTONE MET (2026-08-11):** the REAL client enters the world against the Rust core and stays — the user sustained 50+ seconds in the map (log: session live without timeout; `paquete de juego 0xNN ignorado` for the spawn traffic). World is empty (NPCs/mobs = F5); movement unprocessed (F5); HUD full (real MAX points), hotbar initialized, inventory empty.
- **Workspace: 227 passed / 0 failed / 31 ignored** (channel gated 5/5 incl. deployed; heartbeat test: session lives 2.5× the timeout with pings/pongs).

### Pending

- **F5 (the big part — gameplay)**: movement (speed envelope + walkability + anti-speedhack), combat, drops/items/inventory, NPCs/mobs, quests, shops — the client currently stands still in an empty world.
- F4 tail: client UTF-8 name overrides, minimal Entity core + ECS (bevy_ecs — due at F4/F5).
- F3 tail: PROTO_FROM_DB harness note; capture-snapshot harness; data-channel activation (framing map 162/163 — the client's CheckPacket kills on unregistered headers, noted).

## [2026-08-11] (5th part) — F4 slice 1 (realm WorldStore + select/spawn packets) + F3 tail (snapshot + data channel)

### Added — F4 slice 1 (`realm` crate, world entry)

- **`realm/src/world.rs` — `WorldStore`:** composition over `PlayerRepo`/`Batcher` — `new` (fail-fast SELECT 1), `list_characters`, `select_player(account, slot)` (slot→pid via the C++-literal index query `ClientManagerPlayer.cpp:794`, slot≥5→Err, pid=0→None parity `input_login.cpp:260-271`), `save_character` via `save_mutated` (durable, audited). Verified `log.mutation_audit` live. **Debt documented:** the index query is the only direct SQL (player_index not ported in F3) — move to `database` later.
- **`realm/src/packets.rs` — byte-exact select/spawn mappings** (structs reused from `protocol`): `TSimplePlayer` 71B (`ClientManagerLogin.cpp:324-383`), `GC_LOGIN_SUCCESS_NEWSLOT` 449B (`desc.cpp:955-988`), `TPacketGCCharacterAdd` 37B (`char.cpp:886-920`, type=6, race=job), `TPacketGCCharacterAdditionalInfo` 70B (`char.cpp:924-948`, alignment/10). **GAPs documented:** `part_acce` (not in PlayerSummary), items→parts (WEAPON/HEAD/ACCE), speeds/affects flags → 0 (runtime slices).
- Tests: 6 unit + 4 gated (`realm_pg.rs`: live slots → pids [1,3,5,2], invalid slot Err, save via Batcher + audit, cleanup guaranteed) — **4/4 stable × 4 runs**.

### Added — F3 tail

- **Parity harness snapshot mode** (`parity_check.py`): `--make-snapshot` (MariaDB = cutover reference) + `--snapshot` (PG vs reference, deterministic). Snapshot `/tmp/gpg/cutover_snapshot.json`; post-cleanup: **27 OK / 4 DIFFs — all operational** (account.lang es-vs-en from the client; `hol` + his slot + an item — real gameplay). E2E test residue cleaned (2 `e2e_rust_*` chars, 4 `m2e2_*` messenger rows).
- **Data channel 162+:** `protocol/src/datachannel.rs` — `CG_QUERY` (162: table_id + payload), `GC_RESPONSE` (163: table_id + row_count + payload), 6 tests, registered. **Client:** `PythonNetworkStreamPhaseLogin.cpp` 162/163 contract registration (inert no-ops; `Tracenf` — sys_log is disabled in the client) — rebuilt (0 errors) + deployed (md5 `6FC7A5B9...`, backup preserved).

### Verified

- **Workspace: 196 passed / 0 failed / 22 ignored** (realm 6+4, protocol 43+6 datachannel, database 34+11, mysql_proxy 66, network 24+2, server_realms 14+4).

### Pending

- **F4 slice 2 (the channel):** listener + handshake + channel LOGIN3 (65B) + select flow end-to-end (`WorldStore::account_slots` needed — the 5 pids ordered) + spawn (GC_PHASE LOADING/GAME, map, sectree) + the packet GAPs (items→parts, affects→flags, speeds). `player_index` repo + `part_acce` in PlayerSummary (debt).
- Client data-channel framing map (with the channel activation); PROTO_FROM_DB harness note.

## [2026-08-11] (4th part) — F3 close (world repos + Batcher wiring) + live fixes

### Added (F3 close — commits e32578c, bb30010)

- **World repos in `database`** (contract parity with file:line): `QuestRepo` (load/save, lValue==0 → DELETE else upsert), `AffectRepo` (load/save/remove, upsert PK 4 cols), `SafeboxRepo` (size/load/set_size with the size==1 INSERT parity, set_gold), `ItemRepo` (load_by_owner 22 cols, upsert `ON CONFLICT (id)` with id==0 → DEFAULT + RETURNING, delete, max_id_in_range, item_award load/take idempotent), `MessengerRepo` (list/add/remove; duplicate → Err 23505 documented).
- **Batcher wired to the write path:** `PlayerRepo::save_mutated(&Batcher, &PlayerRow)` — uuidv7 mutation + audit in the SAME tx, ≤100ms batches (tested with paused time), idempotent replay. `wal.rs` extended: `Param::Null` + int2/int4/int8 encoding by target type (**22P03 fix** — postgres-types i64→INT8 only).
- **Workspace: 184 passed / 0 failed / 18 ignored** (unit 34 database, gated 18/18 × 5 consecutive stable runs). Known environmental flake: `f16_peer_smoke` fake-auth timing (pre-existing).

### Fixed (live errors, orchestrated directly — fix-4 lane returned empty with nothing applied)

- **WAL audit DDL applied to live PG:** `log.mutation_audit (mutation_id uuid PK, applied_at timestamptz default now(), payload text)` — verified `\d`.
- **NPC motion data (the Uriel/Mirine animation bug):** `mob_proto.folder=''` for the custom NPCs → mapped 9 races to existing folders with verified motlist (`blacksmith` ×7: Anciana/Vendedor/Mirine/Aranyo/Soon/Uriel/Maestro Lucha Mental; `bkarcher` for Maestro Arquero; `treasure_hunt_box` for 20714). UPDATE applied to **PG and MariaDB** (parity mob_proto OK, md5 `efe5e128...`). Core restarted (pid 4418) — final in-game verification pending (NPC spawn).
- **`Extern\include\boost\preprocessor\debug\error.hpp` restored** (official boost 1.83.0, 1574 B) — client rebuilds no longer need a shim.
- **Guardrail added** (`agent-operations.md` Rule 1): empty terminal result from a writer lane = UNVERIFIED — verify actual artifacts before reconciling.

### Pending

- F3 tail: data channel client packets (162+), `PROTO_FROM_DB` harness note, capture-snapshot harness; in-game NPC animation verification (the user's next entry).
- F4 (world entry + names in Rust — the realm crate with WorldStore = PlayerRepo + Batcher) — the push toward 50%.

## [2026-08-11] (3rd part) — col+0 fixed + F3 phase 2 (WAL/PlayerRepo/auth consolidation) + F0 capture milestone

### Fixed

- **4 `col+0` ENUM/SET gaps (E2E 55/0/4 → 59/0/0):** `translate.rs` now rewrites `col+0` on ENUM/SET columns to the MySQL index/bitmask semantics via a static `ENUM_COLUMNS` catalog (12 columns, SHOW CREATE source; `mob_proto.size/ai_flag/setRaceFlag/setImmuneFlag`, `item_proto.immuneflag`, `skill_proto.setFlag/setAffectFlag/setAffectFlag2/eSkillType`, `item_attr(.rare).apply`, `item.window`). ENUM → 1-based index (''/no-enum → 0, NULL → NULL); SET → bitmask (`AGGR,NOMOVE,BERSERK,...→3459`; `ATTACK,USE_MELEE_DAMAGE→3`). **Bonus:** `item.window+0` in the safebox load was the same live bug (C++ received 'SAFEBOX'→0 instead of 3) — fixed. Docs updated (legacy-sql-compatibility §4, legacy-schema §4 CHECK-loss notes).

### Added — F3 phase 2 (data layer)

- **`database/src/wal.rs`:** uuidv7 hand-rolled (48-bit ms + version 7 + rand; format/uniqueness/chronological tests) + `Batcher` (tokio worker, ≤100ms flush verified with paused time, one transaction per batch, idempotent replay `ON CONFLICT DO NOTHING` + audit append in the SAME tx; `AUDIT_DDL` exported, not applied to the live PG). Gated integration vs real PG: replay 2× → 3+3 rows (not 6), invalid mutation → full rollback 0+0.
- **`database/src/player.rs` (world domain):** `PlayerRepo::load` (43-col contract in the C++ parse order, bytea raw), `save` (41 cols + `last_play=NOW()` + blob round-trip), `list_for_account` (15 cols), `create` (`id=DEFAULT` + `RETURNING id`, rule B5). Gated integration vs real PG 3/3 (ninja load, list ≥3, throwaway create→load→save→reload byte-identical + guaranteed cleanup).
- **Auth consolidated onto `AccountRepo`:** inline SQL removed from `auth.rs` (`pg_validate` → `account_login` using `login`/`set_lang`/`set_hwid`); SQLSTATE added to repo errors to preserve the "hwid column missing → log + continue" handling; 4 smokes green; redeployed (md5 `b7176f7a...`, pid 22219) — **f16_peer LOGIN OK verified** (wire intact).
- **F0 capture harness (F0 milestone MET):** `scripts/gpg/capture_auth.sh` + `extract_pcap_login3.py` (stdlib pcap parser, TCP reassembly) → golden fixture `source/reforge/protocol/tests/golden/auth_login3_40999.bin` (88B, md5 `6a93aa8f...`, deterministic across 2 captures) + `golden_auth.rs` (3 tests) — **the real captured LOGIN3 parses and re-serializes byte-for-byte identical**.

### Verified

- **Workspace: 168 passed / 0 failed / 7 ignored** (database 18+7 gated, mysql_proxy 66, network 24+2, protocol 37+3 golden, server_realms 14+4 smoke) — my own run.
- Gated integration vs real PG: 7/7 (2 account + 3 player + 2 wal).
- E2E DB: **59/0/0** post-fix; stack healthy (auth pid 22219).

### Environment note

- **core1 died silently ~01:48** (no shutdown log, no OOM in dmesg — WSL memory pressure during the WSL-side release build; the 2GB cap is the known constraint). Restarted cleanly (pid 22292, PG confs intact). Watch: WSL builds + the running stack coexist badly on 2GB — do WSL cargo builds when the game is idle.

### Pending

- Wire the `Batcher` into the realm write path; local WAL replay after crash; RLS (post-WAL); next repos: quest/affect/safebox/item/item_award/messenger (Q6 E2E queries); F4 (world entry + names in Rust — the big push toward 50%).

## [2026-08-11] (2nd part) — E2E DB suite green + F3 started (ADR-0008 + database crate)

### Added

- **`scripts/gpg/e2e_db.sh`** — repeatable E2E suite for the DB layer: the real db-binary query set (Q1 QUERY_LOGIN 13 cols, Q2 player load 42 cols, Q3 char list, Q4 creation with escaped blobs, Q5 save, Q6 quest/affect/safebox/item_award/messenger, Q7 locale, Q8 item id probes, Q9 boot protos with `col+0`) replayed through the proxy vs MariaDB as oracle; throwaway character cycle with guaranteed cleanup (`trap`); volatile exceptions documented (last_play/hwid/x/y/playtime). **55 PASS / 0 FAIL / 4 GAP(crate), exit 0.**
- **ADR-0008 (Accepted):** data layer — tokio-postgres 0.7 decided for the `database` crate (evidence in the ADR: proven end-to-end here, 0 new deps, full contract incl. LISTEN/NOTIFY; sqlx deferred to the WAL phase with measurements, pool via deadpool-postgres possible without a driver change); PostgreSQL-only repos, no direct-sql backend; durable = transactional batch ≤100ms, volatile = save 30s+logout; WAL + `mutation_id` (uuidv7) + idempotent replay → F3 phase 2; RLS post-WAL; failover F5/F6.
- **Crate `database`** (first slice, domain `account`): `AccountRepo::login` (13-column QUERY_LOGIN contract with LEFT JOIN player_index, MySQL hash in Rust + defensive col0==col3 re-check parity `CreateAccountTableFromRes:288-292`), `set_lang`/`set_hwid`; world/social/economy/log domain stubs. **7 unit + 2 integration gated (`#[ignore]`) — integration run against real PG in WSL: 2/2 ok** (test/1234 → id=1, empire=3, pids=[1,3,5,0,2], hash exact, status OK; wrong password → None; lang/hwid persist+restore — confirming the F2b hwid fix end-to-end). **Workspace 149 passed, 0 failed, 2 ignored.**

### Found (E2E — crate checklist, not masked)

- **4 `col+0` gaps in mysql_proxy translate:** ENUM/SET columns (mob_proto `size`/`setRaceFlag`, item_proto `immuneflag`, skill_proto `setFlag`) — the C++ reads the enum INDEX (`size+0` → index in MySQL); the proxy returns the raw text (index 0 vs 1 divergences on setRaceFlag). The §4 translation table requires index semantics (same class as the `item.window` fix).
- **Non-ASCII raw bytes in SQL literals** (0xfe/0xde with latin1 client) → PG UTF-8 errors — pending crate item (the `\0` escaping works).

### Pending

- Fix the 4 `col+0` ENUM/SET index gaps + non-ASCII literal handling (mysql_proxy crate — next lane).
- F3 phase 2: WAL pipeline (batches ≤100ms + mutation_id + idempotent replay); migrate the auth's queries to `AccountRepo` (no behavior change); next QIDs: player load/save (world domain).

## [2026-08-11] (1st part) — F2b complete: version check + hardware ID verified with the real client

### Added

- **Client (F2b, additive, ADR-0007):** `TPacketCGLogin3` → **88B at auth** (`dwVersion` 68..72 + `hwid[16]` 72..88 after `szLanguage`; `static_assert` 88 verified) — `UserInterface\Packet.h:479-493`; new `Hwid.h` (MachineGuid hex-decode with `KEY_WOW64_64KEY` — mandatory for Win32 — fallback volume serial (8-arg SDK) → zeros); `AccountConnector.cpp:182-186` fills version + hwid; **channel LOGIN3 stays 65B** (`SendLoginPacket`/`SendLoginPacketNew` subtract the auth-only fields). Rebuilt (3.1 min, 0 errors, 14 pre-existing warnings) → deployed (md5 `7D3783F1...`, backup `metin2client.exe.f2b_backup` preserved).
- **Server (Rust auth):** LOGIN3 parse 68/72/88 (`protocol`); framer variable range `111-auth = (68,88)`; **version gate** (`expected_version = 40999` config default; mismatch → clean close, no invented status — `input_auth.cpp` has no version check); **hwid → `UPDATE account.account SET hwid=$1`** as hex text (fixed the `[u8;16]` ToSql serialization bug); backward compatible with 68B.
- **PG:** `account.hwid VARCHAR(64) NOT NULL DEFAULT ''` (MariaDB frozen without it → `VOLATILE_COLUMNS` updated in parity_check).
- Server time (F2b item 3): **verified already working** — client aligns its clock at handshake (`ELTimer_SetServerMSec`) + receives epoch time at world entry (`GC_TIME` → `SetServerTime`); no change needed (recon evidence).

### Verified

- f16_peer: 88B + version 40999 + hwid → LOGIN OK + hwid persisted; version 99999 → `VERSION MISMATCH got=99999 expected=40999 — cierre limpio`; 68B → backward compat OK.
- **REAL client (new build):** login → select → **world entry** (01:08, `battle_hit` in core1); `account.hwid` = `bb2cad39631b49e2bd0c61bcc577e7e6` (the machine's real MachineGuid).
- Parity: 3 legitimate **operational** diffs (`player` +1 row "hol", `player_index` pid3, `item` +1) — the user played on PG while MariaDB stays frozen; harness target decision (snapshot-based) deferred to F3.

### Pending

- `source\client\Extern\include\boost\preprocessor\debug\error.hpp` missing (client rebuilds need a temporary shim) — restore permanently.
- Parity harness post-cutover target (PG snapshot vs frozen MariaDB) — F3.
- Next phase: **F3** (data layer + data channel).

## [2026-08-10] (11th part) — World entry on PG fixed + F2a: Rust auth serving real clients

### Fixed (post-gate, found by real gameplay)

- **World-entry client crash on PG (A/B proven):** the proxy returned bytea columns as PG's `\x...` TEXT instead of raw bytes for the player_load query → the core binary-copied it into `TPlayerTable` → the client closed ~1s after ENTERGAME. Root cause: `session::first_from_table` matched the first "FROM" without parenthesis-depth filtering — the translated `EXTRACT(EPOCH FROM LOCALTIMESTAMP)` in the projection resolved `localtimestamp` as the table → empty bytea detection. Fixed with depth tracking + safe fallback (unknown table → ERR 1146, never silent `\x` text). Verified with a byte-exact replay diff of the exact query (`ClientManagerPlayer.cpp:361-375`) + A/B: MariaDB enters fine, PG now enters fine (user played: combat, kill events, logs).
- **22021 on player create/save:** MySQL `\0`-escaped blobs in bytea columns (skill_level/quickslot) reached PG text literals → NUL → 22021. Fixed: bytea literals → `decode('<hex>', 'hex')` in INSERT VALUES and UPDATE SET.
- **Proxy translate gaps closed:** `CAST(x AS unsigned)` → MySQL leading-numeric-prefix semantics (`COALESCE((regexp_match(x, '^[[:space:]]*[+-]?[0-9]+'))[1]::bigint, 0)` — fixes 22P02 on game boot); MySQL double-quoted string literals → PG single quotes (fixes 42703 `LIKE "LOCALE"`).
- **Migration gaps closed:** `item_award` (character select), `messenger_list` (world entry) + audit: no other active missing tables (42P01 inventory vs timestamps).

### Added — F2a (ROADMAP Phase 2): `server_realms --role auth`

- First REAL server logic of the rewrite: handshake (`network`) → `GC_PHASE(PHASE_AUTH)` (the client sends LOGIN3 only on it — verified; a missing PHASE_AUTH hangs the real client) → LOGIN3 68B → login validation (trim/lower, valid-login-string, NOID/ALREADY guards, lang UPDATE before validation — `input_auth.cpp` parity) → password verified against PostgreSQL (hash `'*'+UPPER(SHA1(SHA1(pw)))` computed in Rust) → `GC_AUTH_SUCCESS` (0x96, bResult + dwLoginKey unique 1..INT_MAX, panama key XOR) | DB errors → deterministic bResult=0. **Global connection timeout (15s, F1.5 debt).** Driver: **tokio-postgres** (decision documented in auth.rs — proven in the proxy; sqlx stays the F3 candidate). `protocol::legacy` (ADR-0006 → Accepted): PanamaPack 151/289B + hybrid-crypt 152/153 + SDB, runtime-file conditional (panama/ + cshybridcrypt* — parity: the C++ auth sends none without files). **Workspace 140/140 tests** (protocol 35, network 25, mysql_proxy 60, server_realms 20).
- Hybrid test harness: `scripts/gpg/hybrid_auth_test.sh` (swap auth C++ ↔ Rust on :30001, --restore).

### Verified

- f16_peer ↔ Rust auth (WSL+Windows): handshake → PHASE_AUTH → LOGIN3 → `GC_AUTH_SUCCESS key=0x58caefd5 result=1` — LOGIN OK; `account.lang` updated to `es` in PG by the Rust auth.
- **REAL client login against the Rust auth → character select screen** (hybrid stack: Rust auth :30001 + C++ channel :30003 on PG): auth log `login OK test key 1948777150` / `key 404922110` (lang es + en), core1 `LoginSuccess` 00:03:42, CHARACTER COUNT polling active.
- NPC motion errors (Uriel/Mirine animations) audited: **pre-existing data gaps** (baseline MariaDB syserrs from 08-08; `mob_proto.folder=''` for the 20000+ custom NPCs, 1144 races total; `ice_keybox` folder missing on disk) — NOT PG-related; fix = data lane (set folder from the client pack or an existing human folder + core restart), pending.

### Pending

- F2a debt: dwLoginKey real flow (LoginKeyStore skeleton done — the real flow still re-sends the password, AGENTS.md §14); capture harness (F0, tcpdump golden tests); F2b (client batch 1: version check/hardware ID/server time, C++ additive ≤1 week each, ADR-0007); NPC motion data fix (folder audit).

## [2026-08-10] (10th part) — G-PG cutover COMPLETE + F1.6 verified (loop, 5 attempts)

### Added

- **G-PG design closed:** [ADR-0005](../docs/decisions/0005-postgresql-cutover-and-legacy-adapter.md) → **Accepted** with the gate checklist resolved (4/4) and implementation backlog B1–B8; cutover spec `docs/plans/server-rewrite.md` §8.2.1 (a provision, b migration, c adapter, d harness); inventories marked Accepted.
- **PostgreSQL 18.4 (PGDG) provisioned** on WSL Debian-M2: db `metin2`, schemas `account`/`player`/`common`/`log`, role `mt2` (owner, scram, no SUPERUSER); runbook chain vendored in `scripts/gpg/` (02-install … 09-final).
- **Phase-1 migration executed:** 30 tables with data (account, player subset incl. proto tables, guild/marriage/war_reservation, common locale/priv_settings/exp_table/spam_db/gmlist/gmhost, item/quest/affect/safebox, item_award) + the 26 `log` tables DDL-only (empty) + `account.mysql_hash_password` (pgcrypto, verified `*A4B6157319038724E3560894F7F932C8886EBFCF`). Counts parity 30/30 (`scripts/gpg/parity_check.py`; volatile `account.last_play` excluded — live-login write lands on PG only).
- **`mysql_proxy` adapter** (`source/reforge/mysql_proxy`): MySQL wire v10 codec hand-written, `translate` (rewrites per `legacy-sql-compatibility.md` §4 as unit-test table), `session` (tokio-postgres 1:1, per-slot `search_path`, `standard_conforming_strings=off`, TimeZone), minimal TOML config, own SHA-1. 53 tests (workspace 111/111).
- **`f16_peer`** (`source/reforge/network/examples/f16_peer.rs`): F1.6 integration peer (GC_PHASE/GC_HANDSHAKE echo with server-aligned clock + optional LOGIN3) + 2 smoke tests against a local fake-auth.
- **Gate harness:** `scripts/gpg/parity_check.py`, `scripts/gpg/parity_boot.sh` (--only-baseline/--only-pg; A/B SYSERR + boot-lines + LoginSuccess; restores confs and stops at exit), `scripts/gpg/start_pg_stack.sh`.

### Fixed

- **4 gate bugs in `mysql_proxy`** (found by the real C++ boot): (1) text-row cell-count lenenc prefix (binary-protocol artifact) → ERR 2000 on 2+ column results and wrong values (2864 → 4); (2) `SET AUTOCOMMIT` (and other MySQL config SETs) passed to PG → no-op now; (3) session init refactored to a pure `init_statements()` applied before auth OK — a failed init never serves queries; (4) per-connection debug logging (`--debug`/`MYSQL_PROXY_DEBUG=1`).
- **Migration gaps found by the gate:** item/quest/affect/safebox, guild/guild_war_reservation/marriage, exp_table/spam_db/gmlist/gmhost, item_award (character select), 26 log tables.

### Verified

- **parity_boot A/B green** on the PG run (0 new SYSERR, boot table lines identical vs the MariaDB baseline).
- **REAL client login on PostgreSQL:** `test`/`1234` → character select (3 characters read from PG) — `LoginSuccess` 21:39:34 (core1 syslog); auth syslog `QID_AUTH_LOGIN: SUCCESS`; proxy log shows the translated login queries (`SELECT mysql_hash_password('1234'), a.id, ... FROM account a LEFT JOIN player.player_index ...` 13 cols, `availDt > LOCALTIMESTAMP`, `EXTRACT(EPOCH FROM ...)`, `UPDATE account SET last_play=LOCALTIMESTAMP`, `SELECT ... FROM player WHERE account_id=1` → 3 rows).
- **F1.6 transport:** `f16_peer 172.25.104.175 30001` ↔ auth C++ live — GC_PHASE + GC_HANDSHAKE (nonce 0xec4f82ac, clock-aligned echo) → handshake completed, no timeouts, no WRITE floods.
- Stack runtime state: srv1 (db/auth/core) operating on PostgreSQL through the proxy (`*_pg` conf variants active); MariaDB untouched (frozen migration source); `sync` verified.

### Pending (F2a follow-ups, non-blocking)

- `mysql_proxy` translation gaps (queued): 22P02 `ORDER BY (mValue)::bigint` (game boot), 42703 `LIKE "LOCALE"` (double-quoted string), 22021 NUL byte in a player INSERT.
- sqlx/PgPool concrete decision for the `database` crate (F2a start); F1.5 debt (retry-on-wrong-nonce rationale, partial-echo test); real capture harness (F0).

## [2026-08-10] (9th part) — Checkpoint: G-PG database inventories committed + CURRENT updated

### Added

- **`docs/reference/database/legacy-schema.md`** — reproducible inventory of the live MariaDB schema (77 tables, column types/encodings, triggers, hash function, AI counters, hazards) as the PostgreSQL migration baseline. Type Reference, Status Proposed.
- **`docs/reference/database/legacy-sql-compatibility.md`** — inventory of MySQL-specific SQL (204 submission sites; REPLACE/ON DUPLICATE/UPDATE LIMIT/collate/escaping/multi-statement hazards; connection topology; pgcrypto hash expression) for the temporary C++→PostgreSQL adapter. Type Reference, Status Proposed.
- `docs/reference/README.md` — link to the database reference section.

### Verified

- Both inventories audited twice by oracle-fixers against the live MariaDB/WSL and `source/server` (0 blockers after corrections; final counts and citations verified).
- `docs/CURRENT.md` refreshed: snapshot commit `03c03ad`, harness state, next gates (G-PG implementation → F2a).
- Working tree clean after this checkpoint.

## [2026-08-10] (8th part) — Team redefinition: build implements, fixer attacks, oracle supervises, librarian maintains docs

### Changed

- **Team hierarchy (user decision):** `Orchestrator → Oracle (supreme supervisor, second only to the orchestrator) → build (implementer) → fixer (build's adversary) → librarian (documentation maintainer)` + explorer/observer/designer.
- **`.opencode/agents/fixer.md`** (local, gitignored): rewritten as **build's adversary** — read-only reviewer that attacks build's code, says what was done wrong, checks plan alignment and docs; mandatory numbered report. (Was: implementation specialist.)
- **`.opencode/agents/oracle.md`**: rewritten as **supreme supervisor** — meta-review of the whole change (code + docs + plan + gates), catches what the fixer missed, gate verdict (ready to commit / not ready). (Was: per-deliverable adversary.)
- **`.opencode/agents/librarian.md`**: permission `edit: allow`; mission = **documentation maintainer** — audits AND edits docs (applies its own audit fixes), owns doc upkeep, may propose policy improvements. (Was: read-only auditor.)
- **`~/.config/opencode/oh-my-opencode-slim.json`**: fixer skills rebalanced to the adversarial set (grill-me, caveman-review, simplify, ponytail-review, diagnose, clean-code, clean-code-principles, verification-before-completion, cpp-pro, rust-best-practices, rust-testing).
- **`docs/explanation/agent-organization.md`**: hierarchy diagram + roster + standard flow (build → fixer → build fixes → oracle → librarian → orchestrator commits) rewritten for the new model.
- **`docs/DOCUMENTATION.md` §10**: librarian maintains (audits + edits), fixer = code adversary, oracle = supreme supervisor.
- **`docs/guardrails/agent-operations.md`**: fresh-session and report-instruction rules now apply to **every reviewer** (oracle, fixer); reuse rule points at build (implementer).
- **`AGENTS.md` rule 10**: delegation map updated (@build implements, @fixer reviews, @oracle supervises).

### Verified

- Config JSON valid; the 3 agent definition files rewritten; docs links intact. Requires opencode restart to load.

## [2026-08-10] (7th part) — Agent harness: models, skills and team organization

### Added

- **`docs/explanation/agent-organization.md`** — the agent team organization: roster (roles, models, skills), standard per-lane flow (fixer → oracle-fixer → oracle general → verify → commit), spawn/reuse rules, gates and the loop protocol.
- **`docs/guardrails/agent-operations.md`** — operational lessons with evidence: fresh session for every oracle review (resumed oracle sessions returned empty 3+ times on 2026-08-10), mandatory "report in final message" instruction in oracle prompts, reuse fixer sessions within scope, reconcile every lane, distinct task labels (board inherited stale objectives), disjoint write scopes for parallel lanes, opencode restart after config changes.
- **Rust skills installed globally** for the build lane: `rust-best-practices` (apollographql), `rust-async-patterns` (wshobson), `rust-testing` (affaan-m).

### Changed

- **`~/.config/opencode/opencode.jsonc`** (outside the repo): built-in `build` agent now uses `opencode-go/deepseek-v4-flash` variant max, same as orchestrator/explorer/librarian/fixer.
- **`~/.config/opencode/oh-my-opencode-slim.json`** (outside the repo): skill sets rebalanced per role (below).
- **`docs/DOCUMENTATION.md` §10**: extended with the team organization links and the session-discipline rules.
- **`AGENTS.md`**: Work rules now point to the team organization and operations guardrails.

### Skills rebalance (why)

- **oracle**: removed C++ debugger skills (`gdb`, `strace-ltrace`, `sanitizers`) — not the role; added the adversarial-review skills that match its job: `grill-me`, `caveman-review`, `simplify`, `ponytail-review`. Kept `diagnose`, `cpp-pro` (reads legacy C++ evidence), `architecture-designer`, `improve-codebase-architecture`, `verification-before-completion`, `documentation-and-adrs`.
- **fixer**: removed `cmake` (build is Makefiles/MSBuild) and `sanitizers`; added `rust-best-practices`, `rust-async-patterns`, `rust-testing` — the active work is Rust (tokio/sqlx/cargo). Kept `cpp-pro` + `make` for the temporary C++ adapter (G-PG) and the legacy baseline.
- **explorer**: added `graphify` (graphs-first rule) and `codemap` (legacy C/C++ maps).
- **librarian**: already carries `documentation`, `documentation-writer`, `documentation-and-adrs`, `find-skills` (previous session).

### Verified

- `npx skills add` reports the 3 Rust skills installed for OpenCode (the PromptScript integration "failed to install" line is cosmetic — that target does not support global installs; OpenCode universal install succeeded).
- Config files are valid JSON/JSONC; they load at opencode startup only — a restart is required to take effect.

## [2026-08-10] (4th part) — Documentation reorganization (docs hubs, plan reorder, ADR-0005/0006/0007)

### Added

- **Documentation hub restructured** (final layout; the hub files are owned by the documentation lanes, the reorg is coordinated):
  - `docs/README.md` — documentation index (entry point to all docs).
  - `docs/CURRENT.md` — current verified state of the project.
  - `docs/DOCUMENTATION.md` — documentation rules and workflow (Keep a Changelog format, ADR template, graph workflow).
  - `docs/plans/server-rewrite.md` — canonical design reference (replaces `docs/history/2026-08-09-server-rewrite-draft.md`, preserved as historical).
  - `docs/reference/protocol/login-flow.md` — byte-exact login wire spec (moved from `docs/superpowers/specs/2026-08-08-wire-protocol-login-flow.md`).
  - `docs/reference/protocol/legacy-compatibility.md` — legacy wire/pack compatibility boundary (ADR-0006).
  - `docs/reference/quests/quest-dsl.md` — quest DSL spec (moved from `docs/superpowers/specs/2026-08-09-quest-dsl-spec.md`).
  - `docs/how-to/`, `docs/tutorials/`, `docs/explanation/`, `docs/decisions/`, `docs/history/`.
- **ADR-0005 (Proposed)** — `docs/decisions/0005-postgresql-cutover-and-legacy-adapter.md`: PostgreSQL cutover (phase G-PG) + temporary legacy compatibility adapter; **F2 is gated by it**. Not accepted yet — needs confirmation.
- **ADR-0006 (Proposed)** — `docs/decisions/0006-legacy-wire-pack-compat-boundary.md`: legacy wire/pack compatibility boundary — PanamaPack (151, 289B) and hybrid-crypt (152/153) isolated in `protocol::legacy`, never in the new wire core; boundary documented in `docs/reference/protocol/legacy-compatibility.md`; deleted at the new client (F7).
- **ADR-0007 (Accepted — only the already-agreed boundary)** — `docs/decisions/0007-no-partial-rust-in-legacy-client.md`: no partial Rust embedded in the legacy client during F0–F6; the Rust client ships standalone (Slint standalone in F5, wgpu client in F7). Everything else about the new client remains open (own ADRs at F7).

### Changed

- **Root `README.md`:** translated to English; concise current status linking to `docs/README.md` and `docs/CURRENT.md`; final workspace names (`source/reforge`: `protocol`, `network`, `database`, `realm` + binary `server_realms` with `auth|channel` roles); architecture section trimmed (no duplicated design — points to `docs/plans/server-rewrite.md`).
- **`ROADMAP.md`:** translated to English; **plan reorder — G-PG (PostgreSQL cutover) before F2**; F2 split into **F2a** (server-side auth) / **F2b** (client batch 1) and **blocked until the PostgreSQL cutover + ADR-0005**; compatibility packets isolated in `protocol::legacy` (ADR-0006); no partial Rust embedded client (ADR-0007); dependency deferrals documented (clap/config-rs → F2, sqlx → G-PG/F3, bevy_ecs → F4, no mlua ever); links updated to `docs/plans/server-rewrite.md`, `docs/reference/protocol/login-flow.md`, `docs/reference/protocol/legacy-compatibility.md`, `docs/reference/quests/quest-dsl.md`; graph counts updated to **server 13,200/33,251, client 17,501/39,258, merged 30,701/72,509**. F0/F1 actual evidence preserved; **G-PG and F2 NOT marked done**.
- **`AGENTS.md`:** translated to English; repository layout and documentation workflow updated to the new docs structure; all safety/build rules, protocol facts, runbook, crash history and the graph workflow preserved; documentation rules now point to `docs/README.md`, `docs/CURRENT.md`, `docs/DOCUMENTATION.md`.
- **ADRs 0001–0004:** metadata headers added (`Status`/`Date`/`Supersedes`/`Superseded by`); ADR-0003 links updated to the new plan/spec paths (old ones noted as historical); decisions unchanged.
- **Documentation policy:** docs are written in English going forward; old decisions/plans marked historical/superseded, never deleted (no-hide-history rule).

### Verified

- All links in the five owned files (README, ROADMAP, CHANGELOG, AGENTS.md, `docs/decisions/*`) point to the final docs target paths.
- ROADMAP F0/F1 checkbox evidence untouched (56/56 tests, F1.1–F1.5 acceptance criteria); G-PG/F2 left unchecked; ADR statuses explicit (0001–0004 Accepted, 0005/0006 Proposed, 0007 Accepted for the already-agreed boundary).
- No source code touched on this lane — documentation-only change; the other docs lanes' files (hubs, renames) are separate work in the same worktree.

## [2026-08-10] (6th part) — Docs audit: guardrails, metadata normalization, hub sections

### Added

- **`docs/guardrails/`** — new section with 6 files, each rule structured as Rule / Why / Evidence / Consequence / Status (policy `docs/DOCUMENTATION.md` §3.1):
  - `README.md` (Hub index), `rust-rewrite.md` (property boundary, two source copies, ADR-before-code, tests/evidence, minimal deps, no partial Rust in client), `legacy-compatibility.md` (PanamaPack is a wire packet not a library/EIX/EPK, `protocol::legacy` temporary, single canonical PostgreSQL, legacy client contract), `data-and-encoding.md` (CP949, `PROTO_FROM_DB`, `item_proto` names, PostgreSQL encoding, units vs cells), `operations.md` (WSL memory, boot order, `sync` after deploy, IP check, no artifacts in git), `world-entry-crash.md` (0xC0000374 postmortem, closed 2/2, diagnostic lessons).

### Changed

- **`docs/DOCUMENTATION.md`** — `Type: Hub`; metadata scheme extended: `Type: Tutorial | How-to | Reference | Explanation | Plan | Decision | Guardrail | History | Hub | Snapshot`; `Status: Current | Proposed | Accepted | Superseded | Historical`; document-kinds table (Plans/Decisions/Guardrails/History/Hub/Snapshot); guardrail rule structure §3.1; no-empty-Diátaxis-dirs rule; documentation workflow §10 (librarian audits → fixer applies → oracle reviews → orchestrator commits).
- **`docs/README.md`** — hub rewritten with visible **Plans / Decisions / Reference / Guardrails / History** sections; empty `tutorials/`/`how-to/`/`explanation/` links removed (documented as on-demand only); reader directed to CURRENT/ROADMAP/CHANGELOG.
- **ADRs 0001–0007 normalized** — consistent YAML frontmatter (`Type: Decision`, `Status`, `Audience`, `Date`, `Last verified`, `Supersedes`, `Superseded by`); **active ADRs translated to English without changing decisions** (0001, 0002, 0003, 0004 were Spanish); 0001 note on ADR-0005 refinement kept; 0002 note on ADR-0004 process-topology refinement added; 0003 folder list corrected (`source/tools/pack`, not `source/pack`).
- **`docs/CURRENT.md`** — `Type: Snapshot`; docs-structure line updated (no empty Diátaxis dirs listed).
- **`docs/plans/server-rewrite.md`** — `Type: Plan`.
- **`AGENTS.md`** — `docs\` layout row and methodology updated: `guardrails/` added, empty Diátaxis dirs marked on-demand, no empty-dir policy.
- **`ROADMAP.md`** — "How the count is kept" adds `docs/guardrails/`.
- **`README.md` (root)** — docs tree line updated (no empty Diátaxis dirs listed).

### Verified

- **Relative-link scan over `docs/**` + root markdown: 0 broken links.**
- Backtick-path scan: 53 flagged, all explained — policy mode names (`docs/tutorials/` etc., on-demand), brace expansion (`source/{client,server,tools,deploy}`), explicit provenance (historical "Original location" paths under `docs/superpowers/`, the reverted `source/realms` rename note), and read-only historical content references. **No active path is broken.**
- Every guardrail file has complete metadata (`Type: Guardrail`, `Status`, `Audience`, `Last verified`) and linked evidence.
- No link points to empty/missing categories; `docs/superpowers`, `docs/tutorials`, `docs/how-to`, `docs/explanation` directories do not exist and contain no files (nothing to remove).
- Content consistency preserved: single canonical PostgreSQL + adapter (ADR-0005), G-PG before F2, F2 blocked, `protocol::legacy` isolated, no partial Rust in legacy client; 56/56 tests, graph counts 13,200/33,251 + 17,501/39,258 + 30,701/72,509; crates `protocol`/`network`/`database`/`realm`/`server_realms`; F1.6 pending; G-PG/F2 not marked done.

## [2026-08-10] (5th part) — Documentation reconciliation (oracle findings)

### Added

- **`docs/history/2026-08-09-server-rewrite-plan-v0.2.md`**: the original Spanish plan v0.2 **body** restored byte-identically from `HEAD:docs/superpowers/plans/2026-08-09-servidor-rust-plan-unico.md` (verified via the original blob hash `7a108e754229ef...`); migration metadata/provenance was added separately and the body remains non-normative. `docs/plans/server-rewrite.md` links to it.
- **`docs/history/README.md`**: index of all historical documents (`Type: Hub`, `Status: Historical`).
- Historical documents received provenance metadata and limited link corrections; their technical history remains preserved and non-normative.

### Changed

- **Single canonical PostgreSQL (user decision 2026-08-10):** `ADR-0005`, `ROADMAP.md` (G-PG + F3) and `docs/plans/server-rewrite.md` now state: **one PostgreSQL**; the C++ baseline operates on the **same PG** through a temporary compatibility adapter (its MySQL `libsql` is bridged); MariaDB is used only as the **migration/export source**; no dual-store, no `direct-sql` backend, no "C++ stays on MariaDB during F2–F6".
- **`docs/plans/server-rewrite.md` + `ROADMAP.md` F4:** removed the invented `SetLocaleName`/`SetItemLocaleName` API (does not exist in the legacy client) → "new in-memory override API to be added around `CPythonNonPlayer`/`CItemData` after `LoadLocaleData`".
- **`docs/reference/protocol/login-flow.md`:** `GC_AUTH_SUCCESS` corrected to **S→C** (was C→S); `TSimplePlayer` size sum fixed to `4+25+1+1+4+4+4+1+4+4+4+4+4+4+2+1 = 71`.
- **`source/reforge/protocol/src/lib.rs` doc-comment:** contract path → `docs/reference/protocol/login-flow.md` (canonical); obsolete 76B/474B deviation narrative removed.
- **Paths:** `source\pack` → `source\tools\pack` in AGENTS.md, README.md, ROADMAP.md and active docs; `Mysql2Proto` added to the tools list (exists in `source/tools`).
- **QUERY_LOGIN:** columns 12 → **13** in AGENTS.md and ROADMAP (verified `ClientManagerLogin.cpp:395-426` + `CreateAccountTableFromRes:259-297`; `lang` column from the Language System ALTER).
- **World-entry crash state:** AGENTS.md/ROADMAP aligned with CHANGELOG — closed, field test 2/2 (2026-08-09).
- **Plan section refs:** ROADMAP §4.6→§5.6, §2.3→§3.3, §11.11→§11.
- **`docs/CURRENT.md`:** removed the claim that the docs snapshot is in commit `b85a019` → "documentation reorganization pending commit".
- **`docs/DOCUMENTATION.md`:** metadata scheme now allows `Type: Hub | Snapshot` and `Status: Historical` (used by the hubs and history index).
- **`README.md`:** license badge/link → "License: pending decision" (no `LICENSE` file exists; MPL-2.0 still Proposed).
- **`CHANGELOG.md` language note:** corrected — the 2026-08-10 1st–3rd parts are also Spanish; only the 4th part and the new English docs follow the English rule.

### Verified

- Relative-link scan over `docs/**` + root markdown: **0 broken links**.
- Grep: no active `source\pack`; no `SetLocaleName` as an existing API (only negative mentions); no active dual-store/`direct-sql (MariaDB)`; no `12 columns` in active docs (only the historical 2026-08-08 changelog entry, preserved verbatim); `docs/superpowers` appears only as provenance/history policy.
- v0.2 body restoration verified byte-identical against the original blob (`7a108e754229ef378605a2fe7216f7c2b185035d`); the current historical file additionally contains the approved migration metadata.

## [2026-08-10] (3ª parte) — F1.5 handshake + binario `server_realms` + config TOML

### Añadido

- **F1.5 — Handshake** (`network/src/handshake.rs`, 597 líneas, 11 tests nuevos): `perform`/`perform_with` sobre `Connection`+`Framer` — envía `GC_PHASE(HANDSHAKE)`+`GC_HANDSHAKE` (nonce u32 nunca 0, now32 con wrap 2^32, l_delta=0), valida el eco `CG_HANDSHAKE` (nonce, l_delta≥0 parity desc.cpp:693-697, bias |±80ms| simétrico vs [0,50] unilateral del legacy), retries ≤32 con timeout 500ms/intento y respiro 50ms, filtra keepalives (0xfc/0xfe) y descarta paquetes fuera de orden (parity input.cpp:625-626). **Cancelación por timeout demostrada segura** (solo descarta reads Pending; los bytes parciales quedan en `framer.buf`). **56/56 tests** (network 23/23, protocol 30/30, server_realms 3/3), 0 warnings de build (clippy: 4 warnings pre-existentes de F0 en `protocol` — indentación de doc-list, args de función, identity_op en test; no de esta sesión).
- **Binario `server_realms`** (antes `server`; nombre provisional del usuario): `git mv` + package renombrado, members/README actualizados, smoke verificado (roles auth/channel exit 0; rol inválido exit 2).
- **Configs: TOML** (decisión del usuario 2026-08-10): configs de `server_realms` en TOML vía config-rs (F2; clap para args). Registrado en ADR-0004.

### Corregido

- **Carpeta legacy**: el renombre intermedio a `source/realms` se REVIRTIÓ por corrección del usuario — `source/deploy` conserva su nombre; `realms` queda solo como sufijo del binario (`server_realms`).
- `.gitignore`: vuelve a `source/deploy/` (el runtime renombrado quedaba sin ignorar — 50 MB).
- Artefactos residuales del binario viejo (`target/debug/server*.exe/pdb`) eliminados.

### Verificado (reviews adversariales del equipo: 3 fixers → 3 oracles-fixer)

- Ora-7 (rename server_realms): ✓ 7/7, sin hallazgos ≥ baja.
- Ora-8 (docs deploy/server_realms): ✓ 8/8, consistencia disco↔docs verificada; 0 residuales de `source/realms` como ruta activa.
- Ora-10 (handshake F1.5): **✓ LISTO para F2** — byte-parity del wire verificada contra el cliente real; cancelación por timeout demostrada por análisis del código. 7 hallazgos de deuda conocida (abajo).

### Pendiente (deuda conocida de F1.5, no bloqueante — para el kickoff de F2)

- **Racional del retry-on-wrong-nonce incoherente** (handshake.rs:64-67, 254-256): el nonce es fijo entre intentos → un eco duplicado lleva el MISMO nonce y se acepta; el camino solo lo dispara corrupción/malicia, donde el close instantáneo del C++ (input.cpp:179-183) es mejor que 32×(500+50)ms ≈ 17.6s de conexión zombie. Corregir doc o revertir a parity en F2.
- **`Handshake.delta ≈ 0` SIEMPRE con el cliente legacy** (el cliente hace eco de `dwTime + 2·lDelta` con lDelta=0 → bias=0): la doc que dice "el bias real es ~la latencia" es incorrecta; y el mecanismo bias/retry NO converge para peers no-legacy con reloj desviado (el C++ converge con lNewDelta; el Rust reenvía now32/lDelta=0). F2 y el futuro cliente Rust no deben heredar esa suposición.
- **Gap de test**: eco parcial (5 de 13 B) a través del timeout — la propiedad más delicada, analizada correcta pero sin test que la fije.
- Off-by-one vs C++: 32 intentos Rust vs 33 envíos legacy (1 inicial + 32 retries); `Handshake.server_time` es el now_ms del inicio (stale tras retries, el C++ fija el del éxito); `unreachable!()` en ruta de red (demostrablemente seguro hoy); 17.6s de vida de conexión muda (el auth público debe considerar timeout global en F2).

## [2026-08-10] (2ª parte) — Estructura y nombres profesionales (ADR-0004) + layout plano

### Añadido

- **ADR-0004** (`docs/decisions/0004-reforge-structure-and-names.md`): estructura y nombres definitivos del workspace — layout PLANO en `source/reforge` (el subdirectorio `crates/` se propuso y el usuario lo descartó), renombres de dominio (sin prefijo de marca), binario único `server_realms` con roles por config, convenciones de workspace, runtime legacy `source/deploy` (sin cambio).
- **Crate binario `server_realms`** (`source/reforge/server_realms/`, nombre provisional del usuario): UN binario con roles por config — `--role auth` (F2) | `--role channel` (F5); main mínimo con std (sin clap), `parse_role` puro con 3 tests. Resuelve la inconsistencia ADR-0002 ("auth proceso propio") vs plan único ("auth modo del binario"): auth es un ROL del mismo binario.
- **Convenciones de workspace**: `[workspace.dependencies]` (tokio 1.49 → resuelve 1.53.1, features centralizadas), `[workspace.lints.rust] unsafe_code = "forbid"` heredado en los 5 crates (`[lints] workspace = true`), `rust-toolchain.toml` (1.97.0), `README.md` del workspace (estructura + glosario).

### Cambiado

- **Renombres de crates** (git mv, historial preservado): `net` → `network`, `db` → `database`, `game` → `realm`, `protocol` sin cambio; crate `auth` eliminado → módulo `network::auth` (stub F2).
- **Framer de network**: tabla C→S ampliada con `CG_ENTERGAME` (10 → 1B) y `CG_STATE_CHECKER` (206 → 1B, constante añadida a `protocol::header`); doc-comment con los matices (0x00 divergencia deliberada vs no-op del C++ `input.cpp:75-76`; EnterGame/StateChecker para F2/F4; sin idle timeout hasta F2). Test nuevo `entergame_and_state_checker_are_1_byte_packets`.
- **Runtime legacy: conserva `source/deploy`** (copia Windows del runtime, gitignored; el árbol WSL `metin2_svfiles` NO se toca — los scripts dependen de esa ruta). El renombre intermedio a `source/realms` se revirtió por corrección del usuario; `realms` queda solo como sufijo del binario de la reescritura (`server_realms`).

### Corregido

- **`.gitignore`**: `source/realms/` → `source/deploy/` (revertido tras corrección del usuario — el runtime legacy conserva su nombre; la regla vuelve a cubrir `source/deploy/`).

### Verificado

- **45/45 tests** (`cargo test` workspace): protocol 30/30, network 12/12, server_realms 3/3, database/realm 0. Build 0 warnings (debug + release fresco). Smoke del binario: roles auth/channel exit 0, rol inválido/flag desconocido/valor faltante exit 2.
- Review adversarial (oracle-fixer A): 0 críticos; 2 MEDIA corregidos (lints heredados en database/realm — falso positivo, ya estaban — y gitignore); lista para commit.
- **Corrección del usuario (misma fecha):** el runtime legacy conserva `source/deploy` (el renombre a `realms` se revirtió) y el binario de la reescritura pasa a llamarse `server_realms` (nombre provisional; carpeta `source/reforge/server_realms` albergará el binario compilado + configs desde F2). Docs, ROADMAP, ADR-0004 y `.gitignore` actualizados en consecuencia.

## [2026-08-10] — REESCRITURA RUST ARRANCADA: ADR-0003 + workspace `source/reforge` + crate `protocol` (F0)

### Añadido

- **ADR-0003** (`docs/decisions/0003-reforge-workspace-rust-layout.md`): el servidor Rust vive en `source/reforge` (carpeta nueva, mismo repo `reforge-core`), workspace con crates `protocol`/`net`/`db`/`game`/`auth`, edition 2024, `protocol` zero-deps, límite de propiedad: nadie toca la línea base C++ desde esta línea de trabajo.
- **Workspace Cargo** en `source/reforge` (5 crates, `cargo build` OK) + `**/target/` en `.gitignore`.
- **Crate `protocol` implementado** (`source/reforge/protocol/src/lib.rs`, ~1.7k líneas, zero-deps): 17 paquetes del flujo de login del spec §3 (handshake, login/login2/login3 65/68B, phase, auth success/failure, login key, empire, login success 449B + TSimplePlayer 71B, character add 37B, additional info 70B, player select/delete/create) con parseo sin panic (longitud incorrecta → `ProtocolError::BadLength`, también slices largos), LE manual, helpers C-string strlcpy (fix `saturating_sub` anti-panic), constantes de headers verificadas contra `packet.h`.
- **30/30 tests** (`cargo test -p protocol`): golden byte-vectores manuales (login3 65B/68B, login success 449B con offsets críticos handle@441/random_key@445/skill_group@70, character add, additional info, handshake, phase, auth success, login failure), roundtrips de todos los paquetes, `wire_sizes`, `bad_lengths_are_errors`.
- **Review adversarial de 2 vueltas (oracle)**: contrato antes de escribir + código después — sin fallos críticos; 2 huecos MEDIO diferidos a fase (keepalive TIME_SYNC/PING → F1; PanamaPack 151/289B + hybrid-crypt 152/153 → F2).

### Corregido (spec del wire protocol — errores que habrían roto la paridad)

- **`TSimplePlayer` es 71B packed, no 76B natural** (`tables.h:271` abre pack(1) antes del struct; evidencia dual-toolchain gcc -m32 y MSVC x86, ambos 71B) → **`TPacketGCLoginSuccess` = 449B (handle@441, random_key@445), no 474B**; `TAccountTable` = 444B. El cliente real ya coincide (449B en producción).
- `TPacketGDAuthLogin` = 110B (no 100B); SQL del auth = 15 columnas (no 13); `HEADER_GC_LOGIN_FAILURE=7`/`HEADER_GC_LOGIN_KEY=118` añadidos; bug conocido del cliente (registra LOGIN_FAILURE con 6B) documentado.
- Todas las correcciones aplicadas en el cuerpo del spec + sección «Erratas 2026-08-10» (§7) con pendientes por fase.

### Pendiente

- Harness de captura real (tcpdump contra server C++ en WSL) para cerrar el hito F0 con evidencia de red — requiere stack arriba.
- F1 (net): listener tokio + keepalive; F2 (auth): PanamaPack + constructores + validación de header por dispatch.

## [2026-08-09] (3ª sesión, 4ª parte) — Selector de banderas FUNCIONANDO + personajes viejos recuperados (2/2) + stack rearmado

### Resuelto

- **CRASH DE ENTRADA AL MUNDO — CERRADO (prueba de campo 2/2):** los personajes viejos del mapa 41 (lkjsnlfknlsk, ninja) tenían coordenadas basura `(957500,258241)`/`(959878,242236)` (fueron escritas por harness de sesiones anteriores). `UPDATE player SET x=969600, y=278400` (aldea c1, unidades) → **entradas 2/2 seguidas** con el cliente. Los 3 dumps WER de 18:49-18:50 (0xC0000374, confirmado con cdb) eran SIEMPRE con lkjsnlfknlsk — **no el idioma TR** (el servidor aceptó `lang 'tr' -> 15` + `LoginSuccess` correctamente).
- **Selector de idioma con banderas — FUNCIONANDO end-to-end** (login → `locale.cfg` → reinicio → LOGIN3 con el idioma → servidor):
  - Fix del header TGA generado (struct.pack con 6 H's en vez de 4 → width/height=0 → `Cannot GetImageInfo from texture` en syserr.txt del cliente). Header corregido a `20 00 18 00 20 08` (32×24, bpp32, desc 0x08) idéntico al `choise_close.tga` del pack.
  - Fix pantalla negra: `ui.__mem_func__` sobre closure rompía `LoginWindow.Open()` → SetEvent directo (como las lambdas del VK) + try/except blindado.
  - Posición final: anclada al **SaveAccountBoard** (`y = saveAccountBoard.y - 30`), no al LoginBoard — el SAB está más arriba.
  - TR probado por el usuario (entra con el fix de coordenadas).
- **Stack caído y rearmado** (22:23): mariadb + db + auth + core1 levantados con `start_m2_min.sh` (puertos 30000-30004 OK, RAM 617MB/2GB). La BD cayó por el socket pero el demonio seguía en TCP — `mysql -h 127.0.0.1` lo confirmó.

### Pendiente

- Vigilar estabilidad (2/2 es buena señal pero el usuario quiere más muestras).
- Evaluar si Debug build del cliente aporta algo (respuesta: no — ver abajo).

## [2026-08-09] (3ª sesión, 3ª parte) — Selector de idioma con banderas en el login + partición del crash (4/4 personaje nuevo)

### Añadido

- **Selector de idioma con banderas en el login** (pack root, SIN rebuild del cliente):
  - 16 idiomas (los que soporta el servidor, `LANGUAGE_AE..TR` de locale.hpp:20-36): ae, cz, de, dk, en, es, fr, gr, hu, it, nl, pl, pt, ro, ru, tr.
  - 32 imágenes TGA generadas (16 × normal + hover `_over`, 32×24, type-2 32-bit BGRA bottom-left, formato idéntico al `choise_close.tga` del pack) descargadas de flagcdn (w40, `en`→`gb` porque flagcdn no tiene ISO "en") → `pack/root/flag/`.
  - `intrologin.py`: `__CreateLanguageSelector()` (fila de `ui.MakeButton` centrada abajo del todo, `y = SCREEN_HEIGHT-45`, tooltip con el idioma en inglés) + `__OnClickLanguageFlag(lang, codepage, name)` → escribe `client\locale.cfg` (`"10002 <codepage> <lang>"`) y pide reiniciar el juego (el C++ solo lee locale.cfg al arranque; no hay reinicio in-process).
  - Codepages tomados de la tabla nativa `gs_stLocaleData` (Locale.cpp:235-263): ae 1256, cz 1252, de 1252, dk 1252, en 1252, es 1252, fr 1252, gr 1253, hu 1250, it 1252, nl 1252, pl 1250, pt 1252, ro 1250, ru 1251, tr 1254.
  - Repack `PackMakerLite.exe -p root` (538368 B, 18:04) desplegado a `client\pack\` y VERIFICADO desempaquetando: 32 banderas presentes + código del selector en intrologin.py.
  - **PENDIENTE de probar por el usuario:** abrir el cliente → fila de banderas abajo → click → reiniciar → textos del cliente en el idioma elegido.

### Resuelto (partición del crash de entrada al mundo)

- **Prueba de campo 4/4 entradas seguidas con personaje NUEVO** (mapa 0, "Chaman", id 3) → el crash `0xC0000374` NO es global ni del cliente: es de los DATOS de los 2 personajes viejos del mapa 41 (lkjsnlfknlsk id 1, ninja id 2). Inspección BD: items normales (vnums válidos, sin vnum 0 con count, sin counts >200), sin quests, sin affects — el estado del personaje en BD se ve limpio; la causa más probable restante es la posición `(957500,258241)`/`(959878,242236)` en el mapa 41 (fuera de la aldea c1 `969600,278400`) o un dato no inspeccionado. Próximo paso si el usuario quiere recuperar esos personajes: `UPDATE player SET x=969600, y=278400` en ambos (posicionarlos en la aldea) y reintentar; si sigue, borrar y recrear.

### Arreglado (mismo día, 18:12 — pantalla negra en el login)

- **El selector de banderas causó pantalla NEGRA al abrir el login** (primera versión 18:04): `btn.SetEvent(ui.__mem_func__(self.__OnClickLanguageFlag(...)))` envolvía una **closure** con `__mem_func__` (wrapper pensado para métodos bound estilo `self.__OnClickLoginButton`) → excepción en `__CreateLanguageSelector` durante `LoginWindow.Open()` → el login no se construye → negro. **Fix:** `SetEvent` directo con la closure (igual que las lambdas del teclado virtual, `key_space.SetEvent(lambda ...)`) + **try/except blindado** en `__CreateLanguageSelector` (`print` del error, el login se muestra igual aunque el selector falle). Repack 538368 B 18:12, desplegado a `client\pack` y verificado por desempaquetado (línea 379 sin `__mem_func__`, 32 banderas dentro del epk).
- **Verificado el `.rar` del sistema completo** (`systems\Language System 1.2.6.rar`, UnRAR l): contenido idéntico a la carpeta extraída, **sin ninguna imagen de bandera de país** y sin lógica de selector de login. Los 8 `02. Client\root\*.py` del mod son parches del coliseo PVP (dependen de `__LANGUAGE_SYSTEM__` en el C++ del cliente, no integrado) — **copiarlos rompería el login** (ImportError `uiLanguageSystem`, AttributeError `app.LANGUAGE_SYSTEM`, `player.IsLanguageSystem()` inexistente). Confirmada la decisión #8 del doc de estado (no integrar ese root).

## [2026-08-09] (3ª sesión, 2ª parte) — Crash de entrada al mundo: diagnóstico en curso + auditoría del Language System

### Arreglado

- **`string_replace_word` over-read — CORRUPTOR REAL confirmado y arreglado** (pero NO es el único; ver "En curso"): el over-read de `memcmp(base+cur, src, src_len)` (PythonSkill.cpp:62) fue confirmado por los minidumps del cliente (13:15, con AppVerifier: AV `0xC0000005` en 0x495110, ECX=0x96510FFD) y arreglado con bounds check `cur+src_len <= base_len` (PythonSkill.cpp:72-90, build 14:12, hash C7EAD7CC desplegado).
- **Diagnóstico del crash CON herramientas definitivas (cdb instalado):**
  - Dumps WER completos de 14:45-14:46 (466MB c/u, LocalDumps): `0xC0000374` (heap corruption) en ntdll, stack del hilo principal: `metin2client!CPythonMiniMap::Render → CStateManager::DrawIndexedPrimitive → d3d9!CreatePixelShader → igdumdim32!GTPIN_IGC_Instrument → ntdll!RtlAllocateHeap`; hilo 0:015 (pool del driver Intel): `igc32!OpenCompiler9` compilando shaders → detecta el heap ya corrupto.
  - cdb en vivo (15:25): capturó `0xC0000374` detectado en `granny2.dll` (alocando 0x552 B, heap 0x00cc0000, bloque 0x1a722638) — **distinto detector, mismo heap dañado**.
  - **Conclusión:** overflow determinista del cliente durante la carga del mundo (entre login y entrada); la DETECCIÓN depende del layout del heap (ASLR) → intermitente (~75%). Los detectores (igc32, granny2) son víctimas, no culpables.
  - **Estado: NO RESUELTO.** El usuario logró 5/5 entradas seguidas sin instrumentación (buena señal, posible reducción de frecuencia con el fix de string_replace_word, pero la corrupción subyacente sigue: cdb la detectó en la misma ventana).
  - Herramientas ahora instaladas y configuradas: **Debugging Tools (cdb/WinDbg x86)** en `C:\Program Files (x86)\Windows Kits\10\Debuggers\`, LocalDumps full → `C:\dumps`, PageHeap vía gflags. **Próximo paso si reaparece:** `!heap -p -a <bloque>` sobre el dump nuevo (stack de asignación del bloque corrupto) + prueba de campo personaje nuevo en mapa inicial (particionar mapa 41/GM vs bug global).
  - Lección registrada: el syserr del servidor NUNCA verá crashes del cliente (memoria local); los errores del cliente están en `client\logs\*.dmp` (EterExceptionFilter) o `C:\dumps` (WER LocalDumps).

### Auditoría completa del Language System (cliente + servidor + pack)

- **Servidor: 11/11 archivos del doc `docs/reference/legacy/language-system.md` §4 verificados en el código actual.** Motor vivo (`g_iUseLocale=TRUE`), runtime 16 idiomas desplegado, `account.lang='en'` en BD (el cliente lo sobrescribió al loguear en EN — comportamiento por diseño).
- **CORRECCIÓN DE DATO ERRÓNEO de esta misma sesión:** el EN del runtime **cubre el 100% de las claves de ES (0 faltantes, 11 extra)**. El análisis previo de "732 claves ES sin cubrir por EN" fue un **error de parseo** (contaba líneas con comillas, no pares clave→valor). El EN estaba completo; la mezcla ES/EN que vio el usuario tiene otras causas (ver huecos B y C abajo).
- **Huecos reales del servidor** (lo que falta para "todos los textos del servidor en el idioma del jugador"):
  - **A. Broadcasts/notices/timers usan el idioma del ÚLTIMO paquete procesado** — `LC_TEXT_LANG`/`LC_TEXT_NEW_LANG` están definidas (locale.hpp:57-58) pero nunca se usan (1 match = comentario). Los 26 `SendNotice` salen en el idioma del jugador anterior.
  - **B. Textos de quest y monster_chat NO traducen** — cargan lua fijo al boot en español (`translate.lua`, `quest/locale.lua`, `MonsterChat` → `locale.monster_chat` sin pasar por el motor). **Es la causa real de los "NPCs/mensajes en español" con cliente EN.** El mod traía `LC_QUEST_TEXT`/`locale_quest_find` (mod `locale.cpp:333-374`) que NO se integró.
  - **C. ~437 `ChatPacket` sin `LC_TEXT`** (de 1424) — la mayoría son comandos de protocolo (no requieren traducción), pero hay visibles: arena (marcadores), battle (avisos hack), `char.cpp:3045` "You have gained %d exp." hardcodeado en inglés, etc.
  - **D. Nombres de NPC fijos** desde `mob_proto.locale_name` (español) sin rama por `GetLang()` — mitigado client-side hoy (el cliente resuelve NPCs desde su pack), pero el servidor no manda nombre por idioma.
  - **E. ES no tiene 11 claves que EN sí** (10 usadas por el código: exchange de won) → los jugadores ES ven `@0949`+inglés en esos 10 textos.
  - **F. Copia Windows de `svfiles` desincronizada** (los 16 `locale_string_*.txt` solo están en WSL).
- **Selector de idioma en el login (banderas): NO existe.** El diálogo nativo `IDD_SELECT_LOCALE` está **compilado pero muerto** (`LOCALE_SERVICE_GLOBAL` no definido → `LocaleService_LoadGlobal` devuelve siempre false, UserInterface.cpp:759). Hoy el idioma se elige con `config.exe`/`locale.cfg`. Pendiente de implementar (aprobado por el usuario).
- **"El root que faltó" = correctamente NO integrado:** los 8 archivos `02. Client\root\*.py` del mod son parches del **coliseo PVP** (`app.LANGUAGE_SYSTEM`, `IsTournamentMap`, `NAME_COLOR_LANGUAGE_SYSTEM`, `LanguageSystem_ITEM_BOX_REWARD`) — cero lógica de localización. Excluirlos fue la decisión correcta (decisión #8 del doc de estado).

### Compatibilidad de los locale_string del mod con nuestro código (verificado con conteos)

- **Formato: 100% compatible** con el parser (`locale.cpp:222-307`); 11 idiomas base perfectos; solo 4 líneas con comillas embebidas (GR:1409/1415, PT:1409, RU:488 — cosmético, trunca el valor) y 24 claves duplicadas en RU (inocuo).
- **Contenido: parcial.** 769 claves únicas `LC_TEXT` en el código; los 11 idiomas base + AE/EN/GR cubren ~75% (576-587 claves, sets idénticos entre sí). **181 claves (23.5%) no existen en NINGÚN archivo → `@0949`+clave para TODOS los jugadores** (52 inglesas de features MartySama 5.9: exchange de won, dados, fishing; 129 coreanas: chat bans ×4, monarch, char_battle...).
- **PT (43.7%) y RU (19.1%) NO sirven — son de OTRA base/versión del mod** (aportan claves que ES no tiene). Habría que regenerarlos.
- **Fallback confirmado** (locale.cpp:48-80): idioma del jugador → ES (default) → `@0949`+clave. Jugador EN con clave solo en ES ve ESPAÑOL (no `@0949`).

### En curso (actualizado)

- **Verificación del fix del crash**: entrar 2-3 veces seguidas con el cliente nuevo (14:12).
- Reescritura Rust del servidor (ver `ROADMAP.md` — Fase 0 en preparación).
- **Pendientes del Language System por orden**: (1) verificar crash, (2) huecos del servidor A+B+C (broadcasts por idioma, quest/monster_chat multilenguaje, ChatPacket sin LC_TEXT), (3) selector de banderas en el login (pack + imágenes), (4) 181 claves faltantes en los 16 archivos + regenerar PT/RU + 11 claves ES, (5) limpiar 4 líneas con comillas embebidas.
- Selector de idioma en el login (columna de banderas — pendiente de diseño, no confundir con el coliseo del mod).

## [2026-08-09] (3ª sesión) — Multilenguaje: NPCs resuelven nombre desde el pack del cliente

### Arreglado

- **NPCs ahora traducen con el idioma del cliente.** El nombre de los NPCs (guardias, tenderos, Alquimista…) venía del servidor (`GC_CHAR_ADDITIONAL_INFO` → `GetName()` → `szLocaleName` de MySQL, en español) y no pasaba por el Language System → no cambiaba de idioma aunque el cliente estuviera en inglés. Items y mobs sí cambiaban porque el cliente los resuelve desde su pack (`locale/<lang>/item_proto` y `mob_proto` — misma ruta dinámica, `PythonApplication.cpp:878-880`).
  - Fix en el cliente (`PythonNetworkStreamPhaseGameActor.cpp` `RecvCharacterAdditionalInfo`): para `CActorInstance::TYPE_NPC` se usa `CPythonNonPlayer::GetName(race)` del pack del cliente (idioma actual), con fallback al nombre del servidor si el pack no tiene la entrada. `TYPE_PC` (jugadores) intacto — sus nombres son del servidor por diseño.
  - Rebuild Release|Win32 OK (0 errores, `metin2client.exe` 5.115.904 bytes, 12:35). **Despliegue a `client\metin2client.exe` pendiente** — el cliente estaba abierto (deploy falló con IOException). Verificar hash tras copiar.
- Diagnóstico completo del multilenguaje (evidencia por código, no solo teoría):
  - Items: pack cliente → cambian ✓ (verificado por el usuario).
  - Mobs reales: pack cliente, misma ruta que items → ya cambiaban (el usuario veía criaturas tipo NPC — guardias/tenderos — que son las que no cambiaban).
  - NPCs: servidor MySQL (español) → NO cambiaban ← este es el fix de hoy.
  - El pack `locale.epk` (S3llMetin2 v24) tiene los 17 locales con `mob_proto` real por idioma (es `#101=Perro Salvaje`, en `#101=Wild Dog`); el cliente nunca tuvo hardcodeado `locale/es` — AGENTS.md §17 quedó desactualizado en ese punto (la ruta es dinámica desde `locale.cfg` → `MULTI_LOCALE_PATH`).

### En curso (actualizado)

- Reescritura Rust del servidor (ver `ROADMAP.md` — Fase 0 en preparación).
- Prueba end-to-end del Language System: textos ES verificados; **probar ahora con cliente EN**: server texts vía `account.lang` (el cliente sobrescribe con su locale), NPCs en inglés (fix de hoy), mobs/items en inglés (pack). monster_chat/quest strings siguen en español (data `quest/locale.lua` — ver pendientes).
- Selector de idioma en el login (columna de banderas — pendiente de diseño, no confundir con el coliseo del mod).

## [2026-08-08] (2ª sesión) — Language System: motor cargando + limpieza DBG

### Arreglado

- **Language System — motor cargando (falso negativo de log):** el boot del core no mostraba las 16 líneas "Load LocaleString" porque `sys_log` es invisible en `config_init` (el logfile no está abierto aún y `sys_log` a stdout requiere `log_level_bits > 1`, pero el CONFIG fija `DB_LOG_LEVEL: 1`). El motor cargaba desde el inicio; solo la evidencia se perdía.
  - `locale_init_file`/`locale_init_lang` ahora devuelven el nº de entradas cargadas (`int`); el bucle de `LocaleService_LoadLocaleStringFile()` imprime con `fprintf(stdout, "Load LocaleString %s (%d entries)")` — visible en boot.
  - Evidencia (boot 20:31, `core1/stdout`): 16/16 líneas con 764-775 entradas por idioma (`AE 774, CZ/DE/DK/ES/FR/GR/HU/IT/NL/PL/PT/RO/RU/TR 764, EN 775`); `LOCALE_ERROR` = 0.
- **Logs de debug del db eliminados:** `DBG_AQR` (ClientManager.cpp), `DBG_PARSE` y `DBG_RESULT_LOGIN` (ClientManagerLogin.cpp) — rebuild `db_r41023`, deploy y verificación: 0 líneas DBG en el boot nuevo; el item award refresh loguea limpio.
- Ambos cambios aplicados en las dos copias de source (WSL `/home/m2/source` + Windows `source\metin2_server`), md5 sincronizados, binarios desplegados y stack reiniciado (db/auth1/core1, puertos 30000-30004 OK).
- **Crash de entrada al mundo — parte determinista RESUELTA:** los 2 personajes de `test` estaban en la BD con coordenadas basura `(960155, 269313)` en el mapa 41 (~100x fuera; la aldea es `(969600, 278400)`); el cliente crasheaba con `0xc0000374` (heap corruption) al calcular tiles fuera de rango. Fix: `UPDATE player SET x=969600, y=278400` para ambos. El usuario entró al mundo y jugó (combate ✓, mobs con nombre en español ✓, textos del servidor en español ✓).
- **Language System — prueba end-to-end parcial:** los textos del servidor salen en ESPAÑOL en el cliente real (motor traduciendo con la tabla ES); propagación `account.lang` → `g_iCurrentLang` verificada (`login_success: lang 'es' -> 5`). Nota: el cliente sobrescribe `account.lang` con su idioma en cada login (diseño actual).

### Pendiente / conocido

- **Crash INTERMITENTE de entrada al mundo (~75% de entradas, NO RESUELTO):** con coordenadas válidas el cliente crashea aleatoriamente ~8-17s tras `player_load`, misma firma `0xc0000374` que desde las 15:00 (no relacionado con el Language System). Hipótesis principales: overflow del cliente base S3llMetin2 v24 durante la carga del mundo (layout del heap), mismatch de algún paquete de entrada no auditado, o race de hilos del cliente. Detalle completo en AGENTS.md "Crash de entrada al mundo". Captura `/home/m2/cap_entry.pcap` (1 entrada con éxito) para comparar.
- Prueba end-to-end del Language System (ver "En curso").

## [2026-08-08] — Línea base de login verificada + metodología de docs

### Añadido

- **Graphify como MCP conectado en la TUI (omo-slim/opencode):** servidor MCP `graphify` (stdio, `python -m graphify.serve`) registrado en la config global `C:\Users\Ricardo Casamayor\.config\opencode\opencode.jsonc`; dependencia `mcp` instalada en Python; grafo mergeado raíz creado (`graphify merge-graphs` server+client → `graphify-out\graph.json`, 31.141 nodos / 73.349 edges). Handshake MCP verificado (`serverInfo: graphify 0.9.35`). El MCP y el skill ponytail se añadieron al preset del orchestrator (`oh-my-opencode-slim.json`).
- **Regla 13 (permanente):** consultar SIEMPRE los grafos de graphify primero (query/explain/path/GRAPH_REPORT) antes de grep/glob/lectura a ciegas en cualquier tarea de buscar/modificar/refactorizar código.
- **Regla 14 (permanente):** personalidad ponytail — YAGNI, mínima solución que funciona, stdlib/nativo antes que dependencias, una línea antes que cincuenta; sin recortar validación/seguridad/accesibilidad.
- **Skills de ponytail instalados** (github.com/DietrichGebert/ponytail, MIT): `ponytail`, `ponytail-review`, `ponytail-audit`, `ponytail-debt`, `ponytail-gain`, `ponytail-help` en `.agents/skills/`; plugin OpenCode vendeado en `.opencode/ponytail/` y activado en `opencode.json`. Filosofía YAGNI ("la mejor línea es la que nunca se escribe") — alineada con el lema del proyecto "hacer más con menos" (benchmark del autor: -54% LOC, -20% coste, 100% safe).
- **ROADMAP.md**: plan maestro de la reescritura Rust (servidor primero, cliente después) con fases F0–F7, hitos verificables y decisiones abiertas para ADRs.
- **CHANGELOG.md**: este registro cronológico de cambios (metodología "Keep a Changelog").
- **AGENTS.md**: sección de metodología de documentación — el orchestrator anota los cambios de cada sesión en el CHANGELOG y actualiza ROADMAP/ADRs.
- **Grafos actualizados**: `graphify update` sobre `source\metin2_server` (13.190 nodos / 33.233 edges) y `source\metin2_client` (17.951 nodos / 40.116 edges).

### Avance Fase 0 (reescritura Rust)

- **ADR-0002 aceptado** (`docs/decisions/0002-unify-game-and-db.md`): unificar `game`+`db` en un proceso por canal con db como crate; shim legacy del protocolo GD/DG durante F3–F5; unificación final en F6. Recomendación de @oracle con verificación en el código (el db legacy es un broker SQL + coordinador cross-canal, no una BD).
- **Spec byte-exacto del wire protocol de login** (`docs/reference/protocol/login-flow.md`, antes `docs/superpowers/specs/2026-08-08-wire-protocol-login-flow.md`): constantes (LOGIN_MAX_LEN=30, PASSWD_MAX_LEN=16), framing sin prefijo de longitud (tabla `CPacketInfoCG`), 16 structs packed con offsets (TPacketCGLogin3 65/68B, TPacketGCLoginSuccess 474B, TPacketGCCharacterAdd 37B...), máquina de estados auth→canal completa y protocolo peer GD/DG/QID. Extraído con el grafo graphify + lectura de fuentes.
- **Stack Rust investigado y fijado**: tokio 1.49 + sqlx 0.9 + mlua 0.12 + config-rs + clap 4.6 + tracing + proptest (reporte de @librarian; sin actores: task-per-connection, mundo por canal tras `mpsc`).
- **Mapa de módulos del servidor** (reporte de @explorer): los 3 binarios, propiedad de datos, capa de red libthecore/fdwatch y 15 fronteras naturales de port (char.cpp 6.5k LOC, input_main, quest engine, db ClientManager*...).

### Arreglado (línea base C++ — ver AGENTS.md "Fase actual" para detalle)

- **Login completo funcional** (auth + canal + selección de personaje), verificado con el cliente real y la cuenta `test`/`1234`:
  - Semántica de `socket_write` (consumir `result > 0`) en game (`desc.cpp`) y db (`PeerBase.cpp`) — el buffer de salida drenaba.
  - Cifrado plaintext en ambos lados (`_IMPROVED_PACKET_ENCRYPTION_` OFF, `USE_NO_PACKET_ENCRYPTION` ON).
  - `mysql5_password` con asterisco incluido (`"*" + UPPER(SHA1(UNHEX(SHA1(pw))))`), coincidiendo con la función SQL `account.mysql_hash_password`.
  - `QUERY_LOGIN` con las 12 columnas en el orden que espera `CreateAccountTableFromRes`.
  - Ruteo SQL con `iSlot = SQL_ACCOUNT`.
  - `ClientHandleInfo` con `account_index`/`account_id` inicializados.
  - Re-registro del peer con solo READ tras drenar el buffer (evita el flood `AUTH_PEER_WRITE: size 0`).
  - Cliente: eliminados los `ClearLoginInfo()` que borraban `m_stPassword` durante el auth y en `SetLoginPhase` (entrada al mundo vía DirectEnter/warp).
- **Entrada al mundo** verificada (mapa `Venter_the_east.mp3`, stats) con el cliente recompilado.
- **Spam del chat / monster_chat**: `translate.lua` desplegado vacío → restaurado desde `translate_ES.lua`; `quest/locale.lua` con sintaxis rota por coreano UTF-8 (el lexer lua 5.0 es EUC-KR 2 bytes) → convertido a CP949. `LoadQuestLocale returns 0`.
- **Nombres de mobs**: reescritos en MySQL desde el pack del cliente (locale_epk, DumpProto) con los 2864 nombres en español; `item_proto` se dejó en CP949 original (los drops referencian items por nombre — no traducir).

### Reglas nuevas (documentadas en AGENTS.md)

- Los `.lua` de locale del servidor con coreano deben usar **CP949/EUC-KR**, no UTF-8.
- No traducir `item_proto` en el servidor (los txt de drops referencian items por nombre CP949).
- El cliente traduce, el servidor no (contrato de multilenguaje).

## [2026-08-06] — Fundaciones

### Añadido

- **ADR-0001**: PostgreSQL como base principal del futuro servidor Rust, sin TimescaleDB por defecto (en `docs/decisions/0001-postgresql-without-timescaledb-by-default.md`).
- Skills de proyecto (`.agents/skills/`) y planes en `docs/superpowers/`.
- Compatibilidad de la línea base C++ con Alpine/Docker (planes en `docs/superpowers/plans/`).
