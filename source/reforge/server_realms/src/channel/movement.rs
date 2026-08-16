//! `channel/movement.rs` — el handler del CG_MOVE (R-s3 del refactor):
//! la validación anti-speedhack del movimiento del jugador (timer +
//! distancia + envelope por entidad + walkability del destino) y el intent
//! hacia el MUNDO COMPARTIDO.
//!
//! Parity `input_main.cpp:1437-1599` (`CInputMain::Move`):
//! - **Timer speedhack** (la validación ACTIVA del build — `:1494-1516`):
//!   `iDelta >= 30000` → slow timer; `iDelta < -(iServerDelta/50)` → fast
//!   timer; ambos → `DelayedDisconnect(3)` — el C++ kickea. Aquí →
//!   `Outcome::Close` (C6a: el cierre protocolario es un Outcome, no un Err).
//! - **Distancia** (`ENABLE_TP_SPEED_CHECK` — `:1463-1482`, define comentado
//!   pero implementado como defensa con la MISMA tolerancia): `fDist > 2500`
//!   (sin montura) o `> 6000` (montura) → corrección Show+Stop (el MOVE se
//!   rechaza, la posición queda) → `Outcome::Continue`.
//! - **F5.4 (ADR-0011)**: envelope por entidad (speed × Δt de server +
//!   tolerancia — `game_core::movement`) y walkability server-side del destino
//!   como REFUERZO del anti-teleport (2026-08-13: el gate previo atascaba al
//!   jugador en la fuente del pueblo — ver el handler). Ambos → corrección
//!   sin ban.
//! - **Aceptado** → `Goto(lX, lY)` (`:1532`) + el intent `Move` al mundo
//!   (el AI del tick y el spawn dinámico leen la posición NUEVA).
//! - **Sin ack para el jugador local**: el server manda `TPacketGCMove`
//!   SOLO a los observadores (`PacketAround(..., ch)` excluido, `:1576-1588`).
//!
//! C6a (firma uniforme): malformado/rechazos → log + `Outcome::Continue`;
//! el cierre del speedhack → `Outcome::Close(razón)`.

use game_core::ecs::{Intent, MoveIntent};

use crate::channel::session::{Outcome, Session};
use crate::channel::now32;

/// `POSITION_GENERAL = 0`, `POSITION_SITTING_CHAIR = 3`,
/// `POSITION_SITTING_GROUND = 4` (length.h:288-296: GENERAL=0, BATTLE=1,
/// DYING=2, SITTING_CHAIR=3, SITTING_GROUND=4) — posturas del wire.
const POSITION_GENERAL: u8 = 0;
const POSITION_SITTING_CHAIR: u8 = 3;
const POSITION_SITTING_GROUND: u8 = 4;

/// Postura del wire (parity Position input_main.cpp:1276-1295):
/// `None` = posición desconocida (rechazo); `Some((sentado, wire_pos))` con
/// el wire_pos de la RESPUESTA (Standup → GENERAL; Sitdown → SIEMPRE
/// SITTING_GROUND — el C++ ignora `is_ground` en el paquete).
fn posture(position: u8) -> Option<(bool, u8)> {
    match position {
        POSITION_GENERAL => Some((false, POSITION_GENERAL)),
        POSITION_SITTING_CHAIR | POSITION_SITTING_GROUND => {
            Some((true, POSITION_SITTING_GROUND))
        }
        _ => None,
    }
}

