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

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";

fn pg_conn() -> String {
    std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
}

/// load_by_map(41): 18 lands (parity log del core: "map 41 count 18"), con
/// los ids 201..218 del runtime y campos coherentes. Los valores EXACTOS de
/// x/y son structure-only (los lands son configuración viva — el contrato
/// duro es el count y la estructura).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn land_load_map_41_contract() {
    let repo = LandRepo::new(pg_conn());
    let lands = repo.load_by_map(41).await.expect("LAND_LOAD no falla");
    assert_eq!(lands.len(), 18, "18 lands del mapa 41 (parity log del core)");
    let ids: Vec<i64> = lands.iter().map(|l| l.id).collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "ORDER BY id (parity boot)");
    assert_eq!(ids[0], 201, "primer land del mapa 41 (runtime)");
    assert_eq!(ids[17], 218, "último land del mapa 41 (runtime)");
    for l in &lands {
        assert_eq!(l.map_index, 41);
        assert!(l.width > 0 && l.height > 0, "dimensiones positivas: {:?}", l);
        assert!(l.x > 0 && l.y > 0, "coordenadas en células: {:?}", l);
        assert!(l.guild_id >= 0, "guild_id (0 = sin guild)");
    }
    // Mapa sin lands -> vec vacío (el C++ no manda el paquete con 0).
    assert!(repo.load_by_map(999_999).await.expect("DB up").is_empty());
}
