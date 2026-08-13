//! `channel/script.rs` — el handler del CG_SCRIPT_ANSWER (R-s3): el REVIVE
//! del jugador (parity `cmd_general.cpp:534-554` — RestartAtSamePos o el
//! warp a la ciudad).
//!
//! CG_SCRIPT_ANSWER (29, 2 B: header + answer BYTE — Packet.h:679). El
//! diálogo de muerte del cliente manda la respuesta; el C++ revive con
//! `RestartAtSamePos` (el mismo punto) o warpea a la ciudad
//! (`WarpSet EMPIRE_START`).
//!
//! C6a (firma uniforme): sin muerte / answer no-muerto → log + Continue.

use game_core::ecs::{CombatIntent, Intent};
use game_core::packets;

use crate::channel::session::{Outcome, Session};
use crate::channel::parse_listen;

/// CG_SCRIPT_ANSWER (29): revive con la respuesta del diálogo de muerte —
/// answer 1 → GC_WARP a la ciudad (el cliente RECONECTA con el flujo
/// DirectEnter completo); si no → RestartAtSamePos (remove + insert del
/// personaje en su sitio).
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if session.row().hp <= 0 {
        let answer = pkt.get(1).copied().unwrap_or(0);
        // Restaurar hp/mp a los máximos del subset (parity
        // PointChange(POINT_HP, GetMaxHP()) — el revive del C++ restaura
        // antes de mostrar).
        let max = packets::compute_max_points(session.row()).unwrap_or([100, 100, 0]);
        {
            let row = session.row_mut();
            row.hp = max[0];
            row.mp = max[1];
        }
        session.store().save_character(session.row());
        // El mundo COMPARTIDO refleja el HP/SP restaurados (el daño del AI y
        // el coste de las skills los gastan de ahí).
        session.intent(Intent::Combat(CombatIntent::SetHp {
            player_vid: session.player_vid(),
            hp: session.row().hp,
        }))?;
        session.intent(Intent::Combat(CombatIntent::SetMp {
            player_vid: session.player_vid(),
            mp: session.row().mp,
        }))?;
        if answer == 1 {
            // Revive EN LA CIUDAD: GC_WARP — el cliente cierra la conexión y
            // RECONECTA con el flujo DirectEnter completo (RecvWarpPacket →
            // Connect(lAddr, wPort) — F4 ya lo sirve). Destino: el punto de
            // salida del personaje (exit_x/y — el C++ usa EMPIRE_START; el
            // runtime actual: village del mapa 41).
            let (wx, wy) = if session.row().exit_x > 0 && session.row().exit_y > 0 {
                (session.row().exit_x, session.row().exit_y)
            } else {
                (969_600, 278_400) // village c1 mapa 41
            };
            let (ip, port) = parse_listen(&session.config.listen)?;
            let addr = packets::ip_to_inet_addr(&ip)?;
            session
                .send(&protocol::world::TPacketGCWarp::new(wx, wy, addr, port).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_WARP: {e}"))?;
            eprintln!(
                "server_realms: channel conn {}: {} revivió EN LA CIUDAD \
                 (answer {answer}) — GC_WARP {wx},{wy} → {}:{port}, reconexión",
                session.conn_id, session.row().name, ip
            );
        } else {
            // RestartAtSamePos: remove + insert del personaje (el cliente
            // reinicia la instancia en su sitio).
            let vid = session.player_vid();
            session
                .send(&protocol::world::TPacketGCCharacterDelete::new(vid).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_CHARACTER_DEL: {e}"))?;
            session
                .send(&packets::character_add(session.row()).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_CHARACTER_ADD: {e}"))?;
            // ADDITIONAL_INFO con los parts computados del equipo (el revive
            // reinserta la instancia).
            let parts = packets::equipped_parts(session.row(), &session.inventory);
            session
                .send(&packets::character_additional_info_with_parts(
                    session.row(),
                    session.empire,
                    &parts,
                )
                .to_bytes())
                .await
                .map_err(|e| format!("enviando GC_CHARACTER_ADDITIONAL_INFO: {e}"))?;
            // GC_POINTS con hp/mp restaurados.
            session
                .send(&packets::points_packet(session.row(), session.next_exp).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
            eprintln!(
                "server_realms: channel conn {}: {} REVIVIÓ (answer {answer}, \
                 hp {}/{}, mp {}/{})",
                session.conn_id,
                session.row().name,
                session.row().hp,
                max[0],
                session.row().mp,
                max[1]
            );
        }
    } else {
        // Sin muerte: el script answer del diálogo de quests es F5.x — se
        // ignora con log.
        eprintln!(
            "server_realms: channel conn {}: CG_SCRIPT_ANSWER sin muerte — \
             ignorado (quests F5.x)",
            session.conn_id
        );
    }
    Ok(Outcome::Continue)
}
