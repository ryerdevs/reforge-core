//! F4 slice 3: paquetes del WORLD ENTRY (fase Loading/Game) — verificados
//! contra `source/client/UserInterface/Packet.h` (el contrato congelado) y
//! `source/server/game/src/char.cpp` (el oracle):
//! - `TPacketGCMainCharacter` (47 B, header 15 — el CLIENTE mapea 15 = sin
//!   BGM; el C++ server emite 113 SOLO con su struct de 48 B incl. empire,
//!   `packet.h:952-961` + `char.cpp:1539-1549` — ver doc de la struct).
//! - `TPacketGCPoints` (1021 B, header 16) — `packet.h:1000-1004`
//!   + `char.cpp:1553-1581` (PointsPacket; `POINT_MAX_NUM = 255`,
//!   `length.h:70`).
//! - `TPacketGCSkillLevel` (1531 B, header 76) — `packet.h:1006-1010`
//!   + `char_skill.cpp:184-194` (SkillLevelPacket; `SKILL_MAX_NUM = 255`,
//!   `TPlayerSkill` = 6 B x86: `tables.h:351-356`).
//! - `TPacketGCLandList` (3 B + 24 B×N, header 130) — `packet.h:1996-2008`
//!   + `building.cpp:931-979` (SendLandList).
//!
//! Little-endian, packed, sin padding (mismo contrato que el resto del crate).

use crate::{rd_arr, rd_u32, Result, ProtocolError};

/// `TPlayerSkill` (6 B packed x86: `tables.h:351-356` — bMasterType BYTE,
/// bLevel BYTE, tNextRead time_t = DWORD en el build x86 del server).
/// El bytea `player.skill_level` del PG es la serie de 255 entradas.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPlayerSkill {
    pub b_master_type: u8,
    pub b_level: u8,
    pub t_next_read: u32,
}

impl TPlayerSkill {
    pub const SIZE: usize = 6;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self {
            b_master_type: data[0],
            b_level: data[1],
            t_next_read: rd_u32(data, 2),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.b_master_type;
        b[1] = self.b_level;
        b[2..6].copy_from_slice(&self.t_next_read.to_le_bytes());
        b
    }
}

/// `TQuickslot` (2 B — `tables.h:345-349`): type BYTE + pos BYTE. El bytea
/// `player.quickslot` es la serie de 36 (QUICKSLOT_MAX_NUM = 36, length.h:60).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TQuickslot {
    pub slot_type: u8,
    pub pos: u8,
}

impl TQuickslot {
    pub const SIZE: usize = 2;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self { slot_type: data[0], pos: data[1] })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        [self.slot_type, self.pos]
    }
}

/// `TPacketGCQuickSlotAdd` (4 B, header 28 — `char_quickslot.cpp:62-103`:
/// `packet_quickslot_add = {header, pos, slot}`): el server manda UNO por
/// slot al entrar (`input_db.cpp:455-456` SetQuickslot → paquete).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCQuickSlotAdd {
    pub header: u8,
    pub pos: u8,
    pub slot: TQuickslot,
}

impl TPacketGCQuickSlotAdd {
    pub const SIZE: usize = 4;
    pub const HEADER: u8 = 28;
    pub const QUICKSLOT_MAX_NUM: usize = 36;

    pub fn new(pos: u8, slot: TQuickslot) -> Self {
        Self { header: Self::HEADER, pos, slot }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self {
            header: data[0],
            pos: data[1],
            slot: TQuickslot { slot_type: data[2], pos: data[3] },
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        [self.header, self.pos, self.slot.slot_type, self.slot.pos]
    }
}

/// `TItemPos` (3 B — `GameType.h:197-205`): window_type BYTE + cell WORD.
/// Los índices del enum `EWindows` (`GameType.h:175-186`): RESERVED=0,
/// INVENTORY=1, EQUIPMENT=2, SAFEBOX=3, MALL=4, DRAGON_SOUL_INVENTORY=5,
/// BELT_INVENTORY=6, GROUND=7.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TItemPos {
    pub window: u8,
    pub cell: u16,
}

impl TItemPos {
    pub const SIZE: usize = 3;
    pub const WINDOW_INVENTORY: u8 = 1;
    pub const WINDOW_EQUIPMENT: u8 = 2;
    pub const WINDOW_DRAGON_SOUL: u8 = 5;
    pub const WINDOW_BELT: u8 = 6;
}

/// `TPacketGCItemSet` (51 B packed, header 21 — `packet.h:1043-1054` +
/// `Packet.h:1670-1681`): el set de un item del inventario/equipamiento
/// (`ItemLoad` → `AddToCharacter` → paquete; el server manda UNO por item al
/// entrar). Packed (packet.h:304-2305): `long` = 4 B x86 y `TPlayerItemAttribute`
/// = BYTE+short = 3 B (sin padding).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCItemSet {
    pub header: u8,
    pub cell: TItemPos,
    pub vnum: u32,
    pub count: u8,
    /// GAP del slice: el C++ los lee del item_proto (`item->GetFlags()`);
    /// el canal los manda 0 (el cliente no los exige para pintar el slot).
    pub flags: u32,
    pub anti_flags: u32,
    pub highlight: u8,
    /// Wire: `long` x86 = 4 B (el ItemRow los trae de bigint PG — se truncan).
    pub sockets: [i64; 3],
    /// Wire: `TPlayerItemAttribute` packed = type BYTE + value short (3 B).
    pub attrs: [(i16, i16); 7],
}

impl TPacketGCItemSet {
    /// 1 + 3 + 4 + 1 + 4 + 4 + 1 + 3×4 + 7×3 = 51 (packed).
    pub const SIZE: usize = 51;
    pub const HEADER: u8 = 21;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        let mut sockets = [0i64; 3];
        for (i, s) in sockets.iter_mut().enumerate() {
            *s = i64::from(i32::from_le_bytes([data[18 + i * 4], data[19 + i * 4], data[20 + i * 4], data[21 + i * 4]]));
        }
        let mut attrs = [(0i16, 0i16); 7];
        for (i, a) in attrs.iter_mut().enumerate() {
            a.0 = i16::from(data[30 + i * 3]);
            a.1 = i16::from_le_bytes([data[31 + i * 3], data[32 + i * 3]]);
        }
        Ok(Self {
            header: data[0],
            cell: TItemPos { window: data[1], cell: u16::from_le_bytes([data[2], data[3]]) },
            vnum: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            count: data[8],
            flags: u32::from_le_bytes([data[9], data[10], data[11], data[12]]),
            anti_flags: u32::from_le_bytes([data[13], data[14], data[15], data[16]]),
            highlight: data[17],
            sockets,
            attrs,
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1] = self.cell.window;
        b[2..4].copy_from_slice(&self.cell.cell.to_le_bytes());
        b[4..8].copy_from_slice(&self.vnum.to_le_bytes());
        b[8] = self.count;
        b[9..13].copy_from_slice(&self.flags.to_le_bytes());
        b[13..17].copy_from_slice(&self.anti_flags.to_le_bytes());
        b[17] = self.highlight;
        for (i, s) in self.sockets.iter().enumerate() {
            b[18 + i * 4..22 + i * 4].copy_from_slice(&(*s as i32).to_le_bytes());
        }
        for (i, a) in self.attrs.iter().enumerate() {
            b[30 + i * 3] = a.0 as u8;
            b[31 + i * 3..33 + i * 3].copy_from_slice(&a.1.to_le_bytes());
        }
        b
    }
}

