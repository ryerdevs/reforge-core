//! `channel/chat.rs` — los handlers del CHAT del canal (gap-lane-C):
//! CG_CHAT (3) con BROADCAST a los jugadores en rango + CG_WHISPER (19) con
//! resolución por nombre (parity `Chat()` input_main.cpp:641-685 →
//! `ChatPacket` → char.cpp y `Whisper()` input_main.cpp:273-487).
//!
//! CG_CHAT (3): header + length(WORD) + type + msg (el framer ya entrega
//! `length` bytes totales — el formato de TPacketCGChat Packet.h:534-539).
//! El texto se recorta en el PRIMER NUL (el cliente manda strlen+1 — parity
//! strlcpy+strlen input_main.cpp:657-658: sin el recorte "warp 100 200\0" ≠
//! "warp 100 200" → TODO comando '/' responde "No such command", C-02) y se
//! capa a 485 B (C-01 — el cliente legacy hace `Recv(uChatSize)` sobre
//! `char buf[1025]` sin bound-check, PythonNetworkStreamPhaseGame.cpp:
//! 1290-1301; el cap evita el wrap del size u16).
//! El hook de COMANDOS vive ANTES del broadcast (parity input_main.cpp:661-665
//! — `interpret_command` ANTES del echo y del anti-spam; el comando no se
//! muestra ni se difunde).
//! GC_CHAT: header(4) + size(WORD, incluye header 9 B) + type + dwVID +
//! bEmpire + payload (Packet.h:1336-1343; el cliente hace
//! size - sizeof(TPacketGCChat) y pinta el payload verbatim — AppendChat,
//! PythonChat.cpp:436+). El payload es "Name : msg" SIN NUL (parity
//! `snprintf("%s : %s")` input_main.cpp:723-725 + `Packet(msg, len)` — C-05:
//! sin el nombre la ventana no muestra quién habla y los ':' rompen el
//! strip del cliente). El emisor recibe el echo SIEMPRE; el resto lo reciben
//! según el tipo: TALKING → mismo mapa y distancia ≤ rango del view 5500
//! (el C++ difunde a TODO el mapa SIN filtro de distancia server-side —
//! FEmpireChatPacket filtra solo por map_index, input_main.cpp:763-780; el
//! recorte real es client-side, el vid no spawneado se descarta, con spawn
//! range VIEW_RANGE 5000 + 500, config.cpp:104-105 — divergencia
//! documentada: cull por el rango del view, mismo resultado visible y menos
//! fanout), SHOUT → canal COMPLETO (todos los mapas) con id=0 y cooldown
//! 15 s (parity SendShout/FuncShout input_p2p.cpp:208-228 + ChatPacket
//! char.cpp:3947); el resto de tipos NO se difunden (solo el emisor los ve).
//!
//! CG_WHISPER (19): header + wSize(WORD, total) + szNameTo[25] + msg
//! (TPacketCGWhisper Packet.h:540-546 — `CHARACTER_NAME_MAX_LEN=24`).
//! El mensaje se recorta en el primer NUL y se capa a 512 B (parity
//! strlcpy+strlen input_main.cpp:398-401 — el payload del GC_WHISPER no
//! puede desbordar `buf[513]` del cliente, PythonNetworkStreamPhaseGame.cpp:
//! 1422-1429). Resuelve el destino por NOMBRE CASE-INSENSITIVE (parity
//! `FindPC` char_manager.cpp:209-223 — str_lower + mapa keyed por
//! minúsculas) en el registro de sesiones activas y entrega el mensaje
//! (parity `Whisper()`):
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

/// Rango del broadcast TALKING = VIEW_RANGE 5000 + VIEW_BONUS_RANGE 500
/// (config.cpp:104-105) — el radio de spawn/visibilidad del personaje. El
/// C++ NO filtra distancia server-side: difunde a TODO el mapa
/// (FEmpireChatPacket filtra SOLO por map_index, input_main.cpp:763-780; el
/// recorte real es client-side, el vid no spawneado se descarta). Divergencia
/// documentada (C-03): cull server-side por el rango del view — el resultado
/// visible es el MISMO y el fanout menor.
const TALKING_RANGE: i64 = 5000 + 500;

