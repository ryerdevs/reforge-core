//! `channel/gm.rs` — los comandos de GM por el chat (F4 slice, 2026-08-13).
//!
//! # Parity legacy (verificado)
//!
//! - Entrada: `CG_CHAT` con '/' al inicio → `interpret_command(ch, buf+1)`
//!   (input_main.cpp:661-665) — el comando NO se muestra como chat y salta
//!   el anti-spam. El hook vive en `chat.rs` (una línea: el mensaje que
//!   empieza con '/' delega aquí).
//! - Permisos: `gm_get_level` (gm.cpp:50-105) — el nombre del PERSONAJE es
//!   la clave del `common.gmlist` + la cuenta DEBE coincidir. El Rust
//!   re-chequea en la DB por CADA comando (ADR-0011: "permissions re-checked
//!   in DB" — el C++ cachea el map en memoria; la query por llamada es el
//!   patrón del crate y el volumen de comandos GM es trivial).
//! - Gate: `cmd_info[].gm_level > GetGMLevel()` → rechazo (cmd.cpp:710).
//!   Tanto el comando desconocido como el rechazado responden con el mismo
//!   INFO ("그런 명령어가 없습니다") — el Rust manda un mensaje EN (sin
//!   sistema de locale aún — divergencia documentada).
//!
//! # Subset implementado (ponytail)
//!
//! `warp x y` (GC_WARP — el cliente reconecta), `item vnum [count]`
//! (ItemRepo + GC_ITEM_SET), `notice texto` (GC_CHAT NOTICE — al GM; el
//! broadcast a TODOS los jugadores es GAP: necesita el task del canal con
//! las `routes`), `level n` (row + GC_POINTS + mundo + save).
//!
//! # Deferred (GAPs documentados)
//!
//! - `mob <vnum>`: spawn de un mob en el punto del GM — necesita un intent
//!   nuevo del mundo (crear entidad + Spawned). El "existing spawn path"
//!   materializa la tabla de spawns, no mobs arbitrarios.
//! - `kill <jugador>`: necesita el registro nombre→vid (el mundo solo tiene
//!   vids; la conexión solo su propio nombre).
//! - `warp <jugador>` (warp a OTRO) y `set` (stats): igual — registro de
//!   nombres / recálculo de puntos.
//! - Comandos GM_PLAYER (nivel 0: `/logout`, `/restart_here`, `/who`...):
//!   el cliente maneja logout/restart localmente; fuera del subset.

use database::common::CommonRepo;
use database::item::ItemRepo;
use game_core::ecs::{CombatIntent, Intent};
use game_core::gm::{self, GmCommand};
use game_core::packets;
use protocol::world::{TPacketGCItemSet, TItemPos};

use crate::channel::session::{Outcome, Session};
use crate::channel::{parse_listen, INVENTORY_MAX_NUM};

/// CHAT_TYPE_INFO = 1, CHAT_TYPE_NOTICE = 2 (length.h:514-525).
const CHAT_TYPE_INFO: u8 = 1;
const CHAT_TYPE_NOTICE: u8 = 2;

/// Texto del chat al GM (GC_CHAT type INFO — parity del ChatPacket del C++;
/// sin locale system → EN, divergencia documentada).
async fn gm_info(session: &mut Session, text: &str) -> Result<(), String> {
    let size = (9 + text.len()) as u16;
    let mut out = Vec::with_capacity(9 + text.len());
    out.push(protocol::header::GC_CHAT);
    out.extend_from_slice(&size.to_le_bytes());
    out.push(CHAT_TYPE_INFO);
    out.extend_from_slice(&session.player_vid().to_le_bytes());
    out.push(session.empire);
    out.extend_from_slice(text.as_bytes());
    session
        .send(&out)
        .await
        .map_err(|e| format!("enviando GC_CHAT (GM info): {e}"))
}

