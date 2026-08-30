//! CABALLO (slice horse): dominio puro del sistema de montura del Metin2
//! clÃ¡sico â€” nivel y salud del caballo del jugador (parity horse_rider).

/// Nivel mÃ¡ximo de un caballo (parity `HORSE_MAX_LEVEL`, horse_rider.h:12).
pub const HORSE_MAX_LEVEL: u8 = 30;

/// Caballo del jugador: nivel 0..=30 y salud 0..=mÃ¡ximo del nivel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Horse {
    pub level: u8,
    pub health: u16,
}

/// Salud mÃ¡xima por nivel â€” columna `iMaxHealth` de `c_aHorseStat`
/// (parity horse_rider.cpp:18-48), niveles 0..=30.
const MAX_HEALTH: [u16; 31] = [
    1, 3, 4, 5, 7, 8, 9, 11, 12, 13, 15, 18, 19, 21, 22, 24, 25, 27, 28, 30, 32, 35, 36, 37, 38,
    40, 42, 44, 46, 48, 50,
];

/// Stamina mÃ¡xima por nivel â€” columna `iMaxStamina` de `c_aHorseStat`
/// (parity horse_rider.cpp:18-48), niveles 0..=30.
const MAX_STAMINA: [u16; 31] = [
    1, 4, 4, 5, 5, 6, 6, 7, 7, 8, 10, 30, 35, 40, 50, 55, 60, 65, 70, 80, 100, 120, 125, 130, 135,
    140, 145, 150, 160, 170, 200,
];

/// Salud mÃ¡xima del nivel (parity `GetHorseMaxHealth`, horse_rider.cpp:113-117).
pub fn max_health(level: u8) -> u16 {
    MAX_HEALTH[level.min(HORSE_MAX_LEVEL) as usize]
}

/// Stamina mÃ¡xima del nivel (parity `GetHorseMaxStamina`, horse_rider.cpp:118-122).
pub fn max_stamina(level: u8) -> u16 {
    MAX_STAMINA[level.min(HORSE_MAX_LEVEL) as usize]
}

/// Raza del caballo montado por nivel â€” columna `iNPCRace` de `c_aHorseStat`
/// (parity `GetMyHorseVnum` horse_rider.h:83 + horse_rider.cpp:18-48):
/// 1-10 â†' 20101, 11-20 â†' 20104, 21-30 â†' 20107; nivel 0 â†' 0. Sin el
/// +delta de guild del C++ (guildas ausentes en reforge â€” divergencia).
pub fn horse_race_vnum(level: u8) -> u32 {
    match level.min(HORSE_MAX_LEVEL) {
        0 => 0,
        1..=10 => 20101,
        11..=20 => 20104,
        _ => 20107,
    }
}

/// DecisiÃ³n montar/desmontar (parity `CHorseRider::StartRiding`/`StopRiding`
/// horse_rider.cpp:165-193): `None` = rechazo/no-op silencioso (ya en el
/// estado pedido, nivel 0, HP 0 o stamina 0); `Some(vnum)` = aplicar, con
/// el vnum del wire (raza del caballo montando / 0 al desmontar).
pub fn toggle_ride(level: u8, hp: u16, stamina: u16, riding: bool, ride: bool) -> Option<u32> {
    if riding == ride {
        return None;
    }
    if ride && (level == 0 || hp == 0 || stamina == 0) {
        return None;
    }
    Some(if ride { horse_race_vnum(level) } else { 0 })
}

/// Crea un caballo: nivel clampado a [0, 30] (parity `MINMAX(0, iLevel,
/// HORSE_MAX_LEVEL)`, horse_rider.cpp:353) y salud completa del nivel
/// (parity `ReviveHorse` â†' `c_aHorseStat[level].iMaxHealth`,
/// horse_rider.cpp:104).
pub fn create_horse(level: u8) -> Horse {
    let level = level.min(HORSE_MAX_LEVEL);
    Horse {
        level,
        health: MAX_HEALTH[level as usize],
    }
}

