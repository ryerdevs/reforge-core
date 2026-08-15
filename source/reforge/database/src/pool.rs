//! Pool compartido de conexiones PG (ADR-0008 — fix del cuello del entry
//! 2026-08-13): los repos del crate `database` abrían UNA conexión por
//! llamada (`tokio_postgres::connect` por query); el entry del canal hacía
//! ~10 viajes secuenciales y cada uno pagaba el handshake PG completo
//! (login_ms 0.9→4.9 s con 20 bots). El pool reutiliza conexiones; los repos
//! toman `PgPool` y la única apertura real ocurre al arrancar el proceso.
//!
//! # Versión verificada (2026-08-13)
//!
//! `deadpool-postgres 0.14.1` depende de `tokio-postgres 0.7.9` (verificado
//! en el Cargo.toml del crate en el registry) — el MISMO driver 0.7.x del
//! workspace (sin subir tokio-postgres; la feature `with-uuid-1` se unifica).
//!
//! # `pool_max_lifetime` — NO disponible (documentado)
//!
//! deadpool 0.12 (el que usa deadpool-postgres 0.14.1) expone
//! `PoolConfig { max_size, timeouts, queue_mode }` — SIN `max_lifetime`
//! (verificado en `deadpool-0.12.0/src/managed/config.rs`). La clave de
//! config del enunciado no se añade: una clave sin efecto violaría el
//! ponytail. El reciclaje default comprueba `Client::is_closed()` y las
//! conexiones rotas por el server fallan en el `get`/query siguiente.

use deadpool_postgres::{Config, Pool, PoolConfig};
use tokio_postgres::NoTls;

/// El pool compartido (clone = Arc bump — barato; se clona por repo/sesión).
pub type PgPool = Pool;

/// Cliente del pool (wrapper de deadpool-postgres; `Deref` al
/// `tokio_postgres::Client` — las queries/transacciones son las mismas).
pub use deadpool_postgres::Client;

/// Crea el pool del proceso (lazy: la primera conexión PG se abre en el
/// primer `get`). `max_size` = número máximo de conexiones abiertas
/// simultáneas (default del config del binario: 10).
///
/// SIN `timeouts.wait`: deadpool 0.12 exige un runtime tokio activo en el
/// `build()` cuando hay timeouts ("Timeouts require a runtime") — los unit
/// tests del crate (sync) no tienen runtime. Trade-off documentado: un
/// `get()` con el pool agotado espera a que se libere una conexión (las
/// queries del runtime son de ms; con PG caída el `get` falla al crear).
pub fn new_pool(pg_conn: &str, max_size: usize) -> Result<PgPool, String> {
    let cfg = Config {
        url: Some(pg_conn.to_string()),
        pool: Some(PoolConfig {
            max_size,
            timeouts: Default::default(),
            queue_mode: Default::default(),
        }),
        ..Config::default()
    };
    cfg.create_pool(None, NoTls).map_err(|e| format!("PG pool: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// new_pool: URL libpq valida -> pool creado SIN conectar (lazy: el pool
    /// no abre conexiones hasta el primer get); URL invalida -> Err.
    #[test]
    fn new_pool_valid_url_and_invalid() {
        let pool = new_pool("host=127.0.0.1 port=1 user=x password=x dbname=x", 2)
            .expect("URL valida -> pool");
        assert_eq!(pool.status().max_size, 2);
        assert!(new_pool("no-es-una-url", 2).is_err(), "URL invalida -> Err");
    }
}