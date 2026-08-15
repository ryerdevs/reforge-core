//! Lane SOCIAL (F6, 2026-08-13): TIENDAS NPC (compra/venta) + INTERCAMBIO
//! jugador↔jugador — el `handle_social` del `WorldSim` (la trampa N1 ahora
//! tiene brazos reales; guild/party crecen aquí después).
//!
//! # Reparto de responsabilidades (documentado)
//!
//! - El MUNDO valida el estado compartido: shop abierto por jugador
//!   (`open_shops`), par de trade (`trades`), distancias, existencia de
//!   entidades — y emite eventos VALIDADOS (ShopEvent/TradeEvent).
//! - El CANAL (tiene el WorldStore/Batcher) aplica la parte PG: oro + items
//!   como unidad ACID (`ItemExchange`) y traduce al wire (GC_SHOP/
//!   GC_EXCHANGE) en `channel/social.rs`.
//! - Las reglas PURAS viven en `game_core::shop` / `game_core::trade`
//!   (testeadas sin PG).

use std::collections::HashMap;

use bevy_ecs::prelude::*;

use crate::combat::distance_approx;
use crate::ecs::components::{Mob, Position};
use crate::ecs::events::{
    NpcEvent, ShopEvent, ShopIntent, SocialEvent, SocialIntent, TradeEvent, TradeIntent,
    TradeReceivedItem,
};
use crate::ecs::resources::NpcIndex;
use crate::ecs::world::WorldSim;
use crate::shop::{Shop, ShopError, ShopRepo, SHOP_MAX_DISTANCE};
use crate::trade::{TradeCommitPlan, TradeSession};
use database::item::ItemRow;

/// La tabla de tiendas del canal (recurso — cargada UNA vez con
/// `WorldSim::load_shops`; estática en el runtime). `npc_vnum → Shop`.
#[derive(Resource, Debug, Default)]
pub(crate) struct ShopTable(pub HashMap<i64, Shop>);

/// Shop abierto de un jugador (F6): `npc_vnum` (la clave de la `ShopTable`)
/// y el vid del NPC — el wire GC_SHOP START lleva el owner_vid (parity
/// `CShop::AddGuest`, shop.cpp:421-480).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShopOpen {
    pub npc_vnum: i64,
    pub npc_vid: u32,
}

/// El par de trade del mundo: la sesión pura + el mapeo vid → lado.
#[derive(Debug, Clone)]
pub(crate) struct TradePair {
    pub session: TradeSession,
    /// `player_vid → 0|1` (ambos vids del par).
    pub side_of: HashMap<u32, usize>,
}

impl TradePair {
    fn new(a: u32, b: u32) -> Self {
        Self {
            session: TradeSession::default(),
            side_of: HashMap::from([(a, 0usize), (b, 1usize)]),
        }
    }

    fn side_of(&self, player_vid: u32) -> usize {
        self.side_of[&player_vid]
    }

    /// El otro jugador del par.
    pub(crate) fn other(&self, player_vid: u32) -> u32 {
        *self
            .side_of
            .iter()
            .find(|(v, _)| **v != player_vid)
            .map(|(v, _)| v)
            .expect("trade pair: siempre hay dos")
    }
}

impl WorldSim {
    /// Carga (una vez) la tabla de tiendas (`ShopRepo::load` — PG). El
    /// canal la llama al boot (patrón `load_skills`). Errores → `Err` (el
    /// canal degrada a tiendas desactivadas).
    pub async fn load_shops(&mut self, repo: &ShopRepo) -> Result<(), String> {
        if !self.world.resource::<ShopTable>().0.is_empty() {
            return Ok(());
        }
        let shops = repo.load().await?;
        let map: HashMap<i64, Shop> = shops.into_iter().map(|s| (s.npc_vnum, s)).collect();
        self.world.resource_mut::<ShopTable>().0 = map;
        Ok(())
    }

    /// Delegado de `Intent::Social` (C3 + N1): despacha shop/trade. Los
    /// eventos salen por el outbox del tick (el canal los enruta por
    /// `player_vid`).
    pub(crate) fn handle_social(&mut self, s: SocialIntent, now_ms: u64) -> Vec<NpcEvent> {
        let _ = now_ms; // los cooldowns sociales futuros lo usan
        match s {
            SocialIntent::Shop(i) => self.handle_shop(i),
            SocialIntent::Trade(i) => self.handle_trade(i),
        }
    }

