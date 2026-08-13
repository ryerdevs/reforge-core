//! Dominio COMBATE del mundo (C5): los sistemas 1-2 del tick (`chase_attack`,
//! `aggro_detect`) + los métodos del `WorldSim` del combate (`process_attack`
//! y sus helpers `npc_view`/`damage_npc`/`remove_npc`) + los syncs de stats
//! del jugador que alimentan las fórmulas (hp/sp/armor/level).

use bevy_ecs::prelude::*;

use crate::ai::{attack_damage, rotation_5deg, step_toward};
use crate::combat::{
    attack_speed_for_weapon, distance_approx, handle_attack, melee_max_range,
    player_def_grade, CombatState, NpcState, PlayerState,
};
use crate::ecs::components::{
    Affects, Aggro, Combat, Hp, Map, Mob, Mp, Player, Position, Vid,
};
use crate::ecs::events::{CombatEvent, KillInfo, MoveEvent, NpcEvent};
use crate::ecs::resources::{NpcIndex, NpcOutbox, Rand, Tick};
use crate::ecs::world::WorldSim;
use database::item::ProtoItem;

/// Floor del de-aggro por distancia: un mob con sight 0 (nunca proactivo)
/// pero GOLPEADO por el jugador sigue persiguiendo un mínimo (channel.rs).
const DE_AGGRO_FLOOR: i32 = 2_000;

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
    mut mobs: Query<(&Vid, &Mob, &mut Aggro, &mut Position), Without<Player>>,
    mut players: Query<(Entity, &Player, &Position, &mut Hp, &Affects), Without<Mob>>,
    tick: Res<Tick>,
    mut rng: ResMut<Rand>,
    mut outbox: ResMut<NpcOutbox>,
) {
    for (vid, mob, mut aggro, mut pos) in &mut mobs {
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
        let (nx, ny) = step_toward(pos.x, pos.y, ppos.x, ppos.y, mob.move_speed, tick.dt_ms);
        if (nx, ny) == (pos.x, pos.y) {
            continue; // ya en el jugador (o speed 0)
        }
        let rot = rotation_5deg(pos.x, pos.y, nx, ny);
        pos.x = nx;
        pos.y = ny;
        outbox.0.push(MoveEvent::Moved {
            player_vid: p.vid,
            vid: vid.vid,
            x: nx,
            y: ny,
            rot,
            duration_ms: tick.dt_ms as u32,
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

    /// Quita el mob del mundo (muerte o despawn). Idempotente.
    pub(crate) fn remove_npc(&mut self, vid: u32) {
        if let Some(e) = self.world.resource_mut::<NpcIndex>().0.remove(&vid)
            && self.world.get_entity(e).is_ok()
        {
            self.world.despawn(e);
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
        assert_eq!(packets.len(), 2, "GcAttack + GcDamageInfo (daño > 0)");
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
        assert_eq!(dur, 500, "dw_duration = el dt del tick");
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
}
