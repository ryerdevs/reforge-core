//! F5.2: paquetes del COMBATE — `CG_ATTACK` (C→S) + `GC_ATTACK` /
//! `GC_DAMAGE_INFO` (S→C).
//!
//! Layouts **del CLIENTE** (`source/client/UserInterface/Packet.h`, región
//! `#pragma pack(1)` — `:355-356` abre, `:2737` cierra):
//! - `TPacketCGAttack` (8 B — `Packet.h:509-516`): `BYTE header` + `BYTE
//!   bType` + `DWORD dwVictimVID` + `BYTE bCRCMagicCubeProcPiece` + `BYTE
//!   bCRCMagicCubeFilePiece`. El server lo parsea con el MISMO layout
//!   (`source/server/game/src/packet.h:554-561` — el campo `dwVID` del server
//!   ES el VID de la víctima, `input_main.cpp:1658`). Los dos bytes CRC se
//!   inyectan en `SendSpecial` (`PythonNetworkStreamPhaseGame.cpp:2585-2590`)
//!   — el server los consume en `AssembleCRCMagicCube`
//!   (`input_main.cpp:1656`), irrelevantes para el combate.
//! - `TPacketGCAttack` (10 B — `Packet.h:1936-1942`): `BYTE header` + `DWORD
//!   dwVID` (atacante) + `DWORD dwVictimVID` + `BYTE bType`.
//! - `TPacketGCDamageInfo` (10 B — `Packet.h:1872-1878`; el server manda el
//!   mismo layout, `server/game/src/packet.h:2076-2082` desde
//!   `char_battle.cpp:1508-1530`): `BYTE header` + `DWORD dwVID` (víctima) +
//!   `BYTE flag` + `int damage`.
//!
//! # NOTA verificada (lección del "47B")
//!
//! El cliente S3llMetin2 v24 NO registra ni despacha `HEADER_GC_ATTACK` (12):
//! no existe el símbolo en su `Packet.h` (solo el struct, muerto) ni `Set(...)`
//! en su tabla de framing (`PythonNetworkStream.cpp`), y `RecvPhase` salta de
//! `HEADER_GC_STUN` (13) sin caso para 12 (`PythonNetworkStreamPhaseGame.cpp`
//! `:307-314`). El C++ del server NUNCA manda GC_ATTACK (solo el define
//! `packet.h:123`). La animación del atacante es PREDICCIÓN local del cliente
//! (manda `CG_ATTACK` y reproduce su motion); el feedback visible del golpe es
//! `GC_DAMAGE_INFO` (135, `RecvDamageInfoPacket` — `AddDamageEffect`,
//! `PythonNetworkStreamPhaseGame.cpp:2438-2460`; el cliente exige `damage >= 0`
//! para mostrar el número). `GC_ATTACK` se implementa igualmente (contrato
//! wire del Packet.h del cliente — ADR-0007: el server se adapta) para
//! observadores futuros.

use crate::{ProtocolError, Result, rd_u32};

/// `TPacketCGAttack` (8 B, header 2) — el ataque melee del cliente.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct CgAttack {
    pub header: u8,
    /// `uMotAttack` del cliente: 0 = ataque normal (melee); >0 = skill
    /// (motion index — `ComputeSkill`, fuera del subset F5.2).
    pub b_type: u8,
    /// `dwVictimVID` — el VID del objetivo (para el server: `packMelee->dwVID`,
    /// `input_main.cpp:1658`).
    pub victim_vid: u32,
    /// `bCRCMagicCubeProcPiece` — inyectado por `SendSpecial`; el server lo
    /// consume en `AssembleCRCMagicCube` (sin efecto en el combate).
    pub crc_proc: u8,
    /// `bCRCMagicCubeFilePiece` — idem.
    pub crc_file: u8,
}

