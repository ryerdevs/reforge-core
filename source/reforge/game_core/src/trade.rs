//! Intercambio jugador↔jugador (F6 social): la MÁQUINA DE ESTADO PURA del
//! trade (parity `exchange.cpp`) + el constructor de la UNIDAD ACID del
//! commit (ADR-0011 "items as ACID units" — `ItemExchange::exchange_mutated`
//! + `Batcher::flush`; nunca commits por item).
//!
//! # Flujo (parity exchange.cpp)
//!
//! `start` (ExchangeStart, :48-108) → `add_item` (:138-213) / `add_gold`
//! (:240-262) — CUALQUIER cambio resetea ambos accepts (parity `Accept(false)`
//! a los dos, :176-177/231-232/254-255) → `accept` (:487-593): cuando AMBOS
//! aceptan, el mundo construye el `TradeCommitPlan` y el commit corre en el
//! canal del ejecutor (solo él tiene el WorldStore/Batcher).
//!
//! # Dupe-critical (el commit)
//!
//! Los items RECIBIDOS se re-crean con id NUEVO (rango 100M-200M — patrón
//! del split del canal, `channel/items.rs:587-591`), NO se mueven por id:
//! los guards de materiales (`UPDATE ... WHERE id = $1 AND count = $pre`) son
//! agnósticos del owner — re-consumirían un item ya movido si el consume y el
//! produce compartieran id. Con ids nuevos: cada unidad es idempotente (el
//! consume con guard es no-op tras el primer commit; el insert por id es
//! `ON CONFLICT DO UPDATE`) y el conjunto es crash-consistente bajo
//! single-writer-per-region (ADR-0011) + WAL replay (ADR-0008).
//!
//! DESVIACIÓN documentada: el par completa en VARIAS unidades ACID (una por
//! lado: materiales+oro; una por item recibido: insert) — cada una es UNA
//! transacción con audit (la unidad documentada de `ItemExchange`), no el par
//! completo. La alternativa (una sola tx para el par) exige cambios en
//! `database` (fuera del scope del lane social). El oro se valida
//! (`gold_post >= 0` — parity `CExchange::Check`, :283-311) y el `CHECK
//! gold >= 0` de PG (alter_gold_check.sql) es el backstop.

use database::item::{ItemExchange, ItemRow};

/// `EXCHANGE_ITEM_MAX_NUM` (exchange.h:8) — items por lado en la ventana.
pub const EXCHANGE_ITEM_MAX_NUM: usize = 12;
/// `EXCHANGE_MAX_DISTANCE` (exchange.h:9) — rango del start.
pub const EXCHANGE_MAX_DISTANCE: i64 = 1000;
/// Rango de ids nuevos para los items recibidos (patrón del canal —
/// `ItemIDRangeManager.cpp:93,121`).
pub const ITEM_ID_RANGE_MIN: i64 = 100_000_000;
pub const ITEM_ID_RANGE_MAX: i64 = 200_000_000;

/// Rechazos del estado puro (los del canal — oro/distance — se validan
/// ANTES de mandar el intent, parity `input_main.cpp:1217-1231`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeError {
    /// Límite de 12 items por lado (parity `EXCHANGE_ITEM_MAX_NUM`).
    ItemLimit,
    /// La posición de la ventana ya está ocupada (parity grid 4x3).
    DisplayPosTaken,
    /// El item ya está ofrecido (parity `IsExchanging`).
    AlreadyOffered,
    /// El oro ya está ofrecido (parity `m_lGold > 0` — exchange.cpp:251).
    GoldAlreadyOffered,
    /// Oro inválido (<= 0 — parity exchange.cpp:242).
    InvalidGold,
}

/// Un item ofrecido: la fila COMPLETA (el channel la mandó en el intent —
/// el commit necesita id/count/vnum/sockets/attrs para re-crear la fila del
/// receptor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeOfferItem {
    pub row: ItemRow,
    pub display_pos: u8,
}

/// Un lado del trade (los dos lados viven en el `TradeSession` del mundo).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TradeSide {
    pub items: Vec<TradeOfferItem>,
    pub gold: i64,
    pub accept: bool,
}

