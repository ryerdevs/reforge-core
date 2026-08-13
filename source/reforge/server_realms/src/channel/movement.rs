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
//!   (`game_core::map::is_movable` — el C++ NO valida esto en Move(); control
//!   NUEVO server-authoritative, plan §5.7). Ambos → corrección sin ban.
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
    // F5.4 (ADR-0011): walkability server-side ANTES de aceptar — el destino
    // debe ser movible (parity sectree_manager.cpp:753-761:
    // !(attr & (ATTR_BLOCK|ATTR_OBJECT))). El C++ NO valida esto en Move() —
    // es un control NUEVO server-authoritative (plan §5.7: el server es
    // dueño de la posición; corrección, no ban — sin auto-ban aún). Un
    // destino no movible → rechazo: la posición queda y el mundo NO recibe
    // el intent.
    match game_core::map::is_movable(
        &session.map_store,
        &session.config.map_path,
        session.row().map_index,
        mv.x,
        mv.y,
    ) {
        Err(e) => {
            if !session.walkability_warned {
                eprintln!(
                    "server_realms: channel conn {}: \
                     walkability NO disponible (mapa {}): {e} — \
                     chequeo omitido (fail-open), envelope activo",
                    session.conn_id, session.row().map_index
                );
                session.walkability_warned = true;
            }
        }
        Ok(false) => {
            eprintln!(
                "server_realms: channel conn {}: MOVE de {} a \
                 celda NO movible ({},{}) — rechazado (posición {} ,{})",
                session.conn_id,
                session.row().name,
                mv.x,
                mv.y,
                session.motion().x,
                session.motion().y
            );
            return Ok(Outcome::Continue);
        }
        Ok(true) => {}
    }
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
            // F5.4 (ADR-0011): envelope por entidad (speed × Δt de server +
            // tolerancia) — slow-accumulate. Corrección, no ban (plan §5.7):
            // se rechaza el MOVE, la posición queda. El auto-ban por
            // N violaciones queda documentado como follow-up con knobs de
            // config.
            eprintln!(
                "server_realms: channel conn {}: MOVE de {} fuera \
                 del envelope (speed {}) — rechazado (posición {} ,{})",
                session.conn_id,
                session.row().name,
                session.motion().speed,
                session.motion().x,
                session.motion().y
            );
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
