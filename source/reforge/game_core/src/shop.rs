//! Tiendas NPC (F6 social): las reglas PURAS de compra/venta (parity
//! `shop.cpp`/`shop_manager.cpp` — la tienda del C++ NO valida más allá del
//! oro; el Rust valida: item existe en el shop, pos válido, oro suficiente,
//! hueco/stack en el inventario, item vendible) + el repo de las tablas
//! `player.shop`/`shop_item` (G-PG) con el precio resuelto del `item_proto`.
//!
//! # Precios (parity, verificados 2026-08-13)
//!
//! - COMPRA (el jugador paga): `item_proto.gold × shop_item.count`
//!   (`shop.cpp:166-180` — con `ITEM_FLAG_COUNT_PER_1GOLD` → `count/gold`,
//!   `gold == 0` → `count`). El item se compra COMPLETO (el cliente no
//!   tiene selector de cantidad para tiendas NPC — `CShop::Buy` cobra
//!   `r_item.price` = el del stack).
//! - VENTA (la tienda paga): `item_proto.shop_buy_price × count / 5` − 3%
//!   de impuesto (`shop_manager.cpp:297-319` — `GetShopBuyPrice()` =
//!   `dwShopBuyPrice`, `item.cpp:1171-1174`).
//!
//! GAP documentado: el cheque (won, `ENABLE_CHEQUE_SYSTEM`) no existe en el
//! player Rust (la fila no lo carga) — el wire lo manda 0 (parity: las
//! tiendas NPC del C++ mandan `cheque = 0`, `shop.cpp:153`).

use std::collections::HashMap;

use database::item::ItemRow;

/// `SHOP_HOST_ITEM_MAX_NUM` (cliente `Packet.h:345`, server `shop.h`) —
/// el tope de items por tienda en el wire y en la tabla.
pub const SHOP_HOST_ITEM_MAX_NUM: usize = 40;
/// `SHOP_MAX_DISTANCE` (shop.h:6) — rango para abrir la tienda del NPC.
pub const SHOP_MAX_DISTANCE: i64 = 1000;
/// `ITEM_FLAG_COUNT_PER_1GOLD` (`item_length.h:338`) — items que se venden
/// "count por 1 de oro" (flechas, etc.).
pub const ITEM_FLAG_COUNT_PER_1GOLD: i64 = 1 << 3;
/// `ITEM_ANTIFLAG_SELL` (`item_length.h:362`) — items que NO se pueden
/// vender a la tienda.
pub const ITEM_ANTIFLAG_SELL: i64 = 1 << 8;
/// `GOLD_MAX` (`length.h:80`) — el tope de oro del player (overflow).
pub const GOLD_MAX: i64 = 2_000_000_000;
/// `iVal = 3` del impuesto de venta (`shop_manager.cpp:314`).
const SELL_TAX_PCT: i64 = 3;
/// `dwPrice /= 5` de la venta (`shop_manager.cpp:309-311`).
const SELL_PRICE_DIVISOR: i64 = 5;

/// Un item de la tienda NPC (los campos del wire `packet_shop_item` —
/// `GameType.h:348-359`: vnum/price/count/display_pos; sockets/attrs/cheque
/// a 0 para tiendas NPC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShopItem {
    pub vnum: i64,
    pub count: i64,
    pub price: i64,
    pub display_pos: u8,
}

/// Una tienda del runtime (`player.shop` + `player.shop_item`).
#[derive(Debug, Clone)]
pub struct Shop {
    pub vnum: i64,
    pub npc_vnum: i64,
    pub items: Vec<ShopItem>,
}

/// Rechazos de compra/venta (el wire los mapea a los subheaders GC_SHOP:
/// `Packet.h:1801-1819`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopError {
    /// `SHOP_SUBHEADER_GC_INVALID_POS` — pos fuera del shop.
    InvalidPos,
    /// `SHOP_SUBHEADER_GC_SOLDOUT` — el item no existe (vnum 0).
    SoldOut,
    /// `SHOP_SUBHEADER_GC_NOT_ENOUGH_MONEY` — oro insuficiente.
    NotEnoughMoney,
    /// `SHOP_SUBHEADER_GC_INVENTORY_FULL` — sin hueco ni stack posible.
    InventoryFull,
    /// La celda no tiene un item (o no está en el inventario).
    NoItem,
    /// Item equipado (parity `IsEquipped` — shop_manager.cpp:280).
    Equipped,
    /// `ITEM_ANTIFLAG_SELL` (shop_manager.cpp:291).
    NotSellable,
    /// `GOLD_MAX` excedido (shop_manager.cpp:326).
    GoldOverflow,
}

