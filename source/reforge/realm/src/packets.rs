//! F4: mapeos del flujo select/spawn — `PlayerRow`/`PlayerSummary` del crate
//! `database` -> paquetes wire de `protocol` (F0, byte-exactos).
//!
//! Contratos C++ (cada mapeo cita su file:line):
//! - `TSimplePlayer` (71 B packed) <- `ClientManagerLogin.cpp:324-383`
//!   (`CreateAccountPlayerDataFromRes`, branch sin cache: las 15 columnas del
//!   Q3) + `desc.cpp:965` (el game copia el TAccountTable tal cual).
//! - `TPacketGCLoginSuccess` (449 B) <- `desc.cpp:955-988`
//!   (`SendLoginSuccessPacket`: header 0x20, players[5], guilds, handle,
//!   random_key).
//! - `TPacketGCCharacterAdd` (37 B) <- `char.cpp:886-920` (`EncodeInsertPacket`).
//! - `TPacketGCCharacterAdditionalInfo` (70 B) <- `char.cpp:924-948`.
//!
//! Campos que el C++ calcula en runtime (speeds, affects, parts de items,
//! pk_mode, montura, flechas, guilds) NO viven en el row persistido: se mapean
//! a su default (0) y quedan documentados como GAP del slice (los alimentan
//! los subsistemas de F4/F5).

use database::player::{PlayerRow, PlayerSummary};
use protocol::{
    from_cstr, TPacketGCCharacterAdd, TPacketGCCharacterAdditionalInfo, TPacketGCLoginSuccess,
    TSimplePlayer, PLAYER_PER_ACCOUNT,
};

/// `CHAR_TYPE_PC` — `length.h:330` (enum ECharType: MONSTER=0, NPC=1, STONE=2,
/// WARP=3, DOOR=4, BUILDING=5, **PC=6**). El row siempre es un personaje.
const CHAR_TYPE_PC: u8 = 6;

/// Índices de `TPacketGCCharacterAdditionalInfo.awPart` (packet.h:860-870,
/// `CHR_EQUIPPART_*`; ACCE ON -> 5 slots). Solo los que el mapeo usa en
/// producción; WEAPON/HEAD/ACCE (GAP runtime) viven en los tests.
const EQUIPPART_ARMOR: usize = 0;
const EQUIPPART_HAIR: usize = 3;

/// `PlayerSummary` -> `TSimplePlayer` (71 B packed).
///
/// Parity `ClientManagerLogin.cpp:324-383` (branch sin cache — el C++ mapea
/// las columnas del Q3 una a una; los stats van como `BYTE`/`DWORD`).
/// `l_addr`/`w_port` quedan 0: el db los manda 0 (memset del TAccountTable) y
/// el game solo sobreescribe `lAddr` con la IP del proxy si `ENABLE_NEWSTUFF`
/// (`desc.cpp:969-972`).
pub fn summary_to_simple_player(s: &PlayerSummary) -> TSimplePlayer {
    TSimplePlayer {
        dw_id: s.id as u32,
        sz_name: from_cstr(&s.name),
        by_job: s.job as u8,
        by_level: s.level as u8,
        dw_play_minutes: s.playtime as u32,
        by_st: s.st as u8,
        by_ht: s.ht as u8,
        by_dx: s.dx as u8,
        by_iq: s.iq as u8,
        w_main_part: s.part_main as u32,
        b_change_name: s.change_name as u8,
        w_hair_part: s.part_hair as u32,
        // GAP: `part_acce` (columna ACCE del Q3) no está en PlayerSummary del
        // F3 (15 columnas sin ACCE) — el C++ con ENABLE_ACCE_COSTUME_SYSTEM lo
        // mapea de `pt->parts[PART_ACCE]` (`ClientManagerLogin.cpp:336`).
        w_acce_part: 0,
        b_dummy: [0; 4],
        x: s.x,
        y: s.y,
        l_addr: 0,
        w_port: 0,
        skill_group: s.skill_group as u8,
    }
}

