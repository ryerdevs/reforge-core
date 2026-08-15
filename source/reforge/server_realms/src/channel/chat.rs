//! `channel/chat.rs` — los handlers del CHAT del canal (gap-lane-C):
//! CG_CHAT (3) con BROADCAST a los jugadores en rango + CG_WHISPER (19) con
//! resolución por nombre (parity `Chat()` input_main.cpp:641-685 →
//! `ChatPacket` → char.cpp y `Whisper()` input_main.cpp:273-487).
//!
//! CG_CHAT (3): header + length(WORD) + type + msg (el framer ya entrega
//! `length` bytes totales — el formato de TPacketCGChat Packet.h:534-539).
//! El hook de COMANDOS vive ANTES del broadcast (parity input_main.cpp:661-665
//! — `interpret_command` ANTES del echo y del anti-spam; el comando no se
//! muestra ni se difunde).
//! GC_CHAT: header(4) + size(WORD, incluye header 9 B) + type + dwVID +
//! bEmpire + msg (Packet.h:1336-1343; el cliente hace
//! size - sizeof(TPacketGCChat)). El emisor recibe el echo SIEMPRE; el resto
//! de jugadores lo reciben según el tipo: TALKING → mismo mapa y distancia
//! ≤ ~1000 (el radio de visibilidad del personaje — parity del
//! `PacketToVisibleSet` del C++), SHOUT → mismo mapa sin límite; el resto de
//! tipos NO se difunden (solo el emisor los ve).
//!
//! CG_WHISPER (19): header + wSize(WORD, total) + szNameTo[25] + msg
//! (TPacketCGWhisper Packet.h:540-546 — `CHARACTER_NAME_MAX_LEN=24`).
//! Resuelve el destino por NOMBRE en el registro de sesiones activas y
//! entrega el mensaje (parity `Whisper()`):
//! - destino online → GC_WHISPER (34, bType CHAT, szNameFrom = emisor) al
//!   destinatario + CONFIRMACIÓN al emisor (echo GC_WHISPER con el nombre
//!   del destinatario — el cliente pinta "dest : msg" en su pestaña);
//! - destino inexistente → GC_WHISPER (bType NOT_EXIST, sin mensaje) al
//!   emisor (input_main.cpp:322-335);
//! - whisper a sí mismo → no-op (`pkChr == ch`, input_main.cpp:298-300);
//! - mensaje vacío → no-op (`if (buflen > 0)`, input_main.cpp:432).
//!
//! El registro de sesiones activas (`peers()`, vid → peer) es el
//! "equivalente" del map `routes` de mod.rs (ese rutea eventos del MUNDO
//! por el mpsc; el chat necesita además nombre/posición/empire de cada
//! sesión y un outbox de bytes propio — el game loop de cada conexión
//! drena su `chat_rx`). Se registra en el world join (entry.rs) y se libera
//! al cerrar la conexión (guard RAII `ChatPeerGuard`, patrón LeaveGuard).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use protocol::chat as pchat;
use protocol::header;
use tokio::sync::mpsc::UnboundedSender;

use crate::channel::session::{Outcome, Session};

/// Radio del broadcast TALKING (parity del `PacketToVisibleSet` del C++ — el
/// rango de visibilidad del personaje, ~1000).
const TALKING_RANGE: i64 = 1000;

/// Peer de chat de una sesión activa: lo que el broadcast/whisper necesitan
/// de OTRA conexión (nombre para el whisper, posición/mapa para el rango y
/// el outbox para la entrega S→C). `out` es el lado emisor del `chat_rx`
/// que el game loop de esa conexión drena. El empire del GC_CHAT es el del
/// EMISOR (se copia del session al construir el paquete — parity del C++).
#[derive(Clone)]
struct ChatPeer {
    name: String,
    map_index: i32,
    x: i32,
    y: i32,
    out: UnboundedSender<Vec<u8>>,
}