    // ---------------------------------------------------------------------
    // SHOP (tiendas NPC)
    // ---------------------------------------------------------------------

    /// Shop abierto por jugador (si lo tiene) — helper de los handlers.
    fn shop_of(&self, player_vid: u32) -> Option<&ShopOpen> {
        self.open_shops.get(&player_vid)
    }

    fn handle_shop(&mut self, intent: ShopIntent) -> Vec<NpcEvent> {
        match intent {
            ShopIntent::Open { player_vid, npc_vid } => {
                // Parity `StartShopping` (shop_manager.cpp:102-152):
                // distancia < SHOP_MAX_DISTANCE (1000), vivo, sin otro
                // window de trade — y el NPC es de un shop conocido.
                if self.open_shops.contains_key(&player_vid) {
                    return Vec::new();
                }
                if self.trades.contains_key(&player_vid) {
                    return Vec::new(); // PREVENT_TRADE_WINDOW
                }
                let Some(shop) = self.shop_by_npc(npc_vid) else {
                    // Fix diag 2026-08-15: loguear el vnum del NPC clicado
                    // sin shop (el silencio parity impide saber cuál es).
                    let vnum = self
                        .world
                        .resource::<NpcIndex>()
                        .0
                        .get(&npc_vid)
                        .and_then(|e| self.world.get_entity(*e).ok())
                        .and_then(|ent| ent.get::<Mob>().map(|m| m.vnum));
                    eprintln!(
                        "world: shop Open vid {npc_vid} — NPC vnum {:?} SIN shop \
                         en la tabla (silence parity)",
                        vnum
                    );
                    return Vec::new();
                };
                let Some((px, py)) = self.player_pos(player_vid) else { return Vec::new() };
                let Some((nx, ny)) = self.entity_pos(npc_vid) else { return Vec::new() };
                if distance_approx(px - nx, py - ny) >= SHOP_MAX_DISTANCE as i32 {
                    return Vec::new(); // silencioso (parity)
                }
                let items = shop.items.clone();
                self.open_shops.insert(
                    player_vid,
                    ShopOpen { npc_vnum: shop.npc_vnum, npc_vid },
                );
                vec![SocialEvent::Shop(ShopEvent::Opened {
                    player_vid,
                    npc_vid,
                    items,
                })
                .into()]
            }
            ShopIntent::Close { player_vid } => {
                if self.open_shops.remove(&player_vid).is_some() {
                    return vec![SocialEvent::Shop(ShopEvent::Closed { player_vid }).into()];
                }
                Vec::new()
            }
            ShopIntent::Buy { player_vid, pos } => {
                let Some(open) = self.shop_of(player_vid) else { return Vec::new() };
                // La tabla está keyed por npc_vnum (el shop del NPC — la
                // misma clave que resolvió el Open).
                let Some(shop) = self.world.resource::<ShopTable>().0.get(&open.npc_vnum).cloned()
                else {
                    return Vec::new();
                };
                let Some(item) = shop.items.get(pos as usize).copied() else {
                    return vec![SocialEvent::Shop(ShopEvent::BuyRejected {
                        player_vid,
                        pos,
                        error: ShopError::InvalidPos,
                    })
                    .into()];
                };
                // Parity `CShop::Buy` (shop.cpp:219-223): price <= 0 →
                // rechazo (el precio ya viene resuelto del load; defensivo).
                if item.price <= 0 {
                    return vec![SocialEvent::Shop(ShopEvent::BuyRejected {
                        player_vid,
                        pos,
                        error: ShopError::NotEnoughMoney,
                    })
                    .into()];
                }
                vec![SocialEvent::Shop(ShopEvent::BuyResult {
                    player_vid,
                    pos,
                    vnum: item.vnum,
                    count: item.count,
                    price: item.price,
                })
                .into()]
            }
            ShopIntent::Sell { player_vid, cell } => self.shop_sell_gate(player_vid, cell, 0),
            ShopIntent::Sell2 { player_vid, cell, count } => {
                self.shop_sell_gate(player_vid, cell, i64::from(count))
            }
        }
    }

