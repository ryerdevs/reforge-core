//! EVENTOS (slice event stub): stub mínimo del sistema de eventos del
//! Metin2 clásico (eventos temporales: Navidad, doble exp...). Pendiente
//! del lane completo: id persistente, programación, recompensas.

/// Evento: identificador, nombre y estado activo/inactivo.
pub struct Event {
    pub id: u64,
    pub name: String,
    pub active: bool,
}

/// Crea un evento inactivo con el nombre dado. `id` es un stub (0) —
/// la asignación real llegará con la persistencia (lane event, pendiente).
pub fn create_event(name: &str) -> Event {
    Event {
        id: 0,
        name: name.to_string(),
        active: false,
    }
}

/// Devuelve una copia del evento con el estado `active` aplicado.
pub fn set_active(event: Event, active: bool) -> Event {
    Event {
        id: event.id,
        name: event.name,
        active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER: crear evento → inactivo con el nombre; activarlo → activo.
    /// Mutación que falla: `set_active` que no aplica el flag, o
    /// `create_event` que nace activo.
    #[test]
    fn create_and_activate_roundtrip() {
        let ev = create_event("xmas");
        assert!(!ev.active, "evento recién creado → inactivo");
        assert_eq!(ev.name, "xmas");
        assert_eq!(ev.id, 0, "stub: id 0 hasta la persistencia");

        let ev = set_active(ev, true);
        assert!(ev.active, "set_active(true) → activo");
        assert_eq!(ev.name, "xmas", "activar no toca el nombre");

        let ev = set_active(ev, false);
        assert!(!ev.active, "set_active(false) → inactivo de nuevo");
    }
}