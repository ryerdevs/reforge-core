//! `channel/skills.rs` — el handler del CG_USE_SKILL (R-s3): el filtro
//! temprano de nivel + la query del arma (la var `atk` del poly la usa) + el
//! intent `UseSkill` al mundo (la resolución — nivel/cooldown/SP/rango/
//! efecto — vive EN EL MUNDO, `WorldSim::process_skill`).
//!
//! CG_USE_SKILL (52, 9 B — Packet.h:854: `bHeader + dwVnum + dwTargetVID`).
//! El cliente muestra su cooldown; el SERVIDOR lo impone (ADR-0011 — el
//! legacy sin ENABLE_SKILL_COOLDOWN_CHECK solo muestra).
//!
//! C6a (firma uniforme): malformado/sin nivel → log + Continue (antes el
//! malformado cerraba la conexión).

use database::item::ItemRepo;
use game_core::ecs::{Intent, SkillIntent};

use crate::channel::session::{Outcome, Session};
use crate::channel::INVENTORY_MAX_NUM;

/// CG_USE_SKILL (52): filtro temprano de nivel (parity UseSkill:
/// `GetSkillLevel == 0` → rechazo sin respuesta — el mundo re-valida) + el
/// intent `UseSkill` con el arma equipada.
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() < 9 {
        // C6a: malformado → Continue con log (antes cerraba la conexión).
        eprintln!(
            "server_realms: channel conn {}: CG_USE_SKILL malformado ({})",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    }
    let skill_id = u32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]);
    let target_vid = u32::from_le_bytes([pkt[5], pkt[6], pkt[7], pkt[8]]);
    // Filtro temprano (parity UseSkill: GetSkillLevel == 0 → rechazo sin
    // respuesta) — el mundo re-valida.
    if game_core::skill::skill_level_from_blob(
        session.row().skill_level.as_deref().unwrap_or(&[]),
        skill_id,
    ) == 0
    {
        eprintln!(
            "server_realms: channel conn {}: skill {skill_id} de {} \
             sin nivel — ignorado",
            session.conn_id, session.row().name
        );
        return Ok(Outcome::Continue);
    }
    // F5.3 (items): el ARMA equipada — el `atk` del poly del skill usa el
    // melee del jugador (battle.cpp).
    let weapon = if let Some(w) = session.inventory.iter().find(|i| {
        i.window == "EQUIPMENT"
            && i.pos as u16 == INVENTORY_MAX_NUM + 4
    }) {
        ItemRepo::new(&session.config.pg_conn)
            .load_proto_use_values(w.vnum)
            .await?
    } else {
        None
    };
    session.intent(Intent::Skill(SkillIntent::UseSkill {
        player_vid: session.player_vid(),
        skill_id,
        target_vid,
        weapon,
    }))?;
    Ok(Outcome::Continue)
}
