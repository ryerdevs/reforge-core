//! `channel/quest.rs` — emisión S→C del dominio QUEST (N1 → este slice): el
//! `QuestEvent::Run` del mundo → paquetes GC (GC_SCRIPT 45, GC_ITEM_SET 21,
//! GC_ITEM_UPDATE 22, GC_ITEM_DEL 20, GC_WARP 65, GC_CHAT 4) + persistencia
//! de `player.quest` (QuestRepo — save-by-event, patrón ADR-0008 igual que
//! items) + aplicación de recompensas al inventario de la sesión.
//!
//! El diálogo: el markup del event-set del cliente (parity `SendScript`,
//! questmanager.cpp:1060-1109) — texto + `[ENTER]`, `[NEXT]`, `[QUESTION …]`.
//! La respuesta (CG_SCRIPT_ANSWER) la resuelve `channel/script.rs` → mundo.

use database::item::{ItemRepo, ItemRow};
use database::quest::{QuestRepo, QuestRow};
use game_core::ecs::{Intent, QuestEvent, QuestIntent};
use game_core::packets;
use game_core::quest::{DirtyFlag, QuestEffect};
use protocol::world::{
    TItemPos, TPacketGCItemDelDeprecated, TPacketGCItemSet, TPacketGCItemUpdate,
};

use crate::channel::session::{Outcome, Session};
use crate::channel::{parse_listen, ITEM_COUNT_LIMIT, INVENTORY_MAX_NUM};

/// CG_SCRIPT_BUTTON (66, 5 B: header + idx DWORD — Packet.h:665-669): el
/// índice del botón del diálogo/ventana de quest (lane D). Parity
/// `ScriptButton` (input_main.cpp:1850-1868): Confirm timeout si el PC
/// espera confirmación; QuestInfo si `idx & 0x80000000`; QuestButton si no.
/// Se re-envía al mundo (`QuestIntent::Button`) — el engine aún no tiene la
/// API de botones: el mundo lo loguea y no-op (GAP documentado en
/// game_core::quest mod.rs §Cobertura).
pub async fn handle_button(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() < 5 {
        eprintln!(
            "server_realms: channel conn {}: CG_SCRIPT_BUTTON malformado ({})",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    }
    let idx = u32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]);
    session.intent(Intent::Quest(QuestIntent::Button {
        player_vid: session.player_vid(),
        idx,
    }))?;
    eprintln!(
        "server_realms: channel conn {}: botón de quest idx {idx} → mundo",
        session.conn_id
    );
    Ok(Outcome::Continue)
}

/// Texto del CG_QUEST_INPUT_STRING (bytes 1.. hasta el primer NUL — parity
/// strlcpy del C++). `None` si el paquete está malformado (< 2 B).
fn parse_input_string(pkt: &[u8]) -> Option<String> {
    if pkt.len() < 2 {
        return None;
    }
    let end = pkt[1..]
        .iter()
        .position(|&b| b == 0)
        .map(|i| i + 1)
        .unwrap_or(pkt.len());
    Some(String::from_utf8_lossy(&pkt[1..end]).into_owned())
}