/// Registro de sesiones activas del canal para el CHAT (vid → peer). El
/// equivalente del map `routes` de mod.rs (ver el doc del módulo): una
/// sesión entrega bytes a OTRAS sesiones por su outbox.
fn peers() -> &'static Mutex<HashMap<u32, ChatPeer>> {
    static PEERS: OnceLock<Mutex<HashMap<u32, ChatPeer>>> = OnceLock::new();
    PEERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Guard RAII del peer de chat (patrón LeaveGuard): al soltar la sesión se
/// elimina del registro — un jugador desconectado no recibe chats ni puede
/// ser destino de whisper.
pub struct ChatPeerGuard(u32);

impl Drop for ChatPeerGuard {
    fn drop(&mut self) {
        peers().lock().expect("chat peers lock").remove(&self.0);
    }
}

/// Registra la sesión como peer de chat (world join — entry.rs) y devuelve
/// el guard que la desregistra al cerrar la conexión.
pub fn register_peer(
    vid: u32,
    name: String,
    map_index: i32,
    x: i32,
    y: i32,
    out: UnboundedSender<Vec<u8>>,
) -> ChatPeerGuard {
    peers().lock().expect("chat peers lock").insert(
        vid,
        ChatPeer { name, map_index, x, y, out },
    );
    ChatPeerGuard(vid)
}

/// Sincroniza la posición del peer tras un MOVE aceptado (movement.rs) — el
/// rango del broadcast usa la posición VIVA, no la del join.
pub fn update_position(vid: u32, x: i32, y: i32) {
    if let Some(p) = peers().lock().expect("chat peers lock").get_mut(&vid) {
        p.x = x;
        p.y = y;
    }
}

/// ¿El peer `o` recibe un chat de tipo `t` emitido desde (my_map, my_x,
/// my_y)? Parity del C++ (input_main.cpp:641-685): TALKING → mismo mapa +
/// distancia ≤ ~1000; SHOUT → mismo mapa sin límite de distancia; el resto
/// de tipos NO se difunden (solo el emisor los ve — echo).
fn visible_to(t: u8, my_map: i32, my_x: i32, my_y: i32, o: &ChatPeer) -> bool {
    if o.map_index != my_map {
        return false;
    }
    match t {
        pchat::TYPE_TALKING => {
            let dx = i64::from(o.x) - i64::from(my_x);
            let dy = i64::from(o.y) - i64::from(my_y);
            dx * dx + dy * dy <= TALKING_RANGE * TALKING_RANGE
        }
        pchat::TYPE_SHOUT => true,
        _ => false,
    }
}

/// CG_CHAT (3): si el mensaje empieza con '/' → comando (GM — parity
/// `interpret_command`, ANTES del broadcast); si no → eco GC_CHAT al emisor
/// + broadcast a los peers en rango (parity `PacketToVisibleSet`).
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
    // → `interpret_command(ch, buf + 1, ...)`; el comando NO se muestra NI se
    // difunde — el hook vive ANTES del broadcast).
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
    // Echo al emisor SIEMPRE (parity: el emisor está en el set de destino).
    session
        .send(&out)
        .await
        .map_err(|e| format!("enviando GC_CHAT: {e}"))?;
    // Broadcast a los peers en rango (parity del C++: Chat → difusión a los
    // jugadores visibles — TALKING ~1000 / SHOUT mapa; el resto no se difunde).
    let my_vid = session.player_vid();
    let my_map = session.row().map_index;
    let (my_x, my_y) = (session.motion().x, session.motion().y);
    let mut sent = 0usize;
    {
        let ps = peers().lock().expect("chat peers lock");
        for (vid, peer) in ps.iter() {
            if *vid == my_vid {
                continue;
            }
            if !visible_to(chat_type, my_map, my_x, my_y, peer) {
                continue;
            }
            // UnboundedSender::send es síncrono (no await) — el lock no se
            // sostiene a través de un punto de suspensión.
            if peer.out.send(out.clone()).is_ok() {
                sent += 1;
            }
        }
    }
    eprintln!(
        "server_realms: channel conn {}: chat de {} (type {}): {} — echo + {sent} en rango",
        session.conn_id,
        session.row().name,
        chat_type,
        String::from_utf8_lossy(msg)
    );
    Ok(Outcome::Continue)
}

