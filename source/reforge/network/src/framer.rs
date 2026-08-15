//! `Framer` — framing sin prefijo de longitud (spec §2): header BYTE + payload
//! de tamaño fijo por tabla.
//!
//! - Cliente→servidor: tabla de tamaños de `CPacketInfoCG`
//!   (`source/server/game/src/packet_info.cpp:136-236`, subconjunto F1 del
//!   flujo de login, sequence OFF). Header desconocido →
//!   [`FramingError::UnknownHeader`] → el caller cierra la conexión
//!   (paridad `input.cpp:77-84`).
//! - Servidor→cliente: NO hay tabla — el server envía structs crudos
//!   (`desc->Packet(&struct, sizeof(struct))`); el helper [`read_exact_size`]
//!   lee `n` bytes con los tamaños de los structs del crate `protocol`.

use std::io;

use protocol::header;
use protocol::{
    TPacketCGHandshake, TPacketCGLogin, TPacketCGLogin2, TPacketCGLogin3,
    TPacketCGPlayerCreate, TPacketCGPlayerDelete, TPacketCGPlayerSelect,
};
use tokio::io::{AsyncRead, AsyncReadExt};

/// Rol de la conexión: determina el tamaño de `CG_LOGIN3`
/// (`packet_info.cpp:157` — `sizeof(TPacketCGLogin3) + (g_bAuthServer ? 3 : 0)`):
/// 65 B en canal, 68 B en auth (sufijo `szLanguage[3]`, spec §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionRole {
    /// Auth (:30001): `CG_LOGIN3` = 68 B (con idioma).
    Auth,
    /// Canal de juego (:30003+): `CG_LOGIN3` = 65 B (sin idioma).
    Channel,
}

/// Tamaño total (header + payload) de un paquete cliente→servidor, o `None`
/// si el header no está en la tabla (→ cerrar la conexión, `input.cpp:77-84`).
///
/// Subconjunto F1 de `CPacketInfoCG` (flujo de login + fase de juego,
/// sequence OFF): los de tamaño fijo — ver `packet_size` para la lista
/// completa con sus Packet.h del cliente. Los de tamaño VARIABLE del C++
/// (CG_CHAT 3, CG_WHISPER 19, CG_SYNC_POSITION 8, CG_SHOP 50, CG_TEXT... —
/// su `iExtraLen` depende del contenido) NO están en la tabla fija: se
/// resuelven en `try_extract` (CG_CHAT/CG_WHISPER/CG_SHOP) o cierran la
/// conexión si no tienen arm (seguro por defecto, documentado).
///
/// Matices verificados contra el C++:
/// - `0x00` NO se acepta. El C++ lo consume como no-op de 1 byte ANTES del
///   lookup (`input.cpp:75-76`); aquí es divergencia deliberada: ningún cliente
///   real lo envía, y el framer lo cierra como header desconocido (más seguro;
///   el efecto observable con clientes legítimos es idéntico).
/// - `CG_ENTERGAME` (10, 1 B, `packet_info.cpp:165` / `packet.h:613-616`) y
///   `CG_STATE_CHECKER` (206, 1 B, `packet_info.cpp:232` /
///   `ServerStateChecker.cpp:60`) ya están en la tabla: los necesita la entrada
///   al mundo (F4) y el ping del selector de canales (F2).
/// - `CG_MARK_LOGIN` (100, 9 B, `packet_info.cpp:141` / `packet.h:1729-1734`):
///   la conexión del guild mark responde con este paquete al handshake del
///   server (`GuildMarkDownloader.cpp:213-229`); el server normal
///   (`guild_mark_server` OFF) lo rechaza cerrando (`input.cpp:560-572`) — el
///   handshake lo reporta como `HandshakeError::MarkLogin` y el canal cierra.
/// - `CG_CLIENT_VERSION2` (0xf1, 67 B, `Packet.h:135,974-979`): el cliente lo
///   manda al terminar la carga (`SendClientVersionPacket`); el canal lo
///   IGNORA sin validar (parity `input.cpp:205-213` — el auth nunca lo recibe:
///   su flujo termina en el GC_AUTH_SUCCESS).
/// - Sin idle timeout: el C++ tampoco lo tiene (una conexión muda queda abierta
///   hasta que el SO la cierre). **F2 debe añadir un timeout explícito.**
///
/// Los paquetes de tamaño variable del C++ (CG_CHAT, CG_TEXT, ...) NO están
/// en la tabla fija: los que tienen arm en `try_extract` (CG_CHAT 3,
/// CG_WHISPER 19, CG_SHOP 50) se resuelven por su length/subheader; los
/// demás cierran la conexión (mismo comportamiento que un header
/// desconocido, seguro por defecto).
///
/// La tabla de la FASE DE JUEGO (tamaños fijos) está en [`game_phase_size`] y
/// es SOLO canal: el rol Auth la rechaza entera (flujo corto — un cliente del
/// auth nunca envía paquetes de juego; cierre como header desconocido).
pub fn packet_size(role: ConnectionRole, header: u8) -> Option<usize> {
    Some(match header {
        header::CG_HANDSHAKE => TPacketCGHandshake::SIZE, // 0xff, 13
        header::CG_TIME_SYNC => TPacketCGHandshake::SIZE, // 0xfc, 13 (mismo shape)
        header::CG_PONG => 1,                             // 0xfe, sizeof(BYTE)
        header::CG_LOGIN => TPacketCGLogin::SIZE,         // 1, 49
        header::CG_LOGIN2 => TPacketCGLogin2::SIZE,       // 109, 52
        header::CG_LOGIN3 => match role {
            ConnectionRole::Channel => TPacketCGLogin3::SIZE_CHANNEL, // 111, 65
            ConnectionRole::Auth => TPacketCGLogin3::SIZE_AUTH,       // 111, 68
        },
        header::CG_CHARACTER_SELECT => TPacketCGPlayerSelect::SIZE, // 6, 2
        header::CG_CHARACTER_CREATE => TPacketCGPlayerCreate::SIZE, // 4, 34
        header::CG_CHARACTER_DELETE => TPacketCGPlayerDelete::SIZE, // 5, 10
        header::CG_ENTERGAME => 1,   // 10, sizeof(TPacketCGEnterGame) = BYTE (packet.h:613-616)
        header::CG_MARK_LOGIN => protocol::world::TPacketCGMarkLogin::SIZE, // 100, 9 (packet_info.cpp:141)
        header::CG_CLIENT_VERSION2 => 67, // 0xf1, TPacketCGClientVersion2 = 1 + 33 + 33 (Packet.h:974-979)
        header::CG_STATE_CHECKER => 1, // 206, sizeof(BYTE) — ping selector de canales (packet_info.cpp:232)
        header::CG_LOCALE_REQUEST => protocol::locale::CgLocaleRequest::SIZE, // 132, 4 (F1 locale — aditivo, ADR-0009)
        // Fase de juego (tabla C→S de tamaños fijos): SOLO canal — el rol
        // Auth los rechaza (flujo corto; un cliente del auth nunca los envía).
        _ if role == ConnectionRole::Channel => game_phase_size(header)?,
        _ => return None,
    })
}

