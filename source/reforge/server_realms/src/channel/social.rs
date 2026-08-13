//! `channel/social.rs` — emisión S→C del dominio SOCIAL (C3 + N1,
//! 2026-08-13): el sub-enum `SocialEvent` nace VACÍO — el `match s {}` del
//! emisor convierte la PRIMERA variante social en un ERROR DE COMPILACIÓN
//! aquí (en vez de un evento silenciosamente descartado en
//! `events::handle`). Guild/party crecen en este archivo (sus paquetes GC
//! y las mutaciones de la sesión).

use game_core::ecs::SocialEvent;

use crate::channel::session::Session;

/// Delegado N1 de `NpcEvent::Social` (trampa: `match s {}` — ver el
/// módulo). El routing por jugador ya ocurrió en la tarea del canal.
pub(super) fn emit(_session: &mut Session, s: SocialEvent) {
    match s {}
}
