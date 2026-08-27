//! Dominio COMBATE del mundo (C5): los sistemas 1-2 del tick (`chase_attack`,
//! `aggro_detect`) + los métodos del `WorldSim` del combate (`process_attack`
//! y sus helpers `npc_view`/`damage_npc`/`remove_npc`) + los syncs de stats
//! del jugador que alimentan las fórmulas (hp/sp/armor/level).

use std::collections::HashMap;

use bevy_ecs::prelude::*;

use crate::ai::{attack_damage, change_attack_dest, mob_move_speed, move_duration_ms, rotation_5deg, step_toward};
use crate::combat::{
    attack_speed_for_weapon_bonus, can_attack, distance_approx, handle_attack,
    mob_attack_max_range, mob_attack_range_base, player_def_grade, BATTLE_TYPE_MELEE,
    CombatState, NpcState, PlayerState, PvpContext,
};
use crate::ecs::components::{
    Affect, Affects, Aggro, AttackPos, Combat, Hp, LastAttack, Map, Mob, Mp, Player, Position, Pvp,
    SpawnRef, Vid,
};
use crate::ecs::events::{CombatEvent, KillInfo, MoveEvent, NpcEvent};
use crate::ecs::resources::{
    NpcIndex, NpcOutbox, Rand, RespawnQueue, SpawnTable, Tick, WorldClock,
};
use crate::ecs::world::WorldSim;
use database::item::ProtoItem;

/// Floor del de-aggro por distancia: un mob con sight 0 (nunca proactivo)
/// pero GOLPEADO por el jugador sigue persiguiendo un mínimo (channel.rs).
const DE_AGGRO_FLOOR: i32 = 2_000;

/// Radio de SEPARACIÓN entre mobs (C28 — units): un mob no avanza si otro
/// mob está a ≤ SEP_MOBS del punto donde aterrizaría su paso (evita el
/// amontonamiento de la persecución — el C++ mantiene distancia porque
/// spawnea las copias ALEATORIAMENTE dentro del rect y cada mob se detiene
/// en su rango de ataque; el rewrite spawnea en el centro y el check
/// conserva la separación cuando convergen al mismo jugador).
pub(crate) const SEP_MOBS: i32 = 60;

/// C32: `AI_CHANGE_ATTACK_POISITION_TIME_NEAR` = 10000 ms / `TIME_FAR` =
/// 1000 ms (char.h:72-74) — el intervalo del change-attack-position del mob
/// en persecución (10 s cerca de la víctima, 1 s lejos).
const CHANGE_ATTACK_POS_TIME_NEAR: u64 = 10_000;
const CHANGE_ATTACK_POS_TIME_FAR: u64 = 1_000;

/// C32: `MOB_RANK_BOSS` = 4 (length.h:317) — los bosses NO se reposicionan
/// (`GetMobRank() < MOB_RANK_BOSS`, char.cpp:5437).
const MOB_RANK_BOSS: i32 = 4;

/// C32: distancia (units) bajo la cual el mob considera que ya LLEGÓ a su
/// destino lateral (resetea el change-attack-position y vuelve a la
/// víctima).
const MOB_REACH_EPSILON: i32 = 60;

/// ¿Hay OTRO mob (vid distinto) dentro de `radius` de (x, y)? (C28).
fn mob_near(others: &[(u32, i32, i32)], self_vid: u32, x: i32, y: i32, radius: i32) -> bool {
    others
        .iter()
        .any(|(v, ox, oy)| *v != self_vid && distance_approx(x - ox, y - oy) <= radius)
}

/// Destino del paso libre de otros mobs (C28 — chase y patrol): si el
/// aterrizaje recto está a ≤ SEP_MOBS de otro mob, prueba los FLANCOS ±90°
/// (misma longitud de paso — el mob rodea al que bloquea); si todos los
/// destinos están ocupados por mobs YA pegados a mí (a ≤ SEP_MOBS de mi
/// posición actual — copias co-localizadas del spawn), avanza igual al
/// aterrizaje recto (bloquear congelaría al par); si el bloqueo es de un
/// mob LEJANO (a > SEP_MOBS de mí), no moverse este tick (el mob espera a
/// que el camino se despeje — "no avanzar"). Devuelve el destino final.
pub(crate) fn separate_landing(
    others: &[(u32, i32, i32)],
    self_vid: u32,
    pos: (i32, i32),
    nx: i32,
    ny: i32,
) -> Option<(i32, i32)> {
    if !mob_near(others, self_vid, nx, ny, SEP_MOBS) {
        return Some((nx, ny));
    }
    // Flancos: rotar el vector del paso ±90° (rotación exacta en enteros,
    // misma longitud — `(dx,dy) -> (-dy,dx)` / `(dy,-dx)`).
    let (dx, dy) = (nx - pos.0, ny - pos.1);
    for (lx, ly) in [(pos.0 - dy, pos.1 + dx), (pos.0 + dy, pos.1 - dx)] {
        if !mob_near(others, self_vid, lx, ly, SEP_MOBS) {
            return Some((lx, ly));
        }
    }
    // ¿El bloqueo es de mobs YA pegados a mí? -> avanzar (evita congelar
    // al par co-localizado; no empeora la superposición que ya existe).
    if mob_near(others, self_vid, pos.0, pos.1, SEP_MOBS) {
        return Some((nx, ny));
    }
    None // otro mob (lejano) bloquea el avance — no moverse este tick
}

/// La vista `NpcState` que `game_core::combat` consume, desde los componentes
/// (parity del `LiveNpc.state` del canal — la "dx" del mob es su columna ht).
fn npc_state(vid: u32, pos: &Position, mob: &Mob) -> NpcState {
    NpcState {
        vid,
        x: pos.x,
        y: pos.y,
        level: mob.level,
        dx: mob.ht, // parity channel.rs:426 — la "dx" del mob es la columna ht
        ht: mob.ht,
        wdef: mob.wdef,
        battle_type: mob.battle_type,
        attack_range: mob.attack_range,
    }
}

