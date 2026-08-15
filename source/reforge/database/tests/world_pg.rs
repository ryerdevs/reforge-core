//! Integration F3 (dominio world — quest/affect/safebox/item/messenger +
//! pipeline durable) contra PostgreSQL REAL — gated con `#[ignore]` (mismo
//! patron que account_pg.rs / player_pg.rs / wal_pg.rs).
//!
//! Oracle: la suite E2E (`scripts/gpg/e2e_db.sh` Q6/Q8) + los fuentes C++
//! citados en cada contrato.
//!
//! Ejecutar desde WSL (la PG vive en WSL):
//!
//! ```text
//! source ~/.cargo/env
//! cd /mnt/c/projects/Metin2/source/reforge
//! cargo test --package database -- --ignored
//! ```
//!
//! Reglas: SOLO lecturas sobre datos vivos (personajes reales — asserts
//! structure-only documentados) + inserts THROWAWAY con cleanup SIEMPRE
//! (patron trap del E2E — el usuario juega en este stack). El pipeline
//! durable usa schemas `e2e_wal_*` temporales (nunca `log.mutation_audit`).

use database::affect::{AffectRepo, AffectRow};
use database::item::{ItemExchange, ItemRepo, ItemRow};
use database::messenger::MessengerRepo;
use database::player::{PlayerCreate, PlayerRepo};
use database::quest::{QuestRepo, QuestRow};
use database::safebox::SafeboxRepo;
use database::wal::{audit_ddl, Batcher, PgMutationSink};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";
/// Personaje vivo del E2E (no se toca): ninja (id=2).
const NINJA_ID: i64 = 2;
/// Pids/accounts THROWAWAY (lejos de los ids vivos 1..5).
const THROWAWAY_ID: i64 = 990_000_001;

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

/// Poll del contador de audit hasta `expected` (el sink abre conexion PG
/// nueva en cada apply; un sleep fijo flakea en WSL si la conexion tarda —
/// mismo patron wait_for_batches de los unit tests).
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

// ---------------------------------------------------------------------------
// quest / affect — loads sobre personajes vivos (structure-only)
// ---------------------------------------------------------------------------

/// Loads de quest/affect de ninja (E2E Q6): hoy 0 filas (personajes frescos);
/// el assert es structure-only — el contrato es que NO falle y que cada fila
/// round-tripee sus campos.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn quest_affect_load_live_characters_structure_only() {
    let conn = pg_conn();
    let quests = QuestRepo::new(&conn).load(NINJA_ID).await.expect("QUEST_LOAD no falla");
    for q in &quests {
        assert_eq!(q.dw_pid, NINJA_ID, "dwPID del row");
        assert!(!q.sz_name.is_empty(), "szName presente");
    }
    let affects = AffectRepo::new(&conn).load(NINJA_ID).await.expect("AFFECT_LOAD no falla");
    for a in &affects {
        assert_eq!(a.dw_pid, NINJA_ID, "dwPID del row");
    }
    // Ids inexistentes -> vec vacio (sin error).
    assert!(QuestRepo::new(&conn).load(999_999_999).await.expect("DB up").is_empty());
    assert!(AffectRepo::new(&conn).load(999_999_999).await.expect("DB up").is_empty());
}

// ---------------------------------------------------------------------------
// quest / affect — round-trip throwaway con cleanup SIEMPRE
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn quest_save_upsert_roundtrip_throwaway() {
    let conn = pg_conn();
    let repo = QuestRepo::new(&conn);

    let result = async {
        // Insert de 2 quests (semantica QUERY_QUEST_SAVE: lValue!=0 -> upsert).
        repo.save(&[
            QuestRow { dw_pid: THROWAWAY_ID, sz_name: "q1".into(), sz_state: "s1".into(), l_value: 5 },
            QuestRow { dw_pid: THROWAWAY_ID, sz_name: "q2".into(), sz_state: "s2".into(), l_value: 7 },
        ])
        .await
        .expect("save insert");
        let rows = repo.load(THROWAWAY_ID).await.expect("load");
        assert_eq!(rows.len(), 2, "2 quests insertados");
        assert!(rows.iter().any(|r| r.sz_name == "q1" && r.l_value == 5));

        // Upsert (mismo PK, lValue nuevo) + delete (lValue 0).
        repo.save(&[
            QuestRow { dw_pid: THROWAWAY_ID, sz_name: "q1".into(), sz_state: "s1".into(), l_value: 9 },
            QuestRow { dw_pid: THROWAWAY_ID, sz_name: "q2".into(), sz_state: "s2".into(), l_value: 0 },
        ])
        .await
        .expect("save upsert+delete");
        let rows = repo.load(THROWAWAY_ID).await.expect("load");
        assert_eq!(rows.len(), 1, "q2 borrado (lValue 0), q1 actualizado");
        assert_eq!(rows[0].l_value, 9, "upsert sobre el PK (REPLACE parity)");
        Ok::<(), String>(())
    }
    .await;

    let client = raw_client(&conn).await;
    client
        .execute("DELETE FROM player.quest WHERE dwPID = $1", &[&THROWAWAY_ID])
        .await
        .expect("cleanup quest");
    result.expect("quest round-trip contra PG real");
}