impl ShopError {
    /// El subheader GC_SHOP del error (`Packet.h:1801-1819` — los valores
    /// del enum del server, `shop.cpp` `Buy`).
    pub fn wire_subheader(&self) -> u8 {
        match self {
            ShopError::InvalidPos => 8,     // SHOP_SUBHEADER_GC_INVALID_POS
            ShopError::SoldOut => 6,        // SHOP_SUBHEADER_GC_SOLDOUT
            ShopError::NotEnoughMoney => 4, // SHOP_SUBHEADER_GC_NOT_ENOUGH_MONEY
            ShopError::InventoryFull => 7,  // SHOP_SUBHEADER_GC_INVENTORY_FULL
            ShopError::NoItem | ShopError::Equipped | ShopError::NotSellable => 1, // GC_END (rechazo silencioso)
            ShopError::GoldOverflow => 4,   // NOT_ENOUGH_MONEY (overflow)
        }
    }
}

/// Precio de COMPRA del item del shop (parity `CShop::SetShopItems` —
/// `shop.cpp:166-180`): `gold × count` (o `count/gold` con
/// `ITEM_FLAG_COUNT_PER_1GOLD`).
pub fn buy_price(item_gold: i64, count: i64, count_per_1gold: bool) -> i64 {
    if count_per_1gold {
        if item_gold == 0 {
            count
        } else {
            count / item_gold
        }
    } else {
        item_gold * count
    }
}

/// Precio de VENTA a la tienda (parity `CShopManager::Sell` —
/// `shop_manager.cpp:297-319`): `shop_buy_price × count / 5` − 3%.
pub fn sell_price(shop_buy_price: i64, count: i64, count_per_1gold: bool) -> i64 {
    let base = if count_per_1gold {
        if shop_buy_price == 0 {
            count
        } else {
            count / shop_buy_price
        }
    } else {
        shop_buy_price * count
    };
    let price = base / SELL_PRICE_DIVISOR;
    price - price * SELL_TAX_PCT / 100
}

/// Recibo de compra: cómo entra el item al inventario (parity
/// `GetEmptyInventoryEx` + `AutoStackItemEx` — shop.cpp:305-319, 390).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuyReceipt {
    pub price: i64,
    /// Stack sobre un item existente: `(id, count_pre, count_post)` — el
    /// UPDATE con guard (`count = pre`) es la mutation del material.
    pub stack: Option<(i64, i64, i64)>,
    /// Celda del item NUEVO (solo cuando `stack` es None).
    pub new_pos: u16,
}

/// Compra validada (pura): el item del shop COMPLETO (count del shop).
///
/// Reglas (parity `CShop::Buy` — shop.cpp:190-403):
/// 1. `gold >= price` (el precio es del stack completo);
/// 2. hueco libre O stack existente del mismo vnum con hueco
///    (`AutoStackItemEx` — tope `count_limit`).
pub fn buy(
    inventory: &[ItemRow],
    gold: i64,
    shop_item: &ShopItem,
    count_limit: i64,
    inventory_cells: u16,
) -> Result<BuyReceipt, ShopError> {
    if shop_item.vnum <= 0 {
        return Err(ShopError::SoldOut);
    }
    if gold < shop_item.price {
        return Err(ShopError::NotEnoughMoney);
    }
    // Stack primero (parity AutoStackItemEx): mismo vnum con hueco.
    if let Some(existing) = inventory
        .iter()
        .find(|i| i.vnum == shop_item.vnum && i.count + shop_item.count <= count_limit)
    {
        return Ok(BuyReceipt {
            price: shop_item.price,
            stack: Some((existing.id, existing.count, existing.count + shop_item.count)),
            new_pos: 0,
        });
    }
    // Hueco libre en el inventario (celdas 0..inventory_cells).
    let occupied: std::collections::HashSet<i64> =
        inventory.iter().map(|i| i64::from(i.pos)).collect();
    for cell in 0..inventory_cells {
        if !occupied.contains(&i64::from(cell)) {
            return Ok(BuyReceipt { price: shop_item.price, stack: None, new_pos: cell });
        }
    }
    Err(ShopError::InventoryFull)
}