    /// Gate de venta: solo con shop abierto (parity `CShopManager::Sell` —
    /// sin shop → return silencioso, shop_manager.cpp:257-267). El resto de
    /// la validación (item de la celda, precio, overflow) vive en el canal
    /// con la regla pura `shop::sell`.
    fn shop_sell_gate(&self, player_vid: u32, cell: u16, count: i64) -> Vec<NpcEvent> {
        if self.shop_of(player_vid).is_none() {
            return Vec::new();
        }
        vec![SocialEvent::Shop(ShopEvent::SellResult { player_vid, cell, count }).into()]
    }

    /// El shop del NPC (npc_vnum → tabla cargada).
    fn shop_by_npc(&self, npc_vid: u32) -> Option<Shop> {
        let e = *self.world.resource::<NpcIndex>().0.get(&npc_vid)?;
        let ent = self.world.get_entity(e).ok()?;
        let vnum = ent.get::<Mob>()?.vnum;
        self.world.resource::<ShopTable>().0.get(&vnum).cloned()
    }

    /// Posición del jugador (unidades).
    fn player_pos(&self, player_vid: u32) -> Option<(i32, i32)> {
        let e = *self.players.get(&player_vid)?;
        let ent = self.world.get_entity(e).ok()?;
        let pos = ent.get::<Position>()?;
        Some((pos.x, pos.y))
    }

    /// Posición de una entidad (mob/npc/player).
    fn entity_pos(&self, vid: u32) -> Option<(i32, i32)> {
        let e = *self.world.resource::<NpcIndex>().0.get(&vid)?;
        let ent = self.world.get_entity(e).ok()?;
        let pos = ent.get::<Position>()?;
        Some((pos.x, pos.y))
    }

    // ---------------------------------------------------------------------
    // TRADE (intercambio jugador↔jugador)
    // ---------------------------------------------------------------------

    fn handle_trade(&mut self, intent: TradeIntent) -> Vec<NpcEvent> {
        match intent {
            TradeIntent::Start { player_vid, target_vid } => {
                self.trade_start(player_vid, target_vid)
            }
            TradeIntent::ItemAdd { player_vid, row, display_pos } => {
                let Some(pair) = self.trades.get(&player_vid).cloned() else { return Vec::new() };
                let mut pair = pair.lock().expect("trade pair lock");
                let side = pair.side_of(player_vid);
                let other = pair.other(player_vid);
                match pair.session.add_item(side, row.clone(), display_pos) {
                    Ok(()) => {
                        // Parity `exchange_packet(GC_ITEM_ADD)` — a AMBOS
                        // (exchange.cpp:191-205): is_me distingue la ventana.
                        let event = |is_me: bool, pv: u32| {
                            SocialEvent::Trade(TradeEvent::ItemAdded {
                                player_vid: pv,
                                is_me,
                                display_pos,
                                vnum: row.vnum,
                                count: row.count,
                                sockets: row.sockets,
                                attrs: row.attrs,
                            })
                        };
                        vec![event(true, player_vid).into(), event(false, other).into()]
                    }
                    Err(_) => Vec::new(), // silencioso (parity: AddItem false)
                }
            }
            TradeIntent::ItemDel { player_vid, display_pos } => {
                let Some(pair) = self.trades.get(&player_vid).cloned() else { return Vec::new() };
                let mut pair = pair.lock().expect("trade pair lock");
                let side = pair.side_of(player_vid);
                let other = pair.other(player_vid);
                if !pair.session.remove_item(side, display_pos) {
                    return Vec::new();
                }
                let event = |is_me: bool, pv: u32| {
                    SocialEvent::Trade(TradeEvent::ItemRemoved {
                        player_vid: pv,
                        is_me,
                        display_pos,
                    })
                };
                vec![event(true, player_vid).into(), event(false, other).into()]
            }
            TradeIntent::GoldAdd { player_vid, gold } => {
                let Some(pair) = self.trades.get(&player_vid).cloned() else { return Vec::new() };
                let mut pair = pair.lock().expect("trade pair lock");
                let side = pair.side_of(player_vid);
                let other = pair.other(player_vid);
                if pair.session.add_gold(side, gold).is_err() {
                    return Vec::new(); // silencioso (parity AddGold false)
                }
                let event = |is_me: bool, pv: u32| {
                    SocialEvent::Trade(TradeEvent::GoldAdded { player_vid: pv, is_me, gold })
                };
                vec![event(true, player_vid).into(), event(false, other).into()]
            }
            TradeIntent::Accept { player_vid } => self.trade_accept(player_vid),
            TradeIntent::Cancel { player_vid } => self.trade_cancel(player_vid, false),
            TradeIntent::CommitOk { player_vid } => {
                // El ejecutor confirmó: Done a AMBOS (memory+wire) y libera
                // (AMBAS claves del par).
                let Some(pair) = self.trades.remove(&player_vid) else { return Vec::new() };
                let pair = pair.lock().expect("trade pair lock");
                let other = pair.other(player_vid);
                self.trades.remove(&other);
                let side_exec = pair.side_of(player_vid);
                let side_other = 1 - side_exec;
                let (gold_exec, gold_other) =
                    (pair.session.sides[side_exec].gold, pair.session.sides[side_other].gold);
                // El ejecutor ENTREGA sus offers y RECIBE los del partner.
                let (offers_exec, offers_other) = (
                    pair.session.sides[side_exec]
                        .items
                        .iter()
                        .map(|i| i.row.clone())
                        .collect::<Vec<_>>(),
                    pair.session.sides[side_other]
                        .items
                        .iter()
                        .map(|i| i.row.clone())
                        .collect::<Vec<_>>(),
                );
                let done = |pv: u32, delta: i64, received: Vec<ItemRow>, delivered: Vec<ItemRow>| {
                    SocialEvent::Trade(TradeEvent::Done {
                        player_vid: pv,
                        gold_delta: delta,
                        received: received
                            .into_iter()
                            .map(|row| TradeReceivedItem { row })
                            .collect(),
                        delivered,
                    })
                };
                vec![
                    done(player_vid, gold_other, offers_other.clone(), offers_exec.clone()).into(),
                    done(other, gold_exec, offers_exec, offers_other).into(),
                ]
            }
            TradeIntent::CommitFail { player_vid } => self.trade_cancel(player_vid, true),
        }
    }

