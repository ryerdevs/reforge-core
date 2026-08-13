//! `channel/chat.rs` — el handler del CG_CHAT (R-s3): echo GC_CHAT (4) al
//! jugador (parity `Chat()` input_main.cpp:641-685 → `ChatPacket` →
//! char.cpp) + el hook de COMANDOS: el mensaje que empieza con '/' es un
//! comando de GM (parity input_main.cpp:661-665 — `interpret_command` ANTES
//! del echo y del anti-spam; el comando no se muestra).
//!
//! CG_CHAT (3): header + length(WORD) + type + msg (el framer ya entrega
//! `length` bytes totales — el formato de TPacketCGChat Packet.h:534-539).
//! GC_CHAT: header(4) + size(WORD, incluye header 9 B) + type + dwVID +
//! bEmpire + msg (Packet.h:1336-1343; el cliente hace
//! size - sizeof(TPacketGCChat)).
//!
//! C6a (firma uniforme): malformado → log + Continue (antes cerraba).

use crate::channel::session::{Outcome, Session};
use protocol::header;

/// CG_CHAT (3): si el mensaje empieza con '/' → comando (GM — parity
/// `interpret_command`); si no → eco del mensaje al emisor con su vid +
/// empire.
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() < 4 {
        // C6a: malformado → Continue con log (antes cerraba la conexión).
        eprintln!(
            "server_realms: channel conn {}: CG_CHAT malformado ({})",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    }
    let chat_type = pkt[3];
    let msg = &pkt[4..];
    // El comando: '/' + texto (parity input_main.cpp:661-665 — `*buf == '/'`
    // → `interpret_command(ch, buf + 1, ...)`; el comando NO se muestra).
    if msg.len() > 1 && msg[0] == b'/' {
        return crate::channel::gm::handle(
            session,
            &String::from_utf8_lossy(&msg[1..]),
        )
        .await;
    }
    // GC_CHAT: header(4) + size(WORD, incluye header 9 B) + type + dwVID +
    // bEmpire + msg (Packet.h:1336-1343; el cliente hace
    // size - sizeof(TPacketGCChat)).
    let size = (9 + msg.len()) as u16;
    let mut out = Vec::with_capacity(9 + msg.len());
    out.push(header::GC_CHAT);
    out.extend_from_slice(&size.to_le_bytes());
    out.push(chat_type);
    out.extend_from_slice(&session.player_vid().to_le_bytes());
    out.push(session.empire);
    out.extend_from_slice(msg);
    session
        .send(&out)
        .await
        .map_err(|e| format!("enviando GC_CHAT: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: chat de {} (type {}): {}",
        session.conn_id,
        session.row().name,
        chat_type,
        String::from_utf8_lossy(msg)
    );
    Ok(Outcome::Continue)
}