/// `TPacketGCItemUpdate` (38 B packed, header 25 — `packet.h:1078-1085` +
/// `Packet.h:1715-1722`): el UPDATE de un item del inventario (cantidad /
/// sockets / attrs) — el C++ lo manda en `SetCount` (item.cpp:215-217) al
/// apilar (`AutoStackItem`) o al cambiar sockets/attrs. Layout: header +
/// `TItemPos` + count BYTE + sockets (3×long) + attrs (7×3 B).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCItemUpdate {
    pub header: u8,
    pub cell: TItemPos,
    pub count: u8,
    pub sockets: [i64; 3],
    pub attrs: [(i16, i16); 7],
}

impl TPacketGCItemUpdate {
    /// 1 + 3 + 1 + 12 + 21 = 38 (packed).
    pub const SIZE: usize = 38;
    pub const HEADER: u8 = 25;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        let mut sockets = [0i64; 3];
        for (i, s) in sockets.iter_mut().enumerate() {
            *s = i64::from(i32::from_le_bytes([
                data[5 + i * 4],
                data[6 + i * 4],
                data[7 + i * 4],
                data[8 + i * 4],
            ]));
        }
        let mut attrs = [(0i16, 0i16); 7];
        for (i, a) in attrs.iter_mut().enumerate() {
            a.0 = i16::from(data[17 + i * 3]);
            a.1 = i16::from_le_bytes([data[18 + i * 3], data[19 + i * 3]]);
        }
        Ok(Self {
            header: data[0],
            cell: TItemPos { window: data[1], cell: u16::from_le_bytes([data[2], data[3]]) },
            count: data[4],
            sockets,
            attrs,
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1] = self.cell.window;
        b[2..4].copy_from_slice(&self.cell.cell.to_le_bytes());
        b[4] = self.count;
        for (i, s) in self.sockets.iter().enumerate() {
            b[5 + i * 4..9 + i * 4].copy_from_slice(&(*s as i32).to_le_bytes());
        }
        for (i, a) in self.attrs.iter().enumerate() {
            b[17 + i * 3] = a.0 as u8;
            b[18 + i * 3..20 + i * 3].copy_from_slice(&a.1.to_le_bytes());
        }
        b
    }
}

/// `TPacketCGItemUse` (4 B, header 11 — `Packet.h:559-563` +
/// `packet.h:618-622`): el USO de un item del inventario. `command_item_use`
/// = header + TItemPos (el cliente manda la celda; el server aplica el
/// efecto del item_proto y decrementa el count).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketCGItemUse {
    pub header: u8,
    pub pos: TItemPos,
}

impl TPacketCGItemUse {
    /// 1 + 3 = 4 (packed).
    pub const SIZE: usize = 4;
    pub const HEADER: u8 = 11;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self {
            header: data[0],
            pos: TItemPos { window: data[1], cell: u16::from_le_bytes([data[2], data[3]]) },
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        [self.header, self.pos.window, self.pos.cell as u8, (self.pos.cell >> 8) as u8]
    }
}

/// `TPacketCGItemMove` (8 B, header 13 — `Packet.h:593-599` +
/// `packet.h:631-636`): el MOVIMIENTO de un item del inventario
/// (`command_item_move`): header + TItemPos origen + TItemPos destino +
/// BYTE num (0 = todo el stack). El C++ lo procesa en `MoveItem`
/// (char_item.cpp:5609-5767: stack si mismo vnum+sockets, split si
/// `0 < num < count`, si no mover todo).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketCGItemMove {
    pub header: u8,
    pub pos: TItemPos,
    pub change_pos: TItemPos,
    /// 0 = mover todo el stack; > 0 = split de esa cantidad.
    pub num: u8,
}

impl TPacketCGItemMove {
    /// 1 + 3 + 3 + 1 = 8 (packed).
    pub const SIZE: usize = 8;
    pub const HEADER: u8 = 13;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self {
            header: data[0],
            pos: TItemPos { window: data[1], cell: u16::from_le_bytes([data[2], data[3]]) },
            change_pos: TItemPos { window: data[4], cell: u16::from_le_bytes([data[5], data[6]]) },
            num: data[7],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1] = self.pos.window;
        b[2..4].copy_from_slice(&self.pos.cell.to_le_bytes());
        b[4] = self.change_pos.window;
        b[5..7].copy_from_slice(&self.change_pos.cell.to_le_bytes());
        b[7] = self.num;
        b
    }
}

/// `TPacketGCItemDelDeprecated` (42 B packed, header 20 — `Packet.h:1676-1684`
/// + `packet.h:1071-1085`): el borrado de un item del INVENTARIO. El cliente
/// lo registra con `sizeof(TPacketGCItemDelDeprecated)` (PythonNetworkStream
/// .cpp:71) y el handler `RecvItemSetPacket` lee el struct completo — el C++
/// manda el layout LEGACY (header + TItemPos + vnum + count + sockets +
/// attrs) aunque el nombre diga "Del". 1+3+4+1+12+21 = 42.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCItemDelDeprecated {
    pub header: u8,
    pub cell: TItemPos,
    pub vnum: u32,
    pub count: u8,
    pub sockets: [i64; 3],
    pub attrs: [(i16, i16); 7],
}

impl TPacketGCItemDelDeprecated {
    pub const SIZE: usize = 42;
    pub const HEADER: u8 = 20;

