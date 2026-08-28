//! `channel/belt.rs` — el CINTURÓN (belt de pociones, parity
//! `belt_inventory_helper.h`): el wire del belt viaja en el window
//! INVENTORY con cells `BELT_INVENTORY_SLOT_START..END` (242..258 — el
//! cliente pinta su ventana de belt leyendo `GetItemIndex(INVENTORY, 242+i)`,
//! uiinventory.py:247-249; parity `SItemPos::IsBeltInventoryPosition`,
//! length.h:836-839). En PG/memoria los items viven como window
//! "BELT_INVENTORY" pos 0..15 (parity `SaveItem` item_manager.cpp:414-421 y
//! `ItemLoad` input_db.cpp:1490-1495). El grado de cada celda lo decide
//! `is_available_cell(cell, grade)` con el `value0` del belt EQUIPADO
//! (`CItem::GetValue(0)` = item_proto.alValues[0] — item.cpp:1097-1100), y
//! solo entran pociones (`can_move_into_belt`).

use database::item::ItemRow;
use protocol::world::TItemPos;

/// `BELT_INVENTORY_SLOT_START = 242` (length.h:767-775: 180 inventario +
/// 32 wear + 6×2 DS + 6×3 reserva) — la primera celda del belt en el wire.
pub const BELT_INVENTORY_SLOT_START: u16 = 242;
/// `BELT_INVENTORY_SLOT_COUNT = 16` (length.h:93-96).
pub const BELT_INVENTORY_SLOT_COUNT: u16 = 16;
/// `BELT_INVENTORY_SLOT_END` (length.h:772).
pub const BELT_INVENTORY_SLOT_END: u16 = BELT_INVENTORY_SLOT_START + BELT_INVENTORY_SLOT_COUNT;
/// `WEAR_BELT = 23` (length.h:131) — cell del equip = 180 + 23 = 203.
pub const WEAR_BELT: u16 = 23;
/// `ITEM_USE = 3` (item_length.h:58).
const ITEM_USE: i16 = 3;
/// `USE_POTION = 0`, `USE_ABILITY_UP = 7`, `USE_POTION_NODELAY = 11`
/// (item_length.h:251-262) — los subtipos que admiten las celdas del belt.
const USE_POTION: i16 = 0;
const USE_ABILITY_UP: i16 = 7;
const USE_POTION_NODELAY: i16 = 11;

/// `IsBeltInventoryPosition` (length.h:836-839): posición WIRE de belt
/// (window INVENTORY, cells 242..258). Otras ventanas → false (el C++
/// `GetItem` devuelve nullptr para el window BELT_INVENTORY del enum —
/// char_item.cpp:238-269).
pub fn is_belt_cell(p: TItemPos) -> bool {
    p.window == TItemPos::WINDOW_INVENTORY
        && (BELT_INVENTORY_SLOT_START..BELT_INVENTORY_SLOT_END).contains(&p.cell)
}

/// Cell WIRE de un row (parity `SaveItem`/`SetItem`, item_manager.cpp:
/// 414-421 + char_item.cpp:451-457): el belt ocupa INVENTORY 242+pos; el
/// resto usa su pos tal cual.
pub fn wire_cell(it: &ItemRow) -> u16 {
    if it.window == "BELT_INVENTORY" {
        BELT_INVENTORY_SLOT_START + it.pos as u16
    } else {
        it.pos as u16
    }
}

/// pos del PG (0..15) de una celda wire del belt.
pub fn belt_pos_of(cell: u16) -> i32 {
    i32::from(cell - BELT_INVENTORY_SLOT_START)
}

/// Coloca un row en la celda wire destino (parity `SaveItem`: el belt se
/// guarda como window "BELT_INVENTORY" pos 0..15; el resto INVENTORY pos).
pub fn place_at(it: &mut ItemRow, to_belt: bool, cell: u16) {
    if to_belt {
        it.window = "BELT_INVENTORY".to_string();
        it.pos = belt_pos_of(cell);
    } else {
        it.window = "INVENTORY".to_string();
        it.pos = i32::from(cell);
    }
}