impl CgAttack {
    /// 1 + 1 + 4 + 1 + 1 = 8 (packed).
    pub const SIZE: usize = 8;
    pub const HEADER: u8 = crate::header::CG_ATTACK;
    /// `bType == 0` = ataque normal (el único procesado por F5.2; >0 = skill).
    pub const TYPE_NORMAL: u8 = 0;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            b_type: data[1],
            victim_vid: rd_u32(data, 2),
            crc_proc: data[6],
            crc_file: data[7],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1] = self.b_type;
        b[2..6].copy_from_slice(&self.victim_vid.to_le_bytes());
        b[6] = self.crc_proc;
        b[7] = self.crc_file;
        b
    }
}

/// `TPacketGCAttack` (10 B, header 12) — la animación del ataque para los
/// observadores. El cliente v24 no lo despacha (ver nota del módulo); el C++
/// del server nunca lo manda — se implementa por contrato wire.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct GcAttack {
    pub header: u8,
    /// `dwVID` — VID del atacante.
    pub vid: u32,
    /// `dwVictimVID` — VID del objetivo.
    pub victim_vid: u32,
    /// `bType` — el mismo del `CG_ATTACK` (0 = melee normal).
    pub b_type: u8,
}

impl GcAttack {
    /// 1 + 4 + 4 + 1 = 10 (packed).
    pub const SIZE: usize = 10;
    pub const HEADER: u8 = crate::header::GC_ATTACK;

    pub fn new(vid: u32, victim_vid: u32, b_type: u8) -> Self {
        Self {
            header: Self::HEADER,
            vid,
            victim_vid,
            b_type,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            vid: rd_u32(data, 1),
            victim_vid: rd_u32(data, 5),
            b_type: data[9],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.vid.to_le_bytes());
        b[5..9].copy_from_slice(&self.victim_vid.to_le_bytes());
        b[9] = self.b_type;
        b
    }
}

/// `TPacketGCDamageInfo` (10 B, header 135) — el número/efecto del golpe. El
/// server lo manda a la víctima (si es PC) y al atacante
/// (`char_battle.cpp:1508-1530`); el cliente muestra `AddDamageEffect` solo
/// con `damage >= 0` (`PythonNetworkStreamPhaseGame.cpp:2453-2456`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct GcDamageInfo {
    pub header: u8,
    /// `dwVID` — VID de la VÍCTIMA (el número flota sobre ella).
    pub vid: u32,
    /// `flag` — bits `DamageFlag` (`char.h:120-131` / `InstanceBase.h:332-340`).
    pub flag: u8,
    /// `damage` — daño aplicado (i32 con signo; el C++ manda el daño tal cual).
    pub damage: i32,
}

impl GcDamageInfo {
    /// 1 + 4 + 1 + 4 = 10 (packed).
    pub const SIZE: usize = 10;
    pub const HEADER: u8 = crate::header::GC_DAMAGE_INFO;

    pub fn new(vid: u32, flag: u8, damage: i32) -> Self {
        Self {
            header: Self::HEADER,
            vid,
            flag,
            damage,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength {
                expected: Self::SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            header: data[0],
            vid: rd_u32(data, 1),
            flag: data[5],
            damage: i32::from_le_bytes([data[6], data[7], data[8], data[9]]),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.vid.to_le_bytes());
        b[5] = self.flag;
        b[6..10].copy_from_slice(&self.damage.to_le_bytes());
        b
    }
}

/// `DamageFlag` — bits del `flag` de `TPacketGCDamageInfo` (idénticos en
/// server `char.h:120-131` y cliente `InstanceBase.h:332-340`).
pub mod damage_flag {
    pub const NORMAL: u8 = 1 << 0;
    pub const POISON: u8 = 1 << 1;
    pub const DODGE: u8 = 1 << 2;
    pub const BLOCK: u8 = 1 << 3;
    pub const PENETRATE: u8 = 1 << 4;
    pub const CRITICAL: u8 = 1 << 5;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-exacto contra el layout packed del cliente: CG_ATTACK 8 B
    /// (`Packet.h:509-516` — header, bType, dwVictimVID, crc, crc).
    #[test]
    fn cg_attack_wire() {
        assert_eq!(CgAttack::SIZE, 8, "1+1+4+1+1 (Packet.h:509-516)");
        let mut raw = [0u8; 8];
        raw[0] = CgAttack::HEADER;
        raw[1] = CgAttack::TYPE_NORMAL;
        raw[2..6].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        raw[6] = 0xAA; // crc_proc
        raw[7] = 0xBB; // crc_file
        let p = CgAttack::from_bytes(&raw).unwrap();
        assert_eq!(p.header, 2);
        assert_eq!(p.b_type, 0);
        assert_eq!(p.victim_vid, 0x1234_5678);
        assert_eq!((p.crc_proc, p.crc_file), (0xAA, 0xBB));
        assert_eq!(p.to_bytes(), raw, "roundtrip byte-exacto");
    }

