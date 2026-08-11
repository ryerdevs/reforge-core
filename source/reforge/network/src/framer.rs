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
/// Subconjunto F1 de `CPacketInfoCG` (flujo de login, sequence OFF):
/// `0xff`=13, `0xfe`=1, `1`=49, `4`=34, `5`=10, `6`=2, `10`=1, `109`=52,
/// `111`=65/68, `206`=1, `0xfc`=13. Los tamaños vienen de las constantes
/// `SIZE` del crate `protocol` (no de literales duplicados); los de 1 B son
/// `sizeof(BYTE)` (sin struct en el crate).
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
/// - Sin idle timeout: el C++ tampoco lo tiene (una conexión muda queda abierta
///   hasta que el SO la cierre). **F2 debe añadir un timeout explícito.**
///
/// Los paquetes de tamaño variable del C++ (CG_CHAT, CG_TEXT, ...) NO están en
/// la tabla: su `iExtraLen` depende del contenido y quedan fuera de alcance
/// hasta la fase de juego — recibirlos cierra la conexión (mismo
/// comportamiento que un header desconocido, seguro por defecto).
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
        header::CG_STATE_CHECKER => 1, // 206, sizeof(BYTE) — ping selector de canales (packet_info.cpp:232)
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
        // LOGIN3: 65 canal / 68 auth (sufijo szLanguage[3])
        assert_eq!(
            packet_size(ConnectionRole::Channel, header::CG_LOGIN3),
            Some(TPacketCGLogin3::SIZE_CHANNEL)
        );
        assert_eq!(
            packet_size(ConnectionRole::Auth, header::CG_LOGIN3),
            Some(TPacketCGLogin3::SIZE_AUTH)
        );
        // desconocidos → None → el caller cierra la conexión (input.cpp:77-84)
        assert_eq!(packet_size(ConnectionRole::Channel, 0), None);
        assert_eq!(packet_size(ConnectionRole::Auth, 0), None);
        // GC_PHASE (0xfd) es servidor→cliente: no está en la tabla C→S
        assert_eq!(packet_size(ConnectionRole::Auth, 0xfd), None);
        assert_eq!(packet_size(ConnectionRole::Channel, 0x99), None);
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
