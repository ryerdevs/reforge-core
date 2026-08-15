# Comportamiento de los mobs en el servidor C++ legacy (Metin2) — referencia para el rewrite Rust

> Análisis completo del C++ congelado (source/server, NO modificar) vs el rewrite (source/reforge).
> Fecha: 2026-08-15. Cada sección cita file:line y concluye con "En el rewrite:".
> Convenciones: passes_per_sec=25 (config.cpp:25); PASSES_PER_SEC(sec)=sec*25 (stdafx.h:60).

## 1. La máquina de estados AI (char_state.cpp)

### 1.1 Arquitectura

- Estados: `StateIdle` (:349), `StateMove` (:717), `StateBattle` (:851), `StateFlag` (:1062), `StateFlagBase` (:1095), `StateHorse` (:1103).
- El estado se elige por POSITION: POS_STANDING → idle, POS_FIGHTING → battle (char.cpp:1090-1104).
- `BeginFight(victim)` (char_battle.cpp:92-96): SetVictim + SetPosition(POS_FIGHTING) + SetNextStatePulse(1). `CanBeginFight` (:84-89): CanMove() && POS_STANDING && !IsDead() && !IsStun().

### 1.2 StateIdle (349)

- IsStone() → __StateIdle_Stone (:374-490): por umbrales de HP% sube MaxSP y spawnea grupos de minions.
- NPC → __StateIdle_NPC (:492-590): guards buscan víctima en 50000; deambula 1/7 con paso 200-400.
- **Monstruo → __StateIdle_Monster (:592-691)**: Stun/!CanMove → nada; Coward → CowardEscape; Berserk/GodSpeed se resetean en idle; víctima muerta → SetVictim(null); agresivo → FindVictim(wAggressiveSight) (:632-637); víctima viva + CanBeginFight → BeginFight; sin víctima: duración number(1,3) o number(3,5) s; protegido >1000 → Follow; **patrulla** (:668-688): 1/7, rotación 0..359°, distancia number(300,700), valida IsMovablePosition del destino Y punto medio, SetNowWalking(true) + Goto + SendMovePacket(FUNC_WAIT). NOMOVE/no_wander lo excluyen. **NO hay clamp al spawn.**

### 1.3 FindVictim (trigger.cpp:156-166, FuncFindMobVictim :66-133)

- Sectree alrededor del mob (ForEachAround). Excluye: no-characters, muertos, AFF_EUNHYUNG/INVISIBILITY/REVIVE_INVISIBLE, AFF_TERROR sin inmunidad.
- AIFLAG_NOATTACKSHINSU/JINNO/CHUNJO excluyen imperios 1/3/2.
- NPCs solo candidatos si atacante tiene AIFLAG_ATTACKMOB Y no es agresivo.
- Devuelve el MÁS CERCANO dentro de wAggressiveSight.

### 1.4 StateMove (717)

- Interpola posición con fRate = elapsed/m_dwMoveDuration → Move(x,y) (:719-730).
- Con víctima: decaimiento de aggro `UpdateAggrPoint(victim, NORMAL, -(victim->GetLevel()/3 + 1))` (:784-787).
- Boss 2191: warp detrás 1/20. Boss con víctima: 1/4 re-acercarse.
- Al llegar: con víctima → StateBattle (duración 1); si no → StateIdle (number(1,3) s).

### 1.5 StateBattle (851)

- Guards: !CanMove() || IsStun() → return.
- COWARD (:874-885): suelta víctima; 1/50 → CowardEscape, si no → idle.
- Víctima muerta (:888-940): si agresivo busca nueva FindVictim; si no hay → **KILL_AND_GO**: melee elige punto 400-1500 de la víctima muerta, valida IsAttackablePosition (mitad+destino) y va (:899-932).
- Leash: fDist >= 4000 → suelta víctima (:979-985).
- Rango: fDist >= GetMobAttackRange()*1.15 → __CHARACTER_GotoNearTarget (:991-993): melee → Follow(range*9/10); RANGE/MAGIC → Follow(range*8/10); NOMOVE → false (:694-714).
- **Cooldown de ataque** (:1005-1012): dwDuration = CalculateDuration(GetLimitPoint(POINT_ATT_SPEED), 2000); si now-lastAttack < dwDuration → espera. lastAttack se fija en OnMove(true) tras ataque exitoso (char.cpp:4828-4834).
- **Berserk** (:1016-1018): HP% < bBerserkPoint → daño ×2 (char.cpp:1963-1964).
- **GodSpeed** (:1021-1023): HP% < bGodSpeedPoint → POINT_ATT_SPEED=250 (char.cpp:6867-6879).
- Skills de mob (:1027-1053): CanUseMobSkill → SetRotationToXY + UseMobSkill + FUNC_MOB_SKILL.
- Ataque (:1056-1059): Attack(victim) → duración = duración motion MOTION_NORMAL_ATTACK (o 2 s); si falla → passes_per_sec/2.

## 2. El ataque del mob

### 2.1 Dispatch por battle type (Attack() char_battle.cpp:193-292)