/// Tamaño fijo de los paquetes de la FASE DE JUEGO (solo canal) — la tabla
/// C→S de tamaños fijos del `CPacketInfoCG` (packet_info.cpp:158-235; los
/// tamaños son los structs del PACKET.H DEL CLIENTE, packed — la entrada al
/// mundo no puede cerrar por un paquete de juego legítimo).
///
/// gap-lane A (2026-08-15): añadidos los 21 headers que el C++ conoce y el
/// Rust no (safebox, party, guild, refine, dragon soul, fishing, acce, mall,
/// item give, fly targeting, hack, script select item —
/// `packet_info.cpp:191-234`). Antes caían como [`FramingError::UnknownHeader`]
/// → CIERRE de conexión; ahora el framer los acepta con su tamaño exacto y
/// caen en el 'other' del dispatch (ignorados sin desconectar).
///
/// Headers fuera de la tabla → `None` → el caller cierra la conexión
/// (parity `input.cpp:77-84`). En el rol Auth esta tabla NO existe: cualquier
/// header de juego es `UnknownHeader` (ver [`packet_size`]).
fn game_phase_size(header: u8) -> Option<usize> {
    Some(match header {
        header::CG_ATTACK => 8,             // 2, header+bType+vid+2 CRC (Packet.h:509-516)
        header::CG_MOVE => 16,              // 7, header+func+arg+rot+lx+ly+time (Packet.h:677-686)
        header::CG_ITEM_USE => 4, // 11, header + TItemPos (Packet.h:559-563 + packet.h:618-622) — el tamaño CORRECTO C→S (el 16 B era el GC_ITEM_USE S→C, bug latente corregido)
        header::CG_ITEM_DROP => 8,          // 12, header+pos+elk (cheque OFF en el cliente) (Packet.h:556-564)
        header::CG_ITEM_MOVE => 8,          // 13, header+pos+change_pos+num (Packet.h:577-583)
        header::CG_ITEM_PICKUP => 5,        // 15, header+vid (Packet.h:585-589)
        header::CG_QUICKSLOT_ADD => 4,      // 16, header+pos+slot (Packet.h:591-596)
        header::CG_QUICKSLOT_DEL => 2,      // 17 (Packet.h:598-602)
        header::CG_QUICKSLOT_SWAP => 3,     // 18 (Packet.h:604-609)
        header::CG_ITEM_DROP2 => 9,         // 20, header+pos+gold+count (cheque OFF) (Packet.h:566-575)
        header::CG_ON_CLICK => 5,           // 26, header+vid (Packet.h:611-615)
        header::CG_EXCHANGE => 47,          // 27, header+sub+is_me+arg1+arg2+arg3+values+attrs (Packet.h:1812-1822)
        header::CG_CHARACTER_POSITION => 2, // 28, header+position (Packet.h:653-657)
        header::CG_SCRIPT_ANSWER => 2,      // 29 (Packet.h:659-663)
        header::CG_QUEST_INPUT_STRING => 66, // 30, header+szString[65] (Packet.h:1002-1006)
        header::CG_QUEST_CONFIRM => 6,      // 31, header+answer+requestPID (Packet.h:1008-1013)
        header::CG_PVP => 10,               // 41, header+src+dst+mode (Packet.h:2014-2020)
        header::CG_FLY_TARGETING => 17,     // 51, header+shooter+target+x+y (Packet.h:709-716)
        header::CG_USE_SKILL => 9,          // 52, header+vnum+target (Packet.h:833-838)
        header::CG_ADD_FLY_TARGETING => 13, // 53, header+dwTargetVID+x+y (Packet.h:717-723)
        header::CG_SHOOT => 2,              // 54 (Packet.h:718-722)
        header::CG_MYSHOP => 35,            // 55, header+szSign[33]+count (SHOP_SIGN_MAX_LEN=32) (Packet.h:953-958)
        header::CG_ITEM_USE_TO_ITEM => 7,   // 60, header+source+target (Packet.h:549-554)
        header::CG_TARGET => 5,             // 61, header+vid (Packet.h:671-675)
        header::CG_WARP => 15,              // 65, header+x+y+addr+port (Packet.h:2028-2035)
        header::CG_SCRIPT_BUTTON => 5,      // 66, header+idx (Packet.h:665-669)
        header::CG_MESSENGER => 2,          // 67, header+subheader (Packet.h:801-805)
        header::CG_MALL_CHECKOUT => 5,      // 69, header+bMallPos+TItemPos (Packet.h:839-845)
        header::CG_SAFEBOX_CHECKIN => 5,    // 70, header+bSafePos+TItemPos (Packet.h:832-838)
        header::CG_SAFEBOX_CHECKOUT => 5,   // 71, header+bSafePos+TItemPos (Packet.h:825-831)
        header::CG_PARTY_INVITE => 5,       // 72, header+vid (Packet.h:856-860)
        header::CG_PARTY_INVITE_ANSWER => 6, // 73, header+leader_pid+accept (Packet.h:862-867)
        header::CG_PARTY_REMOVE => 5,       // 74, header+pid (Packet.h:869-873)
        header::CG_PARTY_SET_STATE => 7,    // 75, header+dwVID+byState+byFlag (Packet.h:875-881)
        header::CG_PARTY_USE_SKILL => 6,    // 76, header+bySkillIndex+dwTargetVID (Packet.h:897-902)
        header::CG_SAFEBOX_ITEM_MOVE => 8,  // 77, header+pos+change_pos+num — mismo shape que CG_ITEM_MOVE (Packet.h:593-599)
        header::CG_PARTY_PARAMETER => 2,    // 78, header+bDistributeMode (Packet.h:1012-1016)
        header::CG_GUILD => 2,              // 80, header+subheader (Packet.h:923-927)
        header::CG_ANSWER_MAKE_GUILD => 14, // 81, header+guild_name[13] (GUILD_NAME_MAX_LEN=12) (Packet.h:929-933)
        header::CG_FISHING => 2,            // 82, header+dir (packet.h:1800-1804)
        header::CG_ITEM_GIVE => 9,          // 83, header+dwTargetVID+TItemPos+byItemCount (Packet.h:935-941)
        header::CG_REFINE => 3,             // 96, header+pos+type (Packet.h:976-982)
        header::CG_HACK => 257,             // 105, header+szBuf[256] (Packet.h:943-947)
        header::CG_SCRIPT_SELECT_ITEM => 5, // 114, header+selection (Packet.h:1031-1035)
        header::CG_DRAGON_SOUL_REFINE => 47, // 205, header+bSubType+TItemPos[15] (DS_REFINE_WINDOW_MAX_NUM=15 — GameType.h:191) (Packet.h:2715-2722)
        header::CG_ACCE => 23,              // 211, header+subheader+bWindow+dwPrice+bPos+tPos+dwItemVnum+dwMinAbs+dwMaxAbs (Packet.h:2765-2776)
        _ => return None,
    })
}

