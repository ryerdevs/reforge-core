//! `channel/quest.rs` — emisión S→C del dominio QUEST (C3 + N1,
//! 2026-08-13): el sub-enum `QuestEvent` nace VACÍO — el `match q {}` del
//! emisor convierte la PRIMERA variante quest en un ERROR DE COMPILACIÓN
//! aquí (en vez de un evento silenciosamente descartado en
//! `events::handle`). Los flujos de quests (locales, recompensas) crecen
//! en este archivo.

use game_core::ecs::QuestEvent;

use crate::channel::session::Session;

/// Delegado N1 de `NpcEvent::Quest` (trampa: `match q {}` — ver el
/// módulo). El routing por jugador ya ocurrió en la tarea del canal.
pub(super) fn emit(_session: &mut Session, q: QuestEvent) {
    match q {}
}