/// Cap del texto del chat: `CHAT_MAX_LEN 512 - (CHARACTER_NAME_MAX_LEN 24 +
/// 3) + 1` = 486 B de buffer → 485 chars (parity `char buf[...]` + strlcpy,
/// input_main.cpp:657-658). C-01: sin el cap, un msg de 65531 B (wSize
/// 65535) wrappea el size u16 a 4 → el cliente legacy hace `Recv(uChatSize)`
/// sobre `char buf[1025]` sin bound-check (PythonNetworkStreamPhaseGame.cpp:
/// 1290-1301) → stack overflow.
const CHAT_MSG_MAX: usize = 485;

/// Cap del mensaje del whisper: buf `[CHAT_MAX_LEN + 1]` = 513 → 512 chars
/// (parity strlcpy input_main.cpp:398-399 — el payload del GC_WHISPER no
/// puede desbordar `buf[513]` del cliente, PythonNetworkStreamPhaseGame.cpp:
/// 1422-1429, assert solo debug).
const WHISPER_MSG_MAX: usize = 512;

/// Cap del payload del GC_CHAT "Name : msg" (parity `chatbuf[CHAT_MAX_LEN +
/// 1]` — input_main.cpp:720-729: `len >= sizeof(chatbuf)` → 512).
const CHATBUF_MAX: usize = 512;

/// Cooldown del SHOUT: 15 s (parity input_main.cpp:743-748 —
/// `pulse - lastShoutPulse < passes_per_sec * 15` → return silencioso).
const SHOUT_COOLDOWN: tokio::time::Duration = tokio::time::Duration::from_secs(15);

