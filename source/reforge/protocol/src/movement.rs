//! F5.1: el paquete de MOVIMIENTO del cliente — `CG_MOVE` (0x07).
//!
//! `TPacketCGMove` (16 B packed — **layout del CLIENTE**, `Packet.h:677-686`):
//! `BYTE bHeader` + `BYTE bFunc` + `BYTE bArg` + `BYTE bRot` + `LONG lX` +
//! `LONG lY` + `DWORD dwTime` (el reloj del cliente en ms — lo usa el
//! anti-speedhack del server, `input_main.cpp:1494-1516`).
//!
//! NO hay ack para el jugador local: el server responde `TPacketGCMove`
//! (S→C, header 3 — el paquete con dwVID del movimiento) SOLO a los
//! observadores (`PacketAround(&pack, ..., ch)` — el `ch` queda EXCLUIDO,
//! `input_main.cpp:1576-1588`); el cliente mueve su personaje localmente.

use crate::{ProtocolError, Result, rd_u32};

/// `TPacketCGMove` (16 B, header 7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketCGMove {
    pub header: u8,
    /// `FUNC_MOVE`=1 (mover), `FUNC_ATTACK`=2, `FUNC_COMBO`=3, `FUNC_SKILL`=
    /// 0x80|motion (input_main.cpp:1450-1460).
    pub b_func: u8,
    pub b_arg: u8,
    /// Dirección en pasos de 5 grados (el server hace `SetRotation(bRot * 5)`).
    pub b_rot: u8,
    /// Posición destino en UNITS (el cliente divide por 100).
    pub x: i32,
    pub y: i32,
    /// Reloj del cliente en ms (anti-speedhack del server).
    pub dw_time: u32,
}

impl TPacketCGMove {
    /// 1 + 1 + 1 + 1 + 4 + 4 + 4 = 16 (packed).
    pub const SIZE: usize = 16;
    pub const HEADER: u8 = 7;
    /// `FUNC_MOVE` (input_main.cpp:1451-1452 — el movimiento real).
    pub const FUNC_MOVE: u8 = 1;
    /// `FUNC_ATTACK` (input_main.cpp:1453).
    pub const FUNC_ATTACK: u8 = 2;
    /// `FUNC_SKILL = 0x80` (input_main.cpp:1459) — el motion va en los bits bajos.
    pub const FUNC_SKILL: u8 = 0x80;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            b_func: data[1],
            b_arg: data[2],
            b_rot: data[3],
            x: i32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            y: i32::from_le_bytes([data[8], data[9], data[10], data[11]]),
            dw_time: rd_u32(data, 12),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1] = self.b_func;
        b[2] = self.b_arg;
        b[3] = self.b_rot;
        b[4..8].copy_from_slice(&self.x.to_le_bytes());
        b[8..12].copy_from_slice(&self.y.to_le_bytes());
        b[12..16].copy_from_slice(&self.dw_time.to_le_bytes());
        b
    }
}

/// `TPacketGCMove` (24 B, header 3 — S→C): el movimiento de un PERSONAJE
/// (PC o NPC) hacia sus observadores. `Packet.h:1912-1923` +
/// `EncodeMovePacket` (char.cpp:825-836):
/// `BYTE bHeader + BYTE bFunc + BYTE bArg + BYTE bRot + DWORD dwVID +
/// LONG lX + LONG lY + DWORD dwTime + DWORD dwDuration`.
///
/// El jugador local NO recibe su propio move (el C++ mueve al cliente
/// localmente — `PacketAround(..., ch)` excluye a `ch`, input_main.cpp:
/// 1576-1588); los MOBS sí se mandan al jugador (F5 NPC AI).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCMove {
    pub header: u8,
    /// `FUNC_WAIT`=0, `FUNC_MOVE`=1, `FUNC_ATTACK`=2, `FUNC_SKILL`=0x80
    /// (packet.h:565-572).
    pub b_func: u8,
    pub b_arg: u8,
    /// Dirección en pasos de 5 grados (`GetRotation()/5`, char.cpp:2800).
    pub b_rot: u8,
    pub vid: u32,
    /// Posición destino en UNITS.
    pub x: i32,
    pub y: i32,
    /// Reloj del server (get_dword_time — now32 del canal).
    pub dw_time: u32,
    /// Duración del movimiento en ms (el cliente interpola).
    pub dw_duration: u32,
}

