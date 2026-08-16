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
//! las `routes`), `level n` (row + GC_POINTS + mundo + save) y el lote 2
//! GM_PLAYER (lane B, 2026-08-15 — regla 4: nivel 0 SIN gmlist):
//! `set_walk_mode`/`set_run_mode` (estado del personaje + GC_WALK_MODE),
//! `skillup <vnum>` (skill_point del row + blob skill_level + GC_POINTS +
//! GC_SKILL_LEVEL + save). Los 30 del lote SIN sistema subyacente (party,
//! horse, emociones, view_equip, observer, safebox, mount, pvp, gskillup)
//! responden INFO 'not implemented' (regla 3 — el comando EXISTE en el
//! cmd_info[]; el GAP de cada uno está documentado en la variante del enum
//! en game_core/gm.rs).
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
//! - `skillup` sin skill_proto: el C++ valida CanUseSkill/level-limit/
//!   pre-skill/learnability y aplica saltos master por RNG (17→20/30/40 —
//!   char_skill.cpp:745-777); el subset solo gasta el punto y sube +1
//!   (cap 40, con el bMasterType del nivel nuevo — parity SetSkillLevel
//!   char_skill.cpp:207-217).
//! - Lote 2 sin sistema (INFO 'not implemented'): party (grupos +
//!   broadcast), horse (packets HORSE_* + proto), emociones (GC emoticon a
//!   los cercanos), view_equip (equip de OTRO vid), observer (modo
//!   observador), safebox (tablas + paquetes propios), mount (stub vacío
//!   hasta en el C++), pvp (modo PVP), gskillup (guild).
//! - El GC_WALK_MODE del C++ se broadcasta a los que VEN al personaje
//!   (PacketView, char.cpp:5773-5780); el subset lo manda solo a la propia
//!   conexión (el routing por vid del canal es el mismo GAP del notice).

use database::common::CommonRepo;
use database::item::ItemRepo;
use database::npc::MobRepo;
use game_core::ecs::{CombatIntent, Intent};
use game_core::gm::{self, GmCommand, StatPoint};
use game_core::packets;
use protocol::world::{TPacketGCItemSet, TPacketGCSkillLevel, TItemPos, TPlayerSkill};

use crate::channel::session::{Outcome, Session};
use crate::channel::{parse_listen, INVENTORY_MAX_NUM};

/// CHAT_TYPE_INFO = 1, CHAT_TYPE_NOTICE = 2 (length.h:514-525).
const CHAT_TYPE_INFO: u8 = 1;
const CHAT_TYPE_NOTICE: u8 = 2;

/// HEADER_GC_WALK_MODE = 111 (packet.h:212) — el protocol reforge aún no
/// tiene la struct; igual que CHAT_TYPE_* se define aquí (bytes crudos).
const HEADER_GC_WALK_MODE: u8 = 111;
/// WALKMODE_RUN/WALK (packet.h:1880-1882) — el modo del TPacketGCWalkMode
/// (header 111, vid, mode — packet.h:1884-1886).
const WALKMODE_RUN: u8 = 0;
const WALKMODE_WALK: u8 = 1;
/// Cap del nivel de skill (parity SetSkillLevel MIN(40, bLev)
/// char_skill.cpp:207).
const SKILL_LEVEL_MAX: u8 = 40;

/// Texto del chat al GM (GC_CHAT type INFO — parity del ChatPacket del C++;
/// sin locale system → EN, divergencia documentada). Reutilizado por
/// `channel/safebox.rs` para los INFO del safebox (parity ChatPacket del
/// C++ en ReqSafeboxLoad/CloseSafebox).
pub(crate) async fn gm_info(session: &mut Session, text: &str) -> Result<(), String> {
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
    // cmd.cpp:339-466, lote 2 incluido; accesibles a todos). El C++ los
    // despacha directo en do_restart/do_cmd/do_skillup/do_set_walk_mode...;
    // el revive solo aplica muerto (POS_DEAD) y los de cierre mandan su
    // paquete y cierran la conexión.
    if gm::required_level(&command) == gm::gm_level::PLAYER {
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
        // Lote 3 — los GM de verdad (parity cmd.cpp): mob/kill HIGH_WIZARD,
        // purge WIZARD, goto LOW_WIZARD.
        GmCommand::Mob { vnum, count } => gm_mob(session, vnum, count).await?,
        GmCommand::Kill => gm_kill(session).await?,
        GmCommand::Purge { all } => gm_purge(session, all).await?,
        GmCommand::Goto { name } => gm_goto(session, &name).await?,
        // Safebox (tamaño) — GM_HIGH_WIZARD (parity cmd.cpp:351): el lote 2
        // de jugador va por `handle_player_command`; este es el único
        // safebox con nivel GM real.
        GmCommand::Safebox { size } => {
            crate::channel::safebox::set_size(session, size).await?;
        }
        // Inalcanzable: todos los GM_PLAYER (nivel 0) van por
        // `handle_player_command` (routing arriba por required_level).
        _ => {}
    }
    Ok(Outcome::Continue)
}

