//! F5.3: AI mínima de los mobs — el paso de movimiento hacia el jugador
//! (aggro). Funciones PURAS (unit-testables sin PG); el canal las usa en su
//! tick de AI (intervalo fijo) y difunde el `GC_MOVE` a la conexión.
//!
//! Parity del C++ (el subset documentado): el mob aggro se mueve hacia el
//! jugador a `move_speed` UNITS/seg (`m_dwMoveSpeed` del mob_proto) y se
//! detiene al alcanzar su rango de ataque (`wAttackRange`). Fuera del subset
//! (pendiente): patrullaje/estados, el ataque del mob (el C++ envía
//! `FUNC_ATTACK` cuando está en rango), de-aggro por distancia.

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
    (
        x + (dx as f64 * f).round() as i32,
        y + (dy as f64 * f).round() as i32,
    )
}

/// Duración del `GC_MOVE` de un paso de mob en ms (parity
/// `CHARACTER::CalculateMoveDuration`, char.cpp:2765-2768: la duración es
/// `(fDist / motionSpeed) × 1000` — el CLIENTE interpola el paso con ESTA
/// duración). Fix 2026-08-13: el AI emitía `duration_ms = dt del tick` (fijo)
/// — los pasos LARGOS (patrulla/chase) se animaban a velocidad altísima
/// ("los mobs se mueven como muy rápido"). Floor 1 ms (un paso instantáneo
/// de 0 u/s no puede durar 0).
pub fn move_duration_ms(dx: i32, dy: i32, move_speed: i32) -> u32 {
    let dist = ((dx * dx + dy * dy) as f64).sqrt();
    let speed = move_speed.max(1) as f64;
    (dist * 1000.0 / speed).max(1.0) as u32
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

/// Daño del ATAQUE del mob contra el jugador (parity `battle_melee_attack` +
/// `CalcBattleDamage` — battle.cpp:199-206): `iAtk = number(damage_min,
/// damage_max)` (el daño del mob_proto ES su ataque), `iDef = DEF_GRADE del
/// PC como víctima` (`char.cpp:2114` — `level + (int)(ht / 1.25)`, subset
/// SIN armadura — el `iArmor` de items es pendiente),
/// `iDam = MAX(0, iAtk - iDef)` y `if (iDam < 3) iDam = number(1, 5)` (el
/// floor del C++ — un golpe bloqueado casi del todo pega 1..5 igual).
pub fn attack_damage(
    damage_min: i32,
    damage_max: i32,
    victim_def: i32,
    roll: &mut dyn FnMut(i32, i32) -> i32,
) -> i32 {
    let atk = if damage_max <= damage_min {
        damage_min
    } else {
        damage_min + roll(0, damage_max - damage_min)
    };
    let mut dam = (atk - victim_def).max(0);
    if dam < 3 {
        dam = roll(1, 5); // parity CalcBattleDamage (battle.cpp:199-206)
    }
    dam
}

/// Cooldown del ATAQUE del mob en ms — parity `CalculateDuration`
/// (utils.cpp:201-210): `i = 100 - iSpd`; `i>0 → 100+i`, `i<0 → 10000/(100-i)`,
/// `i==0 → 100`; `return iDur * i / 100`. El C++ usa `CalculateDuration(
/// GetLimitPoint(POINT_ATT_SPEED), 2000)` (char_state.cpp:1005-1012): con
/// `attack_speed == 100` (default) el cooldown es EXACTAMENTE 2000 ms — el
/// legacy NO golpea a 250 ms como el rewrite hacía (C29).
pub fn mob_attack_cooldown_ms(attack_speed: i32) -> u64 {
    calculate_duration(attack_speed, 2000) as u64
}

/// `CalculateDuration` pura (parity utils.cpp:201-210) — el factor de
/// velocidad/cooldown del C++: `POINT_*` es el FACTOR (100 = neutro), no un
/// valor directo. Reutilizada por el cooldown del ataque (C29) y la
/// velocidad real del mob (C30).
pub fn calculate_duration(i_spd: i32, i_dur: i64) -> i64 {
    let i = 100 - i_spd;
    let i = if i > 0 {
        100_i64 + i as i64
    } else if i < 0 {
        10_000 / (100_i64 - i as i64)
    } else {
        100
    };
    i_dur * i / 100
}

/// Velocidad REAL del mob en u/s — parity `CHARACTER::GetMoveSpeed`
/// (char.cpp:2751-2754): `GetMoveMotionSpeed() * 10000 / CalculateDuration(
/// POINT_MOV_SPEED, 10000)`. La columna `move_speed` del mob_proto es el
/// FACTOR (POINT_MOV_SPEED, 100 = neutro); la velocidad sale de la ANIMACIÓN
/// (motion RUN de la raza — C30). Sin los `.msa`/`.granny` en el rewrite,
/// `MOTION_SPEED` es la constante aproximada (~300 u/s, el valor típico del
/// motion de las razas legacy — GAP: tabla por raza cuando exista). Con
/// factor 100 → EXACTAMENTE 300 u/s (el rewrite usaba la columna como u/s →
/// mobs ~3× más lentos, C30).
pub fn mob_move_speed(move_speed: i32) -> u32 {
    const MOTION_SPEED: u32 = 300; // u/s del motion RUN legacy (~300)
    let dur = calculate_duration(move_speed, 10_000);
    (MOTION_SPEED * 10_000 / dur.max(1) as u32).max(1)
}

/// `IsAggressive()` parity (`char_state.cpp:224-226` — `AIFLAG_AGGRESSIVE`):
/// el mob ataca PROACTIVAMENTE a quien entra en su `aggressive_sight`. El
/// `ai_flag` del PG es el SET legacy migrado a TEXTO (p.ej. "AGGR,COWARD" —
/// legacy-schema.md §4.6): contiene "AGGR".
pub fn is_aggressive(ai_flag: Option<&str>) -> bool {
    ai_flag.is_some_and(|f| f.split(',').any(|t| t.trim() == "AGGR"))
}

/// `IsCoward()`/`IsBerserker()` parity (char_state.cpp:234-247): COWARD
/// huye con HP bajo (StateBattle:870-886); BERSERK gate del ×2 (1015-1018).
fn has_aiflag(ai_flag: Option<&str>, flag: &str) -> bool {
    ai_flag.is_some_and(|f| f.split(',').any(|t| t.trim() == flag))
}
pub fn is_coward(ai_flag: Option<&str>) -> bool {
    has_aiflag(ai_flag, "COWARD")
}
pub fn is_berserker(ai_flag: Option<&str>) -> bool {
    has_aiflag(ai_flag, "BERSERK")
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
        let (cx, cy) = (
            home_x + (hx * f).round() as i32,
            home_y + (hy * f).round() as i32,
        );
        if (cx, cy) == (x, y) {
            return None; // ya en el borde — sin movimiento este tick
        }
        return Some((cx, cy));
    }
    Some((tx, ty))
}

