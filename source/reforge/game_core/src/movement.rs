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
    /// Ancla del reloj del server al ENTRAR al mundo (parity
    /// `DESC::m_dwClientTime` — el reloj del server en el handshake del
    /// canal, desc.cpp:714). El gate del speedhack (input_main.cpp:1496) y el
    /// `iServerDelta` (input_main.cpp:1501) se miden desde ESTE ancla, no
    /// desde el último MOVE: el check solo se activa 7 s después de anclar y
    /// su umbral crece con la vida de la conexión (el reloj del cliente queda
    /// anclado al AUTH desde 2026-08-14 — canal sin handshake — y el desfase
    /// de arranque entre procesos queda DENTRO del umbral).
    pub anchor_server_time: u32,
    /// Montura (afecta la tolerancia de distancia: 25 m vs 60 m).
    pub riding: bool,
    /// Velocidad efectiva del movimiento en UNITS/segundo (F5.4 — el
    /// envelope por entidad). Base del jugador: 300 u/s — el fallback del C++
    /// cuando el motion no existe (`char.cpp:2747`) con POINT_MOV_SPEED=100
    /// (factor 1.0 — `CalculateDuration`, `utils.cpp:201-213`). Cuando haya
    /// buffs/monturas con velocidad (F5): `300 * 10000 /
    /// CalculateDuration(POINT_MOV_SPEED, 10000)`.
    pub speed: u32,
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
    /// `fDist > 6000` (60 m — the effective unmounted and mounted cap) —
    /// salto/teleport. The legacy unmounted source threshold is 2500
    /// (`ENABLE_TP_SPEED_CHECK`, input_main.cpp:1466), but the rewrite pins
    /// both modes to 6000.
    TooFar,
    /// F5.4 (ADR-0011): la distancia excede el ENVELOPE por entidad —
    /// `speed × Δt` de server desde el último MOVE aceptado (+20% + 100 ms de
    /// tolerancia de lag, plan §5.7). Cierra el hueco del slow-accumulate:
    /// pasos cortos con relojes de cliente plausibles que superan la
    /// velocidad real de media.
    ExceedsEnvelope,
    /// `bFunc` inválido (`>= FUNC_MAX_NUM && !(bFunc & 0x80)`,
    /// input_main.cpp:1444).
    InvalidFunc,
    /// El MOVE no es de movimiento (FUNC_ATTACK/COMBO/SKILL — el procesamiento
    /// de acciones es F5).
    NotMove,
}

/// Distancia máxima por MOVE sin montura: 60 m = 6000 units
/// (el umbral del `ENABLE_TP_SPEED_CHECK`, input_main.cpp:1466, era 25 m —
/// comentado en el source; ampliado 2026-08-13: 40 m → 60 m — los MOVEs
/// espaciados del cliente CORRIENDO (>450 u/s) superaban 4000; 60 m sigue
/// siendo el anti-teleport, defensa nuestra).
const MAX_DIST_NO_RIDING: i128 = 6000;
/// Con montura: 60 m = 6000 units (`input_main.cpp:1466`).
const MAX_DIST_RIDING: i128 = 6000;
/// `iDelta >= 30000` → slow timer (input_main.cpp:1505).
const SLOW_TIMER_MS: i64 = 30_000;

