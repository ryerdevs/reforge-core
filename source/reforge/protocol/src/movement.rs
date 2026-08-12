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

use crate::{rd_u32, Result, ProtocolError};

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
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
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
            assert!(TPacketCGMove::from_bytes(&vec![0u8; len]).is_err(), "len {len}");
        }
    }
}