/// Peer de chat de una sesión activa: lo que el broadcast/whisper necesitan
/// de OTRA conexión (nombre para el whisper, posición/mapa para el rango,
/// imperio para los GC_CHAT dirigidos y el outbox para la entrega S→C).
/// `out` es el lado emisor del `chat_rx` que el game loop de esa conexión
/// drena. El empire del GC_CHAT es el del EMISOR (se copia del session al
/// construir el paquete — parity del C++).
#[derive(Clone)]
struct ChatPeer {
    name: String,
    map_index: i32,
    x: i32,
    y: i32,
    /// Imperio de la sesión (bEmpire del GC_CHAT dirigido a este peer —
    /// parity ChatPacket char.cpp:3948, `d->GetEmpire()` = receptor). Lo
    /// usan los lanes sociales (messenger) que construyen GC_CHAT para OTRA
    /// sesión.
    empire: u8,
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
    empire: u8,
    out: UnboundedSender<Vec<u8>>,
) -> ChatPeerGuard {
    peers().lock().expect("chat peers lock").insert(
        vid,
        ChatPeer {
            name,
            map_index,
            x,
            y,
            empire,
            out,
        },
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

/// Busca un jugador CONECTADO por nombre (case-insensitive — parity
/// `FindPC` char_manager.cpp:209-223: str_lower + mapa keyed por
/// minúsculas; misma resolución que el whisper). Devuelve
/// (vid, map_index, x, y) — lo usa el `/goto <nombre>` de GM (lote 3): el
/// registro de sesiones activas es el "nombre → vid/posición" del canal.
pub fn find_player(name: &str) -> Option<(u32, i32, i32, i32)> {
    let ps = peers().lock().expect("chat peers lock");
    ps.iter()
        .find(|(_, p)| p.name.eq_ignore_ascii_case(name))
        .map(|(vid, p)| (*vid, p.map_index, p.x, p.y))
}

/// Nombre del peer con el vid dado (None = no conectado). Lo usa el
/// messenger para resolver el destino de un ADD_BY_VID (parity
/// `CHARACTER_MANAGER::Find(vid)` input_main.cpp:941).
pub(crate) fn peer_name(vid: u32) -> Option<String> {
    peers()
        .lock()
        .expect("chat peers lock")
        .get(&vid)
        .map(|p| p.name.clone())
}

/// Imperio de la sesión con el vid dado (`d->GetEmpire()` — bEmpire del
/// GC_CHAT dirigido; 0 si el peer no existe). Messenger.
pub(crate) fn peer_empire(vid: u32) -> u8 {
    peers()
        .lock()
        .expect("chat peers lock")
        .get(&vid)
        .map(|p| p.empire)
        .unwrap_or(0)
}

/// Entrega bytes al outbox del peer `vid` (true si el peer existe y su cola
/// sigue viva). El game loop de ESA conexión los drena y los manda al
/// socket. Lo usan los lanes sociales que construyen paquetes S→C para OTRA
/// sesión (messenger: prompt messenger_auth, INFO al invitador, sync
/// REMOVE_FRIEND) — mismo camino de entrega que el whisper/broadcast.
pub(crate) fn send_to_vid(vid: u32, bytes: &[u8]) -> bool {
    match peers().lock().expect("chat peers lock").get(&vid) {
        Some(p) => p.out.send(bytes.to_vec()).is_ok(),
        None => false,
    }
}

/// Broadcast a los peers EN RANGO del view (mismo mapa + distancia ≤ 5500),
/// EXCLUYENDO al emisor `my_vid` — el patrón PacketAround del C++ para las
/// emociones (entity.cpp:73-92 incluye AL EMISOR vía f(this); aquí el echo
/// al emisor lo hace su propio socket directo). Devuelve cuántos peers lo
/// recibieron. Lo usa emotions.rs (GC_CHAT CHAT_TYPE_COMMAND).
pub(crate) fn broadcast_in_range(
    my_vid: u32,
    my_map: i32,
    my_x: i32,
    my_y: i32,
    bytes: &[u8],
) -> usize {
    let ps = peers().lock().expect("chat peers lock");
    let mut sent = 0usize;
    for (vid, peer) in ps.iter() {
        if *vid == my_vid || peer.map_index != my_map {
            continue;
        }
        let dx = i64::from(peer.x) - i64::from(my_x);
        let dy = i64::from(peer.y) - i64::from(my_y);
        if dx * dx + dy * dy > TALKING_RANGE * TALKING_RANGE {
            continue;
        }
        if peer.out.send(bytes.to_vec()).is_ok() {
            sent += 1;
        }
    }
    sent
}

/// Recorta el texto del chat/whisper en el PRIMER NUL y lo capa (parity
/// strlcpy + strlen del C++ — el cliente manda strlen+1 y el C++ corta en el
/// NUL ANTES de parsear/difundir; C-02: sin el recorte, "warp 100 200\0" ≠
/// "warp 100 200" → TODO comando '/' responde "No such command").
fn chat_text(pkt: &[u8], cap: usize) -> &[u8] {
    let end = pkt.iter().position(|&b| b == 0).unwrap_or(pkt.len());
    &pkt[..end.min(cap)]
}

/// Payload del GC_CHAT: "Name : msg" (parity `snprintf("%s : %s")` —
/// input_main.cpp:723-725; el size del paquete usa strlen SIN el NUL — el
/// cliente pinta el payload verbatim, AppendChat PythonChat.cpp:436+). Cap
/// a 512 B (parity `chatbuf[CHAT_MAX_LEN + 1]`, input_main.cpp:720-729).
fn chat_payload(name: &str, msg: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(name.len() + 3 + msg.len());
    payload.extend_from_slice(name.as_bytes());
    payload.extend_from_slice(b" : ");
    payload.extend_from_slice(msg);
    payload.truncate(CHATBUF_MAX);
    payload
}

/// ¿El peer `o` recibe un chat de tipo `t` emitido desde (my_map, my_x,
/// my_y)? Parity del C++ (input_main.cpp:641-685): TALKING → mismo mapa +
/// distancia ≤ rango del view 5500 (el C++ NO filtra distancia server-side;
/// el recorte client-side es por spawn — ver TALKING_RANGE); SHOUT NO pasa
/// por aquí (canal completo, rama propia en handle — C-04); el resto de
/// tipos NO se difunden (solo el emisor los ve — echo).
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
    // C-01/C-02: recorte en el primer NUL + cap a 485 (parity strlcpy+strlen,
    // input_main.cpp:657-658 — el cliente manda strlen+1; sin el recorte el
    // NUL rompe el parseo del comando y sin el cap el size u16 wrappea).
    let msg = chat_text(&pkt[4..], CHAT_MSG_MAX);
    // El comando: '/' + texto (parity input_main.cpp:661-665 — `*buf == '/'`
    // → `interpret_command(ch, buf + 1, ...)`; el comando NO se muestra NI se
    // difunde — el hook vive ANTES del broadcast).
    if msg.len() > 1 && msg[0] == b'/' {
        let text = String::from_utf8_lossy(&msg[1..]).into_owned();
        // Comandos de JUGADOR sin nivel GM del cmd_info[] (parity: en el C++
        // TODOS entran por interpret_command — la tabla cmd_info[] mezcla GM
        // y jugador): messenger_auth (do_messenger_auth cmd_general.cpp:
        // 1167-1189), emotion_allow y las emociones (cmd.cpp:448-473 →
        // do_emotion/do_emotion_allow). Los nombres NO colisionan con el
        // subset GM → dispatch ANTES de gm::handle (la misma tabla en el C++;
        // aquí el orden equivalente). None = comando social ajeno → cae al GM.
        if let Some(outcome) = crate::channel::messenger::try_handle_command(session, &text).await?
        {
            return Ok(outcome);
        }
        if let Some(outcome) = crate::channel::emotions::try_handle_command(session, &text).await? {
            return Ok(outcome);
        }
        return crate::channel::gm::handle(session, &text).await;
    }
    // SHOUT (C-04): canal COMPLETO (todos los mapas) con id=0 y payload
    // "Name : msg" (parity SendShout/FuncShout — input_p2p.cpp:208-228, sin
    // filtro de mapa — + ChatPacket con id=0, char.cpp:3947; el emisor
    // también lo recibe: GetClientSet incluye al emisor). Cooldown 15 s por
    // sesión (parity input_main.cpp:743-748 — return silencioso). Gate de
    // imperio del FuncShout (GM_PLAYER de otro imperio no recibe) NO
    // implementado: el peer no tiene empire/GM level — divergencia
    // documentada.
    if chat_type == pchat::TYPE_SHOUT {
        if session
            .last_shout
            .is_some_and(|t| t.elapsed() < SHOUT_COOLDOWN)
        {
            return Ok(Outcome::Continue);
        }
        session.last_shout = Some(tokio::time::Instant::now());
        let payload = chat_payload(&session.row().name, msg);
        let size = (9 + payload.len()) as u16;
        let mut out = Vec::with_capacity(9 + payload.len());
        out.push(header::GC_CHAT);
        out.extend_from_slice(&size.to_le_bytes());
        out.push(chat_type);
        out.extend_from_slice(&0u32.to_le_bytes()); // id=0 — parity ChatPacket
        out.push(session.empire);
        out.extend_from_slice(&payload);
        let mut sent = 0usize;
        {
            let ps = peers().lock().expect("chat peers lock");
            for peer in ps.values() {
                // UnboundedSender::send es síncrono (no await) — el lock no
                // se sostiene a través de un punto de suspensión.
                if peer.out.send(out.clone()).is_ok() {
                    sent += 1;
                }
            }
        }
        eprintln!(
            "server_realms: channel conn {}: SHOUT de {} (type {}): {} — {sent} en el canal",
            session.conn_id,
            session.row().name,
            chat_type,
            String::from_utf8_lossy(msg)
        );
        return Ok(Outcome::Continue);
    }
    // GC_CHAT: header(4) + size(WORD, incluye header 9 B) + type + dwVID +
    // bEmpire + payload "Name : msg" SIN NUL (Packet.h:1336-1343 — el
    // cliente pinta el payload verbatim; parity input_main.cpp:723-725,
    // C-05).
    let payload = chat_payload(&session.row().name, msg);
    let size = (9 + payload.len()) as u16;
    let mut out = Vec::with_capacity(9 + payload.len());
    out.push(header::GC_CHAT);
    out.extend_from_slice(&size.to_le_bytes());
    out.push(chat_type);
    out.extend_from_slice(&session.player_vid().to_le_bytes());
    out.push(session.empire);
    out.extend_from_slice(&payload);
    // Echo al emisor SIEMPRE (parity: el emisor está en el set de destino).
    session
        .send(&out)
        .await
        .map_err(|e| format!("enviando GC_CHAT: {e}"))?;
    // Broadcast a los peers en rango (parity del C++: Chat → difusión a
    // TODO el mapa sin filtro de distancia; aquí el cull es por el rango del
    // view 5500 — TALKING; el resto de tipos no se difunde — el SHOUT ya
    // volvió por su rama).
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
    // C-01: recorte en el primer NUL + cap a 512 (parity strlcpy+strlen,
    // input_main.cpp:398-401 — el GC_WHISPER viaja SIN el NUL; el cap evita
    // desbordar buf[513] del cliente).
    let msg = chat_text(&pkt[pchat::CG_WHISPER_FIXED..], WHISPER_MSG_MAX);
    if target.is_empty() || msg.is_empty() {
        // Sin destino o sin mensaje → no-op (parity `if (buflen > 0)`).
        return Ok(Outcome::Continue);
    }
    let my_vid = session.player_vid();
    let target_peer = {
        let ps = peers().lock().expect("chat peers lock");
        // Resolución por nombre CASE-INSENSITIVE (parity `FindPC` del C++ —
        // char_manager.cpp:209-223: str_lower + mapa keyed por minúsculas;
        // C-06: "target" llega a "Target").
        ps.iter()
            .find(|(_, p)| p.name.eq_ignore_ascii_case(&target))
            .map(|(vid, p)| (*vid, p.clone()))
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
            session.conn_id,
            session.row().name
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
    to_recipient.extend_from_slice(&((pchat::GC_WHISPER_FIXED + msg.len()) as u16).to_le_bytes());
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
// TEST_LOCK serializa tests que comparten statics de canal: el guard de
// std::Mutex viaja a través de los .await de los tests A PROPÓSITO.
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use std::time::Duration;

    use database::player::PlayerRow;
    use tokio::io::AsyncReadExt;

    /// Serializa los tests ASYNC de chat: el registro de peers es un static
    /// COMPARTIDO y el test del SHOUT difunde al canal COMPLETO (todos los
    /// mapas, C-04) — sin el lock, el shout contamina los outbox de los
    /// peers registrados por OTROS tests que corren en paralelo.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

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
        let cfg = crate::config::Config {
            timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let (intent_tx, _intent_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut s = Session::new(
            server_side,
            cfg,
            vid,
            intent_tx,
            std::sync::Arc::new(std::sync::Mutex::new(game_core::map::MapStore::new())),
            pool,
            batcher,
            std::sync::Arc::new(database::attr::AttrTables::default()),
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
            s.empire,
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
        sock.read_exact(&mut body)
            .await
            .expect("cuerpo del paquete");
        let mut pkt = hdr.to_vec();
        pkt.extend_from_slice(&body);
        pkt
    }

    /// Broadcast TALKING: el emisor recibe el echo; los peers EN RANGO del
    /// view (5500) reciben los mismos bytes por su outbox; el peer FUERA del
    /// view no recibe nada. (Vids/nombres ÚNICOS por test — el registro de
    /// peers es un static compartido y los tests corren en paralelo.)
    #[tokio::test]
    async fn chat_broadcast_reaches_in_range_and_skips_far() {
        let _guard = TEST_LOCK.lock().expect("test lock");
        // Mapa 10 EXCLUSIVO del test (el registro de peers es un static
        // compartido y los tests corren en paralelo — mapas distintos aíslan
        // el rango del broadcast).
        let (mut a, mut a_sock) = test_session(101, "Alice", 10, 100, 100).await;
        let (mut b, _b_sock) = test_session(102, "Bob", 10, 300, 400).await; // a ~360 (< 5500)
        let (mut c, _c_sock) = test_session(103, "Carol", 10, 9000, 9000).await; // ~12586 (> 5500 — fuera del view)
        let (mut d, _d_sock) = test_session(104, "Dave", 10, 3000, 3000).await; // ~4101 (< 5500, > 1000 — C-03: el rango es el del view)
        // CG_CHAT TALKING "hola": length = 4 + 5 = 9 (parity del framer).
        let mut pkt = vec![header::CG_CHAT];
        pkt.extend_from_slice(&9u16.to_le_bytes());
        pkt.push(pchat::TYPE_TALKING);
        pkt.extend_from_slice(b"hola\0");
        let _ = handle(&mut a, &pkt).await.expect("chat OK");
        // Echo al emisor (GC_CHAT byte-exacto — payload "Name : msg" SIN
        // NUL, C-05: parity input_main.cpp:723-725).
        let echo = read_packet(&mut a_sock).await;
        assert_eq!(echo[0], header::GC_CHAT, "GC_CHAT del echo");
        assert_eq!(u16::from_le_bytes([echo[1], echo[2]]) as usize, echo.len());
        assert_eq!(&echo[9..], b"Alice : hola", "payload 'Name : msg' sin NUL");
        // Bob (en rango) recibe EXACTAMENTE los mismos bytes por su outbox.
        let to_bob = tokio::time::timeout(Duration::from_secs(2), b.chat_rx.recv())
            .await
            .expect("Bob recibe el broadcast en 2 s")
            .expect("outbox de Bob abierto");
        assert_eq!(
            to_bob, echo,
            "mismos bytes que el echo (vid/empire del emisor)"
        );
        // Dave (~4101: fuera del viejo rango 1000, dentro del view 5500)
        // TAMBIÉN lo recibe — C-03: el broadcast cubre el rango del view.
        let to_dave = tokio::time::timeout(Duration::from_secs(2), d.chat_rx.recv())
            .await
            .expect("Dave recibe el broadcast en 2 s")
            .expect("outbox de Dave abierto");
        assert_eq!(to_dave, echo, "mismos bytes que el echo");
        // Carol (fuera del view) NO recibe nada.
        assert!(
            tokio::time::timeout(Duration::from_millis(100), c.chat_rx.recv())
                .await
                .is_err(),
            "Carol está fuera del view (9000,9000)"
        );
    }

    /// El hook de comandos ('/' → gm) vive ANTES del broadcast: el comando no
    /// se difunde a los peers (parity interpret_command). El texto lleva el
    /// NUL de cola del cliente (SendChatPacket manda strlen+1) — el hook lo
    /// recorta (C-02).
    #[tokio::test]
    async fn chat_command_hook_before_broadcast() {
        let _guard = TEST_LOCK.lock().expect("test lock");
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

    /// C-02 (unidad): el NUL de cola del cliente (SendChatPacket manda
    /// strlen+1) se recorta ANTES del parseo (parity strlcpy+strlen
    /// input_main.cpp:657-665) — "/warp 100 200\0" ejecuta el comando (no
    /// "No such command": parse_command("warp 100 200\0") → None).
    #[test]
    fn chat_text_strips_trailing_nul_before_command_parse() {
        let stripped = chat_text(b"/warp 100 200\0", CHAT_MSG_MAX);
        assert_eq!(stripped, b"/warp 100 200");
        let cmd = game_core::gm::parse_command(std::str::from_utf8(&stripped[1..]).unwrap());
        assert_eq!(
            cmd,
            Some(game_core::gm::GmCommand::Warp { x: 100, y: 200 }),
            "sin el NUL el comando existe"
        );
    }

    /// C-02 (integración): "/set_walk_mode\0" (con NUL) ejecuta el comando
    /// GM_PLAYER → GC_WALK_MODE (header 111 packet.h:212 + vid + modo), NO
    /// el INFO "No such command".
    #[tokio::test]
    async fn chat_command_with_trailing_nul_executes() {
        let _guard = TEST_LOCK.lock().expect("test lock");
        let (mut a, mut a_sock) = test_session(211, "Cmdr2", 21, 100, 100).await;
        // CG_CHAT "/set_walk_mode\0": length = 4 + 15 = 19.
        let mut pkt = vec![header::CG_CHAT];
        pkt.extend_from_slice(&19u16.to_le_bytes());
        pkt.push(pchat::TYPE_TALKING);
        pkt.extend_from_slice(b"/set_walk_mode\0");
        let _ = handle(&mut a, &pkt).await.expect("comando OK");
        // GC_WALK_MODE no lleva size word: header(111) + vid(4) + modo(1).
        // El vid es el del player (player_vid = row.id — el dummy row tiene
        // id 1, distinto del vid 211 de la conexión).
        let mut resp = [0u8; 6];
        a_sock.read_exact(&mut resp).await.expect("GC_WALK_MODE");
        assert_eq!(resp[0], 111, "GC_WALK_MODE (packet.h:212)");
        assert_eq!(
            u32::from_le_bytes(resp[1..5].try_into().unwrap()),
            1,
            "vid = row.id"
        );
        assert_eq!(resp[5], 1, "WALKMODE_WALK (packet.h:1880-1882)");
    }

    /// C-01 (chat): cap a 485 B (parity strlcpy input_main.cpp:657-658) — el
    /// size u16 del GC_CHAT nunca wrappea (el cliente legacy hace
    /// `Recv(uChatSize)` sin bound-check — PythonNetworkStreamPhaseGame.cpp:
    /// 1290-1301).
    #[tokio::test]
    async fn chat_caps_message_length() {
        let _guard = TEST_LOCK.lock().expect("test lock");
        let (mut a, mut a_sock) = test_session(221, "Long", 22, 100, 100).await;
        // CG_CHAT TALKING con 600 B + NUL: length = 4 + 601 = 605.
        let mut pkt = vec![header::CG_CHAT];
        pkt.extend_from_slice(&605u16.to_le_bytes());
        pkt.push(pchat::TYPE_TALKING);
        pkt.extend_from_slice(&[b'x'; 600]);
        pkt.push(0);
        let _ = handle(&mut a, &pkt).await.expect("chat OK");
        let echo = read_packet(&mut a_sock).await;
        assert_eq!(
            u16::from_le_bytes([echo[1], echo[2]]) as usize,
            echo.len(),
            "size consistente — sin wrap u16"
        );
        // payload "Long : " + 485 x's (sin NUL).
        assert_eq!(&echo[9..16], b"Long : ");
        assert_eq!(echo.len(), 9 + 4 + 3 + 485);
        assert!(echo[16..].iter().all(|&b| b == b'x'), "msg truncado a 485");
    }

    /// C-01 (whisper): cap a 512 B (parity input_main.cpp:398-401 — buf
    /// [CHAT_MAX_LEN + 1]) — el payload del GC_WHISPER no puede desbordar
    /// buf[513] del cliente (PythonNetworkStreamPhaseGame.cpp:1422-1429).
    #[tokio::test]
    async fn whisper_caps_message_length() {
        let _guard = TEST_LOCK.lock().expect("test lock");
        let (mut a, _a_sock) = test_session(321, "LongW", 33, 100, 100).await;
        let (mut b, _b_sock) = test_session(322, "TargetLong", 34, 500, 500).await;
        // CG_WHISPER a "TargetLong" con 600 B + NUL: wSize = 28 + 601 = 629.
        let mut pkt = vec![header::CG_WHISPER];
        pkt.extend_from_slice(&629u16.to_le_bytes());
        let mut name = [0u8; pchat::NAME_BYTES];
        name[..10].copy_from_slice(b"TargetLong");
        pkt.extend_from_slice(&name);
        pkt.extend_from_slice(&[b'y'; 600]);
        pkt.push(0);
        let _ = handle_whisper(&mut a, &pkt).await.expect("whisper OK");
        let to_b = tokio::time::timeout(Duration::from_secs(2), b.chat_rx.recv())
            .await
            .expect("TargetLong recibe en 2 s")
            .expect("outbox abierto");
        assert_eq!(to_b[0], header::GC_WHISPER);
        assert_eq!(
            to_b.len(),
            pchat::GC_WHISPER_FIXED + 512,
            "msg truncado a 512"
        );
        assert!(to_b[29..].iter().all(|&b| b == b'y'), "solo el mensaje");
    }

    /// C-04: SHOUT → canal COMPLETO (todos los mapas) con id=0 y payload
    /// "Name : msg" (parity SendShout/FuncShout input_p2p.cpp:208-228 +
    /// ChatPacket id=0 char.cpp:3947) + cooldown 15 s por sesión (parity
    /// input_main.cpp:743-748). Reloj virtual (pause/advance) para el
    /// cooldown.
    #[tokio::test]
    async fn shout_reaches_whole_channel_with_id_zero_and_15s_cooldown() {
        let _guard = TEST_LOCK.lock().expect("test lock");
        tokio::time::pause();
        let (mut a, _a_sock) = test_session(501, "Shouter", 50, 100, 100).await;
        let (mut b, _b_sock) = test_session(502, "Listener", 51, 1, 1).await; // OTRO mapa — SHOUT es de canal
        // CG_CHAT SHOUT "guerra!\0": length = 4 + 8 = 12.
        let mut pkt = vec![header::CG_CHAT];
        pkt.extend_from_slice(&12u16.to_le_bytes());
        pkt.push(pchat::TYPE_SHOUT);
        pkt.extend_from_slice(b"guerra!\0");
        let _ = handle(&mut a, &pkt).await.expect("shout OK");
        // El emisor TAMBIÉN lo recibe por su outbox (parity: GetClientSet
        // incluye al emisor) — con id=0.
        let to_a = a.chat_rx.try_recv().expect("shout al emisor");
        assert_eq!(
            u32::from_le_bytes(to_a[4..8].try_into().unwrap()),
            0,
            "id=0 (parity ChatPacket char.cpp:3947)"
        );
        assert_eq!(to_a[3], pchat::TYPE_SHOUT);
        assert_eq!(&to_a[9..], b"Shouter : guerra!", "payload 'Name : msg'");
        assert!(a.chat_rx.try_recv().is_err(), "un solo paquete por shout");
        // El peer de OTRO mapa recibe EXACTAMENTE los mismos bytes.
        let to_b = b.chat_rx.try_recv().expect("shout al otro mapa");
        assert_eq!(to_b, to_a, "mismos bytes en todo el canal");
        assert!(b.chat_rx.try_recv().is_err());
        // Segundo shout inmediato → dentro del cooldown de 15 s → no-op
        // silencioso (parity).
        let _ = handle(&mut a, &pkt).await.expect("shout en cooldown OK");
        assert!(a.chat_rx.try_recv().is_err(), "cooldown 15 s activo");
        assert!(b.chat_rx.try_recv().is_err());
        // Tras 15 s el cooldown expira y el shout vuelve a salir.
        tokio::time::advance(Duration::from_secs(16)).await;
        let _ = handle(&mut a, &pkt).await.expect("shout tras cooldown OK");
        let to_a2 = a.chat_rx.try_recv().expect("shout tras el cooldown");
        assert_eq!(&to_a2[9..], b"Shouter : guerra!");
    }

    /// C-06: resolución case-insensitive (parity `FindPC` char_manager.cpp:
    /// 209-223 — str_lower + mapa keyed por minúsculas): "target" en
    /// minúsculas llega a "Target".
    #[tokio::test]
    async fn whisper_resolves_target_case_insensitive() {
        let _guard = TEST_LOCK.lock().expect("test lock");
        let (mut a, _a_sock) = test_session(331, "Wisp", 35, 100, 100).await;
        let (mut b, _b_sock) = test_session(332, "Target", 36, 500, 500).await;
        // CG_WHISPER a "target" (minúsculas): wSize = 28 + 5 = 33.
        let mut pkt = vec![header::CG_WHISPER];
        pkt.extend_from_slice(&33u16.to_le_bytes());
        let mut name = [0u8; pchat::NAME_BYTES];
        name[..6].copy_from_slice(b"target");
        pkt.extend_from_slice(&name);
        pkt.extend_from_slice(b"hola\0");
        let _ = handle_whisper(&mut a, &pkt).await.expect("whisper OK");
        let to_b = tokio::time::timeout(Duration::from_secs(2), b.chat_rx.recv())
            .await
            .expect("Target recibe en 2 s")
            .expect("outbox abierto");
        assert_eq!(to_b[3], pchat::WHISPER_CHAT, "entregado (no NOT_EXIST)");
        let from = String::from_utf8_lossy(&to_b[4..29])
            .trim_end_matches('\0')
            .to_string();
        assert_eq!(from, "Wisp", "szNameFrom = el emisor");
        assert_eq!(&to_b[29..], b"hola", "mensaje sin NUL");
    }

    /// Lote 3 (GM `/goto`): `find_player` resuelve el nombre →
    /// (vid, map_index, x, y) del registro de sesiones activas —
    /// case-insensitive (parity FindPC) y con la posición VIVA del peer.
    #[tokio::test]
    async fn find_player_resolves_goto_target() {
        let _guard = TEST_LOCK.lock().expect("test lock");
        let (a, _a_sock) = test_session(331, "Wisp", 35, 100, 200).await;
        let (b, _b_sock) = test_session(332, "Target", 36, 500, 500).await;
        assert_eq!(
            find_player("wisp"),
            Some((331, 35, 100, 200)),
            "case-insensitive + posición de registro"
        );
        assert_eq!(find_player("TARGET"), Some((332, 36, 500, 500)));
        assert_eq!(find_player("nadie"), None, "desconectado → None");
        drop(a);
        drop(b);
        assert_eq!(
            find_player("target"),
            None,
            "el guard desregistra al soltar"
        );
    }

    /// Whisper entre 2: el destinatario recibe GC_WHISPER (bType CHAT) con el
    /// nombre del EMISOR y el emisor recibe la confirmación (echo con el
    /// nombre del DESTINATARIO). El mensaje viaja SIN el NUL (parity strlen).
    #[tokio::test]
    async fn whisper_delivers_to_recipient_and_confirms_sender() {
        let _guard = TEST_LOCK.lock().expect("test lock");
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
        assert_eq!(
            &to_bob[29..],
            b"hola bob",
            "mensaje sin NUL (parity strlen)"
        );
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
        let _guard = TEST_LOCK.lock().expect("test lock");
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