    pub fn new(cell: TItemPos, vnum: u32, count: u8) -> Self {
        Self {
            header: Self::HEADER,
            cell,
            vnum,
            count,
            sockets: [0; 3],
            attrs: [(0, 0); 7],
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        let mut sockets = [0i64; 3];
        for (i, s) in sockets.iter_mut().enumerate() {
            *s = i64::from(i32::from_le_bytes([
                data[9 + i * 4],
                data[10 + i * 4],
                data[11 + i * 4],
                data[12 + i * 4],
            ]));
        }
        let mut attrs = [(0i16, 0i16); 7];
        for (i, a) in attrs.iter_mut().enumerate() {
            a.0 = i16::from(data[21 + i * 3]);
            a.1 = i16::from_le_bytes([data[22 + i * 3], data[23 + i * 3]]);
        }
        Ok(Self {
            header: data[0],
            cell: TItemPos { window: data[1], cell: u16::from_le_bytes([data[2], data[3]]) },
            vnum: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            count: data[8],
            sockets,
            attrs,
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1] = self.cell.window;
        b[2..4].copy_from_slice(&self.cell.cell.to_le_bytes());
        b[4..8].copy_from_slice(&self.vnum.to_le_bytes());
        b[8] = self.count;
        for (i, s) in self.sockets.iter().enumerate() {
            b[9 + i * 4..13 + i * 4].copy_from_slice(&(*s as i32).to_le_bytes());
        }
        for (i, a) in self.attrs.iter().enumerate() {
            b[21 + i * 3] = a.0 as u8;
            b[22 + i * 3..24 + i * 3].copy_from_slice(&a.1.to_le_bytes());
        }
        b
    }
}

/// `TPacketAffectElement` (21 B — `tables.h:808-816`): el elemento de affect
/// del wire (sin dwPID — el del F3 es la fila de la tabla).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketAffectElement {
    pub dw_type: u32,
    pub b_apply_on: u8,
    pub l_apply_value: i32,
    pub dw_flag: u32,
    pub l_duration: i32,
    pub l_sp_cost: i32,
}

impl TPacketAffectElement {
    pub const SIZE: usize = 21;

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..4].copy_from_slice(&self.dw_type.to_le_bytes());
        b[4] = self.b_apply_on;
        b[5..9].copy_from_slice(&self.l_apply_value.to_le_bytes());
        b[9..13].copy_from_slice(&self.dw_flag.to_le_bytes());
        b[13..17].copy_from_slice(&self.l_duration.to_le_bytes());
        b[17..21].copy_from_slice(&self.l_sp_cost.to_le_bytes());
        b
    }
}

/// `TPacketGCItemGroundAdd` (58 B packed, header 26 — `packet.h:1087-1098`
/// y `Packet.h:1724-1738`): un item EN EL SUELO (spawn de drop). Con
/// `ENABLE_ITEM_GROUND_EX` activo en AMBOS lados (cliente `Locale_inc.h:61`,
/// server `item.cpp:137`): header + x,y,z (long) + dwVID + dwVnum + count +
/// sockets (3×long) + attrs (7×`TPlayerItemAttribute` = 3 B cada uno).
/// `1 + 12 + 4 + 4 + 4 + 12 + 21 = 58`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCItemGroundAdd {
    pub header: u8,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub vid: u32,
    pub vnum: u32,
    pub count: u32,
    /// Wire: `long` x86 = 4 B (`ITEM_SOCKET_MAX_NUM` = 3).
    pub sockets: [i64; 3],
    /// Wire: `TPlayerItemAttribute` packed = type BYTE + value short (3 B).
    pub attrs: [(i16, i16); 7],
}

impl TPacketGCItemGroundAdd {
    pub const SIZE: usize = 58;
    pub const HEADER: u8 = 26;

    pub fn new(vid: u32, vnum: u32, x: i32, y: i32, z: i32, count: u32) -> Self {
        Self {
            header: Self::HEADER,
            x,
            y,
            z,
            vid,
            vnum,
            count,
            sockets: [0; 3],
            attrs: [(0, 0); 7],
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        let mut sockets = [0i64; 3];
        for (i, s) in sockets.iter_mut().enumerate() {
            *s = i64::from(i32::from_le_bytes([
                data[25 + i * 4],
                data[26 + i * 4],
                data[27 + i * 4],
                data[28 + i * 4],
            ]));
        }
        let mut attrs = [(0i16, 0i16); 7];
        for (i, a) in attrs.iter_mut().enumerate() {
            a.0 = i16::from(data[37 + i * 3]);
            a.1 = i16::from_le_bytes([data[38 + i * 3], data[39 + i * 3]]);
        }
        Ok(Self {
            header: data[0],
            x: i32::from_le_bytes([data[1], data[2], data[3], data[4]]),
            y: i32::from_le_bytes([data[5], data[6], data[7], data[8]]),
            z: i32::from_le_bytes([data[9], data[10], data[11], data[12]]),
            vid: u32::from_le_bytes([data[13], data[14], data[15], data[16]]),
            vnum: u32::from_le_bytes([data[17], data[18], data[19], data[20]]),
            count: u32::from_le_bytes([data[21], data[22], data[23], data[24]]),
            sockets,
            attrs,
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.x.to_le_bytes());
        b[5..9].copy_from_slice(&self.y.to_le_bytes());
        b[9..13].copy_from_slice(&self.z.to_le_bytes());
        b[13..17].copy_from_slice(&self.vid.to_le_bytes());
        b[17..21].copy_from_slice(&self.vnum.to_le_bytes());
        b[21..25].copy_from_slice(&self.count.to_le_bytes());
        for (i, s) in self.sockets.iter().enumerate() {
            b[25 + i * 4..29 + i * 4].copy_from_slice(&(*s as i32).to_le_bytes());
        }
        for (i, a) in self.attrs.iter().enumerate() {
            b[37 + i * 3] = a.0 as u8;
            b[38 + i * 3..40 + i * 3].copy_from_slice(&a.1.to_le_bytes());
        }
        b
    }
}

/// `TPacketGCItemGroundDel` (5 B, header 27 — `packet.h:1107-1111`): quita
/// un item del suelo (pickup / expiración).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCItemGroundDel {
    pub header: u8,
    pub vid: u32,
}

impl TPacketGCItemGroundDel {
    pub const SIZE: usize = 5;
    pub const HEADER: u8 = 27;

    pub fn new(vid: u32) -> Self {
        Self { header: Self::HEADER, vid }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self { header: data[0], vid: rd_u32(data, 1) })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.vid.to_le_bytes());
        b
    }
}

/// `TPacketGCItemOwnership` (30 B, header 31 — `packet.h:1100-1105` +
/// `Packet.h:1746-1751`): el dueño de un item del suelo (el cliente pinta el
/// nombre sobre el item; `CHARACTER_NAME_MAX_LEN` = 24).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCItemOwnership {
    pub header: u8,
    pub vid: u32,
    pub name: [u8; 25],
}

impl TPacketGCItemOwnership {
    pub const SIZE: usize = 30;
    pub const HEADER: u8 = 31;

    pub fn new(vid: u32, name: &[u8]) -> Self {
        let mut n = [0u8; 25];
        let len = name.len().min(24);
        n[..len].copy_from_slice(&name[..len]);
        Self { header: Self::HEADER, vid, name: n }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        let mut name = [0u8; 25];
        name.copy_from_slice(&data[5..30]);
        Ok(Self { header: data[0], vid: rd_u32(data, 1), name })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.vid.to_le_bytes());
        b[5..30].copy_from_slice(&self.name);
        b
    }
}

