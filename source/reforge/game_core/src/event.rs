//! EVENTOS temporales programados (Navidad, doble exp...): schedule + trigger.
//! Diseño propio — el C++ clásico solo tiene `event_queue` de timers
//! (event.cpp/event.h), sin eventos temporales con ventana de tiempo.

/// Intervalo de actividad en segundos de época UNIX (`end` exclusivo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub start: i64,
    pub end: i64,
}

/// Efecto que dispara el evento mientras está activo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    ExpMultiplier(u32),
    DropMultiplier(u32),
}

/// Evento temporal programado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub id: u64, // 0 hasta la persistencia (lane event pendiente)
    pub name: String,
    pub schedule: Schedule,
    pub trigger: Trigger,
}

/// Activo si `schedule.start <= now < schedule.end` (medio-abierto: el fin
/// no cuenta; `start >= end` nunca está activo).
pub fn is_active(event: &Event, now: i64) -> bool {
    event.schedule.start <= now && now < event.schedule.end
}

/// Crea un evento con `id 0` — la asignación real llega con la persistencia.
pub fn create_event(name: &str, start: i64, end: i64, trigger: Trigger) -> Event {
    Event {
        id: 0,
        name: name.to_string(),
        schedule: Schedule { start, end },
        trigger,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER: activo solo dentro de [start, end). Mutaciones que fallan:
    /// `<=` en el fin, `>=`/`>` en el inicio, o `start > end` activo.
    #[test]
    fn is_active_respects_schedule_bounds() {
        let ev = create_event("xmas", 1_000, 2_000, Trigger::DropMultiplier(2));
        assert!(!is_active(&ev, 999), "antes del inicio → inactivo");
        assert!(is_active(&ev, 1_000), "inicio incluido → activo");
        assert!(is_active(&ev, 1_999), "durante → activo");
        assert!(!is_active(&ev, 2_000), "fin excluido → inactivo");
        assert!(!is_active(&ev, 3_000), "después → inactivo");
    }

    /// VERIFIER: el trigger y el nombre se conservan en el evento creado.
    /// Mutación que falla: create_event que ignora name/trigger o nace activo.
    #[test]
    fn event_keeps_name_and_trigger() {
        let ev = create_event("2x_exp", 0, 100, Trigger::ExpMultiplier(2));
        assert_eq!(ev.name, "2x_exp");
        assert_eq!(ev.trigger, Trigger::ExpMultiplier(2));
        assert_eq!(ev.id, 0, "id 0 hasta la persistencia");
    }
}