    /// `ExchangeStart` (exchange.cpp:48-108): no self, target PC existente,
    /// distancia < 1000, ninguno ocupado (trade/shop) → par nuevo + GC_START
    /// a ambos. Fallos → silencio o `Already` (parity :85).
    fn trade_start(&mut self, player_vid: u32, target_vid: u32) -> Vec<NpcEvent> {
        if player_vid == target_vid {
            return Vec::new();
        }
        if self.trades.contains_key(&player_vid) {
            return Vec::new(); // self ocupado → silencio (parity :80-82)
        }
        if !self.players.contains_key(&target_vid) {
            return Vec::new(); // target no es un PC conectado
        }
        if self.trades.contains_key(&target_vid) || self.open_shops.contains_key(&target_vid) {
            // Target ocupado → GC_ALREADY al que pidió (parity :84-86).
            return vec![SocialEvent::Trade(TradeEvent::Already { player_vid }).into()];
        }
        if self.open_shops.contains_key(&player_vid) {
            return Vec::new(); // PREVENT_TRADE_WINDOW (shop abierto)
        }
        let Some((px, py)) = self.player_pos(player_vid) else { return Vec::new() };
        let Some((tx, ty)) = self.player_pos(target_vid) else { return Vec::new() };
        if distance_approx(px - tx, py - ty) >= crate::trade::EXCHANGE_MAX_DISTANCE as i32 {
            return Vec::new(); // silencioso (parity :75-78)
        }
        let pair = std::sync::Arc::new(std::sync::Mutex::new(TradePair::new(player_vid, target_vid)));
        self.trades.insert(player_vid, pair.clone());
        self.trades.insert(target_vid, pair);
        vec![
            SocialEvent::Trade(TradeEvent::Start { player_vid, other_vid: target_vid }).into(),
            SocialEvent::Trade(TradeEvent::Start { player_vid: target_vid, other_vid: player_vid })
                .into(),
        ]
    }