/// Los datos del item_proto que la venta necesita (parity
/// `GetShopBuyPrice`/`GetFlag` — el canal los resuelve; GAP: la query del
/// proto completo es del lane database).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SellProto {
    /// `item_proto.shop_buy_price` — la base del precio que la tienda paga.
    pub shop_buy_price: i64,
    /// `ITEM_FLAG_COUNT_PER_1GOLD` del item.
    pub count_per_1gold: bool,
}

/// Recibo de venta: `(item_id, count_pre, count_post)` — `post == 0` =
/// DELETE del stack (parity shop_manager.cpp:340-343).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SellReceipt {
    pub price: i64,
    pub material: (i64, i64, i64),
}

/// Venta validada (pura) — parity `CShopManager::Sell` (shop_manager.cpp:
/// 248-349):
/// 1. item en la celda del INVENTARIO (no equipado);
/// 2. no `ITEM_ANTIFLAG_SELL`;
/// 3. `qty == 0 || qty > count` → todo el stack (parity :294-295);
/// 4. `gold + price <= GOLD_MAX` (overflow, :326).
///
/// `proto` = los datos del item_proto del item vendido (shop_buy_price +
/// flag) — el precio = `sell_price(proto.shop_buy_price, qty,
/// proto.count_per_1gold)`.
///
/// NOTA: el chequeo de `antiflag_sell` es paramétrico — el canal no tiene
/// query del antiflag (GAP: la query del proto completo es del lane
/// database; hoy el canal pasa `false`).
pub fn sell(
    inventory: &[ItemRow],
    gold: i64,
    cell: u16,
    qty: i64,
    antiflag_sell: bool,
    proto: SellProto,
) -> Result<SellReceipt, ShopError> {
    let item = inventory
        .iter()
        .find(|i| i.pos as u16 == cell)
        .ok_or(ShopError::NoItem)?;
    if item.window != "INVENTORY" {
        return Err(ShopError::Equipped);
    }
    if antiflag_sell {
        return Err(ShopError::NotSellable);
    }
    let qty = if qty == 0 || qty > item.count { item.count } else { qty };
    let price = sell_price(proto.shop_buy_price, qty, proto.count_per_1gold);
    if gold + price > GOLD_MAX {
        return Err(ShopError::GoldOverflow);
    }
    let post = item.count - qty;
    Ok(SellReceipt { price, material: (item.id, item.count, post) })
}

/// El query del load (const compartida — el test fija el contrato):
/// `shop → shop_item → item_proto` en el orden del C++
/// (`ClientManagerBoot.cpp:247-254`), con flag/gold para el precio
/// (`shop.cpp:166-180`).
const LOAD_SQL: &str = "\
SELECT s.vnum, s.npc_vnum, si.item_vnum, si.count, ip.flag, ip.gold \
FROM player.shop s \
LEFT JOIN player.shop_item si ON si.shop_vnum = s.vnum \
LEFT JOIN player.item_proto ip ON ip.vnum = si.item_vnum \
ORDER BY s.vnum, si.item_vnum";

/// Repo de las tiendas (`player.shop` + `player.shop_item` + `item_proto`
/// para gold/flag — parity del query del db `ClientManagerBoot.cpp:247-254`
/// + la resolución de precio del game `shop.cpp:166-180`).
pub struct ShopRepo {
    pool: database::pool::PgPool,
}

impl ShopRepo {
    pub fn new(pool: database::pool::PgPool) -> Self {
        Self { pool }
    }

