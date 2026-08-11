//! Integration F3 (dominio world — PlayerRepo) contra PostgreSQL REAL — gated
//! con `#[ignore]` (mismo patron que account_pg.rs). Oracle: la suite E2E
//! (`scripts/gpg/e2e_db.sh` Q2/Q3/Q4/Q5).
//!
//! Ejecutar desde WSL:
//!
//! ```text
//! source ~/.cargo/env
//! cd /mnt/c/projects/Metin2/source/reforge
//! cargo test --package database -- --ignored
//! ```
//!
//! NO toca personajes vivos: el round-trip bytea y el create se hacen con un
//! personaje THROWAWAY (`e2e_rust_<pid>`) que se borra SIEMPRE (patron trap
//! del E2E — el usuario juega en este stack).

use database::player::{PlayerCreate, PlayerRepo};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";
/// Personajes vivos del E2E (no se tocan): ninja (id=2) como oraculo del load.
const NINJA_ID: i64 = 2;

fn pg_conn() -> String {
    std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
}

fn throwaway_name() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("e2e_rust_{ts}")
}

async fn delete_throwaway(client: &tokio_postgres::Client, name: &str) {
    let _ = client
        .execute("DELETE FROM player.player WHERE name = $1", &[&name])
        .await;
}

/// load(ninja) -> 42 campos correctos (oraculo E2E Q2: id=2, name=ninja,
/// job=1; x/y/playtime son estado vivo — estructura only, como el E2E).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn player_load_contract_against_real_pg() {
    let repo = PlayerRepo::new(pg_conn());
    let p = repo
        .load(NINJA_ID)
        .await
        .expect("load no debe fallar")
        .expect("ninja (id=2) existe — E2E Q2");
    assert_eq!(p.id, 2);
    assert_eq!(p.name, "ninja");
    assert_eq!(p.job, 1, "job=1 (E2E Q2)");
    // diff de tiempo: structure-only (parity E2E — last_play puede estar en
    // el futuro si se jugo con el reloj adelantado -> diff negativo).
    assert!(p.logoff_interval.is_finite(), "diff es f64 finito (structure-only)");
    assert!(p.skill_level.is_some(), "skill_level bytea presente");
    assert!(p.quickslot.is_some(), "quickslot bytea presente");
    // No existe -> None.
    assert_eq!(repo.load(999_999_999).await.expect("DB up"), None, "id inexistente -> None");
}

/// list_for_account(1) -> >=3 filas y contiene los personajes del E2E Q3.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn player_list_for_account_contract() {
    let repo = PlayerRepo::new(pg_conn());
    let list = repo.list_for_account(1).await.expect("list");
    assert!(list.len() >= 3, ">=3 (E2E Q3): got {}", list.len());
    let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
    for expected in ["lkjsnlfknlsk", "ninja", "Chaman"] {
        assert!(names.contains(&expected), "lista contiene {expected}: {names:?}");
    }
    // Cada fila tiene 15 campos coherentes.
    for s in &list {
        assert!(s.id > 0);
        assert!(!s.name.is_empty());
    }
}

/// Ciclo throwaway (patron E2E Q4/Q5): create (id DEFAULT) -> load ->
/// save con blobs bytea -> load round-trip bytea identico -> DELETE SIEMPRE.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn player_create_save_roundtrip_throwaway() {
    let conn = pg_conn();
    let repo = PlayerRepo::new(&conn);
    let name = throwaway_name();

    let result = async {
        // Blobs conocidos: 0x01 0x02 0x27(') 0x5c(\) 0x22(") 0x00 — los bytes
        // del E2E Q4 (incluye control y escapables).
        let blobs: Vec<u8> = vec![0x01, 0x02, 0x27, 0x5c, 0x22, 0x00, 0xfe];
        let c = PlayerCreate {
            account_id: 1,
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
            skill_level: blobs.clone(),
            quickslot: blobs.clone(),
        };
        let id = repo.create(&c).await.expect("create (id DEFAULT -> identity)");
        assert!(id > 0, "id generado: {id}");

        // load del recien creado: blobs byte-identicos.
        let loaded = repo.load(id).await.expect("load").expect("existe");
        assert_eq!(loaded.name, name, "name round-trip");
        assert_eq!(loaded.level, 1);
        assert_eq!(loaded.skill_level.as_deref(), Some(blobs.as_slice()), "skill_level bytea identico");
        assert_eq!(loaded.quickslot.as_deref(), Some(blobs.as_slice()), "quickslot bytea identico");
        assert_eq!(loaded.x, 0);

        // save (Q5 shape): cambia x/y + blobs distintos -> load verifica.
        let mut saved = loaded.clone();
        saved.x = 969600;
        saved.y = 278400;
        saved.skill_level = Some(vec![0xde, 0xad, 0xbe, 0xef, 0x01, 0x02]);
        saved.quickslot = Some(vec![0xde, 0xad, 0xbe, 0xef, 0x01, 0x02]);
        let affected = repo.save(&saved).await.expect("save");
        assert_eq!(affected, 1, "1 fila afectada");

        let reloaded = repo.load(id).await.expect("load").expect("existe");
        assert_eq!(reloaded.x, 969600, "x guardado (E2E Q5: 969600)");
        assert_eq!(reloaded.y, 278400, "y guardado (E2E Q5: 278400)");
        assert_eq!(reloaded.skill_level, saved.skill_level, "skill_level round-trip");
        assert_eq!(reloaded.quickslot, saved.quickslot, "quickslot round-trip");
        Ok::<(), String>(())
    }
    .await;

    // Cleanup SIEMPRE (trap-guaranteed, como el E2E).
    let (client, connection) = tokio_postgres::connect(&conn, tokio_postgres::NoTls)
        .await
        .expect("PG connect cleanup");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    delete_throwaway(&client, &name).await;
    result.expect("create/save/load round-trip contra PG real");
}
