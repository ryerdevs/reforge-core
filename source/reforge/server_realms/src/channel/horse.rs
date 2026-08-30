//! `channel/horse.rs` — CABALLO montable (FASE 1): `CG_HORSE` (63, aditivo
//! del reforge — el cliente real monta por `/ride` chat, cmd_general.cpp:
//! 37-65; GAP: el comando chat sigue en el lote INFO del GM) → toggle
//! montado + wire (GC_CHARACTER_UPDATE dwMountVnum — parity MountVnum/
//! UpdatePacket char.cpp:1044 — + horse_state/hide_horse_state —
//! char_horse.cpp:309-345) + persistencia (save — PLAYER_SAVE_SQL con
//! horse_riding/horse_hp: player.rs:450).

use game_core::{horse, packets};

use super::session::{Outcome, Session};

/// `CG_HORSE` (63, 2 B: header + bRide — 1 = montar, 0 = desmontar).
/// Parity gates de `CHorseRider::StartRiding` (horse_rider.cpp:165-193):
/// rechazo silencioso sin caballo/HP/stamina o ya en el estado pedido.
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let ride = pkt.get(1).is_some_and(|&b| b != 0);
    let (level, hp, st, riding, empire) = {
        let r = session.row();
        (
            r.horse_level as u8,
            r.horse_hp as u16,
            r.horse_stamina as u16,
            r.horse_riding != 0,
            session.empire,
        )
    };
    let Some(vnum) = horse::toggle_ride(level, hp, st, riding, ride) else {
        return Ok(Outcome::Continue); // gates silenciosos (parity)
    };
    // El estado NUEVO primero (parity MountVnum → UpdatePacket — el C++
    // manda el dwMountVnum POST-mutación; el builder lo deriva del row).
    {
        let row = session.row_mut();
        row.horse_riding = i16::from(vnum != 0);
    }
    // Wire: UPDATE (19) con dwMountVnum — el cliente monta/desmonta visual
    // (RecvCharacterUpdatePacket → NotifyCharacterUpdate) + el estado del
    // caballo (horse_state pinta el window; hide lo oculta — game.py:1895,
    // 1973-1977). Solo el propio jugador (el broadcast a peers en rango es
    // GAP de fase 2 junto al ADD del mundo compartido).
    let parts = packets::equipped_parts(session.row(), &session.inventory);
    let arrows = super::equipped_arrow_index(&session.inventory)
        .map(|i| session.inventory[i].vnum as u32)
        .unwrap_or(0);
    let upd =
        packets::character_update_with_parts(session.row(), &parts, arrows, session.mov_speed);
    let msg = if vnum == 0 {
        packets::chat_command(empire, "hide_horse_state")
    } else {
        packets::horse_state_command(session.row(), empire)
    };
    session
        .send(&upd.to_bytes())
        .await
        .map_err(|e| format!("enviando GC_CHARACTER_UPDATE (caballo): {e}"))?;
    session
        .send(&msg)
        .await
        .map_err(|e| format!("enviando horse_state (caballo): {e}"))?;
    session.save(); // persiste horse_riding/horse_hp (WAL, idempotente)
    Ok(Outcome::Continue)
}
