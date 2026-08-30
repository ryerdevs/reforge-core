//! Splitter servidor→cliente (S→C) para el bot de benchmark.
//!
//! El server NO usa prefijo de longitud (spec §2): un paquete = header BYTE +
//! payload de tamaño fijo. La tabla cubre TODOS los paquetes que el auth y el
//! canal Rust pueden enviar HOY (verificado 2026-08-13 contra
//! `server_realms/src/{auth,channel}.rs` y los structs del crate `protocol`):
//!
//! - Tamaño FIJO por tabla (los `SIZE` de los structs del protocolo — nunca
//!   números mágicos sueltos).
//! - Tamaño VARIABLE con el tamaño TOTAL embebido (u16 LE en `[1..3]`):
//!   `GC_CHAT` (4 — `size = 9 + msg`, channel.rs), `GC_LAND_LIST` (130 —
//!   `3 + N×24`, `world::land_list_bytes`), y los legacy del auth 152/153
//!   (`7 + stream`, `protocol::legacy`).
//! - `GC_CHANNEL_LIST` (164): 152 B fijos (`auth.rs:108-112` — const local
//!   porque vive en `server_realms`, no en `protocol`).
//!
//! Un header fuera de la tabla → [`SplitError::UnknownHeader`]: el bot lo
//! reporta como DESYNC (drift de protocolo). El harness falla fuerte en vez
//! de adivinar tamaños — un desync silencioso invalidaría las métricas.

use std::io;

use protocol::combat::{GcAttack, GcDamageInfo};
use protocol::header;
use protocol::legacy::{GC_HYBRIDCRYPT_KEYS, GC_HYBRIDCRYPT_SDB, GC_PANAMA_PACK};
use protocol::movement::TPacketGCMove;
use protocol::world::{
    TPacketGCAffectAdd, TPacketGCChannel, TPacketGCCharacterDelete, TPacketGCDead,
    TPacketGCItemDelDeprecated, TPacketGCItemGroundAdd, TPacketGCItemGroundDel,
    TPacketGCItemOwnership, TPacketGCItemSet, TPacketGCItemUpdate, TPacketGCMainCharacter,
    TPacketGCPoints, TPacketGCQuickSlotAdd, TPacketGCSkillLevel, TPacketGCTime, TPacketGCWarp,
};
use protocol::{
    TPacketGCAuthSuccess, TPacketGCEmpire, TPacketGCCharacterAdd, TPacketGCCharacterAdditionalInfo,
    TPacketGCHandshake, TPacketGCLoginFailure, TPacketGCLoginKey, TPacketGCLoginSuccess,
    TPacketGCPhase,
};
use tokio::io::{AsyncRead, AsyncReadExt};

/// `GC_CHANNEL_LIST` (164) — fuente única: `protocol::header`
/// (el wire lo emite `server_realms/src/auth.rs`).
pub use protocol::header::GC_CHANNEL_LIST;
/// Tamaño fijo del 164: header + count + rates(6) + 4×36 (`auth.rs:112`).
const GC_CHANNEL_LIST_SIZE: usize = 152;

/// Error del splitter. Todos son terminales para la conexión del bot.
#[derive(Debug)]
pub enum SplitError {
    /// Header sin entrada en la tabla S→C → drift de protocolo (desync).
    UnknownHeader { header: u8 },
    /// Longitud embebida inválida (p.ej. `GC_CHAT` con `size < 9`).
    BadEmbeddedLength { header: u8, size: usize },
    /// EOF limpio del server sin bytes pendientes.
    Eof,
    /// EOF con bytes a medio paquete.
    UnexpectedEof { buffered: usize },
    /// Error de I/O del socket.
    Io(io::Error),
}

