//! `channel/social.rs` — emisión S→C del dominio SOCIAL (F6, 2026-08-13):
//! los eventos VALIDADOS del mundo (`ShopEvent`/`TradeEvent`) se traducen al
//! wire (GC_SHOP 38 / GC_EXCHANGE 42) y aplican la parte PG (oro + items —
//! unidad ACID del Batcher) en `shop.rs`/`trade.rs`.
//!
//! N1: el stub `match s {}` ahora tiene brazos reales — una variante social
//! nueva SIN handler es un error de compilación aquí.
//!
//! NOTA (async): `Session::send` es async (conn_send + bench_capture) — el
//! emit es `async fn`; el arm del dispatch (`channel/events.rs:445`) añadió
//! el `.await` (cambio de UNA línea en el arm Social — el arm Quest queda
//! con su stub sync intacto).

use game_core::ecs::SocialEvent;

use crate::channel::session::Session;
use crate::channel::{shop, trade};

/// Delegado S→C del lane social: shop y trade delegan en sus módulos (que
/// tienen la lógica PG del wire).
pub(super) async fn emit(session: &mut Session, s: SocialEvent) -> Result<(), String> {
    match s {
        SocialEvent::Shop(e) => shop::emit(session, e).await,
        SocialEvent::Trade(e) => trade::emit(session, e).await,
    }
}
