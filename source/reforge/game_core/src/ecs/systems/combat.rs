//! Dominio COMBATE del mundo (C5): los sistemas 1-2 del tick (`chase_attack`,
//! `aggro_detect`) + los métodos del `WorldSim` del combate (`process_attack`
//! y sus helpers `npc_view`/`damage_npc`/`remove_npc`) + los syncs de stats
//! del jugador que alimentan las fórmulas (hp/sp/armor/level).

use bevy_ecs::prelude::*;

use crate::ai::{attack_damage, move_duration_ms, rotation_5deg, step_toward};
use crate::combat::{
    attack_speed_for_weapon, distance_approx, handle_attack, melee_max_range,
    player_def_grade, CombatState, NpcState, PlayerState,
};
use crate::ecs::components::{
    Affects, Aggro, Combat, Hp, Map, Mob, Mp, Player, Position, SpawnRef, Vid,
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
pub(crate) fn chase_attack_system(
    mut mobs: ParamSet<(
        // Posiciones de TODOS los mobs (C28 — separación; read-only).
        Query<(&Vid, &Position), Without<Player>>,
        Query<(&Vid, &Mob, &mut Aggro, &mut Position), Without<Player>>,
    )>,
    mut players: Query<(Entity, &Player, &Position, &mut Hp, &Affects), Without<Mob>>,
    tick: Res<Tick>,
    mut rng: ResMut<Rand>,
    mut outbox: ResMut<NpcOutbox>,
) {
    // C28: snapshot de las posiciones de los mobs (la separación consulta
    // a los OTROS mobs — el ParamSet evita el conflicto de queries).
    let others: Vec<(u32, i32, i32)> = mobs
        .p0()
        .iter()
        .map(|(v, p)| (v.vid, p.x, p.y))
        .collect();
    for (vid, mob, mut aggro, mut pos) in &mut mobs.p1() {
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
        if dist <= melee_max_range(&state) {
            // EN RANGO: ataque del mob — daño = atk del mob_proto − DEF del
            // jugador (parity char.cpp:2114 + items ARMOR —
            // `player_def_grade`; + el bonus de DEF_GRADE de los buffs —
            // parity POINT_DEF_GRADE_BONUS).
            let damage = attack_damage(
                mob.damage_min,
                mob.damage_max,
                player_def_grade(p.level, p.ht, p.armor) + paff.def_grade_bonus(),
                &mut |lo, hi| rng.roll(lo, hi),
            );
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
        // Persecución: paso hacia el jugador a `move_speed` (parity
        // `step_toward` — el canal difundía el GC_MOVE FUNC_MOVE).
        let (sx, sy) = step_toward(pos.x, pos.y, ppos.x, ppos.y, mob.move_speed, tick.dt_ms);
        if (sx, sy) == (pos.x, pos.y) {
            continue; // ya en el jugador (o speed 0)
        }
        // C28 (separación): el destino del paso debe quedar libre de otros
        // mobs — si está ocupado, probar flancos o no moverse este tick.
        let Some((nx, ny)) = separate_landing(&others, vid.vid, (pos.x, pos.y), sx, sy) else {
            continue; // otro mob bloquea — no moverse este tick
        };
        let rot = rotation_5deg(pos.x, pos.y, nx, ny);
        // La duración REAL del paso (parity CalculateMoveDuration,
        // char.cpp:2765-2768) — el cliente interpola con ESTA duración; el
        // dt del tick fijo animaba los pasos largos a velocidad altísima.
        let duration_ms = move_duration_ms(nx - pos.x, ny - pos.y, mob.move_speed);
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
    pub(crate) fn npc_view(&self, vid: u32) -> Option<NpcView> {
        let e = *self.world.resource::<NpcIndex>().0.get(&vid)?;
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

    /// Aplica `damage` al HP del mob (clamp a 0) y le marca AGGRO contra el
    /// jugador que golpeó (parity: el C++ marca el aggro en `OnDamage`).
    /// Devuelve el HP tras el golpe + si murió; None si el mob no existe.
    pub(crate) fn damage_npc(&mut self, vid: u32, damage: i32, aggro_to: Option<Entity>) -> Option<NpcDamage> {
        let e = *self.world.resource::<NpcIndex>().0.get(&vid)?;
        let hp_after = {
            let mut ent = self.world.get_entity_mut(e).ok()?;
            let mut hp = ent.get_mut::<Hp>()?;
            hp.hp = (hp.hp - damage).max(0);
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
    /// — 5s/30s/60s/3600s en el runtime) o 60 s por defecto si la entrada
    /// no tiene intervalo (tests).
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
            // Default 60 s si la entrada no declara intervalo (el C++ solo
            // re-spawnea entradas con `time != 0`, regen.cpp:682-693; las
            // entradas sin intervalo del runtime no existen — el loader las
            // salta — así que el default solo aplica a los tests).
            let interval = if interval == 0 { 60 } else { interval };
            let due = self.world.resource::<WorldClock>().0 + u64::from(interval) * 1000;
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
        let (px, py, level, ht, job, st, dx, iq, _armor, att_bonus) = {
            let Ok(ent) = self.world.get_entity(pe) else { return Vec::new() };
            let Some(pos) = ent.get::<Position>() else { return Vec::new() };
            let Some(p) = ent.get::<Player>() else { return Vec::new() };
            let Some(aff) = ent.get::<Affects>() else { return Vec::new() };
            (pos.x, pos.y, p.level, p.ht, p.job, p.st, p.dx, p.iq, p.armor, aff.att_grade_bonus())
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
            attack_speed_ms: attack_speed_for_weapon(weapon),
            // El bonus de ATT_GRADE de los buffs (parity POINT_ATT_GRADE_BONUS).
            att_grade_bonus: att_bonus,
        };
        let attack = protocol::combat::CgAttack {
            header: protocol::header::CG_ATTACK,
            b_type,
            victim_vid,
            crc_proc: 0,
            crc_file: 0,
        };
        let target = self.npc_view(victim_vid).map(|v| v.state);
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
        let events = w.update(500);
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

    /// Persecución: el mob aggro fuera de rango da un paso exacto
    /// (move_speed 100 × 0.5 s = 50 units hacia el jugador) + rotación.
    #[test]
    fn aggro_mob_chases_player() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR".into());
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
        assert_eq!((vid, x, y), (10_000, 350, 0), "400 − 100 units/s × 0.5 s");
        assert_eq!(rot, 36, "oeste (180°/5) — el mob avanza hacia el origen");
        assert_eq!(dur, 500, "dw_duration = dist/move_speed = 50 u / 100 u/s = 500 ms (parity CalculateMoveDuration)");
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
            st: 30,
            dx: 30,
            iq: 30,
        });
        let events = w.update(500);
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
        let events = w.update(500);
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
        // 2 copias, rect ±5 celdas (500 u) alrededor de (0,0).
        let e = SpawnEntry { w_x: 5, w_y: 5, ..entry(101, 0, 0, 2) };
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

    /// C23 (respawn por tiempo): el mob MUERTO no reaparece hasta el
    /// intervalo del regen.txt (entry.time — parity `regen_event`,
    /// regen.cpp:582-600: el C++ re-spawnea cada `time` segundos) y luego
    /// re-materializa con vid FRESCO en su entrada.
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
}
