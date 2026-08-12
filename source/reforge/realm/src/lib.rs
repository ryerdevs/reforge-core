//! `realm` — lógica de juego por regiones (F4+): world entry, entidades, ECS.
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

pub mod ai;
pub mod movement;
pub mod packets;
pub mod world;
pub mod npc;
pub mod combat;
