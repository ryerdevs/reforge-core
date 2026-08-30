//! `protocol::guild` — GUILD: constantes + codificadores byte-exactos (parity
//! `packet.h:1640-1720` del server / `Packet.h:904-933` del cliente).
//!
//! C→S (80, `HEADER_CG_GUILD`): `TPacketCGGuild` (2 B: header+subheader,
//! Packet.h:923-927) + payload por subheader (`GetSubPacketSize`,
//! input_main.cpp:2425-2447). El slice reforge define `SUB_CG_CREATE = 1`
//! con nombre crudo de `GUILD_NAME_MAX_LEN+1` = 13 B (total 15 — mismo
//! buffer que `TPacketCGAnswerMakeGuild`, Packet.h:929-933). DIVERGENCIA
//! documentada: en el enum legacy (packet.h:1669-1684) el 1 es
//! REMOVE_MEMBER (+DWORD); la creación legacy va por GC_REQUEST_MAKE_GUILD
//! (82) / CG_ANSWER_MAKE_GUILD (81) — input_main.cpp:3161-3167. El INVITE
//! (sub 0, ADD_MEMBER) y su respuesta (sub 11, GUILD_INVITE_ANSWER) SÍ usan
//! los subs legacy reales (input_main.cpp:2486-2504 / 2749-2763).
//!
//! S→C (75, `HEADER_GC_GUILD` packet.h:178): `TPacketGCGuild` (4 B: header +
//! WORD size + subheader; packet.h:1686-1691) + payload del subheader.
//! `gc_info` = `GUILD_SUBHEADER_GC_INFO` con `TPacketGCGuildInfo`
//! (guild.h:58-69) — parity `SendGuildInfoPacket` guild.cpp:867-897.

/// `HEADER_GC_GUILD` (packet.h:178 — 75).
pub const GC_GUILD: u8 = 75;

/// Tamaño fijo de `TPacketCGGuild` (C→S base: header+subheader).
pub const CG_FIXED: usize = 2;
/// Total del CREATE del slice: base 2 B + nombre crudo de 13 B
/// (`GUILD_NAME_MAX_LEN+1` — mismo buffer que `TPacketCGAnswerMakeGuild`).
pub const CG_CREATE_TOTAL: usize = CG_FIXED + 13;
/// Subheader CREATE del slice. Divergencia: en el legacy es REMOVE_MEMBER.
pub const SUB_CG_CREATE: u8 = 1;
/// `GUILD_SUBHEADER_CG_ADD_MEMBER` (packet.h:1669 — 0): la INVITACIÓN a
/// guild (dispatch input_main.cpp:2486-2504) — payload DWORD con el VID del
/// invitado (`Find(vid)`, input_main.cpp:2489).
pub const SUB_CG_ADD_MEMBER: u8 = 0;
/// Total del ADD_MEMBER: base 2 B + DWORD vid.
pub const CG_ADD_MEMBER_TOTAL: usize = CG_FIXED + 4;
/// `GUILD_SUBHEADER_CG_WAR_DECLARE` (aditivo reforge, CG_GUILD 80): payload DWORD target_guild + BYTE war_type (parity `RequestDeclareWar` guild_war.cpp:290 — FIELD/BATTLE/FLAG).
pub const SUB_CG_WAR_DECLARE: u8 = 15;
pub const CG_WAR_DECLARE_TOTAL: usize = CG_FIXED + 5;
/// `GUILD_SUBHEADER_CG_GUILD_INVITE_ANSWER` (packet.h:1680 — 11): payload
/// DWORD guild_id + BYTE accept (dispatch input_main.cpp:2749-2763).
pub const SUB_CG_INVITE_ANSWER: u8 = 11;
/// Total del INVITE_ANSWER: base 2 B + DWORD + BYTE (GetSubPacketSize,
/// input_main.cpp:2443).
pub const CG_INVITE_ANSWER_TOTAL: usize = CG_FIXED + 5;
/// `GUILD_SUBHEADER_GC_GUILD_INVITE` (packet.h:1658 — 14): la invitación
/// que recibe el invitado (`CGuild::Invite` guild.cpp:1880-1890).
pub const SUB_GC_GUILD_INVITE: u8 = 14;
/// `GUILD_SUBHEADER_GC_INFO` (packet.h:1652 — 8): la guild creada.
pub const SUB_GC_INFO: u8 = 8;

/// `TPacketCGGuild` — 2 B (Packet.h:923-927): header + subheader.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TPacketCGGuild {
    pub header: u8,
    pub subheader: u8,
}

impl TPacketCGGuild {
    pub const SIZE: usize = CG_FIXED;
    pub const HEADER: u8 = crate::header::CG_GUILD;

    pub fn new(subheader: u8) -> Self {
        Self {
            header: Self::HEADER,
            subheader,
        }
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        [self.header, self.subheader]
    }
}

/// `TPacketGCGuild` (4 B, packet.h:1686-1691): header + WORD size + subheader.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TPacketGCGuild {
    pub header: u8,
    pub size: u16,
    pub subheader: u8,
}

