//! `protocol::social` — MESSENGER (amigos): constantes y codificadores S→C
//! byte-exactos (parity `source/server/game/src/packet.h:1400-1490` +
//! `messenger_manager.cpp`).
//!
//! C→S (67, variable — el framer lo resuelve por subheader):
//! - `TPacketCGMessenger` (2 B: BYTE header + BYTE subheader).
//! - ADD_BY_VID (sub 0): + `DWORD vid` → total 6 B
//!   (`TPacketCGMessengerAddByVID`, packet.h:1471-1474).
//! - ADD_BY_NAME (sub 1) / REMOVE (sub 2): + nombre crudo de
//!   [`CHARACTER_NAME_MAX_LEN`] bytes (sin byte de longitud en el wire; el
//!   server strlcpy'a desde el puntero, input_main.cpp:977-982/1017-1022)
//!   → total 26 B.
//! - INVITE_ANSWER (sub 3): existe en el enum del server (packet.h:1461) pero
//!   NADIE lo envía — la invitación/respuesta va como COMANDO DE CHAT
//!   `messenger_auth y|n <nombre>` (`do_messenger_auth`,
//!   cmd_general.cpp:1167-1189; el cliente manda `/messenger_auth ...` por
//!   SendChatPacket, game.py:1007-1013). Solo documentado.
//!
//! S→C (74, `HEADER_GC_MESSENGER` packet.h:177): `TPacketGCMessenger`
//! (4 B: BYTE header + WORD size + BYTE subheader) + payload:
//! - LIST (sub 0): entradas `{ connected u8, len u8, name[len] }`; size =
//!   4 + Σ(2+len) (messenger_manager.cpp:335-376 — con 0 entradas NO se
//!   envía nada).
//! - LOGIN (sub 1) / LOGOUT (sub 2): `{ len u8, name[len] }`; size =
//!   4 + 1 + len (messenger_manager.cpp:385-430).
//! - INVITE (sub 3): existe en el enum (packet.h:1411) sin emisor — la
//!   invitación viaja como GC_CHAT CHAT_TYPE_COMMAND "messenger_auth <nombre>"
//!   (messenger_manager.cpp:174). Solo documentado.
//! - REMOVE_FRIEND (sub 4): `{ len u8, name[len] }` — sincroniza al otro lado
//!   un borrado; activo en AMBAS partes (`ENABLE_MESSENGER_REMOVE_SYNC`:
//!   server common/CommonDefines.h:55 + cliente UserInterface/Locale_inc.h:59,
//!   verificado 2026-08-21) (messenger_manager.cpp:243-258).

/// `HEADER_GC_MESSENGER` (server packet.h:177 — 74).
pub const GC_MESSENGER: u8 = 74;

// --- Subheaders C→S (packet.h:1456-1462) ---
pub const SUB_CG_ADD_BY_VID: u8 = 0;
pub const SUB_CG_ADD_BY_NAME: u8 = 1;
pub const SUB_CG_REMOVE: u8 = 2;
/// En el enum, sin wire real: la respuesta va por chat-command (ver doc del módulo).
pub const SUB_CG_INVITE_ANSWER: u8 = 3;

// --- Subheaders S→C (packet.h:1407-1415) ---
pub const SUB_GC_LIST: u8 = 0;
pub const SUB_GC_LOGIN: u8 = 1;
pub const SUB_GC_LOGOUT: u8 = 2;
/// En el enum, sin emisor (la invitación es un chat-command).
pub const SUB_GC_INVITE: u8 = 3;
/// Activo (`ENABLE_MESSENGER_REMOVE_SYNC` en server Y cliente — ver doc).
pub const SUB_GC_REMOVE_FRIEND: u8 = 4;

/// Tamaño fijo de `TPacketCGMessenger` (C→S base) y de `TPacketGCMessenger`
/// (S→C sobre): header + subheader / header + WORD size + subheader.
pub const CG_FIXED: usize = 2;
pub const GC_FIXED: usize = 4;
/// Total C→S del ADD_BY_VID: base 2 B + `DWORD vid`.
pub const CG_ADD_BY_VID_TOTAL: usize = CG_FIXED + 4;
/// Total C→S del ADD_BY_NAME y del REMOVE: base 2 B + nombre crudo de
/// `CHARACTER_NAME_MAX_LEN` bytes.
pub const CG_NAME_TOTAL: usize = CG_FIXED + crate::CHARACTER_NAME_MAX_LEN;

/// `TPacketGCMessenger` (S→C, 4 B): `BYTE header; WORD size; BYTE subheader`
/// (server packet.h:1418-1423). `size` incluye los 4 B del sobre.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TPacketGCMessenger {
    pub header: u8,
    pub size: u16,
    pub subheader: u8,
}

impl TPacketGCMessenger {
    pub const SIZE: usize = GC_FIXED;
    pub const HEADER: u8 = GC_MESSENGER;

    pub fn new(subheader: u8, payload_len: usize) -> Self {
        Self {
            header: Self::HEADER,
            size: (Self::SIZE + payload_len) as u16,
            subheader,
        }
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        [
            self.header,
            (self.size & 0xff) as u8,
            (self.size >> 8) as u8,
            self.subheader,
        ]
    }
}

/// Entrada de la LIST (login): connected = 1 online / 0 offline
/// (`TPacketGCMessengerListOnline/Offline`, packet.h:1445-1455 — mismo shape).
pub struct ListEntry {
    pub connected: bool,
    pub name: String,
}