#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn affect_save_upsert_remove_roundtrip_throwaway() {
    let conn = pg_conn();
    let repo = AffectRepo::new(&conn);

    let result = async {
        let row = AffectRow {
            dw_pid: THROWAWAY_ID,
            b_type: 1,
            b_apply_on: 2,
            l_apply_value: 3,
            dw_flag: 4,
            l_duration: 5,
            l_sp_cost: 6,
        };
        assert_eq!(repo.save(&row).await.expect("save"), 1, "1 fila escrita");
        let rows = repo.load(THROWAWAY_ID).await.expect("load");
        assert_eq!(rows, vec![row.clone()], "round-trip identico");

        // Upsert sobre el mismo PK (REPLACE parity) con valores nuevos.
        let mut changed = row.clone();
        changed.l_duration = 99;
        assert_eq!(repo.save(&changed).await.expect("upsert"), 1);
        let rows = repo.load(THROWAWAY_ID).await.expect("load");
        assert_eq!(rows.len(), 1, "sigue siendo 1 fila (mismo PK)");
        assert_eq!(rows[0].l_duration, 99, "valor actualizado");

        // Remove (QUERY_REMOVE_AFFECT).
        assert_eq!(repo.remove(THROWAWAY_ID, 1, 2).await.expect("remove"), 1);
        assert!(repo.load(THROWAWAY_ID).await.expect("load").is_empty(), "afecto borrado");
        Ok::<(), String>(())
    }
    .await;

    let client = raw_client(&conn).await;
    client
        .execute("DELETE FROM player.affect WHERE dwPID = $1", &[&THROWAWAY_ID])
        .await
        .expect("cleanup affect");
    result.expect("affect round-trip contra PG real");
}

// ---------------------------------------------------------------------------
// safebox — size structure-only + round-trip throwaway
// ---------------------------------------------------------------------------

/// size() sobre la cuenta viva `test`: structure-only (hoy sin fila -> None,
/// pero el usuario puede abrir un safebox en cualquier momento).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn safebox_size_live_account_structure_only() {
    let repo = SafeboxRepo::new(pg_conn());
    let _ = repo.size(1).await.expect("SAFEBOX_SIZE no falla");
    let _ = repo.load(1).await.expect("SAFEBOX_LOAD no falla");
}