/// `TPacketGCAffectAdd` (22 B, header 126 — `packet.h:2032-2036`): un affect/// activo (`LoadAffect` → `AddAffect` → paquete; el server manda UNO por
/// affect al entrar).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCAffectAdd {
    pub header: u8,
    pub elem: TPacketAffectElement,
}

impl TPacketGCAffectAdd {
    pub const SIZE: usize = 22;
    pub const HEADER: u8 = 126;

    pub fn new(elem: TPacketAffectElement) -> Self {
        Self { header: Self::HEADER, elem }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        let mut b = [0u8; Self::SIZE];
        b.copy_from_slice(data);
        let elem = TPacketAffectElement {
            dw_type: u32::from_le_bytes([b[1], b[2], b[3], b[4]]),
            b_apply_on: b[5],
            l_apply_value: i32::from_le_bytes([b[6], b[7], b[8], b[9]]),
            dw_flag: u32::from_le_bytes([b[10], b[11], b[12], b[13]]),
            l_duration: i32::from_le_bytes([b[14], b[15], b[16], b[17]]),
            l_sp_cost: i32::from_le_bytes([b[18], b[19], b[20], b[21]]),
        };
        Ok(Self { header: data[0], elem })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..].copy_from_slice(&self.elem.to_bytes());
        b
    }
}

/// `TPacketGCDead` (5 B, header 14 — `Packet.h:1349-1353`): la muerte de un
/// personaje/mob (la animación de morir; el cliente lo remueve tras la
/// animación con el `GC_CHARACTER_DEL`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCDead {
    pub header: u8,
    pub vid: u32,
}

impl TPacketGCDead {
    pub const SIZE: usize = 5;
    pub const HEADER: u8 = 14;

    pub fn new(vid: u32) -> Self {
        Self { header: Self::HEADER, vid }
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.vid.to_le_bytes());
        b
    }
}

/// `TPacketGCCharacterDelete` (5 B, header 2 — `Packet.h:1296-1300`): la
/// remoción de un personaje/mob del mundo (el cliente lo remueve; en el C++
/// es el `EncodeRemovePacket` del dead mob).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCCharacterDelete {
    pub header: u8,
    pub vid: u32,
}

/// `TPacketGCTarget` (6 B, header 63 — `Packet.h:1374-1379`): la barra de
/// vida del objetivo (el cliente la dibuja con `SetHPTargetBoard` vía
/// `RecvTargetPacket`). `b_hp_percent` = 0..100; 0 para PCs (parity
/// `CHARACTER::SetTarget`/`BroadcastTargetPacket`, char.cpp:5048-5143).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCTarget {
    pub header: u8,
    pub vid: u32,
    pub b_hp_percent: u8,
}

impl TPacketGCTarget {
    pub const SIZE: usize = 6;
    pub const HEADER: u8 = 63;

    pub fn new(vid: u32, b_hp_percent: u8) -> Self {
        Self { header: Self::HEADER, vid, b_hp_percent }
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.vid.to_le_bytes());
        b[5] = self.b_hp_percent;
        b
    }
}

impl TPacketGCCharacterDelete {
    pub const SIZE: usize = 5;
    pub const HEADER: u8 = 2;

    pub fn new(vid: u32) -> Self {
        Self { header: Self::HEADER, vid }
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.vid.to_le_bytes());
        b
    }
}

/// `TPacketGCTime` (5 B, header 106 — `packet.h:1872-1876`): el reloj del
/// server (time_t x86 = DWORD unix seconds; `input_login.cpp:648-651` —
/// `p.time = get_global_time()`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCTime {
    pub header: u8,
    pub time: u32,
}

impl TPacketGCTime {
    pub const SIZE: usize = 5;
    pub const HEADER: u8 = 106;

    pub fn new(time: u32) -> Self {
        Self { header: Self::HEADER, time }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self { header: data[0], time: rd_u32(data, 1) })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.time.to_le_bytes());
        b
    }
}

/// `TPacketGCChannel` (2 B, header 121 — `packet.h:1968-1972`): el número del
/// canal (`input_login.cpp:653-656` — `p2.channel = g_bChannel`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCChannel {
    pub header: u8,
    pub channel: u8,
}

impl TPacketGCChannel {
    pub const SIZE: usize = 2;
    pub const HEADER: u8 = 121;

    pub fn new(channel: u8) -> Self {
        Self { header: Self::HEADER, channel }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self { header: data[0], channel: data[1] })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        [self.header, self.channel]
    }
}

/// `TPacketGCWarp` (15 B, header 65 — `packet.h:1381-1388` + `Packet.h:199`):
/// el warp del jugador (revive en la ciudad / teletransporte). El cliente al
/// recibirlo hace `__DirectEnterMode_Set` + `Connect(lAddr, wPort)`
/// (`RecvWarpPacket` — PythonNetworkStreamPhaseGame.cpp:942-954): cierra la
/// conexión del canal y RECONECTA con el flujo DirectEnter completo (el
/// canal Rust ya lo sirve — F4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCWarp {
    pub header: u8,
    /// Destino en UNITS (el village del empire — `EMPIRE_START_*`).
    pub x: i32,
    pub y: i32,
    /// `inet_addr` del canal destino (LE — el mismo formato del 449 B).
    pub addr: u32,
    pub port: u16,
}

impl TPacketGCWarp {
    /// 1 + 4 + 4 + 4 + 2 = 15 (packed).
    pub const SIZE: usize = 15;
    pub const HEADER: u8 = 65;

    pub fn new(x: i32, y: i32, addr: u32, port: u16) -> Self {
        Self { header: Self::HEADER, x, y, addr, port }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self {
            header: data[0],
            x: i32::from_le_bytes([data[1], data[2], data[3], data[4]]),
            y: i32::from_le_bytes([data[5], data[6], data[7], data[8]]),
            addr: u32::from_le_bytes([data[9], data[10], data[11], data[12]]),
            port: u16::from_le_bytes([data[13], data[14]]),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.x.to_le_bytes());
        b[5..9].copy_from_slice(&self.y.to_le_bytes());
        b[9..13].copy_from_slice(&self.addr.to_le_bytes());
        b[13..15].copy_from_slice(&self.port.to_le_bytes());
        b
    }
}

/// `TPacketCGMarkLogin` (9 B, header 100 — `packet.h:1729-1734`): el login de
/// la conexión del guild mark (el cliente la abre en paralelo al select con la
/// misma IP/puerto del canal; `GuildMarkDownloader.cpp:213-229` — responde con
/// este paquete al handshake del server). El server normal (`guild_mark_server`
/// OFF) lo rechaza cerrando la conexión (`input.cpp:560-572`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketCGMarkLogin {
    pub header: u8,
    pub handle: u32,
    pub random_key: u32,
}

