//! F5.1: el estado de movimiento del jugador y el procesamiento del `CG_MOVE`
//! con la validación anti-speedhack del C++.
//!
//! Parity `CInputMain::Move` (`input_main.cpp:1437-1599`):
//! - **Timer speedhack** (la validación ACTIVA del build — `:1494-1516`):
//!   `iDelta = server_now - packet.dw_time` (el reloj del cliente);
//!   `iDelta >= 30000` → slow timer; `iDelta < -(iServerDelta/50)` → fast
//!   timer; ambos → `DelayedDisconnect(3)` (kick). El `iServerDelta` = el
//!   tiempo del server entre MOVE.
//! - **Distancia** (`ENABLE_TP_SPEED_CHECK` — `:1463-1482`): el define está
//!   COMENTADO en el source actual, pero es el anti-teleport estándar:
//!   `fDist = sqrt(dx² + dy²)` en UNITS (`utils.h:14-16`); `fDist > 2500`
//!   (25 m, sin montura) o `> 6000` (60 m, con montura) → corrección
//!   (`Show` + `Stop` — el MOVE se rechaza, la posición queda). Se implementa
//!   como defensa con la MISMA tolerancia (documentada como "comentado en el
//!   C++ actual").
//! - **Aceptado** → `Goto(lX, lY)` (`:1532`) — la posición se actualiza.
//! - **Sin ack para el jugador local**: el server manda `TPacketGCMove` SOLO
//!   a los observadores (`PacketAround(..., ch)` excluido, `:1576-1588`).

use protocol::movement::TPacketCGMove;

/// El estado de movimiento de un jugador (mapa simple id → estado; el
/// ECS/bevy_ecs se decide cuando haya NPCs — F5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerMotion {
    pub x: i32,
    pub y: i32,
    /// Reloj del cliente del último MOVE aceptado (anti-speedhack).
    pub last_client_time: u32,
    /// Reloj del server del último MOVE aceptado.
    pub last_server_time: u32,
    /// Montura (afecta la tolerancia de distancia: 25 m vs 60 m).
    pub riding: bool,
}

/// Resultado de un MOVE procesado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveResult {
    pub x: i32,
    pub y: i32,
}

/// Motivo de rechazo del MOVE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveError {
    /// `iDelta >= 30000` — el reloj del cliente va lento (input_main.cpp:1505).
    SlowTimer,
    /// `iDelta < -(iServerDelta/50)` — el reloj del cliente va rápido
    /// (input_main.cpp:1511).
    FastTimer,
    /// `fDist > 2500` (25 m — el umbral del `ENABLE_TP_SPEED_CHECK`,
    /// input_main.cpp:1466) — salto/teleport.
    TooFar,
    /// `bFunc` inválido (`>= FUNC_MAX_NUM && !(bFunc & 0x80)`,
    /// input_main.cpp:1444).
    InvalidFunc,
    /// El MOVE no es de movimiento (FUNC_ATTACK/COMBO/SKILL — el procesamiento
    /// de acciones es F5).
    NotMove,
}

/// Distancia máxima por MOVE sin montura: 25 m = 2500 units
/// (`input_main.cpp:1466` — `fDist > 25`; el C++ divide por 100).
const MAX_DIST_NO_RIDING: i64 = 2500;
/// Con montura: 60 m = 6000 units (`input_main.cpp:1466`).
const MAX_DIST_RIDING: i64 = 6000;
/// `iDelta >= 30000` → slow timer (input_main.cpp:1505).
const SLOW_TIMER_MS: i64 = 30_000;

/// Procesa un `CG_MOVE` con la validación del C++ (timer + distancia).
/// `now_ms` = el reloj del server en ms (get_dword_time).
///
/// Errores:
/// - `SlowTimer`/`FastTimer` → el C++ hace `DelayedDisconnect(3)` (kick) —
///   el caller decide el cierre.
/// - `TooFar` → el C++ (define comentado) hace corrección (Show+Stop) — el
///   MOVE se RECHAZA sin actualizar la posición (el jugador se queda).
/// - `NotMove` → la acción (ataque/skill) es F5 — se loguea, no se procesa.
pub fn process_move(
    state: &mut PlayerMotion,
    packet: &TPacketCGMove,
    now_ms: u32,
) -> Result<MoveResult, MoveError> {
    // Validación del bFunc (input_main.cpp:1444-1448).
    if packet.b_func >= 6 && (packet.b_func & TPacketCGMove::FUNC_SKILL) == 0 {
        return Err(MoveError::InvalidFunc);
    }
    // El movimiento real es FUNC_MOVE; las demás acciones son F5.
    if packet.b_func != TPacketCGMove::FUNC_MOVE {
        return Err(MoveError::NotMove);
    }

    // Timer speedhack (la validación ACTIVA del build — input_main.cpp:1494-1516).
    // El C++ castea la resta u32 a `int` (`iDelta = (int)(dwCurTime - dwTime)`)
    // — el wrap del reloj del cliente se interpreta CON SIGNO (parity).
    let server_delta = i64::from(now_ms.wrapping_sub(state.last_server_time) as i32);
    let i_delta = i64::from(now_ms.wrapping_sub(packet.dw_time) as i32);
    if i_delta >= SLOW_TIMER_MS {
        return Err(MoveError::SlowTimer);
    }
    if i_delta < -(server_delta / 50) {
        return Err(MoveError::FastTimer);
    }

    // Distancia (anti-teleport — el umbral del ENABLE_TP_SPEED_CHECK, comentado
    // en el source pero implementado como defensa con la misma tolerancia).
    let dx = i64::from(packet.x) - i64::from(state.x);
    let dy = i64::from(packet.y) - i64::from(state.y);
    let dist_sq = dx * dx + dy * dy;
    let max_dist = if state.riding { MAX_DIST_RIDING } else { MAX_DIST_NO_RIDING };
    if dist_sq > max_dist * max_dist {
        return Err(MoveError::TooFar);
    }

    // Aceptado (parity `Goto(lX, lY)` — input_main.cpp:1532).
    state.x = packet.x;
    state.y = packet.y;
    state.last_client_time = packet.dw_time;
    state.last_server_time = now_ms;
    Ok(MoveResult { x: packet.x, y: packet.y })
}