/// Un comando '/' del chat: parsea → permisos (gmlist en DB) → dispatch.
/// `cmd` es el texto SIN la '/' (el hook de chat.rs la quitó).
pub async fn handle(session: &mut Session, cmd: &str) -> Result<Outcome, String> {
    let Some(command) = gm::parse_command(cmd) else {
        // Parity cmd.cpp:704-708: comando desconocido → INFO.
        eprintln!(
            "server_realms: channel conn {}: comando '/{}' desconocido ({} — GM {})",
            session.conn_id,
            cmd.trim(),
            session.row().name,
            session.account_login
        );
        gm_info(session, "No such command").await?;
        return Ok(Outcome::Continue);
    };
    // GM_PLAYER (nivel 0): comandos de jugador — SIN gmlist (parity
    // cmd.cpp:340-347; accesibles a todos). El C++ los despacha directo en
    // do_restart/do_cmd; el revive solo aplica muerto (POS_DEAD) y los de
    // cierre mandan su paquete y cierran la conexión.
    if matches!(
        command,
        GmCommand::RestartHere | GmCommand::RestartTown | GmCommand::Logout
            | GmCommand::Quit | GmCommand::PhaseSelect
    ) {
        return handle_player_command(session, command).await;
    }
    // Permisos RE-CHECK en DB por comando (ADR-0011): `common.gmlist` —
    // (mName = personaje, mAccount = cuenta). Sin fila → no es GM.
    let Some(auth) = CommonRepo::new(session.pool.clone())
        .gm_authority(&session.row().name, &session.account_login)
        .await?
    else {
        eprintln!(
            "server_realms: channel conn {}: '/{}' de {} — no está en \
             common.gmlist (rechazado)",
            session.conn_id, cmd.trim(), session.row().name
        );
        gm_info(session, "No such command").await?;
        return Ok(Outcome::Continue);
    };
    let level = gm::gm_level_from_text(&auth).unwrap_or(gm::gm_level::PLAYER);
    if !gm::is_allowed(level, gm::required_level(&command)) {
        // Parity cmd.cpp:710-714: nivel insuficiente → INFO (mismo mensaje).
        eprintln!(
            "server_realms: channel conn {}: '/{}' de {} — nivel GM {level} \
             insuficiente para {:?} (requiere {})",
            session.conn_id,
            cmd.trim(),
            session.row().name,
            command,
            gm::required_level(&command)
        );
        gm_info(session, "No such command").await?;
        return Ok(Outcome::Continue);
    }
    eprintln!(
        "server_realms: channel conn {}: GM {} ({} — nivel {level}): /{}",
        session.conn_id, session.row().name, session.account_login, cmd.trim()
    );
    match command {
        GmCommand::Warp { x, y } => warp(session, x, y).await?,
        GmCommand::GiveItem { vnum, count } => give_item(session, vnum, count).await?,
        GmCommand::Notice { text } => self_notice(session, &text).await?,
        GmCommand::SetLevel { level } => set_level(session, level).await?,
        // Inalcanzable: las variantes de jugador van por `handle_player_command`.
        GmCommand::RestartHere
        | GmCommand::RestartTown
        | GmCommand::Logout
        | GmCommand::Quit
        | GmCommand::PhaseSelect => {}
    }
    Ok(Outcome::Continue)
}

