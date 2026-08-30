//! Dominio MOVIMIENTO del mundo (C5): el sistema 3 del tick (`patrol_system`
//! — el patrullaje idle de los mobs) + el sync de posición del jugador
//! (`set_player_position` — CG_MOVE aceptado).

use std::collections::HashMap;

use bevy_ecs::prelude::*;

use crate::ai::{mob_move_speed, move_duration_ms, patrol_step, rotation_5deg, step_toward};
use crate::combat::distance_approx;
use crate::ecs::components::{Aggro, Map, Mob, Player, Position, Vid};
use crate::ecs::events::MoveEvent;
use crate::ecs::resources::{NpcOutbox, Rand, Tick};
use crate::ecs::systems::combat::separate_landing;
use crate::ecs::world::WorldSim;

/// Radio del patrullaje (units del spawn — parity del estado IDLE del C++).
const PATROL_RADIUS: i32 = 1_500;
/// Rango de visión del patrullaje (units del jugador — mismo radio que el
/// spawn dinámico).
const PATROL_VIEW: i32 = crate::ecs::systems::spawn::SPAWN_VIEW;
/// Límite de GC_MOVE de patrulla por tick (no floodear al cliente).
const PATROL_MAX_SENDS: usize = 20;

/// 3) PATRULLAJE idle (parity channel.rs:1816-1884 — `patrol_step`,
///    char_state.cpp:668-688): los mobs NO aggro y NO NOMOVE caminan cerca de
///    su spawn — probabilidad 1/7 por tick, destino aleatorio 300-700 units
///    dentro del radio del spawn. Solo los VISIBLES para algún jugador de su
///    mapa, con tope de paquetes por tick (multi-jugador: el GC_MOVE se
///    difunde a todos los que lo ven).
#[allow(clippy::type_complexity)] // firma de sistema bevy (ParamSet con 2 queries)
pub(crate) fn patrol_system(
    mut mobs: ParamSet<(
        // Posiciones de TODOS los mobs (C28 — separación; read-only).
        Query<(&Vid, &Position, &Map), Without<Player>>,
        Query<(&Vid, &Mob, &Aggro, &Map, &mut Position), Without<Player>>,
    )>,
    players: Query<(&Player, &Map, &Position), Without<Mob>>,
    tick: Res<Tick>,
    mut rng: ResMut<Rand>,
    mut outbox: ResMut<NpcOutbox>,
) {
    let mut by_map: HashMap<u32, Vec<(u32, i32, i32)>> = HashMap::new();
    for (p, map, pos) in &players {
        by_map
            .entry(map.map_index)
            .or_default()
            .push((p.vid, pos.x, pos.y));
    }
    // C28: snapshot de las posiciones de los mobs AGRUPADO POR MAPA (la
    // separación consulta a los OTROS mobs del MISMO mapa — el ParamSet
    // evita el conflicto de queries; F1: los mobs de otros mapas con coords
    // coincidentes NO deben bloquear el paso de patrulla).
    let mut others_by_map: HashMap<u32, Vec<(u32, i32, i32)>> = HashMap::new();
    for (v, p, m) in &mobs.p0() {
        others_by_map
            .entry(m.map_index)
            .or_default()
            .push((v.vid, p.x, p.y));
    }
    let mut sent = 0usize;
    for (vid, mob, aggro, map, mut pos) in &mut mobs.p1() {
        if aggro.target.is_some() || mob.nomove {
            continue;
        }
        let Some(viewers) = by_map.get(&map.map_index) else {
            continue;
        };
        let visible: Vec<u32> = viewers
            .iter()
            .filter(|(_, x, y)| distance_approx(pos.x - x, pos.y - y) <= PATROL_VIEW)
            .map(|(pv, _, _)| *pv)
            .collect();
        if visible.is_empty() {
            continue;
        }
        if sent >= PATROL_MAX_SENDS {
            break;
        }
        let Some((tx, ty)) = patrol_step(
            pos.x,
            pos.y,
            mob.home_x,
            mob.home_y,
            PATROL_RADIUS,
            &mut |lo, hi| rng.roll(lo, hi),
        ) else {
            continue;
        };
        let (sx, sy) = step_toward(
            pos.x,
            pos.y,
            tx,
            ty,
            mob_move_speed(mob.move_speed) as i32,
            tick.dt_ms,
        );
        if (sx, sy) == (pos.x, pos.y) {
            continue;
        }
        // C28 (separación): el destino del paso de PATRULLA debe quedar
        // libre de otros mobs del MISMO mapa — si está ocupado, probar
        // flancos o no moverse (F1: los mobs de otros mapas no bloquean).
        let Some((nx, ny)) = separate_landing(
            others_by_map
                .get(&map.map_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            vid.vid,
            (pos.x, pos.y),
            sx,
            sy,
        ) else {
            continue; // otro mob bloquea — no moverse este tick
        };
        let rot = rotation_5deg(pos.x, pos.y, nx, ny);
        // La duración REAL del paso (parity CalculateMoveDuration,
        // char.cpp:2765-2768) — el cliente interpola con ESTA duración.
        let duration_ms = move_duration_ms(
            nx - pos.x,
            ny - pos.y,
            mob_move_speed(mob.move_speed) as i32,
        );
        pos.x = nx;
        pos.y = ny;
        for pv in &visible {
            outbox.0.push(
                MoveEvent::Moved {
                    player_vid: *pv,
                    vid: vid.vid,
                    x: nx,
                    y: ny,
                    rot,
                    duration_ms,
                }
                .into(),
            );
        }
        sent += visible.len();
    }
}

impl WorldSim {
    /// Sincroniza la posición del jugador (CG_MOVE aceptado — el AI persigue
    /// la posición NUEVA y el spawn dinámico se evalúa desde ella).
    pub(crate) fn set_player_position(&mut self, player_vid: u32, x: i32, y: i32) {
        let Some(e) = self.players.get(&player_vid).copied() else {
            return;
        };
        if let Ok(mut ent) = self.world.get_entity_mut(e)
            && let Some(mut pos) = ent.get_mut::<Position>()
        {
            pos.x = x;
            pos.y = y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::events::{CombatEvent, CombatIntent, MoveEvent, MoveIntent, NpcEvent};
    use crate::ecs::test_util::*;

    /// NOMOVE: nunca patrulla (parity AIFLAG_NOMOVE — char_state.cpp:668).
    #[test]
    fn nomove_mobs_never_patrol() {
        let mut w = world_with(7);
        let mut row = mob_row(101);
        row.ai_flag = Some("NOMOVE".into());
        load(&mut w, vec![(entry(101, 100, 100, 1), row)]);
        join(&mut w);
        for _ in 0..20 {
            let events = w.update(500);
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, NpcEvent::Move(MoveEvent::Moved { .. }))),
                "NOMOVE no patrulla: {events:?}"
            );
        }
    }

    /// Patrullaje idle: el mob se mueve SOLO dentro del radio del spawn
    /// (1500 units del home) y a lo sumo un paso por tick.
    #[test]
    fn idle_mob_patrols_within_spawn_radius() {
        let mut w = world_with(7);
        load(&mut w, vec![(entry(101, 0, 0, 1), mob_row(101))]);
        join(&mut w);
        for _ in 0..30 {
            let events = w.update(500);
            assert!(events.len() <= 1, "a lo sumo un paso por tick: {events:?}");
            for e in &events {
                if let NpcEvent::Move(MoveEvent::Moved { x, y, .. }) = e {
                    let d = crate::combat::distance_approx(*x, *y);
                    assert!(d <= PATROL_RADIUS, "patrulla fuera del radio: ({x},{y})");
                }
            }
        }
    }

    /// Syncs del jugador vía intents (pociones → SetHp, MOVE → posición).
    #[test]
    fn player_sync_intents_update_world() {
        let mut w = world_with(42);
        join(&mut w);
        w.process_intent(
            CombatIntent::SetHp {
                player_vid: 2,
                hp: 60,
            }
            .into(),
            1_000,
        );
        assert_eq!(w.player_hp(2), 60);
        w.process_intent(
            CombatIntent::SetArmor {
                player_vid: 2,
                armor: 12,
            }
            .into(),
            2_000,
        );
        w.process_intent(
            CombatIntent::SetLevel {
                player_vid: 2,
                level: 6,
            }
            .into(),
            3_000,
        );
        // La posición nueva: un mob aggro en (0,0) persigue (400,400) — dist
        // approx 543 > rango melee 300 → Moved, no MobAttack.
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR".into());
        row.aggressive_sight = 2_000;
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        w.process_intent(
            MoveIntent::Move {
                player_vid: 2,
                x: 400,
                y: 400,
            }
            .into(),
            4_000,
        );
        // Primer tick: spawn (543 ≤ SPAWN_VIEW) + detect → AggroOn. El chase
        // (Moved) llega en el tick SIGUIENTE (parity del orden del canal).
        let events = w.update(500);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, NpcEvent::Combat(CombatEvent::AggroOn { .. }))),
            "el aggro del mob se detecta: {events:?}"
        );
        let events = w.update(500);
        let moved = events.iter().find_map(|e| match e {
            NpcEvent::Move(MoveEvent::Moved { x, y, .. }) => Some((*x, *y)),
            _ => None,
        });
        let (x, y) = moved.expect("paso hacia la posición nueva");
        assert!(x > 0 && y > 0, "el mob avanza hacia (400,400): ({x},{y})");
    }
}
