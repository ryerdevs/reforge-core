//! F3/F4 (ADR-0008): dominio economy — money log + guardas de oro
//! (schemas `log`/`player`).
//!
//! # Tabla de paridad (legacy → Rust)
//!
//! | Query legacy | file:line | Metodo Rust | SQL / semantica |
//! |---|---|---|---|
//! | MoneyLog | `game/src/log.cpp:114-122` (`LogManager::MoneyLog`) | `EconomyRepo::money_log` | `INSERT INTO log.money_log (time, type, vnum, gold) VALUES (NOW(), $1, $2, $3)` — append-only, una fila por evento. Validacion de tipo: `log.cpp:113` — `MONEY_LOG_RESERVED` (0) o `>= MAX_NUM` (9) es error. |
//! | Guard de oro (anti dupe) | ADR-0011; constraint `CHECK (gold >= 0)` migrado 2026-08-13 (`scripts/gpg/alter_gold_check.sql`: player/safebox/guild) | `checked_gold_mutation` | Defensa Rust ANTES de construir la mutation: `gold < 0` -> Err. El UPDATE es absoluto (`SET gold = $2 WHERE id = $1`) — idempotente por naturaleza (parity del save del C++). |
//!
//! # Decision de pipeline (documentada)
//!
//! El money log es AUDIT append-only, NO estado: se escribe DIRECTO (una
//! conexion, sin Batcher), como el legacy (`Query` desde el game). El pipeline
//! WAL es para ESTADO (replay idempotente); re-aplicar un INSERT de log
//! duplicaria la fila de audit — el trade-off de la ventana de crash se
//! documenta y se acepta (un log duplicado en la ventana commit/unlink es
//! tolerable para audit; el estado nunca se duplica).
//!
//! Tipos PG reales: `log.money_log` (time timestamp, type/vnum/gold integer,
//! sin PK — append-only). `player.player.gold` integer.

use crate::pool::{Client, PgPool};

use crate::account::pg_err;
use crate::wal::{Mutation, Param};

// Tipos del money log — parity `length.h:697-706`.
pub const MONEY_LOG_RESERVED: i32 = 0;
pub const MONEY_LOG_MONSTER: i32 = 1;
pub const MONEY_LOG_SHOP: i32 = 2;
pub const MONEY_LOG_REFINE: i32 = 3;
pub const MONEY_LOG_QUEST: i32 = 4;
pub const MONEY_LOG_GUILD: i32 = 5;
pub const MONEY_LOG_MISC: i32 = 6;
pub const MONEY_LOG_MONSTER_KILL: i32 = 7;
pub const MONEY_LOG_DROP: i32 = 8;
/// Centinela del enum (`length.h:706`) — los tipos validos son 1..=8.
pub const MONEY_LOG_TYPE_MAX_NUM: i32 = 9;

/// Maximum wallet balance (`length.h:80`). This is an overflow guard, not a
/// user-facing expansion of the wallet.
pub const GOLD_MAX: i64 = 2_000_000_000;

/// INSERT del money log (`log.cpp:120`): append-only, time = NOW().
const MONEY_LOG_SQL: &str = "\
INSERT INTO log.money_log (time, type, vnum, gold) VALUES (NOW(), $1, $2, $3)";

/// Repositorio del dominio economy (money log). Conexion por llamada (ADR-0008).
pub struct EconomyRepo {
    pool: PgPool,
}

impl EconomyRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn connect(&self) -> Result<Client, String> {
        self.pool
            .get()
            .await
            .map_err(|e| format!("PG pool get: {e}"))
    }

    /// Money log append-only (`log.cpp:114-122`): una fila por evento, escrita
    /// DIRECTO (sin Batcher — ver "Decision de pipeline" arriba). `Err` si el
    /// tipo esta fuera de 1..=8 (parity `log.cpp:113`: RESERVED y >= MAX_NUM
    /// son errores de tipo).
    pub async fn money_log(&self, mtype: i32, vnum: i32, gold: i32) -> Result<u64, String> {
        validate_money_log_type(mtype)?;
        let client = self.connect().await?;
        client
            .execute(MONEY_LOG_SQL, &[&mtype, &vnum, &gold])
            .await
            .map_err(|e| pg_err("MONEY_LOG", &e))
    }
}

/// Valida el tipo del money log (`log.cpp:113`): `1..=8` valido; `0`
/// (RESERVED) o `>= MAX_NUM` (9) -> Err.
pub fn validate_money_log_type(mtype: i32) -> Result<(), String> {
    if mtype <= MONEY_LOG_RESERVED || mtype >= MONEY_LOG_TYPE_MAX_NUM {
        return Err(format!(
            "money log: type {mtype} fuera de 1..={} (RESERVED/MAX)",
            MONEY_LOG_TYPE_MAX_NUM - 1
        ));
    }
    Ok(())
}

/// Returns whether an absolute wallet value is representable by the game
/// contract.
pub fn is_valid_gold(gold: i64) -> bool {
    (0..=GOLD_MAX).contains(&gold)
}

/// Applies a signed wallet delta without allowing overflow or an out-of-range
/// result. Negative deltas remain valid when the resulting balance is valid.
pub fn checked_gold_delta(current: i64, delta: i64) -> Option<i64> {
    if !is_valid_gold(current) {
        return None;
    }
    let next = current.checked_add(delta)?;
    is_valid_gold(next).then_some(next)
}

/// Spends an absolute amount from a wallet without allowing underflow.
pub fn checked_gold_sub(current: i64, amount: i64) -> Option<i64> {
    if amount < 0 {
        return None;
    }
    if !is_valid_gold(current) {
        return None;
    }
    let next = current.checked_sub(amount)?;
    is_valid_gold(next).then_some(next)
}