/// Codifica el paquete completo LOGIN/LOGOUT/REMOVE_FRIEND (los tres comparten
/// el payload `{ len u8, name[len] }` — parity messenger_manager.cpp:385-430
/// y :243-258). El nombre viaja SIN NUL (el C++ hace Packet(name, size)).
fn status(subheader: u8, name: &str) -> Vec<u8> {
    let head = TPacketGCMessenger::new(subheader, 1 + name.len());
    let mut out = Vec::with_capacity(GC_FIXED + 1 + name.len());
    out.extend_from_slice(&head.to_bytes());
    out.push(name.len() as u8);
    out.extend_from_slice(name.as_bytes());
    out
}

/// GC MESSENGER LOGIN (sub 1): el companion `name` está online
/// (messenger_manager.cpp:407-430).
pub fn login(name: &str) -> Vec<u8> {
    status(SUB_GC_LOGIN, name)
}

/// GC MESSENGER LOGOUT (sub 2): el companion `name` se desconectó
/// (messenger_manager.cpp:432-456).
pub fn logout(name: &str) -> Vec<u8> {
    status(SUB_GC_LOGOUT, name)
}

/// GC MESSENGER REMOVE_FRIEND (sub 4): sincroniza al peer que `name` fue
/// borrado (messenger_manager.cpp:243-258 — REMOVE_SYNC activo).
pub fn remove_friend(name: &str) -> Vec<u8> {
    status(SUB_GC_REMOVE_FRIEND, name)
}

/// GC MESSENGER LIST (sub 0, login): UN paquete con TODAS las entradas;
/// `size = 4 + Σ(2+len)` (messenger_manager.cpp:335-376 — SendList). Con 0
/// entradas el caller NO debe enviar nada (parity: el C++ retorna antes de
/// escribir el buffer).
pub fn list(entries: &[ListEntry]) -> Vec<u8> {
    let body: usize = entries.iter().map(|e| 2 + e.name.len()).sum();
    let head = TPacketGCMessenger::new(SUB_GC_LIST, body);
    let mut out = Vec::with_capacity(GC_FIXED + body);
    out.extend_from_slice(&head.to_bytes());
    for e in entries {
        out.push(u8::from(e.connected));
        out.push(e.name.len() as u8);
        out.extend_from_slice(e.name.as_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden bytes del LOGIN (parity messenger_manager.cpp:407-430):
    /// pack(4) + bLen + name SIN NUL; size = 4 + 1 + len.
    #[test]
    fn gc_login_golden_bytes() {
        assert_eq!(
            login("Bob"),
            vec![0x4a, 8, 0, SUB_GC_LOGIN, 3, b'B', b'o', b'b']
        );
    }

    /// Golden bytes del LOGOUT (misma forma, subheader 2 — :432-456).
    #[test]
    fn gc_logout_golden_bytes() {
        assert_eq!(logout("Bob")[..4], [0x4a, 8, 0, SUB_GC_LOGOUT]);
        assert_eq!(&logout("Bob")[4..], &[3, b'B', b'o', b'b']);
    }

    /// Golden bytes del REMOVE_FRIEND (REMOVE_SYNC activo — :243-258).
    #[test]
    fn gc_remove_friend_golden_bytes() {
        assert_eq!(
            remove_friend("Ann"),
            vec![0x4a, 8, 0, SUB_GC_REMOVE_FRIEND, 3, b'A', b'n', b'n']
        );
    }

    /// Golden bytes del LIST (login — SendList :335-376): size =
    /// 4 + Σ(2+len); cada entrada {connected, len, name} SIN NUL.
    #[test]
    fn gc_list_golden_bytes() {
        let entries = [
            ListEntry {
                connected: false,
                name: "Ann".into(),
            },
            ListEntry {
                connected: true,
                name: "Bob".into(),
            },
        ];
        // 4 + (2+3) + (2+3) = 14.
        assert_eq!(
            list(&entries),
            vec![
                0x4a,
                14,
                0,
                SUB_GC_LIST, //
                0,
                3,
                b'A',
                b'n',
                b'n', //
                1,
                3,
                b'B',
                b'o',
                b'b',
            ]
        );
    }

    /// El size del sobre SIEMPRE incluye los 4 B del header (parity
    /// `pack.size = sizeof(TPacketGCMessenger) + ...`).
    #[test]
    fn envelope_size_includes_header() {
        let head = TPacketGCMessenger::new(SUB_GC_LOGIN, 1 + "Bob".len());
        assert_eq!(head.size as usize, GC_FIXED + 1 + 3);
        assert_eq!(head.to_bytes().len(), GC_FIXED);
        // Consistencia global: el tamaño del Vec == head.size.
        let pkt = login("Bob");
        assert_eq!(pkt.len(), head.size as usize);
    }

    /// Consts C→S citadas (el framer las usa para el corte variable).
    #[test]
    fn cg_sizes_match_packet_h() {
        assert_eq!(CG_FIXED, 2); // TPacketCGMessenger (Packet.h:801-805)
        assert_eq!(CG_ADD_BY_VID_TOTAL, 6); // + DWORD vid (:1471-1474)
        assert_eq!(CG_NAME_TOTAL, 26); // + CHARACTER_NAME_MAX_LEN=24
        assert_eq!(SUB_CG_ADD_BY_VID, 0);
        assert_eq!(SUB_CG_ADD_BY_NAME, 1);
        assert_eq!(SUB_CG_REMOVE, 2);
        assert_eq!(SUB_GC_LIST, 0);
        assert_eq!(SUB_GC_LOGIN, 1);
        assert_eq!(SUB_GC_LOGOUT, 2);
        assert_eq!(SUB_GC_REMOVE_FRIEND, 4);
    }
}
