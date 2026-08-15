//! Dominio SPAWN del mundo (C5): el sistema 0 del tick (`spawn_despawn`) +
//! los métodos del `WorldSim` que materializan/desmaterializan. Las
//! constantes `SPAWN_VIEW`/`DESPAWN_RADIUS` (parity channel.rs) viven aquí
//! y se re-exportan en `ecs::mod` (API estable).

use std::collections::{HashMap, HashSet};

use bevy_ecs::prelude::*;

use crate::combat::distance_approx;
use crate::ecs::components::{Aggro, Hp, Map, Mob, Player, Position, SpawnRef, SpawnSeen, Vid};
use crate::ecs::events::CombatEvent;
use crate::ecs::resources::{
    NpcIndex, NpcOutbox, Rand, RespawnQueue, SpawnCache, SpawnTable, VidAlloc, WorldClock,
    WorldMetrics,
};
use crate::ecs::world::WorldSim;
use crate::npc::{entry_spawns, SpawnEntry, SpawnKind};
use database::npc::{MobRepo, MobRow};

/// Rango de materialización del spawn dinámico (units). 8000 (petición
/// explícita del usuario 2026-08-13: "la distancia de visión para cargar las
/// cosas es muy pequeña, agrándala aún más" — ampliado de 5000); el coste del
/// loop por tick (10k entradas × checks de distancia) es despreciable.
pub const SPAWN_VIEW: i32 = 8_000;
/// Radio de desmaterialización (units) — margen de histéresis sobre el
/// spawn (evita el flapping en el borde). Documentado: los mobs aparecen a
/// ≤ 8000 y desaparecen a > 10000 de TODOS los jugadores.
pub const DESPAWN_RADIUS: i32 = 10_000;

