//! Integration F4 (WorldStore) contra PostgreSQL REAL — gated con `#[ignore]`
//! (mismo patrón que `database/tests/*_pg.rs`).
//!
//! Ejecutar desde WSL (la PG vive en WSL):
//!
//! ```text
//! source ~/.cargo/env
//! cd /mnt/c/projects/Metin2/source/reforge
//! cargo test --package game_core -- --ignored
//! ```
//!
//! Reglas: SOLO lecturas sobre datos vivos (cuenta `test` — asserts
//! structure-only documentados) + un personaje THROWAWAY con cleanup SIEMPRE
//! (patrón trap del E2E — el usuario juega en este stack). El audit del
//! pipeline usa el schema temporal `e2e_wal_realm` (nunca `log.mutation_audit`).

use database::player::{PlayerCreate, PlayerRepo};
use database::wal::audit_ddl;
use game_core::world::WorldStore;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";
/// Cuenta viva del E2E (no se toca): test / 1234.
const TEST_ACCOUNT: i64 = 1;

fn pg_conn() -> String {
    std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
}

fn ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn raw_client(conn: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(conn, tokio_postgres::NoTls)
        .await
        .expect("PG connect");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
}

/// Poll del contador de audit hasta `expected` (el sink abre conexión PG
/// nueva en cada apply; un sleep fijo flakea en WSL — patrón verificado en F3).
async fn wait_for_audit(client: &tokio_postgres::Client, audit: &str, expected: i64) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let n: i64 = client
            .query_one(&format!("SELECT COUNT(*) FROM {audit}"), &[])
            .await
            .expect("count audit")
            .get(0);
        if n >= expected {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timeout esperando el flush (audit={n}, expected={expected})"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// list_characters(cuenta test) -> >=3 personajes con los del E2E Q3.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package game_core -- --ignored"]
async fn realm_list_characters_live_account() {
    let store = WorldStore::new(pg_conn()).await.expect("WorldStore::new (PG up)");
    let list = store.list_characters(TEST_ACCOUNT).await.expect("list");
    assert!(list.len() >= 3, ">=3 (E2E Q3): got {}", list.len());
    let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
    for expected in ["lkjsnlfknlsk", "ninja", "Chaman"] {
        assert!(names.contains(&expected), "lista contiene {expected}: {names:?}");
    }
}

/// select_player sobre los slots reales de la cuenta test:
/// player_index pid1..pid5 = [1, 3, 5, 0, 2] (E2E Q1) -> slots 0,1,2,4
/// devuelven personaje; slot 3 (pid=0) -> None; x/y en UNITS.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package game_core -- --ignored"]
async fn realm_select_player_each_live_character() {
    let store = WorldStore::new(pg_conn()).await.expect("WorldStore::new");

    // Slot 0 -> pid1 = 1 (lkjsnlfknlsk).
    let p = store.select_player(TEST_ACCOUNT, 0).await.expect("select slot 0").expect("pid1=1 existe");
    assert_eq!(p.id, 1, "pid1 de la cuenta test");
    assert_eq!(p.name, "lkjsnlfknlsk", "E2E Q3");

    // Slot 1 -> pid2 = 3 (Chaman).
    let p = store.select_player(TEST_ACCOUNT, 1).await.expect("select slot 1").expect("pid2=3 existe");
    assert_eq!(p.id, 3, "pid2 de la cuenta test");

    // Slot 2 -> pid3 = 5.
    let p = store.select_player(TEST_ACCOUNT, 2).await.expect("select slot 2").expect("pid3=5 existe");
    assert_eq!(p.id, 5, "pid3 de la cuenta test");

    // Slot 3 -> pid4 = 0 (slot vacio) -> None.
    assert_eq!(
        store.select_player(TEST_ACCOUNT, 3).await.expect("select slot 3"),
        None,
        "pid4=0 -> None (input_login.cpp:266-271)"
    );

    // Slot 4 -> pid5 = 2 (ninja, E2E Q2: job=1).
    let p = store.select_player(TEST_ACCOUNT, 4).await.expect("select slot 4").expect("pid5=2 existe");
    assert_eq!(p.id, 2, "pid5 de la cuenta test");
    assert_eq!(p.name, "ninja");
    assert_eq!(p.job, 1, "job=1 (E2E Q2)");
    // x/y en UNITS (structure-only: el usuario juega; los viejos están en la
    // aldea 969600/278400 — parity E2E Q5).
    assert!(p.x > 0 && p.y > 0, "x/y units positivos: {},{}", p.x, p.y);
}