/// Comandos GM_PLAYER (nivel 0, cmd.cpp:340-347) — el diálogo de muerte y el
/// menú del cliente. Parity do_restart/do_cmd (cmd_general.cpp):
///
/// - `/restart_here` (SCMD_RESTART_HERE, POS_DEAD): revive en el MISMO
///   punto — RestartAtSamePos (remove + insert del personaje).
/// - `/restart_town` (SCMD_RESTART_TOWN, POS_DEAD): revive EN LA CIUDAD —
///   WarpSet(exit_x/y) → GC_WARP, el cliente reconecta con DirectEnter.
/// - `/logout` (SCMD_LOGOUT): cierra la conexión (PHASE_CLOSE).
/// - `/quit` (SCMD_QUIT): cierra la conexión.
/// - `/phase_select` (SCMD_PHASE_SELECT): GC_PHASE(SELECT) + cierre — el
///   cliente vuelve al selector de personajes y reconecta.
///
/// El revive reutiliza el path del CG_SCRIPT_ANSWER (`script::handle` — el
/// C++ los trata igual: el diálogo de muerte y el comando comparten el flujo
/// de RestartAtSamePos/WarpSet). Subset: sin evento de muerte (m_pkDeadEvent)
/// el C++ rechaza (CloseRestartWindow) — aquí si no está muerto se ignora.
async fn handle_player_command(
    session: &mut Session,
    command: GmCommand,
) -> Result<Outcome, String> {
    match command {
        GmCommand::RestartHere | GmCommand::RestartTown => {
            // Solo muerto (POS_DEAD — parity `ch->IsDead()` cmd_general.cpp:404).
            if session.row().hp > 0 {
                eprintln!(
                    "server_realms: channel conn {}: {} mandó /restart VIVO — \
                     ignorado (parity CloseRestartWindow)",
                    session.conn_id, session.row().name
                );
                return Ok(Outcome::Continue);
            }
            // Syntetiza el CG_SCRIPT_ANSWER (answer 1 = ciudad, 0 = mismo
            // punto) — el path de revive ya existe y reenvía ADDITIONAL_INFO +
            // GC_CHARACTER_DEL + GC_WARP + persistencia.
            let answer = if matches!(command, GmCommand::RestartTown) { 1 } else { 0 };
            crate::channel::script::revive(session, answer).await?;
            Ok(Outcome::Continue)
        }
        GmCommand::Logout | GmCommand::Quit => {
            eprintln!(
                "server_realms: channel conn {}: {} — cierre por /{:?}",
                session.conn_id, session.row().name, command
            );
            Ok(Outcome::Close(format!("comando /{:?} — cierre de conexión", command)))
        }
        GmCommand::PhaseSelect => {
            eprintln!(
                "server_realms: channel conn {}: {} — vuelta al selector \
                 (/phase_select)",
                session.conn_id, session.row().name
            );
            // GC_PHASE(SELECT) → el cliente cambia al selector de personajes
            // (parity `d->SetPhase(PHASE_SELECT)` desc.cpp:585-597) y luego
            // se cierra la conexión (el cliente reconecta al channel).
            session
                .send(&protocol::TPacketGCPhase::new(protocol::phase::SELECT).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_PHASE(SELECT): {e}"))?;
            Ok(Outcome::Close("comando /phase_select — reconexión al selector".into()))
        }
        _ => Ok(Outcome::Continue),
    }
}

/// `warp <x metros> <y metros>` → `GC_WARP` (parity do_warp cmd_gm.cpp:
/// 319-387: `x *= 100; y *= 100` → `WarpSet` — el cliente RECONECTA con el
/// flujo DirectEnter completo, igual que el revive en la ciudad).
async fn warp(session: &mut Session, x: i32, y: i32) -> Result<(), String> {
    let (wx, wy) = (x.saturating_mul(100), y.saturating_mul(100));
    let (ip, port) = parse_listen(&session.config.listen)?;
    let addr = packets::ip_to_inet_addr(&ip)?;
    session
        .send(&protocol::world::TPacketGCWarp::new(wx, wy, addr, port).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_WARP (GM): {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: GM {} warpeó a {wx},{wy} \
         ({}:{port} — reconexión)",
        session.conn_id, session.row().name, ip
    );
    Ok(())
}

/// `item <vnum> [count]` → item nuevo en el primer slot libre del
/// inventario (parity do_item cmd_gm.cpp:398-448: CreateItem + GetEmptyInventory
/// + AutoStackItemEx; el subset crea el slot — el stacking del pickup ya
/// existe). count clamp 1..200 en el parseo.
async fn give_item(session: &mut Session, vnum: u32, count: u32) -> Result<(), String> {
    // El item debe existir en el proto (parity CreateItem → nullptr → INFO).
    if ItemRepo::new(session.pool.clone())
        .load_proto_use_values(i64::from(vnum))
        .await?
        .is_none()
    {
        eprintln!(
            "server_realms: channel conn {}: GM {} — item vnum {vnum} \
             inexistente (item_proto)",
            session.conn_id, session.row().name
        );
        gm_info(session, "No such item by that vnum").await?;
        return Ok(());
    }
    // Primer cell libre (parity GetEmptyInventory, char_item.cpp:709-711).
    let occupied: std::collections::HashSet<u16> = session
        .inventory
        .iter()
        .filter(|i| i.window == "INVENTORY")
        .map(|i| i.pos as u16)
        .collect();
    let Some(slot) = (0u16..INVENTORY_MAX_NUM).find(|c| !occupied.contains(c)) else {
        gm_info(session, "Not enough inventory space").await?;
        return Ok(());
    };
    // id del rango ITEM_ID_RANGE (parity ItemIDRangeManager.cpp:93,121).
    let id = ItemRepo::new(session.pool.clone())
        .max_id_in_range(100_000_000, 200_000_000)
        .await?
        .map(|m| m + 1)
        .unwrap_or(100_000_000);
    let item = database::item::ItemRow {
        id,
        window: "INVENTORY".to_string(),
        pos: slot as i32,
        count: i64::from(count),
        vnum: i64::from(vnum),
        sockets: [0; 3],
        attrs: [(0, 0); 7],
    };
    let set = TPacketGCItemSet {
        header: TPacketGCItemSet::HEADER,
        cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: slot },
        vnum,
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
        .map_err(|e| format!("enviando GC_ITEM_SET (GM item): {e}"))?;
    ItemRepo::new(session.pool.clone())
        .upsert(&item, session.row().id)
        .await?;
    session.inventory.push(item);
    eprintln!(
        "server_realms: channel conn {}: GM {} dió item vnum {vnum} \
         x{count} → slot {slot}",
        session.conn_id, session.row().name
    );
    Ok(())
}

/// `notice <texto>` → GC_CHAT tipo CHAT_TYPE_NOTICE (parity do_notice
/// cmd_gm.cpp:1354+ → BroadcastNotice). GAP documentado: el broadcast a
/// TODOS los jugadores necesita el task del canal (routes por vid — fuera
/// de esta conexión); el subset manda el notice al GM (verificación visual)
/// y lo loguea.
async fn self_notice(session: &mut Session, text: &str) -> Result<(), String> {
    let size = (9 + text.len()) as u16;
    let mut out = Vec::with_capacity(9 + text.len());
    out.push(protocol::header::GC_CHAT);
    out.extend_from_slice(&size.to_le_bytes());
    out.push(CHAT_TYPE_NOTICE);
    out.extend_from_slice(&session.player_vid().to_le_bytes());
    out.push(session.empire);
    out.extend_from_slice(text.as_bytes());
    session
        .send(&out)
        .await
        .map_err(|e| format!("enviando GC_CHAT (GM notice): {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: GM {} NOTICE: {text} \
         (broadcast a todos: GAP — task del canal)",
        session.conn_id, session.row().name
    );
    Ok(())
}

/// `level <nivel>` → nivel del personaje (clamp 1..99 en el parseo) + la
/// exp del nivel nuevo + sync al mundo (la DEF del ataque del mob) +
/// GC_POINTS + save. GAP: el ResetPoint del C++ recalcula stat/skill points
/// y limpia skills — el subset solo mueve el nivel.
async fn set_level(session: &mut Session, level: i32) -> Result<(), String> {
    session.row_mut().level = level as i16;
    // NEXT_EXP del nivel nuevo (el level-up del kill lo recarga igual).
    session.next_exp = CommonRepo::new(session.pool.clone())
        .next_exp(level as i16)
        .await
        .unwrap_or(0);
    session.intent(Intent::Combat(CombatIntent::SetLevel {
        player_vid: session.player_vid(),
        level,
    }))?;
    session
        .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_POINTS (GM level): {e}"))?;
    session.save();
    eprintln!(
        "server_realms: channel conn {}: GM {} puso nivel {level}",
        session.conn_id, session.row().name
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El hook de chat manda el texto SIN la '/' (parity input_main.cpp:663:
    /// `interpret_command(ch, buf + 1, ...)`).
    #[test]
    fn handle_receives_text_after_slash() {
        assert_eq!(gm::parse_command("warp 1 2"), Some(GmCommand::Warp { x: 1, y: 2 }));
        assert_eq!(gm::parse_command("/warp 1 2"), None, "la '/' no llega aquí");
    }
}