- bType 0: MELEE/POWER/TANKER/SUPER_POWER/SUPER_TANKER → battle_melee_attack; RANGE → FlyTarget + Shoot(0); MAGIC → FlyTarget + Shoot(1).
- Éxito → OnMove(true) (fija lastAttackTime).

### 2.2 Rango real

- Umbral persecución: fDist >= GetMobAttackRange()*1.15 (char_state.cpp:991).
- Golpe melee: max = (int)(GetMobAttackRange()*1.15) para no-PC (battle.cpp:151-152); PC vs mob MELEE → MAX(300, range*1.15) (:156-158).
- GetMobAttackRange() (char.cpp:2010-2020): RANGE/MAGIC → wAttackRange + POINT_BOW_DISTANCE; resto → wAttackRange.
- En la práctica el mob se detiene a range*9/10 (melee) o range*8/10 (rango) y golpea desde ahí.

### 2.3 Cooldown — CalculateDuration (utils.cpp:201-210)

```
spd==100 → i=100 → duración = iDur
spd>100  → i=10000/(100-i) → duración = iDur*100/spd
spd<100  → i=100+i → duración = iDur*(200-spd)/100
```

Cooldown mob = CalculateDuration(POINT_ATT_SPEED, 2000). Base = sAttackSpeed del mob_proto.

## 3. El daño que recibe el mob (char_battle.cpp)

### 3.1 SendDamagePacket (1508-1527)

Solo si víctima es PC, o atacante PC con TARGET. Se manda a AMBOS descs. HEADER_GC_DAMAGE_INFO (135).

### 3.2 Damage() (1539-2197)

- Piedra mismo imperio → DAMAGE_BLOCK. TERROR: % evasión.
- BLOCK (POINT_BLOCK%) → DAMAGE_BLOCK; NORMAL_RANGE → DODGE (POINT_DODGE%).
- Crítico ×2, penetración (+DEF), steal HP/SP/gold, mana burn.
- STONESKIN: HP% < bStoneSkinPoint → dam /= 2 (:2082-2084).
- DEATHBLOW: AIFLAG_DEATHBLOW + prob. bDeathBlowPoint% vs job 1-3 → dam ×= 4 (:2086-2095).
- damageFlag: DAMAGE_POISON/BLEEDING/NORMAL + CRITICAL + PENETRATE (:2097-2110).
- Aggro: m_map_kDamage[atacante] = {totalDamage, aggro} + StartRecoveryEvent + UpdateAggrPointEx (:2137-2188).
- HP ≤ 0 → Stun() + killer PID (:2190-2196).

### 3.3 Aggro (3461-3550)

- Pesos: NORMAL_RANGE ×1.2, RANGE ×1.5, MAGIC ×1.2, ×1.2 extra si ya es víctima. Party: líder /2, miembros /3.
- ChangeVictimByAggro: throttle 3 s; solo cambia si supera m_iMaxAggro; escanea a ≤5000.

### 3.4 "Tumbar" / knockdown — NO EXISTE en el servidor

- AIFLAG_FALL (length.h:545) NO tiene NINGÚN uso en source/server (dead code).
- No hay stagger ni interrupción del ataque por daño: solo IsStun()/muerte lo detienen.
- La reacción al golpe (animación de hit) es 100% client-side (RecvDamageInfoPacket → AddDamageEffect).
- IMMUNE_FALL es la resistencia a caerse del caballo — no relacionado.

## 4. Separación entre mobs

- **El C++ NO tiene colisión mob-mob ni distancia mínima.** Solo valida ATTR_BLOCK|ATTR_OBJECT del mapa.
- La separación visual legacy sale de: (1) jitter de spawn (SpawnMobRange, char_manager.cpp:545-563); (2) anillo de ataque (cada mob a su propio rango); (3) **change attack position** cada 10 s (o 1 s si lejos) — el mob elige un punto ALEATORIO alrededor de la víctima y camina allí (char.cpp:5436-5462, 5869-5881); (4) interceptación cinemática en Follow (char.cpp:5365-5405).
- El rewrite SÍ tiene separación server-side (SEP_MOBS=60, flancos ±90°) que el C++ no tiene — compensación razonable, pero no replica el "change attack position" (por eso los mobs se juntan).

## 5. El respawn (regen.cpp)

- Formato: `type sx sy ex ey z direction time percent max_count vnum`.
- regen_load (:680-693): si time != 0 → spawnea inmediatamente + regen_event a number(0,16)+time (con ENABLE_REGEN_RENEWAL activo NO hay evento periódico inicial).
- regen_spawn (:322-380): num = max_count - count; rect → SpawnMobRange (aleatorio); cada mob → SetRegen + ++count.
- Muerte (char.cpp:437-469): --count; si llega a 0 (con RENEWAL) → event_create(regen_event, number(0,16)+time).
- regen_event (:582-600): regen_spawn; time==0 → event=nullptr (no se re-agenda). **time==0 ⇒ el mob muere UNA vez y no reaparece NUNCA.**

