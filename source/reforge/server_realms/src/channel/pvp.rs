//! `channel/pvp.rs` — el handler del CG_PVP (41, lane D): el flag de PvP
//! on/off del jugador.
//!
//! Hallazgo del gap analysis 2026-08-15: el cliente de esta variante define
//! `HEADER_CG_PVP = 41` (Packet.h:53) pero NUNCA lo envía (sin sender — el
//! sistema de duelo/PvP del Metin2 completo no está desplegado) y el C++ de
//! esta variante tampoco tiene handler (input_main.cpp no lo despacha). El
//! framer fija el tamaño a 10 B (header + dwVIDSrc + dwVIDDst + bMode — el
//! layout del TPacketGCPVP del duelo, Packet.h:2014-2020).
//!
//! Este handler es DEFENSIVO: si algún día llega, se parsea (10 B, o 2 B
//! header+mode como fallback), se guarda el flag en la SESIÓN (no hay
//! columna en `player.player` — parity: el TPlayerTable del C++ tampoco la
//! tiene; el modo PvP del Metin2 completo es efímero de sesión) y se
//! loguea. NO se responde GC_PVP (el eco 10 B del duelo es del sistema PvP
//! completo — no existe aquí; responderlo insertaría PVP keys en el cliente
//! sin un duelo real).
//!
//! C6a (firma uniforme): malformado → log + `Outcome::Continue`.

use crate::channel::session::{Outcome, Session};
use game_core::ecs::{CombatIntent, Intent};

/// CG_PVP (41): flag pvp on/off. `mode != 0` → PvP activo. El paquete del
/// framer es de 10 B (header + vidSrc + vidDst + mode — Packet.h:2014-2020);
/// se acepta también el 2 B (header + mode) como fallback defensivo.
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let mode = match pkt.len() {
        10 => pkt[9],
        2 => pkt[1],
        _ => {
            eprintln!(
                "server_realms: channel conn {}: CG_PVP malformado ({})",
                session.conn_id,
                pkt.len()
            );
            return Ok(Outcome::Continue);
        }
    };
    let on = mode != 0;
    session.pvp_mode = on;
    // El mundo COMPARTIDO también lo necesita: el gate `battle_is_attackable`
    // (process_attack — PvP) se evalúa donde están AMBOS jugadores (la
    // sesión del atacante no ve la del objetivo). Error → log (no fatal —
    // el handler es defensivo).
    if let Err(e) = session.intent(Intent::Combat(CombatIntent::SetPvpMode {
        player_vid: session.player_vid(),
        on,
    })) {
        eprintln!(
            "server_realms: channel conn {}: CG_PVP — sincronizando el \
             mundo: {e}",
            session.conn_id
        );
    }
    eprintln!(
        "server_realms: channel conn {}: {} — PvP {} (mode {mode}) — \
         flag de sesión + mundo (sin columna en player.player, parity \
         TPlayerTable; sin eco GC_PVP — el sistema de duelo no está \
         desplegado)",
        session.conn_id,
        session.row().name,
        if on { "ON" } else { "OFF" }
    );
    Ok(Outcome::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El parseo acepta el layout del framer (10 B — Packet.h:2014-2020) y
    /// el fallback 2 B; cualquier otra longitud → rechazo limpio.
    #[test]
    fn pvp_mode_parses_10b_and_2b() {
        assert_eq!(mode_of(&[41, 0, 0, 0, 0, 0, 0, 0, 0, 0]), Some(false), "10 B off");
        assert_eq!(mode_of(&[41, 1, 2, 3, 4, 5, 6, 7, 8, 1]), Some(true), "10 B on");
        assert_eq!(mode_of(&[41, 0]), Some(false), "2 B off");
        assert_eq!(mode_of(&[41, 1]), Some(true), "2 B on");
        assert_eq!(mode_of(&[41]), None, "1 B → malformado");
        assert_eq!(mode_of(&[41, 0, 0]), None, "3 B → malformado");
    }

    /// Extrae el modo con la MISMA lógica que el handler (sin sesión).
    fn mode_of(pkt: &[u8]) -> Option<bool> {
        match pkt.len() {
            10 => Some(pkt[9] != 0),
            2 => Some(pkt[1] != 0),
            _ => None,
        }
    }
}
