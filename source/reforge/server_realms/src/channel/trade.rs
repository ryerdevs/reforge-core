//! `channel/trade.rs` — intercambio jugador↔jugador del canal (F6): el
//! handler del CG_EXCHANGE (27), el wire GC_EXCHANGE (42, 47 B) y el
//! EJECUTOR del commit ACID (dupe-critical — `ItemExchange` +
//! `WorldStore::exchange`; nunca commits por item).
//!
//! # Wire (verificado campo a campo, 2026-08-13)
//!
//! - `TPacketCGExchange` (10 B: header 27 + subheader + arg1 DWORD + arg2
//!   BYTE + Pos TItemPos 3 B — `Packet.h:660-667`; el server lee
//!   `command_exchange`, input_main.cpp:1111-1272): START arg1=vid target;
//!   ITEM_ADD Pos=item del inventario, arg2=display_pos; ITEM_DEL
//!   arg1=display_pos; ELK_ADD arg1=oro; ACCEPT/CANCEL sin args.
//! - `TPacketGCExchange` (47 B: header 42 + subheader + is_me + arg1 4 +
//!   arg2 TItemPos 3 + arg3 4 + alValues 3×4 + aAttr 7×3 — `Packet.h:
//!   1828-1838`; server `packet.h:1210-1220` con ITEM_SOCKET_MAX_NUM=3 /
//!   ITEM_ATTRIBUTE_MAX_NUM=7, item_length.h:13,25).
//! - Subheaders alineados ambos lados (packet.h:1222-1236 = `Packet.h:
//!   1842-1852`): START=0, ITEM_ADD=1, ITEM_DEL=2, GOLD_ADD=3, ACCEPT=4,
//!   END=5, ALREADY=6, LESS_GOLD=7.

use database::item::{ItemRepo, ItemRow};
use database::player::PlayerRepo;
use game_core::ecs::{TradeEvent, TradeIntent};
use game_core::trade::{self, TradeCommitPlan};
use protocol::world::{TItemPos, TPacketGCItemDelDeprecated, TPacketGCItemSet};

use crate::channel::session::{Outcome, Session};
use crate::channel::INVENTORY_MAX_NUM;

/// `HEADER_GC_EXCHANGE` (Packet.h:188) — el protocol crate no lo define.
const GC_EXCHANGE: u8 = 42;

/// Subheaders GC_EXCHANGE (Packet.h:1842-1852 — alineados con el server).
const EXCHANGE_SUBHEADER_GC_START: u8 = 0;
const EXCHANGE_SUBHEADER_GC_ITEM_ADD: u8 = 1;
const EXCHANGE_SUBHEADER_GC_ITEM_DEL: u8 = 2;
const EXCHANGE_SUBHEADER_GC_GOLD_ADD: u8 = 3;
const EXCHANGE_SUBHEADER_GC_ACCEPT: u8 = 4;
const EXCHANGE_SUBHEADER_GC_END: u8 = 5;
const EXCHANGE_SUBHEADER_GC_ALREADY: u8 = 6;
const EXCHANGE_SUBHEADER_GC_LESS_GOLD: u8 = 7;
/// `GOLD_MAX` (length.h:80) — tope del oro.
const GOLD_MAX: i64 = 2_000_000_000;