## 6. Los AIFLAGs (length.h:529-550)

| Flag | Bit | Comportamiento | Rewrite |
| --- | --- | --- | --- |
| AGGRESSIVE | 0 | Aggro proactivo | Implementado |
| NOMOVE | 1 | No patrulla; Follow=false (NUNCA persigue) | Parcial (no patrulla pero SÍ persigue) |
| COWARD | 2 | CowardEscape: huye 500-5000 | **Faltante** |
| NOATTACKSHINSU/JINNO/CHUNJO | 3-5 | Excluye imperios en FindVictim | **Faltante** |
| ATTACKMOB | 6 | Ataca a otros mobs | **Faltante** |
| BERSERK | 7 | HP<bBerserkPoint → daño ×2 | **Faltante** |
| STONESKIN | 8 | HP<bStoneSkinPoint → daño /2 | **Faltante** |
| GODSPEED | 9 | HP<bGodSpeedPoint → ATT_SPEED=250 | **Faltante** |
| DEATHBLOW | 10 | Prob. ×4 vs job 1-3 | **Faltante** |
| REVIVE | 11 | Revive party a 3 s | **Faltante** |
| HEALER | 12 | Solo display | Faltante (cosmético) |
| COUNT..TIMEVIT | 13-21 | Sin uso en game/src (dead code) | N/A |

## 7. La velocidad real del mob

- GetMoveSpeed() (char.cpp:2751-2754): GetMoveMotionSpeed()*10000/CalculateDuration(POINT_MOV_SPEED, 10000).
- GetMoveMotionSpeed() (char.cpp:2726-2749): -accumVector.y/duration del motion RUN de la raza — **la velocidad sale de la ANIMACIÓN, no de la columna** (sMovingSpeed es el factor POINT_MOV_SPEED).
- CalculateMoveDuration() (char.cpp:2756-2776): m_dwMoveDuration = CalculateDuration(POINT_MOV_SPEED, (fDist/motionSpeed)*1000).
- **Divergencia del rewrite**: step_toward/move_duration_ms (ai.rs:16-58) tratan move_speed como UNITS/seg reales (100 u/s). Con el motion legacy a ~300 u/s, los mobs del rewrite avanzan ~3× más lento y el dw_duration es ~3× mayor → "se mueven raro".

## 8. Comportamientos FALTANTES — priorizados por impacto

1. **Cooldown de ataque del mob** — legacy ~2 s (CalculateDuration ATT_SPEED 2000); rewrite ataca cada tick 250 ms → pegan ~8× más rápido. **CRÍTICO**.
2. **Velocidad real (motion-based)** — legacy GetMoveMotionSpeed+CalculateDuration; rewrite columna como u/s → mobs ~3× más lentos + dw_duration incorrecto. **CRÍTICO** ("se mueven raro").
3. **Rango de ataque exacto** — legacy range*1.15 para perseguir, parar en range*0.9/0.8; rewrite MAX(300, range*1.15) solo MELEE → RANGE/MAGIC atacan a 300. **ALTO**.
4. **Berserk** — ×2 daño por HP%. **ALTO** (jefes).
5. **Espera de la animación de ataque** — no decide durante el motion. **ALTO** (ligado a #1).
6. **COWARD** — huida. **MEDIO-ALTO**.
7. **NOMOVE no debe perseguir**. **MEDIO**.
8. **Aggro completo** — pesos, decaimiento, throttle 3 s, m_iMaxAggro. **MEDIO**.
9. **De-aggro/leash legacy** — suelta a 4000, Return a >5000 tras 15 s. **MEDIO**.
10. **Nueva víctima tras matar + KILL_AND_GO**. **MEDIO**.
11. **STONESKIN + GODSPEED**. **MEDIO**.
12. **Change attack position** — reposicionamiento aleatorio cada 10 s/1 s (la causa real de que los mobs legacy "no se junten"). **MEDIO**.
13. **Interceptación cinemática** en Follow. **MEDIO**.
14. **Walkability del mapa** — legacy valida IsMovablePosition en patrulla/huida/KILL_AND_GO; rewrite clampa sin chequear mapa. **MEDIO**.
15. **DEATHBLOW** (×4). **BAJO-MEDIO**.
16. **REVIVE de party**. **MEDIO** (raids).
17. **ATTACKMOB + NOATTACK*** — filtros FindVictim. **BAJO**.
18. **Mob skills** (UseMobSkill, FUNC_MOB_SKILL). **ALTO jefes, BAJO resto**.
19. **GuardNPC** — víctima a 50000, Return, stun al agresor. **BAJO**.
20. **Summoner / party de mobs / boss warp 2191**. **BAJO**.
21. **Flags de daño del wire** — DAMAGE_BLOCK/POISON/CRITICAL/PENETRATE; rewrite siempre NORMAL. **BAJO** (visual).

Nota: el "tumbar al golpear" NO existe en el servidor legacy (AIFLAG_FALL dead code). Es animación del cliente; el servidor solo debe mantener el sync de ataque correcto (FUNC_ATTACK con dw_time/dwDuration).