impl core::fmt::Display for SplitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SplitError::UnknownHeader { header } => {
                write!(f, "server sent unknown packet header 0x{header:02x} (protocol drift — desync)")
            }
            SplitError::BadEmbeddedLength { header, size } => {
                write!(f, "header 0x{header:02x} embedded length {size} out of range")
            }
            SplitError::Eof => write!(f, "server closed the connection"),
            SplitError::UnexpectedEof { buffered } => {
                write!(f, "server closed with {buffered} unparsed bytes buffered")
            }
            SplitError::Io(e) => write!(f, "socket io error: {e}"),
        }
    }
}

impl std::error::Error for SplitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SplitError::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// Tamaño total (header + payload) de un paquete S→C de tamaño FIJO, o
/// `None` si el header no está en la tabla (o es variable — ver
/// [`embedded_size`]).
fn fixed_size(hdr: u8) -> Option<usize> {
    Some(match hdr {
        header::GC_PHASE => TPacketGCPhase::SIZE, // 0xfd, 2
        header::GC_HANDSHAKE => TPacketGCHandshake::SIZE, // 0xff, 13
        header::GC_AUTH_SUCCESS => TPacketGCAuthSuccess::SIZE, // 150, 6
        header::GC_LOGIN_FAILURE => TPacketGCLoginFailure::SIZE, // 7, 10
        header::GC_LOGIN_KEY => TPacketGCLoginKey::SIZE, // 118, 5
        header::GC_EMPIRE => TPacketGCEmpire::SIZE, // 90, 2
        header::GC_LOGIN_SUCCESS_NEWSLOT => TPacketGCLoginSuccess::SIZE, // 32, 449
        header::GC_CHARACTER_ADD => TPacketGCCharacterAdd::SIZE, // 1, 37
        header::GC_CHAR_ADDITIONAL_INFO => TPacketGCCharacterAdditionalInfo::SIZE, // 136, 70
        header::GC_PING => 1, // 44, sizeof(BYTE)
        TPacketGCMainCharacter::HEADER => TPacketGCMainCharacter::SIZE, // 15, 47
        TPacketGCPoints::HEADER => TPacketGCPoints::SIZE, // 16, 1021
        TPacketGCSkillLevel::HEADER => TPacketGCSkillLevel::SIZE, // 76, 1531
        TPacketGCQuickSlotAdd::HEADER => TPacketGCQuickSlotAdd::SIZE, // 28, 4
        TPacketGCItemSet::HEADER => TPacketGCItemSet::SIZE, // 21, 51
        TPacketGCAffectAdd::HEADER => TPacketGCAffectAdd::SIZE, // 126, 22
        TPacketGCTime::HEADER => TPacketGCTime::SIZE, // 106, 5
        TPacketGCChannel::HEADER => TPacketGCChannel::SIZE, // 121, 2
        TPacketGCMove::HEADER => TPacketGCMove::SIZE, // 3, 24
        TPacketGCItemGroundAdd::HEADER => TPacketGCItemGroundAdd::SIZE, // 26, 58
        TPacketGCItemGroundDel::HEADER => TPacketGCItemGroundDel::SIZE, // 27, 5
        TPacketGCItemOwnership::HEADER => TPacketGCItemOwnership::SIZE, // 31, 30
        TPacketGCItemUpdate::HEADER => TPacketGCItemUpdate::SIZE, // 25, 38
        TPacketGCItemDelDeprecated::HEADER => TPacketGCItemDelDeprecated::SIZE, // 20, 42
        TPacketGCDead::HEADER => TPacketGCDead::SIZE, // 14, 5
        TPacketGCCharacterDelete::HEADER => TPacketGCCharacterDelete::SIZE, // 2, 5
        TPacketGCWarp::HEADER => TPacketGCWarp::SIZE, // 65, 15
        GcAttack::HEADER => GcAttack::SIZE, // 12, 10
        GcDamageInfo::HEADER => GcDamageInfo::SIZE, // 135, 10
        GC_PANAMA_PACK => 289, // 151 legacy (1 + 256 + 32)
        GC_CHANNEL_LIST => GC_CHANNEL_LIST_SIZE, // 164, 152 (auth F5)
        _ => return None,
    })
}

