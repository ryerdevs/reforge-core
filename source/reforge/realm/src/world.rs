//! F4 (ADR-0008): `WorldStore` — composición del dominio world con el
//! pipeline durable ya conectado.
//!
//! - `new(pg_conn)` spawna el `Batcher` (100 ms — patrón de `wal.rs`) con el
//!   `PgMutationSink` (audit `log.mutation_audit`, misma tx) y valida la
//!   conexión (fail-fast).
//! - `list_characters` / `select_player` = lecturas del flujo select
//!   (`input_login.cpp:247-287`): el C++ resuelve el pid del slot en
//!   `player_index` (`ClientManagerPlayer.cpp:794` — `SELECT pid%u ...`) y
//!   luego carga el player.
//! - `save_character` = write durable: `PlayerRepo::save_mutated` -> Batcher
//!   (batch transaccional <=100 ms + audit, ADR-0008).
//!
//! NOTA (desviación documentada): `player_index` no tiene repo en `database`
//! (F3 close no lo portó) y este lane no puede tocar `database` (solo LEE) —
//! la query del índice vive aqui como SQL directo con conexión propia
//! (parity literal de `ClientManagerPlayer.cpp:794`). Es la ÚNICA query directa
//! del crate; el resto pasa por repos.

use std::time::Duration;

use database::player::{PlayerRepo, PlayerRow, PlayerSummary};
use database::wal::{Batcher, PgMutationSink};
use tokio_postgres::NoTls;

/// Máximo de personajes por cuenta (`protocol::PLAYER_PER_ACCOUNT` = 5).
const PLAYER_PER_ACCOUNT: usize = 5;

/// Nombres de columna del índice por slot (parity `ClientManagerPlayer.cpp:794`
/// — `SELECT pid%u` con `account_index + 1`). Constante cerrada: el slot se
/// valida contra `PLAYER_PER_ACCOUNT` antes de indexar.
const PID_COLUMNS: [&str; PLAYER_PER_ACCOUNT] = ["pid1", "pid2", "pid3", "pid4", "pid5"];

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
        let slot = slot as usize;
        if slot >= PLAYER_PER_ACCOUNT {
            return Err(format!("select_player: slot {slot} fuera de rango 0..{}", PLAYER_PER_ACCOUNT - 1));
        }
        let pid = self.index_pid(account_id, slot).await?;
        match pid {
            0 => Ok(None),
            pid => self.player.load(pid).await,
        }
    }

    /// Save durable del personaje: `PlayerRepo::save_mutated` -> Batcher
    /// (100 ms) -> sink con audit en la MISMA transacción. Fire-and-forget
    /// (la garantía durable la da el batch transaccional; los fallos se
    /// loguean en el worker y se re-aplicarían con el WAL local de F3 phase 2).
    pub fn save_character(&self, row: &PlayerRow) {
        self.player.save_mutated(&self.batcher, row);
    }

    /// Query directa del índice (única del crate — ver nota del módulo):
    /// `SELECT pid{n} FROM player.player_index WHERE id = $1` (parity literal
    /// `ClientManagerPlayer.cpp:794`). 0 filas -> 0 (slot vacío).
    async fn index_pid(&self, account_id: i64, slot: usize) -> Result<i64, String> {
        let (client, connection) = tokio_postgres::connect(&self.pg_conn, NoTls)
            .await
            .map_err(|e| format!("PG connect: {e}"))?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let sql = format!(
            "SELECT {} FROM player.player_index WHERE id = $1",
            PID_COLUMNS[slot]
        );
        let rows = client
            .query(&sql, &[&account_id])
            .await
            .map_err(|e| format!("PLAYER_INDEX pid{}: {e}", slot + 1))?;
        Ok(rows.first().and_then(|r| r.try_get(0).ok()).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El slot se valida contra el rango antes de indexar `PID_COLUMNS`
    /// (compilación: el array tiene 5 entradas; el error es runtime).
    #[test]
    fn slot_range_and_pid_columns() {
        assert_eq!(PID_COLUMNS.len(), PLAYER_PER_ACCOUNT);
        assert_eq!(PID_COLUMNS, ["pid1", "pid2", "pid3", "pid4", "pid5"]);
        // La validación de select_player cubre 0..4 (se testea en el gated
        // con PG real; aquí solo la invariante del array).
        assert!(PID_COLUMNS[0].starts_with("pid"));
        assert!(PID_COLUMNS[4].ends_with('5'));
    }
}