impl TPacketCGMarkLogin {
    pub const SIZE: usize = 9;
    pub const HEADER: u8 = 100;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self {
            header: data[0],
            handle: rd_u32(data, 1),
            random_key: rd_u32(data, 5),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.handle.to_le_bytes());
        b[5..9].copy_from_slice(&self.random_key.to_le_bytes());
        b
    }
}

/// `TPacketGCMainCharacter` (47 B, header **15** — layout del CLIENTE,
/// `Packet.h:1347-1350/1365-1373`): header, dwVID, wRaceNum, szName[25], lx,
/// ly, lz, skill_group.
///
/// ⚠️ **HEADER VERIFICADO (fix 2026-08-12)**: el cliente mapea **15** =
/// `HEADER_GC_MAIN_CHARACTER` (47 B, `RecvMainCharacter` →
/// `PythonNetworkStreamPhaseLoading.cpp:100-103`) y **113** =
/// `HEADER_GC_MAIN_CHARACTER2_EMPIRE` (**48 B** con `byEmpire` —
/// `Packet.h:1376-1385`). Emitir 113 con 47 B desalinea el stream 1 byte: el
/// cliente lee 48 B (el último = el header del quickslot 0), pierde el slot 0
/// y corrompe `bySkillGroup`; solo se auto-cura porque el quickslot 0 está
/// vacío (bytes 0 → el skip de cabeceras 0 los absorbe). Con el quickslot 0
/// lleno el desync cascada (ADD basura → header inválido → `PostQuitMessage`).
/// El header correcto para el struct sin empire ES **15** (el C++ server manda
/// 113 SOLO con su struct de 48 B incl. empire — que este cliente NO tiene).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCMainCharacter {
    pub header: u8,
    pub dw_vid: u32,
    pub w_race_num: u32,
    pub sz_name: [u8; 25],
    pub lx: i32,
    pub ly: i32,
    pub lz: i32,
    pub skill_group: u8,
}

impl TPacketGCMainCharacter {
    /// 1 + 4 + 4 + 25 + 12 + 1 = 47 (layout del cliente — sin empire).
    pub const SIZE: usize = 47;
    /// Header del CLIENTE sin BGM (`Packet.h:160`): 15. NO 113 — el 113 del
    /// cliente es la variante 48 B con empire (Packet.h:251).
    pub const HEADER: u8 = 15;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self {
            header: data[0],
            dw_vid: rd_u32(data, 1),
            w_race_num: rd_u32(data, 5),
            sz_name: rd_arr(data, 9),
            lx: i32::from_le_bytes([data[34], data[35], data[36], data[37]]),
            ly: i32::from_le_bytes([data[38], data[39], data[40], data[41]]),
            lz: i32::from_le_bytes([data[42], data[43], data[44], data[45]]),
            skill_group: data[46],
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        b[1..5].copy_from_slice(&self.dw_vid.to_le_bytes());
        b[5..9].copy_from_slice(&self.w_race_num.to_le_bytes());
        b[9..34].copy_from_slice(&self.sz_name);
        b[34..38].copy_from_slice(&self.lx.to_le_bytes());
        b[38..42].copy_from_slice(&self.ly.to_le_bytes());
        b[42..46].copy_from_slice(&self.lz.to_le_bytes());
        b[46] = self.skill_group;
        b
    }

    /// Nombre como `&str` (hasta el primer NUL).
    pub fn name(&self) -> std::borrow::Cow<'_, str> {
        crate::cstr_str(&self.sz_name)
    }
}

/// `TPacketGCPoints` (1021 B, header 16 — `packet.h:1000-1004`): header +
/// `INT points[255]` (`POINT_MAX_NUM = 255`, `length.h:70`). Los índices del
/// enum `EPointTypes` (`char.h:133+`) — los del entry se documentan en
/// `game_core::packets::points_packet`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCPoints {
    pub header: u8,
    pub points: [i32; 255],
}

impl TPacketGCPoints {
    pub const SIZE: usize = 1 + 255 * 4;
    pub const HEADER: u8 = 16;
    pub const POINT_MAX_NUM: usize = 255;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        let mut points = [0i32; 255];
        for (i, p) in points.iter_mut().enumerate() {
            *p = i32::from_le_bytes([data[1 + i * 4], data[2 + i * 4], data[3 + i * 4], data[4 + i * 4]]);
        }
        Ok(Self { header: data[0], points })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        for (i, p) in self.points.iter().enumerate() {
            b[1 + i * 4..5 + i * 4].copy_from_slice(&p.to_le_bytes());
        }
        b
    }
}

/// `TPacketGCSkillLevel` (1531 B, header 76 — `packet.h:1006-1010`,
/// `char_skill.cpp:184-194`): header + `TPlayerSkill[255]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TPacketGCSkillLevel {
    pub header: u8,
    pub skills: [TPlayerSkill; 255],
}

impl TPacketGCSkillLevel {
    pub const SIZE: usize = 1 + 255 * TPlayerSkill::SIZE;
    pub const HEADER: u8 = 76;
    pub const SKILL_MAX_NUM: usize = 255;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        let mut skills = [TPlayerSkill { b_master_type: 0, b_level: 0, t_next_read: 0 }; 255];
        for (i, s) in skills.iter_mut().enumerate() {
            *s = TPlayerSkill::from_bytes(&data[1 + i * 6..7 + i * 6])?;
        }
        Ok(Self { header: data[0], skills })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.header;
        for (i, s) in self.skills.iter().enumerate() {
            b[1 + i * 6..7 + i * 6].copy_from_slice(&s.to_bytes());
        }
        b
    }
}

/// `TLandPacketElement` (24 B — `packet.h:1996-2002`): dwID, x, y, width,
/// height, dwGuildID (todos 4 B). Coordenadas en células (el cliente las
/// escala ×100 — parity `building.cpp:956-961`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TLandPacketElement {
    pub dw_id: u32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub dw_guild_id: u32,
}

impl TLandPacketElement {
    pub const SIZE: usize = 24;

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..4].copy_from_slice(&self.dw_id.to_le_bytes());
        b[4..8].copy_from_slice(&self.x.to_le_bytes());
        b[8..12].copy_from_slice(&self.y.to_le_bytes());
        b[12..16].copy_from_slice(&self.width.to_le_bytes());
        b[16..20].copy_from_slice(&self.height.to_le_bytes());
        b[20..24].copy_from_slice(&self.dw_guild_id.to_le_bytes());
        b
    }
}

