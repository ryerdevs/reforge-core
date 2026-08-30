//! `channel/shop.rs` — tiendas NPC del canal (F6): los handlers del CG_SHOP
//! (50) y del click en el NPC (CG_ON_CLICK 26 → Open), el wire GC_SHOP (38)
//! y la APLICACIÓN PG de la compra/venta (oro + items como UNIDAD ACID —
//! `WorldStore::exchange` = `ItemExchange::exchange_mutated`, materiales→
//! resultado→oro en UNA transacción; NUNCA commits por item).
//!
//! # Wire (verificado campo a campo, 2026-08-13)
//!
//! - `TPacketCGShop` (2 B: header 50 + subheader — `Packet.h:641-645`);
//!   BUY + `{BYTE count, BYTE pos}` (cliente `SendShopBuyPacket`,
//!   PythonNetworkStreamPhaseGameItem.cpp:395-424); SELL + `{BYTE cell}`
//!   (:426-447); SELL2 + `{BYTE cell, BYTE count}` (:449-465).
//! - `TPacketGCShop` (4 B: header 38 + WORD size + subheader —
//!   `Packet.h:1821-1826`); START payload = `owner_vid` DWORD + 40 ×
//!   `packet_shop_item` de 47 B (vnum 4 + price 4 + cheque 4 + count 1 +
//!   display_pos 1 + sockets 3×4 + attrs 7×3 — `GameType.h:348-359` con
//!   ENABLE_CHEQUE_SYSTEM; cheque = 0 para tiendas NPC, `shop.cpp:153`).
//!   El cliente lee `dwVID` de `vecBuffer[0]` y los items en `[4..]`
//!   (PythonNetworkStreamPhaseGame.cpp:1698-1711).
//! - Subheaders de error (sin payload): `Packet.h:1801-1819` — el enum del
//!   server (shop.cpp `Buy`) alinea OK=3, NOT_ENOUGH_MONEY=4, SOLDOUT=6,
//!   INVENTORY_FULL=7, INVALID_POS=8.

use game_core::ecs::{Intent, QuestIntent, ShopEvent, ShopIntent};
use game_core::shop::{self, BuyReceipt, ShopError, ShopItem};

use crate::channel::session::{Outcome, Session};
use crate::channel::{INVENTORY_MAX_NUM, ITEM_COUNT_LIMIT};
use database::economy::{checked_gold_delta, checked_gold_sub};
use database::item::{ItemExchange, ItemRepo, ItemRow};
use protocol::header;
use protocol::world::{
    TItemPos, TPacketGCItemDelDeprecated, TPacketGCItemSet, TPacketGCItemUpdate,
};

/// `SHOP_HOST_ITEM_MAX_NUM` (Packet.h:345) — items del wire.
const SHOP_HOST_ITEM_MAX_NUM: usize = 40;

/// Subheaders GC_SHOP (Packet.h:1801-1819 — valores del enum del server).
const SHOP_SUBHEADER_GC_START: u8 = 0;
const SHOP_SUBHEADER_GC_END: u8 = 1;
/// `MONEY_LOG_SHOP` (log.cpp:113 — economy.rs los valida 1..=8).
const MONEY_LOG_SHOP: i32 = 2;

/// CG_ON_CLICK (26): el click en un NPC — si el mundo resuelve un shop para
/// su vnum (npc_vnum → `player.shop`) y la distancia es válida, emite
/// `Opened` (GC_SHOP START). Fallos → silencio (parity `StartShopping`).
pub async fn click(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() != 5 {
        // TPacketCGOnClick: header + DWORD vid (Packet.h:629-631).
        eprintln!(
            "server_realms: channel conn {}: CG_ON_CLICK malformado ({} B)",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    }
    let npc_vid = u32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]);
    eprintln!(
        "server_realms: channel conn {}: CG_ON_CLICK recibido (npc vid {})",
        session.conn_id, npc_vid
    );
    session.intent(ShopIntent::Open {
        player_vid: session.player_vid(),
        npc_vid,
    }.into())?;
    // F5 quests (wiring 2026-08-13): el mismo click dispara el trigger
    // Chat(vnum) de las quests del NPC (el mundo resuelve el vnum del vid) —
    // si el NPC tiene quests de chat, el diálogo GC_SCRIPT sale (sin quests
    // para el vnum -> sin evento, silencio).
    session.intent(Intent::Quest(QuestIntent::NpcClick {
        player_vid: session.player_vid(),
        npc_vid,
        items: session.inventory_counts(),
    }))?;
    Ok(Outcome::Continue)
}