/// Velocidad base del jugador en units/s (F5.4): el fallback del C++ es 300
/// (`char.cpp:2747`; POINT_MOV_SPEED=100 → factor 1.0, `CalculateDuration`,
/// utils.cpp:201-213) — la base del ENVELOPE se ajusta a 500 (2026-08-13): el
/// cliente real CORRE a >450 u/s y con base 300 el margen 1.8× (540 u/s)
/// quedaba justo (la carrera se rechazaba: 28 envelope + 209 teleport en la
/// sesión 231453); con 500 el margen es 900 u/s — la carrera legítima entra
/// holgada y el speedhack sostenido >900 u/s sigue acotado (cap + timers).
pub const DEFAULT_MOVE_SPEED: u32 = 500;
/// Tolerancia de lag del envelope: +80% (plan §5.7 pedía +20%; Ajuste
/// 2026-08-13 cliente real: el patrón de MOVEs del cliente (ráfagas +
/// cambios de dirección + corridas) excedía el 20% → la posición server
/// avanzaba a saltos y los spawns tardaban minutos en materializarse. El
/// 50% sigue acotando el speedhack sostenido (>450 u/s) — el cap absoluto
/// 6000 y los timers del C++ siguen activos; el auto-ban por N violaciones queda
/// pendiente (ADR-0011 follow-up).
const ENVELOPE_TOLERANCE: f64 = 1.80;
/// Tolerancia de lag del envelope: +250 ms de tiempo de server (plan §5.7).
const ENVELOPE_LAG_MS: f64 = 250.0;

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
    // PARITY EXACTA (FIX 2026-08-14 — "al moverte me saca al login"):
    //   - El GATE `CheckSpeedHack` (input_main.cpp:1496): el check SOLO corre
    //     tras 7 s del ancla del reloj (`dwCurTime - GetClientTime() > 7000`).
    //     El ancla = el reloj del server al ENTRAR al mundo (equiv. del
    //     handshake del canal, desc.cpp:714 — `m_dwClientTime` se setea UNA
    //     vez, no por MOVE).
    //   - `iServerDelta` (input_main.cpp:1501) = `dwCurTime - GetClientTime()`
    //     — el tiempo DESDE EL ANCLA (crece con la vida de la conexión), NO el
    //     intervalo entre MOVEs. El rewrite lo medía entre MOVEs (33-200 ms →
    //     umbral -1..-4 ms): con el reloj del cliente anclado al AUTH (canal
    //     sin handshake desde 2026-08-14) el desfase de arranque auth/canal
    //     (~100 ms) disparaba FastTimer en el primer MOVE → kick al login.
    //   - `iDelta` (input_main.cpp:1503) = `dwCurTime - pinfo->dwTime` — el
    //     C++ castea la resta u32 a `int` (el wrap del reloj del cliente se
    //     interpreta CON SIGNO, parity).
    let i_delta = i64::from(now_ms.wrapping_sub(packet.dw_time) as i32);
    let since_anchor = i64::from(now_ms.wrapping_sub(state.anchor_server_time) as i32);
    if since_anchor > 7_000 {
        let server_delta = since_anchor;
        if i_delta >= SLOW_TIMER_MS {
            return Err(MoveError::SlowTimer);
        }
        if i_delta < -(server_delta / 50) {
            return Err(MoveError::FastTimer);
        }
    }

    // Distancia (anti-teleport — el umbral del ENABLE_TP_SPEED_CHECK, comentado
    // en el source pero implementado como defensa con la misma tolerancia).
    // Widen before subtracting and squaring: the full i32 coordinate range is
    // valid on the wire, but its squared delta does not fit in i64.
    let dx = i128::from(packet.x) - i128::from(state.x);
    let dy = i128::from(packet.y) - i128::from(state.y);
    let dist_sq = dx * dx + dy * dy;
    let max_dist = if state.riding {
        MAX_DIST_RIDING
    } else {
        MAX_DIST_NO_RIDING
    };
    if dist_sq > max_dist * max_dist {
        return Err(MoveError::TooFar);
    }

    // F5.4 (ADR-0011): envelope por entidad — la distancia NO puede exceder
    // `speed × Δt` desde el último MOVE aceptado (tolerancia de lag
    // +20%/+100 ms — plan §5.7: "server owns the position; correction not
    // ban"). Sin ancla (`last_server_time == 0` — primer MOVE tras load/warp)
    // el envelope está inerte: el cap absoluto (6000) sigue validando.
    // Cierra el hueco del slow-accumulate: el timer del cliente pasa con
    // relojes plausibles y el cap con pasos cortos — pero la distancia media
    // no puede superar la velocidad real del personaje.
    //
    // FIX 2026-08-13 (cliente real): el Δt se mide con el MÁXIMO del reloj
    // del cliente (`dw_time` — el intervalo REAL del paseo del cliente) y el
    // reloj del server (cubre el lag de red y el cliente rezagado). El Δt de
    // SOLO server rechazaba el caminar legítimo: el cliente manda los MOVEs
    // en ráfagas (varios paquetes en pocos ms) → el intervalo server entre
    // llegadas era diminuto → el margen permitido minúsculo → TODO paso se
    // rechazaba y la posición quedaba congelada (síntoma real: "el server
    // dice que no me moví del spawn" — los spawns no se materializan).
    if state.last_server_time != 0 && state.last_client_time != 0 {
        let server_dt = i64::from(now_ms.wrapping_sub(state.last_server_time) as i32).max(0) as f64;
        let client_dt =
            i64::from(packet.dw_time.wrapping_sub(state.last_client_time) as i32).max(0) as f64;
        let dt_ms = server_dt.max(client_dt);
        let allowed =
            f64::from(state.speed) * (dt_ms + ENVELOPE_LAG_MS) / 1000.0 * ENVELOPE_TOLERANCE;
        if (dist_sq as f64).sqrt() > allowed {
            return Err(MoveError::ExceedsEnvelope);
        }
    }

    // Aceptado (parity `Goto(lX, lY)` — input_main.cpp:1532).
    state.x = packet.x;
    state.y = packet.y;
    state.last_client_time = packet.dw_time;
    state.last_server_time = now_ms;
    Ok(MoveResult {
        x: packet.x,
        y: packet.y,
    })
}