/// Headers de tamaño VARIABLE: el u16 LE en `[1..3]` es el tamaño TOTAL.
/// (header, mínimo válido del campo).
const EMBEDDED_SIZE: &[(u8, usize)] = &[
    (header::GC_CHAT, 9), // header+size+type+dwVID+bEmpire (Packet.h:1336-1343)
    (130, 3), // GC_LAND_LIST: 3 + N×24 (world::land_list_bytes)
    (GC_HYBRIDCRYPT_KEYS, 7), // 152: 7 + stream (legacy.rs)
    (GC_HYBRIDCRYPT_SDB, 7), // 153: 7 + stream (legacy.rs)
];

/// Headers donde el u16 LE en `[1..3]` es la longitud del PAYLOAD (no del
/// total): el paquete completo mide `payload_len + 3` (header + campo).
/// `GC_LOCALE` (140): `0x8c + u16 payload_len + u8 chunk_flag + chunk`
/// (`protocol::locale.rs` — push chunked al conectar, commit `287e414`).
const PAYLOAD_LEN_SIZE: &[(u8, usize)] = &[
    (header::GC_LOCALE, 1), // mínimo: el byte chunk_flag
];

/// Fragmenta el flujo S→C en paquetes completos (equivalente cliente del
/// `network::Framer` — misma semántica: paquetes partidos en varios reads y
/// varios paquetes en un read).
#[derive(Default)]
pub struct Splitter {
    buf: Vec<u8>,
}

impl Splitter {
    pub fn new() -> Self {
        Self { buf: Vec::with_capacity(512) }
    }

    /// Bytes pendientes de formar un paquete completo.
    #[allow(dead_code)] // API simétrica al network::Framer — usada por los tests
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// Empuja bytes (un `read` del socket) y devuelve TODOS los paquetes
    /// completos presentes, en orden. Los bytes incompletos quedan
    /// bufferizados.
    #[allow(dead_code)] // usada por los tests (el flujo del bot usa `next`)
    pub fn push(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>, SplitError> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        while let Some(pkt) = self.try_extract()? {
            out.push(pkt);
        }
        Ok(out)
    }

