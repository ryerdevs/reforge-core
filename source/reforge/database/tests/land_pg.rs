//! Integration F3/F4 (LandRepo) contra PostgreSQL REAL — gated con `#[ignore]`
//! (mismo patrón que `player_pg.rs`). Oracle: el boot de lands del db
//! (`ClientManagerBoot.cpp:846-849`) y el log del core C++ del runtime
//! ("SendLandList map 41 count 18 elem_size: 432").
//!
//! Ejecutar desde WSL:
//!
//! ```text
//! source ~/.cargo/env
//! cd /mnt/c/projects/Metin2/source/reforge
//! cargo test --package database -- --ignored
//! ```

use database::land::LandRepo;
use std::sync::{Mutex, OnceLock};

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";

fn pg_conn() -> String {
    std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
}

/// Serializa los dos tests PG de `land_pg` (G3.2e): el test de load_by_map
/// ve el set estable de 18 lands del runtime, y el test de buy/transfer
/// inserta/borra temporales en la misma tabla. Sin el guard, los dos tests
/// corriendo en paralelo pueden hacer que el de load vea filas insertadas
/// por el otro o se pisen los borrados. El mismo guard cubre también a
/// tests PG de otros módulos que tocan `player.land`.
fn land_pg_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// load_by_map(41): 18 lands (parity log del core: "map 41 count 18"), con
/// los ids 201..218 del runtime y campos coherentes. Los valores EXACTOS de
/// x/y son structure-only (los lands son configuración viva — el contrato
/// duro es el count y la estructura).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn land_load_map_41_contract() {
    let _g = land_pg_lock().lock().unwrap_or_else(|e| e.into_inner());
    let repo = LandRepo::new(database::pool::new_pool(&pg_conn(), 4).expect("pool PG"));
    let lands = repo.load_by_map(41).await.expect("LAND_LOAD no falla");
    assert_eq!(
        lands.len(),
        18,
        "18 lands del mapa 41 (parity log del core)"
    );
    let ids: Vec<i64> = lands.iter().map(|l| l.id).collect();
    assert!(
        ids.windows(2).all(|w| w[0] < w[1]),
        "ORDER BY id (parity boot)"
    );
    assert_eq!(ids[0], 201, "primer land del mapa 41 (runtime)");
    assert_eq!(ids[17], 218, "último land del mapa 41 (runtime)");
    for l in &lands {
        assert_eq!(l.map_index, 41);
        assert!(
            l.width > 0 && l.height > 0,
            "dimensiones positivas: {:?}",
            l
        );
        assert!(l.x > 0 && l.y > 0, "coordenadas en células: {:?}", l);
        assert!(l.guild_id >= 0, "guild_id (0 = sin guild)");
    }
    // Mapa sin lands -> vec vacío (el C++ no manda el paquete con 0).
    assert!(repo.load_by_map(999_999).await.expect("DB up").is_empty());
}

/// VERIFIER (identidad PG — phase land): el id lo asigna la sequence, NO un
/// contador de proceso. Dos compras → ids distintos y ESTRICTAMENTE
/// crecientes (> max(id) 292 del runtime); la transferencia cambia el dueño
/// (parity `SetOwner`) y `load_by_map` lo ve. Limpieza total al final.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn land_buy_sequence_and_transfer_roundtrip() {
    let _g = land_pg_lock().lock().unwrap_or_else(|e| e.into_inner());
    let repo = LandRepo::new(database::pool::new_pool(&pg_conn(), 4).expect("pool PG"));
    let id1 = repo
        .buy(41, 66100, 9400, 300, 300, 10_000)
        .await
        .expect("buy 1");
    let id2 = repo
        .buy(41, 66200, 9400, 300, 300, 10_000)
        .await
        .expect("buy 2");
    assert!(id1 >= 293, "sigue al max(id) 292 del runtime: {id1}");
    assert!(
        id2 > id1,
        "sequence estrictamente creciente: {id1} -> {id2}"
    );
    assert_eq!(repo.transfer(id1, 42).await.expect("transfer"), 1, "1 fila");
    let l = repo
        .load_by_map(41)
        .await
        .expect("load")
        .into_iter()
        .find(|l| l.id == id1)
        .expect("el land comprado existe en el mapa");
    assert_eq!(l.guild_id, 42, "dueño transferido (parity SetOwner)");
    assert_eq!(
        repo.transfer(id1, 0).await.expect("revert"),
        1,
        "revert dueño"
    );
    assert_eq!(repo.delete(id1).await.expect("clear 1"), 1, "limpieza 1");
    assert_eq!(repo.delete(id2).await.expect("clear 2"), 1, "limpieza 2");
}