/// La sesión del par (parity `CExchange` + `GetCompany`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TradeSession {
    pub sides: [TradeSide; 2],
}

impl TradeSession {
    /// `CExchange::AddItem` (:138-213): tope 12, display_pos único, item no
    /// duplicado; resetea los accepts (parity :176-177).
    pub fn add_item(
        &mut self,
        side: usize,
        row: ItemRow,
        display_pos: u8,
    ) -> Result<(), TradeError> {
        let s = &mut self.sides[side];
        if s.items.len() >= EXCHANGE_ITEM_MAX_NUM {
            return Err(TradeError::ItemLimit);
        }
        if s.items.iter().any(|i| i.display_pos == display_pos) {
            return Err(TradeError::DisplayPosTaken);
        }
        if s.items.iter().any(|i| i.row.id == row.id) {
            return Err(TradeError::AlreadyOffered);
        }
        s.items.push(TradeOfferItem { row, display_pos });
        self.reset_accepts();
        Ok(())
    }

    /// `CExchange::RemoveItem` (:215-238): quita por display_pos.
    pub fn remove_item(&mut self, side: usize, display_pos: u8) -> bool {
        let s = &mut self.sides[side];
        let before = s.items.len();
        s.items.retain(|i| i.display_pos != display_pos);
        if s.items.len() != before {
            self.reset_accepts();
            return true;
        }
        false
    }

    /// `CExchange::AddGold` (:240-262): oro > 0, una sola vez.
    pub fn add_gold(&mut self, side: usize, gold: i64) -> Result<(), TradeError> {
        if gold <= 0 {
            return Err(TradeError::InvalidGold);
        }
        let s = &mut self.sides[side];
        if s.gold > 0 {
            return Err(TradeError::GoldAlreadyOffered);
        }
        s.gold = gold;
        self.reset_accepts();
        Ok(())
    }

    /// `CExchange::Accept(true)` — devuelve true cuando el PAR completó.
    pub fn accept(&mut self, side: usize) -> bool {
        self.sides[side].accept = true;
        self.both_accepted()
    }

    pub fn both_accepted(&self) -> bool {
        self.sides[0].accept && self.sides[1].accept
    }

    /// Parity `Accept(false)` + `GetCompany()->Accept(false)` — cualquier
    /// cambio de oferta desacepta a ambos.
    pub fn reset_accepts(&mut self) {
        self.sides[0].accept = false;
        self.sides[1].accept = false;
    }
}

/// El plan de commit que el mundo construye al completar el par: todo lo
/// que el ejecutor necesita (los offers de AMBOS lados, con las filas
/// completas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeCommitPlan {
    /// El jugador que ejecuta el commit (el último en aceptar — su sesión
    /// tiene el WorldStore/Batcher).
    pub executor: u32,
    /// El otro jugador.
    pub partner: u32,
    /// Oro ofrecido por cada lado (0 = nada).
    pub gold_executor: i64,
    pub gold_partner: i64,
    /// Items ofrecidos (filas completas) por cada lado.
    pub offers_executor: Vec<ItemRow>,
    pub offers_partner: Vec<ItemRow>,
}