/// set_size parity del C++: size==1 INSERT (crea la fila), size!=1 UPDATE.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn safebox_set_size_roundtrip_throwaway() {
    let conn = pg_conn();
    let repo = SafeboxRepo::new(&conn);

    let result = async {
        assert_eq!(repo.size(THROWAWAY_ID).await.expect("size"), None, "sin fila todavia");
        // size==1 -> INSERT (primera pagina del safebox, parity C++).
        assert_eq!(repo.set_size(THROWAWAY_ID, 1).await.expect("set_size 1"), 1);
        assert_eq!(repo.size(THROWAWAY_ID).await.expect("size"), Some(1), "fila creada");
        // size!=1 -> UPDATE sobre la fila existente.
        assert_eq!(repo.set_size(THROWAWAY_ID, 4).await.expect("set_size 4"), 1);
        let sb = repo.load(THROWAWAY_ID).await.expect("load").expect("existe");
        assert_eq!(sb.size, 4, "UPDATE aplicado");
        assert_eq!(sb.account_id, THROWAWAY_ID);
        // size==1 con la fila YA existente -> no-op idempotente (0 filas; el
        // INSERT plano del C++ daria 23505 en legacy) — el replay del WAL no
        // duplica la fila ni resetea el tamaño.
        assert_eq!(repo.set_size(THROWAWAY_ID, 1).await.expect("set_size 1 no-op"), 0);
        assert_eq!(repo.size(THROWAWAY_ID).await.expect("size"), Some(4), "el valor previo se conserva");
        // set_gold (QUERY_SAFEBOX_SAVE).
        assert_eq!(repo.set_gold(THROWAWAY_ID, 12345).await.expect("set_gold"), 1);
        Ok::<(), String>(())
    }
    .await;

    let client = raw_client(&conn).await;
    client
        .execute("DELETE FROM player.safebox WHERE account_id = $1", &[&THROWAWAY_ID])
        .await
        .expect("cleanup safebox");
    result.expect("safebox round-trip contra PG real");
}

// ---------------------------------------------------------------------------
// item — load vivo structure-only + upsert/delete throwaway
// ---------------------------------------------------------------------------

/// Load del inventario de ninja (E2E Q2/Q6): 22 items hoy; el assert de
/// cantidad es structure-only — el contrato duro es la estructura de cada
/// fila (window del set, sockets/attrs coherentes).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn item_load_ninja_inventory_structure_only() {
    let repo = ItemRepo::new(pg_conn());
    let items = repo.load_by_owner(NINJA_ID).await.expect("ITEM_LOAD no falla");
    assert!(!items.is_empty(), "ninja tiene items (hoy 22)");
    for it in &items {
        assert!(
            matches!(it.window.as_str(), "INVENTORY" | "EQUIPMENT" | "DRAGON_SOUL_INVENTORY" | "BELT_INVENTORY"),
            "window del set QID_ITEM: {}",
            it.window
        );
        assert!(it.vnum > 0, "vnum presente");
        assert!(it.id > 0, "id presente");
    }
    // Probe del rango ITEM_ID_RANGE (E2E Q8): los ids vivos estan fuera del
    // rango 100M-200M (hoy max 50000005) — estructura-only.
    let _ = repo.max_id_in_range(100_000_000, 200_000_000).await.expect("probe no falla");
}

/// Upsert (id explicito del rango + DEFAULT) y delete — throwaway, cleanup SIEMPRE.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn item_upsert_delete_roundtrip_throwaway() {
    let conn = pg_conn();
    let repo = ItemRepo::new(&conn);
    let explicit_id = 100_000_001i64; // dentro de ITEM_ID_RANGE (E2E Q8)

    let result = async {
        // INSERT con id explicito del rango.
        let mut it = ItemRow {
            id: explicit_id,
            window: "INVENTORY".into(),
            pos: 0,
            count: 1,
            vnum: 27001,
            sockets: [0, 0, 0],
            attrs: [(0, 0); 7],
        };
        assert_eq!(repo.upsert(&it, THROWAWAY_ID).await.expect("upsert insert"), explicit_id);
        // El owner es throwaway: el load lo ve (no hay FK a player).
        let items = repo.load_by_owner(THROWAWAY_ID).await.expect("load");
        assert_eq!(items.len(), 1, "1 item del owner throwaway");
        assert_eq!(items[0].vnum, 27001);
        assert_eq!(items[0].window, "INVENTORY");

        // UPDATE sobre el mismo id (INSERT..ON DUPLICATE KEY UPDATE parity).
        it.pos = 3;
        it.count = 7;
        it.sockets[0] = 0xdead;
        it.attrs[0] = (1, 100);
        assert_eq!(repo.upsert(&it, THROWAWAY_ID).await.expect("upsert update"), explicit_id);
        let items = repo.load_by_owner(THROWAWAY_ID).await.expect("load");
        assert_eq!(items.len(), 1, "sigue siendo 1 (upsert)");
        assert_eq!(items[0].pos, 3);
        assert_eq!(items[0].count, 7);
        assert_eq!(items[0].sockets[0], 0xdead);
        assert_eq!(items[0].attrs[0], (1, 100), "attr round-trip");

        // INSERT con id=0 -> DEFAULT (identity BY DEFAULT, regla B5).
        let gen_item = ItemRow {
            id: 0,
            window: "EQUIPMENT".into(),
            pos: 0,
            count: 1,
            vnum: 19001,
            sockets: [0, 0, 0],
            attrs: [(0, 0); 7],
        };
        let gen_id = repo.upsert(&gen_item, THROWAWAY_ID).await.expect("upsert DEFAULT");
        assert!(gen_id > 0, "id generado: {gen_id}");
        assert_ne!(gen_id, explicit_id, "no colisiona con el explicito");

        // DELETE (QUERY_ITEM_DESTROY).
        assert_eq!(repo.delete(explicit_id).await.expect("delete"), 1);
        assert_eq!(repo.delete(gen_id).await.expect("delete 2"), 1);
        assert!(repo.load_by_owner(THROWAWAY_ID).await.expect("load").is_empty(), "owner limpio");
        Ok::<(), String>(())
    }
    .await;

    let client = raw_client(&conn).await;
    client
        .execute("DELETE FROM player.item WHERE owner_id = $1", &[&THROWAWAY_ID])
        .await
        .expect("cleanup item por owner");
    client
        .execute("DELETE FROM player.item WHERE id = $1", &[&explicit_id])
        .await
        .expect("cleanup item por id");
    result.expect("item round-trip contra PG real");
}