/// Comandos GM_PLAYER (nivel 0, cmd.cpp:339-466 — lote 1: diálogo de muerte
/// y menú del cliente; lote 2 del lane B: modo de movimiento, skillup y los
/// 28 sin sistema → INFO 'not implemented'). Parity do_restart/do_cmd
/// (cmd_general.cpp):
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
        // Lote 2 — REALES (regla 2 del lane B: sistema subyacente + persistencia).
        GmCommand::SafeboxPassword { password } => {
            crate::channel::safebox::open(session, &password).await?;
            Ok(Outcome::Continue)
        }
        GmCommand::SafeboxClose => {
            crate::channel::safebox::close(session).await?;
            Ok(Outcome::Continue)
        }
        GmCommand::SetWalkMode => {
            set_walk_mode(session, true).await?;
            Ok(Outcome::Continue)
        }
        GmCommand::SetRunMode => {
            set_walk_mode(session, false).await?;
            Ok(Outcome::Continue)
        }
        // Lote 3 — `/stat`/`/stat-`: GM_PLAYER (cmd.cpp:324-325 — el
        // cliente los usa para asignar stats SIN gmlist; parity do_stat/
        // do_stat_minus cmd_general.cpp:577-702).
        GmCommand::Stat { point, amount } => {
            gm_stat(session, point, amount).await?;
            Ok(Outcome::Continue)
        }
        GmCommand::StatMinus { point, amount } => {
            gm_stat_minus(session, point, amount).await?;
            Ok(Outcome::Continue)
        }
        // Inalcanzable aquí: `safebox` (tamaño) requiere HIGH_WIZARD — el
        // routing de `handle()` lo manda al match GM (exhaustividad).
        GmCommand::Safebox { .. } => Ok(Outcome::Continue),
        GmCommand::SkillUp { vnum } => {
            skillup(session, vnum).await?;
            Ok(Outcome::Continue)
        }
        // Lote 2 — SIN sistema subyacente en reforge (regla 3): el comando
        // EXISTE en el cmd_info[] → INFO 'not implemented' (NO 'No such
        // command'). El GAP de cada uno está documentado en la variante del
        // enum (game_core/gm.rs). Safebox/SafeboxClose/SafeboxPassword ya
        // NO están aquí (sistema real — channel/safebox.rs).
        GmCommand::Mount
        | GmCommand::HorseState
        | GmCommand::HorseLevel
        | GmCommand::HorseRide
        | GmCommand::HorseSummon
        | GmCommand::HorseUnsummon
        | GmCommand::HorseSetStat
        | GmCommand::PartyRequest
        | GmCommand::PartyRequestAccept
        | GmCommand::PartyRequestDeny
        | GmCommand::Pvp
        | GmCommand::ViewEquip
        | GmCommand::Observer
        | GmCommand::ObserverExit
        | GmCommand::GuildSkillUp
        | GmCommand::EmotionAllow
        | GmCommand::Kiss
        | GmCommand::Slap
        | GmCommand::FrenchKiss
        | GmCommand::Clap
        | GmCommand::Cheer1
        | GmCommand::Cheer2
        | GmCommand::Dance1
        | GmCommand::Dance2
        | GmCommand::Dance3
        | GmCommand::Dance4
        | GmCommand::Dance5
        | GmCommand::Dance6
        | GmCommand::Congratulation
        | GmCommand::Forgive => {
            not_implemented(session, &command).await?;
            Ok(Outcome::Continue)
        }
        // Inalcanzable: los GM de verdad (warp/item/notice/level/mob/kill/
        // purge/goto) van por el dispatch de handle() — el routing manda
        // aquí SOLO los GM_PLAYER (nivel 0). Exhaustividad por si el
        // routing cambia.
        GmCommand::Warp { .. }
        | GmCommand::GiveItem { .. }
        | GmCommand::Notice { .. }
        | GmCommand::SetLevel { .. }
        | GmCommand::Mob { .. }
        | GmCommand::Kill
        | GmCommand::Purge { .. }
        | GmCommand::Goto { .. } => Ok(Outcome::Continue),
    }
}

/// Comandos GM_PLAYER sin sistema subyacente en reforge → INFO
/// 'not implemented' (regla 3 del lane B: NO 'No such command' — el comando
/// SÍ está en el cmd_info[] del C++ congelado). Texto EN (sin locale system
/// — divergencia documentada, igual que el resto de INFO del crate).
async fn not_implemented(session: &mut Session, command: &GmCommand) -> Result<(), String> {
    eprintln!(
        "server_realms: channel conn {}: /{:?} de {} — sin sistema en \
         reforge (GAP) → INFO 'not implemented'",
        session.conn_id, command, session.row().name
    );
    gm_info(session, "not implemented").await
}