    /// Byte-exacto contra el layout packed del cliente: GC_ATTACK 10 B
    /// (`Packet.h:1936-1942` — header, dwVID, dwVictimVID, bType).
    #[test]
    fn gc_attack_wire() {
        assert_eq!(GcAttack::SIZE, 10, "1+4+4+1 (Packet.h:1936-1942)");
        let p = GcAttack::new(2, 101, CgAttack::TYPE_NORMAL);
        assert_eq!(p.to_bytes(), [12, 2, 0, 0, 0, 101, 0, 0, 0, 0]);
        assert_eq!(GcAttack::from_bytes(&p.to_bytes()).unwrap(), p);
    }

    /// Byte-exacto contra el layout packed del cliente: GC_DAMAGE_INFO 10 B
    /// (`Packet.h:1872-1878` — header, dwVID, flag, damage i32 LE).
    #[test]
    fn gc_damage_info_wire() {
        assert_eq!(GcDamageInfo::SIZE, 10, "1+4+1+4 (Packet.h:1872-1878)");
        let p = GcDamageInfo::new(101, damage_flag::NORMAL, 46);
        assert_eq!(p.to_bytes(), [135, 101, 0, 0, 0, 1, 46, 0, 0, 0]);
        assert_eq!(GcDamageInfo::from_bytes(&p.to_bytes()).unwrap(), p);
        // damage negativo (el C++ manda el daño tal cual; el cliente exige >= 0
        // para el efecto — PythonNetworkStreamPhaseGame.cpp:2453).
        let neg = GcDamageInfo::new(1, damage_flag::BLOCK, -5);
        assert_eq!(
            GcDamageInfo::from_bytes(&neg.to_bytes()).unwrap().damage,
            -5
        );
    }

    /// Longitudes incorrectas → `Err` (nunca panic).
    #[test]
    fn bad_lengths_error() {
        for len in [0usize, 1, 7, 9, 11, 32] {
            assert!(
                CgAttack::from_bytes(&vec![0u8; len]).is_err(),
                "CgAttack len {len}"
            );
            assert!(
                GcAttack::from_bytes(&vec![0u8; len]).is_err(),
                "GcAttack len {len}"
            );
            assert!(
                GcDamageInfo::from_bytes(&vec![0u8; len]).is_err(),
                "GcDamageInfo len {len}"
            );
        }
    }

    /// Los flags de daño coinciden con el enum del C++ (`char.h:120-131`).
    #[test]
    fn damage_flags_match_cpp_enum() {
        assert_eq!(damage_flag::NORMAL, 1 << 0);
        assert_eq!(damage_flag::POISON, 1 << 1);
        assert_eq!(damage_flag::DODGE, 1 << 2);
        assert_eq!(damage_flag::BLOCK, 1 << 3);
        assert_eq!(damage_flag::PENETRATE, 1 << 4);
        assert_eq!(damage_flag::CRITICAL, 1 << 5);
    }
}