/// 1) Mobs con AGGRO: de-aggro por distancia (parity channel.rs:1651-1668),
///    ataque en rango melee (GC_MOVE(FUNC_ATTACK) + daño al jugador — parity
///    char_state.cpp:386 + `battle_hit`) o paso de persecución (`step_toward`).
///    Multi-jugador: cada mob persigue SU objetivo (`Aggro.target`).
#[allow(clippy::type_complexity)] // firma de sistema bevy (ParamSet con 2 queries)
pub(crate) fn chase_attack_system(
    mut mobs: ParamSet<(
        // Posiciones de TODOS los mobs (C28 — separación; read-only).
        Query<(&Vid, &Position, &Map), Without<Player>>,
        Query<(&Vid, &Mob, &Map, &mut Aggro, &mut Position, &mut LastAttack, &mut AttackPos, &Hp), Without<Player>>,
    )>,
    mut players: Query<(Entity, &Player, &Position, &mut Hp, &Affects), Without<Mob>>,
    tick: Res<Tick>,
    now: Res<WorldClock>,
    mut rng: ResMut<Rand>,
    mut outbox: ResMut<NpcOutbox>,
) {
    // C28: snapshot de las posiciones de los mobs AGRUPADO POR MAPA (la
    // separación consulta a los OTROS mobs del MISMO mapa — el ParamSet
    // evita el conflicto de queries). F1: el mundo materializa mobs de
    // TODOS los mapas — sin el filtro, un mob de otro mapa con coords
    // coincidentes (≤ 60 u del aterrizaje) bloqueaba falsamente el paso.
    let mut others_by_map: HashMap<u32, Vec<(u32, i32, i32)>> = HashMap::new();
    for (v, p, m) in &mobs.p0() {
        others_by_map.entry(m.map_index).or_default().push((v.vid, p.x, p.y));
    }
    for (vid, mob, map, mut aggro, mut pos, mut last_attack, mut attack_pos, mob_hp) in &mut mobs.p1() {
        let Some(target) = aggro.target else {
            continue;
        };
        let Ok((_pe, p, ppos, mut php, paff)) = players.get_mut(target) else {
            aggro.target = None; // el objetivo ya no está (salió del mundo)
            continue;
        };
        let dist = distance_approx(pos.x - ppos.x, pos.y - ppos.y);
        // De-aggro por distancia (DATA-DRIVEN, parity `FindVictim`): fuera de
        // `aggressive_sight` (floor 2000 — un mob golpeado persigue mínimo).
        let de_aggro = mob.aggressive_sight.max(DE_AGGRO_FLOOR);
        if dist > de_aggro {
            aggro.target = None;
            outbox.0.push(CombatEvent::AggroOff { player_vid: p.vid, vid: vid.vid, vnum: mob.vnum }.into());
            continue;
        }
        let state = npc_state(vid.vid, &pos, mob);
        // COWARD (StateBattle:870-886): nunca ataca ni persigue; con HP <
        // 20% huye al lado opuesto del jugador (CowardEscape:269-307).
        if mob.coward {
            if mob_hp.hp > 0 && mob_hp.hp * 100 / mob_hp.max_hp.max(1) < 20 {
                let (fx, fy) = step_toward(pos.x, pos.y, pos.x * 2 - ppos.x, pos.y * 2 - ppos.y, mob_move_speed(mob.move_speed) as i32, tick.dt_ms);
                if fx != pos.x || fy != pos.y {
                    let rot = rotation_5deg(pos.x, pos.y, fx, fy);
                    let duration_ms = move_duration_ms(fx - pos.x, fy - pos.y, mob_move_speed(mob.move_speed) as i32);
                    pos.x = fx;
                    pos.y = fy;
                    outbox.0.push(MoveEvent::Moved { player_vid: p.vid, vid: vid.vid, x: fx, y: fy, rot, duration_ms }.into());
                }
            }
            aggro.target = None;
            continue; // nunca ataca ni persigue
        }
        // C31: el rango del ataque del MOB (mob→PC) es SOLO su
        // `GetMobAttackRange() * 1.15` — sin el floor 300 del PC
        // (battle.cpp:147-152). `melee_max_range` (con floor) se queda para
        // el ataque del PC→mob de `handle_attack`.
        if dist <= mob_attack_max_range(&state) {
            // C29: COOLDOWN del golpe del mob — parity `CalculateDuration`
            // (utils.cpp:201-210) con `POINT_ATT_SPEED = attack_speed` y
            // `iDur = 2000` (char_state.cpp:1005-1012). El rewrite atacaba
            // CADA TICK (250 ms); el legacy golpea cada ~2 s (att_speed
            // 100) — 8× más rápido. El `last_attack` se actualiza SOLO al
            // golpear (parity `m_dwLastAttackTime` en `OnMove(true)` tras un
            // ataque exitoso — char.cpp:4828-4834).
            // GODSPEED: bajo `sp_godspeed`% HP el mob ataca a 250
            // (`SetGodSpeed` → `POINT_ATT_SPEED = 250`, char_state.cpp:
            // 1021-1023 + char.cpp:6867-6879).
            let attack_speed = if mob.sp_godspeed > 0
                && mob_hp.hp > 0
                && mob_hp.hp * 100 / mob_hp.max_hp.max(1) < mob.sp_godspeed
            {
                250
            } else {
                mob.attack_speed
            };
            let cooldown = crate::ai::mob_attack_cooldown_ms(attack_speed);
            if now.0.saturating_sub(last_attack.at_ms) < cooldown {
                continue; // aún en cooldown — no atacar este tick
            }
            // EN RANGO: ataque del mob — daño = atk del mob_proto − DEF del
            // jugador (parity char.cpp:2114 + items ARMOR —
            // `player_def_grade`; + el bonus de DEF_GRADE de los buffs —
            // parity POINT_DEF_GRADE_BONUS). BERSERK: bajo `sp_berserk`%
            // HP el daño se DOBLA (`GetMobDamageMultiply` ×2, char.cpp:
            // 1963-1964 — activado en StateBattle, char_state.cpp:1016-1018).
            let mut damage = attack_damage(
                mob.damage_min,
                mob.damage_max,
                player_def_grade(p.level, p.ht, p.armor) + paff.def_grade_bonus(),
                &mut |lo, hi| rng.roll(lo, hi),
            );
            if mob.berserk && mob.sp_berserk > 0
                && mob_hp.hp > 0
                && mob_hp.hp * 100 / mob_hp.max_hp.max(1) < mob.sp_berserk
            {
                damage *= 2;
            }
            last_attack.at_ms = now.0;
            php.hp = (php.hp - damage).max(0);
            outbox.0.push(CombatEvent::MobAttack {
                player_vid: p.vid,
                vid: vid.vid,
                vnum: mob.vnum,
                x: pos.x,
                y: pos.y,
                damage,
            }.into());
            continue;
        }
        // Persecución: C32 (change-attack-position) — el mob NO persigue
        // directo a la víctima todo el tiempo: cada
        // AI_CHANGE_ATTACK_POISITION_TIME_NEAR (10 s) cerca, o TIME_FAR
        // (1 s) a > 100+rango (char.h:72-74), elige un punto ALEATORIO a
        // fMinDistance (rango×0.9/0.8) alrededor de la víctima y camina
        // HACIA ÉL — es lo que reparte a los mobs legacy (no se amontonan en
        // el punto más cercano). Solo `rank < MOB_RANK_BOSS` (char.cpp:5437).
        // Al llegar al destino lateral (o vencer el timer sin destino)
        // vuelve a perseguir a la víctima.
        // C32: el umbral lejos/cerca del timer usa `GetMobAttackRange()`
        // (char.cpp:5875-5878 — `AI_CHANGE_ATTACK_POISITION_DISTANCE +
        // GetMobAttackRange()`); para RANGE/MAGIC suma POINT_BOW_DISTANCE
        // (char.cpp:2010-2020) — un arco a 300 u está NEAR (10 s), no FAR.
        let is_far = dist > 100 + mob_attack_range_base(&state);
        let change_time = if is_far { CHANGE_ATTACK_POS_TIME_FAR } else { CHANGE_ATTACK_POS_TIME_NEAR };
        let mut dest = attack_pos.dest;
        if mob.rank < MOB_RANK_BOSS && now.0.saturating_sub(attack_pos.last_change_ms) >= change_time {
            // Timer expirado: nuevo destino lateral (parity Follow — el C++
            // resetea el timer al reponer el destino, char.cpp:5440).
            dest = Some(change_attack_dest(
                pos.x, pos.y, ppos.x, ppos.y, mob.battle_type, mob.attack_range,
                &mut |lo, hi| rng.roll(lo, hi),
            ));
            attack_pos.last_change_ms = now.0;
        }
        // ¿Llegó al destino lateral? → volver a perseguir a la víctima.
        if let Some((dx, dy)) = dest
            && distance_approx(pos.x - dx, pos.y - dy) <= MOB_REACH_EPSILON {
                attack_pos.dest = None;
                dest = None;
            }
        let target = dest.unwrap_or((ppos.x, ppos.y));
        // Paso hacia el destino (víctima o punto lateral) a `move_speed`.
        let (sx, sy) = step_toward(pos.x, pos.y, target.0, target.1, mob_move_speed(mob.move_speed) as i32, tick.dt_ms);
        if (sx, sy) == (pos.x, pos.y) {
            continue; // ya en el destino (o speed 0)
        }
        attack_pos.dest = dest; // recordar el destino lateral activo
        // C28 (separación): el destino del paso debe quedar libre de otros
        // mobs del MISMO mapa — si está ocupado, probar flancos o no
        // moverse este tick (F1: los mobs de otros mapas no bloquean).
        let Some((nx, ny)) = separate_landing(
            others_by_map.get(&map.map_index).map(Vec::as_slice).unwrap_or(&[]),
            vid.vid,
            (pos.x, pos.y),
            sx,
            sy,
        ) else {
            continue; // otro mob bloquea — no moverse este tick
        };
        let rot = rotation_5deg(pos.x, pos.y, nx, ny);
        // La duración REAL del paso (parity CalculateMoveDuration,
        // char.cpp:2765-2768) — el cliente interpola con ESTA duración; el
        // dt del tick fijo animaba los pasos largos a velocidad altísima.
        let duration_ms = move_duration_ms(nx - pos.x, ny - pos.y, mob_move_speed(mob.move_speed) as i32);
        pos.x = nx;
        pos.y = ny;
        outbox.0.push(MoveEvent::Moved {
            player_vid: p.vid,
            vid: vid.vid,
            x: nx,
            y: ny,
            rot,
            duration_ms,
        }.into());
    }
}

/// 2) AGGRO PROACTIVO (parity channel.rs:1792-1815 — `FindVictim(
///    wAggressiveSight)`): un mob `AGGR` detecta al jugador MÁS CERCANO de
///    su mapa dentro de su `aggressive_sight` y empieza a perseguirlo.
///    Sight 0 = nunca proactivo. Corre DESPUÉS del chase (parity del orden
///    del canal: el mob detectado ataca en el tick SIGUIENTE).
pub(crate) fn aggro_detect_system(
    mut mobs: Query<(&Vid, &Position, &Mob, &Map, &mut Aggro), Without<Player>>,
    players: Query<(Entity, &Player, &Map, &Position), Without<Mob>>,
    mut outbox: ResMut<NpcOutbox>,
) {
    let candidates: Vec<(Entity, u32, u32, i32, i32)> = players
        .iter()
        .map(|(e, p, m, pos)| (e, p.vid, m.map_index, pos.x, pos.y))
        .collect();
    for (vid, pos, mob, map, mut aggro) in &mut mobs {
        if aggro.target.is_some() || !mob.aggressive || mob.aggressive_sight <= 0 {
            continue;
        }
        let mut best: Option<(Entity, u32, i32)> = None;
        for (e, pv, m, x, y) in &candidates {
            if *m != map.map_index {
                continue;
            }
            let d = distance_approx(pos.x - x, pos.y - y);
            if d <= mob.aggressive_sight && best.is_none_or(|(_, _, bd)| d < bd) {
                best = Some((*e, *pv, d));
            }
        }
        if let Some((pe, pv, _)) = best {
            aggro.target = Some(pe);
            outbox.0.push(CombatEvent::AggroOn { player_vid: pv, vid: vid.vid, vnum: mob.vnum }.into());
        }
    }
}

/// Estado completo de un mob que el mundo lee internamente (combate/pickup).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NpcView {
    /// La vista de combate (la consumen `handle_attack`/`melee_max_range`).
    pub state: NpcState,
    pub vnum: i64,
    pub max_hp: i32,
    pub hp: i32,
    pub exp: i64,
    pub gold_min: i32,
    pub gold_max: i32,
    pub drop_item: i64,
}

/// Resultado de `damage_npc`: HP tras el golpe + si murió (el mundo decide
/// el despawn; la conexión la recompensa).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NpcDamage {
    pub hp: i32,
    pub dead: bool,
}

impl WorldSim {
    /// Estado completo del mob `vid` (None si no existe).
    pub(crate) fn npc_view(&self, vid: u32) -> Option<NpcView> {        let e = *self.world.resource::<NpcIndex>().0.get(&vid)?;
        let ent = self.world.get_entity(e).ok()?;
        let pos = ent.get::<Position>()?;
        let hp = ent.get::<Hp>()?;
        let mob = ent.get::<Mob>()?;
        Some(NpcView {
            state: npc_state(vid, pos, mob),
            vnum: mob.vnum,
            max_hp: hp.max_hp,
            hp: hp.hp,
            exp: mob.exp,
            gold_min: mob.gold_min,
            gold_max: mob.gold_max,
            drop_item: mob.drop_item,
        })
    }

    /// La vista `NpcState` de un JUGADOR como VÍCTIMA PvP: sus stats
    /// (level/ht/dx) + la DEF del PC (`player_def_grade` — char.cpp:
    /// 2112-2114) DOBLADA en `wdef` para que `melee_damage` (que usa
    /// `def_grade_npc` = level+ht+wdef) produzca la def del PC — parity
    /// `GetDefGrade()` del CHARACTER (el mob y el PC comparten la fórmula
    /// del daño, `CalcMeleeDamage`). El rango: MELEE con attack_range 0 →
    /// `melee_max_range` = 300 (parity battle.cpp:144-167 — una víctima PC
    /// NO extiende el rango del atacante, solo los mobs MELEE lo hacen).
    fn player_npc_state(&self, e: Entity) -> Option<NpcState> {
        let ent = self.world.get_entity(e).ok()?;
        let pos = ent.get::<Position>()?;
        let p = ent.get::<Player>()?;
        let def = player_def_grade(p.level, p.ht, p.armor);
        Some(NpcState {
            vid: p.vid,
            x: pos.x,
            y: pos.y,
            level: p.level,
            dx: p.dx,
            ht: p.ht,
            wdef: def - p.level - p.ht, // def_grade_npc(lv, ht, wdef) == def PC
            battle_type: BATTLE_TYPE_MELEE,
            attack_range: 0,
        })
    }

    /// El gate PvP del mundo: `can_attack` con los contextos de AMBOS
    /// jugadores (zona segura/nivel/guild/party/pk — parity battle.cpp:
    /// 83-125 + pvp.cpp:373-522). Sin componentes → no atacable
    /// (defensivo). Lo usa el PvP (combate) y el modo SPLASH de las skills
    /// (systems/skill.rs).
    pub(crate) fn pvp_attackable(&self, attacker: Entity, victim: Entity) -> bool {
        match (self.pvp_context(attacker), self.pvp_context(victim)) {
            (Some(a), Some(v)) => can_attack(&a, &v),
            _ => false,
        }
    }

