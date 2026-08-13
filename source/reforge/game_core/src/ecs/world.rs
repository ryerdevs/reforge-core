//! `WorldSim` — el mundo COMPARTIDO del canal (la tarea del canal lo posee):
//! la struct + la FACHADA (new/join/leave/process_intent/update/metrics).
//! Los impl blocks de DOMINIO viven en `ecs/systems/*` (C5) — los métodos
//! que la fachada y otros dominios llaman son `pub(crate)`.

use std::collections::HashMap;
use std::sync::Arc;

use bevy_ecs::prelude::*;

use crate::combat::CombatState;
use crate::ecs::components::{
    Affects, Combat, Hp, Map, Mp, Player, Position, SkillCooldowns, SkillLevels,
};
use crate::ecs::events::{
    CombatIntent, Intent, ItemIntent, MoveIntent, NpcEvent, PlayerJoin, SkillIntent,
};
use crate::ecs::resources::{
    ItemIndex, NpcIndex, NpcOutbox, Rand, SkillTable, SpawnCache, SpawnTable, SpawnTableEntry,
    Tick, VidAlloc, WorldMetrics,
};
use crate::ecs::systems::combat::{aggro_detect_system, chase_attack_system};
use crate::ecs::systems::movement::patrol_system;
use crate::ecs::systems::skill::affects_system;
use crate::ecs::systems::spawn::spawn_despawn_system;
use crate::npc::{load_map_spawns, MobCache, SpawnEntry};
use database::npc::{MobRepo, MobRow};

/// El mundo ECS del canal (ADR-0010 §1): el `World` de bevy + el `Schedule`
/// del tick + los índices vid→Entity. La tarea del canal lo posee; las
/// conexiones envían `Intent` por el mpsc y reciben `NpcEvent` por su cola.
///
/// API de la tarea del canal:
/// - `join_player` (async — carga la tabla de spawns del mapa + materializa
///   la vista inicial) / `process_intent` / `update(dt_ms)` → eventos.
/// - `npc_count` / `player_hp` / `metrics` — logs y hooks del harness F5.
///
/// NOTA (C5): `world` es `pub(crate)` — los tests de los submodulos
/// (`ecs/systems/*`) inyectan recursos directamente (p.ej. la `SkillTable`
/// de los tests de skills).
pub struct WorldSim {
    pub(crate) world: World,
    schedule: Schedule,
    pub(crate) players: HashMap<u32, Entity>,
}

