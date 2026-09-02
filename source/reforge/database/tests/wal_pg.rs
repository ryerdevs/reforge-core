//! Integration F3 phase 2 (WAL) contra PostgreSQL REAL — gated con `#[ignore]`
//! (mismo patrón que el resto de la suite PG). La suite normal portable no
//! requiere PostgreSQL; estos tests corren con `--ignored` cuando hay una PG
//! real disponible (ver `scripts/verify.ps1` pata `--ignored`).
//!
//! Usa un schema de test `e2e_wal` (NO toca la PG viva: el DDL de produccion
//! `log.mutation_audit` lo aplica el harness de otro lane) y lo limpia
//! SIEMPRE (patron trap del E2E).

use database::wal::{Batcher, Mutation, Param, PgMutationSink, audit_ddl};
use std::time::Duration;

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";

fn pg_conn() -> String {
    std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
}

/// Schema UNICO por test (los tests del bin corren en paralelo — compartir
/// `e2e_wal` seria un race en el DROP/CREATE).
async fn setup(name: &str) -> (String, tokio_postgres::Client, String, String) {
    let conn = pg_conn();
    let schema = format!("e2e_wal_{name}");
    let audit = format!("{schema}.mutation_audit");
    let replay = format!("{schema}.replay_test");
    let (client, connection) = tokio_postgres::connect(&conn, tokio_postgres::NoTls)
        .await
        .expect("PG connect");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}; \
             CREATE TABLE {replay} (id bigint PRIMARY KEY, val text NOT NULL); \
             {audit_ddl}",
            audit_ddl = audit_ddl(&audit),
        ))
        .await
        .expect("setup schema de test");
    (conn, client, replay, audit)
}

async fn count(client: &tokio_postgres::Client, sql: &str) -> i64 {
    let row = client.query_one(sql, &[]).await.expect("count");
    row.get(0)
}

/// Replay idempotente end-to-end: la misma mutation (mismo `mutation_id`)
/// aplicada 2x -> 1 fila en la tabla de negocio Y 1 fila en el audit.
/// Cleanup SIEMPRE (trap): aunque un assert falle, el schema se borra.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn wal_replay_idempotent_and_audit_same_tx() {
    let (conn, client, replay, audit) = setup("replay").await;

    let result = async {
        let sink = PgMutationSink::new(database::pool::new_pool(&conn, 4).expect("pool"))
            .with_audit_table(audit.clone());
        let batcher = Batcher::spawn(Duration::from_millis(50), 16, sink);

        let sql = format!("INSERT INTO {replay} (id, val) VALUES ($1, $2) ON CONFLICT DO NOTHING");
        // Tres mutations distintas + la primera re-aplicada (mismo id).
        let m1 = Mutation::new(&sql, vec![Param::Int(1), Param::Text("uno".into())]);
        let m2 = Mutation::new(&sql, vec![Param::Int(2), Param::Text("dos".into())]);
        let m3 = Mutation::new(&sql, vec![Param::Int(3), Param::Text("tres".into())]);
        let m1_replay = Mutation::with_id(m1.id, &sql, m1.params.clone());

        batcher.push(m1.clone());
        batcher.push(m2);
        batcher.push(m3);
        batcher.push(m1_replay); // replay de m1

        // Espera el flush: poll del contador (el sink abre conexion PG nueva
        // en el apply — un sleep fijo flakea en WSL cuando la conexion tarda).
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let n = count(&client, &format!("SELECT COUNT(*) FROM {audit}")).await;
            if n >= 3 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timeout esperando el flush (audit={n})"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // La tabla de negocio: 3 filas (no 4) — el replay no duplica.
        assert_eq!(
            count(&client, &format!("SELECT COUNT(*) FROM {replay}")).await,
            3,
            "3 mutations distintas"
        );
        // El audit: 3 filas — el mutation_id repetido se ignora (pk).
        assert_eq!(
            count(&client, &format!("SELECT COUNT(*) FROM {audit}")).await,
            3,
            "audit 3 (pk mutation_id)"
        );
        // Payload presente y con el sql (payload TEXT con json valido).
        let row = client
            .query_one(
                &format!("SELECT payload FROM {audit} WHERE mutation_id = $1"),
                &[&uuid::Uuid::from_bytes(m1.id)],
            )
            .await
            .expect("audit row m1");
        let payload: String = row.get(0);
        assert!(
            payload.contains(&format!("\"sql\":\"INSERT INTO {replay}")),
            "payload del audit: {payload}"
        );
        // Los valores quedaron aplicados.
        assert_eq!(
            count(
                &client,
                "SELECT COUNT(*) FROM e2e_wal_replay.replay_test WHERE id = 1"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &client,
                "SELECT COUNT(*) FROM e2e_wal_replay.replay_test WHERE id = 3"
            )
            .await,
            1
        );
        Ok::<(), String>(())
    }
    .await;

    // Cleanup SIEMPRE.
    client
        .batch_execute("DROP SCHEMA IF EXISTS e2e_wal_replay CASCADE")
        .await
        .expect("cleanup e2e_wal_replay");
    result.expect("wal replay contra PG real");
}

/// El batch falla como UNA transaccion: si una mutation es invalida, NINGUNA
/// del batch se aplica (rollback total) y el audit queda vacio.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn wal_batch_rolls_back_entirely_on_error() {
    let (conn, client, replay, audit) = setup("rollback").await;

    let result = async {
        let sink = PgMutationSink::new(database::pool::new_pool(&conn, 4).expect("pool"))
            .with_audit_table(audit.clone());
        let batcher = Batcher::spawn(Duration::from_millis(50), 16, sink);

        // Mutation valida seguida de una INVALIDA (tabla no existe).
        batcher.push(Mutation::new(
            format!("INSERT INTO {replay} (id, val) VALUES ($1, $2) ON CONFLICT DO NOTHING"),
            vec![Param::Int(9), Param::Text("ok".into())],
        ));
        batcher.push(Mutation::new(
            "INSERT INTO e2e_wal_rollback.no_such_table (id) VALUES ($1)",
            vec![Param::Int(1)],
        ));

        tokio::time::sleep(Duration::from_millis(300)).await;

        // Rollback total: la mutation valida NO se aplico.
        assert_eq!(
            count(&client, &format!("SELECT COUNT(*) FROM {replay}")).await,
            0,
            "rollback de todo el batch"
        );
        assert_eq!(
            count(&client, &format!("SELECT COUNT(*) FROM {audit}")).await,
            0,
            "audit vacio (misma tx)"
        );
        Ok::<(), String>(())
    }
    .await;

    client
        .batch_execute("DROP SCHEMA IF EXISTS e2e_wal_rollback CASCADE")
        .await
        .expect("cleanup e2e_wal_rollback");
    result.expect("wal rollback contra PG real");
}