/// 0) SPAWN/DESPAWN DINÁMICO — el fix del mundo vacío (parity sectree): los
///    mobs se MATERIALIZAN cuando un jugador de su mapa está a ≤ SPAWN_VIEW
///    (8000) de su punto de spawn y se DESMATERIALIZAN cuando están a
///    más de DESPAWN_RADIUS (10000) de TODOS los jugadores (o el mapa se quedó
///    sin jugadores). Los mobs EN COMBATE (hp < max o con aggro) NO se
///    desmaterializan (parity: el C++ no los quita del sectree — evita el
///    reset de HP al kite). Los ADD(+INFO) los construye `entry_spawns`
///    (puro) — el cliente los recibe al ACERCARSE.
///
///    REGRESIÓN bench 2026-08-13: la EMISIÓN del ADD es PER-JUGADOR (parity
///    sectree: cada vista recibe su ADD) — antes ocurría SOLO al materializar
///    y `materialized` saltaba las entradas ya creadas → solo el primer
///    jugador veía los mobs. Ahora: (A) materializar + ADD a los espectadores
///    iniciales; (B) re-emitir el ADD a los jugadores que entran en la vista
///    de una entidad YA materializada (`SpawnSeen` — sin duplicados por
///    jugador, re-ADD al volver a la vista); (C) despawn intactos.
#[allow(clippy::too_many_arguments, clippy::type_complexity)] // firma de sistema bevy
pub(crate) fn spawn_despawn_system(
    mut commands: Commands,
    table: Res<SpawnTable>,
    mut vids: ResMut<VidAlloc>,
    mut npc_index: ResMut<NpcIndex>,
    mut metrics: ResMut<WorldMetrics>,
    now: Res<WorldClock>,
    mut respawns: ResMut<RespawnQueue>,
    mut rng: ResMut<Rand>,
    spawn_refs: Query<&SpawnRef>,
    players: Query<(&Player, &Map, &Position), Without<Mob>>,
    mut mobs: Query<(&Vid, &Map, &Position, &SpawnRef, &mut SpawnSeen), Without<Player>>,
    mobs_state: Query<(Entity, &Vid, &Map, &Position, &Hp, &Aggro, &SpawnRef), Without<Player>>,
    mut outbox: ResMut<NpcOutbox>,
) {
    // Jugadores por mapa: (vid, x, y) — una sola pasada para ambos pases.
    let mut players_by_map: HashMap<u32, Vec<(u32, i32, i32)>> = HashMap::new();
    for (p, map, pos) in &players {
        players_by_map.entry(map.map_index).or_default().push((p.vid, pos.x, pos.y));
    }

    // --- SPAWN: materializar entradas sin entidad a ≤ SPAWN_VIEW de algún
    // jugador (la EMISIÓN del ADD es por jugador — pase B para las ya
    // materializadas) ---
    // C23/F3: copias VIVAS por entrada — el top-up del respawn las cuenta
    // (parity `regen_spawn`: `num = max_count - count`, regen.cpp:325).
    let mut alive_count: HashMap<(u32, usize), u32> = HashMap::new();
    for r in &spawn_refs {
        *alive_count.entry((r.map, r.index)).or_insert(0) += 1;
    }
    for (map_index, entries) in &table.maps {
        let Some(viewers) = players_by_map.get(map_index) else {
            continue; // nadie en este mapa — nada que materializar
        };
        for (index, se) in entries.iter().enumerate() {
            if !matches!(se.entry.kind, SpawnKind::Mob | SpawnKind::Anywhere) {
                continue; // TRAP grupos (parity entry_spawns — vnums de grupo)
            }
            let alive = alive_count.get(&(*map_index, index)).copied().unwrap_or(0);
            let near: Vec<u32> = viewers
                .iter()
                .filter(|(_, x, y)| {
                    distance_approx(se.entry.x - x, se.entry.y - y) <= SPAWN_VIEW
                })
                .map(|(pv, _, _)| *pv)
                .collect();
            if near.is_empty() {
                continue; // nadie cerca — tampoco se consume el respawn
            }
            // C23 (respawn por tiempo): la entrada con mobs muertos
            // pendientes NO se re-materializa hasta el deadline (parity
            // regen_event — el C++ respawnea cada `time` segundos; aquí el
            // delay empieza en la muerte). Al vencer: TOP-UP de las copias
            // que faltan hasta `max_count` (parity `regen_spawn`),
            // INDEPENDIENTE de que la entrada siga materializada con
            // hermanas vivas (F3: kill parcial de un entry count>1 — la
            // copia muerta reaparece en SU deadline aunque queden vivas).
            let spawn_count = match respawns.0.get(&(*map_index, index)).copied() {
                Some((due, _)) if due > now.0 => continue, // aún no toca
                Some((_, _)) => {
                    respawns.0.remove(&(*map_index, index));
                    se.entry.count.saturating_sub(alive)
                }
                None => {
                    if alive > 0 {
                        continue; // ya tiene copias — el ADD de nuevos
                                  // espectadores lo emite el pase B
                                  // (REGRESIÓN bench 2026-08-13)
                    }
                    se.entry.count
                }
            };
            if spawn_count == 0 {
                continue;
            }
            // Materializar las copias con vids FRESCOS del allocador. C28
            // (jitter del spawn — parity `SpawnMobRange`, char_manager.cpp:
            // 545-563: `number(sx, ex)` sobre el rect del regen; el rewrite
            // spawneaba TODAS en el centro -> copias apiladas): cada copia
            // cae en un punto ALEATORIO del rect `±(w_x, w_y)` celdas
            // (×100 a units). El ADD de cada copia lleva SU posición.
            let base = vids.npc;
            let mut copies: Vec<(SpawnEntry, u32)> = Vec::with_capacity(spawn_count as usize);
            for _ in 0..spawn_count {
                let vid = vids.npc;
                vids.npc += 1;
                let jx = if se.entry.w_x > 0 { rng.roll(-se.entry.w_x, se.entry.w_x) * 100 } else { 0 };
                let jy = if se.entry.w_y > 0 { rng.roll(-se.entry.w_y, se.entry.w_y) * 100 } else { 0 };
                let mut copy = se.entry; // Copy
                copy.x = se.entry.x + jx;
                copy.y = se.entry.y + jy;
                copy.count = 1;
                copies.push((copy, vid));
                let e = commands
                    .spawn((
                        Vid { vid },
                        Map { map_index: *map_index },
                        Position { x: copy.x, y: copy.y },
                        Hp { hp: se.mob.max_hp as i32, max_hp: se.mob.max_hp as i32 },
                        Aggro { target: None },
                        Mob::from_row(&copy, &se.mob),
                        SpawnRef { map: *map_index, index },
                    ))
                    .id();
                // Los espectadores del materialize YA recibieron el ADD de
                // esta copia — el pase B no la reemite (sin duplicados).
                commands.entity(e).insert(SpawnSeen(near.iter().copied().collect()));
                npc_index.0.insert(vid, e);
            }
            metrics.mobs_spawned += u64::from(spawn_count);
            // ADD(+INFO) por copia — parity byte-exacta del wire del entry
            // (cada copia con SU posición jittereada y su vid).
            let entries: Vec<(SpawnEntry, MobRow)> = copies
                .iter()
                .map(|(c, _)| (*c, se.mob.clone()))
                .collect();
            let packets = entry_spawns(*map_index, &entries, base);
            for pv in near {
                outbox.0.push(CombatEvent::Spawned { player_vid: pv, packets: packets.clone() }.into());
            }
        }
    }

    // --- ADD PER-JUGADOR de entradas YA materializadas (REGRESIÓN bench
    // 2026-08-13): cada entidad recuerda en `SpawnSeen` los jugadores que ya
    // recibieron su ADD — los que entran ahora (o vuelven a la vista) lo
    // reciben. El paquete lleva el vid EXACTO de la copia y su posición
    // ACTUAL (el ADD del C++ refleja la posición del mob, no su spawn). ---
    for (vid, map, pos, spawn_ref, mut seen) in &mut mobs {
        let Some(viewers) = players_by_map.get(&map.map_index) else {
            continue;
        };
        let in_view: HashSet<u32> = viewers
            .iter()
            .filter(|(_, x, y)| distance_approx(pos.x - x, pos.y - y) <= SPAWN_VIEW)
            .map(|(pv, _, _)| *pv)
            .collect();
        // Los que salieron de la vista: olvidar el envío (re-ADD al volver —
        // parity del re-entry al sectree del C++).
        seen.0.retain(|pv| in_view.contains(pv));
        let fresh: Vec<u32> = in_view.iter().filter(|pv| !seen.0.contains(pv)).copied().collect();
        if fresh.is_empty() {
            continue;
        }
        let Some(se) = table.maps.get(&map.map_index).and_then(|es| es.get(spawn_ref.index)) else {
            continue; // defensivo: entrada sin tabla — el despawn la limpia
        };
        let mut single = se.entry; // Copy (npc.rs:301 usa el mismo patrón)
        single.x = pos.x;
        single.y = pos.y;
        single.count = 1;
        let packets = entry_spawns(map.map_index, &[(single, se.mob.clone())], vid.vid);
        for pv in &fresh {
            seen.0.insert(*pv);
            outbox.0.push(CombatEvent::Spawned { player_vid: *pv, packets: packets.clone() }.into());
        }
    }

    // --- DESPAWN: intactos (hp lleno, sin aggro) a > DESPAWN_RADIUS de
    // TODOS los jugadores de su mapa (o sin jugadores en el mapa) ---
    for (e, vid, map, pos, hp, aggro, _spawn_ref) in &mobs_state {
        if hp.hp != hp.max_hp || aggro.target.is_some() {
            continue; // en combate: se queda (parity sectree + sin reset de HP)
        }
        let map_players = players_by_map.get(&map.map_index);
        let far = match map_players {
            None => true, // nadie en el mapa: limpieza
            Some(ps) => ps
                .iter()
                .all(|(_, x, y)| distance_approx(pos.x - x, pos.y - y) > DESPAWN_RADIUS),
        };
        if !far {
            continue;
        }
        // C23/F3: el despawn por DISTANCIA no es una muerte — la entrada
        // queda libre para re-materializar en cuanto un jugador se acerque.
        // El respawn PENDIENTE (copias muertas esperando su deadline) NO se
        // cancela (parity: la copia muerta del C++ respawnea en SU deadline
        // — `regen_spawn` top-up `max_count - count`; cancelarlo adelantaba
        // el respawn). Si toda la entrada quedó desmaterializada, vuelve
        // completa al vencer el deadline (el spawn hace el top-up).
        commands.entity(e).despawn();
        npc_index.0.remove(&vid.vid);
        metrics.mobs_despawned += 1;
        if let Some(ps) = map_players {
            for (pv, _, _) in ps {
                outbox.0.push(CombatEvent::Despawned { player_vid: *pv, vid: vid.vid }.into());
            }
        }
    }
}

