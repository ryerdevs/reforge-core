//! DUNGEON: instancia de mazmorra de party — ciclo WAIT→START→END.
//! Parity `dungeon.cpp`: ids monotónicos (`Create` :477); una party dueña
//! (:417); `Join`/`IncMember`/`DecMember` (:146/:210/:218) — vacío = muerto
//! (:228-235); `SetDungeon_for_Only_party` (:417). El C++ no tiene enum de
//! estado; WAIT/START/END estructura su ciclo (create→warp→exit→destroy).
use std::sync::atomic::{AtomicU32, Ordering};

/// Estado del ciclo de vida (decisión del slice; el C++ lo lleva implícito).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DungeonState { Wait, Start, End }

/// Mazmorra: id único por proceso, mapa privado, party dueña, estado, miembros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dungeon {
    pub id: u32,
    pub map_index: i32,
    pub party_id: u32,
    pub state: DungeonState,
    pub members: u32,
}

/// Crea la instancia (parity `next_id_++` :477 — los ids mueren con el proceso).
pub fn create_dungeon(party_id: u32, map_index: i32) -> Dungeon {
    static NEXT_ID: AtomicU32 = AtomicU32::new(1);
    Dungeon {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        map_index,
        party_id,
        state: DungeonState::Wait,
        members: 0,
    }
}

/// Entra un miembro (parity `Join`+`IncMember` :146/:210): la 1ª entrada arranca; la terminada rechaza.
pub fn enter(d: &mut Dungeon) -> bool {
    if d.state == DungeonState::End {
        return false;
    }
    d.state = DungeonState::Start;
    d.members += 1;
    true
}

/// Sale un miembro (parity `DecMember` :218): el último cierra (`Start`→`End`).
pub fn exit(d: &mut Dungeon) {
    d.members = d.members.saturating_sub(1);
    if d.members == 0 && d.state == DungeonState::Start {
        d.state = DungeonState::End;
    }
}

/// ¿El jugador (por su party) está dentro? False sin party — entrada de party.
pub fn is_in_dungeon(player_party: Option<u32>, dungeon: &Dungeon) -> bool {
    player_party.is_some_and(|p| p == dungeon.party_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER: ids únicos, ciclo WAIT→START→END, rechazo final, acceso solo de la party dueña.
    #[test]
    fn verifier_dungeon_lifecycle() {
        let a = create_dungeon(7, 41);
        let b = create_dungeon(7, 41);
        assert_ne!(a.id, b.id, "ids únicos");
        let mut d = create_dungeon(7, 41);
        assert_eq!(d.state, DungeonState::Wait, "nace en espera");
        assert!(enter(&mut d), "1ª entrada arranca");
        assert_eq!(d.state, DungeonState::Start, "arrancada");
        assert!(enter(&mut d), "más miembros entran");
        exit(&mut d);
        assert_eq!(d.state, DungeonState::Start, "queda 1 miembro");
        exit(&mut d);
        assert_eq!(d.state, DungeonState::End, "último sale → fin");
        assert!(!enter(&mut d), "terminada rechaza entradas");
        assert!(is_in_dungeon(Some(7), &d) && !is_in_dungeon(Some(8), &d) && !is_in_dungeon(None, &d), "solo la party dueña");
    }
}