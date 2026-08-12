//! Integration F5 (dominio world — MobRepo) contra PostgreSQL REAL — gated
//! con `#[ignore]` (mismo patrón que player_pg.rs). Oracle: la tabla
//! `player.mob_proto` migrada por G-PG con los nombres ES del pack (fix
//! 2026-08-08 §16) y los vnums del runtime del mapa 41.
//!
//! Ejecutar desde WSL:
//!
//! ```text
//! source ~/.cargo/env
//! cd /mnt/c/projects/Metin2/source/reforge
//! cargo test --package database -- --ignored mob
//! ```
//!
//! SOLO lecturas (MariaDB/PG read-only — regla del lane F5).

use database::npc::{wire_b_type, MobRepo};

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";

fn pg_conn() -> String {
    std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
}

/// Los vnums del mapa 41 (npc.txt/boss.txt/stone.txt del runtime) resuelven
/// en `mob_proto` con el subset del spawn coherente: type 0/1/2, level,
/// locale_name presente, size/ai_flag TEXT, folder cuando aplica.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn mob_repo_loads_map41_vnums() {
    let repo = MobRepo::new(pg_conn());
    // NPCs y mobs reales del mapa 41 (npc.txt: 20340, 20001, 9001, 11004;
    // regen.txt: 101 = Perro Salvaje, 191 = Lykos; boss: 151; stone: 8001).
    for vnum in [20340i64, 20001, 9001, 11004, 101, 191, 151, 8001, 5001] {
        let m = repo
            .load_by_vnum(vnum)
            .await
            .unwrap_or_else(|e| panic!("load {vnum}: {e}"))
            .unwrap_or_else(|| panic!("mob {vnum} debe existir (runtime mapa 41)"));
        assert_eq!(m.vnum, vnum, "vnum round-trip");
        assert!(!m.name.is_empty(), "name presente para {vnum}");
        assert!(!m.locale_name.is_empty(), "locale_name presente para {vnum}");
        assert!(m.level > 0, "level > 0 para {vnum}");
        let t = m.b_type;
        assert!(wire_b_type(t).is_ok(), "b_type {t} cabe en BYTE");
        assert!((0..=9).contains(&t), "b_type {t} en el rango ECharType");
        // type 1 = NPC (spawn con addInfo); 0 = monster; 2 = stone.
        match vnum {
            20340 | 20001 | 9001 | 11004 => assert_eq!(m.b_type, 1, "{vnum} es NPC"),
            101 | 191 | 151 | 5001 => assert_eq!(m.b_type, 0, "{vnum} es monster"),
            8001 => assert_eq!(m.b_type, 2, "{vnum} es stone"),
            _ => unreachable!(),
        }
    }
    // Vnum inexistente -> None.
    assert_eq!(
        repo.load_by_vnum(999_999_999).await.expect("DB up"),
        None,
        "vnum inexistente -> None"
    );
}

/// Los campos de combate del MobRow (F5.2): las columnas reales del schema PG
/// (`ht` = la bCon del C++ — tables.h:448; `def` = wDef — tables.h:463;
/// max_hp; attack_range) con los valores del mob 101 (Perro Salvaje — el del
/// harness del combate).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn mob_repo_combat_fields() {
    let repo = MobRepo::new(pg_conn());
    let m = repo.load_by_vnum(101).await.expect("DB up").expect("101 existe");
    assert_eq!(m.ht, 5, "mob 101: la bCon del C++ = la columna ht");
    assert_eq!(m.def, 4, "mob 101: wDef (harness del combate: wdef=4)");
    assert_eq!(m.max_hp, 126, "mob 101: max_hp real");
    assert_eq!(m.attack_range, 175, "mob 101: attack_range (harness: 175)");
    assert!(m.max_hp > 0 && m.attack_range > 0, "campos coherentes");
}

/// El nombre del addInfo viene del locale_name (bytea — bytes CP949 o ES del
/// pack): el C++ usa szLocaleName como GetName() del spawn.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn mob_repo_locale_name_is_bytes() {
    let repo = MobRepo::new(pg_conn());
    let m = repo.load_by_vnum(20340).await.expect("DB up").expect("20340 existe");
    // El dump ES del 2026-08-08 puso nombres UTF-8 en name/locale_name
    // (varbinary 24); lo que importa para el wire: locale_name es BYTES
    // crudos (Vec<u8>) — el cliente los usa solo como fallback (multilang).
    assert!(!m.locale_name.is_empty(), "locale_name no vacío");
    let as_text = String::from_utf8_lossy(&m.locale_name);
    assert!(!as_text.trim().is_empty(), "decodificable (lossy)");
    assert!(
        m.locale_name.len() <= 24,
        "varbinary(24): {} bytes",
        m.locale_name.len()
    );
}

/// Batch (F5 perf): `load_by_vnums` devuelve el MISMO subset que los loads
/// individuales, en UNA query (los 117 vnums del mapa 41 — la resolución de
/// spawns del canal). Vnums inexistentes no aparecen; lista vacía -> Ok([]).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
async fn mob_repo_batch_load_map41_vnums() {
    let repo = MobRepo::new(pg_conn());
    // Los 117 vnums distintos del mapa 41 expandido (evidencia del lane:
    // inventario verificado contra el runtime 2026-08-11).
    let vnums: Vec<i64> = vec![
        101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 136, 138, 142,
        144, 151, 152, 153, 154, 155, 171, 172, 173, 174, 175, 176, 177, 178, 179, 180, 181, 182,
        183, 184, 185, 191, 192, 193, 194, 301, 302, 303, 304, 351, 352, 354, 391, 392, 393, 394,
        395, 396, 397, 398, 2101, 5001, 5004, 8001, 8002, 8003, 8004, 8005, 9001, 9002, 9003, 9005,
        9006, 9007, 9008, 9009, 9012, 10005, 11004, 11005, 20001, 20002, 20003, 20005, 20006, 20008,
        20009, 20011, 20016, 20018, 20023, 20025, 20029, 20030, 20041, 20047, 20049, 20050, 20051,
        20052, 20084, 20086, 20087, 20090, 20095, 20340, 20341, 20342, 20343, 20344, 20345, 20346,
        20347, 20349, 20354, 20355, 20357, 20358, 60003,
    ];
    let map = repo.load_by_vnums(&vnums).await.expect("batch");
    assert_eq!(map.len(), vnums.len(), "todos los vnums del mapa 41 existen");
    for v in &vnums {
        let row = map.get(v).unwrap_or_else(|| panic!("vnum {v} en el batch"));
        assert_eq!(row.vnum, *v);
        assert!(!row.locale_name.is_empty(), "locale_name de {v}");
    }
    // Coincidencia con el load individual (el mapeo es el mismo).
    let single = repo.load_by_vnum(101).await.expect("single").expect("101");
    assert_eq!(map.get(&101).expect("101 batch").name, single.name);
    assert_eq!(map.get(&101).expect("101 batch").b_type, single.b_type);
    // Vnums inexistentes: ausentes del resultado (sin error).
    let partial = repo.load_by_vnums(&[101, 999_999_999]).await.expect("partial");
    assert_eq!(partial.len(), 1);
    assert!(partial.contains_key(&101));
    // Lista vacía -> Ok(vacío), sin query.
    let empty = repo.load_by_vnums(&[]).await.expect("empty");
    assert!(empty.is_empty());
}