/// CG_EXCHANGE (27): parsea el subheader y manda el intent al mundo. La
/// validación de ORO del ELK_ADD vive aquí (parity input_main.cpp:1214-1234
/// — `AddGold` rechaza `gold > row.gold` con GC_LESS_GOLD); el overflow
/// GOLD_MAX del receptor se valida en el COMMIT (desviación documentada: el
/// canal no conoce el oro del partner — el ejecutor valida ambos posts).
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() != 10 {
        eprintln!(
            "server_realms: channel conn {}: CG_EXCHANGE malformado ({} B, esperaba 10)",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    }
    let pv = session.player_vid();
    let sub = pkt[1];
    let arg1 = u32::from_le_bytes([pkt[2], pkt[3], pkt[4], pkt[5]]);
    let arg2 = pkt[6];
    let pos = TItemPos { window: pkt[7], cell: u16::from_le_bytes([pkt[8], pkt[9]]) };
    match sub {
        0 => {
            // EXCHANGE_SUBHEADER_CG_START — arg1 = vid del target.
            session.intent(TradeIntent::Start { player_vid: pv, target_vid: arg1 }.into())?;
        }
        1 => {
            // EXCHANGE_SUBHEADER_CG_ITEM_ADD — Pos = item del inventario,
            // arg2 = display_pos de la ventana. La fila completa la manda el
            // canal (el commit necesita id/count/vnum/sockets/attrs).
            let Some(row) = session
                .inventory
                .iter()
                .find(|i| i.window == "INVENTORY" && i.pos as u16 == pos.cell)
                .cloned()
            else {
                return Ok(Outcome::Continue); // parity: AddItem false (silencioso)
            };
            session.intent(TradeIntent::ItemAdd {
                player_vid: pv,
                row,
                display_pos: arg2,
            }.into())?;
        }
        2 => {
            // EXCHANGE_SUBHEADER_CG_ITEM_DEL — arg1 = display_pos.
            session.intent(TradeIntent::ItemDel {
                player_vid: pv,
                display_pos: arg1 as u8,
            }.into())?;
        }
        3 => {
            // EXCHANGE_SUBHEADER_CG_ELK_ADD — arg1 = oro.
            let gold = i64::from(arg1);
            if gold <= 0 || i64::from(session.row().gold) < gold {
                // Parity `AddGold` (exchange.cpp:240-262): LESS_GOLD al
                // propio owner.
                session
                    .send(&gc_exchange(EXCHANGE_SUBHEADER_GC_LESS_GOLD, 1, 0, TItemPos { window: 0, cell: 0 }, 0, [0; 3], [(0, 0); 7]))
                    .await
                    .map_err(|e| format!("enviando GC_EXCHANGE LESS_GOLD: {e}"))?;
                return Ok(Outcome::Continue);
            }
            session.intent(TradeIntent::GoldAdd { player_vid: pv, gold }.into())?;
        }
        4 => {
            // EXCHANGE_SUBHEADER_CG_ACCEPT.
            session.intent(TradeIntent::Accept { player_vid: pv }.into())?;
        }
        5 => {
            // EXCHANGE_SUBHEADER_CG_CANCEL.
            session.intent(TradeIntent::Cancel { player_vid: pv }.into())?;
        }
        other => {
            eprintln!(
                "server_realms: channel conn {}: CG_EXCHANGE subheader desconocido {other}",
                session.conn_id
            );
        }
    }
    Ok(Outcome::Continue)
}