/// `[Option<PlayerSummary>; 5]` (por slot del `player_index`) ->
/// `TPacketGCLoginSuccess` (449 B, header 0x20).
///
/// Parity `desc.cpp:955-988`:
/// - `players[i]` = mapeo por slot; `None` -> TSimplePlayer zeroed (dwID=0 —
///   el C++ deja el slot del memset del TAccountTable).
/// - `guild_id[i]`/`guild_name[i]` = 0/"" — el C++ los rellena desde
///   `CGuildManager::GetLinkedGuild` (`desc.cpp:973-984`); las guildas son
///   F5 (GAP documentado).
/// - `handle`/`random_key` los provee el caller (`desc.cpp:963-964`:
///   `GetHandle()` y `DESC_MANAGER::MakeRandomKey(handle)` — runtime del desc).
pub fn login_success(
    players: &[Option<PlayerSummary>; PLAYER_PER_ACCOUNT],
    handle: u32,
    random_key: u32,
) -> TPacketGCLoginSuccess {
    let mut out = TPacketGCLoginSuccess {
        header: TPacketGCLoginSuccess::HEADER,
        players: [TSimplePlayer {
            dw_id: 0,
            sz_name: [0; 25],
            by_job: 0,
            by_level: 0,
            dw_play_minutes: 0,
            by_st: 0,
            by_ht: 0,
            by_dx: 0,
            by_iq: 0,
            w_main_part: 0,
            b_change_name: 0,
            w_hair_part: 0,
            w_acce_part: 0,
            b_dummy: [0; 4],
            x: 0,
            y: 0,
            l_addr: 0,
            w_port: 0,
            skill_group: 0,
        }; PLAYER_PER_ACCOUNT],
        guild_id: [0; PLAYER_PER_ACCOUNT],
        guild_name: [[0u8; 13]; PLAYER_PER_ACCOUNT],
        handle,
        random_key,
    };
    for (i, slot) in players.iter().enumerate() {
        if let Some(s) = slot {
            out.players[i] = summary_to_simple_player(s);
        }
    }
    out
}

/// `PlayerRow` -> `TPacketGCCharacterAdd` (37 B, header 1).
///
/// Parity `char.cpp:886-920` (`EncodeInsertPacket`):
/// - `dw_vid` = row.id (el VID de un PC es su player id — `m_vid`).
/// - `b_type` = `CHAR_TYPE_PC` (6, `length.h:330` — `GetCharType()`).
/// - `angle` = 0.0: `GetRotation()` devuelve `fRot` que se inicializa a 0 al
///   cargar (el `dir` persistido NO alimenta el spawn — `SetRotation` solo lo
///   cambia en runtime, `char.cpp:2528-2531`).
/// - `x/y/z` = row.x/y/z (UNITS — el cliente divide por 100).
/// - `w_race_num` = row.job: `GetRaceNum()` para un PC sin polymorph =
///   `m_points.job` (`char.cpp:1634-1643`).
/// - GAP runtime: `b_moving_speed`/`b_attack_speed` (`GetLimitPoint`, se
///   calculan de stats), `b_state_flag` (`m_bAddChrState`) y `dw_affect_flag`
///   (afects cargados con `AffectRepo` -> flags) -> 0 aquí.
pub fn character_add(row: &PlayerRow) -> TPacketGCCharacterAdd {
    TPacketGCCharacterAdd::new(
        row.id as u32,
        0.0,
        row.x,
        row.y,
        row.z,
        CHAR_TYPE_PC,
        row.job as u32,
        0, // mov speed (runtime)
        0, // attack speed (runtime)
        0, // state flag (runtime)
        [0, 0], // affect flags (runtime)
    )
}