/// CG_QUEST_INPUT_STRING (30, 66 B: header + char[65] — Packet.h:1002-1006):
/// el texto del diálogo de input de quest (lane D). Parity `QuestInputString`
/// (input_main.cpp:1894-1903): strlcpy a 65 y `CQuestManager::Input(pid,
/// msg)`. Se re-envía al mundo (`QuestIntent::Input`) — el engine aún no
/// implementa la acción `input` del DSL: el mundo lo loguea y no-op (GAP
/// documentado).
pub async fn handle_input_string(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let Some(text) = parse_input_string(pkt) else {
        eprintln!(
            "server_realms: channel conn {}: CG_QUEST_INPUT_STRING malformado ({})",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    };
    session.intent(Intent::Quest(QuestIntent::Input {
        player_vid: session.player_vid(),
        text,
    }))?;
    eprintln!(
        "server_realms: channel conn {}: input de quest → mundo",
        session.conn_id
    );
    Ok(Outcome::Continue)
}

/// CG_QUEST_CONFIRM (31, 6 B: header + answer + requestPID DWORD —
/// Packet.h:1008-1013): la respuesta del diálogo de confirmación de quest
/// (lane D). Parity `QuestConfirm` (input_main.cpp:1905-1917): el C++ la
/// reenvía al `CQuestManager::Confirm(requestPID, answer, pid)` del jugador
/// que ESPERA. Se re-envía al mundo (`QuestIntent::Confirm`) — el engine no
/// tiene confirmación cross-player: el mundo lo loguea y no-op (GAP
/// documentado).
pub async fn handle_confirm(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() < 6 {
        eprintln!(
            "server_realms: channel conn {}: CG_QUEST_CONFIRM malformado ({})",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    }
    let answer = pkt[1];
    let request_pid = u32::from_le_bytes([pkt[2], pkt[3], pkt[4], pkt[5]]);
    session.intent(Intent::Quest(QuestIntent::Confirm {
        player_vid: session.player_vid(),
        answer,
        request_pid,
    }))?;
    eprintln!(
        "server_realms: channel conn {}: confirm de quest (answer {answer}, \
         requestPID {request_pid}) → mundo",
        session.conn_id
    );
    Ok(Outcome::Continue)
}

/// Delegado de `NpcEvent::Quest` — el routing por jugador ya ocurrió en la
/// tarea del canal.
pub(super) async fn emit(session: &mut Session, q: QuestEvent) -> Result<(), String> {
    match q {
        QuestEvent::Run { script, effects, dirty, suspended, .. } => {
            if let Some(text) = script {
                send_script(session, &text).await?;
            }
            for eff in effects {
                match eff {
                    QuestEffect::GiveItem { vnum, count } => give_item(session, vnum, count).await?,
                    QuestEffect::RemoveItem { vnum, count } => remove_item(session, vnum, count).await?,
                    QuestEffect::Warp { x, y } => warp(session, x, y).await?,
                    QuestEffect::Notice(text) => notice(session, &text).await?,
                }
            }
            if !dirty.is_empty() {
                persist_flags(session, &dirty).await?;
            }
            if suspended {
                eprintln!(
                    "server_realms: channel conn {}: diálogo de quest enviado — \
                     esperando CG_SCRIPT_ANSWER",
                    session.conn_id
                );
            }
            Ok(())
        }
    }
}

/// GC_SCRIPT (45): header + size(WORD, = 6 + src) + skin(BYTE) + src_size(WORD)
///    y markup — parity `packet_script` (packet.h:1250-1259): el TPacketGCScript
///    del cliente es de 6 B (Packet.h:1874-1879 — el server desplegado no define
///    ENABLE_QUEST_CATEGORY) y `RecvScriptPacket` (PythonNetworkStreamPhaseGame.
///    cpp:2247-2283) parsea el markup del event-set.
pub fn script_packet(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + text.len());
    out.push(45); // HEADER_GC_SCRIPT
    let size = (6 + text.len()) as u16;
    out.extend_from_slice(&size.to_le_bytes());
    out.push(0); // skin: QUEST_SKIN_NORMAL
    out.extend_from_slice(&(text.len() as u16).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    out
}

async fn send_script(session: &mut Session, text: &str) -> Result<(), String> {
    let pkt = script_packet(text);
    session
        .send(&pkt)
        .await
        .map_err(|e| format!("enviando GC_SCRIPT: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: quest dialog ({} B): {}",
        session.conn_id,
        pkt.len(),
        String::from_utf8_lossy(text.as_bytes()).chars().take(120).collect::<String>()
    );
    Ok(())
}

/// `give_item2(vnum, count)` — recompensa al inventario (mismo patrón que el
/// pickup de events.rs: stack si hay un slot del mismo vnum con sockets vacíos
/// y count < límite; si no, primer cell libre → GC_ITEM_SET + upsert durable).
async fn give_item(session: &mut Session, vnum: u32, count: u32) -> Result<(), String> {
    let mut remaining = i64::from(count);
    while let Some(idx) = session.inventory.iter().position(|i| {
        i.window == "INVENTORY" && i.vnum == i64::from(vnum) && i.sockets == [0; 3] && i.count < ITEM_COUNT_LIMIT
    }) {
        let add = (ITEM_COUNT_LIMIT - session.inventory[idx].count).min(remaining);
        session.inventory[idx].count += add;
        remaining -= add;
        let up = TPacketGCItemUpdate {
            header: TPacketGCItemUpdate::HEADER,
            cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: session.inventory[idx].pos as u16 },
            count: session.inventory[idx].count as u8,
            sockets: session.inventory[idx].sockets,
            attrs: session.inventory[idx].attrs,
        };
        session
            .send(&up.to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_UPDATE (quest): {e}"))?;
        ItemRepo::new(session.pool.clone())
            .upsert(&session.inventory[idx], session.row().id)
            .await?;
        if remaining <= 0 {
            break;
        }
    }
    if remaining <= 0 {
        eprintln!(
            "server_realms: channel conn {}: quest dio item vnum {vnum} (apilado)",
            session.conn_id
        );
        return Ok(());
    }
    // Primer cell libre (parity `GetEmptyInventory` — igual que el pickup).
    let occupied: std::collections::HashSet<u16> = session
        .inventory
        .iter()
        .filter(|i| i.window == "INVENTORY")
        .map(|i| i.pos as u16)
        .collect();
    let Some(slot) = (0u16..INVENTORY_MAX_NUM).find(|c| !occupied.contains(c)) else {
        eprintln!(
            "server_realms: channel conn {}: quest dio item vnum {vnum} — inventario lleno",
            session.conn_id
        );
        return Ok(());
    };
    let id = ItemRepo::new(session.pool.clone())
        .max_id_in_range(100_000_000, 200_000_000)
        .await?
        .map(|m| m + 1)
        .unwrap_or(100_000_000);
    // Lane attrs (2026-08-16): los rewards de quest son CreateItem SIN
    // bTryMagic (questlua_game.cpp:110 — default false) → SOLO sockets
    // (`socket_pct` abiertos; sin attrs mágicos). Fail-open: proto sin fila
    // o tablas vacías → item plano.
    let (mut sockets, mut attrs) = ([0i64; 3], [(0i16, 0i16); 7]);
    if let Ok(Some(proto)) =
        ItemRepo::new(session.pool.clone()).load_proto_use_values(i64::from(vnum)).await
    {
        let mut rng = crate::channel::rand32;
        database::attr::roll_creation_bonus(
            &mut rng,
            0, // magic_pct 0 — parity quests (sin attrs mágicos)
            proto.socket_pct,
            &session.attr_tables,
            proto.b_type,
            proto.b_sub_type,
            &mut sockets,
            &mut attrs,
        );
    }
    let new_item = ItemRow {
        id,
        window: "INVENTORY".to_string(),
        pos: slot as i32,
        count: remaining,
        vnum: i64::from(vnum),
        sockets,
        attrs,
    };
    let set = TPacketGCItemSet {
        header: TPacketGCItemSet::HEADER,
        cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: slot },
        vnum,
        count: remaining as u8,
        flags: 0,
        anti_flags: 0,
        highlight: 0,
        sockets,
        attrs,
    };
    session
        .send(&set.to_bytes())
        .await
        .map_err(|e| format!("enviando GC_ITEM_SET (quest): {e}"))?;
    ItemRepo::new(session.pool.clone())
        .upsert(&new_item, session.row().id)
        .await?;
    session.inventory.push(new_item);
    eprintln!(
        "server_realms: channel conn {}: quest dio item vnum {vnum} → slot {slot}",
        session.conn_id
    );
    Ok(())
}

/// `remove_item(vnum, count)` — quita count del primer stack del vnum
/// (GC_ITEM_UPDATE; GC_ITEM_DEL + delete de la fila al agotar el stack).
async fn remove_item(session: &mut Session, vnum: u32, count: u32) -> Result<(), String> {
    let mut remaining = i64::from(count);
    let mut idx = 0;
    while remaining > 0 && idx < session.inventory.len() {
        if session.inventory[idx].window != "INVENTORY" || session.inventory[idx].vnum != i64::from(vnum) {
            idx += 1;
            continue;
        }
        let take = session.inventory[idx].count.min(remaining);
        session.inventory[idx].count -= take;
        remaining -= take;
        let consumed = session.inventory[idx].clone();
        if consumed.count == 0 {
            // Stack agotado: GC_ITEM_DEL (20, 42 B deprecated) + delete fila.
            let del = TPacketGCItemDelDeprecated::new(
                TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: consumed.pos as u16 },
                0,
                0,
            );
            session
                .send(&del.to_bytes())
                .await
                .map_err(|e| format!("enviando GC_ITEM_DEL (quest): {e}"))?;
            ItemRepo::new(session.pool.clone()).delete(consumed.id).await?;
            session.inventory.remove(idx);
        } else {
            let up = TPacketGCItemUpdate {
                header: TPacketGCItemUpdate::HEADER,
                cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: consumed.pos as u16 },
                count: consumed.count as u8,
                sockets: consumed.sockets,
                attrs: consumed.attrs,
            };
            session
                .send(&up.to_bytes())
                .await
                .map_err(|e| format!("enviando GC_ITEM_UPDATE (quest): {e}"))?;
            ItemRepo::new(session.pool.clone())
                .upsert(&consumed, session.row().id)
                .await?;
        }
    }
    if remaining > 0 {
        eprintln!(
            "server_realms: channel conn {}: quest pidió quitar vnum {vnum}×{count} — \
             quedan {remaining} sin quitar (inventario insuficiente)",
            session.conn_id
        );
    }
    Ok(())
}

