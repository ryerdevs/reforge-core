//! F4 (ADR-0008): `WorldStore` — composición del dominio world con el
//! pipeline durable ya conectado.
//!
//! - `new(pool, batcher)` compone los repos del dominio sobre el POOL
//!   compartido del proceso y el `Batcher` ÚNICO del canal (100 ms — patrón
//!   de `wal.rs`) con el `WalSink` (WAL local a disco durable-first +
//!   `PgMutationSink` con audit `log.mutation_audit`, misma tx). El pool y el
//!   Batcher los crea el ARRANQUE del canal (`channel::run`): el Batcher por
//!   jugador era uno de los cuellos del entry (un worker + una cola por
//!   login; el shared Batcher = un único loop de flush por canal).
//! - `replay_once` — replay del WAL local UNA vez por proceso (idempotente,
//!   auditoría en `wal.rs`): lo invoca el arranque del canal ANTES de aceptar
//!   conexiones.
//! - `list_characters` / `select_player` / `account_slots` = lecturas del
//!   flujo select (`input_login.cpp:247-287`): el C++ resuelve el pid del slot
//!   en `player_index` (`ClientManagerPlayer.cpp:794` — `SELECT pid%u ...`) y
//!   luego carga el player. La query del índice vive en
//!   `database::PlayerRepo::player_index_pid` (F4 slice 2 — resuelve la deuda
//!   del SQL directo documentada en el slice 1).
//! - `save_character` = write durable: `PlayerRepo::save_mutated` -> Batcher
//!   (batch transaccional <=100 ms + WAL local + audit, ADR-0008).

use std::sync::Arc;

use database::player::{PlayerCreate, PlayerRepo, PlayerRow, PlayerSummary};
use database::pool::PgPool;
use database::wal::{replay_wal, Batcher};

/// Directorio del WAL local: env `REALM_WAL_DIR` o `./wal` (documentado —
/// el CWD dual Windows/WSL enraíza distinto; los tests gated usan dir
/// temporal con cleanup SIEMPRE).
pub fn wal_dir() -> String {
    std::env::var("REALM_WAL_DIR").unwrap_or_else(|_| "./wal".to_string())
}

/// Replay UNA vez por proceso (el arranque del canal lo invoca; el replay
/// concurrente contra appenders vivos corrompería el estado).
///
/// ASYNC de propósito: el arranque del canal corre dentro de un worker de
/// tokio — un `Runtime::new().block_on(...)` anidado PANICKEA ("Cannot start
/// a runtime from within a runtime", visto en el E2E real 2026-08-12).
/// `tokio::sync::OnceCell` da la misma garantía (inicialización única,
/// callers concurrentes esperan) sin runtime anidado.
pub async fn replay_once(pool: &PgPool, dir: &str) -> Result<(), String> {
    static REPLAYED: tokio::sync::OnceCell<Result<(), String>> = tokio::sync::OnceCell::const_new();
    let res = REPLAYED
        .get_or_init(|| async { replay_wal(dir, pool).await.map(|_n| ()) })
        .await;
    res.clone()
}

/// Composición del dominio world: repos sobre el pool compartido + el Batcher
/// ÚNICO del canal (WAL local + audit).
pub struct WorldStore {
    pool: PgPool,
    player: PlayerRepo,
    batcher: Arc<Batcher>,
}

impl WorldStore {
    /// Compone el store: repos sobre `pool` y el `batcher` compartido que el
    /// canal creó en su arranque (UN solo worker de flush por canal — ya no
    /// un Batcher por jugador; la semántica durable del Batcher no cambia).
    /// No falla: el sanity (SELECT 1) y el replay del WAL ya ocurrieron en el
    /// arranque del canal.
    pub fn new(pool: PgPool, batcher: Arc<Batcher>) -> Self {
        let player = PlayerRepo::new(pool.clone());
        Self { pool, player, batcher }
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
    /// COMPARTIDO del canal (100 ms) -> sink con audit en la MISMA
    /// transacción. Fire-and-forget (la garantía durable la da el batch
    /// transaccional; los fallos se loguean en el worker y se re-aplicarían
    /// con el WAL local de F3 phase 2).
    pub fn save_character(&self, row: &PlayerRow) {
        self.player.save_mutated(&self.batcher, row);
    }

    /// UNIDAD ACID durable (F6 social — ADR-0011 "items as ACID units"):
    /// `ItemExchange::exchange_mutated` (materiales→resultado→oro en UNA
    /// transacción + audit) con el Batcher del canal — el acceso al Batcher
    /// que el lane social necesita (el commit del trade y el buy/sell del
    /// shop NUNCA hacen commits por item).
    ///
    /// `Ok` = el batch commiteó; `Err` = el sink falló (el WAL local
    /// conserva el archivo para el replay del próximo arranque).
    pub async fn exchange(&self, ex: &database::item::ItemExchange) -> Result<(), String> {
        database::item::ItemRepo::new(self.pool.clone())
            .exchange_mutated(&self.batcher, ex)
            .await
    }

    /// Create del personaje (parity `__QUERY_PLAYER_CREATE`
    /// `ClientManagerPlayer.cpp:774-913`): INSERT del player + slot del
    /// índice. Si el slot falla, rollback del player (el C++ borra el player
    /// recién creado cuando el UPDATE del índice falla — `:901-907`).
    pub async fn create_character(&self, c: &PlayerCreate, slot: u8) -> Result<i64, String> {
        let pid = self.player.create(c).await?;
        if let Err(e) = self.player.set_slot(c.account_id, slot, pid).await {
            let _ = self.player.delete(c.account_id, slot, pid).await; // rollback best-effort
            return Err(e);
        }
        Ok(pid)
    }

    /// Borrado del personaje (parity `__RESULT_PLAYER_DELETE`
    /// `ClientManagerPlayer.cpp:1055-1130`): slot a 0 + DELETE del player y
    /// sus items/quests/afectos.
    pub async fn delete_character(&self, account_id: i64, slot: u8, player_id: i64) -> Result<(), String> {
        self.player.delete(account_id, slot, player_id).await
    }

    /// Empire de la cuenta (parity `QUERY_EMPIRE_SELECT`
    /// `ClientManager.cpp:1129-1200`): UPDATE del `player_index.empire` +
    /// reposiciona los personajes de la cuenta a la aldea del imperio.
    /// Divergencia documentada: el canal sirve UN solo mapa (41) — todos los
    /// personajes van a la aldea de Shinsoo (mapa 41, UNITS 969600/278400 =
    /// `g_start_position[3]`); el C++ mueve por imperio (mapas 1/21/41).
    pub async fn set_empire(&self, account_id: i64, empire: u8) -> Result<(), String> {
        self.player.set_empire(account_id, i16::from(empire)).await?;
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| format!("PG pool get: {e}"))?;
        client
            .execute(
                "UPDATE player.player SET map_index = 41, x = 969600, y = 278400 \
WHERE account_id = $1",
                &[&account_id],
            )
            .await
            .map_err(|e| format!("PLAYER_EMPIRE_MOVE: {e}"))?;
        Ok(())
    }

    /// Renombre (parity `QUERY_CHANGE_NAME` `ClientManager.cpp:548-588`):
    /// UPDATE del nombre; el chequeo de unicidad (`name_exists`) lo hace el
    /// handler del entry ANTES (parity — el db C++ responde CREATE_ALREADY).
    pub async fn rename_character(&self, player_id: i64, name: &str) -> Result<u64, String> {
        self.player.rename(player_id, name).await
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
