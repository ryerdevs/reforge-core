//! Gated (requiere PG real 127.0.0.1:5432): el load del `ShopRepo` contra
//! `player.shop` real — los tipos del esquema (vnum integer, npc_vnum
//! smallint — fix 2026-08-14: el "error deserializing column 0" era leer
//! int4/int2 como i64) no deben fallar y las tiendas del runtime cargan.

use game_core::shop::ShopRepo;

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";

#[tokio::test]
#[ignore = "requiere PG real (127.0.0.1:5432): cargo test -p game_core -- --ignored"]
async fn shop_repo_loads_real_shops() {
    let pool = database::pool::new_pool(DEFAULT_PG, 2).expect("pool");
    let shops = ShopRepo::new(pool)
        .load()
        .await
        .expect("load de player.shop");
    assert!(!shops.is_empty(), "player.shop tiene tiendas");
    // El vendedor del pueblo: shop 1 -> npc_vnum 9001 (legacy dump
    // mariadb_full_2026-08-12.sql; fix 2026-08-15 — antes apuntaba a 20002).
    let s1 = shops.iter().find(|s| s.vnum == 1).expect("shop 1");
    assert_eq!(s1.npc_vnum, 9001, "npc_vnum del shop 1");
    assert!(!s1.items.is_empty(), "el shop 1 tiene items");
    // Fix 2026-08-15 (paso 2): los 3 vendedores del pueblo visibles tienen
    // shop — 20002/20006/20023 asignados a las filas all_* libres (1002-1004).
    for (vnum, npc) in [(1002, 20002), (1003, 20006), (1004, 20023)] {
        let s = shops
            .iter()
            .find(|s| s.vnum == vnum)
            .unwrap_or_else(|| panic!("shop {vnum}"));
        assert_eq!(s.npc_vnum, npc, "npc_vnum del shop {vnum} (vendedor {npc})");
        assert!(!s.items.is_empty(), "el shop {vnum} tiene items");
    }
    // El resto de tiendas del runtime (1..6 y 9/1002/1003).
    assert!(shops.len() >= 9, "las tiendas del runtime: {}", shops.len());
}