/// CG_SHOP (50): END/BUY/SELL/SELL2 — los intents al mundo (la validación
/// de estado — shop abierto, pos, precio — la hace el mundo; el oro y el
/// inventario se aplican aquí con la regla pura).
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() < 2 {
        eprintln!(
            "server_realms: channel conn {}: CG_SHOP malformado ({} B)",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    }
    let pv = session.player_vid();
    match pkt[1] {
        0 => {
            // SHOP_SUBHEADER_CG_END.
            session.intent(ShopIntent::Close { player_vid: pv }.into())?;
        }
        1 => {
            // SHOP_SUBHEADER_CG_BUY: + {count, pos} (2 B — el server lee
            // bPos en `c_pData + 1`, input_main.cpp:1054-1063).
            if pkt.len() < 4 {
                eprintln!(
                    "server_realms: channel conn {}: CG_SHOP BUY malformado ({} B)",
                    session.conn_id,
                    pkt.len()
                );
                return Ok(Outcome::Continue);
            }
            session.intent(ShopIntent::Buy { player_vid: pv, pos: pkt[3] }.into())?;
        }
        2 => {
            // SHOP_SUBHEADER_CG_SELL: + {cell} (1 B — input_main.cpp:1065-1075).
            if pkt.len() < 3 {
                eprintln!(
                    "server_realms: channel conn {}: CG_SHOP SELL malformado ({} B)",
                    session.conn_id,
                    pkt.len()
                );
                return Ok(Outcome::Continue);
            }
            session.intent(ShopIntent::Sell {
                player_vid: pv,
                cell: u16::from(pkt[2]),
            }.into())?;
        }
        3 => {
            // SHOP_SUBHEADER_CG_SELL2: + {cell, count} (2 B —
            // input_main.cpp:1077-1088).
            if pkt.len() < 4 {
                eprintln!(
                    "server_realms: channel conn {}: CG_SHOP SELL2 malformado ({} B)",
                    session.conn_id,
                    pkt.len()
                );
                return Ok(Outcome::Continue);
            }
            session.intent(ShopIntent::Sell2 {
                player_vid: pv,
                cell: u16::from(pkt[2]),
                count: u32::from(pkt[3]),
            }.into())?;
        }
        other => {
            eprintln!(
                "server_realms: channel conn {}: CG_SHOP subheader desconocido {other}",
                session.conn_id
            );
        }
    }
    Ok(Outcome::Continue)
}

/// S→C del shop: `ShopEvent` validado del mundo → wire + aplicación PG.
pub(super) async fn emit(session: &mut Session, e: ShopEvent) -> Result<(), String> {
    match e {
        ShopEvent::Opened { npc_vid, items, .. } => {
            // GC_SHOP(START): TPacketGCShop{38, size, 0} + owner_vid + 40
            // items de 47 B (los vacíos en 0 — parity AddGuest llena 40).
            let mut out = Vec::with_capacity(4 + 4 + SHOP_HOST_ITEM_MAX_NUM * 47);
            out.push(header::GC_SHOP);
            out.extend_from_slice(&((4 + 4 + SHOP_HOST_ITEM_MAX_NUM * 47) as u16).to_le_bytes());
            out.push(SHOP_SUBHEADER_GC_START);
            out.extend_from_slice(&npc_vid.to_le_bytes());
            for i in 0..SHOP_HOST_ITEM_MAX_NUM {
                let item = items.iter().find(|it| usize::from(it.display_pos) == i);
                push_shop_item(&mut out, item);
            }
            session.send(&out).await.map_err(|e| format!("enviando GC_SHOP START: {e}"))
        }
        ShopEvent::Closed { .. } => {
            session
                .send(&gc_shop(SHOP_SUBHEADER_GC_END))
                .await
                .map_err(|e| format!("enviando GC_SHOP END: {e}"))
        }
        ShopEvent::BuyResult { pos, vnum, count, price, .. } => {
            apply_buy(session, pos, vnum, count, price).await
        }
        ShopEvent::SellResult { cell, count, .. } => apply_sell(session, cell, count).await,
        ShopEvent::BuyRejected { error, .. } => {
            session
                .send(&gc_shop(error.wire_subheader()))
                .await
                .map_err(|e| format!("enviando GC_SHOP error: {e}"))
        }
        ShopEvent::SellRejected { .. } => {
            // Parity: `CShopManager::Sell` no manda paquetes de error (solo
            // chat — sistema pendiente) — silencio.
            Ok(())
        }
    }
}

