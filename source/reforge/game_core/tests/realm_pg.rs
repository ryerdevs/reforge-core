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

async fn test_store() -> game_core::world::WorldStore {
    let pool = database::pool::new_pool(&pg_conn(), 4).expect("pool");
    let sink = database::wal::WalSink::new(
        database::wal::PgMutationSink::new(pool.clone()),
        game_core::world::wal_dir(),
    );
    let batcher = std::sync::Arc::new(database::wal::Batcher::spawn(
        std::time::Duration::from_millis(100),
        64,
        sink,
    ));
    game_core::world::WorldStore::new(pool, batcher)
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
    let store = test_store().await;
    let list = store.list_characters(TEST_ACCOUNT).await.expect("list");
    assert!(list.len() >= 3, ">=3 (E2E Q3): got {}", list.len());
    let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
    for expected in ["lkjsnlfknlsk", "ninja", "Chaman"] {
        assert!(
            names.contains(&expected),
            "lista contiene {expected}: {names:?}"
        );
    }
}

/// select_player sobre los slots reales de la cuenta test:
/// player_index pid1..pid5 = [1, 3, 5, 0, 2] (E2E Q1) -> slots 0,1,2,4
/// devuelven personaje; slot 3 (pid=0) -> None; x/y en UNITS.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package game_core -- --ignored"]
async fn realm_select_player_each_live_character() {
    let store = test_store().await;

    // Slot 0 -> pid1 = 1 (lkjsnlfknlsk).
    let p = store
        .select_player(TEST_ACCOUNT, 0)
        .await
        .expect("select slot 0")
        .expect("pid1=1 existe");
    assert_eq!(p.id, 1, "pid1 de la cuenta test");
    assert_eq!(p.name, "lkjsnlfknlsk", "E2E Q3");

    // Slot 1 -> pid2 = 3 (Chaman).
    let p = store
        .select_player(TEST_ACCOUNT, 1)
        .await
        .expect("select slot 1")
        .expect("pid2=3 existe");
    assert_eq!(p.id, 3, "pid2 de la cuenta test");

    // Slot 2 -> pid3 = 5.
    let p = store
        .select_player(TEST_ACCOUNT, 2)
        .await
        .expect("select slot 2")
        .expect("pid3=5 existe");
    assert_eq!(p.id, 5, "pid3 de la cuenta test");

    // Slot 3 -> pid4 = 0 (slot vacio) -> None.
    assert_eq!(
        store
            .select_player(TEST_ACCOUNT, 3)
            .await
            .expect("select slot 3"),
        None,
        "pid4=0 -> None (input_login.cpp:266-271)"
    );

    // Slot 4 -> pid5 = 2 (ninja, E2E Q2: job=1).
    let p = store
        .select_player(TEST_ACCOUNT, 4)
        .await
        .expect("select slot 4")
        .expect("pid5=2 existe");
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
    let store = test_store().await;
    let slots = store
        .account_slots(TEST_ACCOUNT)
        .await
        .expect("account_slots");
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
    let store = test_store().await;
    for slot in [5u8, 6, 200] {
        let err = store
            .select_player(TEST_ACCOUNT, slot)
            .await
            .expect_err("slot invalido -> Err");
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

    let pool = database::pool::new_pool(&conn, 4).expect("pool");
    let sink = database::wal::WalSink::new(
        database::wal::PgMutationSink::new(pool.clone()).with_audit_table(audit.clone()),
        game_core::world::wal_dir(),
    );
    let batcher = std::sync::Arc::new(database::wal::Batcher::spawn(
        std::time::Duration::from_millis(100),
        64,
        sink,
    ));
    let store = WorldStore::new(pool.clone(), batcher);
    let repo = PlayerRepo::new(pool);
    // Nombre UNICO por test (los tests del bin corren en paralelo).
    let name = format!("e2e_rsave_{}", ts());

    let result = async {
        // Crear el throwaway (id DEFAULT, regla B5) y cargar su row.
        let c = PlayerCreate {
            account_id: TEST_ACCOUNT,
            name: name.clone(),
            level: 1,
            st: 30,
            ht: 30,
            dx: 30,
            iq: 30,
            job: 0,
            voice: 0,
            dir: 0,
            x: 0,
            y: 0,
            z: 0,
            map_index: 41,
            hp: 100,
            mp: 100,
            random_hp: 0,
            random_sp: 0,
            stat_point: 0,
            stamina: 100,
            part_base: 0,
            part_main: 0,
            part_hair: 0,
            gold: 0,
            playtime: 0,
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
        assert_eq!(
            reloaded.skill_level.as_deref(),
            Some(&[0x01u8, 0x02][..]),
            "blob intacto"
        );
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

/// Rollback del create (fix gap-lane-E, verifier E-2): si `set_slot` falla,
/// la fila del player recién insertada se borra SIN el gate del índice
/// (parity `ClientManagerPlayer.cpp:901-907` — el C++ hace `DELETE FROM
/// player WHERE id=%d` incondicionalmente cuando el UPDATE del slot falla).
/// Con el rollback viejo (`delete()` con gate) la fila quedaba huérfana y el
/// nombre bloqueado para siempre (`name_exists` sin except_id).
///
/// Trigger determinista del fallo de `set_slot`: slot 5 inválido (fuera de
/// 0..4) → `index_col` falla ANTES del SQL — mismo camino de rollback que un
/// error real de PG, sin depender del esquema.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package game_core -- --ignored"]
async fn realm_create_character_rollback_deletes_player_row() {
    let store = test_store().await;
    let client = raw_client(&pg_conn()).await;
    // Cuenta THROWAWAY (lejos de la viva 1): el rollback ni el cleanup tocan
    // datos reales.
    let account = 990_000_002i64;
    let rollback_name = format!("e2e_rrollback_{}", ts());
    let ok_name = format!("e2e_rok_{}", ts());

    let result = async {
        let mk = |name: String| PlayerCreate {
            account_id: account,
            name,
            level: 1,
            st: 30,
            ht: 30,
            dx: 30,
            iq: 30,
            job: 0,
            voice: 0,
            dir: 0,
            x: 0,
            y: 0,
            z: 0,
            map_index: 41,
            hp: 100,
            mp: 100,
            random_hp: 0,
            random_sp: 0,
            stat_point: 0,
            stamina: 100,
            part_base: 0,
            part_main: 0,
            part_hair: 0,
            gold: 0,
            playtime: 0,
            skill_level: Vec::new(),
            quickslot: Vec::new(),
        };

        // 1. Rollback: create() inserta el player, set_slot(slot 5) falla →
        //    la fila insertada se borra (parity ClientManagerPlayer.cpp:901-907).
        let err = store
            .create_character(&mk(rollback_name.clone()), 5)
            .await
            .expect_err("slot 5 -> Err de set_slot");
        assert!(err.contains("fuera de rango"), "err: {err}");
        let orphans: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM player.player WHERE name = $1",
                &[&rollback_name],
            )
            .await
            .expect("count")
            .get(0);
        assert_eq!(
            orphans, 0,
            "rollback del create: la fila insertada se borra SIN el gate del índice"
        );

        // 2. Happy path intacto: create + set_slot con slot válido → pid, la
        //    fila existe y el borrado normal (con gate) la limpia.
        let pid = store
            .create_character(&mk(ok_name.clone()), 0)
            .await
            .expect("create con slot 0");
        let row = store
            .select_player(account, 0)
            .await
            .expect("select")
            .expect("existe");
        assert_eq!(row.id, pid, "el slot 0 apunta al pid nuevo");
        store
            .delete_character(account, 0, pid)
            .await
            .expect("delete normal (gate ok)");
        assert_eq!(
            store.select_player(account, 0).await.expect("select"),
            None,
            "delete normal limpia índice + fila"
        );
        Ok::<(), String>(())
    }
    .await;

    // Cleanup SIEMPRE (patrón trap del E2E): filas + fila de índice de la
    // cuenta throwaway (por si un fallo dejara residuo).
    let _ = client
        .execute(
            "DELETE FROM player.player WHERE name = $1",
            &[&rollback_name],
        )
        .await;
    let _ = client
        .execute("DELETE FROM player.player WHERE name = $1", &[&ok_name])
        .await;
    let _ = client
        .execute("DELETE FROM player.player_index WHERE id = $1", &[&account])
        .await;
    result.expect("rollback del create contra PG real");
}