/// Estado inicial desde una posición cargada del player (el primer MOVE tiene
/// `last_server_time = now_ms` — sin `iServerDelta` previo).
pub fn initial(x: i32, y: i32) -> PlayerMotion {
    PlayerMotion {
        x,
        y,
        last_client_time: 0,
        last_server_time: 0,
        riding: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn move_to(x: i32, y: i32, time: u32) -> TPacketCGMove {
        TPacketCGMove {
            header: 7,
            b_func: TPacketCGMove::FUNC_MOVE,
            b_arg: 0,
            b_rot: 0,
            x,
            y,
            dw_time: time,
        }
    }

    /// El envelope: mover dentro del límite (25 m = 2500 units por MOVE)
    /// actualiza la posición; un salto mayor se rechaza (TooFar) sin tocar la
    /// posición — parity del anti-teleport del C++ (input_main.cpp:1466).
    #[test]
    fn envelope_accepts_within_limit_and_rejects_teleport() {
        let mut st = initial(0, 0);
        // 2000 units (< 2500) → OK.
        let r = process_move(&mut st, &move_to(2000, 0, 1000), 1100).expect("dentro del límite");
        assert_eq!(r, MoveResult { x: 2000, y: 0 });
        assert_eq!((st.x, st.y), (2000, 0), "posición actualizada");
        // 3000 units (> 2500) → TooFar; la posición NO cambia.
        let err = process_move(&mut st, &move_to(5000, 0, 2000), 2100).expect_err("teleport");
        assert_eq!(err, MoveError::TooFar);
        assert_eq!((st.x, st.y), (2000, 0), "el MOVE rechazado no actualiza");
        // Diagonal 2000,2000 → sqrt(8M) ≈ 2828 > 2500 → rechazo.
        let err = process_move(&mut st, &move_to(4000, 2000, 3000), 3100).expect_err("diagonal");
        assert_eq!(err, MoveError::TooFar);
        // Con montura: 5000 units (< 6000) → OK.
        st.riding = true;
        let r = process_move(&mut st, &move_to(7000, 0, 4000), 4100).expect("montura");
        assert_eq!(r.x, 7000);
    }

    /// El timer speedhack (la validación ACTIVA — input_main.cpp:1505-1515):
    /// el reloj del cliente 30s+ atrasado → SlowTimer; muy adelantado →
    /// FastTimer.
    #[test]
    fn speedhack_timer_rejects() {
        let mut st = initial(0, 0);
        // El primer MOVE: last_server_time=0 → iServerDelta = now - 0 (grande)
        // — el C++ arranca con GetClientTime() inicializado; para el primer
        // MOVE usamos un estado con el reloj ya anclado (parity: el cliente
        // manda el time del handshake ~= el del server).
        st.last_server_time = 10_000;
        st.last_client_time = 10_000;
        // Slow timer: el dwTime del paquete 40s atrás → iDelta = 40000 >= 30000.
        // (el reloj del cliente wrappea — parity del wire u32).
        let slow_time = 10_000u32.wrapping_sub(40_000);
        let err = process_move(&mut st, &move_to(100, 0, slow_time), 60_000)
            .expect_err("slow timer");
        assert_eq!(err, MoveError::SlowTimer);
        // Fast timer: el dwTime 10s en el futuro con iServerDelta=100ms →
        // iDelta = -10000 < -(100/50) → FastTimer.
        let err = process_move(&mut st, &move_to(100, 0, 70_000), 60_100)
            .expect_err("fast timer");
        assert_eq!(err, MoveError::FastTimer);
        // El reloj razonable pasa.
        let r = process_move(&mut st, &move_to(100, 0, 60_000), 60_100).expect("reloj OK");
        assert_eq!(r.x, 100);
    }

    /// El bFunc inválido y las acciones no-movimiento (ataque/skill) se
    /// rechazan sin tocar la posición (el procesamiento de acciones es F5).
    #[test]
    fn invalid_func_and_actions() {
        let mut st = initial(0, 0);
        let mut p = move_to(100, 0, 1000);
        p.b_func = 6; // FUNC_MAX_NUM, sin el bit 0x80 -> InvalidFunc (input_main.cpp:1444)
        assert_eq!(process_move(&mut st, &p, 1100), Err(MoveError::InvalidFunc));
        p.b_func = TPacketCGMove::FUNC_ATTACK;
        assert_eq!(process_move(&mut st, &p, 1100), Err(MoveError::NotMove));
        assert_eq!((st.x, st.y), (0, 0), "nada se aplicó");
    }
}
