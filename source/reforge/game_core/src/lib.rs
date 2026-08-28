//! `game_core` — lógica de juego por regiones (F4+): world entry, entidades, ECS.
//!
//! Primer slice de F4 (ROADMAP): la composición `WorldStore` (repos + pipeline
//! durable) y el mapeo de paquetes del flujo select/spawn con parity C++
//! (`desc.cpp`, `char.cpp`, `ClientManagerLogin.cpp`).
//!
//! - `world` — `WorldStore`: acceso de escritura/lectura al dominio world con
//!   el Batcher durable (ADR-0008) ya conectado.
//! - `packets` — mapeos PlayerRow/PlayerSummary -> paquetes del wire
//!   (TSimplePlayer 71 B, TPacketGCLoginSuccess 449 B, TPacketGCCharacterAdd
//!   37 B, TPacketGCCharacterAdditionalInfo 70 B). Los structs wire viven en
//!   `protocol` (F0) — aqui SOLO las transformaciones de dominio.
//! - `movement` — F5.1: el estado de movimiento del jugador y la validación
//!   anti-speedhack del CG_MOVE (parity input_main.cpp).
//! - `combat` — F5.2: el core del combate (server-authoritative) — la fórmula
//!   base del C++ (`battle.cpp`/`char.cpp`), cooldown y rango; expuesto como
//!   `handle_attack(...)` para el dispatch del canal (CG_ATTACK).
//! - `ai` — F5.3: la AI mínima de los mobs (paso de movimiento hacia el
//!   jugador + rotación) — funciones puras que el canal usa en su tick.
//! - `belt` — slice stub: la cinta de pociones equipable (items como lista
//!   de vnums) — parity belt_inventory_helper.h.
//! - `ecs` — F5.3 (ADR-0010): el mundo ECS (bevy_ecs standalone) — los
//!   componentes del mundo (Position/Hp/Aggro/Mob/Item), los sistemas del
//!   tick de AI (chase/attack → aggro proactivo → patrulla) y `WorldSim`,
//!   el wrapper del canal sobre el `World` de bevy.
//! - `map` — F5.4 (ADR-0011): walkability server-side — port de
//!   `IsMovablePosition` (`sectree_manager.cpp`): grid de atributos por mapa
//!   desde los archivos del server (`server_attr` LZO1X + `Setting.txt`),
//!   caché por mapa y `is_movable(map, x, y)` en units (celdas de 50 u).
//! - `guild` — slice social: ciclo de vida básico de una guild (create/add/
//!   remove) en dominio puro, sin I/O — la persistencia vive en `database`.
//! - `horse` — slice horse: nivel y salud del caballo (create/feed) en
//!   dominio puro — parity horse_rider.{h,cpp} (HORSE_MAX_LEVEL, iMaxHealth).
//! - `dungeon` — slice stub: identidad de las mazmorras de instancia (id
//!   único por proceso, mapa privado, party dueña) — parity dungeon.cpp.
//! - `land` — slice stub: identidad de los terrenos construibles (id, dueño,
//!   precio) — parity common/building.h (TLand).

pub mod ai;
pub mod belt;
pub mod combat;
pub mod dungeon;
pub mod ecs;
pub mod event;
pub mod gm;
pub mod guild;
pub mod horse;
pub mod land;
pub mod map;
pub mod movement;
pub mod npc;
pub mod packets;
pub mod quest;
pub mod shop;
pub mod skill;
pub mod trade;
pub mod weight;
pub mod world;