/// C32 (change-attack-position — parity `Follow` + `IsChangeAttackPosition`,
/// char.cpp:5436-5462, 5869-5881): el destino LATERAL del mob en persecución
/// — un punto a `fMinDistance` de la VÍCTIMA en un ángulo aleatorio. Si el
/// mob está CERCA de la víctima (< 500 u) el ángulo es la dirección hacia la
/// víctima ± 2 tiradas `number(-90,90)` (≈ ±180° — el C++ las suma; la
/// dirección la da `GetDegreeFromPositionXY(x,y,GetX(),GetY())` sobre el
/// vector víctima→mob); si está LEJOS (≥ 500) el ángulo es `number(0,359)`
/// (completamente aleatorio, char.cpp:5444-5448). `fMinDistance` = rango×0.9
/// melee / rango×0.8 RANGE/MAGIC (`__CHARACTER_GotoNearTarget`, char_state.cpp:
/// 694-714). El C++ valida ATTR_BLOCK|ATTR_OBJECT con 16 retries — la
/// walkability del AI queda pendiente (GAP documentado); aquí el retry solo
/// repite el ángulo (sin mapa). Devuelve el destino en coords del mundo.
pub fn change_attack_dest(
    mob_x: i32,
    mob_y: i32,
    victim_x: i32,
    victim_y: i32,
    battle_type: u8,
    attack_range: u32,
    roll: &mut dyn FnMut(i32, i32) -> i32,
) -> (i32, i32) {
    let dist = ((mob_x - victim_x).pow(2) as f64 + (mob_y - victim_y).pow(2) as f64).sqrt();
    let f_min = if matches!(
        battle_type,
        crate::combat::BATTLE_TYPE_RANGE | crate::combat::BATTLE_TYPE_MAGIC
    ) {
        attack_range as f64 * 0.8
    } else {
        attack_range as f64 * 0.9
    };
    // Dirección víctima→mob (parity GetDegreeFromPositionXY — 0° = +Y
    // horario; GetDeltaByDegree usa sin/cos igual que aquí).
    let deg = if dist < 500.0 {
        let dx = (mob_x - victim_x) as f64;
        let dy = (mob_y - victim_y) as f64;
        let base = if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
            0.0
        } else {
            let d = dx.hypot(dy);
            let mut deg = (dy / d).acos().to_degrees();
            if dx < 0.0 {
                deg = 360.0 - deg;
            }
            deg
        };
        // (rot + number(-90,90) + number(-90,90)) % 360 — char.cpp:5443
        let a = roll(-90, 90) as f64;
        let b = roll(-90, 90) as f64;
        (base + a + b).rem_euclid(360.0)
    } else {
        roll(0, 359) as f64
    };
    let rad = deg.to_radians();
    let (fx, fy) = (f_min * rad.sin(), f_min * rad.cos());
    (victim_x + fx.round() as i32, victim_y + fy.round() as i32)
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

    /// Daño del ataque del mob: `number(damage_min, damage_max)` − DEF de la
    /// víctima, floor 1..5 si < 3 (parity battle.cpp:199-206).
    #[test]
    fn attack_damage_within_range() {
        // DEF 0: el daño es `number(min,max)` (el sorteo del rango).
        let mut roll = |_lo: i32, _hi: i32| 0; // el mínimo del rango
        assert_eq!(attack_damage(3, 8, 0, &mut roll), 3);
        let mut roll = |lo: i32, hi: i32| match (lo, hi) {
            (0, 5) => 5, // sorteo del rango (3..8 → +5 = 8)
            (1, 5) => 1, // floor (no aplica aquí)
            _ => panic!("roll inesperado ({lo},{hi})"),
        };
        assert_eq!(attack_damage(3, 8, 0, &mut roll), 8);
        // Rango degenerado (min == max): daño fijo sin sorteo.
        let mut roll = |_lo: i32, _hi: i32| panic!("no debe sortear");
        assert_eq!(attack_damage(5, 5, 0, &mut roll), 5);
        // min > max (dato corrupto): defensivo, usa el min.
        assert_eq!(attack_damage(9, 4, 0, &mut roll), 9);
    }

    /// La DEF de la víctima se resta (MAX(0, atk − def)) y el floor del C++
    /// pega 1..5 si el resultado es < 3 (parity CalcBattleDamage).
    #[test]
    fn attack_damage_subtracts_def_and_floors() {
        // atk 3..8 − def 5 → 0..3; con sorteo 8 → 3 (≥ 3, sin floor).
        let mut roll = |lo: i32, hi: i32| match (lo, hi) {
            (0, 5) => 5, // sorteo del rango → atk 8
            (1, 5) => panic!("no floor (dam 3)"),
            _ => panic!("roll inesperado ({lo},{hi})"),
        };
        assert_eq!(attack_damage(3, 8, 5, &mut roll), 3, "8 − 5 = 3");
        // atk 3 − def 5 → 0 → floor number(1,5) = 2.
        let mut roll = |lo: i32, hi: i32| match (lo, hi) {
            (0, 5) => 0, // sorteo → atk 3
            (1, 5) => 2, // floor
            _ => panic!("roll inesperado ({lo},{hi})"),
        };
        assert_eq!(
            attack_damage(3, 8, 5, &mut roll),
            2,
            "MAX(0,3−5)=0 → floor 2"
        );
        // atk 4 − def 5 → 0 → floor 5.
        let mut roll = |lo: i32, hi: i32| match (lo, hi) {
            (0, 5) => 1, // sorteo → atk 4
            (1, 5) => 5, // floor
            _ => panic!("roll inesperado ({lo},{hi})"),
        };
        assert_eq!(attack_damage(3, 8, 5, &mut roll), 5, "floor number(1,5)");
    }

    /// `is_aggressive`: parity AIFLAG_AGGRESSIVE — el SET legacy del PG
    /// como texto; "AGGR" en cualquier posición del SET.
    #[test]
    fn is_aggressive_matches_cpp_flag() {
        assert!(is_aggressive(Some("AGGR")));
        assert!(is_aggressive(Some("AGGR,COWARD")));
        assert!(is_aggressive(Some("COWARD,AGGR")));
        assert!(!is_aggressive(Some("COWARD")));
        assert!(!is_aggressive(Some("NOMOVE")));
        assert!(!is_aggressive(None));
        assert!(!is_aggressive(Some("")));
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
            (0, 6) => 0,       // probabilidad 1/7
            (0, 359) => 0,     // 0° → +x
            (300, 700) => 700, // paso máximo
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
        assert_eq!(
            patrol_step(500, 0, 0, 0, 500, &mut roll),
            None,
            "en el borde"
        );
    }

    /// Destino DENTRO del radio: se usa tal cual (sin clamp).
    #[test]
    fn patrol_step_keeps_nearby_target() {
        let mut roll = |lo: i32, hi: i32| match (lo, hi) {
            (0, 6) => 0,
            (0, 359) => 0,     // este
            (300, 700) => 300, // paso mínimo
            _ => panic!("roll inesperado ({lo},{hi})"),
        };
        let (tx, ty) = patrol_step(0, 0, 0, 0, 500, &mut roll).expect("patrulla");
        assert_eq!((tx, ty), (300, 0), "dentro del radio: sin clamp");
    }

    /// move_duration_ms: la duración del GC_MOVE = dist/move_speed × 1000
    /// (parity CalculateMoveDuration, char.cpp:2765-2768). Fix 2026-08-13:
    /// con el dt del tick fijo, los pasos largos del patrulla/chase se
    /// animaban "muy rápido" en el cliente (interpola con ESTA duración).
    #[test]
    fn move_duration_is_dist_over_speed() {
        assert_eq!(move_duration_ms(50, 0, 100), 500, "50 u a 100 u/s = 500 ms");
        assert_eq!(move_duration_ms(0, 50, 100), 500, "dy también");
        assert_eq!(
            move_duration_ms(500, 0, 100),
            5_000,
            "paso largo del patrulla"
        );
        assert_eq!(
            move_duration_ms(50, 0, 300),
            166,
            "50×1000/300 = 166.67 — el C++ TRUNCA ((int) fDist/motionSpeed×1000, char.cpp:2767)"
        );
        assert_eq!(move_duration_ms(0, 0, 0), 1, "floor 1 ms (nunca 0)");
    }

    /// C29: cooldown del ataque del mob — parity `CalculateDuration`
    /// (utils.cpp:201-210). El rewrite atacaba cada tick (250 ms); el legacy
    /// golpea cada `CalculateDuration(POINT_ATT_SPEED, 2000)` (char_state.cpp:
    /// 1005-1012): attack_speed 100 → EXACTAMENTE 2000 ms.
    #[test]
    fn mob_attack_cooldown_parity() {
        // i = 100 - 100 = 0 → i = 100 → 2000 × 100 / 100 = 2000 ms
        assert_eq!(mob_attack_cooldown_ms(100), 2_000, "default attack_speed");
        // i = 100 - 200 = -100 → i = 10000/(100+100) = 50 → 2000×50/100 = 1000
        assert_eq!(
            mob_attack_cooldown_ms(200),
            1_000,
            "doble velocidad → mitad"
        );
        // i = 100 - 50 = 50 → i = 150 → 2000×150/100 = 3000
        assert_eq!(mob_attack_cooldown_ms(50), 3_000, "media velocidad → 1.5×");
        // i = 100 - 80 = 20 → i = 120 → 2000×120/100 = 2400
        assert_eq!(mob_attack_cooldown_ms(80), 2_400, "80 = 20% más lento");
    }

    /// C30: la velocidad REAL del mob = motion(300) × 10000/CalculateDuration
    /// (char.cpp:2751-2754). La columna es el FACTOR: 100 → 300 u/s exactos
    /// (el rewrite usaba la columna como u/s → ~3× más lento, "se mueven
    /// raro"); 0/1 (308 mobs del PG) ya no congelan (el C++ los mueve a
    /// ~mitad de velocidad); 200 → 600 u/s.
    #[test]
    fn mob_move_speed_is_motion_times_factor() {
        assert_eq!(mob_move_speed(100), 300, "factor neutro → motion");
        // CalculateDuration(50, 10000): i=150 → 15000 → 300×10000/15000 = 200
        assert_eq!(mob_move_speed(50), 200, "más lento");
        // CalculateDuration(200, 10000): i=50 → 5000 → 300×10000/5000 = 600
        assert_eq!(mob_move_speed(200), 600, "2× la velocidad");
        // CalculateDuration(0, 10000): i=200 → 20000 → 300×10000/20000 = 150
        assert_eq!(mob_move_speed(0), 150, "factor 0 no congela (parity)");
        assert_eq!(mob_move_speed(1), 150, "factor 1 ≈ 150 u/s (parity)");
    }

    /// C32: el destino del change-attack-position (char.cpp:5436-5462) — un
    /// punto a `fMinDistance` (rango×0.9 melee / ×0.8 rango) de la víctima en
    /// ángulo aleatorio (±180° alrededor de la dirección a la víctima si
    /// cerca; 0..359 si lejos).
    #[test]
    fn change_attack_dest_lands_on_the_ring() {
        use crate::combat::{BATTLE_TYPE_MAGIC, BATTLE_TYPE_MELEE, BATTLE_TYPE_RANGE};
        // Mob cerca (100 u < 500): dirección hacia la víctima ± 2×number(-90,90).
        // roll fijo 0 → ángulo base exacto: mob (100,0) sobre víctima (0,0) →
        // dx=100, dy=0 → acos(0)=90°; x>0 → 90°. Con roll 0: 90+0+0 = 90°.
        // GetDeltaByDegree(90°, 157.5) = (157.5×sin90, 157.5×cos90) = (157.5, 0)
        // → destino (157, 0). (fMin melee = 175×0.9 = 157.5 → round 158.)
        let (x, y) = change_attack_dest(100, 0, 0, 0, BATTLE_TYPE_MELEE, 175, &mut |lo, hi| {
            assert!(
                (-90..=90).contains(&lo) || (0..=359).contains(&lo),
                "roll({lo},{hi})"
            );
            0 // desvío 0
        });
        assert_eq!(
            (x, y),
            (158, 0),
            "a 157.5 u de la víctima en la dirección del mob"
        );
        // RANGE: fMin = 175×0.8 = 140.
        let (x, y) = change_attack_dest(100, 0, 0, 0, BATTLE_TYPE_RANGE, 175, &mut |_lo, _hi| 0);
        assert_eq!((x, y), (140, 0), "fMin rango = ×0.8");
        // Lejos (600 > 500): ángulo completamente aleatorio (roll 90 → 90°).
        let (x, y) = change_attack_dest(600, 0, 0, 0, BATTLE_TYPE_MAGIC, 175, &mut |lo, hi| {
            assert_eq!((lo, hi), (0, 359), "lejos: número(0,359)");
            90
        });
        assert_eq!((x, y), (140, 0), "(0,0)+140×(sin90,cos90) = (140, 0)");
    }
}