    /// El contexto PvP de un jugador (parity `GetPKMode`/`GetParty`/
    /// `GetGuild`/`GetLevel`/`IsDead` — los consume el gate `can_attack`).
    /// GAP (2026-08-27): `guild_id` (el lane de guildas la sincronizará) y
    /// `safe_zone` (el lane de mapas alimentará el ATTR_BANPK) quedan
    /// inertes — el gate puro los cubre con tests; `level` es real.
    pub(crate) fn pvp_context(&self, e: Entity) -> Option<PvpContext> {
        let ent = self.world.get_entity(e).ok()?;
        let pvp = ent.get::<Pvp>()?;
        let hp = ent.get::<Hp>()?;
        let player = ent.get::<Player>()?;
        Some(PvpContext {
            pvp_mode: pvp.mode,
            party_id: pvp.party_id,
            hp: hp.hp,
            level: player.level,
            guild_id: None,
            safe_zone: false,
        })
    }

    /// Aplica `damage` al HP del mob (clamp a 0) y le marca AGGRO contra el
    /// jugador que golpeó (parity: el C++ marca el aggro en `OnDamage`).
    /// Devuelve el HP tras el golpe + si murió; None si el mob no existe.
    pub(crate) fn damage_npc(&mut self, vid: u32, damage: i32, aggro_to: Option<Entity>) -> Option<NpcDamage> {
        let e = *self.world.resource::<NpcIndex>().0.get(&vid)?;
        let hp_after = {
            let mut ent = self.world.get_entity_mut(e).ok()?;
            // STONESKIN: el daño recibido se divide por 2 si el HP% del mob
            // está bajo `sp_stoneskin` (parity `dam /= 2`,
            // char_battle.cpp:2082-2084 — activado por `IsStoneSkinner`).
            let stoneskin = ent.get::<Mob>().map(|m| m.sp_stoneskin).unwrap_or(0);
            let mut hp = ent.get_mut::<Hp>()?;
            let mut dmg = damage;
            if stoneskin > 0 && hp.hp > 0 && hp.hp * 100 / hp.max_hp.max(1) < stoneskin {
                dmg /= 2;
            }
            hp.hp = (hp.hp - dmg).max(0);
            hp.hp
        };
        // El aggro se marca en un borrow aparte (borrows secuenciales).
        let mut ent = self.world.get_entity_mut(e).ok()?;
        ent.get_mut::<Aggro>()?.target = aggro_to;
        Some(NpcDamage { hp: hp_after, dead: hp_after <= 0 })
    }

    /// Quita el mob del mundo (muerte o despawn). Idempotente. C23: si el
    /// mob vino de una entrada de la tabla de spawns, programa su RESPAWN
    /// por tiempo (parity `regen_event`, regen.cpp:582-600): la entrada
    /// vuelve a materializarse tras el intervalo del regen.txt (`entry.time`
    /// — 5s/30s/60s/3600s en el runtime). `time == 0` → SIN respawn (parity
    /// regen.cpp:680-693 — el C++ no re-spawnea esas entradas).
    pub(crate) fn remove_npc(&mut self, vid: u32) {
        // Deadline + copias ANTES del despawn (necesita el SpawnRef y la
        // tabla — borrows secuenciales).
        let respawn = {
            let Some(e) = self.world.resource::<NpcIndex>().0.get(&vid).copied() else {
                return;
            };
            let Ok(ent) = self.world.get_entity(e) else {
                return;
            };
            let Some(sr) = ent.get::<SpawnRef>() else {
                return; // sin entrada de spawn (p.ej. tests con entidades sueltas)
            };
            let interval = self
                .world
                .resource::<SpawnTable>()
                .maps
                .get(&sr.map)
                .and_then(|es| es.get(sr.index))
                .map(|se| se.entry.time)
                .unwrap_or(0);
            // C23/F2 (parity `regen_load` + `regen_event`, regen.cpp:680-693
            // y 582-600): `time == 0` → SIN respawn — el C++ solo spawnea y
            // re-spawnea entradas con `time != 0` (el runtime spain tiene
            // ~2800 entradas "0s": holyplace_flame 2303, sungzi_flame_pass
            // 2311 — mueren UNA vez y no vuelven). El sentinel u64::MAX
            // marca la entrada como muerta para siempre: el spawn dinámico
            // NO debe re-materializarla al re-acercarse un jugador (parity:
            // el mob del C++ no reaparece NUNCA, ni con players
            // entrando/saliendo del área).
            let due = if interval == 0 {
                u64::MAX
            } else {
                self.world.resource::<WorldClock>().0 + u64::from(interval) * 1000
            };
            Some((sr.map, sr.index, due))
        };
        if let Some(e) = self.world.resource_mut::<NpcIndex>().0.remove(&vid)
            && self.world.get_entity(e).is_ok()
        {
            self.world.despawn(e);
        }
        if let Some((map, index, due)) = respawn {
            // count>1: las copias muertas se acumulan (se spaw nean juntas
            // al vencer el deadline de la ÚLTIMA muerte).
            let mut q = self.world.resource_mut::<RespawnQueue>();
            match q.0.get_mut(&(map, index)) {
                Some((d, copies)) => {
                    *d = due;
                    *copies += 1;
                }
                None => {
                    q.0.insert((map, index), (due, 1));
                }
            }
        }
    }

    /// Resuelve el CG_ATTACK del jugador EN EL MUNDO (server-authoritative,
    /// parity `handle_attack` puro — cooldown/rango/daño). El cooldown vive
    /// en el componente `Combat` de la entidad del jugador. Emite
    /// `AttackResult` (paquetes + daño + estado del objetivo) o NADA si el
    /// ataque se rechazó (parity: el canal previo no enviaba nada).
    pub(crate) fn process_attack(
        &mut self,
        player_vid: u32,
        victim_vid: u32,
        b_type: u8,
        weapon: Option<&ProtoItem>,
        now_ms: u64,
    ) -> Vec<NpcEvent> {
        let Some(pe) = self.players.get(&player_vid).copied() else {
            return Vec::new();
        };
        let (px, py, level, ht, job, st, dx, iq, _armor, att_bonus, crit_pct, att_spd_bonus) = {
            let Ok(ent) = self.world.get_entity(pe) else { return Vec::new() };
            let Some(pos) = ent.get::<Position>() else { return Vec::new() };
            let Some(p) = ent.get::<Player>() else { return Vec::new() };
            let Some(aff) = ent.get::<Affects>() else { return Vec::new() };
            (pos.x, pos.y, p.level, p.ht, p.job, p.st, p.dx, p.iq, p.armor, aff.att_grade_bonus(), aff.critical_pct(), aff.att_speed_bonus())
        };
        let player_state = PlayerState {
            vid: player_vid,
            x: px,
            y: py,
            level,
            ht,
            job,
            st,
            dx,
            iq,
            // ATT_SPEED de los buffs (parity GET_ATTACK_SPEED battle.cpp:757-782).
            attack_speed_ms: attack_speed_for_weapon_bonus(weapon, att_spd_bonus),
            // El bonus de ATT_GRADE de los buffs (parity POINT_ATT_GRADE_BONUS).
            att_grade_bonus: att_bonus,
            // El % de crítico REAL de los buffs (parity POINT_CRITICAL_PCT).
            critical_pct: crit_pct,
        };
        let attack = protocol::combat::CgAttack {
            header: protocol::header::CG_ATTACK,
            b_type,
            victim_vid,
            crc_proc: 0,
            crc_file: 0,
        };
        // Resuelve el objetivo: mob materializado (NpcIndex) o — PvP — OTRO
        // jugador del mundo (`players` — un PC no está en el NpcIndex).
        let mut target = self.npc_view(victim_vid).map(|v| v.state);
        let pvp_victim = if target.is_none() {
            self.players.get(&victim_vid).copied()
        } else {
            None
        };
        // GATE PvP (parity `can_attack` — battle.cpp:83-139 + pvp.cpp:373-...; se
        // evalúa ANTES del cooldown, parity `CHARACTER::Attack`
        // char_battle.cpp:205-210): un PC→PC no atacable → sin evento
        // (parity: return false — el canal no mandaba nada).
        if let Some(ve) = pvp_victim {
            if !self.pvp_attackable(pe, ve) {
                return Vec::new();
            }
            target = self.player_npc_state(ve);
        }
        // El cooldown del jugador: componente Combat de su entidad (se
        // extrae y se devuelve — borrows secuenciales del mundo).
        let mut combat = {
            let Ok(mut ent) = self.world.get_entity_mut(pe) else { return Vec::new() };
            match ent.get_mut::<Combat>() {
                Some(mut c) => std::mem::replace(&mut c.0, CombatState::new()),
                None => return Vec::new(),
            }
        };
        let mut rng = self.world.resource_mut::<Rand>();
        let result = handle_attack(
            &mut combat,
            &attack,
            &player_state,
            target.as_ref(),
            weapon,
            now_ms,
            &mut |lo, hi| rng.roll(lo, hi),
        );
        // El borrow del RNG termina aquí (el xorshift es Copy — el writeback
        // del Combat necesita un borrow aparte del mundo).
        if let Ok(mut ent) = self.world.get_entity_mut(pe)
            && let Some(mut c) = ent.get_mut::<Combat>()
        {
            c.0 = combat;
        }
        if result.packets.is_empty() {
            return Vec::new(); // rechazado (cooldown/rango/sin objetivo)
        }
        let damage = result.damage;
        // PvP: el daño va al Hp del PC VÍCTIMA (igual que al mob) y se
        // emiten DOS eventos — el del atacante (`PvPAttackResult`) y el de
        // la víctima (`PvPVictimHit`): parity `SendDamagePacket`
        // (char_battle.cpp:1508-1527) manda el mismo GC_DAMAGE_INFO a
        // AMBOS descs; la víctima además recibe GC_POINTS (su barra) y
        // GC_DEAD si murió (flujo de muerte/revive compartido).
        if let Some(ve) = pvp_victim {
            let hp_after = {
                let Ok(mut ent) = self.world.get_entity_mut(ve) else {
                    return Vec::new(); // defensivo: el objetivo desapareció
                };
                let Some(mut hp) = ent.get_mut::<Hp>() else {
                    return Vec::new();
                };
                hp.hp = (hp.hp - damage).max(0);
                hp.hp
            };
            let dead = hp_after <= 0;
            return vec![
                CombatEvent::PvPAttackResult {
                    player_vid,
                    victim_vid,
                    packets: result.packets.clone(),
                    damage,
                    dead,
                    victim_hp: hp_after,
                }.into(),
                CombatEvent::PvPVictimHit {
                    player_vid: victim_vid,
                    attacker_vid: player_vid,
                    packets: result.packets,
                    damage,
                    dead,
                }.into(),
            ];
        }
        let Some(view) = self.npc_view(victim_vid) else {
            return Vec::new(); // defensivo: el objetivo desapareció
        };
        let mut damage = result.damage;
        let mut dead = false;
        let mut hp_after = view.hp;
        if damage > 0 {
            if let Some(dmg) = self.damage_npc(victim_vid, damage, Some(pe)) {
                dead = dmg.dead;
                hp_after = dmg.hp;
                if dead {
                    self.remove_npc(victim_vid); // el mundo quita el mob; la
                                                 // conexión hace la recompensa
                }
            } else {
                damage = 0; // defensivo: el objetivo desapareció
            }
        }
        let victim = Some(KillInfo {
            vnum: view.vnum,
            x: view.state.x,
            y: view.state.y,
            hp: hp_after,
            max_hp: view.max_hp,
            exp: view.exp,
            gold_min: view.gold_min,
            gold_max: view.gold_max,
            drop_item: view.drop_item,
            mob_level: view.state.level,
        });
        vec![CombatEvent::AttackResult {
            player_vid,
            victim_vid,
            packets: result.packets,
            damage,
            dead,
            victim,
        }.into()]
    }