impl TPacketGCMove {
    /// 1+1+1+1+4+4+4+4+4 = 24 (packed).
    pub const SIZE: usize = 24;
    pub const HEADER: u8 = 3;
    /// `FUNC_WAIT` (packet.h:565).
    pub const FUNC_WAIT: u8 = 0;
    /// `FUNC_MOVE` (packet.h:566).
    pub const FUNC_MOVE: u8 = 1;
    /// `FUNC_ATTACK` (packet.h:567).
    pub const FUNC_ATTACK: u8 = 2;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            b_func: data[1],
            b_arg: data[2],
            b_rot: data[3],
            vid: rd_u32(data, 4),
            x: i32::from_le_bytes([data[8], data[9], data[10], data[11]]),
            y: i32::from_le_bytes([data[12], data[13], data[14], data[15]]),
            dw_time: rd_u32(data, 16),
            dw_duration: rd_u32(data, 20),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1] = self.b_func;
        b[2] = self.b_arg;
        b[3] = self.b_rot;
        b[4..8].copy_from_slice(&self.vid.to_le_bytes());
        b[8..12].copy_from_slice(&self.x.to_le_bytes());
        b[12..16].copy_from_slice(&self.y.to_le_bytes());
        b[16..20].copy_from_slice(&self.dw_time.to_le_bytes());
        b[20..24].copy_from_slice(&self.dw_duration.to_le_bytes());
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_size_and_parse() {
        assert_eq!(TPacketCGMove::SIZE, 16, "1+1+1+1+4+4+4 (Packet.h:677-686)");
        let mut raw = [0u8; 16];
        raw[0] = 7;
        raw[1] = TPacketCGMove::FUNC_MOVE;
        raw[3] = 4; // bRot
        raw[4..8].copy_from_slice(&969600i32.to_le_bytes());
        raw[8..12].copy_from_slice(&278400i32.to_le_bytes());
        raw[12..16].copy_from_slice(&12345u32.to_le_bytes());
        let p = TPacketCGMove::from_bytes(&raw).unwrap();
        assert_eq!(p.header, 7);
        assert_eq!(p.b_func, 1);
        assert_eq!(p.b_rot, 4);
        assert_eq!((p.x, p.y), (969600, 278400));
        assert_eq!(p.dw_time, 12345);
        assert_eq!(p.to_bytes(), raw, "roundtrip byte-exacto");
    }

    #[test]
    fn bad_lengths_error() {
        for len in [0usize, 4, 15, 17, 32] {
            assert!(
                TPacketCGMove::from_bytes(&vec![0u8; len]).is_err(),
                "len {len}"
            );
        }
    }

    /// El paquete S→C del movimiento de mobs (F5 NPC AI): 24 B packed,
    /// layout `Packet.h:1912-1923` (header + func + arg + rot + vid + x + y +
    /// time + duration).
    #[test]
    fn gc_move_wire_size_and_parse() {
        assert_eq!(TPacketGCMove::SIZE, 24, "1+1+1+1+4+4+4+4+4");
        let mut raw = [0u8; 24];
        raw[0] = TPacketGCMove::HEADER; // 3
        raw[1] = TPacketGCMove::FUNC_MOVE;
        raw[3] = 12; // bRot
        raw[4..8].copy_from_slice(&10_001u32.to_le_bytes()); // vid del mob
        raw[8..12].copy_from_slice(&969_700i32.to_le_bytes());
        raw[12..16].copy_from_slice(&278_400i32.to_le_bytes());
        raw[16..20].copy_from_slice(&55_000u32.to_le_bytes()); // dwTime
        raw[20..24].copy_from_slice(&500u32.to_le_bytes()); // dwDuration ms
        let p = TPacketGCMove::from_bytes(&raw).unwrap();
        assert_eq!(p.header, 3);
        assert_eq!(p.b_func, TPacketGCMove::FUNC_MOVE);
        assert_eq!(p.b_rot, 12);
        assert_eq!(p.vid, 10_001);
        assert_eq!((p.x, p.y), (969_700, 278_400));
        assert_eq!(p.dw_time, 55_000);
        assert_eq!(p.dw_duration, 500);
        assert_eq!(p.to_bytes(), raw, "roundtrip byte-exacto");
    }

    #[test]
    fn gc_move_bad_lengths_error() {
        for len in [0usize, 3, 23, 25] {
            assert!(
                TPacketGCMove::from_bytes(&vec![0u8; len]).is_err(),
                "len {len}"
            );
        }
    }
}
