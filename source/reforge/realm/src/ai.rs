//! F5.3: AI mínima de los mobs — el paso de movimiento hacia el jugador
//! (aggro). Funciones PURAS (unit-testables sin PG); el canal las usa en su
//! tick de AI (intervalo fijo) y difunde el `GC_MOVE` a la conexión.
//!
//! Parity del C++ (el subset documentado): el mob aggro se mueve hacia el
//! jugador a `move_speed` UNITS/seg (`m_dwMoveSpeed` del mob_proto) y se
//! detiene al alcanzar su rango de ataque (`wAttackRange`). Fuera del subset
//! (pendiente): patrullaje/estados (`COWARD`/`BERSERK` de `ai_flag`), el
//! ataque del mob (el C++ envía `FUNC_ATTACK` cuando está en rango), de-aggro
//! por distancia.

/// Paso de movimiento: desde `(x,y)` hacia `(tx,ty)` a `speed` UNITS/seg
/// durante `dt_ms`. Devuelve la nueva posición (redondeada). Sin movimiento
/// si ya está en el destino o `speed == 0`; se recorta si el paso cubre la
/// distancia restante.
pub fn step_toward(x: i32, y: i32, tx: i32, ty: i32, speed: i32, dt_ms: u64) -> (i32, i32) {
    let dx = tx - x;
    let dy = ty - y;
    let dist = ((dx * dx + dy * dy) as f64).sqrt();
    let step = speed as f64 * dt_ms as f64 / 1000.0;
    if dist <= f64::EPSILON || step <= f64::EPSILON {
        // Ya en el destino, o speed 0 — SIN movimiento (nunca saltar al
        // destino con speed 0: un mob inmóvil no se teletransporta).
        return (x, y);
    }
    if step >= dist {
        // El paso cubre la distancia restante — llegar al destino.
        return (tx, ty);
    }
    let f = step / dist;
    (x + (dx as f64 * f).round() as i32, y + (dy as f64 * f).round() as i32)
}

/// Rotación del movimiento en pasos de 5 grados (parity `bRot` del
/// `TPacketGCMove` — el C++ manda `GetRotation()/5`, char.cpp:2800).
/// 0 = derecha, 90°/5 = 18 = abajo... (convención atan2 estándar; el
/// cliente interpola la posición destino — la rotación exacta es estética).
pub fn rotation_5deg(x: i32, y: i32, tx: i32, ty: i32) -> u8 {
    let (dx, dy) = ((tx - x) as f64, (ty - y) as f64);
    if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
        return 0;
    }
    let deg = dy.atan2(dx).to_degrees();
    let deg = if deg < 0.0 { deg + 360.0 } else { deg };
    ((deg / 5.0).round() as u32 % 72) as u8
}

/// Daño del ATAQUE del mob (parity `number(damage_min, damage_max)` del
/// C++ — el mob_proto define el rango; el `roll` es el `number()` inclusive
/// inyectado por el canal, los tests uno fijo). El subset NO resta la DEF
/// del jugador (pendiente: la fórmula completa del PC como víctima,
/// `char.cpp:2113-2114`).
pub fn attack_damage(damage_min: i32, damage_max: i32, roll: &mut dyn FnMut(i32, i32) -> i32) -> i32 {
    if damage_max <= damage_min {
        return damage_min;
    }
    damage_min + roll(0, damage_max - damage_min)
}

