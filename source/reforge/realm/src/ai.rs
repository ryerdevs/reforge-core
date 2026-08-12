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
}