/// CG_WHISPER (19): resuelve el destino por NOMBRE en el registro de
/// sesiones activas y entrega el mensaje (parity `Whisper()` input_main.cpp:
/// 273-487). Formato: header + wSize(WORD, total) + szNameTo[25] + msg.
pub async fn handle_whisper(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() < pchat::CG_WHISPER_FIXED {
        // C6a: malformado → Continue con log.
        eprintln!(
            "server_realms: channel conn {}: CG_WHISPER malformado ({})",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    }
    let name_bytes = &pkt[3..pchat::CG_WHISPER_FIXED];
    let end = name_bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(name_bytes.len());
    let target = String::from_utf8_lossy(&name_bytes[..end]).into_owned();
    // El mensaje: el cliente manda strlen+1 (con NUL); el C++ lo corta con
    // strlen (input_main.cpp:400-401) — el GC_WHISPER viaja SIN el NUL.
    let mut msg = &pkt[pchat::CG_WHISPER_FIXED..];
    if let Some(stripped) = msg.strip_suffix(&[0]) {
        msg = stripped;
    }
    if target.is_empty() || msg.is_empty() {
        // Sin destino o sin mensaje → no-op (parity `if (buflen > 0)`).
        return Ok(Outcome::Continue);
    }
    let my_vid = session.player_vid();
    let target_peer = {
        let ps = peers().lock().expect("chat peers lock");
        // Resolución por nombre EXACTO (parity `FindPC` del C++).
        ps.iter().find(|(_, p)| p.name == target).map(|(vid, p)| (*vid, p.clone()))
    };
    let Some(target_peer) = target_peer else {
        // Destino inexistente → GC_WHISPER NOT_EXIST al emisor (parity
        // input_main.cpp:322-335 — sin mensaje, con el nombre buscado).
        let mut out = vec![header::GC_WHISPER];
        out.extend_from_slice(&(pchat::GC_WHISPER_FIXED as u16).to_le_bytes());
        out.push(pchat::WHISPER_NOT_EXIST);
        let mut name = [0u8; pchat::NAME_BYTES];
        let n = target.len().min(pchat::NAME_BYTES - 1);
        name[..n].copy_from_slice(&target.as_bytes()[..n]);
        out.extend_from_slice(&name);
        session
            .send(&out)
            .await
            .map_err(|e| format!("enviando GC_WHISPER (NOT_EXIST): {e}"))?;
        eprintln!(
            "server_realms: channel conn {}: whisper de {} a {target}: destino inexistente",
            session.conn_id, session.row().name
        );
        return Ok(Outcome::Continue);
    };
    // Whisper a sí mismo → no-op (parity `if (pkChr == ch) return;`).
    if target_peer.0 == my_vid {
        return Ok(Outcome::Continue);
    }
    // GC_WHISPER: header(34) + wSize(WORD, incluye 29 B) + bType + szNameFrom[25]
    // + msg (TPacketGCWhisper Packet.h:1346-1351 — sin NUL al final).
    let mut to_recipient = vec![header::GC_WHISPER];
    to_recipient
        .extend_from_slice(&((pchat::GC_WHISPER_FIXED + msg.len()) as u16).to_le_bytes());
    to_recipient.push(pchat::WHISPER_CHAT);
    let sender = session.row().name.clone();
    let mut from_name = [0u8; pchat::NAME_BYTES];
    let n = sender.len().min(pchat::NAME_BYTES - 1);
    from_name[..n].copy_from_slice(&sender.as_bytes()[..n]);
    to_recipient.extend_from_slice(&from_name);
    to_recipient.extend_from_slice(msg);
    // Confirmación al emisor: echo con el nombre del DESTINATARIO (el
    // cliente pinta "dest : msg" en su pestaña de whisper).
    let mut to_sender = to_recipient.clone();
    let n = target.len().min(pchat::NAME_BYTES - 1);
    to_sender[4..4 + n].copy_from_slice(&target.as_bytes()[..n]);
    to_sender[4 + n..pchat::GC_WHISPER_FIXED].fill(0);
    // Entrega al destinatario (outbox — el game loop de esa conexión drena).
    if target_peer.1.out.send(to_recipient).is_err() {
        eprintln!(
            "server_realms: channel conn {}: whisper a {target}: outbox cerrado",
            session.conn_id
        );
    }
    session
        .send(&to_sender)
        .await
        .map_err(|e| format!("enviando GC_WHISPER (echo): {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: whisper de {} a {target}: {}",
        session.conn_id,
        session.row().name,
        String::from_utf8_lossy(msg)
    );
    Ok(Outcome::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use database::player::PlayerRow;
    use tokio::io::AsyncReadExt;

    /// Fila mínima del player para los handlers (solo los campos que el chat
    /// lee: id/name/map_index/posición).
    fn dummy_row(name: &str, map_index: i32, x: i32, y: i32) -> PlayerRow {
        PlayerRow {
            id: 1,
            name: name.into(),
            job: 0,
            voice: 0,
            dir: 0,
            x,
            y,
            z: 0,
            map_index,
            exit_x: 0,
            exit_y: 0,
            exit_map_index: 0,
            hp: 100,
            mp: 100,
            stamina: 0,
            random_hp: 0,
            random_sp: 0,
            playtime: 0,
            gold: 0,
            level: 1,
            level_step: 0,
            st: 0,
            ht: 0,
            dx: 0,
            iq: 0,
            exp: 0,
            stat_point: 0,
            skill_point: 0,
            sub_skill_point: 0,
            stat_reset_count: 0,
            part_base: 0,
            part_hair: 0,
            part_main: 0,
            skill_level: None,
            quickslot: None,
            skill_group: 0,
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

    /// Sesión de test: par de sockets localhost (el lado `client_side` lee
    /// lo que la sesión envía — `Session::new` exige `TcpStream` concreto),
    /// pool sin conectar (chat no toca PG) y el peer registrado con la
    /// posición dada.
    async fn test_session(
        vid: u32,
        name: &str,
        map_index: i32,
        x: i32,
        y: i32,
    ) -> (Session, tokio::net::TcpStream) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind localhost");
        let addr = listener.local_addr().expect("addr");
        let client_side = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (server_side, _peer) = listener.accept().await.expect("accept");
        let pool = database::pool::new_pool("host=localhost dbname=metin2", 2)
            .expect("pool sin conectar (lazy)");
        let wal_dir = std::env::temp_dir()
            .join(format!("chat_test_wal_{}", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let batcher = std::sync::Arc::new(database::wal::Batcher::spawn(
            Duration::from_millis(100),
            64,
            database::wal::WalSink::new(database::wal::PgMutationSink::new(pool.clone()), wal_dir),
        ));
        let mut cfg = crate::config::Config::default();
        cfg.timeout = Duration::from_secs(5);
        let (intent_tx, _intent_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut s = Session::new(
            server_side,
            cfg,
            vid,
            intent_tx,
            std::sync::Arc::new(std::sync::Mutex::new(game_core::map::MapStore::new())),
            pool,
            batcher,
        );
        s.empire = 1;
        s.row = Some(dummy_row(name, map_index, x, y));
        s.motion = Some(game_core::movement::initial(x, y));
        s.chat_guard = Some(register_peer(
            vid,
            name.to_string(),
            map_index,
            x,
            y,
            s.chat_tx.clone(),
        ));
        (s, client_side)
    }

    /// Lee UN paquete S→C del lado cliente del socket (header + size WORD +
    /// resto — GC_CHAT y GC_WHISPER comparten el layout size-prefixed).
    async fn read_packet(sock: &mut tokio::net::TcpStream) -> Vec<u8> {
        let mut hdr = [0u8; 3];
        sock.read_exact(&mut hdr).await.expect("paquete del server");
        let size = u16::from_le_bytes([hdr[1], hdr[2]]) as usize;
        let mut body = vec![0u8; size - 3];
        sock.read_exact(&mut body).await.expect("cuerpo del paquete");
        let mut pkt = hdr.to_vec();
        pkt.extend_from_slice(&body);
        pkt
    }

    /// Broadcast TALKING: el emisor recibe el echo; el peer EN RANGO recibe
    /// los mismos bytes por su outbox; el peer LEJANO no recibe nada.
    /// (Vids/nombres ÚNICOS por test — el registro de peers es un static
    /// compartido y los tests corren en paralelo.)
    #[tokio::test]
    async fn chat_broadcast_reaches_in_range_and_skips_far() {
        // Mapa 10 EXCLUSIVO del test (el registro de peers es un static
        // compartido y los tests corren en paralelo — mapas distintos aíslan
        // el rango del broadcast).
        let (mut a, mut a_sock) = test_session(101, "Alice", 10, 100, 100).await;
        let (mut b, _b_sock) = test_session(102, "Bob", 10, 300, 400).await; // a ~360 (< 1000)
        let (mut c, _c_sock) = test_session(103, "Carol", 10, 9000, 9000).await; // lejos (> 1000)
        // CG_CHAT TALKING "hola": length = 4 + 5 = 9 (parity del framer).
        let mut pkt = vec![header::CG_CHAT];
        pkt.extend_from_slice(&9u16.to_le_bytes());
        pkt.push(pchat::TYPE_TALKING);
        pkt.extend_from_slice(b"hola\0");
        let _ = handle(&mut a, &pkt).await.expect("chat OK");
        // Echo al emisor (GC_CHAT byte-exacto).
        let echo = read_packet(&mut a_sock).await;
        assert_eq!(echo[0], header::GC_CHAT, "GC_CHAT del echo");
        assert_eq!(u16::from_le_bytes([echo[1], echo[2]]) as usize, echo.len());
        assert_eq!(&echo[9..], b"hola\0", "mensaje intacto");
        // Bob (en rango) recibe EXACTAMENTE los mismos bytes por su outbox.
        let to_bob = tokio::time::timeout(Duration::from_secs(2), b.chat_rx.recv())
            .await
            .expect("Bob recibe el broadcast en 2 s")
            .expect("outbox de Bob abierto");
        assert_eq!(to_bob, echo, "mismos bytes que el echo (vid/empire del emisor)");
        // Carol (lejana) NO recibe nada.
        assert!(
            tokio::time::timeout(Duration::from_millis(100), c.chat_rx.recv())
                .await
                .is_err(),
            "Carol está fuera de rango (9000,9000)"
        );
    }

    /// El hook de comandos ('/' → gm) vive ANTES del broadcast: el comando no
    /// se difunde a los peers (parity interpret_command).
    #[tokio::test]
    async fn chat_command_hook_before_broadcast() {
        // Mapa 20 exclusivo del test (aislamiento entre tests en paralelo).
        let (mut a, _a_sock) = test_session(201, "Cmdr", 20, 100, 100).await;
        let (mut b, _b_sock) = test_session(202, "Peer", 20, 300, 400).await;
        // CG_CHAT "/restart_here" (comando GM_PLAYER — sin DB; vivo → el C++
        // lo ignora sin eco). length = 4 + 14 = 18.
        let mut pkt = vec![header::CG_CHAT];
        pkt.extend_from_slice(&18u16.to_le_bytes());
        pkt.push(pchat::TYPE_TALKING);
        pkt.extend_from_slice(b"/restart_here\0");
        let _ = handle(&mut a, &pkt).await.expect("comando OK");
        assert!(
            tokio::time::timeout(Duration::from_millis(100), b.chat_rx.recv())
                .await
                .is_err(),
            "el comando NO se difunde"
        );
    }

    /// Whisper entre 2: el destinatario recibe GC_WHISPER (bType CHAT) con el
    /// nombre del EMISOR y el emisor recibe la confirmación (echo con el
    /// nombre del DESTINATARIO). El mensaje viaja SIN el NUL (parity strlen).
    #[tokio::test]
    async fn whisper_delivers_to_recipient_and_confirms_sender() {
        // Mapas exclusivos del test (31/32 — el whisper es global, sin rango).
        let (mut a, mut a_sock) = test_session(301, "Whisperer", 31, 100, 100).await;
        let (mut b, _b_sock) = test_session(302, "Target", 32, 500, 500).await; // otro mapa — el whisper es global
        // CG_WHISPER a "Target": wSize = 28 + 9 = 37 ("hola bob\0").
        let mut pkt = vec![header::CG_WHISPER];
        pkt.extend_from_slice(&37u16.to_le_bytes());
        let mut name = [0u8; pchat::NAME_BYTES];
        name[..6].copy_from_slice(b"Target");
        pkt.extend_from_slice(&name);
        pkt.extend_from_slice(b"hola bob\0");
        let _ = handle_whisper(&mut a, &pkt).await.expect("whisper OK");
        // El destinatario lo recibe por su outbox: GC_WHISPER con el emisor.
        let to_bob = tokio::time::timeout(Duration::from_secs(2), b.chat_rx.recv())
            .await
            .expect("Target recibe el whisper en 2 s")
            .expect("outbox de Target abierto");
        assert_eq!(to_bob[0], header::GC_WHISPER);
        assert_eq!(to_bob[3], pchat::WHISPER_CHAT, "bType normal");
        let from = String::from_utf8_lossy(&to_bob[4..29])
            .trim_end_matches('\0')
            .to_string();
        assert_eq!(from, "Whisperer", "szNameFrom = el emisor");
        assert_eq!(&to_bob[29..], b"hola bob", "mensaje sin NUL (parity strlen)");
        // La confirmación al emisor: echo GC_WHISPER con el nombre del destino.
        let echo = read_packet(&mut a_sock).await;
        assert_eq!(echo[0], header::GC_WHISPER);
        assert_eq!(echo[3], pchat::WHISPER_CHAT);
        let to = String::from_utf8_lossy(&echo[4..29])
            .trim_end_matches('\0')
            .to_string();
        assert_eq!(to, "Target", "szNameFrom = el destinatario (confirmación)");
        assert_eq!(&echo[29..], b"hola bob");
    }

    /// Whisper a un destino inexistente: el emisor recibe GC_WHISPER
    /// (bType NOT_EXIST) con el nombre buscado y SIN mensaje (parity).
    #[tokio::test]
    async fn whisper_to_nonexistent_notifies_sender() {
        // Mapa 40 exclusivo del test.
        let (mut a, mut a_sock) = test_session(401, "Solo", 40, 100, 100).await;
        // CG_WHISPER a "Ghost" (no registrado): wSize = 28 + 5 = 33 ("hola\0").
        let mut pkt = vec![header::CG_WHISPER];
        pkt.extend_from_slice(&33u16.to_le_bytes());
        let mut name = [0u8; pchat::NAME_BYTES];
        name[..5].copy_from_slice(b"Ghost");
        pkt.extend_from_slice(&name);
        pkt.extend_from_slice(b"hola\0");
        let _ = handle_whisper(&mut a, &pkt).await.expect("whisper OK");
        let reply = read_packet(&mut a_sock).await;
        assert_eq!(reply[0], header::GC_WHISPER);
        assert_eq!(reply[3], pchat::WHISPER_NOT_EXIST, "bType NOT_EXIST");
        let who = String::from_utf8_lossy(&reply[4..29])
            .trim_end_matches('\0')
            .to_string();
        assert_eq!(who, "Ghost", "el nombre buscado");
        assert_eq!(reply.len(), pchat::GC_WHISPER_FIXED, "sin mensaje");
    }
}