/// Alimenta el caballo: +1 salud clampeada al mÃ¡ximo del nivel; sin
/// efecto con nivel 0 o caballo muerto (parity `FeedHorse` + `MINMAX`,
/// horse_rider.cpp:125-133, :334).
pub fn feed_horse(horse: Horse) -> Horse {
    let level = horse.level.min(HORSE_MAX_LEVEL);
    let max = MAX_HEALTH[level as usize];
    if level > 0 && horse.health > 0 {
        Horse {
            level,
            health: (horse.health + 1).min(max),
        }
    } else {
        horse
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER: la salud nace completa (tabla) y sube con la comida hasta
    /// el mÃ¡ximo del nivel; revertir el feed o el clamp rompe el test.
    #[test]
    fn feed_raises_health_up_to_level_max() {
        let horse = create_horse(1);
        assert_eq!(
            horse.health, 3,
            "nivel 1 â†’ iMaxHealth 3 (horse_rider.cpp:19)"
        );
        let half = Horse { health: 1, ..horse };
        assert_eq!(feed_horse(half).health, 2, "comer sube la salud +1");
        assert_eq!(feed_horse(horse).health, 3, "a tope â†’ clamp al mÃ¡ximo");
    }

    /// VERIFIER: valores golden de `c_aHorseStat` + clamp del nivel.
    #[test]
    fn create_clamps_level_and_uses_golden_max() {
        assert_eq!(create_horse(99).level, HORSE_MAX_LEVEL);
        assert_eq!(
            create_horse(10).health,
            15,
            "nivel 10 â†’ 15 (horse_rider.cpp:28)"
        );
        assert_eq!(
            create_horse(30).health,
            50,
            "nivel 30 â†’ 50 (horse_rider.cpp:48)"
        );
    }

    /// VERIFIER: caballo muerto o nivel 0 â†' el feed no hace nada.
    #[test]
    fn dead_or_level_zero_horse_is_not_fed() {
        let dead = Horse {
            level: 1,
            health: 0,
        };
        assert_eq!(feed_horse(dead), dead, "caballo muerto â†' no revive");
        let none = create_horse(0);
        assert_eq!(
            feed_horse(none),
            none,
            "nivel 0 â†' sin efecto (parity FeedHorse)"
        );
    }

    /// VERIFIER (FASE 1 caballo jugable): gates de montar/desmontar (parity
    /// StartRiding/StopRiding horse_rider.cpp:165-193) + razas golden de
    /// `iNPCRace`. Revertir un gate o la tabla de razas rompe el test.
    #[test]
    fn toggle_ride_gates_and_races() {
        assert_eq!(horse_race_vnum(0), 0);
        assert_eq!(horse_race_vnum(10), 20101, "nivel 10 â†' 20101");
        assert_eq!(horse_race_vnum(11), 20104, "nivel 11 â†' 20104");
        assert_eq!(horse_race_vnum(30), 20107, "nivel 30 â†' 20107");
        // rechazos silenciosos (parity :167-177): nivel 0 / HP 0 / stamina 0
        assert_eq!(toggle_ride(0, 1, 1, false, true), None);
        assert_eq!(toggle_ride(1, 0, 1, false, true), None);
        assert_eq!(toggle_ride(1, 1, 0, false, true), None);
        // no-op: ya montado pidiendo montar / ya desmontado pidiendo bajar
        assert_eq!(toggle_ride(1, 1, 1, true, true), None);
        assert_eq!(toggle_ride(1, 1, 1, false, false), None);
        // aplicar: montar â†' raza del tier; desmontar â†' 0
        assert_eq!(toggle_ride(5, 3, 4, false, true), Some(20101));
        assert_eq!(toggle_ride(5, 3, 4, true, false), Some(0));
        assert_eq!(toggle_ride(25, 50, 200, false, true), Some(20107));
    }

    /// VERIFIER: stamina mÃ¡xima golden (columna `iMaxStamina`).
    #[test]
    fn max_stamina_golden() {
        assert_eq!(max_stamina(0), 1);
        assert_eq!(max_stamina(1), 4, "nivel 1 â†' 4 (horse_rider.cpp:19)");
        assert_eq!(max_stamina(10), 10, "nivel 10 â†' 10 (horse_rider.cpp:28)");
        assert_eq!(
            max_stamina(30),
            200,
            "nivel 30 â†' 200 (horse_rider.cpp:48)"
        );
        assert_eq!(max_health(30), 50);
    }
}