/// Guard de oro para mutations (anti-gold-dupe, ADR-0011): rechaza `gold < 0`
/// o `gold > GOLD_MAX` ANTES de construir la mutation. Documentado: desde 2026-08-13 la PG tiene
/// el constraint `CHECK (gold >= 0)` en las tablas de wallet (`player.player`,
/// `player.safebox`, `player.guild` — `scripts/gpg/alter_gold_check.sql`;
/// `log.money_log` EXCLUIDO a proposito: el legacy registra gastos como
/// deltas negativos, `char.cpp:7804`/`shop.cpp:395`). Este helper es
/// defense-in-depth: la DB garantiza, el guard falla temprano y con mensaje
/// claro; la mutation usa UPDATE absoluto (parity del save legacy,
/// idempotente por naturaleza).
pub fn checked_gold_mutation(player_id: i64, gold: i64) -> Result<Mutation, String> {
    if !is_valid_gold(gold) {
        return Err(format!(
            "gold fuera de 0..={GOLD_MAX} ({gold}) para el player {player_id} — rechazado por el guard"
        ));
    }
    Ok(Mutation::new(
        "UPDATE player.player SET gold = $2 WHERE id = $1",
        vec![Param::Int(player_id), Param::Int(gold)],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// money_log: 4 columnas en el orden del INSERT del legacy
    /// (`log.cpp:120` — time, type, vnum, gold) + NOW() para el tiempo.
    #[test]
    fn money_log_sql_shape() {
        let cols: Vec<&str> = MONEY_LOG_SQL
            .split_once(" VALUES ")
            .expect("VALUES")
            .0
            .split_once('(')
            .expect("(")
            .1
            .trim_end_matches(')')
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(
            cols,
            ["time", "type", "vnum", "gold"],
            "orden del INSERT legacy"
        );
        assert!(MONEY_LOG_SQL.contains("NOW()"), "time = NOW()");
        assert!(MONEY_LOG_SQL.contains("log.money_log"), "schema log");
    }

    /// Tipos validos 1..=8 (parity `log.cpp:113` — RESERVED=0 y >=9 son error).
    #[test]
    fn money_log_type_bounds() {
        assert!(validate_money_log_type(MONEY_LOG_MONSTER).is_ok());
        assert!(validate_money_log_type(MONEY_LOG_DROP).is_ok());
        assert!(
            validate_money_log_type(MONEY_LOG_RESERVED).is_err(),
            "RESERVED es error"
        );
        assert!(
            validate_money_log_type(MONEY_LOG_TYPE_MAX_NUM).is_err(),
            ">= MAX es error"
        );
        assert!(validate_money_log_type(-1).is_err(), "negativo es error");
        // Constantes del enum (length.h:697-706).
        assert_eq!(MONEY_LOG_TYPE_MAX_NUM, 9);
        assert_eq!(MONEY_LOG_MONSTER, 1);
        assert_eq!(MONEY_LOG_SHOP, 2);
        assert_eq!(MONEY_LOG_REFINE, 3);
        assert_eq!(MONEY_LOG_QUEST, 4);
        assert_eq!(MONEY_LOG_GUILD, 5);
        assert_eq!(MONEY_LOG_MISC, 6);
        assert_eq!(MONEY_LOG_MONSTER_KILL, 7);
        assert_eq!(MONEY_LOG_DROP, 8);
    }

    /// Guard de oro: negativo -> Err; 0 y positivo -> mutation absoluta
    /// idempotente (SET gold = $2 WHERE id = $1).
    #[test]
    fn checked_gold_mutation_rejects_negative() {
        assert!(
            checked_gold_mutation(1, -5).is_err(),
            "gold negativo rechazado"
        );
        assert!(
            checked_gold_mutation(1, 2_000_000_001).is_err(),
            "gold por encima de GOLD_MAX rechazado"
        );
        let m = checked_gold_mutation(1, 0).expect("0 ok");
        assert_eq!(
            m.sql, "UPDATE player.player SET gold = $2 WHERE id = $1",
            "UPDATE absoluto"
        );
        assert_eq!(m.params, vec![Param::Int(1), Param::Int(0)]);
        let m = checked_gold_mutation(2, 1_000).expect("positivo ok");
        assert_eq!(m.params, vec![Param::Int(2), Param::Int(1_000)]);
    }

    /// Property verifier: every accepted signed delta preserves the inclusive
    /// wallet bounds, while underflow, overflow, and invalid starting values
    /// are rejected without wrapping.
    #[test]
    fn checked_gold_delta_preserves_wallet_bounds() {
        let current_values = [
            -1,
            0,
            1,
            GOLD_MAX - 1,
            GOLD_MAX,
            GOLD_MAX + 1,
            i64::MIN,
            i64::MAX,
        ];
        let deltas = [i64::MIN, -GOLD_MAX, -1, 0, 1, GOLD_MAX, i64::MAX];
        for current in current_values {
            for delta in deltas {
                match checked_gold_delta(current, delta) {
                    Some(next) => {
                        assert!(is_valid_gold(current));
                        assert!(is_valid_gold(next));
                        assert_eq!(current.checked_add(delta), Some(next));
                    }
                    None => {
                        assert!(
                            !is_valid_gold(current)
                                || current.checked_add(delta).is_none()
                                || !is_valid_gold(current.checked_add(delta).unwrap_or(-1))
                        );
                    }
                }
            }
        }
        assert_eq!(checked_gold_sub(GOLD_MAX, GOLD_MAX), Some(0));
        assert_eq!(checked_gold_sub(0, 1), None);
        assert_eq!(checked_gold_sub(0, -1), None);
    }
}
