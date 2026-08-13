//! Los sistemas del tick de AI (cadena — parity del ORDEN del canal) + los
//! impl blocks de DOMINIO del `WorldSim` (C5, 2026-08-13: split por IMPL
//! BLOCKS, NO sub-mundos — UN único `WorldSim` en `ecs/world.rs`; las
//! fachadas sobre el mismo `World` de bevy pelearían el borrow de `&mut
//! World`).
//!
//! Cada archivo es UN dominio: los métodos del `WorldSim` que la fachada
//! (`world.rs::process_intent`) y otros dominios llaman son `pub(crate)` —
//! consecuencia de dividir los impl blocks entre archivos. Los lanes futuros
//! ya tienen SU esqueleto aquí (`social.rs`, `quest.rs` — N1: el delegado
//! `match s {}` hace que la primera variante sea un error de compilación en
//! su archivo, no un intent descartado en silencio).

pub(crate) mod combat;
pub(crate) mod items;
pub(crate) mod movement;
pub(crate) mod quest;
pub(crate) mod skill;
pub(crate) mod social;
pub(crate) mod spawn;