    /// CG_TARGET del jugador (fix bug 5, 2026-08-15): responde con el HP%
    /// del mob apuntado (parity `SetTarget` char.cpp:5048-5094 — GC_TARGET
    /// 63 con bHPPercent = hp*100/max). Sin mob materializado para el vid →
    /// nada (el cliente mantiene la barra anterior; parity: SetTarget con
    /// nullptr manda vid 0/hp 0 — el subset no borra la barra).
    pub(crate) fn process_target(&mut self, player_vid: u32, target_vid: u32) -> Vec<NpcEvent> {
        let Some(view) = self.npc_view(target_vid) else {
            return Vec::new();
        };
        vec![CombatEvent::TargetResult {
            player_vid,
            vid: target_vid,
            hp: view.hp,
            max_hp: view.max_hp,
        }.into()]
    }

    /// `/kill` de GM (parity do_kill cmd_gm.cpp:1505+ → `SetDead` directo:
    /// SIN drop ni exp): mata el mob del target del GM. Emite `GmKilled`
    /// (el canal manda el GC_DEAD — animación de muerte — al GM) + un
    /// `Despawned` (GC_CHARACTER_DEL) a TODOS los jugadores del mapa del
    /// mob (el mob desaparece para todos — parity PacketView del C++). Sin
    /// mob para el vid (PC o inexistente) → sin eventos (parity: el subset
    /// solo apunta mobs; un PC no está en el NpcIndex). El respawn por
    /// tiempo de la entrada aplica (parity: un mob de un regen reaparece en
    /// su deadline — lo programa `remove_npc`).
    pub(crate) fn gm_kill(&mut self, player_vid: u32, target_vid: u32) -> Vec<NpcEvent> {
        let Some((vnum, mut events)) = self.gm_remove_mob(target_vid) else {
            return Vec::new();
        };
        let mut out = vec![CombatEvent::GmKilled { player_vid, vid: target_vid, vnum }.into()];
        out.append(&mut events);
        out
    }

    /// `/purge [all]` de GM (parity do_purge cmd_gm.cpp:775+ → FuncPurge +
    /// M2_DESTROY_CHARACTER: SIN drop ni exp): mata los mobs del mapa del
    /// GM — radio 1000 units sin `all` (parity `iDist >= 1000 → return`),
    /// TODO el mapa con `all`. Sin animación de muerte (destroy directo —
    /// solo GC_CHARACTER_DEL a los espectadores, igual que el C++). Los
    /// mobs de entradas con respawn reaparecen en su deadline (`remove_npc`).
    pub(crate) fn gm_purge(
        &mut self,
        _player_vid: u32,
        map_index: u32,
        x: i32,
        y: i32,
        all: bool,
    ) -> Vec<NpcEvent> {
        // Snapshot de vids ANTES de mutar (borrows secuenciales del mundo).
        let targets: Vec<u32> = self
            .world
            .resource::<NpcIndex>()
            .0
            .iter()
            .filter_map(|(vid, e)| {
                let Ok(ent) = self.world.get_entity(*e) else {
                    return None;
                };
                let map = ent.get::<Map>()?;
                if map.map_index != map_index {
                    return None;
                }
                let pos = ent.get::<Position>()?;
                if !all && distance_approx(pos.x - x, pos.y - y) >= 1000 {
                    return None; // parity FuncPurge: fuera de 1000 units
                }
                Some(*vid)
            })
            .collect();
        let mut events = Vec::new();
        for vid in targets {
            if let Some((_, mut ev)) = self.gm_remove_mob(vid) {
                events.append(&mut ev);
            }
        }
        events
    }

    /// Quita el mob del mundo + emite `Despawned` (GC_CHARACTER_DEL) a los
    /// jugadores de su mapa (la pieza común del kill/purge de GM — parity
    /// PacketView: todos los del mapa ven el mob desaparecer). None si el
    /// vid no es un mob materializado (PC o inexistente).
    fn gm_remove_mob(&mut self, target_vid: u32) -> Option<(i64, Vec<NpcEvent>)> {
        let (vnum, map_index) = self.npc_vnum_map(target_vid)?;
        self.remove_npc(target_vid);
        let mut events = Vec::new();
        for (pv, _, _) in self.map_players(map_index) {
            events.push(CombatEvent::Despawned { player_vid: pv, vid: target_vid }.into());
        }
        Some((vnum, events))
    }

    /// Sincroniza las stats del jugador (`/stat`/`/stat-` de GM — el AI las
    /// usa en `player_def_grade` y en el ataque del jugador; parity
    /// `SetRealPoint`+`SetPoint` del C++).
    pub(crate) fn set_player_stats(&mut self, player_vid: u32, st: i32, dx: i32, iq: i32, ht: i32) {
        let Some(e) = self.players.get(&player_vid).copied() else {
            return;
        };
        if let Ok(mut ent) = self.world.get_entity_mut(e)
            && let Some(mut p) = ent.get_mut::<Player>()
        {
            p.st = st;
            p.dx = dx;
            p.iq = iq;
            p.ht = ht;
        }
    }

    /// (vnum, map_index) del mob `vid` — None si no hay mob materializado
    /// (helper del GM kill/purge — `npc_view` no lleva el mapa).
    fn npc_vnum_map(&self, vid: u32) -> Option<(i64, u32)> {
        let e = *self.world.resource::<NpcIndex>().0.get(&vid)?;
        let ent = self.world.get_entity(e).ok()?;
        Some((ent.get::<Mob>()?.vnum, ent.get::<Map>()?.map_index))
    }

    /// (vid, x, y) de los jugadores del mapa `map_index` (broadcast del GM
    /// kill — parity PacketView: todos los del mapa ven el mob desaparecer).
    fn map_players(&self, map_index: u32) -> Vec<(u32, i32, i32)> {
        let mut out = Vec::new();
        for (pv, e) in &self.players {
            let Ok(ent) = self.world.get_entity(*e) else {
                continue;
            };
            let Some(map) = ent.get::<Map>() else {
                continue;
            };
            if map.map_index != map_index {
                continue;
            }
            let Some(pos) = ent.get::<Position>() else {
                continue;
            };
            out.push((*pv, pos.x, pos.y));
        }
        out
    }

    /// Sincroniza el HP del jugador (pociones/revive — la sesión ya aplicó el
    /// cambio a row.hp; el mundo lo refleja para el daño del AI).
    pub(crate) fn set_player_hp(&mut self, player_vid: u32, hp: i32) {
        let Some(e) = self.players.get(&player_vid).copied() else { return };
        if let Ok(mut ent) = self.world.get_entity_mut(e)
            && let Some(mut h) = ent.get_mut::<Hp>()
        {
            h.hp = hp;
        }
    }

    /// Sincroniza el SP del jugador (pociones/revive — el coste de las
    /// skills lo paga el mundo desde su componente Mp).
    pub(crate) fn set_player_mp(&mut self, player_vid: u32, mp: i32) {
        let Some(e) = self.players.get(&player_vid).copied() else { return };
        if let Ok(mut ent) = self.world.get_entity_mut(e)
            && let Some(mut m) = ent.get_mut::<Mp>()
        {
            m.mp = mp;
        }
    }

    /// Sincroniza el iArmor del jugador (equipar/desequipar — `equipped_armor`
    /// del canal; el AI lo usa en `player_def_grade`).
    pub(crate) fn set_player_armor(&mut self, player_vid: u32, armor: i32) {
        let Some(e) = self.players.get(&player_vid).copied() else { return };
        if let Ok(mut ent) = self.world.get_entity_mut(e)
            && let Some(mut p) = ent.get_mut::<Player>()
        {
            p.armor = armor;
        }
    }

    /// Sincroniza el nivel del jugador (level-up del kill — la DEF del AI).
    pub(crate) fn set_player_level(&mut self, player_vid: u32, level: i32) {
        let Some(e) = self.players.get(&player_vid).copied() else { return };
        if let Ok(mut ent) = self.world.get_entity_mut(e)
            && let Some(mut p) = ent.get_mut::<Player>()
        {
            p.level = level;
        }
    }

    /// Aplica un buff de ITEM (USE_ABILITY_UP — las pociones de buff del
    /// lane; parity `AddAffect` con bOverride=true, char_affect.cpp:518-590):
    /// mismo (dwType, bApplyOn) reemplaza al anterior y el nuevo entra al
    /// componente `Affects` — el combate lee de ahí ATT_SPEED /
    /// ATT_GRADE_BONUS / DEF_GRADE_BONUS / CRITICAL y el `affects_system` lo
    /// expira (→ `AffectRemoved` → GC_AFFECT_REMOVE en el canal). Los pools
    /// MAX_HP/MAX_SP no aplican: el switch USE_ABILITY_UP del C++ no los usa
    /// (solo POINT_* de buffs numéricos).
    pub(crate) fn set_player_affect(
        &mut self,
        player_vid: u32,
        dw_type: u32,
        point: u8,
        value: i32,
        flag: u32,
        duration_secs: i32,
    ) {
        let Some(e) = self.players.get(&player_vid).copied() else { return };
        if let Ok(mut ent) = self.world.get_entity_mut(e)
            && let Some(mut aff) = ent.get_mut::<Affects>()
        {
            // Override del mismo (dwType, bApplyOn) — parity bOverride.
            aff.0.retain(|a| !(a.skill_id == dw_type && a.point == point));
            aff.0.push(Affect {
                skill_id: dw_type,
                point,
                value,
                flag,
                duration_ms: u64::from(duration_secs.max(0) as u32) * 1000,
                sp_cost: 0,
            });
        }
    }

    /// Sincroniza el PK mode del jugador (CG_PVP 41 — el handler del canal
    /// lo manda al setear el flag de sesión; el gate `battle_is_attackable`
    /// del PvP lo consume).
    pub(crate) fn set_player_pvp_mode(&mut self, player_vid: u32, on: bool) {
        let Some(e) = self.players.get(&player_vid).copied() else { return };
        if let Ok(mut ent) = self.world.get_entity_mut(e)
            && let Some(mut p) = ent.get_mut::<Pvp>()
        {
            p.mode = on;
        }
    }