    /// Carga TODAS las tiendas. Un shop sin items no se incluye (parity: el
    /// C++ los salta al materializar). Tope `SHOP_HOST_ITEM_MAX_NUM` items.
    pub async fn load(&self) -> Result<Vec<Shop>, String> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| format!("PG pool get: {e}"))?;
        let rows = client
            .query(LOAD_SQL, &[])
            .await
            .map_err(|e| format!("SHOP_LOAD: {e}"))?;
        let mut shops: HashMap<i64, Shop> = HashMap::new();
        for r in &rows {
            // Tipos REALES del esquema (verificados 2026-08-14 con \d):
            // shop.vnum integer, shop.npc_vnum smallint, shop_item.item_vnum
            // integer, shop_item.count bigint, item_proto.flag bigint,
            // item_proto.gold integer. El error "deserializing column 0" era
            // leer vnum (int4) como i64 — los casts a i64 van DESPUÉS.
            let shop_vnum: i32 = r.try_get(0).map_err(|e| format!("shop.vnum: {e}"))?;
            let npc_vnum: i16 = r.try_get(1).map_err(|e| format!("shop.npc_vnum: {e}"))?;
            let shop_vnum = i64::from(shop_vnum);
            let npc_vnum = i64::from(npc_vnum);
            let shop = shops.entry(shop_vnum).or_insert_with(|| Shop {
                vnum: shop_vnum,
                npc_vnum,
                items: Vec::new(),
            });
            let item_vnum: Option<i32> = r.try_get(2).ok();
            let Some(item_vnum) = item_vnum.filter(|v| *v > 0).map(i64::from) else {
                continue;
            };
            if shop.items.len() >= SHOP_HOST_ITEM_MAX_NUM {
                continue; // defensivo: el C++ llena 40 slots como tope
            }
            let count: i64 = r.try_get(3).map_err(|e| format!("shop_item.count: {e}"))?;
            let flag: i64 = r.try_get(4).map_err(|e| format!("item_proto.flag: {e}"))?;
            let gold: i32 = r.try_get(5).map_err(|e| format!("item_proto.gold: {e}"))?;
            let gold = i64::from(gold);
            shop.items.push(ShopItem {
                vnum: item_vnum,
                count,
                price: buy_price(gold, count, flag & ITEM_FLAG_COUNT_PER_1GOLD != 0),
                display_pos: shop.items.len() as u8,
            });
        }
        let mut out: Vec<Shop> = shops.into_values().collect();
        out.sort_by_key(|s| s.vnum);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pos: u16, vnum: i64, count: i64) -> ItemRow {
        ItemRow {
            id: i64::from(pos) + 1000,
            window: "INVENTORY".into(),
            pos: i32::from(pos),
            count,
            vnum,
            sockets: [0; 3],
            attrs: [(0, 0); 7],
        }
    }

    /// Precio de compra: gold × count; COUNT_PER_1GOLD → count/gold.
    #[test]
    fn buy_price_formula_parity() {
        assert_eq!(buy_price(400, 1, false), 400, "poción x1");
        assert_eq!(buy_price(5000, 10, false), 50_000, "stack x10");
        assert_eq!(buy_price(360_000, 1, false), 360_000);
        assert_eq!(buy_price(3, 300, true), 100, "COUNT_PER_1GOLD: 300/3");
        assert_eq!(buy_price(0, 50, true), 50, "COUNT_PER_1GOLD gold 0 → count");
    }

    /// Precio de venta: shop_buy_price × count / 5 − 3% (shop_manager.cpp:
    /// 297-319).
    #[test]
    fn sell_price_formula_parity() {
        // 90000/5 = 18000 − 540 = 17460.
        assert_eq!(sell_price(90_000, 1, false), 17_460);
        // 400/5 = 80 − 2 = 78.
        assert_eq!(sell_price(400, 1, false), 78);
        // 4000/5 = 800 − 24 = 776.
        assert_eq!(sell_price(400, 10, false), 776);
    }

    /// Compra: stack sobre el mismo vnum con hueco; item nuevo en celda libre.
    #[test]
    fn buy_stacks_or_places() {
        let inv = vec![row(0, 101, 5), row(1, 103, 10)];
        let item = ShopItem { vnum: 101, count: 3, price: 300, display_pos: 0 };
        let r = buy(&inv, 1000, &item, 200, 180).expect("buy");
        assert_eq!(r.price, 300);
        assert_eq!(r.stack, Some((1000, 5, 8)), "stack sobre el 101");
        // Item sin stack existente → celda libre (la primera: 2).
        let item2 = ShopItem { vnum: 999, count: 1, price: 50, display_pos: 1 };
        let r = buy(&inv, 1000, &item2, 200, 180).expect("buy");
        assert_eq!(r.stack, None);
        assert_eq!(r.new_pos, 2, "primera celda libre");
    }

    /// Compra: oro insuficiente → NotEnoughMoney; inventario lleno → Full.
    #[test]
    fn buy_rejects_no_gold_and_full_inventory() {
        let item = ShopItem { vnum: 999, count: 1, price: 50, display_pos: 0 };
        // Stack existente del mismo vnum con hueco → compra aunque esté lleno.
        let inv = vec![row(0, 999, 1)];
        assert_eq!(buy(&inv, 49, &item, 200, 180), Err(ShopError::NotEnoughMoney));
        let r = buy(&inv, 1000, &item, 200, 180).expect("buy");
        assert_eq!(r.stack, Some((1000, 1, 2)), "stackea aunque el resto esté lleno");
        // Sin stack posible + sin hueco → InventoryFull.
        let full: Vec<ItemRow> = (0..180).map(|i| row(i, i64::from(200u16 + i), 1)).collect();
        assert_eq!(buy(&full, 1000, &item, 200, 180), Err(ShopError::InventoryFull));
        assert_eq!(
            buy(&full, 1000, &ShopItem { vnum: 0, count: 1, price: 0, display_pos: 0 }, 200, 180),
            Err(ShopError::SoldOut)
        );
    }

    /// Venta: precio ÷5 −3%, DELETE del stack cuando se vacía, qty 0 → todo.
    #[test]
    fn sell_consumes_and_prices() {
        let inv = vec![row(3, 101, 5)];
        // 101: shop_buy 90000 × 5 / 5 = 90000 − 3% = 87300.
        let proto = SellProto { shop_buy_price: 90_000, count_per_1gold: false };
        let r = sell(&inv, 100, 3, 0, false, proto).expect("sell");
        assert_eq!(r.price, 87_300);
        assert_eq!(r.material, (1003, 5, 0), "stack vacío → DELETE");
        // Venta parcial (2): 180000/5 = 36000 − 1080 = 34920.
        let r = sell(&inv, 100, 3, 2, false, proto).expect("sell");
        assert_eq!(r.material, (1003, 5, 3));
        assert_eq!(r.price, 34_920);
    }

    /// Venta: celda vacía/equipada/no vendible/overflow.
    #[test]
    fn sell_rejects_invalid() {
        let proto = SellProto { shop_buy_price: 90_000, count_per_1gold: false };
        let mut equipped = row(3, 101, 5);
        equipped.window = "EQUIPMENT".into();
        assert_eq!(sell(&[equipped], 100, 3, 1, false, proto), Err(ShopError::Equipped));
        assert_eq!(sell(&[], 100, 3, 1, false, proto), Err(ShopError::NoItem));
        assert_eq!(sell(&[row(3, 101, 5)], 100, 3, 1, true, proto), Err(ShopError::NotSellable));
        // Overflow: gold 2_000_000_000 − 1 + precio 87300 > GOLD_MAX.
        assert_eq!(
            sell(&[row(3, 101, 5)], GOLD_MAX - 1, 3, 1, false, proto),
            Err(ShopError::GoldOverflow)
        );
    }

    /// El repo: SQL con el orden del C++ (shop → shop_item → item_proto).
    #[test]
    fn load_sql_contract() {
        assert!(LOAD_SQL.contains("FROM player.shop s"), "tabla shop");
        assert!(LOAD_SQL.contains("LEFT JOIN player.shop_item si ON si.shop_vnum = s.vnum"));
        assert!(LOAD_SQL.contains("LEFT JOIN player.item_proto ip ON ip.vnum = si.item_vnum"));
        assert!(LOAD_SQL.contains("ORDER BY s.vnum, si.item_vnum"), "orden del C++");
        let cols: Vec<&str> = LOAD_SQL
            .split_once(" FROM ")
            .expect("FROM")
            .0
            .trim_start_matches("SELECT")
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(
            cols,
            ["s.vnum", "s.npc_vnum", "si.item_vnum", "si.count", "ip.flag", "ip.gold"],
            "6 columnas: shop + shop_item + proto (flag/gold para el precio)"
        );
    }
}