// ---------------------------------------------------------------------------
// item_award — pending load + take (throwaway)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn item_award_load_pending_and_take_throwaway() {
    let conn = pg_conn();
    let repo = ItemRepo::new(&conn);
    let login = format!("m2e2_{}", ts() % 1_000_000_000);

    let result = async {
        // 2 awards pendientes throwaway (INSERT directo, id DEFAULT).
        let client = raw_client(&conn).await;
        client
            .batch_execute(&format!(
                "INSERT INTO player.item_award (login, vnum, count, why) VALUES \
                 ('{login}', 27001, 1, 'e2e_rust_why'), \
                 ('{login}', 27002, 2, 'e2e_rust_why')"
            ))
            .await
            .expect("insert awards");

        // load_pending(0): los pendientes (hoy solo los nuestros).
        let pending = repo.load_pending_awards(0).await.expect("load pending");
        let ours: Vec<_> = pending.iter().filter(|a| a.login == login).collect();
        assert_eq!(ours.len(), 2, "2 awards del login throwaway");
        assert!(ours.iter().all(|a| a.vnum == 27001 || a.vnum == 27002));
        assert_eq!(ours[0].why.as_deref(), Some("e2e_rust_why"), "why round-trip");

        // take: marca taken_time + item_id; idempotente (AND taken_time IS NULL).
        let award_id = ours[0].id;
        assert_eq!(repo.take_award(award_id, 100_000_001).await.expect("take"), 1);
        assert_eq!(repo.take_award(award_id, 100_000_001).await.expect("take 2x"), 0, "idempotente");
        let pending = repo.load_pending_awards(0).await.expect("load pending");
        let ours: Vec<_> = pending.iter().filter(|a| a.login == login).collect();
        assert_eq!(ours.len(), 1, "solo queda el no tomado");
        assert_ne!(ours[0].id, award_id);
        Ok::<(), String>(())
    }
    .await;

    let client = raw_client(&conn).await;
    client
        .execute("DELETE FROM player.item_award WHERE login = $1", &[&login])
        .await
        .expect("cleanup item_award");
    result.expect("item_award contra PG real");
}

