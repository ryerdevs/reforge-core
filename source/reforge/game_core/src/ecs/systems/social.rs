//! Lane futuro SOCIAL (C3 + N1, 2026-08-13): el sub-enum `SocialIntent`
//! nace VACÍO — el `match s {}` del delegado convierte la PRIMERA variante
//! social en un ERROR DE COMPILACIÓN aquí (en vez de un intent
//! silenciosamente descartado en `process_intent`). Guild/party crecen en
//! este archivo (su impl block + su sistema en la cadena de `world.rs`).

use crate::ecs::events::{NpcEvent, SocialIntent};
use crate::ecs::world::WorldSim;

impl WorldSim {
    /// Delegado N1 de `Intent::Social` (trampa: `match s {}` — ver el
    /// módulo). `now_ms` = el reloj del server (los cooldowns sociales).
    pub(crate) fn handle_social(&mut self, s: SocialIntent, _now_ms: u64) -> Vec<NpcEvent> {
        match s {}
    }
}