    /// `Accept(true)` — cuando AMBOS aceptan: plan + `Commit` al EJECUTOR
    /// (el último en aceptar — su sesión tiene el WorldStore/Batcher). El
    /// resto de cambios desaceptan (parity exchange.cpp:487-593).
    fn trade_accept(&mut self, player_vid: u32) -> Vec<NpcEvent> {
        let Some(pair) = self.trades.get(&player_vid).cloned() else { return Vec::new() };
        let mut pair = pair.lock().expect("trade pair lock");
        let side = pair.side_of(player_vid);
        let other = pair.other(player_vid);
        if !pair.session.accept(side) {
            // Primer accept: GC_ACCEPT(is_me, 1) a ambos (parity :589-590).
            let event = |is_me: bool, pv: u32| {
                SocialEvent::Trade(TradeEvent::AcceptState { player_vid: pv, is_me, accept: true })
            };
            return vec![event(true, player_vid).into(), event(false, other).into()];
        }
        // Par completado: plan + Commit al ejecutor (sin GC_ACCEPT — parity:
        // el segundo accept va directo al commit).
        let executor = player_vid;
        let partner = other;
        let (gold_exec, gold_partner) = {
            let (se, sp) = (pair.side_of(executor), pair.side_of(partner));
            (pair.session.sides[se].gold, pair.session.sides[sp].gold)
        };
        let (offers_exec, offers_partner) = {
            let (se, sp) = (pair.side_of(executor), pair.side_of(partner));
            (
                pair.session.sides[se].items.iter().map(|i| i.row.clone()).collect::<Vec<_>>(),
                pair.session.sides[sp].items.iter().map(|i| i.row.clone()).collect::<Vec<_>>(),
            )
        };
        let plan = TradeCommitPlan {
            executor,
            partner,
            gold_executor: gold_exec,
            gold_partner,
            offers_executor: offers_exec,
            offers_partner,
        };
        vec![SocialEvent::Trade(TradeEvent::Commit { player_vid: executor, plan }).into()]
    }