// ---------------------------------------------------------------------------
// messenger — add/list/remove throwaway
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn messenger_add_list_remove_throwaway() {
    let conn = pg_conn();
    let repo = MessengerRepo::new(&conn);
    // varchar(16) en PG: login corto.
    let account = format!("m2e2_{}", ts() % 1_000_000_000);
    let companion = format!("m2e2c_{}", ts() % 1_000_000_000);

    let result = async {
        assert!(repo.list(&account).await.expect("list vacio").is_empty(), "sin amigos");
        assert_eq!(repo.add(&account, &companion).await.expect("add"), 1);
        let list = repo.list(&account).await.expect("list");
        assert_eq!(list.len(), 1, "1 amigo");
        assert_eq!(list[0].account, account);
        assert_eq!(list[0].companion, companion);
        // Idempotente (ON CONFLICT (account, companion) DO NOTHING): un par ya
        // existente es un no-op — 0 filas y 1 sola fila en la tabla (el replay
        // del WAL no duplica). El game comprueba duplicados antes (parity C++),
        // asi que el caso legitimo nunca llega aqui.
        assert_eq!(repo.add(&account, &companion).await.expect("add dup"), 0, "no-op idempotente");
        assert_eq!(repo.list(&account).await.expect("list").len(), 1, "sigue 1 fila (no duplicado)");
        assert_eq!(repo.remove(&account, &companion).await.expect("remove"), 1);
        assert!(repo.list(&account).await.expect("list").is_empty(), "borrado");
        Ok::<(), String>(())
    }
    .await;

    let client = raw_client(&conn).await;
    client
        .execute("DELETE FROM player.messenger_list WHERE account = $1", &[&account])
        .await
        .expect("cleanup messenger");
    result.expect("messenger contra PG real");
}

// ---------------------------------------------------------------------------
// TAREA 2 — pipeline durable: save_mutated -> Batcher -> sink (audit misma tx)
// ---------------------------------------------------------------------------

/// Setup del schema de test del pipeline (patron wal_pg.rs): schema unico por
/// test + tabla de audit. Devuelve (conn, client, audit_table).
async fn pipeline_setup(name: &str) -> (String, tokio_postgres::Client, String) {
    let conn = pg_conn();
    let schema = format!("e2e_wal_{name}");
    let audit = format!("{schema}.mutation_audit");
    let client = raw_client(&conn).await;
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}; {}",
            audit_ddl(&audit)
        ))
        .await
        .expect("setup schema de test");
    (conn, client, audit)
}

/// save de un personaje throwaway a traves del pipeline: fila + audit en la
/// MISMA transaccion (ADR-0008: durable = batch transaccional <=100ms).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn player_save_mutated_via_batcher_applies_with_audit() {
    let (conn, client, audit) = pipeline_setup("pmut").await;
    let repo = PlayerRepo::new(&conn);
    // Nombre UNICO por test (los tests del bin corren en paralelo y
    // player_pg.rs usa e2e_rust_<ts> — un prefijo compartido haria que los
    // cleanups se borraran los personajes entre si).
    let name = format!("e2e_pmut_{}", ts());

    let result = async {
        // Crear el throwaway (id DEFAULT) — write directo, no pipeline.
        let blobs: Vec<u8> = vec![0x01, 0x02, 0x27, 0x5c, 0x22, 0x00, 0xfe];
        let c = PlayerCreate {
            account_id: 1,
            name: name.clone(),
            level: 1,
            st: 30, ht: 30, dx: 30, iq: 30,
            job: 0, voice: 0, dir: 0,
            x: 0, y: 0, z: 0,
            hp: 100, mp: 100,
            random_hp: 0, random_sp: 0, stat_point: 0, stamina: 100,
            part_base: 0, part_main: 0, part_hair: 0,
            gold: 0, playtime: 0,
            skill_level: blobs.clone(),
            quickslot: blobs.clone(),
        };
        let id = repo.create(&c).await.expect("create");

        // save_mutated a traves del Batcher real (sink PG + audit e2e).
        let mut p = repo.load(id).await.expect("load").expect("existe");
        p.x = 969600;
        p.y = 278400;
        let sink = PgMutationSink::new(&conn).with_audit_table(audit.clone());
        let batcher = Batcher::spawn(Duration::from_millis(100), 64, sink);
        repo.save_mutated(&batcher, &p);
        // Espera el flush: poll del audit (el sink abre conexion nueva).
        wait_for_audit(&client, &audit, 1).await;

        // La fila quedo aplicada.
        let reloaded = repo.load(id).await.expect("reload").expect("existe");
        assert_eq!(reloaded.x, 969600, "x durable");
        assert_eq!(reloaded.y, 278400, "y durable");

        // El audit tiene la mutation (misma tx que el UPDATE).
        let count: i64 = client
            .query_one(&format!("SELECT COUNT(*) FROM {audit}"), &[])
            .await
            .expect("count audit")
            .get(0);
        assert_eq!(count, 1, "1 mutation auditada");
        Ok::<(), String>(())
    }
    .await;

    // Cleanup SIEMPRE: personaje + schema de test.
    client
        .execute("DELETE FROM player.player WHERE name = $1", &[&name])
        .await
        .expect("cleanup player");
    client
        .batch_execute("DROP SCHEMA IF EXISTS e2e_wal_pmut CASCADE")
        .await
        .expect("cleanup schema");
    result.expect("save_mutated contra PG real");
}