/// CG_CHARACTER_POSITION (28, 2 B: header + position — Packet.h:653-657).
/// Parity `Position` (input_main.cpp:1276-1295): POSITION_GENERAL →
/// `Standup()`; SITTING_CHAIR/GROUND → `Sitdown()`. El estado de postura
/// vive en la SESIÓN (`session.sitting` — el `m_pointsInstant.position` del
/// C++ no se persiste); el C++ difunde `packet_position` a la zona
/// visible (`PacketAround` — TPacketGCPosition 6 B: header + vid +
/// position; el broadcast multi-jugador es F5 — aquí se reenvía al propio
/// jugador para mantener el wire en sync). Parity del envío: Standup →
/// POSITION_GENERAL; Sitdown → SIEMPRE POSITION_SITTING_GROUND (el C++
/// ignora `is_ground` en el paquete). Muerto → rechazo (POS_DEAD no
/// cambia).
pub async fn handle_position(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() < 2 {
        eprintln!(
            "server_realms: channel conn {}: CG_CHARACTER_POSITION malformado ({})",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    }
    let position = pkt[1];
    if session.row().hp <= 0 {
        eprintln!(
            "server_realms: channel conn {}: {} — postura pedida con hp 0 \
             (muerto) — ignorada (parity POS_DEAD)",
            session.conn_id, session.row().name
        );
        return Ok(Outcome::Continue);
    }
    // El position del WIRE de vuelta (parity del C++: Standup → GENERAL;
    // Sitdown → SITTING_GROUND siempre).
    let Some((sitting, wire_pos)) = posture(position) else {
        eprintln!(
            "server_realms: channel conn {}: postura {position} desconocida \
             — ignorada",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    };
    session.sitting = sitting;
    // TPacketGCPosition (HEADER_GC_CHARACTER_POSITION = 43, 6 B: header +
    // vid + position — packet.h:159, packet.h:1238-1243; el cliente lo
    // recibe en RecvCharacterPositionPacket).
    let vid = session.player_vid();
    let reply = [43, vid as u8, (vid >> 8) as u8, (vid >> 16) as u8, (vid >> 24) as u8, wire_pos];
    session
        .send(&reply)
        .await
        .map_err(|e| format!("enviando GC_CHARACTER_POSITION: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: {} — {} (postura {} — \
         broadcast a la zona pendiente, F5)",
        session.conn_id,
        session.row().name,
        if sitting { "SENTADO" } else { "PARADO" },
        wire_pos
    );
    Ok(Outcome::Continue)
}

/// CG_MOVE (7): valida el movimiento y, si pasa, actualiza la posición del
/// jugador (game_core::movement) + el intent `Move` al mundo.
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    // F5.1: el movimiento del jugador. El cliente se mueve LOCALMENTE (sin
    // ack — el server responde el GC_CHARACTER_MOVE solo a los observadores,
    // input_main.cpp:1576-1588). La validación anti-speedhack: timer
    // (input_main.cpp:1494-1516) + distancia (el umbral del TP_SPEED_CHECK)
    // + F5.4 envelope por entidad + walkability del destino.
    let mv = match protocol::movement::TPacketCGMove::from_bytes(pkt) {
        Ok(mv) => mv,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_MOVE malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    // F5.4 (ADR-0011): walkability server-side como refuerzo del
    // ANTI-TELEPORT — DIAGNÓSTICO 2026-08-13: el gate previo (rechazar todo
    // MOVE a celda no movible) atascaba al jugador real en la FUENTE del
    // pueblo (celdas ATTR_BLOCK legítimas del server_attr — el C++ las ve
    // igual, sectree_manager.cpp:760 — pero el cliente, con colisión por
    // píxel del modelo, permite pararse en su borde: (969595,278398) cae en
    // la celda de la fuente). El C++ NO valida walkability en Move()
    // (input_main.cpp:1437-1599): los pasos NORMALES (dentro del envelope) se
    // aceptan como el C++ (el cliente valida su propia colisión); SOLO los
    // saltos anómalos (fuera del envelope) verifican que el destino no sea
    // terreno bloqueado — un teleport no puede aterrizar en una montaña.
    // C27: el envelope anti-speedhack escala con la velocidad EFECTIVA del
    // personaje (POINT_MOV_SPEED — la bota equipada la sube; parity
    // `GetMoveMotionSpeed() * 10000 / CalculateDuration(GetLimitPoint(
    // POINT_MOV_SPEED), 10000)` — char.cpp:2753: el cliente se mueve a
    // `velocidad_base × mov_speed/100`). El `speed` del motion es la base
    // 500 del envelope (DEFAULT_MOVE_SPEED, ajustada al cliente real
    // 2026-08-13) — con la bota, la base escala igual que el C++.
    session.motion_mut().speed =
        game_core::movement::DEFAULT_MOVE_SPEED * u32::from(session.mov_speed) / 100;
    match game_core::movement::process_move(session.motion_mut(), &mv, now32()) {
        Ok(r) => {
            eprintln!(
                "server_realms: channel conn {}: MOVE {} -> {},{} (func {})",
                session.conn_id, session.row().name, r.x, r.y, mv.b_func
            );
            // El mundo COMPARTIDO persigue la posición NUEVA (intent — el AI
            // del tick y el spawn dinámico la leen).
            session.intent(Intent::Move(MoveIntent::Move {
                player_vid: session.player_vid(),
                x: session.motion().x,
                y: session.motion().y,
            }))?;
            // El peer de chat persigue la posición (el rango del broadcast
            // GC_CHAT usa la posición VIVA — gap-lane-C).
            crate::channel::chat::update_position(
                session.player_vid(),
                session.motion().x,
                session.motion().y,
            );
            // El peer/miembro del party persigue la posición (el rango del
            // reparto de exp usa la posición VIVA — lane 2026-08-16).
            crate::channel::party::update_position(
                session.player_vid(),
                session.motion().x,
                session.motion().y,
            );
            Ok(Outcome::Continue)
        }
        Err(game_core::movement::MoveError::NotMove) => {
            // ACCIÓN (ataque/skill/combo) — el procesamiento es F5; se loguea.
            eprintln!(
                "server_realms: channel conn {}: MOVE func {} de {} — \
                 acción pendiente de integración (F5)",
                session.conn_id, mv.b_func, session.row().name
            );
            Ok(Outcome::Continue)
        }
        Err(e @ (game_core::movement::MoveError::SlowTimer
        | game_core::movement::MoveError::FastTimer)) => {
            // Kick del C++ (DelayedDisconnect(3), input_main.cpp:1505-1515) —
            // el canal cierra la conexión (C6a: cierre protocolario).
            eprintln!(
                "server_realms: channel conn {}: SPEEDHACK {} ({:?}) — \
                 cierre (parity DelayedDisconnect)",
                session.conn_id, session.row().name, e
            );
            Ok(Outcome::Close(format!("speedhack de {}", session.row().name)))
        }
        Err(game_core::movement::MoveError::TooFar) => {
            // Corrección del C++ (Show+Stop — el define TP_SPEED_CHECK está
            // comentado, pero es el anti-teleport estándar): se rechaza el
            // MOVE, la posición queda.
            eprintln!(
                "server_realms: channel conn {}: MOVE teleport de {} — \
                 rechazado (posición {} ,{})",
                session.conn_id,
                session.row().name,
                session.motion().x,
                session.motion().y
            );
            Ok(Outcome::Continue)
        }
        Err(game_core::movement::MoveError::ExceedsEnvelope) => {
            // F5.4 (ADR-0011): el MOVE excede el envelope por entidad (speed ×
            // Δt de server + tolerancia) — slow-accumulate o teleport. Si el
            // destino además NO es movible, el rechazo se reporta como
            // walkability (anti-teleport a terreno bloqueado — el refuerzo
            // del 2026-08-13). Ambos: corrección, no ban (plan §5.7): el MOVE
            // se rechaza, la posición queda. El auto-ban por N violaciones
            // queda documentado como follow-up con knobs de config.
            let dest_movable = match game_core::map::is_movable(
                &session.map_store,
                &session.config.map_path,
                session.row().map_index,
                mv.x,
                mv.y,
            ) {
                Ok(m) => m,
                Err(e) => {
                    // Fail-open: el mapa no cargó — el envelope decide solo.
                    if !session.walkability_warned {
                        eprintln!(
                            "server_realms: channel conn {}: \
                             walkability NO disponible (mapa {}): {e} — \
                             chequeo omitido (fail-open), envelope activo",
                            session.conn_id, session.row().map_index
                        );
                        session.walkability_warned = true;
                    }
                    true
                }
            };
            if !dest_movable {
                eprintln!(
                    "server_realms: channel conn {}: MOVE de {} a \
                     celda NO movible ({},{}) — teleport rechazado \
                     (posición {} ,{})",
                    session.conn_id,
                    session.row().name,
                    mv.x,
                    mv.y,
                    session.motion().x,
                    session.motion().y
                );
            } else {
                eprintln!(
                    "server_realms: channel conn {}: MOVE de {} fuera \
                     del envelope (speed {}) — rechazado (posición {} ,{})",
                    session.conn_id,
                    session.row().name,
                    session.motion().speed,
                    session.motion().x,
                    session.motion().y
                );
            }
            Ok(Outcome::Continue)
        }
        Err(game_core::movement::MoveError::InvalidFunc) => {
            eprintln!(
                "server_realms: channel conn {}: MOVE func inválido de {}",
                session.conn_id, session.row().name
            );
            Ok(Outcome::Continue)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Posturas del CG_CHARACTER_POSITION (parity Position input_main.cpp:
    /// 1276-1295): GENERAL → parado; SITTING_CHAIR/GROUND → sentado con
    /// wire SIEMPRE SITTING_GROUND (el C++ ignora is_ground en el paquete);
    /// cualquier otro valor → rechazo.
    #[test]
    fn posture_matches_cpp_position_switch() {
        assert_eq!(posture(0), Some((false, 0)), "GENERAL → Standup");
        assert_eq!(posture(3), Some((true, 4)), "SITTING_CHAIR → Sitdown (wire GROUND)");
        assert_eq!(posture(4), Some((true, 4)), "SITTING_GROUND → Sitdown");
        assert_eq!(posture(1), None, "BATTLE → rechazo (sin case en Position)");
        assert_eq!(posture(2), None, "DYING → rechazo (sin case en Position)");
        assert_eq!(posture(5), None, "POSITION_INTRO → rechazo");
        assert_eq!(posture(0xFF), None);
        // Constantes del length.h:288-296.
        assert_eq!(POSITION_GENERAL, 0);
        assert_eq!(POSITION_SITTING_CHAIR, 3);
        assert_eq!(POSITION_SITTING_GROUND, 4);
    }

    /// El wire del GC_CHARACTER_POSITION (43, 6 B: header + vid LE + pos) —
    /// shape verificado contra `packet_position` (packet.h:1238-1243).
    #[test]
    fn position_reply_wire_shape() {
        let vid: u32 = 0x1122_3344;
        let reply = [
            43,
            vid as u8,
            (vid >> 8) as u8,
            (vid >> 16) as u8,
            (vid >> 24) as u8,
            POSITION_SITTING_GROUND,
        ];
        assert_eq!(reply.len(), 6);
        assert_eq!(reply[0], 43, "header GC_CHARACTER_POSITION");
        assert_eq!(&reply[1..5], &[0x44, 0x33, 0x22, 0x11], "vid LE");
        assert_eq!(reply[5], 4, "position SITTING_GROUND");
    }
}