impl WorldSim {
    /// Mundo con RNG sembrado del reloj (runtime). `spawn_cache` = la caché
    /// COMPARTIDA de mob_proto del canal (recurso `SpawnCache` del mundo).
    pub fn new(spawn_cache: Arc<tokio::sync::Mutex<MobCache>>) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self::with_seed(spawn_cache, seed)
    }

    /// Mundo con RNG DETERMINISTA (tests — `patrol_step`/`attack_damage`
    /// dependen del roll).
    pub fn with_seed(spawn_cache: Arc<tokio::sync::Mutex<MobCache>>, seed: u64) -> Self {
        let mut world = World::new();
        world.insert_resource(SpawnCache(spawn_cache));
        world.insert_resource(Tick { dt_ms: 500 });
        world.insert_resource(Rand::new(seed));
        world.insert_resource(NpcOutbox(Vec::new()));
        world.insert_resource(SpawnTable::default());
        world.insert_resource(VidAlloc::default());
        world.insert_resource(NpcIndex::default());
        world.insert_resource(ItemIndex::default());
        world.insert_resource(WorldMetrics::default());
        world.insert_resource(SkillTable::default());
        let mut schedule = Schedule::default();
        // Cadena: parity del ORDEN del tick del canal (spawn → chase → detect
        // → patrol → affects) y sin ambigüedad entre sistemas (comparten
        // recursos).
        schedule.add_systems((
            spawn_despawn_system,
            chase_attack_system,
            aggro_detect_system,
            patrol_system,
            affects_system,
        ).chain());
        Self { world, schedule, players: HashMap::new() }
    }

    /// Carga (una vez por mapa) la tabla COMPLETA de spawns: `load_map_spawns`
    /// (files del runtime) + resolución del `mob_proto` con la CACHÉ
    /// COMPARTIDA + UNA query batch por los vnums que falten (F5 perf — la
    /// resolución previa stallaba la entrada ~3-4 min). Errores → `Err` (el
    /// canal degrada a mundo sin spawns, como el entry previo).
    pub async fn join_player(
        &mut self,
        join: PlayerJoin,
        repo: &MobRepo,
        map_path: &str,
    ) -> Result<Vec<NpcEvent>, String> {
        if !self.world.resource::<SpawnTable>().maps.contains_key(&join.map_index) {
            let spawns = load_map_spawns(join.map_index, map_path)
                .map_err(|e| format!("spawns del mapa {}: {e}", join.map_index))?;
            let resolved = self.resolve_spawns(repo, &spawns).await.unwrap_or_default();
            let entries = resolved
                .into_iter()
                .map(|(entry, mob)| SpawnTableEntry { entry, mob })
                .collect();
            self.world.resource_mut::<SpawnTable>().maps.insert(join.map_index, entries);
        }
        Ok(self.join_player_ready(join))
    }

    /// Entra al jugador al mundo CON la tabla ya cargada (tests y el flujo
    /// interno): crea la entidad (Position/Hp/Player/Combat/Map) y corre el
    /// primer tick — el spawn dinámico materializa los mobs visibles desde su
    /// posición y devuelve los eventos (Spawned...) para su cola.
    pub fn join_player_ready(&mut self, join: PlayerJoin) -> Vec<NpcEvent> {
        let e = self
            .world
            .spawn((
                Map { map_index: join.map_index },
                Position { x: join.x, y: join.y },
                Hp { hp: join.hp, max_hp: join.max_hp },
                Player {
                    vid: join.vid,
                    level: join.level,
                    ht: join.ht,
                    armor: join.armor,
                    job: join.job,
                    st: join.st,
                    dx: join.dx,
                    iq: join.iq,
                },
                Combat(CombatState::new()),
                Mp { mp: join.mp, max_mp: join.max_mp },
                SkillLevels(join.skill_level),
                SkillCooldowns::default(),
                Affects::default(),
            ))
            .id();
        self.players.insert(join.vid, e);
        // Primer tick: materializa la vista inicial (mismos mobs que el
        // filtro SPAWN_VIEW del entry que este slice elimina).
        self.update(500)
    }

    /// Inyecta una tabla de spawns YA resuelta (tests y recargas futuras —
    /// el runtime la carga vía `join_player`).
    pub fn load_table(&mut self, map_index: u32, entries: Vec<(SpawnEntry, MobRow)>) {
        let entries = entries
            .into_iter()
            .map(|(entry, mob)| SpawnTableEntry { entry, mob })
            .collect();
        self.world.resource_mut::<SpawnTable>().maps.insert(map_index, entries);
    }

    /// Saca al jugador del mundo (disconnect — el RAII de la conexión manda
    /// `Intent::Leave`). Los mobs intactos de su mapa se limpian en el
    /// siguiente tick (el despawn sin jugadores).
    pub fn leave_player(&mut self, player_vid: u32) {
        if let Some(e) = self.players.remove(&player_vid)
            && self.world.get_entity(e).is_ok()
        {
            self.world.despawn(e);
        }
    }

    /// Procesa un intent de una conexión (la tarea del canal los drena del
    /// mpsc). `now_ms` = el reloj del server (el cooldown del combate).
    /// Devuelve los eventos S→C a enrutar (vacío para los intents sin
    /// respuesta).
    pub fn process_intent(&mut self, intent: Intent, now_ms: u64) -> Vec<NpcEvent> {
        self.world.resource_mut::<WorldMetrics>().intents_processed += 1;
        // Los brazos del wrapper (C3) son estables — cada dominio despacha a
        // su impl block (`ecs/systems/*`).
        let events = match intent {
            Intent::Join { .. } => Vec::new(), // la tarea del canal lo maneja aparte (async)
            Intent::Leave { player_vid } => {
                self.leave_player(player_vid);
                Vec::new()
            }
            Intent::Combat(c) => match c {
                CombatIntent::Attack { player_vid, victim_vid, b_type, weapon } => {
                    self.process_attack(player_vid, victim_vid, b_type, weapon.as_ref(), now_ms)
                }
                CombatIntent::SetHp { player_vid, hp } => {
                    self.set_player_hp(player_vid, hp);
                    Vec::new()
                }
                CombatIntent::SetMp { player_vid, mp } => {
                    self.set_player_mp(player_vid, mp);
                    Vec::new()
                }
                CombatIntent::SetArmor { player_vid, armor } => {
                    self.set_player_armor(player_vid, armor);
                    Vec::new()
                }
                CombatIntent::SetLevel { player_vid, level } => {
                    self.set_player_level(player_vid, level);
                    Vec::new()
                }
            },
            Intent::Move(MoveIntent::Move { player_vid, x, y }) => {
                self.set_player_position(player_vid, x, y);
                Vec::new()
            }
            Intent::Skill(SkillIntent::UseSkill { player_vid, skill_id, target_vid, weapon }) => {
                self.process_skill(player_vid, skill_id, target_vid, weapon.as_ref(), now_ms)
            }
            Intent::Item(item) => match item {
                ItemIntent::DropItem { player_vid, vnum, count, x, y, z } => {
                    self.process_drop(player_vid, vnum, count, x, y, z)
                }
                ItemIntent::PickupItem { player_vid, item_vid } => {
                    self.process_pickup(player_vid, item_vid)
                }
                ItemIntent::RemoveItem { item_vid } => {
                    self.remove_item(item_vid);
                    Vec::new()
                }
            },
            // Lanes futuros (C3 + N1): los delegados viven en sus archivos
            // (`ecs/systems/social.rs` / `quest.rs`) con `match s {}` — la
            // primera variante social/quest es un ERROR DE COMPILACIÓN en
            // su archivo, no un intent silenciosamente descartado aquí.
            Intent::Social(s) => self.handle_social(s, now_ms),
            Intent::Quest(q) => self.handle_quest(q, now_ms),
        };
        self.world.resource_mut::<WorldMetrics>().events_emitted += events.len() as u64;
        events
    }

    /// TICK del mundo (AI 500 ms — el `ai_timer` de la tarea del canal):
    /// corre los sistemas (spawn/despawn → chase → detect → patrulla) y
    /// devuelve los eventos S→C para el routing por jugador.
    pub fn update(&mut self, dt_ms: u64) -> Vec<NpcEvent> {
        self.world.resource_mut::<Tick>().dt_ms = dt_ms;
        self.schedule.run(&mut self.world);
        let events: Vec<NpcEvent> = self.world.resource_mut::<NpcOutbox>().0.drain(..).collect();
        let mut m = self.world.resource_mut::<WorldMetrics>();
        m.ticks += 1;
        m.events_emitted += events.len() as u64;
        events
    }

    /// Mobs materializados (el log del join y los hooks del harness).
    pub fn npc_count(&self) -> usize {
        self.world.resource::<NpcIndex>().0.len()
    }

    /// HP actual del jugador en el mundo (0 sin jugador — tests/harness).
    pub fn player_hp(&self, player_vid: u32) -> i32 {
        let Some(e) = self.players.get(&player_vid).copied() else { return 0 };
        self.world
            .get_entity(e)
            .ok()
            .and_then(|ent| ent.get::<Hp>())
            .map(|h| h.hp)
            .unwrap_or(0)
    }

    /// Métricas del mundo para el harness F5 (bench_bot/bench_capture).
    pub fn metrics(&self) -> WorldMetrics {
        *self.world.resource::<WorldMetrics>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::events::{CombatIntent, Intent};
    use crate::ecs::test_util::*;

    /// Leave: la entidad del jugador sale y los mobs intactos de su mapa se
    /// limpian en el siguiente tick (mapa sin jugadores).
    #[test]
    fn leave_cleans_player_and_mobs() {
        let mut w = world_with(42);
        load(&mut w, vec![(entry(101, 0, 0, 1), mob_row(101))]);
        join(&mut w);
        assert_eq!(w.npc_count(), 1);
        w.process_intent(Intent::Leave { player_vid: 2 }, 1_000);
        let events = w.update(500);
        assert!(events.is_empty(), "sin jugadores: sin eventos S→C");
        assert_eq!(w.npc_count(), 0, "limpieza del mapa sin jugadores");
        assert_eq!(w.player_hp(2), 0);
    }

    /// Las métricas del harness F5: ticks, intents, spawns/despawns.
    #[test]
    fn metrics_track_world_activity() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("NOMOVE".into());
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        assert_eq!(w.metrics(), WorldMetrics::default());
        join(&mut w); // 1 tick + Spawned
        w.process_intent(CombatIntent::SetHp { player_vid: 2, hp: 50 }.into(), 1_000);
        let m = w.metrics();
        assert_eq!(m.ticks, 1, "el join corre el primer tick");
        assert_eq!(m.intents_processed, 1);
        assert_eq!(m.mobs_spawned, 1);
        assert!(m.events_emitted >= 1, "el Spawned del join");
    }
}
