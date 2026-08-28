//! CG_DUNGEON (110) CREATE — ADITIVO reforge (solo quest-lua en el C++,
//! dungeon.cpp:466; patrón CG_PVP). 5 B; SIN persistencia PG (parity :477).

use crate::channel::session::{Outcome, Session};

pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let Some(map_index) = map_of(pkt) else {
        eprintln!(
            "server_realms: channel conn {}: CG_DUNGEON malformado ({} B)",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    };
    let d = game_core::dungeon::create_dungeon(session.party_id.unwrap_or(0), map_index);
    eprintln!(
        "server_realms: channel conn {}: mazmorra {d:?}",
        session.conn_id
    );
    Ok(Outcome::Continue)
}

fn map_of(pkt: &[u8]) -> Option<i32> {
    (pkt.len() == 5).then(|| i32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER wire+dominio: tamaño/offset/endian mutados fallan; la
    /// instancia nace ligada a la party con id de proceso.
    #[test]
    fn dungeon_create_wire_and_binding() {
        assert_eq!(map_of(&[110, 41, 0, 0, 0]), Some(41));
        assert_eq!(map_of(&[110, 0xA8, 0x61, 0, 0]), Some(0x61A8), "LE");
        assert_eq!(map_of(&[110, 41, 0, 0]), None, "4 B");
        assert_eq!(map_of(&[110, 41, 0, 0, 0, 0]), None, "6 B");
        let d = game_core::dungeon::create_dungeon(7, 41);
        assert_eq!((d.party_id, d.map_index), (7, 41));
        assert!(d.id > 0);
    }
}
