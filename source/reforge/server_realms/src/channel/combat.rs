//! `channel/combat.rs` — el handler del CG_ATTACK (R-s3): la resolución del
//! ataque EN EL MUNDO COMPARTIDO (cooldown/rango/daño — server-authoritative,
//! parity `game_core::combat::handle_attack` puro). La conexión hace la query del
//! arma (PG) y manda el intent; el resultado llega por su cola
//! (`AttackResult` → paquetes del golpe + flujo de kill/recompensa/drop).
//!
//! C6a (firma uniforme): malformado/skills pendientes → log + Continue.

use database::item::ItemRepo;
use game_core::ecs::{CombatIntent, Intent};

use crate::channel::INVENTORY_MAX_NUM;
use crate::channel::session::{Outcome, Session};

/// CG_ATTACK (8): valida el parseo y manda el intent `Attack` al mundo (la
/// resolución — cooldown/rango/daño — la hace `WorldSim::process_attack`).
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let attack = match protocol::combat::CgAttack::from_bytes(pkt) {
        Ok(a) => a,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_ATTACK malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    if attack.b_type != protocol::combat::CgAttack::TYPE_NORMAL {
        // Skills (bType > 0) — F5.2+ (el combat lane las rechaza con empty()).
        eprintln!(
            "server_realms: channel conn {}: CG_ATTACK tipo {} de {} — \
             skills pendientes (F5.2+)",
            session.conn_id,
            attack.b_type,
            session.row().name
        );
        return Ok(Outcome::Continue);
    }
    // F5.3 (items): el ARMA equipada (WEAR_WEAPON = 4 → cell =
    // INVENTORY_MAX_NUM + 4 = 184) — su ProtoItem (value3/4 daño, value5
    // bonus) y el attack_speed del arma (GET_ATTACK_SPEED).
    let weapon = if let Some(w) = session
        .inventory
        .iter()
        .find(|i| i.window == "EQUIPMENT" && i.pos as u16 == INVENTORY_MAX_NUM + 4)
    {
        ItemRepo::new(session.pool.clone())
            .load_proto_use_values(w.vnum)
            .await?
    } else {
        None
    };
    session.intent(Intent::Combat(CombatIntent::Attack {
        player_vid: session.player_vid(),
        victim_vid: attack.victim_vid,
        b_type: attack.b_type,
        weapon,
    }))?;
    Ok(Outcome::Continue)
}
