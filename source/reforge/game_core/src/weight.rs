//! PESO (slice weight): dominio puro del sistema de peso del Metin2
//! clásico. El C++ de esta variante NO tiene el sistema (verificado:
//! sin `Weight` en char.cpp, sin columna weight en item_proto.txt, sin
//! barra en el cliente; `player.item_proto.weight` = 0 en 11 002 filas)
//! — la fórmula es la clásica del upstream (GetMaxWeight): escalado ×10,
//! gate server-side, fail-open si el proto no tiene peso.

/// Peso de `count` items de un vnum: `proto.weight × count / 10` (escala
/// 0.1 del `GetWeight` clásico). Sin peso en el proto → 0 (fail-open).
pub fn weight_for_item(proto_weight: i64, count: i64) -> i64 {
    proto_weight * count / 10
}

/// ¿Cabe `add_weight` con `current_weight` cargado y `max_weight` límite?
/// (el límite INCLUYE el máximo: al peso exacto no cabe nada más).
pub fn can_carry(current_weight: i64, add_weight: i64, max_weight: i64) -> bool {
    current_weight.saturating_add(add_weight) <= max_weight
}

/// Máximo cargable del jugador: `(30 + level×3 + ST×2) × 10` (parity
/// `GetMaxWeight` del Metin2 clásico, `30 + level*3 + ST*2` unidades).
pub fn max_weight(level: i64, st: i64) -> i64 {
    (30 + level * 3 + st * 2) * 10
}

#[cfg(test)]
mod tests {
    use super::*;

    const SWORD: i64 = 1900; // proto weight típico de espada

    /// VERIFIER: al peso LÍMITE no se puede recoger; con exactamente el
    /// hueco del item, sí (gate `can_carry` del pickup).
    #[test]
    fn weight_limit_rejects_pickup() {
        let max = max_weight(1, 12); // (30 + 3 + 24) × 10 = 570
        let sword = weight_for_item(SWORD, 1); // 190
        assert!(
            !can_carry(max, sword, max),
            "peso al límite → el pickup se rechaza"
        );
        assert!(
            can_carry(max - sword, sword, max),
            "con el hueco exacto del item → cabe"
        );
        assert!(can_carry(0, sword, max), "inventario vacío → cabe");
    }

    #[test]
    fn weight_scales_with_count_and_fails_open() {
        assert_eq!(weight_for_item(60, 1), 6);
        assert_eq!(weight_for_item(60, 10), 60);
        assert_eq!(weight_for_item(0, 1), 0, "proto sin peso → 0 (fail-open)");
    }

    #[test]
    fn max_weight_grows_with_level_and_st() {
        assert_eq!(max_weight(1, 12), 570);
        assert_eq!(max_weight(60, 100), (30 + 180 + 200) * 10);
    }
}