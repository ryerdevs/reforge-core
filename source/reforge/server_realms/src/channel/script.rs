//! `channel/script.rs` — el handler del CG_SCRIPT_ANSWER (R-s3): el REVIVE
//! del jugador (parity `cmd_general.cpp:534-554` — RestartAtSamePos o el
//! warp a la ciudad) y — desde el lane quest — la RESPUESTA del diálogo de
//! quest (el [NEXT]/[QUESTION] del GC_SCRIPT 45 → reanuda la quest
//! suspendida en el mundo).
//!
//! CG_SCRIPT_ANSWER (29, 2 B: header + answer BYTE — Packet.h:679). El
//! diálogo de muerte del cliente manda la respuesta; el C++ revive con
//! `RestartAtSamePos` (el mismo punto) o warpea a la ciudad
//! (`WarpSet EMPIRE_START`). El diálogo de quest (mismo paquete) solo puede
//! estar abierto VIVO — la distinción es el hp (parity del C++: el quest
//! manager reanuda la quest antes que el flujo de muerte).
//!
//! C6a (firma uniforme): sin muerte / answer no-muerto → log + Continue.

use game_core::ecs::{CombatIntent, Intent, QuestIntent};
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
        revive(session, answer).await?;
    } else {
        // Diálogo de quest suspendido (CG_SCRIPT_ANSWER del GC_SCRIPT 45 —
        // el [NEXT]/[QUESTION] del quest dialog): la reanudación la resuelve
        // el mundo (`QuestIntent::Answer` — no-op si no hay quest suspendida;
        // el answer del select (1..n) se ata al capture `as name`).
        let answer = pkt.get(1).copied().unwrap_or(0);
        session.intent(Intent::Quest(QuestIntent::Answer {
            player_vid: session.player_vid(),
            answer,
        }))?;
        eprintln!(
            "server_realms: channel conn {}: respuesta de quest {answer} → mundo",
            session.conn_id
        );
    }
    Ok(Outcome::Continue)
}

/// REVIVE del jugador (compartido por el CG_SCRIPT_ANSWER del diálogo de
/// muerte y los comandos `/restart_here`/`/restart_town` — el C++ trata
/// ambos con el mismo flujo de do_restart, cmd_general.cpp:402-570).
///
/// `answer == 1` → revive EN LA CIUDAD (GC_WARP — el cliente reconecta con
/// DirectEnter; parity `WarpSet` de SCMD_RESTART_TOWN). Cualquier otro →
/// RestartAtSamePos (remove + insert en el mismo punto; parity
/// `ch->RestartAtSamePos()` + `PointChange(HP, 50-hp)` — el subset restaura
/// a los máximos, divergencia documentada). Restaura hp/mp a los máximos,
/// sincroniza el mundo COMPARTIDO, reenvía ADDITIONAL_INFO con los parts y
/// persiste.
pub async fn revive(session: &mut Session, answer: u8) -> Result<(), String> {
    // Restaurar hp/mp a los máximos del subset (parity
    // PointChange(POINT_HP, GetMaxHP()) — el revive del C++ restaura
    // antes de mostrar).
    let max = packets::compute_max_points(session.row()).unwrap_or([100, 100, 0]);
    {
        let row = session.row_mut();
        row.hp = max[0];
        row.mp = max[1];
    }
    session.save();
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
        let arrows = super::equipped_arrow_index(&session.inventory)
            .map(|i| session.inventory[i].count as u32)
            .unwrap_or(0);
        session
            .send(&packets::character_additional_info_with_parts(
                session.row(),
                session.empire,
                &parts,
                arrows,
            )
            .to_bytes())
            .await
            .map_err(|e| format!("enviando GC_CHARACTER_ADDITIONAL_INFO: {e}"))?;
        // GC_POINTS con hp/mp restaurados.
        session
            .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
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
    Ok(())
}