/// S→C del trade: eventos validados del mundo → GC_EXCHANGE + memoria.
/// El `Commit` corre la unidad ACID (dupe-critical) y responde
/// CommitOk/CommitFail al mundo.
pub(super) async fn emit(session: &mut Session, e: TradeEvent) -> Result<(), String> {
    match e {
        TradeEvent::Start { other_vid, .. } => {
            session
                .send(&gc_exchange(EXCHANGE_SUBHEADER_GC_START, 0, other_vid, TItemPos { window: 0, cell: 0 }, 0, [0; 3], [(0, 0); 7]))
                .await
                .map_err(|e| format!("enviando GC_EXCHANGE START: {e}"))
        }
        TradeEvent::Already { .. } => {
            session
                .send(&gc_exchange(EXCHANGE_SUBHEADER_GC_ALREADY, 0, 0, TItemPos { window: 0, cell: 0 }, 0, [0; 3], [(0, 0); 7]))
                .await
                .map_err(|e| format!("enviando GC_EXCHANGE ALREADY: {e}"))
        }
        TradeEvent::ItemAdded { is_me, display_pos, vnum, count, sockets, attrs, .. } => {
            session
                .send(&gc_exchange(
                    EXCHANGE_SUBHEADER_GC_ITEM_ADD,
                    u8::from(is_me),
                    vnum as u32,
                    TItemPos { window: 0, cell: u16::from(display_pos) },
                    count as u32,
                    sockets,
                    attrs,
                ))
                .await
                .map_err(|e| format!("enviando GC_EXCHANGE ITEM_ADD: {e}"))
        }
        TradeEvent::ItemRemoved { is_me, display_pos, .. } => {
            session
                .send(&gc_exchange(
                    EXCHANGE_SUBHEADER_GC_ITEM_DEL,
                    u8::from(is_me),
                    u32::from(display_pos),
                    TItemPos { window: 0, cell: 0 },
                    0,
                    [0; 3],
                    [(0, 0); 7],
                ))
                .await
                .map_err(|e| format!("enviando GC_EXCHANGE ITEM_DEL: {e}"))
        }
        TradeEvent::GoldAdded { is_me, gold, .. } => {
            session
                .send(&gc_exchange(EXCHANGE_SUBHEADER_GC_GOLD_ADD, u8::from(is_me), gold as u32, TItemPos { window: 0, cell: 0 }, 0, [0; 3], [(0, 0); 7]))
                .await
                .map_err(|e| format!("enviando GC_EXCHANGE GOLD_ADD: {e}"))
        }
        TradeEvent::AcceptState { is_me, accept, .. } => {
            session
                .send(&gc_exchange(EXCHANGE_SUBHEADER_GC_ACCEPT, u8::from(is_me), u32::from(accept), TItemPos { window: 0, cell: 0 }, 0, [0; 3], [(0, 0); 7]))
                .await
                .map_err(|e| format!("enviando GC_EXCHANGE ACCEPT: {e}"))
        }
        TradeEvent::Commit { plan, .. } => apply_commit(session, &plan).await,
        TradeEvent::Done { gold_delta, received, delivered, .. } => {
            apply_done(session, gold_delta, &received, &delivered).await
        }
        TradeEvent::Cancelled { .. } => {
            session
                .send(&gc_exchange(EXCHANGE_SUBHEADER_GC_END, 0, 0, TItemPos { window: 0, cell: 0 }, 0, [0; 3], [(0, 0); 7]))
                .await
                .map_err(|e| format!("enviando GC_EXCHANGE END: {e}"))
        }
    }
}

