//! Integration F4 slice 3.1 (CommonRepo) contra PostgreSQL REAL — gated.
//! Oracle: la tabla `common.exp_table` del runtime (el C++ la carga en el
//! boot, `config.cpp:1389`; `GetNextExp` = `exp_table[level]`, char.cpp:7190).

use database::common::CommonRepo;

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";

fn pg_conn() -> String {
    std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
}

/// next_exp de niveles reales (tabla common.exp_table — level 1 -> 300).
/// Los valores EXACTOS son estructura viva del runtime (el boot la lee); el
/// contrato duro: 1:1 por nivel y valores positivos crecientes.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn next_exp_contract_against_real_table() {
    let repo = CommonRepo::new(pg_conn());
    let lvl1 = repo.next_exp(1).await.expect("next_exp(1)");
    assert_eq!(lvl1, 300, "exp_table[1] del runtime (verificado en PG)");
    let lvl2 = repo.next_exp(2).await.expect("next_exp(2)");
    assert!(lvl2 > lvl1, "creciente: {lvl1} -> {lvl2}");
    assert!(repo.next_exp(5).await.expect("next_exp(5)") > lvl2);
}