/// Error de framing. Todos los errores son terminales: el caller DEBE cerrar
/// la conexión (paridad `input.cpp:77-84` `PHASE_CLOSE`).
#[derive(Debug)]
pub enum FramingError {
    /// Header sin entrada en la tabla cliente→servidor → cerrar la conexión.
    UnknownHeader { header: u8 },
    /// EOF del peer sin datos pendientes (cierre limpio del otro lado).
    Eof,
    /// EOF con bytes a medio paquete en el buffer (cierre sucio; se descartan).
    UnexpectedEof { buffered: usize },
    /// Error de I/O del socket.
    Io(io::Error),
}

impl core::fmt::Display for FramingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FramingError::UnknownHeader { header } => {
                write!(f, "unknown packet header 0x{header:02x} (closing connection, parity input.cpp:77-84)")
            }
            FramingError::Eof => write!(f, "connection closed cleanly"),
            FramingError::UnexpectedEof { buffered } => {
                write!(f, "connection closed with {buffered} unparsed bytes buffered")
            }
            FramingError::Io(e) => write!(f, "socket io error: {e}"),
        }
    }
}

impl std::error::Error for FramingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FramingError::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// Rango de tamaños del paquete para un header: `(mín, máx)`.
///
/// F2b: el LOGIN3 del auth es de tamaño VARIABLE — 68 B base, 72 con
/// `version[4]`, 88 con `version[4]+hwid[16]` (el cliente los manda en un solo
/// write y espera la respuesta). El framer entrega el paquete completo cuando
/// el buffer tiene ≥ mín y recorta a máx (los bytes extra — imposibles en el
/// flujo auth — quedarían bufferizados). El canal sigue fijo en 65 B.
pub fn packet_range(role: ConnectionRole, header: u8) -> Option<(usize, usize)> {
    if header == header::CG_LOGIN3 && role == ConnectionRole::Auth {
        return Some((TPacketCGLogin3::SIZE_AUTH, TPacketCGLogin3::SIZE_AUTH_FULL));
    }
    packet_size(role, header).map(|n| (n, n))
}

/// Fragmenta el flujo cliente→servidor en paquetes completos `Vec<u8>`.
///
/// Estado: buffer interno de bytes pendientes + rol de la conexión. Maneja
/// paquetes partidos en varios reads y varios paquetes en un mismo read.
pub struct Framer {
    role: ConnectionRole,
    buf: Vec<u8>,
}

impl Framer {
    /// Framer para el rol dado (auth o canal).
    pub fn new(role: ConnectionRole) -> Self {
        Self { role, buf: Vec::with_capacity(256) }
    }

    /// Bytes pendientes de formar un paquete completo.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// Empuja bytes (p.ej. un `read` del socket) y devuelve TODOS los paquetes
    /// completos presentes en el buffer, en orden de llegada. Los bytes que no
    /// completan un paquete quedan bufferizados para el siguiente `push`.
    ///
    /// `Err(UnknownHeader)` → el caller DEBE cerrar la conexión. Los paquetes
    /// completos que hubiera antes del header desconocido dentro del MISMO
    /// `push` se descartan (nunca se entregan); en el flujo pull
    /// ([`Framer::next_packet`]) esto no ocurre: cada paquete se entrega en su
    /// propia llamada, así que los anteriores ya fueron procesados.
    pub fn push(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>, FramingError> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        while let Some(pkt) = self.try_extract()? {
            out.push(pkt);
        }
        Ok(out)
    }

