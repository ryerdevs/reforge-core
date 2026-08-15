//! Integration F3 (dominio account) contra PostgreSQL REAL — gated con
//! `#[ignore]`: no corre en el `cargo test` normal (requiere la PG del
//! entorno WSL, 127.0.0.1:5432 metin2/mt2).
//!
//! Ejecutar desde WSL (la PG vive en WSL; desde Windows 127.0.0.1 no llega):
//!
//! ```text
//! source ~/.cargo/env
//! cd /mnt/c/projects/Metin2/source/reforge
//! cargo test --package database -- --ignored
//! ```
//!
//! El conn string se puede sobreescribir con `DATABASE_TEST_PG` (por si el
//! entorno difiere). El test NO deja estado: guarda `lang`/`hwid` previos de
//! la cuenta `test` y los restaura siempre (aunque un assert falle).

use database::account::AccountRepo;

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";
/// Vector REAL del entorno: test / 1234 (AGENTS.md).
const TEST_HASH: &str = "*A4B6157319038724E3560894F7F932C8886EBFCF";

fn pg_conn() -> String {
    std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
}

/// SELECT directo de lang/hwid (para verificar persistencia y guardar previos).
async fn read_lang_hwid(conn: &str, login: &str) -> (String, String) {
    let (client, connection) = tokio_postgres::connect(conn, tokio_postgres::NoTls)
        .await
        .expect("PG connect");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let row = client
        .query_one(
            "SELECT lang, hwid FROM account.account WHERE login = $1",
            &[&login],
        )
        .await
        .expect("SELECT lang/hwid");
    (row.get(0), row.get(1))
}

async fn write_lang_hwid(conn: &str, login: &str, lang: &str, hwid: &str) {
    let (client, connection) = tokio_postgres::connect(conn, tokio_postgres::NoTls)
        .await
        .expect("PG connect");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
        .execute(
            "UPDATE account.account SET lang = $1, hwid = $2 WHERE login = $3",
            &[&lang, &hwid, &login],
        )
        .await
        .expect("UPDATE lang/hwid");
}

#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn account_login_contract_against_real_pg() {
    let conn = pg_conn();
    let repo = AccountRepo::new(database::pool::new_pool(&conn, 4).expect("pool PG"));

    // login valido -> Some con las 13 columnas correctas.
    let acc = repo
        .login("test", "1234")
        .await
        .expect("login no debe fallar (DB up)")
        .expect("test/1234 es valido");
    assert_eq!(acc.id, 1, "a.id");
    assert_eq!(acc.login, "test", "a.login");
    assert_eq!(acc.password_hash, TEST_HASH, "a.password (hash almacenado)");
    assert_eq!(acc.social_id, "1234567", "a.social_id");
    assert_eq!(acc.empire, Some(3), "pi.empire (cuenta test)");
    assert_eq!(
        acc.player_ids,
        [Some(1), Some(3), Some(5), Some(0), Some(2)],
        "pi.pid1..pid5 (cuenta test)"
    );
    assert_eq!(acc.status, "OK", "a.status");
    assert_eq!(acc.lang.len(), 2, "a.lang es ISO 2 chars (el valor exacto lo sobreescribe el cliente)");
    assert_eq!(acc.player_id(0), 1, "helper player_id");
    assert_eq!(acc.player_id(3), 0, "pid4=0 -> ranura vacia");

    // password mala -> None (semantica QUERY_LOGIN).
    assert_eq!(
        repo.login("test", "mala").await.expect("DB up"),
        None,
        "password incorrecta -> None"
    );

    // login inexistente -> None.
    assert_eq!(
        repo.login("no_existe_xyz", "1234").await.expect("DB up"),
        None,
        "login inexistente -> None"
    );
}

#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn account_set_lang_hwid_persist_and_restore() {
    let conn = pg_conn();
    let repo = AccountRepo::new(database::pool::new_pool(&conn, 4).expect("pool PG"));

    // Estado previo de la cuenta test (se restaura SIEMPRE al final).
    let (prev_lang, prev_hwid) = read_lang_hwid(&conn, "test").await;

    let result = async {
        repo.set_lang("test", "de").await.expect("set_lang");
        repo.set_hwid("test", "aabbccddeeff00112233445566778899")
            .await
            .expect("set_hwid");
        let (lang, hwid) = read_lang_hwid(&conn, "test").await;
        assert_eq!(lang, "de", "lang persistido");
        assert_eq!(hwid, "aabbccddeeff00112233445566778899", "hwid persistido (hex TEXT)");

        // login() sigue funcionando con el estado modificado.
        let acc = repo.login("test", "1234").await.expect("login").expect("valido");
        assert_eq!(acc.lang, "de", "login() ve el lang nuevo");
        Ok::<(), String>(())
    }
    .await;

    // Restaurar SIEMPRE (aunque el bloque falle) — no romper la cuenta test.
    write_lang_hwid(&conn, "test", &prev_lang, &prev_hwid).await;
    let (lang, hwid) = read_lang_hwid(&conn, "test").await;
    assert_eq!((lang.as_str(), hwid.as_str()), (prev_lang.as_str(), prev_hwid.as_str()), "estado restaurado");

    result.expect("set_lang/set_hwid contra PG real");
}