/// Las unidades ACID del commit (parity `CExchange::Done` — exchange.cpp:
/// 410-485):
/// 1. unidad del ejecutor: materiales (sus offers → DELETE/0) + oro;
/// 2. unidad del partner: materiales (sus offers) + oro;
/// 3. una unidad por item RECIBIDO: insert con id NUEVO (el row del
///    receptor — owner nuevo, window INVENTORY, pos = el del oferente; el
///    receptor lo re-coloca en memoria y re-upsertea — idempotente por id).
///
/// `gold_now_*` = el oro ACTUAL de cada lado (el ejecutor lee el del partner
/// fresco de PG antes de construir — parity `CExchange::Check`). Unidades
/// vacías (sin materiales/result/oro) se omiten.
pub fn build_commit_units(
    plan: &TradeCommitPlan,
    gold_now_executor: i64,
    gold_now_partner: i64,
    next_id: &mut impl FnMut() -> i64,
) -> Vec<ItemExchange> {
    let mut units = Vec::with_capacity(plan.offers_executor.len() + plan.offers_partner.len() + 2);
    let exec_post = gold_now_executor - plan.gold_executor;
    let partner_post = gold_now_partner - plan.gold_partner;

    // Unidad del ejecutor: consume sus offers + su oro.
    if !plan.offers_executor.is_empty() || plan.gold_executor > 0 {
        units.push(ItemExchange {
            owner_id: i64::from(plan.executor),
            materials: plan
                .offers_executor
                .iter()
                .map(|r| (r.id, r.count, 0))
                .collect(),
            result: None,
            gold: Some((gold_now_executor, exec_post)),
        });
    }
    // Unidad del partner: consume sus offers + su oro.
    if !plan.offers_partner.is_empty() || plan.gold_partner > 0 {
        units.push(ItemExchange {
            owner_id: i64::from(plan.partner),
            materials: plan
                .offers_partner
                .iter()
                .map(|r| (r.id, r.count, 0))
                .collect(),
            result: None,
            gold: Some((gold_now_partner, partner_post)),
        });
    }
    // Items recibidos: el partner recibe los offers del ejecutor (ids nuevos).
    for r in &plan.offers_executor {
        units.push(ItemExchange {
            owner_id: i64::from(plan.partner),
            materials: Vec::new(),
            result: Some((received_row(r, next_id()), i64::from(plan.partner))),
            gold: None,
        });
    }
    // El ejecutor recibe los offers del partner.
    for r in &plan.offers_partner {
        units.push(ItemExchange {
            owner_id: i64::from(plan.executor),
            materials: Vec::new(),
            result: Some((received_row(r, next_id()), i64::from(plan.executor))),
            gold: None,
        });
    }
    units
}