    /// Cancel del par (parity `CExchange::Cancel` — exchange.cpp:595-613):
    /// GC_END a ambos + libera. `from_fail` = el commit falló (mismo wire).
    fn trade_cancel(&mut self, player_vid: u32, from_fail: bool) -> Vec<NpcEvent> {
        let Some(pair) = self.trades.remove(&player_vid) else { return Vec::new() };
        let other = pair.lock().expect("trade pair lock").other(player_vid);
        self.trades.remove(&other);
        let _ = from_fail; // el wire es el mismo (GC_END); el log del canal
                           // distingue (CommitFail se loguea en el ejecutor)
        vec![
            SocialEvent::Trade(TradeEvent::Cancelled { player_vid }).into(),
            SocialEvent::Trade(TradeEvent::Cancelled { player_vid: other }).into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::test_util::*;
    use crate::shop::ShopItem;
    use database::item::ItemRow;

    fn inv_row(id: i64, pos: i32, vnum: i64, count: i64) -> ItemRow {
        ItemRow {
            id,
            window: "INVENTORY".into(),
            pos,
            count,
            vnum,
            sockets: [0; 3],
            attrs: [(0, 0); 7],
        }
    }

    /// El par: lado de cada vid + el otro.
    #[test]
    fn trade_pair_maps_sides() {
        let p = TradePair::new(1, 2);
        assert_eq!(p.side_of(1), 0);
        assert_eq!(p.side_of(2), 1);
        assert_eq!(p.other(1), 2);
        assert_eq!(p.other(2), 1);
    }

    /// Start: el par se crea (ambos vids apuntan al mismo), los eventos
    /// Start llegan a ambos; sin target → silencio; target ocupado →
    /// Already.
    #[test]
    fn trade_start_creates_pair_and_events() {
        let mut w = world_with(42);
        join_at(&mut w, 1, 0, 0);
        join_at(&mut w, 2, 200, 0); // dentro de EXCHANGE_MAX_DISTANCE (1000)
        let ev = w.process_intent(TradeIntent::Start { player_vid: 1, target_vid: 2 }.into(), 0);
        let starts: Vec<u32> = ev
            .iter()
            .filter_map(|e| match e {
                NpcEvent::Social(SocialEvent::Trade(TradeEvent::Start { player_vid, .. })) => {
                    Some(*player_vid)
                }
                _ => None,
            })
            .collect();
        assert_eq!(starts, vec![1, 2], "ambos reciben Start");
        assert!(w.trades.contains_key(&1) && w.trades.contains_key(&2));
        // Target ocupado → Already al que pidió.
        let ev = w.process_intent(TradeIntent::Start { player_vid: 3, target_vid: 1 }.into(), 0);
        assert!(
            ev.iter().any(|e| matches!(
                e,
                NpcEvent::Social(SocialEvent::Trade(TradeEvent::Already { player_vid: 3 }))
            )),
            "{ev:?}"
        );
    }

    /// Start: lejos → silencio; sin jugador → silencio; self → silencio.
    #[test]
    fn trade_start_rejects_far_and_missing() {
        let mut w = world_with(42);
        join_at(&mut w, 1, 0, 0);
        join_at(&mut w, 2, 2_000, 0); // fuera de 1000
        assert!(
            w.process_intent(TradeIntent::Start { player_vid: 1, target_vid: 2 }.into(), 0)
                .is_empty(),
            "lejos → silencio"
        );
        assert!(
            w.process_intent(TradeIntent::Start { player_vid: 1, target_vid: 99 }.into(), 0)
                .is_empty(),
            "target inexistente → silencio"
        );
        assert!(
            w.process_intent(TradeIntent::Start { player_vid: 1, target_vid: 1 }.into(), 0)
                .is_empty(),
            "self → silencio"
        );
    }

    /// ItemAdd/GoldAdd: eventos a ambos (is_me correcto); ItemDel quita;
    /// cualquier cambio desacepta (parity).
    #[test]
    fn trade_offer_events_broadcast_to_both() {
        let mut w = world_with(42);
        join_at(&mut w, 1, 0, 0);
        join_at(&mut w, 2, 100, 0);
        w.process_intent(TradeIntent::Start { player_vid: 1, target_vid: 2 }.into(), 0);
        let ev = w.process_intent(
            TradeIntent::ItemAdd { player_vid: 1, row: inv_row(10, 3, 101, 5), display_pos: 0 }
                .into(),
            0,
        );
        let adds: Vec<(u32, bool)> = ev
            .iter()
            .filter_map(|e| match e {
                NpcEvent::Social(SocialEvent::Trade(TradeEvent::ItemAdded {
                    player_vid,
                    is_me,
                    display_pos: 0,
                    ..
                })) => Some((*player_vid, *is_me)),
                _ => None,
            })
            .collect();
        assert_eq!(adds, vec![(1, true), (2, false)], "owner is_me=true, target false");
        // GoldAdd a ambos.
        let ev = w.process_intent(TradeIntent::GoldAdd { player_vid: 2, gold: 500 }.into(), 0);
        let golds: Vec<(u32, bool, i64)> = ev
            .iter()
            .filter_map(|e| match e {
                NpcEvent::Social(SocialEvent::Trade(TradeEvent::GoldAdded {
                    player_vid,
                    is_me,
                    gold,
                })) => Some((*player_vid, *is_me, *gold)),
                _ => None,
            })
            .collect();
        assert_eq!(golds, vec![(2, true, 500), (1, false, 500)]);
        // ItemDel.
        let ev = w.process_intent(TradeIntent::ItemDel { player_vid: 1, display_pos: 0 }.into(), 0);
        assert_eq!(ev.len(), 2, "ItemRemoved a ambos");
        // El estado puro: la sesión del mundo quedó consistente.
        let pair = w.trades.get(&1).expect("par").lock().expect("lock");
        assert!(pair.session.sides[0].items.is_empty());
        assert_eq!(pair.session.sides[1].gold, 500);
    }

    /// Accept: el primer accept emite AcceptState a ambos; el segundo —
    /// Commit al ejecutor con el plan (gold + offers de ambos).
    #[test]
    fn trade_both_accepts_emit_commit_plan() {
        let mut w = world_with(42);
        join_at(&mut w, 1, 0, 0);
        join_at(&mut w, 2, 100, 0);
        w.process_intent(TradeIntent::Start { player_vid: 1, target_vid: 2 }.into(), 0);
        w.process_intent(
            TradeIntent::ItemAdd { player_vid: 1, row: inv_row(10, 3, 101, 5), display_pos: 0 }
                .into(),
            0,
        );
        w.process_intent(TradeIntent::GoldAdd { player_vid: 2, gold: 500 }.into(), 0);
        // Primer accept: AcceptState a ambos.
        let ev = w.process_intent(TradeIntent::Accept { player_vid: 1 }.into(), 0);
        assert_eq!(ev.len(), 2, "AcceptState a ambos: {ev:?}");
        assert!(ev.iter().all(|e| matches!(
            e,
            NpcEvent::Social(SocialEvent::Trade(TradeEvent::AcceptState { accept: true, .. }))
        )));
        // Segundo accept: Commit al ejecutor (el 2) con el plan.
        let ev = w.process_intent(TradeIntent::Accept { player_vid: 2 }.into(), 0);
        let plan = ev.iter().find_map(|e| match e {
            NpcEvent::Social(SocialEvent::Trade(TradeEvent::Commit { player_vid, plan })) => {
                Some((*player_vid, plan.clone()))
            }
            _ => None,
        });
        let (executor, plan) = plan.expect("Commit");
        assert_eq!(executor, 2, "el último en aceptar ejecuta");
        assert_eq!((plan.executor, plan.partner), (2, 1));
        // El ejecutor (2) ofreció 500; el partner (1) ofreció el item 101.
        assert_eq!(plan.gold_executor, 500);
        assert_eq!(plan.gold_partner, 0);
        assert_eq!(plan.offers_executor.len(), 0);
        assert_eq!(plan.offers_partner.len(), 1);
        // La sesión del mundo conserva el estado (el par sigue vivo hasta el
        // CommitOk/Fail).
        let pair = w.trades.get(&1).expect("par").lock().expect("lock");
        assert_eq!(pair.session.sides[0].items.len(), 1, "el 1 ofreció el item");
        assert_eq!(pair.session.sides[1].gold, 500, "el 2 ofreció el oro");
        assert!(pair.session.both_accepted());
    }

    /// CommitOk: Done a AMBOS con gold_delta + received/delivered cruzados
    /// (el ejecutor recibe las ofertas del partner y viceversa) + par libre.
    #[test]
    fn trade_commit_ok_emits_done_both() {
        let mut w = world_with(42);
        join_at(&mut w, 1, 0, 0);
        join_at(&mut w, 2, 100, 0);
        w.process_intent(TradeIntent::Start { player_vid: 1, target_vid: 2 }.into(), 0);
        w.process_intent(
            TradeIntent::ItemAdd { player_vid: 1, row: inv_row(10, 3, 101, 5), display_pos: 0 }
                .into(),
            0,
        );
        w.process_intent(TradeIntent::GoldAdd { player_vid: 2, gold: 500 }.into(), 0);
        w.process_intent(TradeIntent::Accept { player_vid: 1 }.into(), 0);
        w.process_intent(TradeIntent::Accept { player_vid: 2 }.into(), 0);
        let ev = w.process_intent(TradeIntent::CommitOk { player_vid: 2 }.into(), 0);
        let dones: Vec<(u32, i64, usize, usize)> = ev
            .iter()
            .filter_map(|e| match e {
                NpcEvent::Social(SocialEvent::Trade(TradeEvent::Done {
                    player_vid,
                    gold_delta,
                    received,
                    delivered,
                })) => Some((*player_vid, *gold_delta, received.len(), delivered.len())),
                _ => None,
            })
            .collect();
        // El ejecutor (2) ofreció 500 de oro: recibe el item 101 del 1 y no
        // entrega items. El 1 ofreció el item: recibe 500 y entrega el item.
        assert_eq!(dones, vec![(2, 0, 1, 0), (1, 500, 0, 1)]);
        assert!(!w.trades.contains_key(&1) && !w.trades.contains_key(&2), "par liberado");
    }

    /// CommitFail: Cancelled a ambos + par liberado.
    #[test]
    fn trade_commit_fail_cancels_both() {
        let mut w = world_with(42);
        join_at(&mut w, 1, 0, 0);
        join_at(&mut w, 2, 100, 0);
        w.process_intent(TradeIntent::Start { player_vid: 1, target_vid: 2 }.into(), 0);
        w.process_intent(TradeIntent::Accept { player_vid: 1 }.into(), 0);
        w.process_intent(TradeIntent::Accept { player_vid: 2 }.into(), 0);
        let ev = w.process_intent(TradeIntent::CommitFail { player_vid: 2 }.into(), 0);
        assert_eq!(ev.len(), 2, "Cancelled a ambos");
        assert!(!w.trades.contains_key(&1) && !w.trades.contains_key(&2));
    }

    /// Cancel en cualquier momento: Cancelled a ambos.
    #[test]
    fn trade_cancel_any_time() {
        let mut w = world_with(42);
        join_at(&mut w, 1, 0, 0);
        join_at(&mut w, 2, 100, 0);
        w.process_intent(TradeIntent::Start { player_vid: 1, target_vid: 2 }.into(), 0);
        let ev = w.process_intent(TradeIntent::Cancel { player_vid: 1 }.into(), 0);
        assert_eq!(ev.len(), 2, "Cancelled a ambos");
        assert!(w.trades.is_empty());
    }

    /// Shop: Open resuelve el shop por npc_vid + distancia; Buy da el
    /// precio; Close cierra. (El NPC se materializa con `load_table`.)
    #[test]
    fn shop_open_buy_close_flow() {
        let mut w = world_with(42);
        // El NPC 9001 (npc_vnum del shop 1) materializado en (0,0).
        w.load_table(41, vec![(entry(9001, 0, 0, 1), mob_row(9001))]);
        w.world.resource_mut::<ShopTable>().0.insert(
            9001,
            Shop {
                vnum: 1,
                npc_vnum: 9001,
                items: vec![ShopItem { vnum: 20, count: 1, price: 400, display_pos: 0 }],
            },
        );
        join_at(&mut w, 1, 0, 0);
        // Open: el shop del NPC.
        let ev = w.process_intent(ShopIntent::Open { player_vid: 1, npc_vid: 10_000 }.into(), 0);
        let opened = ev.iter().find_map(|e| match e {
            NpcEvent::Social(SocialEvent::Shop(ShopEvent::Opened { items, .. })) => Some(items.clone()),
            _ => None,
        });
        let items = opened.expect("Opened");
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].vnum, items[0].price), (20, 400));
        // Buy: precio resuelto.
        let ev = w.process_intent(ShopIntent::Buy { player_vid: 1, pos: 0 }.into(), 0);
        let buy = ev.iter().find_map(|e| match e {
            NpcEvent::Social(SocialEvent::Shop(ShopEvent::BuyResult { price, vnum, .. })) => {
                Some((*vnum, *price))
            }
            _ => None,
        });
        assert_eq!(buy, Some((20, 400)));
        // Buy pos inválido → BuyRejected.
        let ev = w.process_intent(ShopIntent::Buy { player_vid: 1, pos: 9 }.into(), 0);
        assert!(
            ev.iter().any(|e| matches!(
                e,
                NpcEvent::Social(SocialEvent::Shop(ShopEvent::BuyRejected {
                    error: ShopError::InvalidPos,
                    ..
                }))
            )),
            "{ev:?}"
        );
        // Close.
        let ev = w.process_intent(ShopIntent::Close { player_vid: 1 }.into(), 0);
        assert!(ev.iter().any(|e| matches!(
            e,
            NpcEvent::Social(SocialEvent::Shop(ShopEvent::Closed { .. }))
        )));
        // Sin shop abierto: Buy → silencio (parity).
        assert!(
            w.process_intent(ShopIntent::Buy { player_vid: 1, pos: 0 }.into(), 0).is_empty()
        );
    }

    /// Shop: NPC lejos → Open silencioso; sell sin shop → silencio.
    #[test]
    fn shop_open_far_is_silent() {
        let mut w = world_with(42);
        w.load_table(41, vec![(entry(9001, 3_000, 0, 1), mob_row(9001))]);
        w.world.resource_mut::<ShopTable>().0.insert(
            9001,
            Shop { vnum: 1, npc_vnum: 9001, items: Vec::new() },
        );
        w.world.resource_mut::<ShopTable>().0.insert(
            9001,
            Shop { vnum: 1, npc_vnum: 9001, items: Vec::new() },
        );
        join_at(&mut w, 1, 0, 0);
        assert!(
            w.process_intent(ShopIntent::Open { player_vid: 1, npc_vid: 10_000 }.into(), 0)
                .is_empty(),
            "a 3000 (>= SHOP_MAX_DISTANCE 1000) → silencio"
        );
        assert!(
            w.process_intent(ShopIntent::Sell { player_vid: 1, cell: 3 }.into(), 0).is_empty(),
            "sell sin shop abierto → silencio"
        );
    }
}