/// `/set_walk_mode`/`/set_run_mode` — modo de movimiento del personaje
/// (parity do_set_walk_mode/do_set_run_mode cmd_general.cpp:927-937:
/// SetNowWalking + SetWalking, char.cpp:5759-5780) + GC_WALK_MODE (header
/// 111, packet.h:212; vid + WALKMODE_RUN=0/WALKMODE_WALK=1,
/// packet.h:1880-1886). El C++ NO persiste el flag en DB (la row de 42
/// columnas no tiene columna walking — parity) — vive en el CHARACTER; aquí
/// en la Session (por conexión). El broadcast del C++ es PacketView (a los
/// que VEN al personaje) — el subset manda solo a la propia conexión (GAP:
/// routing por vid del canal, mismo que el notice).
async fn set_walk_mode(session: &mut Session, walk: bool) -> Result<(), String> {
    session.walking = walk;
    let mut out = Vec::with_capacity(6);
    out.push(HEADER_GC_WALK_MODE);
    out.extend_from_slice(&session.player_vid().to_le_bytes());
    out.push(if walk { WALKMODE_WALK } else { WALKMODE_RUN });
    session
        .send(&out)
        .await
        .map_err(|e| format!("enviando GC_WALK_MODE: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: {} — modo {}",
        session.conn_id,
        session.row().name,
        if walk { "caminar" } else { "correr" }
    );
    Ok(())
}

/// El bMasterType del nivel nuevo (parity SetSkillLevel
/// char_skill.cpp:207-217 + length.h:628-631: SKILL_NORMAL=0,
/// SKILL_MASTER=1, SKILL_GRAND_MASTER=2, SKILL_PERFECT_MASTER=3 — el C++
/// compara el nivel SIN cap (`bLev`); el del subset ya viene capado a
/// SKILL_LEVEL_MAX y 40 ≥ 40 → PERFECT, equivalente a bLev 41 → PERFECT).
fn master_type_for_level(level: u8) -> u8 {
    if level >= 40 {
        3 // SKILL_PERFECT_MASTER
    } else if level >= 30 {
        2 // SKILL_GRAND_MASTER
    } else if level >= 20 {
        1 // SKILL_MASTER
    } else {
        0 // SKILL_NORMAL
    }
}

/// Aplica el skillup al blob de `skill_level` (255 × TPlayerSkill de 6 B —
/// tables.h:351-356): +1 nivel (cap SKILL_LEVEL_MAX) en bLevel (byte +1 de
/// la entrada) y el bMasterType del nivel nuevo en el byte 0 (parity
/// SetSkillLevel char_skill.cpp:207-217 — el C++ lo escribe en CADA
/// subida). vnum 0 → None (NO-OP: parity CanUseSkill char_skill.cpp:3572
/// `if (0 == dwSkillVnum) return false;` — el switch del do_skillup no
/// matchea 0). Invariantes del caller: blob completo (1530 B) y
/// vnum < SKILL_MAX_NUM.
fn skillup_apply(blob: &mut [u8], vnum: u32) -> Option<u8> {
    if vnum == 0 {
        return None; // NO-OP: ni nivel ni master se tocan
    }
    let off = vnum as usize * TPlayerSkill::SIZE;
    let new_level = blob[off + 1].saturating_add(1).min(SKILL_LEVEL_MAX);
    blob[off + 1] = new_level;
    blob[off] = master_type_for_level(new_level);
    Some(new_level)
}

/// `/skillup <vnum>` — sube un skill gastando 1 skill_point del ROW (parity
/// do_skillup cmd_general.cpp:754-793 → SkillLevelUp char_skill.cpp:641-760:
/// `GetPoint(idx) < 1 → return; PointChange(idx, -1)`): skill_point −1,
/// +1 nivel con cap 40 (SetSkillLevel MIN(40, bLev) char_skill.cpp:207) y
/// el bMasterType del nivel nuevo en el byte 0 de la entrada (parity
/// SetSkillLevel char_skill.cpp:207-217 — el cliente deriva el nivel
/// mostrado del grado, PythonPlayer.cpp:970-985),
/// GC_POINTS + GC_SKILL_LEVEL (orden parity: PointChange antes que
/// SkillLevelPacket) + save. El bytea `player.skill_level` ES la serie de
/// 255 × TPlayerSkill (6 B — tables.h:351-356); bMasterType en el byte 0,
/// b_level en el byte +1 de la entrada. `/skillup 0` → NO-OP silencioso
/// (parity CanUseSkill char_skill.cpp:3572 + el switch del do_skillup).
/// GAPs: sin skill_proto en reforge → sin CanUseSkill/level-limit/
/// pre-skill/learnability, sin el chequeo del tipo del skill
/// (POINT_SUB_SKILL/POINT_HORSE_SKILL — el subset siempre POINT_SKILL) y sin
/// los saltos master por RNG (17→20/30/40, char_skill.cpp:745-777).
async fn skillup(session: &mut Session, vnum: Option<u32>) -> Result<(), String> {
    let Some(vnum) = vnum else {
        // Parity cmd_general.cpp:759-761: sin argumento → no-op silencioso.
        eprintln!(
            "server_realms: channel conn {}: /skillup sin vnum — no-op (parity)",
            session.conn_id
        );
        return Ok(());
    };
    if vnum as usize >= TPacketGCSkillLevel::SKILL_MAX_NUM {
        // Parity char_skill.cpp:669-673: vnum overflow → sys_err + no-op.
        eprintln!(
            "server_realms: channel conn {}: /skillup vnum {vnum} >= SKILL_MAX_NUM \
             ({}) — no-op (parity)",
            session.conn_id,
            TPacketGCSkillLevel::SKILL_MAX_NUM
        );
        return Ok(());
    }
    // Parity char_skill.cpp:703-705 (`if (!GetSkillGroup()) return;`) y
    // 724-726 (`GetPoint(idx) < 1 → return` — sin mensaje en el C++).
    if session.row().skill_group == 0 || session.row().skill_point < 1 {
        eprintln!(
            "server_realms: channel conn {}: /skillup {vnum} — sin skill_group \
             o sin skill_point — no-op (parity)",
            session.conn_id
        );
        return Ok(());
    }
    // None/corto → cero (defensivo, mismo criterio que skill_level_packet).
    let mut blob = match session.row().skill_level.clone() {
        Some(b) if b.len() == TPacketGCSkillLevel::SKILL_MAX_NUM * TPlayerSkill::SIZE => b,
        _ => vec![0; TPacketGCSkillLevel::SKILL_MAX_NUM * TPlayerSkill::SIZE],
    };
    // vnum 0 → None → NO-OP silencioso: ni blob mutado ni punto gastado
    // (el None corta ANTES de `skill_point -= 1` — parity CanUseSkill
    // char_skill.cpp:3572, defecto 1 verifier 2026-08-15).
    let Some(new_level) = skillup_apply(&mut blob, vnum) else {
        eprintln!(
            "server_realms: channel conn {}: /skillup 0 — no-op (parity)",
            session.conn_id
        );
        return Ok(());
    };
    session.row_mut().skill_level = Some(blob);
    session.row_mut().skill_point -= 1;
    // Orden parity: PointChange (GC_POINTS) → SkillLevelPacket (GC_SKILL_LEVEL).
    session
        .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_POINTS (GM skillup): {e}"))?;
    session
        .send(&packets::skill_level_packet(session.row().skill_level.as_ref()).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_SKILL_LEVEL (GM skillup): {e}"))?;
    session.save();
    eprintln!(
        "server_realms: channel conn {}: {} subió skill {vnum} → nivel {} \
         (skill_point {})",
        session.conn_id,
        session.row().name,
        new_level,
        session.row().skill_point
    );
    Ok(())
}

/// `warp <x metros> <y metros>` → `GC_WARP` (parity do_warp cmd_gm.cpp:
/// 319-387: `x *= 100; y *= 100` → `WarpSet` — el cliente RECONECTA con el
/// flujo DirectEnter completo, igual que el revive en la ciudad).
async fn warp(session: &mut Session, x: i32, y: i32) -> Result<(), String> {
    let (wx, wy) = (x.saturating_mul(100), y.saturating_mul(100));
    warp_units(session, wx, wy, "GM warp").await
}

/// `GC_WARP` a UNITS concretas (base del `warp` en metros y del `/goto` de
/// GM — el cliente RECONECTA con el flujo DirectEnter completo).
async fn warp_units(session: &mut Session, x: i32, y: i32, why: &str) -> Result<(), String> {
    let (ip, port) = parse_listen(&session.config.listen)?;
    let addr = packets::ip_to_inet_addr(&ip)?;
    session
        .send(&protocol::world::TPacketGCWarp::new(x, y, addr, port).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_WARP ({why}): {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: GM {} {why} → {x},{y} \
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
    let Some(proto) = ItemRepo::new(session.pool.clone())
        .load_proto_use_values(i64::from(vnum))
        .await?
    else {
        eprintln!(
            "server_realms: channel conn {}: GM {} — item vnum {vnum} \
             inexistente (item_proto)",
            session.conn_id, session.row().name
        );
        gm_info(session, "No such item by that vnum").await?;
        return Ok(());
    };
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
    // Lane attrs: el GM `item` es CreateItem(..., true) (cmd_gm.cpp:437) —
    // roll de magic_pct → attrs mágicos + rare, y socket_pct sockets abiertos.
    let mut sockets = [0i64; 3];
    let mut attrs = [(0i16, 0i16); 7];
    let mut rng = crate::channel::rand32;
    database::attr::roll_creation_bonus(
        &mut rng,
        proto.magic_pct,
        proto.socket_pct,
        &session.attr_tables,
        proto.b_type,
        proto.b_sub_type,
        &mut sockets,
        &mut attrs,
    );
    let item = database::item::ItemRow {
        id,
        window: "INVENTORY".to_string(),
        pos: slot as i32,
        count: i64::from(count),
        vnum: i64::from(vnum),
        sockets,
        attrs,
    };
    let set = TPacketGCItemSet {
        header: TPacketGCItemSet::HEADER,
        cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: slot },
        vnum,
        count: count as u8,
        flags: 0,
        anti_flags: 0,
        highlight: 0,
        sockets,
        attrs,
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

// === Lote 3 (2026-08-17): mob / kill / purge / goto / stat ===

/// `mob <vnum> [count]` — spawn de mobs alrededor del GM (parity do_mob
/// cmd_gm.cpp:630-700 → SpawnMobRange: rect ±(200..750) units, count clamp
/// 1..20). El mob row se carga de `mob_proto` (MobRepo::load_by_vnum); vnum
/// inexistente → "No such mob by that vnum" (parity). El MUNDO materializa
/// las copias y emite los ADDs (el wire lo construye `entry_spawns` — el
/// mundo no toca PG). GAP: el caso nombre del C++ (`CMobManager::Get(arg1,
/// true)`) no se resuelve — reforge no tiene el índice nombre→vnum.
async fn gm_mob(session: &mut Session, vnum: u32, count: u32) -> Result<(), String> {
    let Some(row) = MobRepo::new(session.pool.clone())
        .load_by_vnum(i64::from(vnum))
        .await?
    else {
        eprintln!(
            "server_realms: channel conn {}: GM {} — mob vnum {vnum} \
             inexistente (mob_proto)",
            session.conn_id, session.row().name
        );
        gm_info(session, &format!("No such mob by that vnum: {vnum}")).await?;
        return Ok(());
    };
    // Posición VIVA del GM (la fuente de verdad del x/y — session.rs save).
    let (x, y) = (session.motion().x, session.motion().y);
    let map_index = session.row().map_index;
    session.intent(Intent::Combat(CombatIntent::GmSpawn {
        player_vid: session.player_vid(),
        map_index: map_index as u32,
        x,
        y,
        count,
        mob: row,
    }))?;
    eprintln!(
        "server_realms: channel conn {}: GM {} spawneó mob {vnum} x{count} \
         en {x},{y} (mapa {map_index})",
        session.conn_id, session.row().name
    );
    Ok(())
}

/// `kill` — mata el TARGET del jugador (CG_TARGET) si es un mob (parity
/// do_kill cmd_gm.cpp:1505+ → `SetDead` directo: SIN drop ni exp; PC →
/// no-op — el mundo solo resuelve mobs del NpcIndex). Divergencia
/// documentada: el C++ mata a un JUGADOR por nombre; el rewrite usa el
/// target. El GC_DEAD (animación) lo manda el evento GmKilled; el
/// GC_CHARACTER_DEL a todos los espectadores lo emite el Despawned del
/// mismo kill (routing del mundo).
async fn gm_kill(session: &mut Session) -> Result<(), String> {
    let Some(target_vid) = session.target_vid else {
        gm_info(session, "No target (click a mob first)").await?;
        return Ok(());
    };
    session.intent(Intent::Combat(CombatIntent::GmKill {
        player_vid: session.player_vid(),
        target_vid,
    }))?;
    eprintln!(
        "server_realms: channel conn {}: GM {} — /kill del target {target_vid}",
        session.conn_id, session.row().name
    );
    Ok(())
}

/// `purge [all]` — mata los mobs del área (parity do_purge cmd_gm.cpp:775+
/// → FuncPurge: radio 1000 units sin `all`, todo el mapa con `all`;
/// M2_DESTROY_CHARACTER — sin drop ni exp, sin animación de muerte). El
/// mundo emite los GC_CHARACTER_DEL a los espectadores.
async fn gm_purge(session: &mut Session, all: bool) -> Result<(), String> {
    let (x, y) = (session.motion().x, session.motion().y);
    let map_index = session.row().map_index;
    session.intent(Intent::Combat(CombatIntent::GmPurge {
        player_vid: session.player_vid(),
        map_index: map_index as u32,
        x,
        y,
        all,
    }))?;
    eprintln!(
        "server_realms: channel conn {}: GM {} — /purge{} en {x},{y} \
         (mapa {map_index})",
        session.conn_id,
        session.row().name,
        if all { " all" } else { "" }
    );
    Ok(())
}

/// `goto <nombre>` — teletransporta al GM a la posición del jugador
/// nombrado (parity do_goto → WarpSet; el C++ congelado es
/// `goto <x y>`/`<mapname>` — divergencia deliberada del lane: la forma
/// jugador es la de mayor valor jugable). El destino sale del registro de
/// sesiones activas del chat (chat.rs::find_player — name → vid/posición
/// VIVA). Parity WarpSet: mover + persistir ANTES del GC_WARP (el
/// DirectEnter de la reconexión recarga el row guardado — bug C26 del
/// revive). GAP: sin cross-map por eventos — el row.map_index del destino
/// se persiste igual (el entry de la reconexión carga el mapa del row).
async fn gm_goto(session: &mut Session, name: &str) -> Result<(), String> {
    let Some((_vid, map_index, x, y)) = crate::channel::chat::find_player(name) else {
        gm_info(session, &format!("{name}: no such a player")).await?;
        return Ok(());
    };
    {
        let row = session.row_mut();
        row.x = x;
        row.y = y;
        row.map_index = map_index;
    }
    session.motion = Some(game_core::movement::initial(x, y));
    session.save();
    warp_units(session, x, y, "goto").await
}

/// `g_iStatusPointSetMaxValue = 90` (config.cpp:48) — el cap MAX_STAT del
/// do_stat (`GetRealPoint(idx) >= MAX_STAT` — cmd_general.cpp:675).
const STAT_MAX: i16 = 90;

/// Valor actual del stat en el row (parity `GetRealPoint`).
fn stat_value(row: &database::player::PlayerRow, point: StatPoint) -> i16 {
    match point {
        StatPoint::St => row.st,
        StatPoint::Dx => row.dx,
        StatPoint::Ht => row.ht,
        StatPoint::Iq => row.iq,
    }
}

/// Acceso mutable al stat del row (`SetRealPoint`).
fn stat_value_mut(row: &mut database::player::PlayerRow, point: StatPoint) -> &mut i16 {
    match point {
        StatPoint::St => &mut row.st,
        StatPoint::Dx => &mut row.dx,
        StatPoint::Ht => &mut row.ht,
        StatPoint::Iq => &mut row.iq,
    }
}

/// `JobInitialPoints` (constants.cpp:6-15): st/ht/dx/iq iniciales por job
/// (race 0..7 → job 0..3 — parity RaceToJob) — el FLOOR del `/stat-`
/// (parity do_stat_minus: no baja de ahí, cmd_general.cpp:587-625).
fn job_initial_stat(row: &database::player::PlayerRow, point: StatPoint) -> i16 {
    let init = match packets::race_to_job(row.job).unwrap_or(1) {
        0 => (6, 4, 3, 3), // JOB_WARRIOR
        1 => (4, 3, 6, 3), // JOB_ASSASSIN
        2 => (5, 3, 3, 5), // JOB_SURA
        _ => (3, 4, 3, 6), // JOB_SHAMAN
    };
    match point {
        StatPoint::St => init.0,
        StatPoint::Ht => init.1,
        StatPoint::Dx => init.2,
        StatPoint::Iq => init.3,
    }
}

/// Sync post-stat: mundo (el AI usa st/dx/iq/ht) + GC_POINTS + save
/// (parity PointChange → SendPointsPacket del C++).
async fn sync_stats(session: &mut Session) -> Result<(), String> {
    let (st, dx, iq, ht) = (session.row().st, session.row().dx, session.row().iq, session.row().ht);
    session.intent(Intent::Combat(CombatIntent::SetStats {
        player_vid: session.player_vid(),
        st: i32::from(st),
        dx: i32::from(dx),
        iq: i32::from(iq),
        ht: i32::from(ht),
    }))?;
    session
        .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_POINTS (GM stat): {e}"))?;
    session.save();
    Ok(())
}

/// `/stat <st|dx|ht|iq> [cantidad]` — asigna puntos de stat (parity do_stat
/// cmd_general.cpp:644-702: gasta POINT_STAT, cap MAX_STAT = 90 —
/// `nPoint = 90 - actual`; la cantidad es la extensión del lane, default 1).
/// Sin POINT_STAT suficiente → no-op silencioso (parity `GetPoint(POINT_STAT)
/// <= 0 → return`). El recálculo de MAX_HP/MAX_SP del C++ lo refleja el
/// GC_POINTS vía compute_max_points (max_hp = f(ht), max_sp = f(iq)).
async fn gm_stat(session: &mut Session, point: StatPoint, amount: i32) -> Result<(), String> {
    if i32::from(session.row().stat_point) < amount {
        eprintln!(
            "server_realms: channel conn {}: /stat {} {amount} — sin \
             POINT_STAT suficiente ({}) — no-op (parity)",
            session.conn_id,
            point.name(),
            session.row().stat_point
        );
        return Ok(());
    }
    let cur = stat_value(session.row(), point);
    // Cap MAX_STAT (parity do_stat: `nPoint = 90 - GetPoint` — el exceso no
    // se aplica NI se gasta).
    let applied = (i32::from(cur) + amount).min(i32::from(STAT_MAX)) - i32::from(cur);
    if applied <= 0 {
        eprintln!(
            "server_realms: channel conn {}: /stat {} — ya en el cap \
             MAX_STAT ({STAT_MAX}) — no-op (parity)",
            session.conn_id, point.name()
        );
        return Ok(());
    }
    *stat_value_mut(session.row_mut(), point) += applied as i16;
    session.row_mut().stat_point -= applied as i16;
    sync_stats(session).await?;
    eprintln!(
        "server_realms: channel conn {}: {} asignó +{applied} a {} \
         (queda {} POINT_STAT)",
        session.conn_id,
        session.row().name,
        point.name(),
        session.row().stat_point
    );
    Ok(())
}

/// `/stat- <st|dx|ht|iq> [cantidad]` — devuelve puntos de stat (parity
/// do_stat_minus cmd_general.cpp:577-643: gasta POINT_STAT_RESET_COUNT y no
/// baja del floor de los iniciales del job; `PointChange(POINT_STAT, +1)`).
/// Sin POINT_STAT_RESET_COUNT → no-op silencioso (parity).
async fn gm_stat_minus(session: &mut Session, point: StatPoint, amount: i32) -> Result<(), String> {
    if session.row().stat_reset_count < 1 {
        eprintln!(
            "server_realms: channel conn {}: /stat- {} — sin \
             POINT_STAT_RESET_COUNT — no-op (parity)",
            session.conn_id, point.name()
        );
        return Ok(());
    }
    let floor = job_initial_stat(session.row(), point);
    let cur = stat_value(session.row(), point);
    // Floor del job (parity `GetRealPoint <= JobInitialPoints → return`) +
    // no más del pedido.
    let applied = (cur - floor).min(amount as i16).max(0);
    if applied <= 0 {
        eprintln!(
            "server_realms: channel conn {}: /stat- {} — ya en el floor \
             inicial del job ({floor}) — no-op (parity)",
            session.conn_id, point.name()
        );
        return Ok(());
    }
    *stat_value_mut(session.row_mut(), point) -= applied;
    session.row_mut().stat_point = (session.row().stat_point + applied).min(STAT_MAX * 100);
    session.row_mut().stat_reset_count = (session.row().stat_reset_count - applied).max(0);
    sync_stats(session).await?;
    eprintln!(
        "server_realms: channel conn {}: {} devolvió {applied} de {} \
         (POINT_STAT {} — reset restantes {})",
        session.conn_id,
        session.row().name,
        point.name(),
        session.row().stat_point,
        session.row().stat_reset_count
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

    /// Defecto 1 (verifier 2026-08-15): `/skillup 0` → NO-OP silencioso
    /// SIEMPRE — parity CanUseSkill char_skill.cpp:3572 (`if (0 ==
    /// dwSkillVnum) return false;`) + el switch del do_skillup
    /// (cmd_general.cpp:766-791) no matchea 0. El None corta antes de
    /// `skill_point -= 1` en el handler → tampoco gasta el punto.
    #[test]
    fn skillup_vnum_zero_is_noop() {
        let mut blob = vec![0u8; 255 * TPlayerSkill::SIZE];
        blob[1 * TPlayerSkill::SIZE] = 1; // skill 1: MASTER
        blob[1 * TPlayerSkill::SIZE + 1] = 20;
        let original = blob.clone();
        assert_eq!(skillup_apply(&mut blob, 0), None, "vnum 0 → no-op");
        assert_eq!(blob, original, "el blob NO se muta");
        assert_eq!(skillup_apply(&mut blob, 1), Some(21), "los demás vnums sí suben");
    }

    /// Defecto 2 (verifier 2026-08-15): bMasterType se escribe en CADA
    /// subida en el byte 0 de la entrada (tables.h:351-356 — bMasterType,
    /// bLevel, tNextRead), parity SetSkillLevel char_skill.cpp:207-217 con
    /// los thresholds 20/30/40 (length.h:628-631: SKILL_NORMAL=0,
    /// SKILL_MASTER=1, SKILL_GRAND_MASTER=2, SKILL_PERFECT_MASTER=3). El
    /// cliente deriva el nivel mostrado del grado (PythonPlayer.cpp:970-985:
    /// grade 1 → level−20+1, etc.).
    #[test]
    fn skillup_writes_master_type_on_threshold_cross() {
        let mut blob = vec![0u8; 255 * TPlayerSkill::SIZE];
        // 19 → 20: cruza a MASTER(1)
        blob[1 * TPlayerSkill::SIZE + 1] = 19;
        assert_eq!(skillup_apply(&mut blob, 1), Some(20));
        assert_eq!(blob[1 * TPlayerSkill::SIZE + 1], 20);
        assert_eq!(blob[1 * TPlayerSkill::SIZE], 1, "20..29 → SKILL_MASTER(1)");
        // 29 → 30: GRAND_MASTER(2)
        blob[1 * TPlayerSkill::SIZE + 1] = 29;
        blob[1 * TPlayerSkill::SIZE] = 1;
        assert_eq!(skillup_apply(&mut blob, 1), Some(30));
        assert_eq!(blob[1 * TPlayerSkill::SIZE], 2, "30..39 → SKILL_GRAND_MASTER(2)");
        // 39 → 40: PERFECT_MASTER(3)
        blob[1 * TPlayerSkill::SIZE + 1] = 39;
        blob[1 * TPlayerSkill::SIZE] = 2;
        assert_eq!(skillup_apply(&mut blob, 1), Some(40));
        assert_eq!(blob[1 * TPlayerSkill::SIZE], 3, "40+ → SKILL_PERFECT_MASTER(3)");
        // Cap MIN(40, bLev): ya PERFECT no sube más (C++ bLev 41 → PERFECT).
        assert_eq!(skillup_apply(&mut blob, 1), Some(40));
        assert_eq!(blob[1 * TPlayerSkill::SIZE + 1], 40, "cap 40");
        assert_eq!(blob[1 * TPlayerSkill::SIZE], 3);
        // 5 → 6: NORMAL(0) se mantiene.
        blob[1 * TPlayerSkill::SIZE + 1] = 5;
        blob[1 * TPlayerSkill::SIZE] = 0;
        assert_eq!(skillup_apply(&mut blob, 1), Some(6));
        assert_eq!(blob[1 * TPlayerSkill::SIZE], 0, "0..19 → SKILL_NORMAL(0)");
    }

    /// Lote 3 (GM `/stat-`): el floor del job (parity JobInitialPoints
    /// constants.cpp:6-15 — st/ht/dx/iq iniciales por job, race→job del
    /// RaceToJob) + los accessors del stat del row.
    #[test]
    fn stat_job_floors_and_accessors() {
        fn row(job: i16) -> database::player::PlayerRow {
            database::player::PlayerRow {
                id: 1,
                name: "gm".into(),
                job,
                voice: 0,
                dir: 0,
                x: 0,
                y: 0,
                z: 0,
                map_index: 41,
                exit_x: 0,
                exit_y: 0,
                exit_map_index: 0,
                hp: 100,
                mp: 100,
                stamina: 100,
                random_hp: 0,
                random_sp: 0,
                playtime: 0,
                gold: 0,
                level: 5,
                level_step: 0,
                st: 30,
                ht: 30,
                dx: 30,
                iq: 30,
                exp: 0,
                stat_point: 10,
                skill_point: 0,
                sub_skill_point: 0,
                stat_reset_count: 3,
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
        // WARRIOR (race 0): 6/4/3/3; ASSASSIN (1): 4/3/6/3; SURA (2):
        // 5/3/3/5; SHAMAN (3): 3/4/3/6 — constants.cpp:6-15.
        let w = row(0);
        assert_eq!(job_initial_stat(&w, StatPoint::St), 6);
        assert_eq!(job_initial_stat(&w, StatPoint::Ht), 4);
        assert_eq!(job_initial_stat(&w, StatPoint::Dx), 3);
        assert_eq!(job_initial_stat(&w, StatPoint::Iq), 3);
        let a = row(1);
        assert_eq!(job_initial_stat(&a, StatPoint::St), 4);
        assert_eq!(job_initial_stat(&a, StatPoint::Dx), 6);
        let s = row(2);
        assert_eq!(job_initial_stat(&s, StatPoint::Iq), 5);
        let sh = row(3);
        assert_eq!(job_initial_stat(&sh, StatPoint::Ht), 4);
        assert_eq!(job_initial_stat(&sh, StatPoint::Iq), 6);
        // Accessors: lectura/escritura del stat del row (SetRealPoint).
        let mut r = row(1);
        assert_eq!(stat_value(&r, StatPoint::St), 30);
        *stat_value_mut(&mut r, StatPoint::St) += 5;
        assert_eq!(stat_value(&r, StatPoint::St), 35);
    }
}