/// `TPacketGCShop` (4 B) con el subheader — errores y END.
fn gc_shop(subheader: u8) -> Vec<u8> {
    vec![header::GC_SHOP, 4, 0, subheader]
}

/// Un `packet_shop_item` de 47 B (parity `GameType.h:348-359` con cheque).
fn push_shop_item(out: &mut Vec<u8>, item: Option<&ShopItem>) {
    let (vnum, price, count, display_pos) = match item {
        Some(i) => (i.vnum as u32, i.price as u32, i.count as u8, i.display_pos),
        None => (0, 0, 0, 0),
    };
    out.extend_from_slice(&vnum.to_le_bytes());
    out.extend_from_slice(&price.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // cheque = 0 (NPC shops)
    out.push(count);
    out.push(display_pos);
    out.extend_from_slice(&[0u8; 12]); // sockets
    out.extend_from_slice(&[0u8; 21]); // attrs
}

/// Compra aplicada (parity `CShop::Buy` — shop.cpp:190-403): la regla pura
/// `shop::buy` (oro + hueco/stack) → UNIDAD ACID (oro + item en una tx) →
/// GC_ITEM_SET/UPDATE + GC_POINTS + money_log.
async fn apply_buy(
    session: &mut Session,
    pos: u8,
    vnum: i64,
    count: i64,
    price: i64,
) -> Result<(), String> {
    let item = ShopItem { vnum, count, price, display_pos: pos };
    let gold = i64::from(session.row().gold);
    let receipt = match shop::buy(&session.inventory, gold, &item, ITEM_COUNT_LIMIT, INVENTORY_MAX_NUM) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: {} compra rechazada: {e:?}",
                session.conn_id,
                session.row().name
            );
            session
                .send(&gc_shop(e.wire_subheader()))
                .await
                .map_err(|s| format!("enviando GC_SHOP error: {s}"))?;
            return Ok(());
        }
    };
    let Some(new_gold) = checked_gold_sub(gold, receipt.price) else {
        session
            .send(&gc_shop(ShopError::GoldOverflow.wire_subheader()))
            .await
            .map_err(|s| format!("enviando GC_SHOP error: {s}"))?;
        return Ok(());
    };
    // La unidad ACID: oro (pre→post) + item (stack UPDATE o INSERT nuevo).
    let ex = match receipt {
        BuyReceipt { stack: Some((id, pre, post)), .. } => ItemExchange {
            owner_id: session.row().id,
            materials: vec![(id, pre, post)],
            result: None,
            gold: Some((gold, new_gold)),
        },
        BuyReceipt { stack: None, new_pos, .. } => {
            let id = ItemRepo::new(session.pool.clone())
                .max_id_in_range(100_000_000, 200_000_000)
                .await?
                .map(|m| m + 1)
                .unwrap_or(100_000_000);
            ItemExchange {
                owner_id: session.row().id,
                materials: Vec::new(),
                result: Some((
                    ItemRow {
                        id,
                        window: "INVENTORY".into(),
                        pos: i32::from(new_pos),
                        count,
                        vnum,
                        sockets: [0; 3],
                        attrs: [(0, 0); 7],
                    },
                    session.row().id,
                )),
                gold: Some((gold, new_gold)),
            }
        }
    };
    if let Err(e) = session.store().exchange(&ex).await {
        eprintln!(
            "server_realms: channel conn {}: compra de vnum {vnum}: unidad ACID falló: {e} \
             (el WAL lo re-aplicará al arrancar)",
            session.conn_id
        );
        session
            .send(&gc_shop(SHOP_SUBHEADER_GC_END))
            .await
            .map_err(|s| format!("enviando GC_SHOP END: {s}"))?;
        return Ok(());
    }
    // Memoria + wire.
    session.row_mut().gold = i32::try_from(new_gold)
        .map_err(|e| format!("convirtiendo gold de compra: {e}"))?;
    match receipt {
        BuyReceipt { stack: Some((id, pre, _)), .. } => {
            if let Some(idx) = session.inventory.iter().position(|i| i.id == id) {
                session.inventory[idx].count = pre + count;
                let up = TPacketGCItemUpdate {
                    header: TPacketGCItemUpdate::HEADER,
                    cell: TItemPos {
                        window: TItemPos::WINDOW_INVENTORY,
                        cell: session.inventory[idx].pos as u16,
                    },
                    count: session.inventory[idx].count as u8,
                    sockets: session.inventory[idx].sockets,
                    attrs: session.inventory[idx].attrs,
                };
                session
                    .send(&up.to_bytes())
                    .await
                    .map_err(|e| format!("enviando GC_ITEM_UPDATE: {e}"))?;
            }
        }
        BuyReceipt { stack: None, new_pos, .. } => {
            let row = ItemRow {
                id: ex.result.as_ref().map(|(r, _)| r.id).unwrap_or(0),
                window: "INVENTORY".into(),
                pos: i32::from(new_pos),
                count,
                vnum,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            };
            let set = TPacketGCItemSet {
                header: TPacketGCItemSet::HEADER,
                cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: new_pos },
                vnum: vnum as u32,
                count: count as u8,
                flags: 0,
                anti_flags: 0,
                highlight: 0,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            };
            session
                .send(&set.to_bytes())
                .await
                .map_err(|e| format!("enviando GC_ITEM_SET: {e}"))?;
            session.inventory.push(row);
        }
    }
    send_points_and_log(session, vnum, -price).await
}