/// account_slots: los 5 pids de la cuenta test EN ORDEN de slot
/// (pid1..pid5 = [1, 3, 5, 0, 2], E2E Q1) — el orden es el contrato del 449B.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package game_core -- --ignored"]
async fn realm_account_slots_live_account() {
    let store = WorldStore::new(pg_conn()).await.expect("WorldStore::new");
    let slots = store.account_slots(TEST_ACCOUNT).await.expect("account_slots");
    assert_eq!(
        slots,
        [Some(1), Some(3), Some(5), None, Some(2)],
        "pid1..pid5 de la cuenta test (E2E Q1), en orden de slot"
    );
    // Cuenta sin fila de índice -> 5 × None.
    assert_eq!(
        store.account_slots(999_999_999).await.expect("DB up"),
        [None; 5],
        "sin fila de player_index -> slots vacios"
    );
}

/// select_player con slot inválido -> Err (parity: el game valida antes,
/// input_login.cpp:260-264).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package game_core -- --ignored"]
async fn realm_select_player_invalid_slot() {
    let store = WorldStore::new(pg_conn()).await.expect("WorldStore::new");
    for slot in [5u8, 6, 200] {
        let err = store.select_player(TEST_ACCOUNT, slot).await.expect_err("slot invalido -> Err");
        assert!(err.contains("fuera de rango"), "err: {err}");
    }
}

/// save_character de un THROWAWAY a través del pipeline durable: el Batcher
/// (100 ms) aplica el UPDATE con audit en la MISMA transacción (schema
/// e2e_wal_realm) -> la fila queda persistida. Cleanup SIEMPRE.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn realm_save_character_via_batcher_audit() {
    let conn = pg_conn();
    let schema = "e2e_wal_realm";
    let audit = format!("{schema}.mutation_audit");
    let client = raw_client(&conn).await;
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}; {}",
            audit_ddl(&audit)
        ))
        .await
        .expect("setup schema de test");

    let store = WorldStore::new(&conn)
        .await
        .expect("WorldStore::new")
        .with_audit_table(audit.clone());
    let repo = PlayerRepo::new(&conn);
    // Nombre UNICO por test (los tests del bin corren en paralelo).
    let name = format!("e2e_rsave_{}", ts());

    let result = async {
        // Crear el throwaway (id DEFAULT, regla B5) y cargar su row.
        let c = PlayerCreate {
            account_id: TEST_ACCOUNT,
            name: name.clone(),
            level: 1,
            st: 30, ht: 30, dx: 30, iq: 30,
            job: 0, voice: 0, dir: 0,
            x: 0, y: 0, z: 0,
            hp: 100, mp: 100,
            random_hp: 0, random_sp: 0, stat_point: 0, stamina: 100,
            part_base: 0, part_main: 0, part_hair: 0,
            gold: 0, playtime: 0,
            skill_level: vec![0x01, 0x02],
            quickslot: vec![0x01, 0x02],
        };
        let id = repo.create(&c).await.expect("create");
        let mut row = repo.load(id).await.expect("load").expect("existe");
        row.x = 969600;
        row.y = 278400;

        // Write durable: save_character -> Batcher -> sink (audit misma tx).
        store.save_character(&row);
        wait_for_audit(&client, &audit, 1).await;

        // La fila quedo aplicada (el UPDATE del save incluye x/y).
        let reloaded = repo.load(id).await.expect("reload").expect("existe");
        assert_eq!(reloaded.x, 969600, "x durable via pipeline");
        assert_eq!(reloaded.y, 278400, "y durable via pipeline");
        assert_eq!(reloaded.skill_level.as_deref(), Some(&[0x01u8, 0x02][..]), "blob intacto");
        Ok::<(), String>(())
    }
    .await;

    // Cleanup SIEMPRE: personaje + schema.
    client
        .execute("DELETE FROM player.player WHERE name = $1", &[&name])
        .await
        .expect("cleanup player");
    client
        .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .await
        .expect("cleanup schema");
    result.expect("save_character contra PG real");
}