/// Estado inicial desde una posición cargada del player (el primer MOVE tiene
/// `last_server_time = now_ms` — sin `iServerDelta` previo). El `anchor` (el
/// reloj del server al entrar al mundo — parity `m_dwClientTime`) lo setea el
/// caller con `anchor_server_time = now` (entry/warp): con 0, el gate de 7 s
/// está cerrado y el timer check inerte.
pub fn initial(x: i32, y: i32) -> PlayerMotion {
    PlayerMotion {
        x,
        y,
        last_client_time: 0,
        last_server_time: 0,
        anchor_server_time: 0,
        riding: false,
        speed: DEFAULT_MOVE_SPEED,
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

    /// El cap de distancia (60 m = 6000 units por MOVE sin montura — el
    /// anti-teleport del 2026-08-13, ampliado para el correr real del
    /// cliente): mover dentro del límite actualiza la posición; un salto
    /// mayor se rechaza (TooFar) sin tocar la posición — parity del
    /// anti-teleport del C++ (input_main.cpp:1466, define comentado).
    #[test]
    fn envelope_accepts_within_limit_and_rejects_teleport() {
        let mut st = initial(0, 0);
        // 2000 units (< 6000 — MAX_DIST_NO_RIDING) → OK (sin ancla aún — el
        // envelope está inerte, el cap absoluto valida; F5.4).
        let r = process_move(&mut st, &move_to(2000, 0, 1000), 1100).expect("dentro del límite");
        assert_eq!(r, MoveResult { x: 2000, y: 0 });
        assert_eq!((st.x, st.y), (2000, 0), "posición actualizada");
        // dx = 7000 (> 6000) → TooFar; la posición NO cambia.
        let err = process_move(&mut st, &move_to(9000, 0, 2000), 2100).expect_err("teleport");
        assert_eq!(err, MoveError::TooFar);
        assert_eq!((st.x, st.y), (2000, 0), "el MOVE rechazado no actualiza");
        // Diagonal dx=5000, dy=4000 → sqrt(41M) ≈ 6403 > 6000 → rechazo.
        let err = process_move(&mut st, &move_to(7000, 4000, 3000), 3100).expect_err("diagonal");
        assert_eq!(err, MoveError::TooFar);
        // Con montura: cap 6000; paso de 1000 u en 3000 ms → dentro del
        // envelope (allowed = 500×3.25×1.8 = 2925) → OK.
        st.riding = true;
        let r = process_move(&mut st, &move_to(3000, 0, 4000), 4100).expect("montura");
        assert_eq!(r.x, 3000);
    }

    /// El timer speedhack (la validación ACTIVA — input_main.cpp:1505-1515):
    /// el reloj del cliente 30s+ atrasado → SlowTimer; muy adelantado →
    /// FastTimer. El gate de 7 s del C++ (input_main.cpp:1496) se modela con
    /// el ancla (el check corre: now - anchor > 7000).
    #[test]
    fn speedhack_timer_rejects() {
        let mut st = initial(0, 0);
        // El primer MOVE: last_server_time=0 → iServerDelta = now - 0 (grande)
        // — el C++ arranca con GetClientTime() inicializado; para el primer
        // MOVE usamos un estado con el reloj ya anclado (parity: el cliente
        // manda el time del handshake ~= el del server). El ancla 10 s atrás
        // abre el gate (now 60 s - anchor 10 s = 50 s > 7 s).
        st.last_server_time = 10_000;
        st.last_client_time = 10_000;
        st.anchor_server_time = 10_000;
        // Slow timer: el dwTime del paquete 40s atrás → iDelta = 65000 >= 30000.
        // (el reloj del cliente wrappea — parity del wire u32).
        let slow_time = 10_000u32.wrapping_sub(40_000);
        let err =
            process_move(&mut st, &move_to(100, 0, slow_time), 60_000).expect_err("slow timer");
        assert_eq!(err, MoveError::SlowTimer);
        // Fast timer: el dwTime 10s en el futuro con iServerDelta=50s →
        // iDelta = -9900 < -(50000/50) → FastTimer.
        let err = process_move(&mut st, &move_to(100, 0, 70_000), 60_100).expect_err("fast timer");
        assert_eq!(err, MoveError::FastTimer);
        // El reloj razonable pasa.
        let r = process_move(&mut st, &move_to(100, 0, 60_000), 60_100).expect("reloj OK");
        assert_eq!(r.x, 100);
    }

    /// El GATE del C++ (input_main.cpp:1496) — FIX 2026-08-14 ("al moverte me
    /// saca al login"): el check solo corre tras 7 s del ancla (el reloj del
    /// server al entrar al mundo — equiv. del handshake del canal). Con el
    /// reloj del cliente anclado al AUTH (canal sin handshake desde
    /// 2026-08-14), el desfase de arranque auth/canal (~100 ms) queda DENTRO
    /// del umbral `-(iServerDelta/50)` — que crece con la vida de la
    /// conexión — mientras un reloj realmente adelantado sigue disparando.
    #[test]
    fn timer_gate_opens_after_7s_and_tolerates_clock_skew() {
        // Gate CERRADO (ancla = la entrada, ahora - ancla = 5 s < 7 s): un
        // paquete con el reloj 35 s adelantado NO se chequea (parity: los
        // primeros 7 s sin check).
        let mut st = initial(0, 0);
        st.anchor_server_time = 50_000;
        let r = process_move(&mut st, &move_to(100, 0, 90_000), 55_000).expect("gate cerrado");
        assert_eq!(r.x, 100);
        // Gate ABIERTO (ahora - ancla = 20 s): el desfase constante de ~100 ms
        // del cliente (reloj anclado al auth) PASA (umbral -(20000/50) = -400).
        st.anchor_server_time = 40_000;
        st.last_client_time = 60_000;
        st.last_server_time = 60_000;
        let r = process_move(&mut st, &move_to(200, 0, 59_900), 60_000).expect("desfase 100 ms OK");
        assert_eq!(r.x, 200);
        // Un reloj realmente adelantado (30 s) SÍ dispara.
        let err = process_move(&mut st, &move_to(300, 0, 90_000), 60_100).expect_err("fast real");
        assert_eq!(err, MoveError::FastTimer);
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

    /// Estado con ancla para los tests del envelope (parity del arranque:
    /// el primer MOVE real ancla el reloj — `last_server_time != 0`).
    fn anchored(x: i32, y: i32, now: u32) -> PlayerMotion {
        let mut st = initial(x, y);
        st.last_server_time = now;
        st.last_client_time = now;
        st
    }

    /// F5.4: el slow-accumulate se rechaza — pasos de 30 u cada 100 ms pasan
    /// (500 u/s de la base del envelope + tolerancia), pero un paso de 600 u
    /// en 100 ms excede el envelope (`allowed = 500*0.35*1.8 = 315 u`) aunque
    /// el reloj del cliente sea plausible y la distancia esté MUY por debajo
    /// del cap. La posición NO cambia (corrección, no ban — plan §5.7).
    #[test]
    fn envelope_rejects_slow_accumulate() {
        let mut st = anchored(0, 0, 10_000);
        // Pasos normales: 30 u cada 100 ms → Ok (dentro de 315 u de allowed).
        for i in 0..5 {
            let now = 10_100u32 + i as u32 * 100;
            let r =
                process_move(&mut st, &move_to((i + 1) * 30, 0, now), now).expect("paso normal");
            assert_eq!(r.x, (i + 1) * 30);
        }
        // Paso de 600 u en 100 ms (12× la velocidad base) → envelope.
        let err = process_move(&mut st, &move_to(750, 0, 10_700), 10_700).expect_err("aceleración");
        assert_eq!(err, MoveError::ExceedsEnvelope);
        assert_eq!((st.x, st.y), (150, 0), "el MOVE rechazado no actualiza");
    }

    /// F5.4: tolerancia de lag — un cliente con 2 s de lag manda la posición
    /// acumulada: 600 u en 2000 ms → allowed = 500*2.25*1.8 = 2025 → Ok; el
    /// límite duro (2500 u) se rechaza.
    #[test]
    fn envelope_lag_tolerance_passes() {
        let mut st = anchored(0, 0, 10_000);
        let r = process_move(&mut st, &move_to(600, 0, 8_000), 12_000).expect("lag 2 s: 600 u");
        assert_eq!(r.x, 600);
        let err = process_move(&mut st, &move_to(3_100, 0, 8_000), 12_000)
            .expect_err("fuera de la tolerancia");
        assert_eq!(err, MoveError::ExceedsEnvelope);
        // El reloj del cliente atrasado (dw_time 2 s antes) sigue pasando el
        // timer speedhack (iDelta = 4000 < 30000, no-fast) — el envelope es
        // quien acota la distancia.
        assert_eq!((st.x, st.y), (600, 0));
    }

    /// F5.4: sin ancla (`last_server_time == 0` — primer MOVE tras load o
    /// warp) el envelope está inerte; el cap absoluto (6000) sigue validando
    /// el primer salto. El caller re-ancla con `initial()` tras un warp.
    #[test]
    fn envelope_inert_without_anchor() {
        let mut st = initial(0, 0);
        let r = process_move(&mut st, &move_to(2_000, 0, 1_000), 1_100).expect("primer MOVE");
        assert_eq!(r.x, 2_000);
        // Con ancla ya puesta (last_server_time = 1100), el mismo salto de
        // 2000 u en 100 ms se rechaza.
        let err = process_move(&mut st, &move_to(4_000, 0, 1_200), 1_200).expect_err("anclado");
        assert_eq!(err, MoveError::ExceedsEnvelope);
    }

    /// G0.1b: 6000 units is the inclusive absolute limit for both movement
    /// modes; the next unit is a teleport. The first MOVE keeps the envelope
    /// inert so this isolates the absolute cap.
    #[test]
    fn absolute_distance_limit_is_exactly_6000_for_both_modes() {
        for riding in [false, true] {
            let mut at_max = initial(0, 0);
            at_max.riding = riding;
            assert_eq!(
                process_move(&mut at_max, &move_to(6_000, 0, 0), 0),
                Ok(MoveResult { x: 6_000, y: 0 })
            );

            let mut over_max = initial(0, 0);
            over_max.riding = riding;
            assert_eq!(
                process_move(&mut over_max, &move_to(6_001, 0, 0), 0),
                Err(MoveError::TooFar)
            );
            assert_eq!((over_max.x, over_max.y), (0, 0));
        }
    }

    /// Wire coordinates are signed i32 values. Full-range deltas must be
    /// rejected as TooFar, not wrap a squared i64 distance or panic.
    #[test]
    fn absolute_distance_rejects_full_range_coordinates_without_overflow() {
        let mut st = initial(i32::MIN, i32::MIN);
        let err = process_move(&mut st, &move_to(i32::MAX, i32::MAX, 0), 0)
            .expect_err("full-range coordinate jump");

        assert_eq!(err, MoveError::TooFar);
        assert_eq!((st.x, st.y), (i32::MIN, i32::MIN));
    }
}
