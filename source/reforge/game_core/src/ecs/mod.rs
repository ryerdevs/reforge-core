//! F5.3 (ADR-0010 §1-2): el mundo ECS COMPARTIDO del canal — `bevy_ecs`
//! standalone + intents por mpsc (patrón Veloren).
//!
//! El slice anterior dejó un `WorldSim` POR CONEXIÓN (los mobs solo existían
//! cerca del punto de entrada — el síntoma del mundo vacío al caminar). Este
//! slice lo reemplaza por el mundo ÚNICO del canal:
//!
//! - **Una instancia por canal** (`WorldSim` en la tarea del canal): las
//!   conexiones envían `Intent` por el mpsc y reciben los `NpcEvent` S→C por
//!   su cola (routing por `player_vid`).
//! - **SPAWN DINÁMICO** (`spawn_despawn_system`): la lista COMPLETA de spawns
//!   del mapa vive en el recurso `SpawnTable`; los mobs se MATERIALIZAN
//!   cuando un jugador está a ≤ `SPAWN_VIEW` (2500) de su punto de spawn y se
//!   DESMATERIALIZAN a > `DESPAWN_RADIUS` (4000 — margen de histéresis) de
//!   TODOS los jugadores. Los ADD(+INFO) los construye el módulo puro
//!   `game_core::npc::entry_spawns` (parity byte-exacta) — el cliente recibe los
//!   adds al ACERCARSE, no todos en el entry.
//! - **ADD PER-JUGADOR** (REGRESIÓN bench 2026-08-13): la EMISIÓN del ADD es
//!   por vista de jugador (parity sectree — cada vista recibe su ADD), no
//!   "una vez por mundo". El componente `SpawnSeen` de cada entidad recuerda
//!   los jugadores que ya recibieron su ADD — los que entran después (o
//!   vuelven a la vista) lo reciben en el tick siguiente.
//! - **VID allocation GLOBAL**: los contadores `next_npc_vid`/`next_item_vid`
//!   del canal pasan al recurso `VidAlloc` (los vids no colisionan entre
//!   conexiones — el slice por conexión los repetía).
//! - **Combate en el mundo**: el CG_ATTACK se resuelve con `handle_attack`
//!   (puro) dentro del mundo (el cooldown vive en el componente `Combat` del
//!   jugador); la conexión recibe `AttackResult` con los paquetes + el estado
//!   del objetivo para su flujo de kill/recompensa (PG-bound).
//!
//! Componentes: `Vid`, `Position`, `Hp`, `Aggro` (`Option<Entity>` — el
//! objetivo es multi-jugador), `Mob` (stats estáticas del mob_proto), `Item`
//! (item del suelo), `Player` (stats de combate + vid), `Combat` (cooldown),
//! `Map` (mapa de la entidad — las tablas y sistemas son por mapa), `SpawnRef`
//! (qué entrada de la tabla materializó la entidad).
//!
//! Sistemas (cadena — parity del orden del tick previo del canal):
//!   0. `spawn_despawn_system` — materializar/desmaterializar por distancia.
//!   1. `chase_attack_system` — aggro: de-aggro, ataque o persecución.
//!   2. `aggro_detect_system` — aggro proactivo (el jugador más cercano).
//!   3. `patrol_system` — patrullaje idle (1/7 por tick, radio del spawn).
//!
//! Las fórmulas de parity siguen en los módulos puros (`game_core::ai`,
//! `game_core::combat`, `game_core::npc`) — los sistemas solo orquestan. La emisión
//! wire sigue en channel.rs (los eventos llevan `player_vid` para el routing
//! por conexión).
//!
//! # Layout (refactor C5/C3, 2026-08-13)
//!
//! `ecs.rs` (2 694 líneas) se dividió en submodulos por dominio — UN único
//! `WorldSim` en `world.rs` (C5: impl blocks por dominio, NO sub-mundos — las
//! fachadas sobre el mismo `World` de bevy pelearían el borrow de `&mut
//! World`):
//!
//! - `components.rs` — los componentes bevy (estado del mundo).
//! - `resources.rs` — los recursos (Tick/Rand/NpcOutbox/SpawnTable/...).
//! - `events.rs` — `PlayerJoin`/`Intent`/`NpcEvent`/`KillInfo`/`ItemView`.
//! - `world.rs` — `WorldSim` + la fachada (new/join/leave/process_intent/
//!   update/metrics); los impl blocks de dominio viven en `systems/*`.
//! - `systems/` — los sistemas del tick + los impl blocks del dominio:
//!   `spawn.rs`, `combat.rs`, `movement.rs`, `skill.rs`, `items.rs`
//!   (futuro: `social.rs`, `quest.rs`).
//!
//! API estable hacia el canal (`server_realms` importa
//! `game_core::ecs::{Intent, NpcEvent, PlayerJoin, WorldSim}`): los submodulos
//! re-exportan lo que el módulo plano anterior exponía (`pub use *`).
//!
//! NOTA de visibilidad: los métodos de dominio del `WorldSim` que la fachada
//! (`world.rs`) y otros dominios llaman son `pub(crate)` — consecuencia
//! natural de dividir los impl blocks entre archivos (el módulo plano previo
//! los tenía privados).

pub mod components;
pub mod events;
pub mod resources;
pub mod systems;
pub mod world;

// API estable (el módulo plano anterior exponía todo a nivel de módulo).
pub use components::*;
pub use events::*;
pub use resources::*;
pub use world::*;
// Constantes del spawn dinámico (parity channel.rs — antes vivían en el
// módulo plano).
pub use systems::spawn::{DESPAWN_RADIUS, SPAWN_VIEW};

// Fixtures compartidos de los tests de los submodulos (DRY — la fila del
// mob 101, el jugador del harness, etc.).
#[cfg(test)]
mod test_util;
