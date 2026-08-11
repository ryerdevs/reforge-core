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

pub mod packets;
pub mod world;