/// 2 saves en <100ms -> 1 batch -> audit con 2 filas y el MISMO applied_at
/// (now() es el timestamp de la transaccion: misma tx == mismo batch).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn player_two_saves_same_batch_same_tx() {
    let (conn, client, audit) = pipeline_setup("pbatch").await;
    let repo = PlayerRepo::new(&conn);
    let name = format!("e2e_pbatch_{}", ts());

    let result = async {
        let c = PlayerCreate {
            account_id: 1,
            name: name.clone(),
            level: 1,
            st: 30, ht: 30, dx: 30, iq: 30,
            job: 0, voice: 0, dir: 0,
            x: 0, y: 0, z: 0,
            hp: 100, mp: 100,
            random_hp: 0, random_sp: 0, stat_point: 0, stamina: 100,
            part_base: 0, part_main: 0, part_hair: 0,
            gold: 0, playtime: 0,
            skill_level: vec![0x01],
            quickslot: vec![0x01],
        };
        let id = repo.create(&c).await.expect("create");
        let p = repo.load(id).await.expect("load").expect("existe");

        let sink = PgMutationSink::new(&conn).with_audit_table(audit.clone());
        let batcher = Batcher::spawn(Duration::from_millis(100), 64, sink);
        // Dos saves back-to-back: caen en la misma ventana de 100ms.
        let mut a = p.clone();
        a.x = 969600;
        repo.save_mutated(&batcher, &a);
        let mut b = p.clone();
        b.x = 278400;
        repo.save_mutated(&batcher, &b);
        wait_for_audit(&client, &audit, 2).await;

        // El ultimo save gana (orden preservado dentro del batch).
        let reloaded = repo.load(id).await.expect("reload").expect("existe");
        assert_eq!(reloaded.x, 278400, "last-write-wins");

        // Audit: 2 filas, misma transaccion (mismo applied_at).
        let rows = client
            .query(&format!("SELECT mutation_id, applied_at FROM {audit} ORDER BY mutation_id"), &[])
            .await
            .expect("audit rows");
        assert_eq!(rows.len(), 2, "2 mutations auditadas");
        let t0: std::time::SystemTime = rows[0].get(1);
        let t1: std::time::SystemTime = rows[1].get(1);
        assert_eq!(t0, t1, "mismo applied_at -> misma tx -> mismo batch");
        Ok::<(), String>(())
    }
    .await;

    client
        .execute("DELETE FROM player.player WHERE name = $1", &[&name])
        .await
        .expect("cleanup player");
    client
        .batch_execute("DROP SCHEMA IF EXISTS e2e_wal_pbatch CASCADE")
        .await
        .expect("cleanup schema");
    result.expect("2 saves en 1 batch contra PG real");
}

// ---------------------------------------------------------------------------
// Unidad ACID (ADR-0011): materiales → resultado → oro en UNA tx (F5 skeleton)
// ---------------------------------------------------------------------------

