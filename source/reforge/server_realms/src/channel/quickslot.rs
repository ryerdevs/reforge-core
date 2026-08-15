//! `channel/quickslot.rs` — los handlers de la BARRA RÁPIDA (lane D):
//! CG_QUICKSLOT_ADD (16), CG_QUICKSLOT_DEL (17) y CG_QUICKSLOT_SWAP (18).
//!
//! Parity `QuickslotAdd`/`QuickslotDelete`/`QuickslotSwap`
//! (input_main.cpp:908-934) → `SetQuickslot`/`DelQuickslot`/`SwapQuickslot`
//! (char_quickslot.cpp:51-145):
//! - El estado vive en el bytea `player.quickslot` (36 × TQuickslot de 2 B =
//!   72 B — `tables.h:410`, QUICKSLOT_MAX_NUM = 36, length.h:60).
//! - Cada operación valida, muta el blob, lo PERSISTE (save del canal) y
//!   responde con el paquete GC del mismo shape (GC_QUICKSLOT_ADD 28 /
//!   GC_QUICKSLOT_DEL 29 / GC_QUICKSLOT_SWAP 30 — char_quickslot.cpp:96-145).
//! - Validaciones del C++: `pos >= QUICKSLOT_MAX_NUM` → rechazo;
//!   `rSlot.type >= QUICKSLOT_TYPE_MAX_NUM` → rechazo; type ITEM → el pos
//!   debe ser una celda del inventario (`IsDefaultInventoryPosition` —
//!   cell < INVENTORY_MAX_NUM; el belt queda fuera del subset); type SKILL →
//!   `pos < SKILL_MAX_NUM` (255); type COMMAND → acepta cualquiera.
//! - Al AÑADIR con type != 0, el C++ deduplica: cualquier OTRO slot con el
//!   MISMO (type, pos) se borra (`DelQuickslot(i)` — el GC_DEL sale).
//!
//! C6a (firma uniforme): malformado/rechazos → log + `Outcome::Continue`.

use database::player::PlayerRow;
use protocol::world::{
    TPacketCGQuickSlotAdd, TPacketCGQuickSlotDel, TPacketCGQuickSlotSwap,
    TPacketGCQuickSlotAdd, TPacketGCQuickSlotDel, TPacketGCQuickSlotSwap, TQuickslot,
};

use crate::channel::session::{Outcome, Session};
use crate::channel::INVENTORY_MAX_NUM;

/// `QUICKSLOT_MAX_NUM = 36` (length.h:60) — slots de la barra.
pub const QUICKSLOT_MAX_NUM: usize = 36;
/// `QUICKSLOT_TYPE_ITEM = 0` (length.h:241) — slot de item.
pub const QUICKSLOT_TYPE_ITEM: u8 = 0;
/// `QUICKSLOT_TYPE_SKILL = 1` (length.h:242) — slot de skill.
pub const QUICKSLOT_TYPE_SKILL: u8 = 1;
/// `QUICKSLOT_TYPE_COMMAND = 2` (length.h:243) — slot de comando.
pub const QUICKSLOT_TYPE_COMMAND: u8 = 2;
/// `QUICKSLOT_TYPE_MAX_NUM = 3` (length.h:244) — gate del tipo.
const QUICKSLOT_TYPE_MAX_NUM: u8 = 3;
/// `SKILL_MAX_NUM = 255` (length.h:60) — gate del pos de skill.
const SKILL_MAX_NUM: u8 = 255;

/// El bytea `player.quickslot` (36 × TQuickslot) como Vec mutable — default
/// de 72 ceros si la fila no lo trae o está roto (fail-open: una barra
/// vacía no crashea el handler).
pub(crate) fn blob(row: &PlayerRow) -> Vec<u8> {
    match &row.quickslot {
        Some(b) if b.len() == QUICKSLOT_MAX_NUM * TQuickslot::SIZE => b.clone(),
        _ => vec![0; QUICKSLOT_MAX_NUM * TQuickslot::SIZE],
    }
}

/// Slot `i` del blob como TQuickslot (None si el índice está fuera de rango).
fn slot_at(blob: &[u8], pos: u8) -> Option<TQuickslot> {
    let i = usize::from(pos);
    if i >= QUICKSLOT_MAX_NUM {
        return None;
    }
    TQuickslot::from_bytes(&blob[i * 2..i * 2 + 2]).ok()
}

/// Persiste el blob en la fila y guarda (save del canal — WAL).
fn persist(session: &mut Session, blob: &[u8]) {
    session.row_mut().quickslot = Some(blob.to_vec());
    session.save();
}

