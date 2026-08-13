//! Lane futuro QUEST (C3 + N1, 2026-08-13): el sub-enum `QuestIntent`
//! nace VACÍO — el `match q {}` del delegado convierte la PRIMERA variante
//! quest en un ERROR DE COMPILACIÓN aquí (en vez de un intent
//! silenciosamente descartado en `process_intent`). El DSL de quests
//! (`quest_dsl`) crece en este archivo.

use crate::ecs::events::{NpcEvent, QuestIntent};
use crate::ecs::world::WorldSim;

impl WorldSim {
    /// Delegado N1 de `Intent::Quest` (trampa: `match q {}` — ver el
    /// módulo). `now_ms` = el reloj del server (los timers de quests).
    pub(crate) fn handle_quest(&mut self, q: QuestIntent, _now_ms: u64) -> Vec<NpcEvent> {
        match q {}
    }
}