    /// Extrae UN paquete completo del stream, leyendo del socket todo lo que
    /// falte (flujo pull). EOF sin datos pendientes → [`FramingError::Eof`];
    /// EOF con datos a medio paquete → [`FramingError::UnexpectedEof`].
    pub async fn next_packet<R: AsyncRead + Unpin>(
        &mut self,
        reader: &mut R,
    ) -> Result<Vec<u8>, FramingError> {
        let mut chunk = [0u8; 1024];
        loop {
            if let Some(pkt) = self.try_extract()? {
                return Ok(pkt);
            }
            let n = reader.read(&mut chunk).await.map_err(FramingError::Io)?;
            if n == 0 {
                return if self.buf.is_empty() {
                    Err(FramingError::Eof)
                } else {
                    Err(FramingError::UnexpectedEof { buffered: self.buf.len() })
                };
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    fn try_extract(&mut self) -> Result<Option<Vec<u8>>, FramingError> {
        if self.buf.is_empty() {
            return Ok(None);
        }
        let hdr = self.buf[0];
        // CG_CHAT (3): paquete de tamaño VARIABLE del C++ — el WORD LE en
        // [1..3] es el tamaño TOTAL (header 4 B + mensaje; parity
        // `Packet.h:534-539` TPacketCGChat + `input_main.cpp:641-655` donde
        // `iExtraLen = pinfo->size - sizeof(TPacketCGChat)`). Fuera de la
        // tabla fija (`packet_size` -> None) por diseño; se resuelve aquí.
        if hdr == header::CG_CHAT && self.role == ConnectionRole::Channel {
            if self.buf.len() < 4 {
                return Ok(None); // aún no está el header completo
            }
            let total = u16::from_le_bytes([self.buf[1], self.buf[2]]) as usize;
            if total < 4 {
                // length inválido — parity PHASE_CLOSE del C++ (input_main.cpp:650-655).
                return Err(FramingError::UnknownHeader { header: hdr });
            }
            if self.buf.len() < total {
                return Ok(None);
            }
            return Ok(Some(self.buf.drain(..total).collect()));
        }
        // CG_WHISPER (19): paquete de tamaño VARIABLE del C++ — el WORD LE
        // en [1..3] es el tamaño TOTAL (header 28 B fijos — TPacketCGWhisper
        // BYTE+WORD+szNameTo[25] — + mensaje; parity `Packet.h:540-546` +
        // `input_main.cpp:273-286` donde `iExtraLen = wSize - sizeof`).
        // Fuera de la tabla fija por diseño; se resuelve aquí como CG_CHAT.
        if hdr == header::CG_WHISPER && self.role == ConnectionRole::Channel {
            if self.buf.len() < protocol::chat::CG_WHISPER_FIXED {
                return Ok(None); // aún no está el header completo
            }
            let total = u16::from_le_bytes([self.buf[1], self.buf[2]]) as usize;
            if total < protocol::chat::CG_WHISPER_FIXED {
                // length inválido — parity PHASE_CLOSE del C++ (input_main.cpp:282-286).
                return Err(FramingError::UnknownHeader { header: hdr });
            }
            if self.buf.len() < total {
                return Ok(None);
            }
            return Ok(Some(self.buf.drain(..total).collect()));
        }
        // CG_SHOP (50): paquete de tamaño VARIABLE del C++ — `TPacketCGShop`
        // (2 B: header + subheader, Packet.h:641-645) + payload según el
        // subheader: END=0 (0 extra), BUY=1 (+count,pos = 2), SELL=2 (+cell
        // = 1), SELL2=3 (+cell,count = 2) (input_main.cpp:1054-1088). El
        // header NO está en la tabla fija (parity: `Set(HEADER_CG_SHOP,
        // sizeof(TPacketCGShop), ...)` + `iExtraLen` del C++); se resuelve
        // aquí como CG_CHAT — el dispatch `game.rs` ya tiene el arm.
        if hdr == header::CG_SHOP && self.role == ConnectionRole::Channel {
            if self.buf.len() < 2 {
                return Ok(None); // falta el subheader
            }
            let total = match self.buf[1] {
                0 => 2, // END — TPacketCGShop base
                1 => 4, // BUY — + count, pos
                2 => 3, // SELL — + cell
                3 => 4, // SELL2 — + cell, count
                _ => 2, // subheader desconocido — el handler lo descarta
            };
            if self.buf.len() < total {
                return Ok(None);
            }
            return Ok(Some(self.buf.drain(..total).collect()));
        }
        let Some((min, max)) = packet_range(self.role, hdr) else {
            return Err(FramingError::UnknownHeader { header: hdr });
        };
        if self.buf.len() < min {
            return Ok(None);
        }
        // F2b: tamaño variable (LOGIN3 auth 68..88) — se entrega todo lo que
        // haya (hasta máx); los fijos (min == max) conservan el comportamiento
        // histórico.
        let n = self.buf.len().min(max);
        Ok(Some(self.buf.drain(..n).collect()))
    }
}

/// Lee exactamente `n` bytes del stream (dirección servidor→cliente).
///
/// A diferencia del framer (tabla fija), el servidor envía structs crudos sin
/// prefijo de longitud: el caller pasa el tamaño del struct del crate
/// `protocol` (p.ej. `TPacketGCHandshake::SIZE`, `TPacketGCLoginSuccess::SIZE`).
/// `n == 0` devuelve un `Vec` vacío sin tocar el stream; EOF a medio paquete →
/// `io::ErrorKind::UnexpectedEof`.
pub async fn read_exact_size<R: AsyncRead + Unpin>(reader: &mut R, n: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; n];
    reader.read_exact(&mut buf).await?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::phase;
    use protocol::TPacketGCHandshake;
    use protocol::TPacketGCPhase;
    use tokio::io::AsyncWriteExt;

    // ------------------------------------------------------------------
    // Tabla de tamaños (F1.3)
    // ------------------------------------------------------------------

    #[test]
    fn size_table_covers_login_flow() {
        // espec §2 + packet_info.cpp:136-236 (sequence OFF)
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_HANDSHAKE), Some(13));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_PONG), Some(1));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_LOGIN), Some(49));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_CHARACTER_CREATE), Some(34));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_CHARACTER_DELETE), Some(10));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_CHARACTER_SELECT), Some(2));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_LOGIN2), Some(52));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_TIME_SYNC), Some(13));
        // 1 B: entrada al mundo (F4) y ping del selector de canales (F2)
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_ENTERGAME), Some(1));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_STATE_CHECKER), Some(1));
        // 0xf1 (67 B): la versión del cliente al terminar la carga
        // (TPacketCGClientVersion2 = 1 + 33 + 33, Packet.h:974-979).
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_CLIENT_VERSION2), Some(67));
        assert_eq!(packet_size(ConnectionRole::Auth, header::CG_CLIENT_VERSION2), Some(67), "el auth no lo recibe (flujo corto) — la tabla es común");
        // Fase de juego: los paquetes del spawn/idle con sus tamaños del
        // Packet.h del cliente (packed) — el MOVE del spawn (16 B) es el que
        // cerraba la entrada (slice 3.7).
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_MOVE), Some(16));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_ATTACK), Some(8));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_ITEM_USE), Some(4), "11, header + TItemPos (Packet.h:559-563) — el 16 B era el GC S→C, bug corregido");
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_QUICKSLOT_ADD), Some(4));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_MYSHOP), Some(35));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_WARP), Some(15));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_ITEM_DROP), Some(8), "cheque OFF en el cliente");
        // Los variables del C++ siguen fuera de la tabla fija (se resuelven
        // en try_extract — CG_CHAT/CG_WHISPER/CG_SHOP — o cierran).
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_CHAT), None);
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_WHISPER), None);
        // LOGIN3: 65 canal / 68 auth (sufijo szLanguage[3])
        assert_eq!(
            packet_size(ConnectionRole::Channel, header::CG_LOGIN3),
            Some(TPacketCGLogin3::SIZE_CHANNEL)
        );
        assert_eq!(
            packet_size(ConnectionRole::Auth, header::CG_LOGIN3),
            Some(TPacketCGLogin3::SIZE_AUTH)
        );
        // F1 (locale, aditivo): CG_LOCALE_REQUEST (132) = 4 B en ambos roles
        // (la tabla es común; el canal no lo recibe en el flujo real).
        assert_eq!(packet_size(ConnectionRole::Auth, header::CG_LOCALE_REQUEST), Some(4));
        assert_eq!(packet_size(ConnectionRole::Channel, header::CG_LOCALE_REQUEST), Some(4));
        // desconocidos → None → el caller cierra la conexión (input.cpp:77-84)
        assert_eq!(packet_size(ConnectionRole::Channel, 0), None);
        assert_eq!(packet_size(ConnectionRole::Auth, 0), None);
        // GC_PHASE (0xfd) es servidor→cliente: no está en la tabla C→S
        assert_eq!(packet_size(ConnectionRole::Auth, 0xfd), None);
        assert_eq!(packet_size(ConnectionRole::Channel, 0x99), None);
    }

    // ------------------------------------------------------------------
    // gap-lane A (2026-08-15): los 21 headers C→S que el C++ conoce
    // (packet_info.cpp:191-234) y el Rust no — safebox, party, guild,
    // refine, dragon soul, fishing, acce, mall, item give, fly targeting,
    // hack, script select item. Tamaño EXACTO del struct packed del
    // Packet.h del cliente; antes: UnknownHeader → CIERRE de conexión.
    // ------------------------------------------------------------------

    /// (header, tamaño total) de los 21 — verificado contra packet_info.cpp
    /// (sizeof del struct del server) y Packet.h del cliente (packed, pack(1)).
    const GAP_LANE_A: &[(u8, usize)] = &[
        (header::CG_ADD_FLY_TARGETING, 13), // 53, TPacketCGFlyTargeting (Packet.h:717-723)
        (header::CG_MESSENGER, 2), // 67, TPacketCGMessenger (Packet.h:801-805)
        (header::CG_MALL_CHECKOUT, 5), // 69, TPacketCGMallCheckout (Packet.h:839-845)
        (header::CG_SAFEBOX_CHECKIN, 5), // 70, TPacketCGSafeboxCheckin (Packet.h:832-838)
        (header::CG_SAFEBOX_CHECKOUT, 5), // 71, TPacketCGSafeboxCheckout (Packet.h:825-831)
        (header::CG_PARTY_INVITE, 5), // 72, TPacketCGPartyInvite (Packet.h:856-860)
        (header::CG_PARTY_INVITE_ANSWER, 6), // 73, TPacketCGPartyInviteAnswer (Packet.h:862-867)
        (header::CG_PARTY_REMOVE, 5), // 74, TPacketCGPartyRemove (Packet.h:869-873)
        (header::CG_PARTY_SET_STATE, 7), // 75, TPacketCGPartySetState (Packet.h:875-881)
        (header::CG_PARTY_USE_SKILL, 6), // 76, TPacketCGPartyUseSkill (Packet.h:897-902)
        (header::CG_SAFEBOX_ITEM_MOVE, 8), // 77, TPacketCGItemMove (Packet.h:593-599)
        (header::CG_PARTY_PARAMETER, 2), // 78, TPacketCGPartyParameter (Packet.h:1012-1016)
        (header::CG_GUILD, 2), // 80, TPacketCGGuild (Packet.h:923-927)
        (header::CG_ANSWER_MAKE_GUILD, 14), // 81, TPacketCGAnswerMakeGuild (Packet.h:929-933)
        (header::CG_FISHING, 2), // 82, TPacketCGFishing (packet.h:1800-1804)
        (header::CG_ITEM_GIVE, 9), // 83, TPacketCGGiveItem (Packet.h:935-941)
        (header::CG_REFINE, 3), // 96, TPacketCGRefine (Packet.h:976-982)
        (header::CG_HACK, 257), // 105, TPacketCGHack (Packet.h:943-947)
        (header::CG_SCRIPT_SELECT_ITEM, 5), // 114, TPacketCGScriptSelectItem (Packet.h:1031-1035)
        (header::CG_DRAGON_SOUL_REFINE, 47), // 205, TPacketCGDragonSoulRefine (Packet.h:2715-2722)
        (header::CG_ACCE, 23), // 211, SPacketAcce (Packet.h:2765-2776)
    ];

    #[test]
    fn gap_lane_a_headers_have_exact_sizes() {
        // El framer del canal conoce los 21 headers con el tamaño EXACTO del
        // struct packed del Packet.h del cliente (packet_info.cpp Set(...)).
        for &(hdr, size) in GAP_LANE_A {
            assert_eq!(
                packet_size(ConnectionRole::Channel, hdr),
                Some(size),
                "header 0x{hdr:02x}: tamaño del struct del Packet.h del cliente"
            );
        }
    }

    #[test]
    fn gap_lane_a_headers_rejected_on_auth() {
        // El rol Auth solo habla el flujo de login: cualquier header de la
        // fase de juego (los 21 del gap incluidos) → None → UnknownHeader →
        // cierre (parity input.cpp:77-84).
        for &(hdr, _) in GAP_LANE_A {
            assert_eq!(
                packet_size(ConnectionRole::Auth, hdr),
                None,
                "header 0x{hdr:02x}: el auth NO conoce la fase de juego"
            );
        }
    }

    #[test]
    fn gap_lane_a_packets_flow_on_channel_close_on_auth() {
        // Un paquete de cada familia (payload de relleno del tamaño exacto)
        // fluye por el framer del canal → cae en el 'other' del dispatch
        // (ignorado sin desconectar); el auth los rechaza cerrando.
        let samples: &[(u8, usize)] = &[
            (header::CG_GUILD, 2),
            (header::CG_SAFEBOX_CHECKIN, 5),
            (header::CG_PARTY_SET_STATE, 7),
            (header::CG_ANSWER_MAKE_GUILD, 14),
            (header::CG_DRAGON_SOUL_REFINE, 47),
            (header::CG_ACCE, 23),
            (header::CG_HACK, 257),
        ];
        for &(hdr, size) in samples {
            let mut pkt = vec![hdr];
            pkt.resize(size, 0xAA);
            let out = Framer::new(ConnectionRole::Channel).push(&pkt).unwrap();
            assert_eq!(out, vec![pkt.clone()], "header 0x{hdr:02x} en canal");
            assert!(
                matches!(
                    Framer::new(ConnectionRole::Auth).push(&pkt),
                    Err(FramingError::UnknownHeader { header: h }) if h == hdr
                ),
                "header 0x{hdr:02x} en auth → UnknownHeader"
            );
        }
    }

    /// Los 21 del gap concatenados en un solo read → 21 paquetes en orden
    /// (los tamaños no se pisan entre sí).
    #[test]
    fn gap_lane_a_concatenated_in_one_push() {
        let mut data = Vec::new();
        for &(hdr, size) in GAP_LANE_A {
            let mut pkt = vec![hdr];
            pkt.resize(size, 0x55);
            data.extend_from_slice(&pkt);
        }
        let out = Framer::new(ConnectionRole::Channel).push(&data).unwrap();
        assert_eq!(out.len(), GAP_LANE_A.len());
        let mut off = 0;
        for &(hdr, size) in GAP_LANE_A {
            assert_eq!(out[off][0], hdr, "orden del header 0x{hdr:02x}");
            assert_eq!(out[off].len(), size);
            off += 1;
        }
    }

    // ------------------------------------------------------------------
    // F1.3: fragmentación y concatenación
    // ------------------------------------------------------------------

    /// F2b: el LOGIN3 del auth es de tamaño variable (68/72/88 B) — el framer
    /// entrega el paquete completo con el tamaño que el cliente mandó; el
    /// canal sigue fijo en 65 B.
    #[test]
    fn auth_login3_variable_size_68_72_88() {
        let base = TPacketCGLogin3::new_auth("test", "1234", [1, 2, 3, 4], "es");
        let hwid = [0x22u8; 16];
        // 68 B (cliente actual).
        let p68 = base.to_bytes_auth();
        let out = Framer::new(ConnectionRole::Auth).push(&p68).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 68);
        // 72 B (con version).
        let p72 = base.to_bytes_auth_with(Some(40999), None);
        let out = Framer::new(ConnectionRole::Auth).push(&p72).unwrap();
        assert_eq!(out[0].len(), 72);
        // 88 B (version + hwid) — entregado ENTERO.
        let p88 = base.to_bytes_auth_with(Some(40999), Some(hwid));
        assert_eq!(p88.len(), 88);
        let mut f = Framer::new(ConnectionRole::Auth);
        let out = f.push(&p88).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], p88, "88 B se entregan enteros");
        assert_eq!(f.buffered(), 0);
        // Fragmentado en dos pushes (40 + 48) → completo al segundo.
        let mut f = Framer::new(ConnectionRole::Auth);
        assert!(f.push(&p88[..40]).unwrap().is_empty(), "< 68 B → incompleto");
        let out = f.push(&p88[40..]).unwrap();
        assert_eq!(out[0], p88);
        // El canal sigue fijo en 65 B.
        let p65 = TPacketCGLogin3::new_channel("test", "1234", [1, 2, 3, 4]).to_bytes_channel();
        assert_eq!(packet_range(ConnectionRole::Channel, header::CG_LOGIN3), Some((65, 65)));
        let out = Framer::new(ConnectionRole::Channel).push(&p65).unwrap();
        assert_eq!(out[0].len(), 65);
    }

    #[test]
    fn fragment_byte_by_byte() {
        let pkt = TPacketCGLogin3::new_channel("test", "1234", [1, 2, 3, 4]).to_bytes_channel();
        let mut f = Framer::new(ConnectionRole::Channel);
        for (i, b) in pkt.iter().enumerate() {
            let out = f.push(&[*b]).unwrap();
            if i + 1 < pkt.len() {
                assert!(out.is_empty(), "paquete emitido antes de tiempo en byte {i}");
            } else {
                assert_eq!(out.len(), 1);
                assert_eq!(out[0], pkt.to_vec());
            }
        }
        assert_eq!(f.buffered(), 0);
    }

    #[test]
    fn concatenated_packets_in_one_push() {
        let login = TPacketCGLogin::new("test", "1234").to_bytes(); // 49B, header 1
        let select = TPacketCGPlayerSelect::new(0).to_bytes(); // 2B, header 6
        let del = TPacketCGPlayerDelete::new(3, "12345678").to_bytes(); // 10B, header 5
        let mut data = Vec::new();
        data.extend_from_slice(&login);
        data.extend_from_slice(&select);
        data.extend_from_slice(&del);

        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&data).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], login.to_vec());
        assert_eq!(out[1], select.to_vec());
        assert_eq!(out[2], del.to_vec());
        assert_eq!(f.buffered(), 0);
    }

    #[test]
    fn unknown_header_errors() {
        // garbage puro → Err
        let mut f = Framer::new(ConnectionRole::Channel);
        let err = f.push(&[0x99, 0x01, 0x02]).unwrap_err();
        assert!(matches!(err, FramingError::UnknownHeader { header: 0x99 }));

        // paquete válido seguido de garbage en el mismo push → Err
        // (los ya extraídos se descartan; el caller cierra — input.cpp:77-84)
        let mut f = Framer::new(ConnectionRole::Channel);
        let mut data = TPacketCGPlayerSelect::new(0).to_bytes().to_vec();
        data.extend_from_slice(&[0xAB, 0xCD]);
        assert!(matches!(f.push(&data), Err(FramingError::UnknownHeader { header: 0xAB })));

        // un cliente que envíe GC_PHASE (0xfd) es cerrado: la tabla C→S del
        // C++ tampoco lo registra (CPacketInfoCG, packet_info.cpp:136-236)
        let mut f = Framer::new(ConnectionRole::Auth);
        assert!(matches!(
            f.push(&TPacketGCPhase::new(phase::AUTH).to_bytes()),
            Err(FramingError::UnknownHeader { header: 0xfd })
        ));
    }

    /// CG_CHAT (3) — paquete de tamaño VARIABLE (F5.3): el WORD LE en [1..3]
    /// es el tamaño TOTAL (header 4 B + mensaje; `Packet.h:534-539` +
    /// `input_main.cpp:641-655`). El framer lo entrega completo, fragmentado o
    /// concatenado; length < 4 -> cierre (parity PHASE_CLOSE).
    #[test]
    fn cg_chat_variable_size() {
        // "hola" + NUL: length = 4 + 5 = 9 (parity `sizeof + iTextLen`).
        let mut pkt = vec![header::CG_CHAT];
        pkt.extend_from_slice(&9u16.to_le_bytes());
        pkt.push(0); // CHAT_TYPE_TALKING
        pkt.extend_from_slice(b"hola\0");
        assert_eq!(pkt.len(), 9);

        // completo en un push.
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&pkt).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], pkt);
        assert_eq!(f.buffered(), 0);

        // fragmentado byte a byte -> completo solo al final.
        let mut f = Framer::new(ConnectionRole::Channel);
        for (i, b) in pkt.iter().enumerate() {
            let out = f.push(&[*b]).unwrap();
            if i + 1 < pkt.len() {
                assert!(out.is_empty(), "emitido antes en byte {i}");
            } else {
                assert_eq!(out, vec![pkt.clone()]);
            }
        }

        // dos chats concatenados en un solo read -> 2 paquetes.
        let mut two = pkt.clone();
        two.extend_from_slice(&pkt);
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&two).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], pkt);
        assert_eq!(out[1], pkt);

        // length inválido (< 4) -> cierre (parity PHASE_CLOSE).
        let mut bad = vec![header::CG_CHAT];
        bad.extend_from_slice(&2u16.to_le_bytes());
        bad.push(0);
        let mut f = Framer::new(ConnectionRole::Channel);
        assert!(matches!(f.push(&bad), Err(FramingError::UnknownHeader { header: header::CG_CHAT })));

        // el rol Auth NO lo acepta (flujo corto — tabla común).
        let mut f = Framer::new(ConnectionRole::Auth);
        assert!(matches!(
            f.push(&pkt),
            Err(FramingError::UnknownHeader { header: header::CG_CHAT })
        ));
    }

    /// CG_WHISPER (19) — paquete de tamaño VARIABLE (gap-lane-C): el WORD LE
    /// en [1..3] es el tamaño TOTAL (header 28 B fijos — TPacketCGWhisper
    /// BYTE+WORD+szNameTo[25] — + mensaje; `Packet.h:540-546` +
    /// `input_main.cpp:273-286`). El framer lo entrega completo, fragmentado
    /// o concatenado; wSize < 28 -> cierre (parity PHASE_CLOSE).
    #[test]
    fn cg_whisper_variable_size() {
        // "hola bob" + NUL: wSize = 28 + 9 = 37 (parity `sizeof + iTextLen`).
        let mut pkt = vec![header::CG_WHISPER];
        pkt.extend_from_slice(&37u16.to_le_bytes());
        let mut name = [0u8; protocol::chat::NAME_BYTES];
        name[..3].copy_from_slice(b"Bob");
        pkt.extend_from_slice(&name);
        pkt.extend_from_slice(b"hola bob\0");
        assert_eq!(pkt.len(), 37);

        // completo en un push.
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&pkt).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], pkt);
        assert_eq!(f.buffered(), 0);

        // fragmentado byte a byte -> completo solo al final.
        let mut f = Framer::new(ConnectionRole::Channel);
        for (i, b) in pkt.iter().enumerate() {
            let out = f.push(&[*b]).unwrap();
            if i + 1 < pkt.len() {
                assert!(out.is_empty(), "emitido antes en byte {i}");
            } else {
                assert_eq!(out, vec![pkt.clone()]);
            }
        }

        // dos whispers concatenados en un solo read -> 2 paquetes.
        let mut two = pkt.clone();
        two.extend_from_slice(&pkt);
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&two).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], pkt);
        assert_eq!(out[1], pkt);

        // wSize inválido (< 28) -> cierre (parity PHASE_CLOSE).
        let mut bad = vec![header::CG_WHISPER];
        bad.extend_from_slice(&27u16.to_le_bytes());
        bad.extend_from_slice(&[0u8; 25]);
        let mut f = Framer::new(ConnectionRole::Channel);
        assert!(matches!(
            f.push(&bad),
            Err(FramingError::UnknownHeader { header: header::CG_WHISPER })
        ));

        // el rol Auth NO lo acepta (flujo corto — tabla común).
        let mut f = Framer::new(ConnectionRole::Auth);
        assert!(matches!(
            f.push(&pkt),
            Err(FramingError::UnknownHeader { header: header::CG_WHISPER })
        ));
    }

    /// CG_SHOP (50) — paquete de tamaño VARIABLE (fix bug 3, 2026-08-15):
    /// `TPacketCGShop` (2 B: header + subheader, Packet.h:641-645) + payload
    /// según subheader — END=0 (2 B), BUY=1 (+count,pos = 4 B), SELL=2
    /// (+cell = 3 B), SELL2=3 (+cell,count = 4 B) (input_main.cpp:1054-1088).
    /// El header NO está en la tabla fija (el C++ lo registra con
    /// `sizeof(TPacketCGShop)` + `iExtraLen`); el framer resuelve por
    /// subheader. Antes: UnknownHeader -> la conexión se cerraba en el primer
    /// BUY/SELL/END de la tienda.
    #[test]
    fn cg_shop_variable_size() {
        // END: 2 B.
        let end = [header::CG_SHOP, 0];
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&end).unwrap();
        assert_eq!(out, vec![end.to_vec()]);

        // BUY: 4 B (header + sub + count + pos).
        let buy = [header::CG_SHOP, 1, 1, 7];
        let mut f = Framer::new(ConnectionRole::Channel);
        assert_eq!(f.push(&buy).unwrap(), vec![buy.to_vec()]);

        // SELL: 3 B (header + sub + cell).
        let sell = [header::CG_SHOP, 2, 9];
        let mut f = Framer::new(ConnectionRole::Channel);
        assert_eq!(f.push(&sell).unwrap(), vec![sell.to_vec()]);

        // SELL2: 4 B (header + sub + cell + count).
        let sell2 = [header::CG_SHOP, 3, 9, 5];
        let mut f = Framer::new(ConnectionRole::Channel);
        assert_eq!(f.push(&sell2).unwrap(), vec![sell2.to_vec()]);

        // Fragmentado byte a byte -> completo solo al final (BUY).
        let mut f = Framer::new(ConnectionRole::Channel);
        for (i, b) in buy.iter().enumerate() {
            let out = f.push(&[*b]).unwrap();
            if i + 1 < buy.len() {
                assert!(out.is_empty(), "emitido antes en byte {i}");
            } else {
                assert_eq!(out, vec![buy.to_vec()]);
            }
        }

        // BUY + END concatenados en un solo read -> 2 paquetes.
        let mut two = buy.to_vec();
        two.extend_from_slice(&end);
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&two).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], buy.to_vec());
        assert_eq!(out[1], end.to_vec());

        // El rol Auth NO lo acepta (flujo corto).
        let mut f = Framer::new(ConnectionRole::Auth);
        assert!(matches!(
            f.push(&buy),
            Err(FramingError::UnknownHeader { header: header::CG_SHOP })
        ));
    }

    /// Los headers de 1 byte se parsean como paquetes de 1 byte en el rol
    /// Channel: `CG_ENTERGAME` (10, entrada al mundo — F4) y `CG_STATE_CHECKER`
    /// (206, ping del selector de canales — F2).
    #[test]
    fn entergame_and_state_checker_are_1_byte_packets() {
        // CG_ENTERGAME (10): struct de un BYTE (packet.h:613-616,
        // packet_info.cpp:165); el cliente lo envía al entrar al mundo
        // (PythonNetworkStreamPhaseLoading.cpp:346)
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&[header::CG_ENTERGAME]).unwrap();
        assert_eq!(out, vec![vec![header::CG_ENTERGAME]]);
        assert_eq!(f.buffered(), 0);

        // CG_STATE_CHECKER (206): 1 BYTE (ServerStateChecker.cpp:60-61,
        // packet_info.cpp:232)
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&[header::CG_STATE_CHECKER]).unwrap();
        assert_eq!(out, vec![vec![header::CG_STATE_CHECKER]]);
        assert_eq!(f.buffered(), 0);

        // ambos concatenados en un solo read → 2 paquetes de 1 B en orden
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&[header::CG_ENTERGAME, header::CG_STATE_CHECKER]).unwrap();
        assert_eq!(
            out,
            vec![vec![header::CG_ENTERGAME], vec![header::CG_STATE_CHECKER]]
        );
        assert_eq!(f.buffered(), 0);
    }

    #[test]
    fn login3_role_sizes() {
        let auth_pkt = TPacketCGLogin3::new_auth("test", "1234", [1, 2, 3, 4], "es").to_bytes_auth();
        assert_eq!(auth_pkt.len(), 68);

        // Rol AUTH: 68 B → un solo paquete, incluso fragmentado en 65+3
        let mut f = Framer::new(ConnectionRole::Auth);
        assert!(f.push(&auth_pkt[..65]).unwrap().is_empty()); // incompleto
        assert_eq!(f.buffered(), 65);
        let out = f.push(&auth_pkt[65..]).unwrap();
        assert_eq!(out, vec![auth_pkt.to_vec()]);
        assert_eq!(f.buffered(), 0);

        // Rol CHANNEL: 65 B → un solo paquete
        let chan_pkt =
            TPacketCGLogin3::new_channel("test", "1234", [1, 2, 3, 4]).to_bytes_channel();
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&chan_pkt).unwrap();
        assert_eq!(out, vec![chan_pkt.to_vec()]);

        // Rol CHANNEL recibiendo 68 B (misconfig/abuso): los 65 primeros se
        // extraen como Login3 de canal y los 3 restantes ("es\0") se
        // re-parsean como header → desconocido → cierre. Mismo comportamiento
        // que el C++ (la tabla del canal dice 65 y el sobrante re-entra al
        // loop de Process, input.cpp:70-84).
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&auth_pkt[..65]).unwrap();
        assert_eq!(out, vec![chan_pkt.to_vec()]);
        assert_eq!(f.buffered(), 0);
        assert!(matches!(
            f.push(&auth_pkt[65..]),
            Err(FramingError::UnknownHeader { header: b'e' })
        ));
    }

    // ------------------------------------------------------------------
    // F1.4: keepalives intercalados no rompen el parseo del flujo
    // ------------------------------------------------------------------

    /// Secuencia real cliente→servidor del login de canal (spec §4b) con
    /// keepalives intercalados (spec §7): handshake (0xff) → time sync (0xfc)
    /// → pong (0xfe) → login3 (0x6f, 65B).
    ///
    /// DESVIACIÓN documentada del criterio del plan ("phase(2B) → handshake →
    /// time sync intercalado → login3"): `GC_PHASE` (0xfd) es servidor→cliente
    /// (`packet.h:105`; el cliente solo lo RECIBE, `UserInterface/Packet.h:301`)
    /// — ningún cliente lo envía y la tabla C→S no lo registra. Un framer C→S
    /// lo rechaza como header desconocido (cierre, igual que el C++), así que
    /// el "phase" del criterio no puede ser el primer paquete de un framer
    /// cliente→servidor. La secuencia C→S real del login es la de abajo, y el
    /// criterio de aceptación (los keepalives no rompen el flujo, 4 paquetes en
    /// orden) se cumple con ella.
    #[test]
    fn keepalives_do_not_break_login_flow() {
        let handshake = TPacketCGHandshake::new(0x1122_3344, 0x0102_0304, 0).to_bytes();
        let mut timesync = [0u8; TPacketCGHandshake::SIZE];
        timesync[0] = header::CG_TIME_SYNC;
        let pong = [header::CG_PONG];
        let login3 = TPacketCGLogin3::new_channel("test", "1234", [1, 2, 3, 4]).to_bytes_channel();

        let mut data = Vec::new();
        data.extend_from_slice(&handshake);
        data.extend_from_slice(&timesync);
        data.extend_from_slice(&pong);
        data.extend_from_slice(&login3);

        // varios paquetes en un solo read
        let mut f = Framer::new(ConnectionRole::Channel);
        let out = f.push(&data).unwrap();
        assert_eq!(out.len(), 4);
        assert_eq!(out[0], handshake.to_vec());
        assert_eq!(out[1], timesync.to_vec());
        assert_eq!(out[2], pong.to_vec());
        assert_eq!(out[3], login3.to_vec());
        assert_eq!(f.buffered(), 0);

        // la misma secuencia entregada byte a byte emite los mismos 4 paquetes
        let mut f = Framer::new(ConnectionRole::Channel);
        let mut seen = Vec::new();
        for b in &data {
            seen.extend(f.push(&[*b]).unwrap());
        }
        assert_eq!(seen.len(), 4);
        assert_eq!(seen[0], handshake.to_vec());
        assert_eq!(seen[1], timesync.to_vec());
        assert_eq!(seen[2], pong.to_vec());
        assert_eq!(seen[3], login3.to_vec());
        assert_eq!(f.buffered(), 0);
    }

    // ------------------------------------------------------------------
    // Flujo pull (next_packet) y helper servidor→cliente
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn next_packet_fragmented_delivery() {
        use tokio::io::duplex;
        let (mut writer, mut reader) = duplex(256);
        let handshake = TPacketCGHandshake::new(7, 8, 9).to_bytes();
        let login3 = TPacketCGLogin3::new_channel("test", "1234", [1, 2, 3, 4]).to_bytes_channel();

        let writer_task = tokio::spawn(async move {
            // entrega fragmentada: 1 byte, luego 12, luego el login3 completo
            writer.write_all(&handshake[..1]).await.unwrap();
            writer.write_all(&handshake[1..]).await.unwrap();
            writer.write_all(&login3).await.unwrap();
            drop(writer); // EOF
        });

        let mut f = Framer::new(ConnectionRole::Channel);
        let p1 = f.next_packet(&mut reader).await.unwrap();
        assert_eq!(p1, handshake.to_vec());
        let p2 = f.next_packet(&mut reader).await.unwrap();
        assert_eq!(p2, login3.to_vec());
        // EOF limpio tras los dos paquetes
        assert!(matches!(f.next_packet(&mut reader).await, Err(FramingError::Eof)));
        writer_task.await.unwrap();
    }

    #[tokio::test]
    async fn next_packet_eof_with_partial_data() {
        use tokio::io::duplex;
        let (mut writer, mut reader) = duplex(256);
        let login3 = TPacketCGLogin3::new_channel("test", "1234", [1, 2, 3, 4]).to_bytes_channel();
        let writer_task = tokio::spawn(async move {
            writer.write_all(&login3[..10]).await.unwrap(); // a medio paquete
            drop(writer);
        });

        let mut f = Framer::new(ConnectionRole::Channel);
        assert!(matches!(
            f.next_packet(&mut reader).await,
            Err(FramingError::UnexpectedEof { buffered: 10 })
        ));
        writer_task.await.unwrap();
    }

    #[tokio::test]
    async fn read_exact_size_reads_protocol_packets() {
        use tokio::io::duplex;
        let (mut writer, mut reader) = duplex(256);
        let phase_pkt = TPacketGCPhase::new(phase::LOGIN).to_bytes();
        let hs = TPacketGCHandshake::new(1, 2, 3).to_bytes();
        writer.write_all(&phase_pkt).await.unwrap();
        writer.write_all(&hs).await.unwrap();
        // tamaños desde el crate protocol, no de una tabla
        assert_eq!(
            read_exact_size(&mut reader, TPacketGCPhase::SIZE).await.unwrap(),
            phase_pkt.to_vec()
        );
        assert_eq!(
            read_exact_size(&mut reader, TPacketGCHandshake::SIZE).await.unwrap(),
            hs.to_vec()
        );
        // EOF a medio paquete → error io
        drop(writer);
        let err = read_exact_size(&mut reader, 4).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }
}