/// End-to-end de la unidad ACID contra PG real: crea un player throwaway +
/// 2 items material + aplica `exchange_mutated` (Batcher + PgMutationSink con
/// audit e2e) — la unidad entera commit en UNA transaccion: counts de los
/// materiales decrementados, resultado creado, oro actualizado, audit con
/// TODAS las mutations (mismo applied_at = misma tx). Cleanup SIEMPRE.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn item_exchange_atomic_unit_against_real_pg() {
    let conn = pg_conn();
    let client = raw_client(&conn).await;
    // Schema e2e propio (patron pipeline_setup).
    let schema = "e2e_wal_xchg";
    let audit = format!("{schema}.mutation_audit");
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}; {}",
            audit_ddl(&audit)
        ))
        .await
        .expect("setup schema de test");

    // Ids THROWAWAY dentro del rango ITEM_ID_RANGE (100M-200M, E2E Q8).
    const MAT_A: i64 = 100_000_101;
    const MAT_B: i64 = 100_000_102;
    const RESULT_ID: i64 = 100_000_103;

    let result = async {
        // Player throwaway (sin FK a account; el id lo da la identity).
        let repo = PlayerRepo::new(&conn);
        let pid = repo
            .create(&PlayerCreate {
                account_id: THROWAWAY_ID,
                name: format!("xchg_{}", ts()),
                level: 1,
                st: 30, ht: 30, dx: 30, iq: 30,
                job: 0, voice: 0, dir: 0,
                x: 969600, y: 278400, z: 0,
                hp: 100, mp: 100, random_hp: 0, random_sp: 0,
                stat_point: 0, stamina: 100,
                part_base: 0, part_main: 0, part_hair: 0,
                gold: 1_000, playtime: 0,
                skill_level: vec![0x01], quickslot: vec![0x01],
            })
            .await
            .expect("create player throwaway");

        // Items material (INVENTORY, stacks de 5 y 3).
        let items = ItemRepo::new(&conn);
        let mk = |id: i64, pos: i32, vnum: i64, count: i64| ItemRow {
            id,
            window: "INVENTORY".into(),
            pos,
            count,
            vnum,
            sockets: [0, 0, 0],
            attrs: [(0, 0); 7],
        };
        assert_eq!(items.upsert(&mk(MAT_A, 0, 10, 5), pid).await.expect("mat A"), MAT_A);
        assert_eq!(items.upsert(&mk(MAT_B, 1, 11, 3), pid).await.expect("mat B"), MAT_B);

        // Unidad: consume 3 de A (5→2), TODO B (3→0 DELETE), crea resultado
        // (id del rango) y sube el oro 1_000→1_200.
        let ex = ItemExchange {
            owner_id: pid,
            materials: vec![(MAT_A, 5, 2), (MAT_B, 3, 0)],
            result: Some((mk(RESULT_ID, 2, 30001, 1), pid)),
            gold: Some((1_000, 1_200)),
        };
        let sink = PgMutationSink::new(&conn).with_audit_table(audit.clone());
        let batcher = Batcher::spawn(Duration::from_millis(100), 64, sink);
        items.exchange_mutated(&batcher, &ex).await.expect("unidad ACID commit");

        // Estado post-unidad: 4 mutations aplicadas (2 materiales + resultado + oro).
        let loaded = items.load_by_owner(pid).await.expect("load post-unidad");
        let get = |id: i64| loaded.iter().find(|r| r.id == id).expect("item presente");
        assert_eq!(get(MAT_A).count, 2, "A consumido 5→2");
        assert!(
            loaded.iter().all(|r| r.id != MAT_B),
            "B consumido por completo (3→0) -> DELETE"
        );
        assert_eq!(get(RESULT_ID).count, 1, "resultado creado");
        assert_eq!(get(RESULT_ID).vnum, 30001);
        let player = PlayerRepo::new(&conn).load(pid).await.expect("load").expect("player");
        assert_eq!(player.gold, 1_200, "oro 1_000→1_200 en la misma tx");

        // Audit: 4 filas con el MISMO applied_at (misma transaccion).
        let rows = client
            .query(&format!("SELECT applied_at FROM {audit} ORDER BY mutation_id"), &[])
            .await
            .expect("audit rows");
        assert_eq!(rows.len(), 4, "4 mutations auditadas (la unidad entera)");
        let t0: std::time::SystemTime = rows[0].get(0);
        for r in &rows {
            let t: std::time::SystemTime = r.get(0);
            assert_eq!(t, t0, "mismo applied_at -> misma tx -> unidad ACID");
        }
        Ok::<(), String>(())
    }
    .await;

    // Cleanup SIEMPRE: items throwaway -> player -> schema.
    let _ = client
        .batch_execute(&format!(
            "DELETE FROM player.item WHERE id IN ({MAT_A}, {MAT_B}, {RESULT_ID}); \
             DELETE FROM player.player WHERE name LIKE 'xchg_%'; \
             DROP SCHEMA IF EXISTS {schema} CASCADE"
        ))
        .await;
    result.expect("unidad ACID contra PG real");
}
