//! Fixtures COMPARTIDOS de los tests de los submodulos `ecs/*` (DRY — los 6
//! modulos de test usan la fila del mob 101, el jugador del harness, etc.).
//! Solo se compila en `cfg(test)` (declarado `#[cfg(test)] mod test_util` en
//! `ecs/mod.rs`).

use std::sync::Arc;

use crate::ecs::events::{CombatEvent, NpcEvent, PlayerJoin};
use crate::ecs::world::WorldSim;
use crate::npc::{MobCache, SpawnEntry, SpawnKind};
use database::npc::MobRow;

pub fn cache() -> Arc<tokio::sync::Mutex<MobCache>> {
    Arc::new(tokio::sync::Mutex::new(MobCache::new()))
}

/// Fila del mob 101 real (mob_proto — mismos valores que los fixtures del
/// canal): lvl 1, dx/ht 5, DEF 4, MELEE rango 175, daño 3..8, exp 22,
/// gold 15..45, drop 101, move_speed 100, sight 400.
pub fn mob_row(vnum: i64) -> MobRow {
    MobRow {
        vnum,
        name: "test".into(),
        locale_name: b"test".to_vec(),
        b_type: 0,
        battle_type: 0,
        level: 1,
        size: "SMALL".into(),
        ai_flag: None,
        folder: String::new(),
        ht: 5,
        def: 4,
        max_hp: 126,
        attack_range: 175,
        exp: 22,
        gold_min: 15,
        gold_max: 45,
        drop_item: 101,
        move_speed: 100,
        damage_min: 3,
        damage_max: 8,
        aggressive_sight: 400,
    }
}

pub fn entry(vnum: u32, x: i32, y: i32, count: u32) -> SpawnEntry {
    SpawnEntry { vnum, x, y, count, kind: SpawnKind::Mob }
}

pub fn world_with(seed: u64) -> WorldSim {
    WorldSim::with_seed(cache(), seed)
}

/// Jugador del harness (parity `dummy_row` del canal — ninja lvl 5,
/// ht 30, ASSASSIN st/dx/iq 30) en el mapa 41.
pub fn join_at(w: &mut WorldSim, vid: u32, x: i32, y: i32) -> Vec<NpcEvent> {
    w.join_player_ready(PlayerJoin {
        vid,
        map_index: 41,
        x,
        y,
        hp: 100,
        max_hp: 100,
        mp: 100,
        max_mp: 100,
        skill_level: Vec::new(),
        level: 5,
        ht: 30,
        armor: 0,
        job: 1,
        st: 30,
        dx: 30,
        iq: 30,
    })
}

/// Join con el blob de skills: `grant` = (skill_id, nivel) — el blob
/// `255 × 6 B` con el nivel en `id*6+1` (parity del layout del player).
pub fn join_with_skills(w: &mut WorldSim, vid: u32, grant: &[(u32, u8)]) -> Vec<NpcEvent> {
    let mut blob = vec![0u8; 255 * 6];
    for (id, lv) in grant {
        let off = (*id as usize) * 6 + 1;
        if off < blob.len() {
            blob[off] = *lv;
        }
    }
    w.join_player_ready(PlayerJoin {
        vid,
        map_index: 41,
        x: 0,
        y: 0,
        hp: 100,
        max_hp: 100,
        mp: 100,
        max_mp: 100,
        skill_level: blob,
        level: 5,
        ht: 30,
        armor: 0,
        job: 1,
        st: 30,
        dx: 30,
        iq: 30,
    })
}

pub fn join(w: &mut WorldSim) -> Vec<NpcEvent> {
    join_at(w, 2, 0, 0)
}

pub fn load(w: &mut WorldSim, entries: Vec<(SpawnEntry, MobRow)>) {
    w.load_table(41, entries);
}

pub fn spawn_events(events: &[NpcEvent]) -> Vec<&Vec<Vec<u8>>> {
    events
        .iter()
        .filter_map(|e| match e {
            NpcEvent::Combat(CombatEvent::Spawned { packets, .. }) => Some(packets),
            _ => None,
        })
        .collect()
}