    /// Sincroniza la party del jugador (Joined/LeftParty del canal —
    /// "cannot attack same party", pvp.cpp:439-441).
    pub(crate) fn set_player_party(&mut self, player_vid: u32, party_id: Option<u32>) {
        let Some(e) = self.players.get(&player_vid).copied() else { return };
        if let Ok(mut ent) = self.world.get_entity_mut(e)
            && let Some(mut p) = ent.get_mut::<Pvp>()
        {
            p.party_id = party_id;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs::events::{CombatEvent, CombatIntent, MoveEvent, MoveIntent, NpcEvent, PlayerJoin};
    use crate::ecs::test_util::*;

    /// El CG_ATTACK se resuelve EN EL MUNDO: AttackResult con los paquetes
    /// del golpe y el estado del objetivo (el ninja lvl 5 hace 46-47 al mob
    /// 101 — el roll(0,1) del arma; la fórmula exacta la fijan los tests de
    /// game_core::combat).
    #[test]
    fn attack_intent_resolves_in_world() {
        let mut w = world_with(42);
        load(&mut w, vec![(entry(101, 0, 0, 1), mob_row(101))]);
        join(&mut w);
        let events = w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            1_000,
        );
        let attack = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::AttackResult { victim_vid, packets, damage, dead, victim, .. }) => {
                Some((*victim_vid, packets.clone(), *damage, *dead, *victim))
            }
            _ => None,
        });
        let (victim_vid, packets, damage, dead, victim) = attack.expect("AttackResult");
        assert_eq!(victim_vid, 10_000);
        assert_eq!(packets.len(), 1, "solo GcDamageInfo (fix 2026-08-14 — GcAttack 12 cerraba el cliente)");
        assert_eq!(packets[0][0], 135, "header GC_DAMAGE_INFO");
        assert!((46..=47).contains(&damage), "daño del ninja vs mob 101: {damage}");
        assert!(!dead);
        let v = victim.expect("víctima");
        assert_eq!(v.vnum, 101);
        assert_eq!(v.hp, 126 - damage, "hp tras el golpe");
        assert_eq!((v.x, v.y), (0, 0));
        // Sin respuesta para ataques rechazados (cooldown — parity).
        let events = w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            1_100,
        );
        assert!(events.is_empty(), "cooldown 1250 ms: rechazo sin evento");
    }

    /// Kill: el mundo quita el mob y el AttackResult lleva `dead` con la
    /// recompensa del mob (la conexión hace el reward/level-up).
    #[test]
    fn kill_removes_mob_and_reports_dead() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.max_hp = 10; // un golpe (46+) mata
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        join(&mut w);
        let events = w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            1_000,
        );
        let result = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::AttackResult { dead, damage, victim, .. }) => {
                Some((*dead, *damage, *victim))
            }
            _ => None,
        });
        let (dead, damage, victim) = result.expect("AttackResult");
        assert!(dead, "10 hp < 46 de daño");
        assert!(damage >= 46);
        let v = victim.expect("víctima");
        assert_eq!((v.exp, v.gold_min, v.gold_max, v.drop_item), (22, 15, 45, 101));
        assert_eq!(w.npc_count(), 0, "el mundo despawnó al mob muerto");
    }

    /// El mob aggro ataca al jugador (MobAttack con el daño del mob_proto —
    /// DEF del player lvl 5/ht 30 = 29 → floor 1..5) y el mundo aplica el
    /// daño al Hp del jugador.
    #[test]
    fn mob_attack_hits_player() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR".into()); // proactivo (sight 400)
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        let events = join(&mut w); // el primer tick: spawn + detect → AggroOn
        assert!(events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::AggroOn { .. }))), "{events:?}");
        // C29: el cooldown del golpe es 2000 ms (attack_speed 100) — un tick
        // de 2000 ms lo satisface (el mob está EN RANGO, no se mueve).
        let events = w.update(2_000);
        let attack = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::MobAttack { vid, damage, player_vid, .. }) => {
                Some((*vid, *damage, *player_vid))
            }
            _ => None,
        });
        let (vid, damage, player_vid) = attack.expect("MobAttack");
        assert_eq!(vid, 10_000);
        assert_eq!(player_vid, 2);
        assert!((1..=5).contains(&damage), "floor de CalcBattleDamage: {damage}");
        assert_eq!(w.player_hp(2), 100 - damage, "el mundo aplicó el daño");
    }

    /// AIFLAGs de combate del mob: BERSERK (daño ×2 bajo sp_berserk% HP,
    /// char_state.cpp:1016-1018 + char.cpp:1963-1964) y GODSPEED (ATT_SPEED
    /// 250 bajo sp_godspeed% — char_state.cpp:1021-1023 → cooldown de 1000
    /// ms en vez de 2000). STONESKIN (/2 al recibir) se prueba por separado
    /// en damage_npc.
    #[test]
    fn mob_combat_aiflags_berserk_and_godspeed() {
        // BERSERK: daño fijo 200; sp_berserk=30 con HP bajo → 200−def(5,30,0)
        // = 171... pero para ver el ×2 sin floor, def del player = 0 no
        // aplica aquí — usamos damage 200 y def del harness (29): 171 base,
        // 342 con berserk. El mob se daña AL 20% vía un ataque del jugador
        // (46 de daño → max_hp 60 → hp 14 = 23% < 30%).
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR,BERSERK".into()); // gate del flag (StateBattle:1015-1018)
        row.damage_min = 200;
        row.damage_max = 200;
        row.sp_berserk = 30;
        row.max_hp = 60; // 46 de daño del jugador → hp 14 (23% < 30%)
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        join(&mut w);
        // El jugador ataca al mob (daño 46 → hp 14).
        w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            1_000,
        );
        // C29 cooldown: 2000 ms (att_speed 100 — sin godspeed).
        let events = w.update(2_000);
        let attack = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::MobAttack { damage, .. }) => Some(*damage),
            _ => None,
        });
        // 200 − 29 = 171; berserk ×2 = 342 (parity GetMobDamageMultiply ×2).
        assert_eq!(attack, Some(342), "200−29=171 → berserk ×2 = 342");

        // GODSPEED: sp_godspeed=30, HP bajo → cooldown 1000 ms (att_speed
        // 250 → CalculateDuration(250, 2000) = 1000) en vez de 2000.
        let mut w3 = world_with(42);
        let mut row3 = mob_row(101);
        row3.ai_flag = Some("AGGR".into());
        row3.attack_speed = 100;
        row3.sp_godspeed = 30;
        row3.max_hp = 60;
        load(&mut w3, vec![(entry(101, 0, 0, 1), row3)]);
        join(&mut w3);
        w3.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            1_000,
        );
        // Un tick de 1000 ms dispara el golpe SOLO con godspeed (sin él
        // harían falta 2000).
        let events = w3.update(1_000);
        assert!(
            events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::MobAttack { .. }))),
            "godspeed: cooldown 1000 ms con att_speed 250 — el tick de 1000 dispara"
        );
    }

    /// C29 (test de PROTECCIÓN — hallazgo del verifier): el cooldown del
    /// golpe del mob DEBE silenciar los ticks < cooldown. Un mob en rango
    /// con att_speed 100 (cooldown 2000 ms) NO ataca con un tick de 1000 ms
    /// (la mutación "quitar el continue" hace fallar ESTE test).
    #[test]
    fn mob_attack_respects_cooldown_silently() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR".into());
        row.attack_speed = 100; // cooldown 2000 ms
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        join(&mut w); // spawn + AggroOn (mob EN RANGO)
        // Tick de 1000 ms: MENOR que el cooldown (2000) → SIN golpe.
        let events = w.update(1_000);
        assert!(
            !events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::MobAttack { .. }))),
            "cooldown 2000 ms: el tick de 1000 NO dispara el golpe — {events:?}"
        );
        // Tick adicional de 1000 ms: ahora 2000 ≥ cooldown → SÍ golpea.
        let events = w.update(1_000);
        assert!(
            events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::MobAttack { .. }))),
            "tras 2000 ms acumulados el mob golpea"
        );
    }

    /// STONESKIN: el mob bajo sp_stoneskin% HP recibe la MITAD del daño
    /// (parity `dam /= 2`, char_battle.cpp:2082-2084).
    #[test]
    fn mob_stoneskin_halves_incoming_damage() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.max_hp = 80; // el jugador pega ~46
        row.sp_stoneskin = 50;
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        join(&mut w);
        // Primer golpe: 80 → 34 (100% > 50% — el skin NO aplica).
        w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            1_000,
        );
        let hp1 = w.npc_view(10_000).expect("mob").hp;
        assert!(hp1 > 30 && hp1 < 36, "primer golpe sin skin (100% > 50%): hp {hp1}");
        // Segundo golpe (esperando el cooldown del jugador 1250 ms): hp
        // ~34/80 = 42% < 50% → el daño se divide (46/2 = 23 → ~11).
        w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            3_000,
        );
        let hp2 = w.npc_view(10_000).expect("mob").hp;
        assert!(
            hp2 >= 8 && hp2 <= 14,
            "segundo golpe CON skin (42% < 50%): 34 − ~23 = ~11, hp {hp2}"
        );
    }

    /// Persecución: el mob aggro fuera de rango da un paso exacto
    /// (move_speed 100 × 0.5 s = 50 units hacia el jugador) + rotación.
    #[test]
    fn aggro_mob_chases_player() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR".into());
        // C32: rank BOSS (4) — los bosses NO hacen change-attack-position
        // (char.cpp:5437); este test aísla la persecución DIRECTA.
        row.rank = 4;
        load(&mut w, vec![(entry(101, 400, 0, 1), row)]);
        join(&mut w); // spawn + AggroOn (dist 384 ≤ sight 400)
        let events = w.update(500);
        let moved = events.iter().find_map(|e| match e {
            NpcEvent::Move(MoveEvent::Moved { vid, x, y, rot, duration_ms, .. }) => {
                Some((*vid, *x, *y, *rot, *duration_ms))
            }
            _ => None,
        });
        let (vid, x, y, rot, dur) = moved.expect("paso hacia el jugador");
        // C30: la velocidad REAL = motion(300) × factor — move_speed 100 →
        // 300 u/s (el rewrite usaba la columna como u/s → 3× más lento).
        assert_eq!((vid, x, y), (10_000, 250, 0), "400 − 300 units/s × 0.5 s");
        assert_eq!(rot, 36, "oeste (180°/5) — el mob avanza hacia el origen");
        assert_eq!(dur, 500, "dw_duration = dist/move_speed = 150 u / 300 u/s = 500 ms (parity CalculateMoveDuration)");
        assert!(!events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::MobAttack { .. }))), "400 > rango melee");
    }

    /// De-aggro por distancia: el mob golpeado persigue hasta
    /// `aggressive_sight.max(2000)` y luego suelta (parity FindVictim).
    #[test]
    fn de_aggro_beyond_sight() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("NOMOVE".into());
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        join(&mut w);
        w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            1_000,
        ); // daño → aggro
        w.process_intent(MoveIntent::Move { player_vid: 2, x: 2_500, y: 0 }.into(), 2_000);
        let events = w.update(500);
        assert!(events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::AggroOff { .. }))), "{events:?}");
        assert!(!events.iter().any(|e| matches!(e, NpcEvent::Move(MoveEvent::Moved { .. }))));
    }

    /// Las stats de combate del jugador (level/ht/armor del PlayerJoin)
    /// alimentan la fórmula del ataque del mob: 100 − (6 + 24 + 12) = 58
    /// EXACTO (daño fijo sin sorteo → determinista con cualquier seed).
    #[test]
    fn player_combat_stats_feed_attack_formula() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR".into());
        row.damage_min = 100;
        row.damage_max = 100;
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        w.join_player_ready(PlayerJoin {
            vid: 2,
            map_index: 41,
            x: 0,
            y: 0,
            hp: 100,
            max_hp: 100,
            mp: 100,
            max_mp: 100,
            skill_level: Vec::new(),
            level: 6,
            ht: 30,
            armor: 12,
            job: 1,
            skill_group: 1,
            st: 30,
            dx: 30,
            iq: 30,
        });
        let events = w.update(2_000); // C29: cooldown 2000 ms — el mob ataca
        let attack = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::MobAttack { damage, .. }) => Some(*damage),
            _ => None,
        });
        assert_eq!(attack, Some(58), "100 − (6 + 24 + 12) — parity player_def_grade");
        assert_eq!(w.player_hp(2), 100 - 58, "el mundo aplicó el daño");
    }

    /// Multi-jugador: el aggro proactivo elige al jugador MÁS CERCANO del
    /// mismo mapa; los eventos llevan SU player_vid.
    #[test]
    fn aggro_targets_nearest_player() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR".into());
        load(&mut w, vec![(entry(101, 200, 0, 1), row)]);
        // El join de p2 corre el primer tick: spawn + detect → AggroOn{p2}.
        let join2 = join_at(&mut w, 2, 0, 0);
        assert!(
            join2.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::AggroOn { player_vid: 2, .. }))),
            "el jugador 2 es el más cercano: {join2:?}"
        );
        join_at(&mut w, 3, 5_000, 0); // lejos del mob (4800 > sight 400)
        let events = w.update(2_000); // C29: cooldown 2000 ms — el mob ataca
        let attack = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::MobAttack { player_vid, .. }) => Some(*player_vid),
            _ => None,
        });
        assert_eq!(attack, Some(2), "el ataque va al objetivo del aggro");
    }

    /// Fix bug 5 (2026-08-15): el CG_TARGET del jugador responde con el
    /// HP% del mob apuntado (parity SetTarget, char.cpp:5048-5094 — GC_TARGET
    /// 63). Un vid sin mob materializado -> nada.
    #[test]
    fn target_returns_mob_hp_percent() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.max_hp = 100;
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        join(&mut w);
        // El mob tiene 100 max — el target reporta hp/max sin dañar.
        let events = w.process_intent(
            CombatIntent::Target { player_vid: 2, target_vid: 10_000 }.into(),
            1_000,
        );
        let result = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::TargetResult { vid, hp, max_hp, .. }) => {
                Some((*vid, *hp, *max_hp))
            }
            _ => None,
        });
        assert_eq!(result, Some((10_000, 100, 100)), "hp/max del mob apuntado");
        // Vid sin entidad -> sin evento (el cliente mantiene la barra).
        let events = w.process_intent(
            CombatIntent::Target { player_vid: 2, target_vid: 99_999 }.into(),
            1_000,
        );
        assert!(events.is_empty(), "vid inexistente -> sin respuesta");
    }

    /// C28 (separación): `separate_landing` — el aterrizaje recto ocupado
    /// por otro mob se evita probando los flancos ±90°; si TODO está
    /// ocupado por un mob LEJANO -> None (no moverse este tick); si el
    /// bloqueo es de mobs YA pegados a mi posición -> avanzar igual (no
    /// congelar al par co-localizado del spawn).
    #[test]
    fn separate_landing_blocks_flanks_and_advances_when_adjacent() {
        use super::separate_landing;
        // Aterrizaje recto (50,0) libre -> se usa directo.
        assert_eq!(
            separate_landing(&[(10_001, 200, 0)], 10_000, (0, 0), 50, 0),
            Some((50, 0))
        );
        // Aterrizaje recto ocupado (mob a 0 u del landing) -> flanco ±90°
        // libre ((0,50): el mob queda a ~70 u).
        assert_eq!(
            separate_landing(&[(10_001, 50, 0)], 10_000, (0, 0), 50, 0),
            Some((0, 50)),
            "el primer flanco (rotación +90°)"
        );
        // Todos los destinos ocupados por mobs LEJANOS (a > 60 de mi
        // posición, pero ≤ 60 de cada candidato) -> None: no moverse este
        // tick (el mob espera a que el camino se despeje).
        let wall = [(10_001, 90, 0), (10_002, 0, 90), (10_003, 0, -90)];
        assert_eq!(separate_landing(&wall, 10_000, (0, 0), 50, 0), None);
        // Bloqueo por un mob YA pegado a mi posición -> avanzar igual
        // (el par co-localizado del spawn no se congela).
        assert_eq!(
            separate_landing(&[(10_001, 0, 0)], 10_000, (0, 0), 50, 0),
            Some((50, 0))
        );
        // El propio vid nunca bloquea (el snapshot incluye al mob mismo).
        assert_eq!(
            separate_landing(&[(10_000, 0, 0)], 10_000, (0, 0), 50, 0),
            Some((50, 0))
        );
    }

    /// C28 (separación POR MAPA — F1): el snapshot de `others` se filtra
    /// por el mapa del mob — 2 mobs de mapas DISTINTOS con coordenadas
    /// coincidentes NO se bloquean (el mundo materializa mobs de TODOS los
    /// mapas; sin el filtro, un mob de otro mapa sentado sobre el aterrizaje
    /// del paso desviaba el chase al flanco o lo congelaba).
    #[test]
    fn mobs_on_different_maps_never_block_each_other() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR".into()); // persigue al jugador del mapa 41
        // C32: rank BOSS (4) — sin change-attack-position: el paso es RECTO
        // (el test aísla la separación POR MAPA, no el reposicionamiento).
        row.rank = 4;
        // C30: el paso real es 150 u (300 u/s × 0.5 s) — el mob spawnea a
        // 600 para que el update (tras el join del jugador 3) PISE las
        // coords del mob 2101 (300,0) exactamente; sight ampliado a 700
        // para que el aggro proactivo lo alcance (600 ≤ 700).
        row.aggressive_sight = 700;
        let mut other_map = mob_row(2101);
        other_map.ai_flag = Some("NOMOVE".into()); // se queda quieto en (300,0)
        load(&mut w, vec![(entry(101, 600, 0, 1), row)]);
        w.load_table(42, vec![(entry(2101, 300, 0, 1), other_map)]);
        // Jugador 2 en el mapa 41: materializa el mob 101 (aggro → chase).
        let events = join_at(&mut w, 2, 0, 0);
        assert_eq!(w.npc_count(), 1, "solo el mob del mapa 41");
        assert!(events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::AggroOn { .. }))));
        // Jugador 3 en el mapa 42 (mismas coords del mundo): materializa el
        // mob 2101 — el primer tick del join también mueve al mob 101
        // (paso 600 → 450 con la velocidad real 300 u/s; el chase corre
        // DESPUÉS del detect en el mismo tick, parity del orden del canal).
        w.join_player_ready(PlayerJoin {
            vid: 3,
            map_index: 42,
            x: 0,
            y: 0,
            hp: 100,
            max_hp: 100,
            mp: 100,
            max_mp: 100,
            skill_level: Vec::new(),
            level: 5,
            ht: 30,
            armor: 0,
            job: 1,
            skill_group: 1,
            st: 30,
            dx: 30,
            iq: 30,
        });
        assert_eq!(w.npc_count(), 2, "el mob del mapa 42 también se materializó");
        // Tick siguiente: el mob 101 (en 450, fuera de rango 300) avanza
        // RECTO y pisa las coordenadas EXACTAS del mob del mapa 42
        // (450 → 300, paso de 150 u con la velocidad real 300 u/s) — sin el
        // filtro por mapa, el mob 2101 (a 0 u del aterrizaje) lo desviaría
        // al flanco (300, ∓50).
        let events = w.update(500);
        let moved = events.iter().find_map(|e| match e {
            NpcEvent::Move(MoveEvent::Moved { vid, x, y, .. }) => Some((*vid, *x, *y)),
            _ => None,
        });
        assert_eq!(moved, Some((10_000, 300, 0)), "paso recto: el mob del mapa 42 no bloquea");
        assert_eq!(w.npc_view(10_000).map(|v| (v.state.x, v.state.y)), Some((300, 0)));
        assert_eq!(w.npc_view(10_001).map(|v| (v.state.x, v.state.y)), Some((300, 0)), "el mob del mapa 42 sigue en su sitio");
    }

    /// C28 (amontonamiento): 2 mobs del MISMO entry persiguiendo al mismo
    /// jugador NUNCA ocupan la misma celda — el jitter del spawn los separa
    /// (parity `SpawnMobRange`, char_manager.cpp:545-563) y la separación
    /// del chase conserva la distancia cuando convergen (flanco/espera).
    /// Con seed 42 las posiciones son deterministas (incluido el jitter).
    #[test]
    fn mobs_chasing_same_player_never_share_a_cell() {
        use crate::ecs::world::WorldSim;
        use crate::npc::SpawnEntry;
        fn pos(w: &WorldSim, vid: u32) -> (i32, i32) {
            let v = w.npc_view(vid).expect("mob vivo");
            (v.state.x, v.state.y)
        }
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR".into());
        // C32: rank BOSS — persecución directa determinista (el test aísla
        // la SEPARACIÓN entre copias, no el reposicionamiento lateral).
        row.rank = 4;
        // 2 copias, rect ±2 celdas (200 u) alrededor de (0,0) — AMBOS caen
        // dentro del sight de aggro (400) y persiguen de verdad (un rect de
        // ±500 dejaría una copia a 455 > sight → patrulla sin target y el
        // test no prueba la separación en persecución).
        let e = SpawnEntry { w_x: 2, w_y: 2, ..entry(101, 0, 0, 2) };
        load(&mut w, vec![(e, row)]);
        join(&mut w); // spawn con jitter + AggroOn de ambos
        assert_eq!(w.npc_count(), 2);
        let (x0, y0) = pos(&w, 10_000);
        let (x1, y1) = pos(&w, 10_001);
        assert!(
            (x0, y0) != (x1, y1),
            "el jitter separó las copias en el spawn: ({x0},{y0}) vs ({x1},{y1})"
        );
        // Correr la persecución hasta que ambos ataquen (rango 300) — en
        // NINGÚN tick ocupan la misma celda.
        for _ in 0..30 {
            w.update(500);
            let (a, b) = (pos(&w, 10_000), pos(&w, 10_001));
            assert_ne!(a, b, "los mobs se apilaron: {a:?} vs {b:?}");
        }
        // Ambos llegaron al rango del jugador (atacan — no siguen moviéndose
        // y la posición final queda estable y separada).
        let (x0, y0) = pos(&w, 10_000);
        let (x1, y1) = pos(&w, 10_001);
        let d0 = crate::combat::distance_approx(x0, y0);
        let d1 = crate::combat::distance_approx(x1, y1);
        assert!(d0 <= 300, "mob A en rango: {d0}");
        assert!(d1 <= 300, "mob B en rango: {d1}");
    }

    /// C32: el change-attack-position — un mob rank < BOSS en persecución,
    /// con el timer vencido, se desvía del camino DIRECTO a la víctima hacia
    /// un punto lateral a `fMinDistance` (parity char.cpp:5436-5462); el
    /// test verifica que la posición final NO está en el eje directo
    /// (mismo x, y ≈ 0) y que la distancia a la víctima es ~fMinDistance.
    #[test]
    fn mob_repositions_laterally_when_pursuing() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR".into());
        row.rank = 0; // PAWN — reposiciona
        // Sight ampliado: el aggro proactivo alcanza al mob a 600.
        row.aggressive_sight = 700;
        // Mob a 600 u del jugador: primer tick del join → 450 (step 150).
        load(&mut w, vec![(entry(101, 600, 0, 1), row)]);
        join(&mut w); // spawn + AggroOn (600 ≤ sight 700)
        // Un tick más: el mob se mueve hacia el jugador. Luego, el timer FAR
        // (1 s a > 100+rango) expira y el mob elige un destino lateral.
        for _ in 0..4 {
            w.update(500);
        }
        let (x, y) = {
            let v = w.npc_view(10_000).expect("mob vivo");
            (v.state.x, v.state.y)
        };
        // Después de 2 s (4 ticks) de persecución a 300 u/s desde ~600,
        // el mob está cerca del jugador — la clave: NO en el eje x directo
        // (|y| > 0 significa el desvío lateral del change-attack-position).
        // El rng determinista con seed 42 da un ángulo no-trivial.
        assert!(y.abs() > 20, "el mob se desvió del eje directo (C32): ({x},{y})");
        // Y está a ≤ rango de ataque del jugador (o muy cerca — persiguiendo
        // el punto lateral que queda a fMinDistance ≈ 158 de la víctima).
        let d = crate::combat::distance_approx(x, y);
        assert!(d <= 300, "cerca del jugador: {d}");
    }

    /// C23 (respawn por tiempo — F2, caso `time > 0`): el mob MUERTO no
    /// reaparece hasta el intervalo del regen.txt (entry.time — parity
    /// `regen_event`, regen.cpp:582-600: el C++ re-spawnea cada `time`
    /// segundos) y luego re-materializa con vid FRESCO en su entrada.
    /// (El caso `time == 0` — SIN respawn — lo cubre
    /// `killed_mob_with_time_zero_never_respawns`.)
    #[test]
    fn killed_mob_respawns_after_regen_interval() {
        use crate::npc::SpawnEntry;
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.max_hp = 10; // un golpe (46+) mata
        let e = SpawnEntry { time: 2, ..entry(101, 0, 0, 1) }; // intervalo 2 s
        load(&mut w, vec![(e, row)]);
        join(&mut w);
        assert_eq!(w.npc_count(), 1);
        // Muerte (el reloj del mundo va en 500 ms — el del join).
        w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            1_000,
        );
        assert_eq!(w.npc_count(), 0, "muerto: despawn inmediato");
        // t+1.0 s y t+1.5 s: aún dentro del intervalo de 2 s -> sin spawn.
        assert_eq!(w.update(500).iter().filter(|e| matches!(e, NpcEvent::Combat(CombatEvent::Spawned { .. }))).count(), 0, "t+0.5 s");
        assert_eq!(w.update(500).iter().filter(|e| matches!(e, NpcEvent::Combat(CombatEvent::Spawned { .. }))).count(), 0, "t+1.0 s");
        assert_eq!(w.update(500).iter().filter(|e| matches!(e, NpcEvent::Combat(CombatEvent::Spawned { .. }))).count(), 0, "t+1.5 s");
        assert_eq!(w.npc_count(), 0, "aún pendiente");
        // t+2.0 s: vence el deadline -> la entrada re-materializa (el
        // jugador sigue en la vista) con un vid FRESCO (el allocador no
        // reusa — parity del flujo dinámico).
        let events = w.update(500);
        assert!(events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::Spawned { .. }))), "respawn: {events:?}");
        assert_eq!(w.npc_count(), 1, "reapareció tras el intervalo");
        assert!(w.npc_view(10_001).is_some(), "vid fresco 10_001 (el 10_000 murió)");
    }

    /// C23/F2 (time=0 → SIN respawn): las entradas con `time == 0` del
    /// regen.txt NO respawnean (parity regen_load: solo spawnea/re-spawnea
    /// con `time != 0`, regen.cpp:680-693 + regen_event auto-cancel,
    /// regen.cpp:582-600 — el runtime spain tiene ~2800 entradas "0s":
    /// holyplace_flame 2303, sungzi_flame_pass 2311 — mueren UNA vez y no
    /// vuelven NUNCA, ni al re-acercarse un jugador).
    #[test]
    fn killed_mob_with_time_zero_never_respawns() {
        use crate::npc::SpawnEntry;
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.max_hp = 10; // un golpe (46+) mata
        load(&mut w, vec![(SpawnEntry { time: 0, ..entry(101, 0, 0, 1) }, row)]);
        join(&mut w);
        assert_eq!(w.npc_count(), 1);
        // Muerte (el reloj del mundo va en 500 ms — el del join).
        w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            1_000,
        );
        assert_eq!(w.npc_count(), 0, "muerto: despawn inmediato");
        // 3 × 20 s = 60 s: el antiguo default de 60 s ya venció — la entrada
        // NO re-materializa (parity: el mob no vuelve).
        for _ in 0..3 {
            let events = w.update(20_000);
            assert!(
                !events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::Spawned { .. }))),
                "time=0 no respawnea: {events:?}"
            );
            assert_eq!(w.npc_count(), 0);
        }
        // Ni al salir y re-acercarse: el sentinel del RespawnQueue bloquea
        // la re-materialización del spawn dinámico (parity: el mob del C++
        // no reaparece aunque los players entren/salgan del área).
        w.process_intent(MoveIntent::Move { player_vid: 2, x: 10_800, y: 0 }.into(), 61_000);
        w.update(500);
        w.process_intent(MoveIntent::Move { player_vid: 2, x: 0, y: 0 }.into(), 62_000);
        let events = w.update(500);
        assert!(
            !events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::Spawned { .. }))),
            "no reaparece al volver el jugador: {events:?}"
        );
        assert_eq!(w.npc_count(), 0);
    }

    /// PvP básico: el ataque a OTRO JUGADOR (players map) con PK mode ON
    /// hace daño a su Hp y emite los DOS eventos — `PvPAttackResult`
    /// (atacante) y `PvPVictimHit` (víctima) con los MISMOS paquetes
    /// (parity `SendDamagePacket`, char_battle.cpp:1508-1527 — el
    /// GC_DAMAGE_INFO va a ambos descs). Sin PK mode en ninguno → el gate
    /// `can_attack` rechaza SIN evento (parity battle.cpp:83-139 — return
    /// false, el canal no mandaba nada). Ambos a nivel 20 (≥
    /// PK_PROTECT_LEVEL — el gate rechaza a los inferiores).
    #[test]
    fn pvp_attack_requires_pk_mode_and_damages_victim() {
        let mut w = world_with(42);
        join_pvp(&mut w, 2, 0, 0); // atacante (ninja lvl 20 del harness)
        join_pvp(&mut w, 3, 0, 0); // víctima (el mismo dummy)
        // PK OFF en ambos → el gate rechaza (PK_MODE_PEACE — el resto del
        // switch del C++ cae al duelo → false).
        let events = w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 3, b_type: 0, weapon: None }.into(),
            1_000,
        );
        assert!(events.is_empty(), "PK OFF → battle_is_attackable false: {events:?}");
        // Atacante PK ON → atacable: el golpe hace daño al Hp del jugador 3.
        w.process_intent(CombatIntent::SetPvpMode { player_vid: 2, on: true }.into(), 1_000);
        let events = w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 3, b_type: 0, weapon: None }.into(),
            1_000,
        );
        let atk = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::PvPAttackResult { victim_vid, packets, damage, dead, victim_hp, .. }) => {
                Some((*victim_vid, packets.clone(), *damage, *dead, *victim_hp))
            }
            _ => None,
        }).expect("PvPAttackResult");
        let hit = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::PvPVictimHit { player_vid, attacker_vid, packets, damage, dead, .. }) => {
                Some((*player_vid, *attacker_vid, packets.clone(), *damage, *dead))
            }
            _ => None,
        }).expect("PvPVictimHit");
        let (victim_vid, atk_packets, damage, dead, victim_hp) = atk;
        let (hit_vid, attacker_vid, hit_packets, hit_damage, hit_dead) = hit;
        assert_eq!(victim_vid, 3);
        assert_eq!(hit_vid, 3, "el evento de la víctima va a SU cola (routing)");
        assert_eq!(attacker_vid, 2);
        assert_eq!(atk_packets, hit_packets, "el mismo GC_DAMAGE_INFO a ambos descs");
        assert_eq!(atk_packets.len(), 1);
        assert_eq!(atk_packets[0][0], 135, "header GC_DAMAGE_INFO");
        assert!(!dead && !hit_dead);
        // DEF del PC víctima: player_def_grade(20, 30, 0) = 44 (char.cpp:
        // 2112-2114 — level + ht/1.25 + armor) → 87-44 = 43 / 88-44 = 44
        // (ATT_GRADE 100 lvl 20, fAR 0.7866 — la MISMA fórmula del mob,
        // con la def del PC como víctima).
        assert!((43..=44).contains(&damage), "daño del ninja vs PC lvl 20: {damage}");
        assert_eq!(hit_damage, damage);
        assert_eq!(victim_hp, 100 - damage, "hp del PC tras el golpe");
        assert_eq!(w.player_hp(3), 100 - damage, "el mundo aplicó el daño al Hp del PC");
        // La VÍCTIMA con PK ON también es atacable con el atacante OFF
        // (parity IsKillerMode — pvp.cpp:443-445). Esperando el cooldown
        // del jugador (1250 ms → 2_500).
        w.process_intent(CombatIntent::SetPvpMode { player_vid: 2, on: false }.into(), 2_000);
        w.process_intent(CombatIntent::SetPvpMode { player_vid: 3, on: true }.into(), 2_000);
        let events = w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 3, b_type: 0, weapon: None }.into(),
            2_500,
        );
        assert!(
            events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::PvPAttackResult { .. }))),
            "PK ON de la víctima → atacable: {events:?}"
        );
    }

    /// PvP: la misma PARTY bloquea el ataque aunque el atacante tenga PK
    /// ON (pvp.cpp:446-450 — "Cannot attack same party on any pvp model");
    /// al salir de la party el ataque YA pasa.
    #[test]
    fn pvp_attack_blocked_by_same_party() {
        let mut w = world_with(42);
        join_pvp(&mut w, 2, 0, 0);
        join_pvp(&mut w, 3, 0, 0);
        w.process_intent(CombatIntent::SetPvpMode { player_vid: 2, on: true }.into(), 1_000);
        w.process_intent(CombatIntent::SetParty { player_vid: 2, party_id: Some(7) }.into(), 1_000);
        w.process_intent(CombatIntent::SetParty { player_vid: 3, party_id: Some(7) }.into(), 1_000);
        let events = w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 3, b_type: 0, weapon: None }.into(),
            1_000,
        );
        assert!(events.is_empty(), "misma party → no atacable: {events:?}");
        // El jugador 3 sale de la party → el ataque YA pasa (cooldown intacto
        // — el gate corre ANTES, parity char_battle.cpp:205-210).
        w.process_intent(CombatIntent::SetParty { player_vid: 3, party_id: None }.into(), 1_000);
        let events = w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 3, b_type: 0, weapon: None }.into(),
            1_000,
        );
        assert!(
            events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::PvPAttackResult { .. }))),
            "parties distintas → atacable: {events:?}"
        );
    }

    /// PvP: la MUERTE del PC — hp ≤ 0 → `dead` en ambos eventos y el Hp
    /// del mundo a 0 (la víctima entra al flujo de muerte/revive del canal:
    /// GC_DEAD + CG_SCRIPT_ANSWER → script.rs). El muerto YA no es atacable
    /// (can_attack — IsDead, battle.cpp:116).
    #[test]
    fn pvp_kill_drops_victim_hp_to_zero() {
        let mut w = world_with(42);
        join_pvp(&mut w, 2, 0, 0);
        join_pvp(&mut w, 3, 0, 0);
        w.process_intent(CombatIntent::SetPvpMode { player_vid: 2, on: true }.into(), 1_000);
        // La víctima a 10 hp: un golpe (27-28) la mata.
        w.process_intent(CombatIntent::SetHp { player_vid: 3, hp: 10 }.into(), 1_000);
        let events = w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 3, b_type: 0, weapon: None }.into(),
            1_000,
        );
        let (dead, hp) = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::PvPAttackResult { dead, victim_hp, .. }) => {
                Some((*dead, *victim_hp))
            }
            _ => None,
        }).expect("PvPAttackResult");
        assert!(dead, "10 hp < 27 de daño → muere");
        assert_eq!(hp, 0);
        assert!(
            events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::PvPVictimHit { dead: true, .. }))),
            "el evento de la víctima marca la muerte"
        );
        assert_eq!(w.player_hp(3), 0, "el mundo dejó el Hp del PC a 0");
        // El muerto YA no es atacable (IsDead → false, battle.cpp:116).
        let events = w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 3, b_type: 0, weapon: None }.into(),
            2_500,
        );
        assert!(events.is_empty(), "víctima muerta → no atacable: {events:?}");
    }

    /// Lote 3 (GM `/kill` — parity do_kill → SetDead directo): el mob del
    /// target muere SIN recompensa — GmKilled (GC_DEAD del canal) al GM +
    /// Despawned (GC_CHARACTER_DEL) a los jugadores del mapa; un PC (o vid
    /// inexistente) NO se mata (parity: el subset solo apunta mobs). El
    /// mob de un entry con respawn (time != 0) reaparece en su deadline
    /// (parity: el C++ respawnea la entrada del regen).
    #[test]
    fn gm_kill_removes_target_mob_without_reward() {
        use crate::npc::SpawnEntry;
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("NOMOVE".into());
        let e = SpawnEntry { time: 2, ..entry(101, 0, 0, 1) }; // regen 2 s
        load(&mut w, vec![(e, row)]);
        join(&mut w); // jugador 2 + mob 10_000
        let events = w.process_intent(
            CombatIntent::GmKill { player_vid: 2, target_vid: 10_000 }.into(),
            1_000,
        );
        assert!(
            events.iter().any(|ev| matches!(
                ev,
                NpcEvent::Combat(CombatEvent::GmKilled { player_vid: 2, vid: 10_000, vnum: 101 })
            )),
            "GmKilled al GM: {events:?}"
        );
        assert!(
            events.iter().any(|ev| matches!(
                ev,
                NpcEvent::Combat(CombatEvent::Despawned { player_vid: 2, vid: 10_000 })
            )),
            "el GM ve el GC_CHARACTER_DEL: {events:?}"
        );
        assert_eq!(w.npc_count(), 0, "el mob se quita del mundo");
        // PC (el propio jugador) → no-op (parity: solo mobs).
        let events = w.process_intent(
            CombatIntent::GmKill { player_vid: 2, target_vid: 2 }.into(),
            1_000,
        );
        assert!(events.is_empty(), "un PC no se mata: {events:?}");
        // Respawn por tiempo de la entrada (parity regen_event): al vencer
        // el deadline (2 s) la entrada re-materializa con vid fresco.
        let events = w.update(2_500);
        assert!(
            events.iter().any(|ev| matches!(ev, NpcEvent::Combat(CombatEvent::Spawned { .. }))),
            "respawn de la entrada con regen: {events:?}"
        );
        assert_eq!(w.npc_count(), 1, "el regen reaparece");
    }

    /// Lote 3 (GM `/purge` — parity FuncPurge cmd_gm.cpp:757): sin `all`
    /// solo mueren los mobs a < 1000 units del GM (`iDist >= 1000 → return`);
    /// con `all` muere TODO el mapa. Sin animación de muerte (destroy — solo
    /// Despawned, sin GmKilled).
    #[test]
    fn gm_purge_radius_and_all() {
        let mut w = world_with(42);
        let mut far = mob_row(2101);
        far.ai_flag = Some("NOMOVE".into());
        load(&mut w, vec![
            (entry(101, 0, 0, 1), mob_row(101)),
            (entry(2101, 2_000, 0, 1), far), // fuera del radio 1000 del GM
        ]);
        join(&mut w); // jugador 2 en (0,0) — ambos dentro de SPAWN_VIEW
        // purge sin all: solo el mob cercano.
        let events = w.process_intent(
            CombatIntent::GmPurge { player_vid: 2, map_index: 41, x: 0, y: 0, all: false }.into(),
            1_000,
        );
        let dels: Vec<u32> = events
            .iter()
            .filter_map(|ev| match ev {
                NpcEvent::Combat(CombatEvent::Despawned { vid, .. }) => Some(*vid),
                _ => None,
            })
            .collect();
        assert_eq!(dels, vec![10_000], "solo el cercano: {events:?}");
        assert!(
            !events.iter().any(|ev| matches!(ev, NpcEvent::Combat(CombatEvent::GmKilled { .. }))),
            "purge = destroy directo, sin animación (parity M2_DESTROY_CHARACTER)"
        );
        assert_eq!(w.npc_count(), 1);
        assert!(w.npc_view(10_001).is_some(), "el lejano sobrevive");
        // purge all: mata el resto del mapa.
        let events = w.process_intent(
            CombatIntent::GmPurge { player_vid: 2, map_index: 41, x: 0, y: 0, all: true }.into(),
            1_000,
        );
        assert!(
            events.iter().any(|ev| matches!(
                ev,
                NpcEvent::Combat(CombatEvent::Despawned { player_vid: 2, vid: 10_001 })
            )),
            "all mata el lejano: {events:?}"
        );
        assert_eq!(w.npc_count(), 0);
    }

    /// Lote 3 (GM `/stat` — parity SetPoint): el sync de stats llega al
    /// componente Player del mundo (el AI las usa en player_def_grade y en
    /// el ataque del jugador).
    #[test]
    fn gm_set_stats_syncs_player_component() {
        let mut w = world_with(42);
        join(&mut w);
        w.process_intent(
            CombatIntent::SetStats { player_vid: 2, st: 50, dx: 60, iq: 70, ht: 80 }.into(),
            1_000,
        );
        let e = w.players.get(&2).copied().expect("jugador 2");
        let ent = w.world.get_entity(e).expect("entidad");
        let p = ent.get::<crate::ecs::components::Player>().expect("Player");
        assert_eq!((p.st, p.dx, p.iq, p.ht), (50, 60, 70, 80));
    }

    /// Verifier AIFLAGs (slice 2026-08-27): BERSERK requiere el flag del
    /// `ai_flag` (IsBerserker + `GetHPPct() < sp_berserk`, char_state.cpp:
    /// 1015-1018): sin flag 171, con flag ×2 = 342. COWARD: no ataca y con
    /// HP < 20% huye al lado opuesto.
    #[test]
    fn berserk_requires_flag_and_coward_flees() {
        for (flags, expected) in [("AGGR", 171), ("AGGR,BERSERK", 342)] { // HP 23% < sp_berserk 30
            let mut w = world_with(42);
            let mut row = mob_row(101);
            row.ai_flag = Some(flags.into());
            row.damage_min = 200;
            row.damage_max = 200;
            row.sp_berserk = 30;
            row.max_hp = 60;
            load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
            join(&mut w);
            w.process_intent(CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(), 1_000);
            let damage = w.update(2_000).iter().find_map(|e| match e {
                NpcEvent::Combat(CombatEvent::MobAttack { damage, .. }) => Some(*damage),
                _ => None,
            });
            assert_eq!(damage, Some(expected), "flags {flags}: 200−29=171 → {expected}");
        }
        // COWARD (AGGR, en rango): no ataca; HP 8% → el mob (100,0) huye al
        // lado opuesto del jugador (0,0) → (200,0).
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR,COWARD".into());
        row.max_hp = 100;
        load(&mut w, vec![(entry(101, 100, 0, 1), row)]);
        join(&mut w);
        w.process_intent(CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(), 1_000);
        w.process_intent(CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(), 3_000); // hp 8 (8%)
        let run = w.update(2_000);
        assert_eq!(
            run.iter().find_map(|e| match e { NpcEvent::Move(MoveEvent::Moved { x, .. }) => Some(*x), _ => None }), Some(200),
            "huye al lado opuesto: {run:?}"
        );
        assert!(!run.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::MobAttack { .. }))), "no ataca huyendo");
    }
}
