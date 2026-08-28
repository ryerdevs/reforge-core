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
    1, 3, 4, 5, 7, 8, 9, 11, 12, 13, 15, 18, 19, 21, 22, 24, 25, 27, 28, 30, 32, 35, 36, 37,
    38, 40, 42, 44, 46, 48, 50,
];

/// Crea un caballo: nivel clampado a [0, 30] (parity `MINMAX(0, iLevel,
/// HORSE_MAX_LEVEL)`, horse_rider.cpp:353) y salud completa del nivel
/// (parity `ReviveHorse` â†’ `c_aHorseStat[level].iMaxHealth`,
/// horse_rider.cpp:104).
pub fn create_horse(level: u8) -> Horse {
    let level = level.min(HORSE_MAX_LEVEL);
    Horse { level, health: MAX_HEALTH[level as usize] }
}

/// Alimenta el caballo: +1 salud clampeada al mÃ¡ximo del nivel; sin
/// efecto con nivel 0 o caballo muerto (parity `FeedHorse` + `MINMAX`,
/// horse_rider.cpp:125-133, :334).
pub fn feed_horse(horse: Horse) -> Horse {
    let level = horse.level.min(HORSE_MAX_LEVEL);
    let max = MAX_HEALTH[level as usize];
    if level > 0 && horse.health > 0 {
        Horse { level, health: (horse.health + 1).min(max) }
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
        assert_eq!(horse.health, 3, "nivel 1 â†’ iMaxHealth 3 (horse_rider.cpp:19)");
        let half = Horse { health: 1, ..horse };
        assert_eq!(feed_horse(half).health, 2, "comer sube la salud +1");
        assert_eq!(feed_horse(horse).health, 3, "a tope â†’ clamp al mÃ¡ximo");
    }

    /// VERIFIER: valores golden de `c_aHorseStat` + clamp del nivel.
    #[test]
    fn create_clamps_level_and_uses_golden_max() {
        assert_eq!(create_horse(99).level, HORSE_MAX_LEVEL);
        assert_eq!(create_horse(10).health, 15, "nivel 10 â†’ 15 (horse_rider.cpp:28)");
        assert_eq!(create_horse(30).health, 50, "nivel 30 â†’ 50 (horse_rider.cpp:48)");
    }

    /// VERIFIER: caballo muerto o nivel 0 â†’ el feed no hace nada.
    #[test]
    fn dead_or_level_zero_horse_is_not_fed() {
        let dead = Horse { level: 1, health: 0 };
        assert_eq!(feed_horse(dead), dead, "caballo muerto â†’ no revive");
        let none = create_horse(0);
        assert_eq!(feed_horse(none), none, "nivel 0 â†’ sin efecto (parity FeedHorse)");
    }
}
