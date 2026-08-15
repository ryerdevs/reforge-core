//! Recursos del mundo ECS (los datos globales del canal que los sistemas y
//! los métodos del `WorldSim` comparten).

use std::collections::HashMap;
use std::sync::Arc;

use bevy_ecs::prelude::*;

use crate::ecs::events::NpcEvent;
use crate::npc::{MobCache, SpawnEntry};
use database::npc::MobRow;

/// Reloj del tick actual (dt en ms — el paso del AI: `move_speed × dt/1000`).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    pub dt_ms: u64,
}

/// Reloj ACUMULADO del mundo en ms (C23 — el respawn por tiempo): `update`
/// lo avanza con el dt de cada tick; el `RespawnQueue` guarda los deadlines
/// en ESTA escala (monótona y determinista en los tests).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorldClock(pub u64);

/// Cola de RESPAWN por TIEMPO (C23 — parity `regen_event`, regen.cpp:582-600:
/// el C++ re-spawnea la entrada cada `regen->time` segundos): `(map, index)`
/// de la entrada → (deadline en ms del reloj del mundo, copias a spaw near).
/// Lo llena `remove_npc` cuando un mob MUERE (intervalo del regen.txt o
/// default 60 s) y lo consume el `spawn_despawn_system` (la entrada no se
/// re-materializa hasta el deadline). `copies` = número de copias muertas
/// pendientes (count>1: se acumulan — se spaw nean juntas al vencer).
#[derive(Resource, Debug, Default)]
pub struct RespawnQueue(pub std::collections::HashMap<(u32, usize), (u64, u32)>);

/// RNG del mundo (xorshift64* — determinista con seed; los tests la fijan).
/// `roll(min, max)` = el `number()` INCLUSIVE del C++ (mismo patrón que el
/// `rand32() % span` del canal).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rand {
    state: u64,
}

impl Rand {
    pub fn new(seed: u64) -> Self {
        Self { state: seed | 1 } // state 0 degeneraría el xorshift
    }

    pub fn roll(&mut self, min: i32, max: i32) -> i32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        let span = i64::from(max) - i64::from(min) + 1;
        if span <= 1 {
            return min; // rango degenerado (parity `(hi - lo + 1).max(1)`)
        }
        min + (x % span as u64) as i32
    }
}

/// Eventos S→C del tick (el canal los drena tras `update` y los traduce a
/// paquetes GC — la emisión wire NO vive en los sistemas).
#[derive(Resource, Debug, Default)]
pub struct NpcOutbox(pub Vec<NpcEvent>);

/// La caché COMPARTIDA de `mob_proto` entre conexiones (F5 perf — la
/// resolución de spawns hace UNA query batch por los vnums que falten). El
/// canal la crea en `run()` y el mundo la recibe como recurso.
#[derive(Resource, Clone)]
pub struct SpawnCache(pub Arc<tokio::sync::Mutex<MobCache>>);

/// Contadores GLOBALES de VIDs del canal (parity de los statics del canal:
/// NPCs desde 10 000, items del suelo desde 50 000). El mundo los asigna —
/// los vids no colisionan entre conexiones (el slice por conexión los
/// repetía).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct VidAlloc {
    pub npc: u32,
    pub item: u32,
}

impl Default for VidAlloc {
    fn default() -> Self {
        Self { npc: 10_000, item: 50_000 }
    }
}

/// La lista COMPLETA de spawns del mapa (la que el C++ carga en el sectree al
/// boot): el `spawn_despawn_system` materializa/desmaterializa desde aquí —
/// el entry ya NO pre-filtra por visión (el filtro estático de channel.rs se
/// eliminó en este slice).
#[derive(Resource, Debug, Default)]
pub struct SpawnTable {
    /// `map_index` → entradas resueltas (kind Mob/Anywhere — la expansión de
    /// grupos ya ocurrió en `load_map_spawns`).
    pub maps: HashMap<u32, Vec<SpawnTableEntry>>,
}

/// Una entrada resuelta de la tabla: el entry (posición/count) + su fila del
/// `mob_proto` (stats para el componente `Mob` y los paquetes ADD).
#[derive(Debug, Clone)]
pub struct SpawnTableEntry {
    pub entry: SpawnEntry,
    pub mob: MobRow,
}

/// Índice vid→Entity de los MOBS materializados (lo comparten los sistemas
/// — `spawn_despawn` lo muta — y los métodos del `WorldSim`).
#[derive(Resource, Debug, Default)]
pub struct NpcIndex(pub HashMap<u32, Entity>);

/// Índice vid→Entity de los ITEMS del suelo.
#[derive(Resource, Debug, Default)]
pub struct ItemIndex(pub HashMap<u32, Entity>);

/// Contadores del mundo para el harness F5 (bench_bot / bench_capture): el
/// lane del benchmark los lee con `WorldSim::metrics()`.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorldMetrics {
    /// Ticks de AI ejecutados (`update`).
    pub ticks: u64,
    /// Intents procesados (`process_intent`).
    pub intents_processed: u64,
    /// Entidades de mob materializadas por el spawn dinámico.
    pub mobs_spawned: u64,
    /// Entidades de mob desmaterializadas (fuera de rango de todos).
    pub mobs_despawned: u64,
    /// Eventos S→C emitidos (routing del canal).
    pub events_emitted: u64,
    /// Tiempo del ÚLTIMO tick (ms — `schedule.run` de `update`): el timing de
    /// sistemas que el run de bench registra por tick en `--bench-capture`.
    pub last_tick_ms: u64,
}

/// El proto de las skills (del `player.skill_proto` — cargado UNA vez con
/// `WorldSim::load_skills`; estático en el runtime). El cooldown/efecto de
/// cada skill se resuelve con estos datos (game_core::skill).
#[derive(Resource, Debug, Default)]
pub struct SkillTable(pub std::collections::HashMap<u32, crate::skill::SkillProto>);