/// `TPacketGCLandList` (tamaño variable: 3 B + 24 B×N, header 130 —
/// `packet.h:2004-2008` + `building.cpp:969-978`): header + `size` WORD
/// (3 + N×24) + N elementos. Serialización directa (el tamaño depende de los
/// lands del mapa — sin struct fijo).
pub fn land_list_bytes(elements: &[TLandPacketElement]) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + elements.len() * 24);
    out.push(130);
    let size = (3 + elements.len() * 24) as u16;
    out.extend_from_slice(&size.to_le_bytes());
    for e in elements {
        out.extend_from_slice(&e.to_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_cstr;

    #[test]
    fn wire_sizes() {
        assert_eq!(TPlayerSkill::SIZE, 6, "x86: BYTE + BYTE + time_t(4)");
        assert_eq!(TPacketCGMarkLogin::SIZE, 9, "header + handle + random_key");
        assert_eq!(TQuickslot::SIZE, 2);
        assert_eq!(TPacketGCQuickSlotAdd::SIZE, 4, "header + pos + TQuickslot");
        assert_eq!(TItemPos::SIZE, 3, "window BYTE + cell WORD");
        assert_eq!(TPacketGCItemSet::SIZE, 51, "1+3+4+1+4+4+1+12+21 (packed — attrs 3 B)");
        assert_eq!(TPacketAffectElement::SIZE, 21);
        assert_eq!(TPacketGCAffectAdd::SIZE, 22, "header + element");
        assert_eq!(TPacketGCTime::SIZE, 5, "header + time_t(4)");
        assert_eq!(TPacketGCChannel::SIZE, 2, "header + channel");
        assert_eq!(TPacketGCMainCharacter::SIZE, 47, "layout del CLIENTE (sin empire — Packet.h:1349-1357)");
        assert_eq!(TPacketGCDead::SIZE, 5, "header + vid");
        assert_eq!(TPacketGCCharacterDelete::SIZE, 5, "header + vid");
        assert_eq!(TPacketGCTarget::SIZE, 6, "header + vid + bHPPercent (Packet.h:1374-1379)");
        assert_eq!(TPacketGCPoints::SIZE, 1 + 255 * 4);
        assert_eq!(TPacketGCSkillLevel::SIZE, 1 + 255 * 6);
        assert_eq!(TLandPacketElement::SIZE, 24);
    }

    #[test]
    fn roundtrip_quickslot_and_itemset() {
        let q = TPacketGCQuickSlotAdd::new(3, TQuickslot { slot_type: 0, pos: 5 });
        let b = q.to_bytes();
        assert_eq!(b, [28, 3, 0, 5]);
        assert_eq!(TPacketGCQuickSlotAdd::from_bytes(&b).unwrap(), q);

        // GC_TARGET (63): header + vid LE + bHPPercent (fix bug 5).
        let t = TPacketGCTarget::new(0x1234_5678, 37);
        let tb = t.to_bytes();
        assert_eq!(tb, [63, 0x78, 0x56, 0x34, 0x12, 37]);

        let it = TPacketGCItemSet {
            header: TPacketGCItemSet::HEADER,
            cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: 7 },
            vnum: 27001,
            count: 1,
            flags: 0,
            anti_flags: 0,
            highlight: 0,
            // Wire: long x86 = 4 B — el roundtrip trunca a i32 (bits); el
            // assert de bytes cubre el 0xDEAD_BEEF truncado.
            sockets: [0x1234, 0, 0],
            attrs: [(1, 100), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
        };
        let b = it.to_bytes();
        assert_eq!(b.len(), 51, "packed (attrs 3 B, sockets long 4 B)");
        assert_eq!(b[0], 21);
        assert_eq!(&b[1..4], &[1, 7, 0], "TItemPos: window=1(INVENTORY), cell=7");
        assert_eq!(&b[4..8], &27001u32.to_le_bytes());
        assert_eq!(b[8], 1, "count BYTE");
        assert_eq!(&b[18..22], &0x1234i32.to_le_bytes(), "socket0 (long 4 B)");
        assert_eq!(&b[30..33], &[1, 100, 0], "attr0 packed: type BYTE + value short");
        let it2 = TPacketGCItemSet::from_bytes(&b).unwrap();
        assert_eq!(it, it2);
    }

    #[test]
    fn roundtrip_affect_add() {
        let a = TPacketGCAffectAdd::new(TPacketAffectElement {
            dw_type: 1,
            b_apply_on: 2,
            l_apply_value: 3,
            dw_flag: 0xDEAD_BEEF,
            l_duration: 60,
            l_sp_cost: 0,
        });
        let b = a.to_bytes();
        assert_eq!(b.len(), 22);
        assert_eq!(b[0], 126);
        assert_eq!(&b[1..5], &1u32.to_le_bytes(), "dwType");
        assert_eq!(b[5], 2, "bApplyOn");
        assert_eq!(&b[10..14], &0xDEAD_BEEFu32.to_le_bytes(), "dwFlag");
        assert_eq!(TPacketGCAffectAdd::from_bytes(&b).unwrap(), a);
    }

    #[test]
    fn roundtrip_time_and_channel() {
        let t = TPacketGCTime::new(1_752_300_000);
        let b = t.to_bytes();
        assert_eq!(b.len(), 5);
        assert_eq!(b[0], 106);
        assert_eq!(TPacketGCTime::from_bytes(&b).unwrap(), t);
        assert_eq!(TPacketGCTime::from_bytes(&b[..4]).is_err(), true, "5 B exactos");

        let c = TPacketGCChannel::new(1);
        let b = c.to_bytes();
        assert_eq!(b, [121, 1]);
        assert_eq!(TPacketGCChannel::from_bytes(&b).unwrap(), c);
    }

    #[test]
    fn roundtrip_mark_login() {
        let p = TPacketCGMarkLogin { header: 100, handle: 0xDEAD_BEEF, random_key: 0xCAFE_BABE };
        let b = p.to_bytes();
        assert_eq!(b.len(), 9);
        assert_eq!(b[0], 100);
        let p2 = TPacketCGMarkLogin::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert!(TPacketCGMarkLogin::from_bytes(&b[..8]).is_err(), "9 B exactos");
    }

    #[test]
    fn roundtrip_main_character() {
        let p = TPacketGCMainCharacter {
            header: TPacketGCMainCharacter::HEADER,
            dw_vid: 2,
            w_race_num: 1,
            sz_name: from_cstr("ninja"),
            lx: 969600,
            ly: 278400,
            lz: 0,
            skill_group: 3,
        };
        let b = p.to_bytes();
        assert_eq!(b.len(), 47, "layout del CLIENTE (sin empire)");
        assert_eq!(b[0], TPacketGCMainCharacter::HEADER, "header 15 = MAIN_CHARACTER sin BGM (Packet.h:160)");
        assert_eq!(b[46], 3, "skill_group@46 (el offset del empire del server NO existe en el cliente)");
        let p2 = TPacketGCMainCharacter::from_bytes(&b).unwrap();
        assert_eq!(p, p2);
        assert_eq!(p2.name(), "ninja");
    }

    #[test]
    fn roundtrip_points_and_skills() {
        let mut pts = TPacketGCPoints { header: TPacketGCPoints::HEADER, points: [0; 255] };
        pts.points[1] = 5; // POINT_LEVEL
        pts.points[5] = 100; // POINT_HP
        let b = pts.to_bytes();
        assert_eq!(b.len(), 1021);
        let p2 = TPacketGCPoints::from_bytes(&b).unwrap();
        assert_eq!(p2.points[1], 5);
        assert_eq!(p2.points[5], 100);
        assert_eq!(p2, pts);

        let mut sk = TPacketGCSkillLevel { header: TPacketGCSkillLevel::HEADER, skills: [TPlayerSkill { b_master_type: 0, b_level: 0, t_next_read: 0 }; 255] };
        sk.skills[1].b_level = 20;
        sk.skills[1].b_master_type = 2;
        sk.skills[1].t_next_read = 0xDEAD_BEEF;
        let b = sk.to_bytes();
        assert_eq!(b.len(), 1531);
        let s2 = TPacketGCSkillLevel::from_bytes(&b).unwrap();
        assert_eq!(s2.skills[1].b_level, 20);
        assert_eq!(s2.skills[1].b_master_type, 2);
        assert_eq!(s2.skills[1].t_next_read, 0xDEAD_BEEF);
        assert_eq!(s2, sk);
    }

    #[test]
    fn skill_level_parse_bytea_slice() {
        // El bytea del PG es la serie cruda: 255 × 6 B — el primer skill en
        // los bytes [0..6], el 254 en [1524..1530].
        let mut raw = [0u8; 1530];
        raw[0] = 1; // bMasterType skill 0
        raw[1] = 3; // bLevel skill 0
        let mut sk = TPacketGCSkillLevel { header: 76, skills: [TPlayerSkill { b_master_type: 0, b_level: 0, t_next_read: 0 }; 255] };
        for (i, s) in sk.skills.iter_mut().enumerate() {
            *s = TPlayerSkill::from_bytes(&raw[i * 6..(i + 1) * 6]).unwrap();
        }
        assert_eq!(sk.skills[0].b_master_type, 1);
        assert_eq!(sk.skills[0].b_level, 3);
        assert_eq!(sk.skills[254].b_level, 0);
    }

    #[test]
    fn land_list_size_and_layout() {
        // El log del core C++: "SendLandList map 41 count 18 elem_size: 432"
        // — 18 × 24 B; el paquete total = 3 + 432 = 435 B.
        let elements: Vec<TLandPacketElement> = (0..18)
            .map(|i| TLandPacketElement {
                dw_id: 201 + i as u32,
                x: 66100 + i * 100,
                y: 9400,
                width: 3000,
                height: 3000,
                dw_guild_id: 0,
            })
            .collect();
        let bytes = land_list_bytes(&elements);
        assert_eq!(bytes.len(), 435, "3 + 18×24 (parity log del core)");
        assert_eq!(bytes[0], 130, "header GC_LAND_LIST");
        assert_eq!(u16::from_le_bytes([bytes[1], bytes[2]]), 435, "size WORD");
        // Primer elemento: dwID@3, x@7...
        assert_eq!(u32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]), 201u32);
        assert_eq!(i32::from_le_bytes([bytes[7], bytes[8], bytes[9], bytes[10]]), 66100);
        assert_eq!(u16::from_le_bytes([bytes[1], bytes[2]]), bytes.len() as u16);
    }

    #[test]
    fn land_list_empty() {
        let bytes = land_list_bytes(&[]);
        assert_eq!(bytes, [130, 3, 0], "sin lands: header + size 3");
    }

    /// Warp wire (F5.3): `GC_WARP` (65) = 15 B packed — header + lX + lY +
    /// lAddr (inet_addr LE) + wPort (`packet.h:1381-1388` + `Packet.h:199`).
    #[test]
    fn gc_warp_wire_size_and_parse() {
        assert_eq!(TPacketGCWarp::SIZE, 15, "1+4+4+4+2 (packed)");
        let w = TPacketGCWarp::new(969600, 278400, 0xC9A8_8019, 30003);
        let b = w.to_bytes();
        assert_eq!(b.len(), 15);
        assert_eq!(b[0], 65, "header GC_WARP");
        assert_eq!(&b[1..5], &969600i32.to_le_bytes(), "lX");
        assert_eq!(&b[5..9], &278400i32.to_le_bytes(), "lY");
        assert_eq!(&b[9..13], &0xC9A8_8019u32.to_le_bytes(), "lAddr inet_addr LE");
        assert_eq!(&b[13..15], &30003u16.to_le_bytes(), "wPort");
        assert_eq!(TPacketGCWarp::from_bytes(&b).unwrap(), w);
        // Bad lengths → Err.
        assert!(TPacketGCWarp::from_bytes(&b[..14]).is_err());
        assert!(TPacketGCWarp::from_bytes(&[65, 0]).is_err());
    }

    /// Item update wire (F5.3): `GC_ITEM_UPDATE` (25) = 38 B packed —
    /// header + TItemPos + count + sockets (3×long) + attrs (7×3 B)
    /// (`packet.h:1078-1085` + `Packet.h:1715-1722`).
    #[test]
    fn gc_item_update_wire_size_and_parse() {
        assert_eq!(TPacketGCItemUpdate::SIZE, 38, "1+3+1+12+21 (packed)");
        let u = TPacketGCItemUpdate {
            header: TPacketGCItemUpdate::HEADER,
            cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: 7 },
            count: 200,
            sockets: [0x1234, 0, 0],
            attrs: [(1, 100), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
        };
        let b = u.to_bytes();
        assert_eq!(b.len(), 38);
        assert_eq!(b[0], 25, "header GC_ITEM_UPDATE");
        assert_eq!(&b[1..4], &[1, 7, 0], "TItemPos: window=1(INVENTORY), cell=7");
        assert_eq!(b[4], 200, "count");
        assert_eq!(&b[5..9], &0x1234i32.to_le_bytes(), "socket0 (long 4 B)");
        assert_eq!(b[17], 1, "attr0 type");
        assert_eq!(&b[18..20], &100i16.to_le_bytes(), "attr0 value");
        assert_eq!(TPacketGCItemUpdate::from_bytes(&b).unwrap(), u);
        assert!(TPacketGCItemUpdate::from_bytes(&b[..37]).is_err(), "BadLength");
    }

    /// Item use wire (F5.3): `CG_ITEM_USE` (11) = 4 B — header + TItemPos
    /// (`Packet.h:559-563` + `packet.h:618-622`). El 16 B del framer era el
    /// GC S→C (bug corregido).
    #[test]
    fn cg_item_use_wire_size_and_parse() {
        assert_eq!(TPacketCGItemUse::SIZE, 4, "1+3 (header + TItemPos)");
        let u = TPacketCGItemUse {
            header: TPacketCGItemUse::HEADER,
            pos: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: 7 },
        };
        let b = u.to_bytes();
        assert_eq!(b.len(), 4);
        assert_eq!(b, [11, 1, 7, 0], "header + window=1(INVENTORY) + cell=7");
        assert_eq!(TPacketCGItemUse::from_bytes(&b).unwrap(), u);
        assert!(TPacketCGItemUse::from_bytes(&b[..3]).is_err(), "BadLength");
    }

    /// Item move wire (F5.3): `CG_ITEM_MOVE` (13) = 8 B — header + TItemPos
    /// origen + TItemPos destino + BYTE num (`Packet.h:593-599` +
    /// `packet.h:631-636`). El framer ya lo tenía como 8 B.
    #[test]
    fn cg_item_move_wire_size_and_parse() {
        assert_eq!(TPacketCGItemMove::SIZE, 8, "1+3+3+1 (packed)");
        let m = TPacketCGItemMove {
            header: TPacketCGItemMove::HEADER,
            pos: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: 7 },
            change_pos: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: 3 },
            num: 5,
        };
        let b = m.to_bytes();
        assert_eq!(b.len(), 8);
        assert_eq!(b, [13, 1, 7, 0, 1, 3, 0, 5], "header + 2×TItemPos + num");
        assert_eq!(TPacketCGItemMove::from_bytes(&b).unwrap(), m);
        // num 0 = mover todo el stack.
        let m0 = TPacketCGItemMove { num: 0, ..m };
        assert_eq!(TPacketCGItemMove::from_bytes(&m0.to_bytes()).unwrap(), m0);
        assert!(TPacketCGItemMove::from_bytes(&b[..7]).is_err(), "BadLength");
    }

    /// Item del deprecated wire (F5.3): `GC_ITEM_DEL` (20) = 42 B — header +
    /// TItemPos + vnum + count + sockets (3×long) + attrs (7×3 B)
    /// (`Packet.h:1676-1684`; el cliente lo registra con este sizeof).
    #[test]
    fn gc_item_del_deprecated_wire_size_and_parse() {
        assert_eq!(TPacketGCItemDelDeprecated::SIZE, 42, "1+3+4+1+12+21");
        let d = TPacketGCItemDelDeprecated::new(
            TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: 7 },
            101,
            1,
        );
        let b = d.to_bytes();
        assert_eq!(b.len(), 42);
        assert_eq!(b[0], 20, "header GC_ITEM_DEL");
        assert_eq!(&b[1..4], &[1, 7, 0], "TItemPos");
        assert_eq!(&b[4..8], &101u32.to_le_bytes(), "vnum");
        assert_eq!(b[8], 1, "count");
        assert_eq!(TPacketGCItemDelDeprecated::from_bytes(&b).unwrap(), d);
        assert!(TPacketGCItemDelDeprecated::from_bytes(&b[..41]).is_err(), "BadLength");
    }

    /// Drop wire (F5.3): `GC_ITEM_GROUND_ADD` (26) = 58 B packed con
    /// `ENABLE_ITEM_GROUND_EX` (cliente `Locale_inc.h:61` — parity
    /// `packet.h:1087-1098` + `Packet.h:1724-1738`); el layout del struct
    /// cliente: header + x,y,z + dwVID + dwVnum + count + sockets(3×long) +
    /// attrs(7×3 B). `GC_ITEM_GROUND_DEL` (27) = 5 B; `GC_ITEM_OWNERSHIP`
    /// (31) = 30 B.
    #[test]
    fn ground_item_packets_roundtrip_and_sizes() {
        // Ground add: header 26 + x,y,z + vid + vnum + count + sockets + attrs.
        let add = TPacketGCItemGroundAdd {
            header: TPacketGCItemGroundAdd::HEADER,
            x: 969600,
            y: 278400,
            z: 0,
            vid: 50_001,
            vnum: 2101,
            count: 3,
            sockets: [0x1234, 0, 0],
            attrs: [(1, 100), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
        };
        assert_eq!(TPacketGCItemGroundAdd::SIZE, 58, "1+12+4+4+4+12+21 (packed)");
        let b = add.to_bytes();
        assert_eq!(b.len(), 58);
        assert_eq!(b[0], 26, "header");
        assert_eq!(&b[1..5], &969600i32.to_le_bytes(), "x");
        assert_eq!(&b[5..9], &278400i32.to_le_bytes(), "y");
        assert_eq!(&b[9..13], &0i32.to_le_bytes(), "z");
        assert_eq!(&b[13..17], &50_001u32.to_le_bytes(), "dwVID");
        assert_eq!(&b[17..21], &2101u32.to_le_bytes(), "dwVnum");
        assert_eq!(&b[21..25], &3u32.to_le_bytes(), "count");
        assert_eq!(&b[25..29], &0x1234i32.to_le_bytes(), "socket0 (long 4 B)");
        assert_eq!(b[37], 1, "attr0 type");
        assert_eq!(&b[38..40], &100i16.to_le_bytes(), "attr0 value");
        let add2 = TPacketGCItemGroundAdd::from_bytes(&b).unwrap();
        assert_eq!(add2, add);

        // Ground del: header + vid.
        let del = TPacketGCItemGroundDel::new(50_001);
        assert_eq!(TPacketGCItemGroundDel::SIZE, 5);
        let b = del.to_bytes();
        assert_eq!(b, [27, 0x51, 0xC3, 0, 0], "50_001 = 0xC351 LE");
        assert_eq!(TPacketGCItemGroundDel::from_bytes(&b).unwrap(), del);

        // Ownership: header + dwVID + name[25] (el array ya es zeroed — los
        // bytes del nombre + NUL implícito del resto del buffer).
        let own = TPacketGCItemOwnership::new(50_001, b"ninja");
        assert_eq!(TPacketGCItemOwnership::SIZE, 30);
        let b = own.to_bytes();
        assert_eq!(b[0], 31);
        assert_eq!(&b[1..5], &50_001u32.to_le_bytes());
        assert_eq!(&b[5..10], b"ninja", "bytes del nombre");
        assert_eq!(b[10], 0, "NUL tras el nombre (array zeroed)");
        assert_eq!(TPacketGCItemOwnership::from_bytes(&b).unwrap(), own);

        // Longitudes malas → Err (BadLength).
        assert!(TPacketGCItemGroundAdd::from_bytes(&b[..20]).is_err());
        assert!(TPacketGCItemGroundDel::from_bytes(&[27]).is_err());
        assert!(TPacketGCItemOwnership::from_bytes(&[31, 0]).is_err());
    }
}
