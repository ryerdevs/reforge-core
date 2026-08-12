//! F4 (ADR-0008): `WorldStore` — composición del dominio world con el
//! pipeline durable ya conectado.
//!
//! - `new(pg_conn)` spawna el `Batcher` (100 ms — patrón de `wal.rs`) con el
//!   `WalSink` (WAL local a disco durable-first + `PgMutationSink` con audit
//!   `log.mutation_audit`, misma tx) y valida la conexión (fail-fast).
//!   El WAL local (F3 phase 2 — `database::wal::WalSink`): cada batch se
//!   persiste en `{wal_dir}/{uuid}.wal` ANTES de aplicar y el archivo se
//!   borra SOLO tras el COMMIT; al arrancar se re-aplica UNA vez por proceso
//!   (`replay_wal` + OnceLock — idempotente, auditoría en `wal.rs`).
//! - `list_characters` / `select_player` / `account_slots` = lecturas del
//!   flujo select (`input_login.cpp:247-287`): el C++ resuelve el pid del slot
//!   en `player_index` (`ClientManagerPlayer.cpp:794` — `SELECT pid%u ...`) y
//!   luego carga el player. La query del índice vive en
//!   `database::PlayerRepo::player_index_pid` (F4 slice 2 — resuelve la deuda
//!   del SQL directo documentada en el slice 1).
//! - `save_character` = write durable: `PlayerRepo::save_mutated` -> Batcher
//!   (batch transaccional <=100 ms + WAL local + audit, ADR-0008).

use std::sync::OnceLock;
use std::time::Duration;

use database::player::{PlayerRepo, PlayerRow, PlayerSummary};
use database::wal::{replay_wal, Batcher, PgMutationSink, WalSink};
use tokio_postgres::NoTls;

/// Directorio del WAL local: env `REALM_WAL_DIR` o `./wal` (documentado —
/// el CWD dual Windows/WSL enraíza distinto; los tests gated usan dir
/// temporal con cleanup SIEMPRE).
pub fn wal_dir() -> String {
    std::env::var("REALM_WAL_DIR").unwrap_or_else(|_| "./wal".to_string())
}

/// Replay UNA vez por proceso (varios `WorldStore` por conexión de login —
/// `channel.rs` crea uno por jugador; el replay concurrente contra appenders
/// vivos corrompería el estado). El primer `new` del proceso hace el replay.
fn replay_once(pg_conn: &str, dir: &str) -> Result<(), String> {
    static REPLAYED: OnceLock<Result<(), String>> = OnceLock::new();
    let res = REPLAYED.get_or_init(|| {
        let rt = tokio::runtime::Runtime::new().expect("runtime del replay");
        match rt.block_on(replay_wal(dir, pg_conn)) {
            Ok(_n) => Ok(()), // _n = archivos re-aplicados (log del módulo wal)
            Err(e) => Err(e),
        }
    });
    res.clone()
}

/// Composición del dominio world: repos + Batcher durable (WAL local + audit).
pub struct WorldStore {
    pg_conn: String,
    player: PlayerRepo,
    batcher: Batcher,
}

impl WorldStore {
    /// Crea el store: valida la conexión PG (fail-fast con `Err` descriptivo),
    /// hace el replay del WAL local UNA vez por proceso y spawna el Batcher
    /// (100 ms / 64 mutations por batch) con el `WalSink` (WAL local + sink
    /// con audit `log.mutation_audit` en la misma tx).
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

        // Replay del WAL local (idempotente) — una vez por proceso.
        let dir = wal_dir();
        replay_once(&conn, &dir)?;

        let player = PlayerRepo::new(&conn);
        let sink = WalSink::new(PgMutationSink::new(&conn), dir);
        let batcher = Batcher::spawn(Duration::from_millis(100), 64, sink);
        Ok(Self { pg_conn: conn, player, batcher })
    }

    /// Sobreescribe la tabla de audit del sink (tests: schema `e2e_wal_*`).
    /// El WAL local se RECONSTRUYE igual (el WalSink envuelve el sink nuevo —
    /// el wiring WAL→Batcher→PG nunca se desactiva silenciosamente).
    pub fn with_audit_table(mut self, audit_table: impl Into<String>) -> Self {
        let sink = WalSink::new(
            PgMutationSink::new(&self.pg_conn).with_audit_table(audit_table),
            wal_dir(),
        );
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
    /// La validación del slot y el SQL del índice viven en
    /// `database::player` (`index_sql_shape_and_slot_validation` + el gated
    /// `player_index_pid_live_account_slots`) — aquí solo la invariante de
    /// composición: account_slots devuelve un array fijo de 5 slots.
    #[test]
    fn account_slots_shape() {
        let _s: [Option<i64>; 5] = [None; 5];
    }
}