/// El row del item recibido: id NUEVO, INVENTORY, pos = el del oferente
/// (el receptor lo re-coloca en memoria y re-upsertea).
fn received_row(offer: &ItemRow, id: i64) -> ItemRow {
    ItemRow {
        id,
        window: "INVENTORY".to_string(),
        pos: offer.pos,
        count: offer.count,
        vnum: offer.vnum,
        sockets: offer.sockets,
        attrs: offer.attrs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, vnum: i64, count: i64, pos: i32) -> ItemRow {
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

    fn session_pair() -> (TradeSession, u32, u32) {
        // vids 1 y 2 — el 1 es el ejecutor en los tests del commit.
        (TradeSession::default(), 1, 2)
    }

    /// add_item: cap 12, display_pos único, item no duplicado; resetea los
    /// accepts (parity exchange.cpp:176-177).
    #[test]
    fn add_item_enforces_caps_and_resets_accepts() {
        let (mut t, _, _) = session_pair();
        t.accept(0);
        t.accept(1);
        assert!(t.both_accepted());
        t.add_item(0, row(10, 101, 5, 3), 0).expect("add");
        assert!(!t.both_accepted(), "cualquier cambio desacepta (parity)");
        assert!(t.add_item(0, row(11, 101, 1, 4), 0).is_err(), "display_pos duplicado");
        assert!(t.add_item(0, row(10, 101, 1, 4), 1).is_err(), "item ya ofrecido");
        for i in 1..EXCHANGE_ITEM_MAX_NUM {
            t.add_item(0, row(100 + i as i64, 200 + i as i64, 1, i as i32), i as u8)
                .expect("add");
        }
        assert_eq!(t.sides[0].items.len(), EXCHANGE_ITEM_MAX_NUM);
        assert!(
            t.add_item(0, row(999, 999, 1, 99), 99).is_err(),
            "cap 12 (EXCHANGE_ITEM_MAX_NUM)"
        );
    }

    /// remove_item + add_gold: una sola vez, oro > 0.
    #[test]
    fn remove_and_gold_rules() {
        let (mut t, _, _) = session_pair();
        t.add_item(0, row(10, 101, 5, 3), 2).expect("add");
        assert!(t.remove_item(0, 2), "quita por display_pos");
        assert!(!t.remove_item(0, 2), "ya no está");
        assert!(t.add_gold(0, 1000).is_ok());
        assert_eq!(t.sides[0].gold, 1000);
        assert!(t.add_gold(0, 500).is_err(), "una sola vez (parity m_lGold > 0)");
        assert!(t.add_gold(1, 0).is_err(), "oro <= 0 rechazado");
    }

    /// accept: ambos → par completado; cambios posteriores desaceptan.
    #[test]
    fn both_accepts_complete_pair() {
        let (mut t, _, _) = session_pair();
        assert!(!t.accept(0), "solo un lado");
        assert!(t.accept(1), "ambos aceptaron → completado");
        // Un cambio después de completar resetea (aunque el par ya fue
        // "completado" — el mundo decide con el plan).
        t.reset_accepts();
        assert!(!t.both_accepted());
    }

    /// El commit: unidades por lado (materiales + oro) + una por item
    /// recibido, con ids NUEVOS y gold posts validados.
    #[test]
    fn build_commit_units_shape_and_ids() {
        let (_, exec, partner) = session_pair();
        let plan = TradeCommitPlan {
            executor: exec,
            partner,
            gold_executor: 1000,
            gold_partner: 500,
            offers_executor: vec![row(10, 101, 5, 3), row(11, 102, 1, 4)],
            offers_partner: vec![row(20, 201, 3, 7)],
        };
        let mut next = (100_000_000i64..).into_iter();
        let units = build_commit_units(&plan, 5_000, 3_000, &mut || {
            let id = next.next().expect("ids");
            id
        });
        // 2 unidades de lado (materiales+oro) + 3 recibidos = 5.
        assert_eq!(units.len(), 5);
        // Unidad del ejecutor: consume sus 2 offers + oro 5000→4000.
        assert_eq!(units[0].owner_id, 1);
        assert_eq!(units[0].materials, vec![(10, 5, 0), (11, 1, 0)]);
        assert_eq!(units[0].gold, Some((5_000, 4_000)));
        assert_eq!(units[0].result, None);
        // Unidad del partner: 1 offer + oro 3000→2500.
        assert_eq!(units[1].owner_id, 2);
        assert_eq!(units[1].materials, vec![(20, 3, 0)]);
        assert_eq!(units[1].gold, Some((3_000, 2_500)));
        // Recibidos: el partner recibe los 2 del ejecutor, el ejecutor 1 del
        // partner — ids nuevos 100000000+.
        assert_eq!(units[2].result.as_ref().map(|(r, o)| (r.id, *o)), Some((100_000_000, 2)));
        assert_eq!(units[3].result.as_ref().map(|(r, o)| (r.id, *o)), Some((100_000_001, 2)));
        assert_eq!(units[4].result.as_ref().map(|(r, o)| (r.id, *o)), Some((100_000_002, 1)));
        // El row recibido conserva vnum/count/sockets/attrs, pos = la del
        // oferente, window INVENTORY.
        let (r, _) = units[4].result.as_ref().expect("result");
        assert_eq!((r.vnum, r.count, r.pos, r.window.as_str()), (201, 3, 7, "INVENTORY"));
    }

    /// Unidades vacías se omiten (trade sin oro ni items → sin unidades).
    #[test]
    fn empty_trade_builds_no_units() {
        let (_, exec, partner) = session_pair();
        let plan = TradeCommitPlan {
            executor: exec,
            partner,
            gold_executor: 0,
            gold_partner: 0,
            offers_executor: Vec::new(),
            offers_partner: Vec::new(),
        };
        assert!(build_commit_units(&plan, 100, 100, &mut || 1).is_empty());
    }

    /// gold_post negativo → la unidad se construye con post < 0 (el CALLER
    /// lo valida antes — parity CExchange::Check; el guard de PG lo rechaza).
    #[test]
    fn commit_units_gold_post_must_be_validated_by_caller() {
        let (_, exec, partner) = session_pair();
        let plan = TradeCommitPlan {
            executor: exec,
            partner,
            gold_executor: 5_000,
            gold_partner: 0,
            offers_executor: Vec::new(),
            offers_partner: Vec::new(),
        };
        let units = build_commit_units(&plan, 4_000, 100, &mut || 1);
        assert_eq!(
            units[0].gold,
            Some((4_000, -1_000)),
            "post negativo — el ejecutor debe rechazar ANTES (gold_now < oferta)"
        );
    }
}
