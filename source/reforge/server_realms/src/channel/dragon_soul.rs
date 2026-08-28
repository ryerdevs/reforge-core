//! `channel/dragon_soul.rs` — refino de Dragon Soul (phase 1: wire + ledger
//! PG). CG_DRAGON_SOUL_REFINE (205, 47 B: header+bSubType+TItemPos[15] —
//! Packet.h:2715-2722; el SIZE 47 ya lo verifica el framer). Parity
//! `CInputMain` input_main.cpp:3197-3222: despacha solo bSubType 1..4
//! (CLOSE/GRADE/STEP/STRENGTH); OPEN(0) y el resto caen al olvido. El
//! refine real (materiales/fee/prob — DragonSoul.cpp:488+) es fase 2: hoy
//! cada intento se REGISTRA en `player.dragon_soul` (id por la IDENTITY de
//! PG) y el cliente recibe el FAIL determinista — la ventana no se cuelga.

use database::dragon_soul::DragonSoulRepo;

use super::session::{Outcome, Session};

/// `DS_SUB_HEADER_REFINE_FAIL_NOT_ENOUGH_MATERIAL` (packet.h:2234).
const REFINE_FAIL_NOT_ENOUGH_MATERIAL: u8 = 10;

/// Decodifica el wire: bSubType en [1] de un paquete de 47 B. El grid de 15
/// TItemPos (stride 3 desde [2]) entra solo en fase 2 (el handler no lee
/// materiales todavía).
fn parse(b: &[u8]) -> Option<u8> {
    let b: &[u8; 47] = b.try_into().ok()?;
    Some(b[1])
}

/// CG_DRAGON_SOUL_REFINE: valida el subType (parity input_main.cpp:3200-3220
/// — sin default) y registra el intento en el ledger PG.
pub async fn handle_refine(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let Some(sub) = parse(pkt) else {
        return Err("CG_DRAGON_SOUL_REFINE: 47 B".into());
    };
    if !(1..=4).contains(&sub) {
        return Ok(Outcome::Continue); // CLOSE/OPEN/desconocido — parity C++
    }
    let id = DragonSoulRepo::new(session.pool.clone())
        .record(session.row().id, i16::from(sub))
        .await?;
    // GC_DRAGON_SOUL_REFINE (209, 5 B: header+bSubType+TItemPos —
    // Packet.h:2724-2730): FAIL con Pos NPOS (0,0) — parity
    // SendRefineResultPacket (DragonSoul.cpp:970-987).
    session
        .send(&[protocol::header::GC_DRAGON_SOUL_REFINE, REFINE_FAIL_NOT_ENOUGH_MATERIAL, 0, 0, 0])
        .await
        .map_err(|e| format!("enviando GC_DRAGON_SOUL_REFINE: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: dragon soul refine {sub} → ledger id {id}",
        session.conn_id
    );
    Ok(Outcome::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER del wire (mutación): bSubType en el offset 1 de un paquete
    /// de 47 B exactos (el grid de 15 TItemPos — stride 3 — lo validará el
    /// refine de fase 2; el SIZE ya está anclado por el framer). Mutar el
    /// offset/layout → rojo.
    #[test]
    fn parse_layout_is_byte_exact() {
        let mut pkt = [0u8; 47];
        pkt[0] = 205;
        pkt[1] = 3; // DO_REFINE_STEP
        assert_eq!(parse(&pkt), Some(3), "bSubType en [1]");
        assert!(parse(&[205; 46]).is_none(), "layout corto → None");
        assert!(parse(&[205; 48]).is_none(), "layout largo → None");
        let mut open = pkt;
        open[1] = 0;
        assert_eq!(parse(&open), Some(0), "OPEN (0) viaja (el C++ lo ignora)");
    }
}