impl TPacketGCGuild {
    pub const SIZE: usize = 4;
    pub const HEADER: u8 = GC_GUILD;

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

/// GC GUILD INFO de una guild recién creada (35 B de payload — parity
/// `SendGuildInfoPacket` guild.cpp:879-890): member_count 1 (el master),
/// max 32 (`GetMaxMemberCount` guild.cpp:1680 con level 1), exp 0, level 1,
/// nombre NUL-padded a 13, gold 0, has_land 0.
pub fn gc_info(guild_id: u32, master_pid: u32, name: &str) -> Vec<u8> {
    let mut body = [0u8; 35];
    body[0..2].copy_from_slice(&1u16.to_le_bytes()); // member_count
    body[2..4].copy_from_slice(&32u16.to_le_bytes()); // max_member_count
    body[4..8].copy_from_slice(&guild_id.to_le_bytes());
    body[8..12].copy_from_slice(&master_pid.to_le_bytes());
    // exp @12..16 = 0; level @16 = 1
    body[16] = 1;
    let name = &name.as_bytes()[..name.len().min(12)];
    body[17..17 + name.len()].copy_from_slice(name);
    // gold @30..34 = 0; has_land @34 = 0
    let head = TPacketGCGuild::new(SUB_GC_INFO, 35);
    let mut out = Vec::with_capacity(TPacketGCGuild::SIZE + 35);
    out.extend_from_slice(&head.to_bytes());
    out.extend_from_slice(&body);
    out
}

/// GC GUILD INVITE (sub 14): DWORD guild_id + nombre NUL-padded a 13 B
/// (`GUILD_NAME_MAX_LEN+1`) — parity `CGuild::Invite` guild.cpp:1880-1890
/// (size = 4 + 4 + 13 = 21 B).
pub fn gc_guild_invite(guild_id: u32, name: &str) -> Vec<u8> {
    let mut body = [0u8; 17];
    body[0..4].copy_from_slice(&guild_id.to_le_bytes());
    let name = &name.as_bytes()[..name.len().min(12)];
    body[4..4 + name.len()].copy_from_slice(name);
    let head = TPacketGCGuild::new(SUB_GC_GUILD_INVITE, 17);
    let mut out = Vec::with_capacity(TPacketGCGuild::SIZE + 17);
    out.extend_from_slice(&head.to_bytes());
    out.extend_from_slice(&body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden bytes del CREATE (slice): header 80 + sub 1; el payload de 13 B
    /// es del handler (wire crudo del C++ — sin codificador aquí).
    #[test]
    fn cg_guild_base_is_two_bytes() {
        let p = TPacketCGGuild::new(SUB_CG_CREATE);
        assert_eq!(p.to_bytes(), [80, 1]);
        assert_eq!(CG_CREATE_TOTAL, 15);
        assert_eq!(CG_FIXED, 2);
    }

    /// Golden bytes del GC GUILD INFO (parity guild.cpp:867-897): sobre
    /// 4 B + payload 35 B; size = 39; subheader 8.
    #[test]
    fn gc_info_golden_bytes() {
        let out = gc_info(77, 42, "Valientes");
        assert_eq!(out.len(), 39);
        assert_eq!(&out[..4], &[0x4b, 39, 0, SUB_GC_INFO]);
        assert_eq!(&out[4..8], &[1, 0, 32, 0], "count 1 / max 32");
        assert_eq!(&out[8..12], &77u32.to_le_bytes(), "guild_id");
        assert_eq!(&out[12..16], &42u32.to_le_bytes(), "master_pid");
        assert_eq!(&out[16..20], &[0, 0, 0, 0], "exp 0");
        assert_eq!(out[20], 1, "level 1");
        assert_eq!(&out[21..34], b"Valientes\0\0\0\0");
        assert_eq!(&out[34..38], &[0, 0, 0, 0], "gold 0");
        assert_eq!(out[38], 0, "has_land 0");
    }

    /// Golden bytes del GUILD INVITE (parity guild.cpp:1880-1890): sobre
    /// 4 B + payload 17 B (gid + nombre de 13), size 21, subheader 14;
    /// subs del INVITE/ANSWER del lado C→S.
    #[test]
    fn gc_guild_invite_golden_bytes() {
        let out = gc_guild_invite(0x01020304, "Valientes");
        assert_eq!(out.len(), 21);
        assert_eq!(&out[..4], &[0x4b, 21, 0, SUB_GC_GUILD_INVITE]);
        assert_eq!(&out[4..8], &[4, 3, 2, 1], "guild_id");
        assert_eq!(&out[8..21], b"Valientes\0\0\0\0");
        assert_eq!(SUB_CG_ADD_MEMBER, 0);
        assert_eq!(CG_ADD_MEMBER_TOTAL, 6);
        assert_eq!(SUB_CG_INVITE_ANSWER, 11);
        assert_eq!(CG_INVITE_ANSWER_TOTAL, 7);
    }
}
