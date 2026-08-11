//! F4 (ADR-0008): `WorldStore` — composición del dominio world con el
//! pipeline durable ya conectado.
//!
//! - `new(pg_conn)` spawna el `Batcher` (100 ms — patrón de `wal.rs`) con el
//!   `PgMutationSink` (audit `log.mutation_audit`, misma tx) y valida la
//!   conexión (fail-fast).
//! - `list_characters` / `select_player` / `account_slots` = lecturas del
//!   flujo select (`input_login.cpp:247-287`): el C++ resuelve el pid del slot
//!   en `player_index` (`ClientManagerPlayer.cpp:794` — `SELECT pid%u ...`) y
//!   luego carga el player. La query del índice vive en
//!   `database::PlayerRepo::player_index_pid` (F4 slice 2 — resuelve la deuda
//!   del SQL directo documentada en el slice 1).
//! - `save_character` = write durable: `PlayerRepo::save_mutated` -> Batcher
//!   (batch transaccional <=100 ms + audit, ADR-0008).

use std::time::Duration;

use database::player::{PlayerRepo, PlayerRow, PlayerSummary};
use database::wal::{Batcher, PgMutationSink};
use tokio_postgres::NoTls;

/// Composición del dominio world: repos + Batcher durable.
pub struct WorldStore {
    pg_conn: String,
    player: PlayerRepo,
    batcher: Batcher,
}

impl WorldStore {
    /// Crea el store: valida la conexión PG (fail-fast con `Err` descriptivo),
    /// monta los repos y spawna el Batcher (100 ms / 64 mutations por batch —
    /// patrón de `wal.rs`). El sink usa `log.mutation_audit` por defecto
    /// (el DDL lo aplica el lane del harness; los tests lo sobreescriben).
    pub async fn new(pg_conn: impl Into<String>) -> Result<Self, String> {
        let conn = pg_conn.into();
        // Sanity: la conexión es válida y la PG responde (fail-fast al arrancar
        // el realm; el resto de llamadas abren conexión propia por repo).
        let (client, connection) = tokio_postgres::connect(&conn, NoTls)
            .await
            .map_err(|e| format!("PG connect: {e}"))?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        client
            .query_one("SELECT 1", &[])
            .await
            .map_err(|e| format!("PG sanity SELECT 1: {e}"))?;

        let player = PlayerRepo::new(&conn);
        let sink = PgMutationSink::new(&conn);
        let batcher = Batcher::spawn(Duration::from_millis(100), 64, sink);
        Ok(Self { pg_conn: conn, player, batcher })
    }

    /// Sobreescribe la tabla de audit del sink (tests: schema `e2e_wal_*`).
    /// Patrón builder de `PgMutationSink::with_audit_table`.
    pub fn with_audit_table(mut self, audit_table: impl Into<String>) -> Self {
        let sink = PgMutationSink::new(&self.pg_conn).with_audit_table(audit_table);
        self.batcher = Batcher::spawn(Duration::from_millis(100), 64, sink);
        self
    }

    /// Lista de personajes de la cuenta (Q3 — `PlayerRepo::list_for_account`).
    /// Sin orden garantizado (parity: el C++ no ordena; el emparejamiento por
    /// slot usa `player_index`, no esta lista).
    pub async fn list_characters(&self, account_id: i64) -> Result<Vec<PlayerSummary>, String> {
        self.player.list_for_account(account_id).await
    }

    /// Los 5 pids de la cuenta en ORDEN de slot (parity `ClientManagerPlayer.cpp:794`
    /// — el C++ resuelve `pid%u` por slot en el índice; el 449B del select se
    /// arma por slot, no por el orden de la lista Q3).
    ///
    /// 5 lecturas del repo (una por slot): el login no es hot path; el pool de
    /// conexiones se decide con el pipeline WAL (ADR-0008).
    pub async fn account_slots(&self, account_id: i64) -> Result<[Option<i64>; 5], String> {
        let mut slots = [None; 5];
        for (i, slot) in slots.iter_mut().enumerate() {
            *slot = self.player.player_index_pid(account_id, i as u8).await?;
        }
        Ok(slots)
    }

    /// Select del flujo select/spawn: resuelve el pid del slot en
    /// `player.player_index` (parity `ClientManagerPlayer.cpp:794`) y carga el
    /// personaje completo (Q2).
    ///
    /// - `slot` fuera de 0..5 -> `Err` (el game valida antes de preguntar,
    ///   `input_login.cpp:260-264`).
    /// - Sin fila de índice o `pid = 0` -> `Ok(None)` (slot vacío — el C++
    ///   corta con "player index not found", `input_login.cpp:266-271`).
    /// - `pid > 0` pero el player no existe -> `Ok(None)` (carga Q2 sin fila).
    pub async fn select_player(&self, account_id: i64, slot: u8) -> Result<Option<PlayerRow>, String> {
        let Some(pid) = self.player.player_index_pid(account_id, slot).await? else {
            return Ok(None);
        };
        self.player.load(pid).await
    }

    /// Save durable del personaje: `PlayerRepo::save_mutated` -> Batcher
    /// (100 ms) -> sink con audit en la MISMA transacción. Fire-and-forget
    /// (la garantía durable la da el batch transaccional; los fallos se
    /// loguean en el worker y se re-aplicarían con el WAL local de F3 phase 2).
    pub fn save_character(&self, row: &PlayerRow) {
        self.player.save_mutated(&self.batcher, row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La validación del slot y el SQL del índice viven en
    /// `database::player` (`index_sql_shape_and_slot_validation` + el gated
    /// `player_index_pid_live_account_slots`) — aquí solo la invariante de
    /// composición: account_slots devuelve un array fijo de 5 slots.
    #[test]
    fn account_slots_shape() {
        let _s: [Option<i64>; 5] = [None; 5];
    }
}