/// PASO DE PATRULLAJE del mob idle (parity `UpdateState` IDLE —
/// `char_state.cpp:668-688`): con probabilidad `1/7` por tick, el mob elige
/// una dirección aleatoria (0..359°) y un paso de 300-700 UNITS hacia un
/// destino DENTRO del radio de su spawn (el C++ no clampa al spawn pero el
/// estado IDLE lo mantiene cerca; el clamp evita que el subset "pierda"
/// mobs — documentado). `None` = este tick no patrulla.
///
/// El mob con `AIFLAG_NOMOVE` NO patrulla (el caller lo filtra — parity
/// `char_state.cpp:668`: `!IS_SET(dwAIFlag, AIFLAG_NOMOVE)`).
pub fn patrol_step(
    x: i32,
    y: i32,
    home_x: i32,
    home_y: i32,
    spawn_radius: i32,
    roll: &mut dyn FnMut(i32, i32) -> i32,
) -> Option<(i32, i32)> {
    // `!number(0, 6)` — probabilidad 1/7 (char_state.cpp:670).
    if roll(0, 6) != 0 {
        return None;
    }
    // Dirección aleatoria + paso 300-700 (char_state.cpp:672-675).
    let deg = roll(0, 359) as f64;
    let dist = roll(300, 700) as f64;
    let rad = deg.to_radians();
    let (dx, dy) = (dist * rad.cos(), dist * rad.sin());
    let (tx, ty) = (x + dx.round() as i32, y + dy.round() as i32);
    // Clamp al radio del spawn (documentado — el C++ no lo hace; la
    // walkability del mapa queda pendiente, parity parcial).
    let (hx, hy) = ((tx - home_x) as f64, (ty - home_y) as f64);
    let d = (hx * hx + hy * hy).sqrt();
    if d > spawn_radius as f64 && d > f64::EPSILON {
        let f = spawn_radius as f64 / d;
        let (cx, cy) = (home_x + (hx * f).round() as i32, home_y + (hy * f).round() as i32);
        if (cx, cy) == (x, y) {
            return None; // ya en el borde — sin movimiento este tick
        }
        return Some((cx, cy));
    }
    Some((tx, ty))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El paso es exactamente `speed * dt/1000` units hacia el destino.
    #[test]
    fn step_toward_moves_by_speed() {
        // 400 units al este, speed 100, 2000ms -> 200 units.
        let (nx, ny) = step_toward(0, 0, 400, 0, 100, 2000);
        assert_eq!((nx, ny), (200, 0), "100 units/s * 2s = 200");
    }

    /// El paso no sobrepasa el destino (se recorta a la distancia restante).
    #[test]
    fn step_toward_clamps_to_target() {
        let (nx, ny) = step_toward(0, 0, 50, 0, 100, 2000); // paso 200 > 50
        assert_eq!((nx, ny), (50, 0), "se recorta al destino");
    }

    /// Sin movimiento: destino == origen, o speed 0.
    #[test]
    fn step_toward_noop() {
        assert_eq!(step_toward(10, 20, 10, 20, 100, 500), (10, 20));
        assert_eq!(step_toward(0, 0, 100, 0, 0, 500), (0, 0), "speed 0");
    }

    /// Diagonal: el paso normalizado se mueve en la dirección correcta
    /// (45°: 100 units en (100,100) a speed 100, 1000ms -> ~70,70).
    #[test]
    fn step_toward_diagonal() {
        let (nx, ny) = step_toward(0, 0, 100, 100, 100, 1000);
        assert_eq!((nx, ny), (71, 71), "(100/sqrt2) redondeado");
        // La dirección se mantiene (proporción dx:dy ~ 1:1).
        assert_eq!(nx.abs(), ny.abs());
    }

    /// bRot en pasos de 5 grados: este (0°), sur (90° -> 18), oeste (180° ->
    /// 36), norte (270° -> 54).
    #[test]
    fn rotation_5deg_cardinal() {
        assert_eq!(rotation_5deg(0, 0, 100, 0), 0, "este");
        assert_eq!(rotation_5deg(0, 0, 0, 100), 18, "sur");
        assert_eq!(rotation_5deg(0, 0, -100, 0), 36, "oeste");
        assert_eq!(rotation_5deg(0, 0, 0, -100), 54, "norte");
        assert_eq!(rotation_5deg(0, 0, 0, 0), 0, "sin movimiento");
    }

    /// Daño del ataque del mob: `number(damage_min, damage_max)` inclusive.
    #[test]
    fn attack_damage_within_range() {
        let mut roll = |_lo: i32, _hi: i32| 0; // el mínimo del rango
        assert_eq!(attack_damage(3, 8, &mut roll), 3);
        let mut roll = |_lo: i32, hi: i32| hi; // el máximo
        assert_eq!(attack_damage(3, 8, &mut roll), 8);
        // Rango degenerado (min == max): daño fijo sin sorteo.
        let mut roll = |_lo: i32, _hi: i32| panic!("no debe sortear");
        assert_eq!(attack_damage(5, 5, &mut roll), 5);
        // min > max (dato corrupto): defensivo, devuelve el min.
        assert_eq!(attack_damage(9, 4, &mut roll), 9);
    }

    /// Patrullaje: probabilidad 1/7 por tick (parity `!number(0, 6)`).
    #[test]
    fn patrol_step_one_in_seven() {
        // roll devuelve 1..6 → NO patrulla este tick (None).
        let mut roll = |_lo: i32, _hi: i32| 1;
        assert_eq!(patrol_step(100, 100, 100, 100, 1000, &mut roll), None);
        // roll devuelve 0 → patrulla (Some).
        let mut roll = |lo: i32, _hi: i32| lo;
        assert!(patrol_step(100, 100, 100, 100, 1000, &mut roll).is_some());
    }

    /// El destino se mantiene DENTRO del radio del spawn (clamp — el C++ no
    /// lo hace pero el estado IDLE mantiene al mob cerca; documentado).
    #[test]
    fn patrol_step_clamps_to_spawn_radius() {
        // roll: prob=0 (patrulla), deg=0 (este), dist=700 → destino x+700.
        let mut roll = |lo: i32, hi: i32| match (lo, hi) {
            (0, 6) => 0,           // probabilidad 1/7
            (0, 359) => 0,         // 0° → +x
            (300, 700) => 700,     // paso máximo
            _ => panic!("roll inesperado ({lo},{hi})"),
        };
        // Spawn en (0,0), mob en (0,0), radio 500 → el destino (700,0) se
        // clampa a (500,0).
        let (tx, ty) = patrol_step(0, 0, 0, 0, 500, &mut roll).expect("patrulla");
        assert_eq!((tx, ty), (500, 0), "clamp al radio del spawn");
        // Ya en el borde (500,0): el clamp devolvería el mismo punto → None.
        let mut roll = |lo: i32, hi: i32| match (lo, hi) {
            (0, 6) => 0,
            (0, 359) => 0,
            (300, 700) => 700,
            _ => panic!("roll inesperado"),
        };
        assert_eq!(patrol_step(500, 0, 0, 0, 500, &mut roll), None, "en el borde");
    }

    /// Destino DENTRO del radio: se usa tal cual (sin clamp).
    #[test]
    fn patrol_step_keeps_nearby_target() {
        let mut roll = |lo: i32, hi: i32| match (lo, hi) {
            (0, 6) => 0,
            (0, 359) => 0,   // este
            (300, 700) => 300, // paso mínimo
            _ => panic!("roll inesperado ({lo},{hi})"),
        };
        let (tx, ty) = patrol_step(0, 0, 0, 0, 500, &mut roll).expect("patrulla");
        assert_eq!((tx, ty), (300, 0), "dentro del radio: sin clamp");
    }
}
