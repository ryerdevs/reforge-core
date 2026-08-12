//! F4 slice 3: paquetes del WORLD ENTRY (fase Loading/Game) — verificados
//! contra `source/server/game/src/packet.h` y `char.cpp`:
//! - `TPacketGCMainCharacter` (48 B, header 113) — `packet.h:952-961`
//!   + `char.cpp:1539-1549` (MainCharacterPacket, sin BGM).
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

/// `TPacketGCAffectAdd` (22 B, header 126 — `packet.h:2032-2036`): un affect
/// activo (`LoadAffect` → `AddAffect` → paquete; el server manda UNO por
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

/// `TPacketGCMainCharacter` (47 B, header 113 — **layout del CLIENTE**,
/// `Packet.h:1349-1357`): header, dwVID, wRaceNum, szName[25], lx, ly, lz,
/// skill_group.
///
/// ⚠️ **DISCREPANCIA VERIFICADA (F4 slice 3.4)**: el struct del SERVIDOR
/// (`packet.h:952-961`) tiene además `BYTE empire` (48 B) — el cliente 40999
/// NO: parsea 47 B con `skill_group` en el offset 46. Emitir 48 B desalinea
/// TODO el stream del cliente (el byte sobrante corrompe los paquetes
/// siguientes → cierre limpio). **El cliente es el contrato congelado** — el
/// canal emite 47 B (el empire del 113 no existe en el cliente; el cliente
/// pone `m_dwMainActorEmpire = 0`, `PythonNetworkStreamPhaseLoading.cpp:200`).
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
    pub const HEADER: u8 = 113;

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
/// `realm::packets::points_packet`.
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
        assert_eq!(b[0], 113);
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
}