/// `PlayerRow` -> `TPacketGCCharacterAdditionalInfo` (70 B, header 136).
///
/// Parity `char.cpp:924-948`:
/// - `aw_part[ARMOR]` = row.part_main, `aw_part[HAIR]` = row.part_hair
///   (`GetPart(PART_MAIN/HAIR)` — el part persistido del último save).
/// - GAP runtime: `WEAPON`/`HEAD`/`ACCE` (se derivan de los items equipados),
///   `dw_mount_vnum`, `dw_arrow` (quiver), `b_pk_mode` -> 0.
/// - `b_empire` viene del ACCOUNT (el row no lo tiene) — parámetro del caller
///   (`m_bEmpire`, `char.cpp:939`).
/// - `dw_guild_id` = 0 (guildas F5; el C++: `GetGuild() ? GetID() : 0`).
/// - `dw_level` = row.level (`IsPC() ? GetLevel() : 0` — siempre PC aqui).
/// - `s_alignment` = row.alignment / 10 (`m_iAlignment / 10`, `char.cpp:947`).
pub fn character_additional_info(row: &PlayerRow, empire: u8) -> TPacketGCCharacterAdditionalInfo {
    let mut aw_part = [0u32; 5];
    aw_part[EQUIPPART_ARMOR] = row.part_main as u32;
    aw_part[EQUIPPART_HAIR] = row.part_hair as u32;
    TPacketGCCharacterAdditionalInfo {
        header: TPacketGCCharacterAdditionalInfo::HEADER,
        dw_vid: row.id as u32,
        name: from_cstr(&row.name),
        aw_part,
        b_empire: empire,
        dw_guild_id: 0,
        dw_level: row.level as u32,
        s_alignment: (row.alignment / 10) as i16,
        b_pk_mode: 0,
        dw_mount_vnum: 0,
        dw_arrow: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::player::{PlayerRow, PlayerSummary};

    /// Índices GAP del `awPart` (WEAPON/HEAD/ACCE — runtime de items,
    /// packet.h:860-870): usados solo por los tests del mapeo.
    const EQUIPPART_WEAPON: usize = 1;
    const EQUIPPART_HEAD: usize = 2;
    const EQUIPPART_ACCE: usize = 4;

    fn summary() -> PlayerSummary {
        PlayerSummary {
            id: 2,
            name: "ninja".into(),
            job: 1,
            level: 1,
            playtime: 3600,
            st: 30,
            ht: 30,
            dx: 30,
            iq: 30,
            part_main: 0x1122_3344,
            part_hair: 0xAABB_CCDD,
            x: 969600,
            y: 278400,
            skill_group: 3,
            change_name: 0,
        }
    }

    fn row() -> PlayerRow {
        // dummy_row equivalente al de database::player::tests (campos del
        // mapeo; el resto no participa).
        PlayerRow {
            id: 2,
            name: "ninja".into(),
            job: 1,
            voice: 0,
            dir: 0,
            x: 969600,
            y: 278400,
            z: 0,
            map_index: 41,
            exit_x: 0,
            exit_y: 0,
            exit_map_index: 0,
            hp: 100,
            mp: 100,
            stamina: 100,
            random_hp: 0,
            random_sp: 0,
            playtime: 0,
            gold: 0,
            level: 5,
            level_step: 0,
            st: 30,
            ht: 30,
            dx: 30,
            iq: 30,
            exp: 0,
            stat_point: 0,
            skill_point: 0,
            sub_skill_point: 0,
            stat_reset_count: 0,
            part_base: 0,
            part_hair: 0xAABB_CCDD,
            part_main: 0x1122_3344,
            skill_level: None,
            quickslot: None,
            skill_group: 3,
            alignment: 1234,
            horse_level: 0,
            horse_riding: 0,
            horse_hp: 0,
            horse_hp_droptime: 0,
            horse_stamina: 0,
            logoff_interval: 0.0,
            horse_skill_point: 0,
        }
    }

    /// summary -> TSimplePlayer: 71 B y campos spot (parity
    /// `ClientManagerLogin.cpp:324-383`).
    #[test]
    fn summary_to_simple_player_fields_and_size() {
        let p = summary_to_simple_player(&summary());
        let b = p.to_bytes();
        assert_eq!(b.len(), TSimplePlayer::SIZE, "71 B packed");
        assert_eq!(p.dw_id, 2);
        assert_eq!(p.name(), "ninja");
        assert_eq!(p.by_job, 1);
        assert_eq!(p.by_level, 1);
        assert_eq!(p.dw_play_minutes, 3600);
        assert_eq!((p.by_st, p.by_ht, p.by_dx, p.by_iq), (30, 30, 30, 30));
        assert_eq!(p.w_main_part, 0x1122_3344);
        assert_eq!(p.w_hair_part, 0xAABB_CCDD);
        assert_eq!(p.w_acce_part, 0, "GAP part_acce (documentado)");
        assert_eq!((p.x, p.y), (969600, 278400), "units crudos");
        assert_eq!(p.skill_group, 3);
        assert_eq!((p.l_addr, p.w_port), (0, 0), "el db los manda 0 (ENABLE_NEWSTUFF lo sobreescribe)");
        // Bytes spot en el wire (LE): dwID@0, name@4, byJob@29, x@56.
        assert_eq!(&b[0..4], &[2, 0, 0, 0]);
        assert_eq!(&b[4..10], b"ninja\0");
        assert_eq!(b[29], 1);
        assert_eq!(&b[56..60], &969600u32.to_le_bytes(), "x=969600 @56");
        assert_eq!(&b[60..64], &278400u32.to_le_bytes(), "y=278400 @60");
    }

    /// login_success: 449 B, slots None -> zeroed, handle/random_key en los
    /// offsets del spec (desc.cpp:955-988).
    #[test]
    fn login_success_size_slots_and_tail() {
        let mut slots: [Option<PlayerSummary>; 5] = [None, None, None, None, None];
        slots[0] = Some(summary());
        let p = login_success(&slots, 0xDEAD_BEEF, 0xCAFE_BABE);
        let b = p.to_bytes();
        assert_eq!(b.len(), TPacketGCLoginSuccess::SIZE, "449 B packed");
        assert_eq!(b[0], TPacketGCLoginSuccess::HEADER, "header 0x20");
        // Slot 0 con datos; slots 1..4 zeroed (dwID=0).
        assert_eq!(p.players[0].dw_id, 2);
        for i in 1..5 {
            assert_eq!(p.players[i].dw_id, 0, "slot {i} vacio -> zeroed");
        }
        // guilds a 0 (F5).
        assert_eq!(p.guild_id, [0; 5]);
        assert!(p.guild_name.iter().all(|g| *g == [0u8; 13]));
        // handle@441, random_key@445 (spec §3 / protocol).
        assert_eq!(&b[441..445], &0xDEAD_BEEFu32.to_le_bytes());
        assert_eq!(&b[445..449], &0xCAFE_BABEu32.to_le_bytes());
        assert_eq!(p.handle, 0xDEAD_BEEF);
        assert_eq!(p.random_key, 0xCAFE_BABE);
    }

    /// character_add: 37 B y campos spot (parity char.cpp:886-920).
    #[test]
    fn character_add_fields_and_size() {
        let p = character_add(&row());
        let b = p.to_bytes();
        assert_eq!(b.len(), TPacketGCCharacterAdd::SIZE, "37 B");
        assert_eq!(b[0], TPacketGCCharacterAdd::HEADER, "header 1");
        assert_eq!(p.dw_vid, 2, "VID = player id");
        assert_eq!(p.angle, 0.0, "fRot inicial 0 (dir no alimenta el spawn)");
        assert_eq!((p.x, p.y, p.z), (969600, 278400, 0), "units");
        assert_eq!(p.b_type, CHAR_TYPE_PC, "CHAR_TYPE_PC = 6");
        assert_eq!(p.w_race_num, 1, "GetRaceNum() = job para PC");
        assert_eq!((p.b_moving_speed, p.b_attack_speed, p.b_state_flag), (0, 0, 0), "runtime GAP");
        assert_eq!(p.dw_affect_flag, [0, 0], "runtime GAP");
        // wRaceNum@22 (LE) en el wire.
        assert_eq!(&b[22..26], &[1, 0, 0, 0]);
    }

    /// character_additional_info: 70 B y campos spot (parity char.cpp:924-948).
    #[test]
    fn character_additional_info_fields_and_size() {
        let p = character_additional_info(&row(), 3);
        let b = p.to_bytes();
        assert_eq!(b.len(), TPacketGCCharacterAdditionalInfo::SIZE, "70 B");
        assert_eq!(b[0], TPacketGCCharacterAdditionalInfo::HEADER, "header 136");
        assert_eq!(p.dw_vid, 2);
        assert_eq!(p.name(), "ninja");
        assert_eq!(p.aw_part[EQUIPPART_ARMOR], 0x1122_3344, "PART_MAIN persistido");
        assert_eq!(p.aw_part[EQUIPPART_HAIR], 0xAABB_CCDD, "PART_HAIR persistido");
        assert_eq!(p.aw_part[EQUIPPART_WEAPON], 0, "GAP items runtime");
        assert_eq!(p.aw_part[EQUIPPART_HEAD], 0, "GAP items runtime");
        assert_eq!(p.aw_part[EQUIPPART_ACCE], 0, "GAP items runtime");
        assert_eq!(p.b_empire, 3, "del account (parametro)");
        assert_eq!(p.dw_level, 5, "IsPC() -> GetLevel()");
        assert_eq!(p.s_alignment, 123, "m_iAlignment / 10");
        assert_eq!(p.dw_guild_id, 0, "guildas F5");
        // sAlignment@59 (i16 LE) en el wire.
        assert_eq!(&b[59..61], &123i16.to_le_bytes());
        // name@5 en el wire (25 B: "ninja\0" + ceros).
        assert_eq!(&b[5..11], b"ninja\0");
    }

    /// Los 5 slots con datos -> 5×71 B ocupados (tamaño total 449).
    #[test]
    fn login_success_full_slots_size() {
        let s = summary();
        let (a, b, c) = (summary(), summary(), summary());
        let slots = [Some(s.clone()), Some(s), Some(a), Some(b), Some(c)];
        let p = login_success(&slots, 1, 2);
        assert_eq!(p.to_bytes().len(), 449);
        assert!(p.players.iter().all(|pl| pl.dw_id == 2));
    }
}