/// `warp(x, y)` — GC_WARP (65, 15 B): el cliente reconecta con el flujo
/// DirectEnter (parity del revive del script.rs).
async fn warp(session: &mut Session, x: i64, y: i64) -> Result<(), String> {
    let (ip, port) = parse_listen(&session.config.listen)?;
    let addr = packets::ip_to_inet_addr(&ip)?;
    session
        .send(&protocol::world::TPacketGCWarp::new(x as i32, y as i32, addr, port).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_WARP (quest): {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: quest warpeó a {x},{y} (GC_WARP → {}:{port})",
        session.conn_id, ip
    );
    Ok(())
}

/// `notice(text)` — GC_CHAT (4) CHAT_TYPE_NOTICE (parity
/// `_notice` questlua_global.cpp:133-139 — ChatPacket(CHAT_TYPE_NOTICE)).
async fn notice(session: &mut Session, text: &str) -> Result<(), String> {
    let size = (9 + text.len()) as u16;
    let mut out = Vec::with_capacity(9 + text.len());
    out.push(protocol::header::GC_CHAT);
    out.extend_from_slice(&size.to_le_bytes());
    out.push(4); // CHAT_TYPE_NOTICE (char.h — el mismo valor que usa el legacy)
    out.extend_from_slice(&session.player_vid().to_le_bytes());
    out.push(session.empire);
    out.extend_from_slice(text.as_bytes());
    session
        .send(&out)
        .await
        .map_err(|e| format!("enviando GC_CHAT (notice quest): {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: quest notice: {}",
        session.conn_id,
        String::from_utf8_lossy(text.as_bytes()).chars().take(120).collect::<String>()
    );
    Ok(())
}

/// Persistencia de las filas sucias (`player.quest`): save-by-event con
/// QuestRepo (patrón ADR-0008 — conexión por llamada, como items). El pid
/// lo pone la conexión; `value == 0` → DELETE (parity QUERY_QUEST_SAVE).
async fn persist_flags(session: &mut Session, dirty: &[DirtyFlag]) -> Result<(), String> {
    let rows: Vec<QuestRow> = dirty
        .iter()
        .map(|d| QuestRow {
            dw_pid: session.row().id,
            sz_name: d.quest.clone(),
            sz_state: d.flag.clone(),
            l_value: d.value as i32,
        })
        .collect();
    let affected = QuestRepo::new(session.pool.clone()).save(&rows).await?;
    eprintln!(
        "server_realms: channel conn {}: quest flags persistidos ({} filas)",
        session.conn_id, affected
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El wire del GC_SCRIPT (45) verificado contra el cliente
    /// (RecvScriptPacket — PythonNetworkStreamPhaseGame.cpp:2247-2283):
    /// header + size(WORD = 6 + src) + skin + src_size(WORD) + markup.
    #[test]
    fn script_packet_wire_shape() {
        let pkt = script_packet("hola[ENTER]");
        assert_eq!(pkt[0], 45, "header GC_SCRIPT");
        let size = u16::from_le_bytes([pkt[1], pkt[2]]);
        assert_eq!(size as usize, 6 + "hola[ENTER]".len());
        assert_eq!(pkt[3], 0, "skin QUEST_SKIN_NORMAL");
        let src = u16::from_le_bytes([pkt[4], pkt[5]]);
        assert_eq!(src as usize, "hola[ENTER]".len());
        assert_eq!(&pkt[6..], b"hola[ENTER]");
    }

    /// ADR-0009 (server dueño del texto): las claves de diálogo se RESUELVEN
    /// contra el diccionario del engine — la clave real del corpus
    /// `gameforge.map_warp._20_sayTitle` se envía como su TEXTO; una clave
    /// sin entrada queda tal cual (fallback).
    #[test]
    fn dialog_keys_resolved_from_texts_dictionary() {
        let mut texts = std::collections::HashMap::new();
        texts.insert(
            "gameforge.map_warp._20_sayTitle".to_string(),
            "Este es el titulo del warp.".to_string(),
        );
        let e = game_core::quest::QuestEngine::load(
            "quest d\n  state start\n    on letter\n      -> say(@gameforge.map_warp._20_sayTitle)\n      -> say(@sin_texto)\n",
        )
        .expect("parse")
        .with_texts(texts);
        let mut rt = game_core::quest::QuestRuntime::default();
        let items = std::collections::HashMap::new();
        let mut rng = |_, _| 0i64;
        let out = e.run(&mut rt, game_core::quest::QuestTrigger::Letter, 5, 41, 0, &items, &mut rng);
        let script = out.script.expect("diálogo");
        assert!(script.starts_with("Este es el titulo del warp.[ENTER]"), "{script}");
        assert!(script.contains("sin_texto[ENTER]"), "fallback a la clave: {script}");
    }

    /// El markup del event-set del cliente: texto + [ENTER], [NEXT] y
    /// [QUESTION 1;k1|2;k2] (parity questlua.cpp:62-68, 901-937).
    #[test]
    fn dialog_markup_roundtrips_through_the_engine() {
        let e = game_core::quest::QuestEngine::load(
            "quest d\n  state start\n    on letter\n      -> say(@texto)\n      -> wait()\n      -> say(@mas)\n",
        )
        .expect("parse");
        let mut rt = game_core::quest::QuestRuntime::default();
        let items = std::collections::HashMap::new();
        let mut rng = |_, _| 0i64;
        let out = e.run(&mut rt, game_core::quest::QuestTrigger::Letter, 5, 41, 0, &items, &mut rng);
        let script = out.script.expect("diálogo");
        assert!(script.starts_with("texto[ENTER][NEXT]"), "{script}");
        assert!(out.suspended);
        let out = e.answer(&mut rt, 0, 5, 41, 0, &items, &mut rng);
        assert_eq!(out.script.as_deref(), Some("mas[ENTER]"), "{:?}", out.script);
    }

    /// El parseo del texto del CG_QUEST_INPUT_STRING (30): C-string de 65 B
    /// (hasta el primer NUL — parity strlcpy); malformado (< 2 B) → None.
    #[test]
    fn quest_input_string_parses_c_string() {
        // "hola\0..." en un paquete de 66 B.
        let mut pkt = vec![0u8; 66];
        pkt[0] = 30;
        pkt[1..5].copy_from_slice(b"hola");
        assert_eq!(parse_input_string(&pkt).as_deref(), Some("hola"));
        // Sin NUL: se toma todo el buffer (65 B máx — el framer lo limita).
        let mut pkt = vec![0u8; 66];
        pkt[0] = 30;
        pkt[1..].copy_from_slice(&[b'a'; 65]);
        assert_eq!(parse_input_string(&pkt).unwrap().len(), 65);
        // Malformado.
        assert_eq!(parse_input_string(&[30]), None);
        assert_eq!(parse_input_string(&[]), None);
    }

    /// El idx del CG_SCRIPT_BUTTON (66) y el answer/requestPID del
    /// CG_QUEST_CONFIRM (31) se leen LE byte-exacto (Packet.h:665-669,
    /// 1008-1013).
    #[test]
    fn quest_button_and_confirm_wire_fields() {
        // Button: header + idx DWORD LE.
        let pkt = [66, 0x78, 0x56, 0x34, 0x12];
        let idx = u32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]);
        assert_eq!(idx, 0x1234_5678);
        // Confirm: header + answer + requestPID DWORD LE.
        let pkt = [31, 1, 0x44, 0x33, 0x22, 0x11];
        assert_eq!(pkt[1], 1, "answer");
        let request_pid = u32::from_le_bytes([pkt[2], pkt[3], pkt[4], pkt[5]]);
        assert_eq!(request_pid, 0x1122_3344, "requestPID LE");
    }
}