/// `availableRuleByGrade[cell] <= grade` (belt_inventory_helper.h:37-55) —
/// qué celdas abre cada grade del cinturón.
pub fn is_available_cell(cell: u16, grade: i32) -> bool {
    const RULE: [i32; BELT_INVENTORY_SLOT_COUNT as usize] =
        [1, 2, 4, 6, 3, 3, 4, 6, 5, 5, 5, 6, 7, 7, 7, 7];
    let i = (cell - BELT_INVENTORY_SLOT_START) as usize;
    i < RULE.len() && RULE[i] <= grade
}

/// `CanMoveIntoBeltInventory` (belt_inventory_helper.h:70-87): solo
/// ITEM_USE con subtipo POTION / POTION_NODELAY / ABILITY_UP.
pub fn can_move_into_belt(b_type: i16, b_sub_type: i16) -> bool {
    b_type == ITEM_USE
        && matches!(b_sub_type, USE_POTION | USE_POTION_NODELAY | USE_ABILITY_UP)
}

/// `GetWear(WEAR_BELT)` (char_item.cpp:470-482): el cinturón EQUIPADO
/// (window EQUIPMENT, cell 180+23=203). `None` = sin cinturón.
pub fn equipped_belt(inventory: &[ItemRow]) -> Option<&ItemRow> {
    inventory
        .iter()
        .find(|i| i.window == "EQUIPMENT" && i.pos as u16 == super::INVENTORY_MAX_NUM + WEAR_BELT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::session::Session;
    use database::item::ItemRow;
    use database::player::PlayerRow;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    fn row(window: &str, pos: i32, vnum: i64) -> ItemRow {
        ItemRow {
            id: 0,
            window: window.into(),
            pos,
            count: 10,
            vnum,
            sockets: [0; 3],
            attrs: [(0, 0); 7],
        }
    }

    /// VERIFIER (mutation): la tabla de grade del belt es EXACTAMENTE
    /// `availableRuleByGrade` (belt_inventory_helper.h:39-44) — quitar un
    /// requisito de la fila → rojo. Grade 1 abre solo la celda 0; grade 2
    /// las 0/1; grade 3 añade la 4/5; grade 7 las 16.
    #[test]
    fn grade_rule_matches_belt_inventory_helper() {
        let cell = |i: u16| BELT_INVENTORY_SLOT_START + i;
        assert!(is_available_cell(cell(0), 1), "grade 1: celda 0");
        assert!(!is_available_cell(cell(1), 1), "grade 1: celda 1 NO");
        assert!(is_available_cell(cell(0), 2), "grade 2: celda 0");
        assert!(is_available_cell(cell(1), 2), "grade 2: celda 1");
        assert!(!is_available_cell(cell(4), 2), "grade 2: celda 4 NO (requiere 3)");
        assert!(!is_available_cell(cell(2), 2), "grade 2: celda 2 NO (requiere 4)");
        assert!(is_available_cell(cell(4), 3), "grade 3: celda 4");
        assert!(!is_available_cell(cell(0), 0), "grade 0: ninguna");
        assert!((0..16).all(|i| is_available_cell(cell(i), 7)), "grade 7: las 16");
        assert!(!is_available_cell(cell(16), 7), "fuera del rango");
    }

    /// VERIFIER (mutation): CanMoveIntoBeltInventory (belt_inventory_helper.h:
    /// 70-87) — solo ITEM_USE con subtipo POTION(0)/ABILITY_UP(7)/
    /// POTION_NODELAY(11); quitar un subtipo → rojo.
    #[test]
    fn can_move_into_belt_only_use_potions() {
        assert!(can_move_into_belt(3, 0), "USE/POTION");
        assert!(can_move_into_belt(3, 7), "USE/ABILITY_UP");
        assert!(can_move_into_belt(3, 11), "USE/POTION_NODELAY");
        assert!(!can_move_into_belt(3, 2), "USE/TUNING no");
        assert!(!can_move_into_belt(3, 8), "USE/AFFECT no");
        assert!(!can_move_into_belt(1, 0), "arma no");
        assert!(!can_move_into_belt(4, 0), "AUTOUSE no");
    }

    /// VERIFIER (mutation): las celdas del belt en el wire son el window
    /// INVENTORY 242..258 (length.h:836-839 + uiinventory.py:247-249) —
    /// el belt NUNCA viaja como window 6 en los CG_ITEM_MOVE (el C++ no lo
    /// resuelve, char_item.cpp:238-269). Desplazar el start → rojo.
    #[test]
    fn belt_cells_are_inventory_window_242_258() {
        assert_eq!(BELT_INVENTORY_SLOT_START, 242, "180 + 32 + 6×2 + 6×3");
        assert_eq!(BELT_INVENTORY_SLOT_END, 258, "242 + 16");
        let p = |w: u8, cell: u16| TItemPos { window: w, cell };
        assert!(!is_belt_cell(p(1, 241)));
        assert!(is_belt_cell(p(1, 242)), "primera celda");
        assert!(is_belt_cell(p(1, 257)), "última celda");
        assert!(!is_belt_cell(p(1, 258)));
        assert!(!is_belt_cell(p(TItemPos::WINDOW_BELT, 0)), "window 6 no (parity GetItem)");
        assert!(!is_belt_cell(p(2, 242)), "EQUIPMENT no");
    }

    /// VERIFIER (mutation): el mapeo DB↔wire (parity SaveItem
    /// item_manager.cpp:414-421): BELT_INVENTORY pos 0..15 ↔ INVENTORY cell
    /// 242+pos. Romper wire_cell/place_at → rojo.
    #[test]
    fn wire_and_db_positions_roundtrip() {
        let belt_item = row("BELT_INVENTORY", 3, 27001);
        assert_eq!(wire_cell(&belt_item), 245, "242 + 3");
        assert_eq!(wire_cell(&row("INVENTORY", 7, 27001)), 7, "inv tal cual");
        let mut it = row("BELT_INVENTORY", 0, 27001);
        place_at(&mut it, false, 12);
        assert_eq!((it.window.as_str(), it.pos), ("INVENTORY", 12));
        place_at(&mut it, true, 242);
        assert_eq!((it.window.as_str(), it.pos), ("BELT_INVENTORY", 0));
        assert_eq!(belt_pos_of(257), 15, "última pos del PG");
    }

    // ------------------------------------------------------------------
    // VERIFIER live-PG (regla 20 — antimutación del CABLEADO): el
    // CG_ITEM_MOVE → BELT_INVENTORY está cableado en `items::handle_move`
    // con el gate de grade y la persistencia del window, Y el checkout de
    // la caja puede aterrizar en el belt (`safebox::handle_checkout`).
    // Mutations que fallan: quitar la rama belt del handler (el item acaba
    // en INVENTORY 242 sin gates o el move se rechaza — asserts rojos) o
    // quitar el gate de grade (el move a la celda 243 PASA → assert rojo).
    // Requiere la PG local (patrón de los tests live de la suite — el stack
    // diario la mantiene arriba; DATABASE_TEST_PG para otra instancia).
    #[tokio::test]
    async fn belt_move_wired_and_grade_gated() {
        let conn = std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| {
            "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2".to_string()
        });
        let pool = database::pool::new_pool(&conn, 3).expect("pool PG");
        // item_proto THROWAWAY (cleanup SIEMPRE): la PG desplegada NO tiene
        // belts (type 32 = 0 filas, verificado) → el verifier crea uno con
        // grade 1 (abre solo la celda 242). Vnum POR PID (leftovers de un
        // run roto nunca colisionan) + DELETE+ON CONFLICT al inicio (un
        // re-run se auto-sana aunque el run anterior murió sin cleanup).
        let belt_vnum: i64 = 2_000_000 + i64::from(std::process::id() % 900_000);
        let item_id: i64 = 990_000_001;
        let pc = pool.get().await.expect("client PG");
        pc.batch_execute(&format!("DELETE FROM player.item_proto WHERE vnum = {belt_vnum}"))
            .await
            .expect("cleanup proto previo");
        pc.batch_execute(&format!("DELETE FROM player.item WHERE id IN ({item_id}, {item_id} + 1)"))
            .await
            .expect("cleanup items previo");
        pc.batch_execute(&format!(
            "INSERT INTO player.item_proto (vnum, name, locale_name, type, subtype, immuneflag, shop_buy_price, refined_vnum, refine_set, refine_set2, magic_pct, specular, socket_pct, addon_type, value0) VALUES ({belt_vnum}, ''::bytea, ''::bytea, 32, 0, '', 0, 0, 0, 0, 0, 0, 0, 0, 1) ON CONFLICT (vnum) DO UPDATE SET type = 32, subtype = 0, value0 = 1"
        ))
        .await
        .expect("insert proto belt");
        let (mut s, mut sock) = test_session(9510, &conn).await;
        s.inventory = vec![
            ItemRow {
                id: item_id,
                window: "INVENTORY".into(),
                pos: 0,
                count: 10,
                vnum: 27001, // USE/POTION real de la PG
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            },
            row("EQUIPMENT", 203, belt_vnum),
        ];
        let mv = |from: u16, to: u16| protocol::world::TPacketCGItemMove {
            header: protocol::world::TPacketCGItemMove::HEADER,
            pos: TItemPos {
                window: TItemPos::WINDOW_INVENTORY,
                cell: from,
            },
            change_pos: TItemPos {
                window: TItemPos::WINDOW_INVENTORY,
                cell: to,
            },
            num: 0,
        };
        // Inv(0) → belt(242): DEL (42 B, header 20) + SET (51 B, header 21)
        // en la celda wire 242 y memoria/persistencia BELT_INVENTORY pos 0.
        crate::channel::items::handle_move(&mut s, &mv(0, 242).to_bytes())
            .await
            .expect("move a belt OK");
        assert_eq!(s.inventory[0].window, "BELT_INVENTORY", "window PG (persistencia)");
        assert_eq!(s.inventory[0].pos, 0, "pos 0..15 del PG");
        let del = read_n(&mut sock, 42).await;
        assert_eq!(del[0], 20, "GC_ITEM_DEL del origen");
        let set = read_n(&mut sock, 51).await;
        assert_eq!(set[0], 21, "GC_ITEM_SET");
        assert_eq!(&set[1..4], &[1, 242, 0], "SET en INVENTORY 242 (wire del belt)");
        assert_eq!(u32::from_le_bytes(set[4..8].try_into().unwrap()), 27001);
        // Grade 1 NO abre la celda 243 (rule[1]=2): rechazo silencioso.
        crate::channel::items::handle_move(&mut s, &mv(242, 243).to_bytes())
            .await
            .expect("rechazo no es error");
        assert_eq!(s.inventory[0].pos, 0, "sin cambios tras el rechazo");
        let mut probe = [0u8; 1];
        assert!(
            tokio::time::timeout(Duration::from_millis(200), sock.read(&mut probe))
                .await
                .is_err(),
            "grade 1 no debe emitir paquetes a la celda 243"
        );
        // Belt(242) → inv(5): el camino de vuelta conserva window INVENTORY.
        crate::channel::items::handle_move(&mut s, &mv(242, 5).to_bytes())
            .await
            .expect("move belt→inv OK");
        assert_eq!((s.inventory[0].window.as_str(), s.inventory[0].pos), ("INVENTORY", 5));
        let _ = read_n(&mut sock, 42).await;
        let set = read_n(&mut sock, 51).await;
        assert_eq!(&set[1..4], &[1, 5, 0], "SET en INVENTORY 5");
        // CHECKOUT de la caja AL BELT (segundo flujo del slice — parity
        // SafeboxCheckout @fixme119 input_main.cpp:2096-2100 + rama belt de
        // IsEmptyItemGrid): caja con poción → GC_SAFEBOX_DEL + GC_ITEM_SET
        // en la celda wire 242 + row BELT_INVENTORY@0. Quitar la rama
        // `to_belt` del checkout → el gate viejo rechaza (sin paquetes, la
        // caja conserva el item) → asserts rojos.
        s.safebox = Some(crate::channel::safebox::SafeboxState {
            size: 1,
            gold: 0,
            items: vec![ItemRow {
                id: item_id + 1,
                window: "SAFEBOX".into(),
                pos: 0,
                count: 3,
                vnum: 27001,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            }],
        });
        let co = protocol::world::TPacketCGSafeboxCheckout {
            header: protocol::world::TPacketCGSafeboxCheckout::HEADER,
            b_safe_pos: 0,
            item_pos: TItemPos {
                window: TItemPos::WINDOW_INVENTORY,
                cell: 242,
            },
        };
        crate::channel::safebox::handle_checkout(&mut s, &co.to_bytes())
            .await
            .expect("checkout a belt OK");
        assert!(
            s.safebox.as_ref().expect("caja").items.is_empty(),
            "la caja se vacía tras el checkout"
        );
        let belt_item = s
            .inventory
            .iter()
            .find(|i| i.id == item_id + 1)
            .expect("poción del checkout en la sesión");
        assert_eq!(belt_item.window, "BELT_INVENTORY", "checkout: window PG");
        assert_eq!(belt_item.pos, 0, "checkout: pos 0..15 del PG");
        let _ = read_n(&mut sock, 2).await; // GC_SAFEBOX_DEL (1 + pos)
        let set = read_n(&mut sock, 51).await;
        assert_eq!(set[0], 21, "checkout: GC_ITEM_SET");
        assert_eq!(&set[1..4], &[1, 242, 0], "checkout: SET en INVENTORY 242");
        // Cleanup SIEMPRE (regla world_pg.rs): items + proto throwaway.
        let repo = database::item::ItemRepo::new(pool.clone());
        let _ = repo.delete(item_id).await;
        let _ = repo.delete(item_id + 1).await;
        pool.get()
            .await
            .expect("client")
            .batch_execute(&format!("DELETE FROM player.item_proto WHERE vnum = {belt_vnum}"))
            .await
            .expect("cleanup proto belt");
    }

    fn dummy_row(id: i64) -> PlayerRow {
        PlayerRow {
            id,
            name: "BeltTester".into(),
            job: 1,
            voice: 0,
            dir: 0,
            x: 969600,
            y: 278400,
            z: 0,
            map_index: 41,
            exit_x: 0,
            exit_y: 0,
            exit_map_index: 0,
            hp: 100,
            mp: 100,
            stamina: 0,
            random_hp: 0,
            random_sp: 0,
            playtime: 0,
            gold: 0,
            level: 50,
            level_step: 0,
            st: 30,
            ht: 30,
            dx: 30,
            iq: 30,
            exp: 0,
            stat_point: 0,
            skill_point: 0,
            sub_skill_point: 0,
            stat_reset_count: 0,
            part_base: 0,
            part_hair: 0,
            part_main: 0,
            skill_level: None,
            quickslot: None,
            skill_group: 3,
            alignment: 0,
            horse_level: 0,
            horse_riding: 0,
            horse_hp: 0,
            horse_hp_droptime: 0,
            horse_stamina: 0,
            logoff_interval: 0.0,
            horse_skill_point: 0,
        }
    }

    /// Sesión de test: sockets localhost + pool PG REAL (los gates del belt
    /// leen item_proto). Patrón de party.rs (test_session) con la conexión
    /// de la PG local.
    async fn test_session(vid: u32, conn: &str) -> (Session, tokio::net::TcpStream) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind localhost");
        let addr = listener.local_addr().expect("addr");
        let client_side = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (server_side, _peer) = listener.accept().await.expect("accept");
        let pool = database::pool::new_pool(conn, 2).expect("pool PG");
        let wal_dir = std::env::temp_dir()
            .join(format!("belt_test_wal_{}", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let batcher = std::sync::Arc::new(database::wal::Batcher::spawn(
            Duration::from_millis(100),
            64,
            database::wal::WalSink::new(database::wal::PgMutationSink::new(pool.clone()), wal_dir),
        ));
        let mut cfg = crate::config::Config::default();
        cfg.timeout = Duration::from_secs(5);
        let (intent_tx, _intent_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut s = Session::new(
            server_side,
            cfg,
            vid,
            intent_tx,
            std::sync::Arc::new(std::sync::Mutex::new(game_core::map::MapStore::new())),
            pool,
            batcher,
            std::sync::Arc::new(database::attr::AttrTables::default()),
        );
        s.row = Some(dummy_row(i64::from(vid)));
        (s, client_side)
    }

    async fn read_n(sock: &mut tokio::net::TcpStream, n: usize) -> Vec<u8> {
        let mut buf = vec![0u8; n];
        sock.read_exact(&mut buf).await.expect("lectura wire");
        buf
    }
}