    /// Extrae UN paquete completo del stream, leyendo del socket todo lo que
    /// falte (flujo pull). EOF limpio → [`SplitError::Eof`]; EOF con bytes a
    /// medio paquete → [`SplitError::UnexpectedEof`].
    pub async fn next<R: AsyncRead + Unpin>(&mut self, reader: &mut R) -> Result<Vec<u8>, SplitError> {
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(pkt) = self.try_extract()? {
                return Ok(pkt);
            }
            let n = reader.read(&mut chunk).await.map_err(SplitError::Io)?;
            if n == 0 {
                return if self.buf.is_empty() {
                    Err(SplitError::Eof)
                } else {
                    Err(SplitError::UnexpectedEof { buffered: self.buf.len() })
                };
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    fn try_extract(&mut self) -> Result<Option<Vec<u8>>, SplitError> {
        if self.buf.is_empty() {
            return Ok(None);
        }
        let hdr = self.buf[0];
        if let Some((_, min)) = PAYLOAD_LEN_SIZE.iter().find(|(h, _)| *h == hdr) {
            if self.buf.len() < 3 {
                return Ok(None); // aún no está el campo de tamaño
            }
            let payload = u16::from_le_bytes([self.buf[1], self.buf[2]]) as usize;
            if payload < *min {
                return Err(SplitError::BadEmbeddedLength { header: hdr, size: payload });
            }
            let total = payload + 3; // header + campo de longitud
            if self.buf.len() < total {
                return Ok(None);
            }
            return Ok(Some(self.buf.drain(..total).collect()));
        }
        if let Some((_, min)) = EMBEDDED_SIZE.iter().find(|(h, _)| *h == hdr) {
            if self.buf.len() < 3 {
                return Ok(None); // aún no está el campo de tamaño
            }
            let total = u16::from_le_bytes([self.buf[1], self.buf[2]]) as usize;
            if total < *min {
                return Err(SplitError::BadEmbeddedLength { header: hdr, size: total });
            }
            if self.buf.len() < total {
                return Ok(None);
            }
            return Ok(Some(self.buf.drain(..total).collect()));
        }
        let Some(size) = fixed_size(hdr) else {
            return Err(SplitError::UnknownHeader { header: hdr });
        };
        if self.buf.len() < size {
            return Ok(None);
        }
        Ok(Some(self.buf.drain(..size).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    fn chat_pkt(msg: &[u8]) -> Vec<u8> {
        let mut p = vec![header::GC_CHAT];
        p.extend_from_slice(&((9 + msg.len()) as u16).to_le_bytes());
        p.push(0); // type
        p.extend_from_slice(&1u32.to_le_bytes()); // dwVID
        p.push(1); // bEmpire
        p.extend_from_slice(msg);
        p
    }

    fn land_list(n: usize) -> Vec<u8> {
        let mut p = vec![130u8];
        p.extend_from_slice(&((3 + n * 24) as u16).to_le_bytes());
        p.extend(vec![0u8; n * 24]);
        p
    }

    #[test]
    fn fixed_sizes_cover_the_channel_stream() {
        // Los paquetes que el canal/auth envían HOY (channel.rs/auth.rs) —
        // cada tamaño con su fuente en el crate protocol.
        assert_eq!(fixed_size(0xfd), Some(2), "GC_PHASE");
        assert_eq!(fixed_size(0xff), Some(13), "GC_HANDSHAKE");
        assert_eq!(fixed_size(150), Some(6), "GC_AUTH_SUCCESS");
        assert_eq!(fixed_size(7), Some(10), "GC_LOGIN_FAILURE");
        assert_eq!(fixed_size(118), Some(5), "GC_LOGIN_KEY");
        assert_eq!(fixed_size(90), Some(2), "GC_EMPIRE");
        assert_eq!(fixed_size(32), Some(449), "GC_LOGIN_SUCCESS_NEWSLOT");
        assert_eq!(fixed_size(1), Some(37), "GC_CHARACTER_ADD");
        assert_eq!(fixed_size(136), Some(70), "GC_CHAR_ADDITIONAL_INFO");
        assert_eq!(fixed_size(44), Some(1), "GC_PING");
        assert_eq!(fixed_size(15), Some(47), "GC_MAIN_CHARACTER");
        assert_eq!(fixed_size(16), Some(1021), "GC_POINTS");
        assert_eq!(fixed_size(76), Some(1531), "GC_SKILL_LEVEL");
        assert_eq!(fixed_size(28), Some(4), "GC_QUICKSLOT_ADD");
        assert_eq!(fixed_size(21), Some(51), "GC_ITEM_SET");
        assert_eq!(fixed_size(header::GC_AFFECT_ADD), Some(22), "GC_AFFECT_ADD");
        assert_eq!(fixed_size(106), Some(5), "GC_TIME");
        assert_eq!(fixed_size(121), Some(2), "GC_CHANNEL");
        assert_eq!(fixed_size(3), Some(24), "GC_MOVE");
        assert_eq!(fixed_size(header::GC_ITEM_GROUND_ADD), Some(58), "GC_ITEM_GROUND_ADD");
        assert_eq!(fixed_size(27), Some(5), "GC_ITEM_GROUND_DEL");
        assert_eq!(fixed_size(31), Some(30), "GC_ITEM_OWNERSHIP");
        assert_eq!(fixed_size(25), Some(38), "GC_ITEM_UPDATE");
        assert_eq!(fixed_size(20), Some(42), "GC_ITEM_DEL (deprecated)");
        assert_eq!(fixed_size(14), Some(5), "GC_DEAD");
        assert_eq!(fixed_size(2), Some(5), "GC_CHARACTER_DEL");
        assert_eq!(fixed_size(65), Some(15), "GC_WARP");
        assert_eq!(fixed_size(12), Some(10), "GC_ATTACK");
        assert_eq!(fixed_size(135), Some(10), "GC_DAMAGE_INFO");
        assert_eq!(fixed_size(151), Some(289), "GC_PANAMA_PACK legacy");
        assert_eq!(fixed_size(164), Some(152), "GC_CHANNEL_LIST (auth.rs:112)");
        // Los variables NO están en la tabla fija.
        assert_eq!(fixed_size(header::GC_CHAT), None);
        assert_eq!(fixed_size(130), None);
        assert_eq!(fixed_size(header::GC_LOCALE), None); // sobre payload_len
        assert_eq!(fixed_size(162), None); // CG_QUERY (C→S, datachannel)
        assert_eq!(fixed_size(163), None); // GC_RESPONSE (sin longitud embebida)
        // Headers C→S (o ajenos) → None → desync del bot.
        assert_eq!(fixed_size(6), None);
        assert_eq!(fixed_size(0x6f), None);
        assert_eq!(fixed_size(0), None);
    }

    #[test]
    fn variable_packets_with_embedded_size() {
        let chat = chat_pkt(b"hola");
        assert_eq!(chat.len(), 13);
        let mut sp = Splitter::new();
        let out = sp.push(&chat).unwrap();
        assert_eq!(out, vec![chat.clone()]);

        let lands = land_list(3);
        assert_eq!(lands.len(), 75);
        let mut sp = Splitter::new();
        let out = sp.push(&lands).unwrap();
        assert_eq!(out, vec![lands.clone()]);

        // 152/153 legacy: header + size + i32 len + stream.
        let mut legacy = vec![GC_HYBRIDCRYPT_KEYS];
        legacy.extend_from_slice(&(7u16 + 10).to_le_bytes());
        legacy.extend_from_slice(&10i32.to_le_bytes());
        legacy.extend_from_slice(&[0xabu8; 10]);
        assert_eq!(legacy.len(), 17);
        let mut sp = Splitter::new();
        assert_eq!(sp.push(&legacy).unwrap(), vec![legacy]);
    }

    #[test]
    fn locale_push_chunked_envelope() {
        // GC_LOCALE (140): 0x8c + u16 payload_len + flag + chunk — el u16 es
        // la longitud del PAYLOAD, no del total (protocol::locale.rs).
        let mut locale = vec![header::GC_LOCALE];
        locale.extend_from_slice(&(4u16).to_le_bytes()); // payload_len
        locale.push(0); // chunk_flag = final
        locale.extend_from_slice(b"abc"); // 4 B de payload con el flag
        assert_eq!(locale.len(), 7); // 3 + 4
        let mut sp = Splitter::new();
        assert_eq!(sp.push(&locale).unwrap(), vec![locale.clone()]);

        // Fragmentado byte a byte → un único paquete en el último byte.
        let mut sp = Splitter::new();
        for (i, b) in locale.iter().enumerate() {
            let out = sp.push(&[*b]).unwrap();
            assert_eq!(out.len(), usize::from(i + 1 == locale.len()), "byte {i}");
        }
        assert_eq!(sp.buffered(), 0);

        // payload_len 0 → error (mínimo 1: el chunk_flag).
        let mut bad = vec![header::GC_LOCALE];
        bad.extend_from_slice(&0u16.to_le_bytes());
        let mut sp = Splitter::new();
        assert!(matches!(
            sp.push(&bad),
            Err(SplitError::BadEmbeddedLength { header: header::GC_LOCALE, size: 0 })
        ));
    }

    #[test]
    fn fragmentation_and_concatenation() {
        let chat = chat_pkt(b"hola");
        let pong = [44u8]; // GC_PING
        let mut data = chat.clone();
        data.extend_from_slice(&pong);
        data.extend_from_slice(&chat);
        // Todo en un push → 3 paquetes.
        let mut sp = Splitter::new();
        let out = sp.push(&data).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], chat);
        assert_eq!(out[1], pong.to_vec());
        assert_eq!(out[2], chat);
        // Byte a byte → cada paquete se emite EXACTAMENTE en su último byte:
        // chat (13 B) en el byte 12, pong (1 B) en el 13, chat en el 26.
        let mut sp = Splitter::new();
        for (i, b) in data.iter().enumerate() {
            let out = sp.push(&[*b]).unwrap();
            let n = i + 1;
            let expected = usize::from(n == 13) + usize::from(n == 14) + usize::from(n == 27);
            assert_eq!(out.len(), expected, "byte {i}");
        }
        assert_eq!(sp.buffered(), 0);
    }

    #[test]
    fn embedded_size_partial_then_complete() {
        let lands = land_list(2);
        let mut sp = Splitter::new();
        assert!(sp.push(&lands[..3]).unwrap().is_empty(), "solo el header + size");
        assert!(sp.push(&lands[3..10]).unwrap().is_empty(), "elementos a medio");
        let out = sp.push(&lands[10..]).unwrap();
        assert_eq!(out, vec![lands]);
        assert_eq!(sp.buffered(), 0);
    }

    #[test]
    fn bad_embedded_length_errors() {
        // GC_CHAT con size < 9 → error (desync).
        let mut bad = vec![header::GC_CHAT];
        bad.extend_from_slice(&5u16.to_le_bytes());
        let mut sp = Splitter::new();
        assert!(matches!(
            sp.push(&bad),
            Err(SplitError::BadEmbeddedLength { header: header::GC_CHAT, size: 5 })
        ));
        // GC_LAND_LIST con size 1 → error.
        let mut bad = vec![130u8];
        bad.extend_from_slice(&1u16.to_le_bytes());
        let mut sp = Splitter::new();
        assert!(matches!(sp.push(&bad), Err(SplitError::BadEmbeddedLength { header: 130, size: 1 })));
    }

    #[test]
    fn unknown_header_errors() {
        // 0x60 no está en la tabla S→C (ni fijo ni embebido) → desync.
        let mut sp = Splitter::new();
        let err = sp.push(&[0x60, 0x01]).unwrap_err();
        assert!(matches!(err, SplitError::UnknownHeader { header: 0x60 }));
        // GC_PHASE es S→C; un C→S (6 = CG_CHARACTER_SELECT) aquí es desync.
        let mut sp = Splitter::new();
        assert!(matches!(sp.push(&[6u8, 0]), Err(SplitError::UnknownHeader { header: 6 })));
    }

    #[tokio::test]
    async fn next_pull_fragmented_and_eof() {
        use tokio::io::duplex;
        let (mut writer, mut reader) = duplex(256);
        let lands = land_list(1);
        let lands_clone = lands.clone();
        let pong = [44u8];
        let writer_task = tokio::spawn(async move {
            writer.write_all(&lands_clone[..20]).await.unwrap();
            writer.write_all(&lands_clone[20..]).await.unwrap();
            writer.write_all(&pong).await.unwrap();
            drop(writer);
        });
        let mut sp = Splitter::new();
        assert_eq!(sp.next(&mut reader).await.unwrap(), lands);
        assert_eq!(sp.next(&mut reader).await.unwrap(), pong.to_vec());
        assert!(matches!(sp.next(&mut reader).await, Err(SplitError::Eof)));
        writer_task.await.unwrap();
    }

    #[tokio::test]
    async fn next_pull_eof_with_partial_data() {
        use tokio::io::duplex;
        let (mut writer, mut reader) = duplex(256);
        let lands = land_list(2);
        let lands_clone = lands.clone();
        let writer_task = tokio::spawn(async move {
            writer.write_all(&lands_clone[..10]).await.unwrap();
            drop(writer);
        });
        let mut sp = Splitter::new();
        assert!(matches!(
            sp.next(&mut reader).await,
            Err(SplitError::UnexpectedEof { buffered: 10 })
        ));
        writer_task.await.unwrap();
    }
}