/// El wire GC_EXCHANGE (47 B packed — `Packet.h:1828-1838`).
fn gc_exchange(
    subheader: u8,
    is_me: u8,
    arg1: u32,
    arg2: TItemPos,
    arg3: u32,
    sockets: [i64; 3],
    attrs: [(i16, i16); 7],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(47);
    out.push(GC_EXCHANGE);
    out.push(subheader);
    out.push(is_me);
    out.extend_from_slice(&arg1.to_le_bytes());
    out.push(arg2.window);
    out.extend_from_slice(&arg2.cell.to_le_bytes());
    out.extend_from_slice(&arg3.to_le_bytes());
    for s in sockets {
        out.extend_from_slice(&(s as i32).to_le_bytes());
    }
    for (t, v) in attrs {
        out.push(t as u8);
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Rechazo del commit: log + `CommitFail` al mundo (que cancela a ambos).
async fn commit_fail(
    session: &mut Session,
    executor: u32,
    partner: u32,
    why: String,
) -> Result<(), String> {
    eprintln!(
        "server_realms: channel conn {}: trade {}<->{}: commit rechazado: {why}",
        session.conn_id, executor, partner
    );
    session.intent(TradeIntent::CommitFail { player_vid: executor }.into())?;
    Ok(())
}

/// EJECUTOR del commit (dupe-critical — el plan lo validó el mundo):
/// 1. re-valida SU lado (parity `CExchange::Check` — exchange.cpp:283-311):
///    sus items ofrecidos siguen en su inventario con el count ofrecido y
///    su oro alcanza;
/// 2. lee el oro del partner fresco de PG y valida su post;
/// 3. construye las unidades ACID (`trade::build_commit_units`) y las
///    ejecuta con `WorldStore::exchange` (cada unidad = UNA transacción con
///    audit; los ids nuevos + guards hacen el conjunto crash-consistente —
///    ver `game_core::trade`);
/// 4. responde CommitOk/CommitFail (el mundo emite Done/Cancelled).
async fn apply_commit(session: &mut Session, plan: &TradeCommitPlan) -> Result<(), String> {
    // (1) Check del lado del ejecutor: items aún ofrecidos (id + count) y
    // oro suficiente.
    for offer in &plan.offers_executor {
        let Some(mine) = session.inventory.iter().find(|i| i.id == offer.id) else {
            return commit_fail(
                session,
                plan.executor,
                plan.partner,
                format!("item {} ya no está en el inventario", offer.id),
            )
            .await;
        };
        if mine.count != offer.count {
            return commit_fail(
                session,
                plan.executor,
                plan.partner,
                format!("item {} cambió de count", offer.id),
            )
            .await;
        }
    }
    let gold_now = i64::from(session.row().gold);
    let gold_post_own = gold_now - plan.gold_executor;
    if gold_post_own < 0 {
        return commit_fail(session, plan.executor, plan.partner, "oro insuficiente".into()).await;
    }
    // (2) El oro del partner (fresco — parity `CExchange::Check` sobre el
    // company) + overflow GOLD_MAX del post propio.
    let partner_gold = PlayerRepo::new(&session.config.pg_conn)
        .load(i64::from(plan.partner))
        .await?
        .map(|r| i64::from(r.gold))
        .unwrap_or(0);
    if partner_gold - plan.gold_partner < 0 {
        return commit_fail(session, plan.executor, plan.partner, "oro del partner insuficiente".into())
            .await;
    }
    if gold_post_own + plan.gold_partner > GOLD_MAX {
        return commit_fail(session, plan.executor, plan.partner, "GOLD_MAX del ejecutor excedido".into())
            .await;
    }
    // (3) Las unidades ACID (ids nuevos del rango — patrón del split).
    let base = ItemRepo::new(&session.config.pg_conn)
        .max_id_in_range(
            trade::ITEM_ID_RANGE_MIN,
            trade::ITEM_ID_RANGE_MAX,
        )
        .await?
        .map(|m| m + 1)
        .unwrap_or(trade::ITEM_ID_RANGE_MIN);
    let mut next_id = base;
    let mut next = move || {
        let id = next_id;
        next_id += 1;
        id
    };
    let units = trade::build_commit_units(plan, gold_now, partner_gold, &mut next);
    for unit in &units {
        if let Err(e) = session.store().exchange(unit).await {
            return commit_fail(session, plan.executor, plan.partner, format!("unidad ACID falló: {e}"))
                .await;
        }
    }
    // (4) Confirmación.
    session.intent(TradeIntent::CommitOk { player_vid: plan.executor }.into())?;
    eprintln!(
        "server_realms: channel conn {}: trade {}<->{} COMMITEADO ({} unidades ACID)",
        session.conn_id,
        plan.executor,
        plan.partner,
        units.len()
    );
    Ok(())
}

/// Done (a ambos — tras el CommitOk): memoria (oro + inventario) + wire
/// (GC_EXCHANGE END + GC_ITEM_SET de los recibidos + GC_ITEM_DEL de los
/// entregados + GC_POINTS). La DB ya commiteó — los recibidos se RE-COLOCAN
/// en celdas libres y se re-upsertean (idempotente por id; la fila del
/// commit llevaba el pos del oferente).
async fn apply_done(
    session: &mut Session,
    gold_delta: i64,
    received: &[game_core::ecs::TradeReceivedItem],
    delivered: &[ItemRow],
) -> Result<(), String> {
    // Items entregados: GC_ITEM_DEL + salen del inventario local.
    for d in delivered {
        if let Some(idx) = session.inventory.iter().position(|i| i.id == d.id) {
            let del = TPacketGCItemDelDeprecated::new(
                TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: session.inventory[idx].pos as u16 },
                d.vnum as u32,
                0,
            );
            session
                .send(&del.to_bytes())
                .await
                .map_err(|e| format!("enviando GC_ITEM_DEL (trade): {e}"))?;
            session.inventory.remove(idx);
        }
    }
    // Items recibidos: re-colocar en la primera celda libre + GC_ITEM_SET +
    // re-upsert (la DB ya tiene la fila — el pos se corrige).
    for r in received {
        let occupied: std::collections::HashSet<i32> =
            session.inventory.iter().map(|i| i.pos).collect();
        let cell = (0..INVENTORY_MAX_NUM)
            .find(|c| !occupied.contains(&i32::from(*c)))
            .unwrap_or(0);
        let mut row = r.row.clone();
        row.pos = i32::from(cell);
        let set = TPacketGCItemSet {
            header: TPacketGCItemSet::HEADER,
            cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell },
            vnum: row.vnum as u32,
            count: row.count as u8,
            flags: 0,
            anti_flags: 0,
            highlight: 0,
            sockets: row.sockets,
            attrs: row.attrs,
        };
        session
            .send(&set.to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_SET (trade): {e}"))?;
        // El re-upsert corrige el pos (idempotente por id — la fila ya
        // existe del commit).
        ItemRepo::new(&session.config.pg_conn)
            .upsert(&row, session.row().id)
            .await?;
        session.inventory.push(row);
    }
    // Oro + cierre del window + puntos.
    session.row_mut().gold = (i64::from(session.row().gold) + gold_delta) as i32;
    session
        .send(&gc_exchange(EXCHANGE_SUBHEADER_GC_END, 0, 0, TItemPos { window: 0, cell: 0 }, 0, [0; 3], [(0, 0); 7]))
        .await
        .map_err(|e| format!("enviando GC_EXCHANGE END: {e}"))?;
    session
        .send(&game_core::packets::points_packet(session.row(), session.next_exp).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El wire del exchange (verificado campo a campo contra el cliente):
    /// TPacketGCExchange = 47 B packed (Packet.h:1828-1838 — 1+1+1+4+3+4+
    /// 12+21); TPacketCGExchange = 10 B (Packet.h:660-667). Los subheaders
    /// alineados (packet.h:1222-1236 = Packet.h:1842-1852).
    #[test]
    fn exchange_wire_sizes_parity() {
        assert_eq!(GC_EXCHANGE, 42, "HEADER_GC_EXCHANGE (Packet.h:188)");
        let pkt = gc_exchange(
            EXCHANGE_SUBHEADER_GC_ITEM_ADD,
            1,
            0x01020304,
            TItemPos { window: 0, cell: 0x0506 },
            7,
            [1, 2, 3],
            [(4, 5), (6, 7), (8, 9), (10, 11), (12, 13), (14, 15), (16, 17)],
        );
        assert_eq!(pkt.len(), 47, "TPacketGCExchange: 1+1+1+4+3+4+3×4+7×3");
        assert_eq!(pkt[0], 42);
        assert_eq!(pkt[1], 1, "subheader ITEM_ADD");
        assert_eq!(pkt[2], 1, "is_me");
        assert_eq!(&pkt[3..7], &[4, 3, 2, 1], "arg1 LE");
        assert_eq!(pkt[7], 0, "arg2.window (RESERVED)");
        assert_eq!(&pkt[8..10], &[6, 5], "arg2.cell LE");
        assert_eq!(&pkt[10..14], &[7, 0, 0, 0], "arg3 LE");
        assert_eq!(&pkt[14..18], &[1, 0, 0, 0], "sockets[0] LE");
        assert_eq!(pkt[26], 4, "aAttr[0].bType");
        assert_eq!(&pkt[27..29], &[5, 0], "aAttr[0].sValue LE");
        assert_eq!(pkt[44], 16, "aAttr[6].bType (26 + 6×3)");
        assert_eq!(&pkt[45..47], &[17, 0], "aAttr[6].sValue — el último campo");
        // Subheaders (valores del enum — alineados ambos lados).
        assert_eq!(EXCHANGE_SUBHEADER_GC_START, 0);
        assert_eq!(EXCHANGE_SUBHEADER_GC_ITEM_ADD, 1);
        assert_eq!(EXCHANGE_SUBHEADER_GC_ITEM_DEL, 2);
        assert_eq!(EXCHANGE_SUBHEADER_GC_GOLD_ADD, 3);
        assert_eq!(EXCHANGE_SUBHEADER_GC_ACCEPT, 4);
        assert_eq!(EXCHANGE_SUBHEADER_GC_END, 5);
        assert_eq!(EXCHANGE_SUBHEADER_GC_ALREADY, 6);
        assert_eq!(EXCHANGE_SUBHEADER_GC_LESS_GOLD, 7);
    }
}