/// Venta aplicada (parity `CShopManager::Sell` — shop_manager.cpp:248-349):
/// la regla pura `shop::sell` (item de la celda, precio ÷5 −3%, overflow) →
/// UNIDAD ACID (item consumido + oro en una tx) → GC_ITEM_UPDATE/DEL +
/// GC_POINTS + money_log. Rechazos SILENCIOSOS (parity: el C++ solo manda
/// chat).
async fn apply_sell(session: &mut Session, cell: u16, qty: i64) -> Result<(), String> {
    // GAP documentado: el antiflag del item_proto no tiene query en el canal
    // (la query del proto completo es del lane database) — hoy `false`.
    let gold = i64::from(session.row().gold);
    let proto = {
        let item = session
            .inventory
            .iter()
            .find(|i| i.pos as u16 == cell)
            .map(|i| i.vnum);
        let Some(vnum) = item else {
            return Ok(()); // celda vacía — silencio (parity)
        };
        sell_proto(&session.pool, vnum).await?
    };
    let receipt = match shop::sell(&session.inventory, gold, cell, qty, false, proto) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: {} venta de celda {cell} rechazada: {e:?}",
                session.conn_id,
                session.row().name
            );
            return Ok(());
        }
    };
    let Some(new_gold) = checked_gold_delta(gold, receipt.price) else {
        eprintln!(
            "server_realms: channel conn {}: venta de celda {cell} excede \
             el rango de oro 0..={}: rechazado",
            session.conn_id,
            database::economy::GOLD_MAX
        );
        return Ok(());
    };
    let ex = ItemExchange {
        owner_id: session.row().id,
        materials: vec![receipt.material],
        result: None,
        gold: Some((gold, new_gold)),
    };
    if let Err(e) = session.store().exchange(&ex).await {
        eprintln!(
            "server_realms: channel conn {}: venta de celda {cell}: unidad ACID falló: {e} \
             (el WAL lo re-aplicará al arrancar)",
            session.conn_id
        );
        return Ok(());
    }
    // Memoria + wire.
    session.row_mut().gold = i32::try_from(new_gold)
        .map_err(|e| format!("convirtiendo gold de venta: {e}"))?;
    let mut vnum_log = 0i64;
    if let Some(idx) = session.inventory.iter().position(|i| i.pos as u16 == cell) {
        vnum_log = session.inventory[idx].vnum;
        session.inventory[idx].count = receipt.material.2;
        if session.inventory[idx].count <= 0 {
            let del = TPacketGCItemDelDeprecated::new(
                TItemPos { window: TItemPos::WINDOW_INVENTORY, cell },
                0,
                0,
            );
            session
                .send(&del.to_bytes())
                .await
                .map_err(|e| format!("enviando GC_ITEM_DEL: {e}"))?;
            session.inventory.remove(idx);
        } else {
            let up = TPacketGCItemUpdate {
                header: TPacketGCItemUpdate::HEADER,
                cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell },
                count: session.inventory[idx].count as u8,
                sockets: session.inventory[idx].sockets,
                attrs: session.inventory[idx].attrs,
            };
            session
                .send(&up.to_bytes())
                .await
                .map_err(|e| format!("enviando GC_ITEM_UPDATE: {e}"))?;
        }
    }
    send_points_and_log(session, vnum_log, receipt.price).await
}