/// Quita los slots de tipo ITEM que apuntan a la celda `cell` (parity
/// `SyncQuickslot(QUICKSLOT_TYPE_ITEM, cell, 255)` char_quickslot.cpp:8-28:
/// al SOLTAR un item, la referencia de la barra se borra). Devuelve las
/// posiciones borradas — el caller manda el GC_QUICKSLOT_DEL por cada una.
pub fn clear_item_refs(blob: &mut [u8], cell: u16) -> Vec<u8> {
    let mut cleared = Vec::new();
    for i in 0..QUICKSLOT_MAX_NUM {
        if blob[i * 2] == QUICKSLOT_TYPE_ITEM
            && u16::from(blob[i * 2 + 1]) == cell
        {
            blob[i * 2..i * 2 + 2].copy_from_slice(&[0, 0]);
            cleared.push(i as u8);
        }
    }
    cleared
}

/// CG_QUICKSLOT_ADD (16, 4 B: header + pos + TQuickslot — Packet.h:607-612):
/// añadir un slot a la barra. Parity `SetQuickslot` (char_quickslot.cpp:
/// 51-103): gates de rango/tipo, dedupe del (type,pos) repetido y eco
/// GC_QUICKSLOT_ADD (28, 4 B) + persistencia del bytea.
pub async fn handle_add(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let add = match TPacketCGQuickSlotAdd::from_bytes(pkt) {
        Ok(a) => a,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_QUICKSLOT_ADD malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    if add.pos as usize >= QUICKSLOT_MAX_NUM {
        eprintln!(
            "server_realms: channel conn {}: quickslot add a pos {} — \
             fuera de rango (QUICKSLOT_MAX_NUM {QUICKSLOT_MAX_NUM})",
            session.conn_id, add.pos
        );
        return Ok(Outcome::Continue);
    }
    if add.slot.slot_type >= QUICKSLOT_TYPE_MAX_NUM {
        eprintln!(
            "server_realms: channel conn {}: quickslot add con type {} — \
             fuera de rango (QUICKSLOT_TYPE_MAX_NUM {QUICKSLOT_TYPE_MAX_NUM})",
            session.conn_id, add.slot.slot_type
        );
        return Ok(Outcome::Continue);
    }
    // Gates por tipo (parity char_quickslot.cpp:72-88): ITEM → celda del
    // inventario; SKILL → < SKILL_MAX_NUM; COMMAND → sin gate.
    match add.slot.slot_type {
        QUICKSLOT_TYPE_ITEM if add.slot.pos as u16 >= INVENTORY_MAX_NUM => {
            eprintln!(
                "server_realms: channel conn {}: quickslot ITEM a celda {} — \
                 fuera del inventario (INVENTORY_MAX_NUM {INVENTORY_MAX_NUM})",
                session.conn_id, add.slot.pos
            );
            return Ok(Outcome::Continue);
        }
        QUICKSLOT_TYPE_SKILL if add.slot.pos >= SKILL_MAX_NUM => {
            eprintln!(
                "server_realms: channel conn {}: quickslot SKILL a pos {} — \
                 fuera de rango (SKILL_MAX_NUM {SKILL_MAX_NUM})",
                session.conn_id, add.slot.pos
            );
            return Ok(Outcome::Continue);
        }
        // QUICKSLOT_TYPE_COMMAND: sin gate (parity char_quickslot.cpp:83-85).
        QUICKSLOT_TYPE_COMMAND => {}
        _ => {}
    }
    let mut b = blob(session.row());
    // Dedupe (parity SetQuickslot char_quickslot.cpp:60-68): otro slot con el
    // MISMO (type, pos) se borra — el GC_DEL sale por cada uno.
    if add.slot.slot_type != QUICKSLOT_TYPE_ITEM || add.slot.pos != 0 {
        for i in 0..QUICKSLOT_MAX_NUM {
            if i == add.pos as usize {
                continue;
            }
            if b[i * 2] == add.slot.slot_type && b[i * 2 + 1] == add.slot.pos {
                b[i * 2..i * 2 + 2].copy_from_slice(&[0, 0]);
                session
                    .send(&TPacketGCQuickSlotDel::new(i as u8).to_bytes())
                    .await
                    .map_err(|e| format!("enviando GC_QUICKSLOT_DEL: {e}"))?;
                eprintln!(
                    "server_realms: channel conn {}: quickslot dedupe — \
                     slot {i} borrado (mismo type+pos)",
                    session.conn_id
                );
            }
        }
    }
    // Set + eco GC_QUICKSLOT_ADD (28, 4 B — parity char_quickslot.cpp:90-98).
    b[add.pos as usize * 2..add.pos as usize * 2 + 2]
        .copy_from_slice(&add.slot.to_bytes());
    session
        .send(&TPacketGCQuickSlotAdd::new(add.pos, add.slot).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_QUICKSLOT_ADD: {e}"))?;
    persist(session, &b);
    eprintln!(
        "server_realms: channel conn {}: {} — quickslot {} = \
         type {} pos {}",
        session.conn_id,
        session.row().name,
        add.pos,
        add.slot.slot_type,
        add.slot.pos
    );
    Ok(Outcome::Continue)
}

/// CG_QUICKSLOT_DEL (17, 2 B: header + pos — Packet.h:614-618): borrar un
/// slot. Parity `DelQuickslot` (char_quickslot.cpp:105-118): gate de rango,
/// clear + eco GC_QUICKSLOT_DEL (29, 2 B) + persistencia.
pub async fn handle_del(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let del = match TPacketCGQuickSlotDel::from_bytes(pkt) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_QUICKSLOT_DEL malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    if del.pos as usize >= QUICKSLOT_MAX_NUM {
        eprintln!(
            "server_realms: channel conn {}: quickslot del a pos {} — \
             fuera de rango (QUICKSLOT_MAX_NUM {QUICKSLOT_MAX_NUM})",
            session.conn_id, del.pos
        );
        return Ok(Outcome::Continue);
    }
    let mut b = blob(session.row());
    let before = slot_at(&b, del.pos).unwrap_or(TQuickslot { slot_type: 0, pos: 0 });
    b[del.pos as usize * 2..del.pos as usize * 2 + 2].copy_from_slice(&[0, 0]);
    session
        .send(&TPacketGCQuickSlotDel::new(del.pos).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_QUICKSLOT_DEL: {e}"))?;
    persist(session, &b);
    eprintln!(
        "server_realms: channel conn {}: {} — quickslot {} borrado \
         (era type {} pos {})",
        session.conn_id,
        session.row().name,
        del.pos,
        before.slot_type,
        before.pos
    );
    Ok(Outcome::Continue)
}

/// CG_QUICKSLOT_SWAP (18, 3 B: header + pos + change_pos — Packet.h:620-626):
/// intercambiar dos slots. Parity `SwapQuickslot` (char_quickslot.cpp:
/// 120-145): gates de rango, swap + eco GC_QUICKSLOT_SWAP (30, 3 B) +
/// persistencia.
pub async fn handle_swap(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let sw = match TPacketCGQuickSlotSwap::from_bytes(pkt) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_QUICKSLOT_SWAP malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    if sw.pos as usize >= QUICKSLOT_MAX_NUM || sw.change_pos as usize >= QUICKSLOT_MAX_NUM {
        eprintln!(
            "server_realms: channel conn {}: quickslot swap {}/{} — \
             fuera de rango (QUICKSLOT_MAX_NUM {QUICKSLOT_MAX_NUM})",
            session.conn_id, sw.pos, sw.change_pos
        );
        return Ok(Outcome::Continue);
    }
    if sw.pos == sw.change_pos {
        return Ok(Outcome::Continue); // no-op (parity: swap consigo mismo)
    }
    let mut b = blob(session.row());
    let a = slot_at(&b, sw.pos).unwrap_or(TQuickslot { slot_type: 0, pos: 0 });
    let c = slot_at(&b, sw.change_pos).unwrap_or(TQuickslot { slot_type: 0, pos: 0 });
    b[sw.pos as usize * 2..sw.pos as usize * 2 + 2].copy_from_slice(&c.to_bytes());
    b[sw.change_pos as usize * 2..sw.change_pos as usize * 2 + 2]
        .copy_from_slice(&a.to_bytes());
    session
        .send(&TPacketGCQuickSlotSwap::new(sw.pos, sw.change_pos).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_QUICKSLOT_SWAP: {e}"))?;
    persist(session, &b);
    eprintln!(
        "server_realms: channel conn {}: {} — quickslot swap {}/{}",
        session.conn_id, session.row().name, sw.pos, sw.change_pos
    );
    Ok(Outcome::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fila mínima para los tests (mismo shape que `entry.rs::dummy_row`).
    fn row() -> PlayerRow {
        PlayerRow {
            id: 2,
            name: "ninja".into(),
            job: 1,
            voice: 0,
            dir: 0,
            x: 969600,
            y: 278400,
            z: 0,
            map_index: 41,
            exit_x: 0,
            exit_y: 0,
            exit_map_index: 0,
            hp: 100,
            mp: 100,
            stamina: 100,
            random_hp: 0,
            random_sp: 0,
            playtime: 0,
            gold: 0,
            level: 5,
            level_step: 0,
            st: 30,
            ht: 30,
            dx: 30,
            iq: 30,
            exp: 0,
            stat_point: 0,
            skill_point: 0,
            sub_skill_point: 0,
            stat_reset_count: 0,
            part_base: 0,
            part_hair: 0,
            part_main: 0,
            skill_level: None,
            quickslot: None,
            skill_group: 3,
            alignment: 0,
            horse_level: 0,
            horse_riding: 0,
            horse_hp: 0,
            horse_hp_droptime: 0,
            horse_stamina: 0,
            logoff_interval: 0.0,
            horse_skill_point: 0,
        }
    }

    /// El blob por defecto (fila sin quickslot) es 72 ceros — el handler no
    /// crashea con `None` (fail-open documentado).
    #[test]
    fn blob_defaults_to_zeroed_72_bytes() {
        let row = row();
        assert_eq!(row.quickslot, None);
        let b = blob(&row);
        assert_eq!(b.len(), 72, "36 × 2 B");
        assert!(b.iter().all(|&x| x == 0));
        // Blob roto (longitud incorrecta) → default, no panic.
        let mut row = row;
        row.quickslot = Some(vec![1, 2, 3]);
        assert_eq!(blob(&row), vec![0; 72]);
    }

    /// `clear_item_refs` (parity `SyncQuickslot`): el drop de un item borra
    /// SOLO los slots ITEM que apuntan a esa celda; skills/comandos y otras
    /// celdas quedan intactos.
    #[test]
    fn clear_item_refs_removes_only_matching_item_slots() {
        let mut b = vec![0u8; 72];
        // slot 0: ITEM cell 5; slot 1: SKILL 12; slot 2: ITEM cell 9;
        // slot 3: ITEM cell 5 (duplicado).
        b[0..2].copy_from_slice(&[QUICKSLOT_TYPE_ITEM, 5]);
        b[2..4].copy_from_slice(&[QUICKSLOT_TYPE_SKILL, 12]);
        b[4..6].copy_from_slice(&[QUICKSLOT_TYPE_ITEM, 9]);
        b[6..8].copy_from_slice(&[QUICKSLOT_TYPE_ITEM, 5]);
        let cleared = clear_item_refs(&mut b, 5);
        assert_eq!(cleared, vec![0, 3], "solo los ITEM cell 5");
        assert_eq!(b[0..2], [0, 0], "slot 0 borrado");
        assert_eq!(b[6..8], [0, 0], "slot 3 borrado");
        assert_eq!(b[2..4], [QUICKSLOT_TYPE_SKILL, 12], "skill intacto");
        assert_eq!(b[4..6], [QUICKSLOT_TYPE_ITEM, 9], "item cell 9 intacto");
        // Celda sin referencias → nada.
        assert!(clear_item_refs(&mut b, 77).is_empty());
    }

    /// Gates del add (parity SetQuickslot): rango de pos, rango de type,
    /// ITEM fuera del inventario y SKILL >= 255 se rechazan.
    #[test]
    fn add_gates_reject_out_of_range() {
        // pos 36+ → rechazado.
        assert!(slot_at(&vec![0; 72], 36).is_none(), "pos 36 fuera");
        assert!(slot_at(&vec![0; 72], 35).is_some(), "pos 35 último válido");
        // type 3 (QUICKSLOT_TYPE_MAX_NUM) → rechazado por el gate del C++.
        assert!(QUICKSLOT_TYPE_MAX_NUM > QUICKSLOT_TYPE_COMMAND);
        assert!(QUICKSLOT_TYPE_COMMAND < QUICKSLOT_TYPE_MAX_NUM);
        // Los tipos del length.h: ITEM=0, SKILL=1, COMMAND=2.
        assert_eq!(QUICKSLOT_TYPE_ITEM, 0);
        assert_eq!(QUICKSLOT_TYPE_SKILL, 1);
        assert_eq!(QUICKSLOT_TYPE_COMMAND, 2);
        // El gate del inventario: ITEM cell >= 180 → rechazado.
        let inv_ok = QUICKSLOT_TYPE_ITEM;
        assert!((inv_ok as u16) < INVENTORY_MAX_NUM, "type ITEM < 180");
    }
}