impl WorldSim {
    /// Resuelve los spawns con la CACHÉ COMPARTIDA + UNA query batch por los
    /// vnums que falten (`MobCache::resolve`).
    pub(crate) async fn resolve_spawns(
        &self,
        repo: &MobRepo,
        spawns: &[SpawnEntry],
    ) -> Result<Vec<(SpawnEntry, MobRow)>, String> {
        let cache = self.world.resource::<SpawnCache>().0.clone();
        cache.lock().await.resolve(repo, spawns).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::events::{CombatEvent, CombatIntent, MoveIntent, NpcEvent};
    use crate::ecs::test_util::*;

    /// El spawn dinámico materializa SOLO las entradas a ≤ SPAWN_VIEW del
    /// jugador (el fix del mundo vacío: los mobs aparecen al acercarse).
    #[test]
    fn spawns_only_entries_within_view() {
        let mut w = world_with(42);
        let mut far = mob_row(2101);
        far.ai_flag = Some("NOMOVE".into()); // determinista: no patrulla hacia la vista
        load(&mut w, vec![
            (entry(101, 0, 0, 1), mob_row(101)),
            (entry(2101, 8_600, 0, 1), far), // fuera de SPAWN_VIEW 8000 (dist_approx 8264)
        ]);
        let events = join(&mut w);
        let spawned = spawn_events(&events);
        assert_eq!(spawned.len(), 1, "solo la entrada cercana: {events:?}");
        assert_eq!(w.npc_count(), 1);
        // El ADD del wire: header 1, vid 10000 (primer vid del allocador).
        let add = &spawned[0][0];
        assert_eq!(add[0], 1, "GC_CHARACTER_ADD");
        assert_eq!(&add[1..5], &10_000u32.to_le_bytes(), "vid del allocador");
    }

    /// El despawn: un mob intacto a > DESPAWN_RADIUS de TODOS los jugadores
    /// se desmaterializa (GC_CHARACTER_DEL al jugador).
    #[test]
    fn despawns_when_player_leaves_range() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("NOMOVE".into()); // nunca patrulla (determinista)
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        let events = join(&mut w);
        assert_eq!(spawn_events(&events).len(), 1);
        assert_eq!(w.npc_count(), 1);
        w.process_intent(MoveIntent::Move { player_vid: 2, x: 10_800, y: 0 }.into(), 1_000);
        let events = w.update(500);
        assert!(events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::Despawned { vid: 10_000, .. }))), "{events:?}");
        assert_eq!(w.npc_count(), 0);
        assert!(w.update(500).is_empty(), "desmaterializado: sin más eventos");
    }

    /// Los mobs EN COMBATE (hp < max o con aggro) NO se desmaterializan
    /// (parity sectree + sin reset de HP al kite).
    #[test]
    fn combat_mobs_stay_when_player_leaves() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("NOMOVE".into());
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        join(&mut w);
        // Un golpe del jugador: daño + aggro (OnDamage).
        w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            1_000,
        );
        w.process_intent(MoveIntent::Move { player_vid: 2, x: 10_800, y: 0 }.into(), 2_000);
        let events = w.update(500);
        assert!(!events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::Despawned { .. }))), "{events:?}");
        assert_eq!(w.npc_count(), 1, "dañado: se queda en el mundo");
    }

    /// Re-materialización al volver: la entrada vuelve con un vid NUEVO
    /// (el allocador global no reusa).
    #[test]
    fn respawns_with_new_vid_on_approach() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("NOMOVE".into());
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        join(&mut w);
        w.process_intent(MoveIntent::Move { player_vid: 2, x: 10_800, y: 0 }.into(), 1_000);
        w.update(500);
        assert_eq!(w.npc_count(), 0);
        w.process_intent(MoveIntent::Move { player_vid: 2, x: 0, y: 0 }.into(), 2_000);
        let events = w.update(500);
        let spawned = spawn_events(&events);
        assert_eq!(spawned.len(), 1, "re-materializa al volver");
        assert_eq!(&spawned[0][0][1..5], &10_001u32.to_le_bytes(), "vid fresco");
        assert_eq!(w.npc_count(), 1);
    }

    /// El count > 1 materializa TODAS las copias (vids consecutivos).
    #[test]
    fn spawns_all_copies_with_consecutive_vids() {
        let mut w = world_with(42);
        load(&mut w, vec![(entry(101, 0, 0, 3), mob_row(101))]);
        let events = join(&mut w);
        let spawned = spawn_events(&events);
        assert_eq!(spawned.len(), 1, "un evento con los 3 adds");
        assert_eq!(spawned[0].len(), 3, "3 copias → 3 paquetes");
        assert_eq!(&spawned[0][0][1..5], &10_000u32.to_le_bytes());
        assert_eq!(&spawned[0][1][1..5], &10_001u32.to_le_bytes());
        assert_eq!(&spawned[0][2][1..5], &10_002u32.to_le_bytes());
        assert_eq!(w.npc_count(), 3);
    }

    /// C23/F3 (kill parcial de un entry multi-copia): matar 1 copia de 3
    /// NO bloquea el respawn de la copia muerta mientras las hermanas
    /// viven — la entrada sigue materializada, pero al vencer el deadline
    /// el spawn hace el TOP-UP (parity `regen_spawn`: `num = max_count -
    /// count`, regen.cpp:325 — la copia muerta reaparece en SU deadline
    /// aunque queden 2 vivas).
    #[test]
    fn killed_copy_respawns_while_sisters_alive() {
        use crate::npc::SpawnEntry;
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.max_hp = 10; // un golpe (46+) mata
        row.ai_flag = Some("NOMOVE".into()); // las hermanas no se mueven (determinista)
        let e = SpawnEntry { time: 2, ..entry(101, 0, 0, 3) }; // 3 copias, intervalo 2 s
        load(&mut w, vec![(e, row)]);
        join(&mut w);
        assert_eq!(w.npc_count(), 3);
        // Matar UNA copia (la 10_000): quedan 2 hermanas vivas.
        w.process_intent(
            CombatIntent::Attack { player_vid: 2, victim_vid: 10_000, b_type: 0, weapon: None }.into(),
            1_000,
        );
        assert_eq!(w.npc_count(), 2, "2 hermanas vivas");
        // La copia muerta NO reaparece antes del deadline (t+0.5..1.5 s).
        for _ in 0..3 {
            let events = w.update(500);
            assert!(
                !events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::Spawned { .. }))),
                "aún pendiente: {events:?}"
            );
        }
        assert_eq!(w.npc_count(), 2);
        // t+2.0 s: vence el deadline → top-up: la copia muerta reaparece
        // con vid FRESCO aunque la entrada siga materializada (2 hermanas).
        let events = w.update(500);
        assert!(events.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::Spawned { .. }))), "respawn parcial: {events:?}");
        assert_eq!(w.npc_count(), 3, "las 3 copias de vuelta");
        assert!(w.npc_view(10_003).is_some(), "vid fresco 10_003 (la 10_000 murió)");
    }

    /// REGRESIÓN (bench 2026-08-13 — "solo el primer jugador veía los mobs"):
    /// el ADD del spawn es PER-JUGADOR — un jugador que entra DESPUÉS de que
    /// otro materializó las entradas recibe sus ADDs igualmente (parity: el
    /// C++ manda el ADD a cada vista, no "una vez por mundo"). Y sin
    /// duplicados para el mismo jugador en los ticks siguientes.
    #[test]
    fn spawn_adds_reach_every_player_in_view() {
        let mut w = world_with(42);
        load(&mut w, vec![
            (entry(101, 0, 0, 2), mob_row(101)),
            (entry(2101, 1_000, 0, 1), mob_row(2101)),
        ]);
        // Jugador 1: materializa (2 copias del 101 + 1 del 2101) y recibe
        // sus ADDs en el tick del join (un evento por entrada).
        let events1 = join_at(&mut w, 1, 0, 0);
        assert_eq!(spawn_events(&events1).len(), 2, "un evento por entrada: {events1:?}");
        assert_eq!(w.npc_count(), 3);
        // Jugador 2 en el MISMO punto: las entradas YA están materializadas
        // — `materialized` las salta, pero la emisión es por vista → recibe
        // los ADDs con los vids EXACTOS de las entidades existentes.
        let events2 = join_at(&mut w, 2, 0, 0);
        let vids: Vec<u32> = spawn_events(&events2)
            .iter()
            .flat_map(|pkts| pkts.iter().map(|p| u32::from_le_bytes(p[1..5].try_into().unwrap())))
            .collect();
        assert_eq!(vids, vec![10_000, 10_001, 10_002], "mismos vids que las entidades");
        // Sin re-emisión para los jugadores que YA recibieron el ADD.
        let events3 = w.update(500);
        assert!(
            !events3.iter().any(|e| matches!(e, NpcEvent::Combat(CombatEvent::Spawned { .. }))),
            "sin duplicados: {events3:?}"
        );
    }

    /// REGRESIÓN (alcance por jugador): la re-emisión del ADD es POR VISTA —
    /// un segundo jugador solo recibe las entradas DENTRO de su SPAWN_VIEW
    /// (las lejanas no se emiten aunque otro jugador las vea).
    #[test]
    fn spawn_adds_scoped_to_each_players_view() {
        let mut w = world_with(42);
        let mut far = mob_row(2101);
        far.ai_flag = Some("NOMOVE".into()); // determinista: no patrulla hacia el P2
        load(&mut w, vec![
            (entry(101, 0, 0, 1), mob_row(101)),
            (entry(2101, 6_900, 0, 1), far), // vista del P1 (6900 -> 6631 < 8000), fuera de la del P2 (8900 -> 8554 > 8000)
        ]);
        let events1 = join_at(&mut w, 1, 0, 0);
        assert_eq!(spawn_events(&events1).len(), 2, "P1 ve ambas entradas");
        assert_eq!(w.npc_count(), 2);
        // P2 a 2000 units del spawn del 101: el 2101 (8900 units) queda fuera
        // de su vista → solo recibe el ADD del 101.
        let events2 = join_at(&mut w, 2, -2_000, 0);
        let vids: Vec<u32> = spawn_events(&events2)
            .iter()
            .flat_map(|pkts| pkts.iter().map(|p| u32::from_le_bytes(p[1..5].try_into().unwrap())))
            .collect();
        assert_eq!(vids, vec![10_000], "solo la entrada dentro de su vista: {vids:?}");
    }

    /// TRAP grupos (parity entry_spawns): un entry con kind de grupo NO se
    /// materializa.
    #[test]
    fn group_kinds_never_materialize() {
        let mut w = world_with(42);
        load(&mut w, vec![(
            SpawnEntry { kind: SpawnKind::Group, ..entry(318, 0, 0, 1) },
            mob_row(318),
        )]);
        let events = join(&mut w);
        assert!(spawn_events(&events).is_empty());
        assert_eq!(w.npc_count(), 0);
    }

    /// Sin jugadores: el tick no hace nada (defensivo).
    #[test]
    fn update_without_players_is_a_noop() {
        let mut w = world_with(42);
        load(&mut w, vec![(entry(101, 0, 0, 1), mob_row(101))]);
        assert!(w.update(500).is_empty());
        assert_eq!(w.npc_count(), 0, "sin jugadores no materializa");
    }
}