/// Los datos del item_proto para la venta (shop_buy_price + flag) - a traves
/// del crate database (ItemRepo::load_sell_proto) y el pool compartido
/// (ADR-0008 2: sin SQL directo fuera del crate).
async fn sell_proto(pool: &database::pool::PgPool, vnum: i64) -> Result<shop::SellProto, String> {
    let (shop_buy_price, flag) =
        database::item::ItemRepo::new(pool.clone()).load_sell_proto(vnum).await?;
    Ok(shop::SellProto {
        shop_buy_price,
        count_per_1gold: flag & game_core::shop::ITEM_FLAG_COUNT_PER_1GOLD != 0,
    })
}

/// GC_POINTS (oro nuevo) + `SendMoneyLog(MONEY_LOG_SHOP, vnum, delta)` —
/// parity shop.cpp:395 (compra: delta negativo) / shop_manager.cpp:338
/// (venta: positivo). El money_log es un INSERT directo (audit, no va en la
/// unidad ACID — parity: el C++ lo manda al db aparte); un fallo del log
/// NO bloquea la operación (best-effort, logueado).
async fn send_points_and_log(
    session: &mut Session,
    vnum: i64,
    gold_delta: i64,
) -> Result<(), String> {
    session
        .send(&game_core::packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
    match i32::try_from(gold_delta) {
        Ok(gold_delta) => {
            if let Err(e) = database::economy::EconomyRepo::new(session.pool.clone())
                .money_log(MONEY_LOG_SHOP, vnum as i32, gold_delta)
                .await
            {
                eprintln!(
                    "server_realms: channel conn {}: money_log SHOP (vnum {vnum}, {gold_delta}): {e}",
                    session.conn_id
                );
            }
        }
        Err(e) => eprintln!(
            "server_realms: channel conn {}: money_log SHOP (vnum {vnum}, {gold_delta}) \
             fuera de i32: {e}",
            session.conn_id
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::shop::ShopError;

    /// El wire del shop (verificado campo a campo contra el cliente):
    /// TPacketGCShop = 4 B (header 38 + WORD size + subheader — Packet.h:
    /// 1821-1826); packet_shop_item = 47 B con cheque (GameType.h:348-359 +
    /// Locale_inc.h:110); START = 4 + owner_vid 4 + 40×47 = 1888 B.
    #[test]
    fn shop_wire_sizes_parity() {
        assert_eq!(header::GC_SHOP, 38, "HEADER_GC_SHOP (Packet.h:183)");
        // HEADER_CG_SHOP = 50 (Packet.h:62) — literal del dispatch en
        // game.rs (el protocol crate no lo define — GAP del lane protocol).
        let mut item = Vec::new();
        push_shop_item(&mut item, None);
        assert_eq!(item.len(), 47, "packet_shop_item: 4+4+4+1+1+12+21");
        assert_eq!(gc_shop(0).len(), 4, "TPacketGCShop: header+size+subheader");
        let start_len = 4 + 4 + SHOP_HOST_ITEM_MAX_NUM * 47;
        assert_eq!(start_len, 1888, "START: TPacketGCShop + owner_vid + 40 items");
        let mut out = Vec::new();
        out.push(header::GC_SHOP);
        out.extend_from_slice(&(start_len as u16).to_le_bytes());
        out.push(SHOP_SUBHEADER_GC_START);
        out.extend_from_slice(&7u32.to_le_bytes());
        for i in 0..SHOP_HOST_ITEM_MAX_NUM {
            push_shop_item(&mut out, None);
        }
        assert_eq!(out.len(), 4 + 4 + 1880, "el paquete completo del START");
        // Los subheaders de error (Packet.h:1801-1819).
        assert_eq!(ShopError::InvalidPos.wire_subheader(), 8);
        assert_eq!(ShopError::SoldOut.wire_subheader(), 6);
        assert_eq!(ShopError::NotEnoughMoney.wire_subheader(), 4);
        assert_eq!(ShopError::InventoryFull.wire_subheader(), 7);
    }
}
