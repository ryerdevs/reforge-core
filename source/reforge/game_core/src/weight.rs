//! PESO (slice weight): dominio puro del sistema de peso del Metin2
//! clásico. El C++ de esta variante NO tiene el sistema (verificado:
//! sin `Weight` en char.cpp, sin columna weight en item_proto.txt, sin
//! barra en el cliente). `item_proto.weight` = 0 en TODAS las fuentes
//! clásicas (re-verificado 2026-08-28 con TEA+LZO — key DumpProto — y el
//! dump MariaDB): PG 11 002 filas, dump 11 002 filas (2 bloques), item_proto del pack ×3
//! idiomas (bWeight a 0, TItemTable_r156 pack(1) offset 60), el txt del
//! C++ 0 B, upstream old-metin2.com sin columna WEIGHT, DumpProto sin
//! weight en su CSV — el gate es fail-open hasta que exista una fuente
//! real (pendiente progress.md:35). La fórmula es la clásica del
//! upstream (GetMaxWeight): escalado ×10, gate server-side.

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

    /// Peso clásico de la Espada (vnum 10, WEAPON_SWORD) — parity de la
    /// tabla clásica del Metin2: 190 = 19.0 unidades (÷10), la escala
    /// REAL de la columna `player.item_proto.weight` (smallint; el bWeight
    /// BYTE del TItemTable del cliente cabría 190). Esta variante la lleva
    /// a 0 (verificado: pack + PG + dump + txt, 2026-08-28) — el gate es
    /// fail-open; al importar los pesos clásicos, este verifier fija la
    /// escala correcta (un 1900 fabricado pasaría con una importación ×10
    /// errónea).
    const ESPADA: i64 = 190; // vnum 10, peso clásico 0.1-u

    /// VERIFIER: con peso REAL del proto al límite NO se permite recoger;
    /// con exactamente el hueco del item, sí (gate `can_carry` del pickup,
    /// events.rs).
    #[test]
    fn real_proto_weight_rejects_carry_at_limit() {
        let max = max_weight(1, 12); // (30 + 3 + 24) × 10 = 570
        let espada = weight_for_item(ESPADA, 1); // 19 u = 3.3 % del máximo
        assert_eq!(weight_for_item(ESPADA, 10), 190, "10 espadas = 1/3 del máximo");
        assert!(
            !can_carry(max, espada, max),
            "peso al límite → el pickup se rechaza"
        );
        assert!(
            can_carry(max - espada, espada, max),
            "con el hueco exacto del item → cabe"
        );
        assert!(can_carry(0, espada, max), "inventario vacío → cabe");
    }

    #[test]
    fn weight_scales_with_count_and_fails_open() {
        assert_eq!(weight_for_item(60, 1), 6);
        assert_eq!(weight_for_item(60, 10), 60);
        assert_eq!(weight_for_item(0, 1), 0, "proto sin peso → 0 (fail-open)");
        assert!(
            can_carry(570, 0, 570),
            "fail-open: hasta al límite, un item sin peso (0) nunca se rechaza"
        );
    }

    #[test]
    fn max_weight_grows_with_level_and_st() {
        assert_eq!(max_weight(1, 12), 570);
        assert_eq!(max_weight(60, 100), (30 + 180 + 200) * 10);
    }
}