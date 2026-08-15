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

use database::affect::AffectRow;
use database::item::ItemRow;
use database::land::LandRow;
use database::player::{PlayerRow, PlayerSummary};
use protocol::world::{
    land_list_bytes as wire_land_list, TLandPacketElement, TPacketGCAffectAdd, TPacketGCItemSet,
    TPacketGCMainCharacter, TPacketGCPoints, TPacketGCQuickSlotAdd, TPacketGCSkillLevel,
    TPacketAffectElement, TItemPos, TPlayerSkill, TQuickslot,
};
use protocol::{
    from_cstr, TPacketGCCharacterAdd, TPacketGCCharacterAdditionalInfo, TPacketGCCharacterUpdate,
    TPacketGCLoginSuccess, TSimplePlayer, PLAYER_PER_ACCOUNT,
};

/// `CHAR_TYPE_PC` — `length.h:330` (enum ECharType: MONSTER=0, NPC=1, STONE=2,
/// WARP=3, DOOR=4, BUILDING=5, **PC=6**). El row siempre es un personaje.
const CHAR_TYPE_PC: u8 = 6;

/// Índices de `TPacketGCCharacterAdditionalInfo.awPart` (packet.h:860-870,
/// `CHR_EQUIPPART_*`; ACCE ON -> 5 slots). WEAPON/HEAD/ACCE se alimentan de
/// los items equipados (`equipped_parts` — F5.3).
const EQUIPPART_ARMOR: usize = 0;
const EQUIPPART_WEAPON: usize = 1;
const EQUIPPART_HEAD: usize = 2;
const EQUIPPART_HAIR: usize = 3;
const EQUIPPART_ACCE: usize = 4;

/// Cell base del window EQUIPMENT en el wire (`INVENTORY_MAX_NUM` con
/// `ENABLE_EXTEND_INVEN_SYSTEM` — length.h:29 + CommonDefines.h:32):
/// el cell del equip = `180 + wear` (length.h:827 `IsEquipPosition`).
const EQUIP_CELL_BASE: u16 = 180;

/// Los slots del equip del C++ (length.h:101-111 — `WEAR_BODY // 0`,
/// `WEAR_HEAD // 1`, `WEAR_WEAPON // 4`).
const WEAR_BODY: u16 = 0;
const WEAR_HEAD: u16 = 1;
const WEAR_WEAPON: u16 = 4;

/// `ComputeParts` subset (parity `char.cpp:924-932` + `item.cpp:793,833` —
/// `SetPart(PART_WEAPON/MAIN, GetVnum())`): los 5 parts del
/// ADDITIONAL_INFO desde los items EQUIPMENT del inventario. El part de un
/// item = SU VNUM (el cliente resuelve el modelo por vnum). HAIR viene del
/// row persistido (`part_hair` — parity char.cpp:1710); ACCE = 0 (GAP).
/// Los slots sin item quedan 0 (parity: el C++ parte de 0 al quitar).
pub fn equipped_parts(row: &PlayerRow, inventory: &[ItemRow]) -> [u32; 5] {
    let mut parts = [0u32; 5];
    parts[EQUIPPART_HAIR] = row.part_hair as u32;
    for it in inventory.iter().filter(|i| i.window == "EQUIPMENT") {
        let wear = it.pos as u16 - EQUIP_CELL_BASE;
        match wear {
            WEAR_BODY => parts[EQUIPPART_ARMOR] = it.vnum as u32,
            WEAR_HEAD => parts[EQUIPPART_HEAD] = it.vnum as u32,
            WEAR_WEAPON => parts[EQUIPPART_WEAPON] = it.vnum as u32,
            _ => {} // otros wear (FOOTS/WRIST/NECK/...) sin part en el subset
        }
    }
    parts
}

/// Bits `WEARABLE_*` del `wearflag` del item_proto (item_length.h:379-392).
pub mod wearable {
    pub const BODY: u32 = 1 << 0;
    pub const HEAD: u32 = 1 << 1;
    pub const FOOTS: u32 = 1 << 2;
    pub const WRIST: u32 = 1 << 3;
    pub const WEAPON: u32 = 1 << 4;
    pub const NECK: u32 = 1 << 5;
    pub const EAR: u32 = 1 << 6;
    pub const UNIQUE: u32 = 1 << 7;
    pub const SHIELD: u32 = 1 << 8;
    pub const ARROW: u32 = 1 << 9;
    pub const HAIR: u32 = 1 << 10;
    pub const ABILITY: u32 = 1 << 11;
}

/// `FindEquipCell` parity (item.cpp:509-623): el slot del equip de un item
/// según los bits `WEARABLE_*` de su `wearflag`. Orden EXACTO del C++
/// (item.cpp:568-592): BODY→0, HEAD→1, FOOTS→2, WRIST→3, WEAPON→4,
/// SHIELD→10, NECK→5, EAR→6, ARROW→9, UNIQUE→7, ABILITY→11 (length.h:99-119).
/// `None` = no equipable (wearflag 0, o solo bits fuera del subset: HAIR,
/// PENDANT, GLOVE — el C++ los gestiona por otros paths).
pub fn find_equip_cell(proto: &database::item::ProtoItem) -> Option<u16> {
    let w = proto.wear_flag as u32;
    if w == 0 {
        return None; // item.cpp:511-519 — sin wearflag no es equipable
    }
    // El orden de los else-if del C++ es el que decide (item.cpp:568-592):
    // un item con varios bits cae en el PRIMERO de este orden.
    if w & wearable::BODY != 0 {
        Some(0) // WEAR_BODY
    } else if w & wearable::HEAD != 0 {
        Some(1) // WEAR_HEAD
    } else if w & wearable::FOOTS != 0 {
        Some(2) // WEAR_FOOTS
    } else if w & wearable::WRIST != 0 {
        Some(3) // WEAR_WRIST
    } else if w & wearable::WEAPON != 0 {
        Some(4) // WEAR_WEAPON
    } else if w & wearable::SHIELD != 0 {
        Some(10) // WEAR_SHIELD
    } else if w & wearable::NECK != 0 {
        Some(5) // WEAR_NECK
    } else if w & wearable::EAR != 0 {
        Some(6) // WEAR_EAR
    } else if w & wearable::ARROW != 0 {
        Some(9) // WEAR_ARROW
    } else if w & wearable::UNIQUE != 0 {
        Some(7) // WEAR_UNIQUE1 (el C++ usa 1 si libre, si no 2)
    } else if w & wearable::ABILITY != 0 {
        Some(11) // WEAR_ABILITY1 (el C++ busca el primero libre)
    } else {
        None // solo HAIR/PENDANT/GLOVE — fuera del subset (GAP)
    }
}

/// `PlayerSummary` -> `TSimplePlayer` (71 B packed).
///
/// Parity `ClientManagerLogin.cpp:324-383` (branch sin cache — el C++ mapea
/// las columnas del Q3 una a una; los stats van como `BYTE`/`DWORD`).
/// `l_addr`/`w_port` = la dirección del server de juego que el cliente usa en
/// el DirectEnter (`introselect.py:739-741` → `ConnectGameServer` →
/// `CNetworkStream::Connect(lAddr, wPort)`, `PythonNetworkStream.cpp:458-469`):
/// - `l_addr`: `inet_addr(ip)` — network byte order (el cliente decodifica los
///   4 bytes desde el byte BAJO, `NetStream.cpp:467-473`; el C++ lo rellena
///   con `inet_addr(g_stProxyIP)`, `desc.cpp:970-971` ENABLE_NEWSTUFF).
/// - `w_port`: el puerto del canal en HOST order (el cliente hace `htons`,
///   `NetAddress.cpp:79-82`).
/// Con 0/0 el DirectEnter conecta a `0.0.0.0:0` → `OnConnectFailure` →
/// ClosePhase → vuelta al login en silencio (evidencia del slice 3.5).
pub fn summary_to_simple_player(s: &PlayerSummary, l_addr: u32, w_port: u16) -> TSimplePlayer {
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
        l_addr: l_addr as i32, // el wire es long x86 — los bits del inet_addr
        w_port,
        skill_group: s.skill_group as u8,
    }
}

/// `"a.b.c.d"` -> DWORD en el formato de `inet_addr` (el valor que en memoria
/// LE tiene los bytes [a, b, c, d] = `d<<24 | c<<16 | b<<8 | a` — el cliente
/// decodifica los 4 bytes desde el byte bajo, `NetStream.cpp:467-473`).
pub fn ip_to_inet_addr(ip: &str) -> Result<u32, String> {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 {
        return Err(format!("ip inválida: {ip}"));
    }
    let mut octets = [0u32; 4];
    for (i, p) in parts.iter().enumerate() {
        let octet: u32 = p
            .parse()
            .map_err(|_| format!("ip inválida: {ip} (octeto '{p}')"))?;
        if octet > 255 {
            return Err(format!("ip inválida: {ip} (octeto {octet} > 255)"));
        }
        octets[i] = octet;
    }
    Ok((octets[3] << 24) | (octets[2] << 16) | (octets[1] << 8) | octets[0])
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
/// - `server_ip`/`server_port`: el server de juego del DirectEnter (ver
///   `summary_to_simple_player`) — el C++ los pone en `lAddr`/`wPort` de cada
///   slot (desc.cpp:969-972; el wPort queda 0 en el C++ pero el cliente lo
///   usa — el canal los rellena con la dirección real del listener).
pub fn login_success(
    players: &[Option<PlayerSummary>; PLAYER_PER_ACCOUNT],
    handle: u32,
    random_key: u32,
    server_ip: u32,
    server_port: u16,
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
            out.players[i] = summary_to_simple_player(s, server_ip, server_port);
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
/// - `b_moving_speed` = 100, `b_attack_speed` = 100: `GetLimitPoint(
///   POINT_MOV_SPEED/ATT_SPEED)` — ComputePoints fija ambos a 100 para un PC
///   (`char.cpp:2245-2246`). El cliente los usa DIRECTOS (SetMoveSpeed(x/100)
///   — InstanceBaseMovement.cpp:20); un 0 congela al personaje (no avanza,
///   no refresca su z del terreno → invisible) y bloquea las animaciones de
///   ataque (`m_fAtkSpd < 1.0f` → skip, ActorInstanceBattle.cpp:587).
/// - GAP runtime: `b_state_flag` (`m_bAddChrState`) y `dw_affect_flag`
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
        100, // mov speed (parity char.cpp:2245)
        100, // attack speed (parity char.cpp:2246)
        0, // state flag (runtime)
        [0, 0], // affect flags (runtime)
    )
}

/// `PlayerRow` -> `TPacketGCCharacterAdditionalInfo` (70 B, header 136).
///
/// Parity `char.cpp:924-948`:
/// - `aw_part[ARMOR]` = row.part_main, `aw_part[HAIR]` = row.part_hair
///   (`GetPart(PART_MAIN/HAIR)` — el part persistido del último save).
/// - GAP runtime: `WEAPON`/`HEAD`/`ACCE` (se derivan de los items equipados —
///   ver `character_additional_info_with_parts` + `equipped_parts`, F5.3),
///   `dw_mount_vnum`, `dw_arrow` (quiver), `b_pk_mode` -> 0.
/// - `b_empire` viene del ACCOUNT (el row no lo tiene) — parámetro del caller
///   (`m_bEmpire`, `char.cpp:939`).
/// - `dw_guild_id` = 0 (guildas F5; el C++: `GetGuild() ? GetID() : 0`).
/// - `dw_level` = row.level (`IsPC() ? GetLevel() : 0` — siempre PC aqui).
/// - `s_alignment` = row.alignment / 10 (`m_iAlignment / 10`, `char.cpp:947`).
pub fn character_additional_info(row: &PlayerRow, empire: u8) -> TPacketGCCharacterAdditionalInfo {
    // Default: los parts persistidos del row (sin items — GAP heredado).
    let parts = equipped_parts(row, &[]);
    character_additional_info_with_parts(row, empire, &parts, 0)
}

/// Igual que `character_additional_info` pero con los 5 parts COMPUTADOS del
/// runtime (F5.3 — `equipped_parts`: el personaje muestra el arma/armadura
/// equipada; el C++ los deriva de `GetPart()` tras `SetPart` al equipar,
/// item.cpp:793,833). `arrow_count` = count de flechas equipadas (dw_arrow —
/// ENABLE_QUIVER_SYSTEM, Packet.h:1229; el C++ `GetArrowAndBow` lo usa para
/// mostrar el count; 0 = sin flechas — parity `GetCount()`).
pub fn character_additional_info_with_parts(
    row: &PlayerRow,
    empire: u8,
    parts: &[u32; 5],
    arrow_count: u32,
) -> TPacketGCCharacterAdditionalInfo {
    let mut aw_part = [0u32; 5];
    aw_part.copy_from_slice(parts);
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
        dw_arrow: arrow_count,
    }
}

/// `TPacketGCCharacterUpdate` (header 19, 51 B) — el update del personaje
/// YA EN el mundo (parity `UpdatePacket` char.cpp:1017-1052): el C++ lo manda
/// al equipar/desequipar (`CItem::EquipTo` item.cpp:1004-1005 y
/// `CItem::Unequip`) para que el cliente recalcule el arma (ATT_MIN/ATT_MAX —
/// `__SetWeaponPower` lee value3/value4 del item por el part del arma,
/// PythonNetworkStreamPhaseGameActor.cpp:38-73) y refresque la ventana
/// (`__RecvCharacterUpdatePacket` → `__SetWeaponPower` + `__RefreshStatus`).
/// El ADDITIONAL_INFO (136) NO sirve en runtime: es el paquete de la
/// secuencia de ENTRADA (el cliente lo aplica solo si el VID coincide con el
/// `s_kNetActorData` pendiente — PythonNetworkStreamPhaseGameActor.cpp:153,
/// 165). Speeds = 100 (GetLimitPoint — char.cpp:1025-1026); flags/guild/
/// montura a 0 (sin affects/mount — F5); `s_alignment` = row.alignment/10
/// (char.cpp:1034); `dw_arrow` = count de flechas equipadas (QUIVER).
pub fn character_update_with_parts(
    row: &PlayerRow,
    parts: &[u32; 5],
    arrow_count: u32,
) -> TPacketGCCharacterUpdate {
    let mut aw_part = [0u32; 5];
    aw_part.copy_from_slice(parts);
    TPacketGCCharacterUpdate {
        header: TPacketGCCharacterUpdate::HEADER,
        dw_vid: row.id as u32,
        aw_part,
        b_moving_speed: 100,
        b_attack_speed: 100,
        b_state_flag: 0,
        dw_affect_flag: [0, 0],
        dw_guild_id: 0,
        s_alignment: (row.alignment / 10) as i16,
        b_pk_mode: 0,
        dw_mount_vnum: 0,
        dw_arrow: arrow_count,
    }
}

// ---------------------------------------------------------------------------
// F4 slice 3: paquetes del world entry (fase Loading/Game)
// ---------------------------------------------------------------------------

/// Índices del enum `EPointTypes` (`char.h:133+`) que el entry usa — el
/// resto del array (255 INTs) va a 0.
const POINT_LEVEL: usize = 1;
const POINT_VOICE: usize = 2;
const POINT_EXP: usize = 3;
const POINT_NEXT_EXP: usize = 4;
const POINT_HP: usize = 5;
const POINT_MAX_HP: usize = 6;
const POINT_SP: usize = 7;
const POINT_MAX_SP: usize = 8;
const POINT_STAMINA: usize = 9;
const POINT_MAX_STAMINA: usize = 10;
const POINT_GOLD: usize = 11;
const POINT_ST: usize = 12;
const POINT_HT: usize = 13;
const POINT_DX: usize = 14;
const POINT_IQ: usize = 15;
const POINT_ATT_SPEED: usize = 17;
const POINT_MOV_SPEED: usize = 19;
const POINT_CASTING_SPEED: usize = 21;
    const POINT_LEVEL_STEP: usize = 25;
    const POINT_STAT: usize = 26;
    const POINT_SUB_SKILL: usize = 27;
    const POINT_SKILL: usize = 28;
    const POINT_PLAYTIME: usize = 31;

    // Battle points — índices del enum del SERVIDOR C++ (`char.h:152-166`).
    // El CLIENTE S3ll lee por sus propios índices: su DEF_GRADE (20) = el
    // CLIENT_DEF_GRADE del server (el "show def" — INTERNATIONAL_VERSION,
    // char.cpp:2147 — la ventana del personaje muestra level+HT+armor) y su
    // ataque = ATT_MIN/ATT_MAX (29/30 = WEAPON_MIN/MAX del server — el daño
    // del arma; el C++ NUNCA los llena — la ventana mostraba 0 — el rewrite
    // sí los llena). Los grades 16/18 alimentan el combate.
    const POINT_DEF_GRADE: usize = 16; // char.h:152 (el DEF real — combate)
    const POINT_ATT_GRADE: usize = 18; // char.h:154 (el ATK base — combate)
    const POINT_CLIENT_DEF_GRADE: usize = 20; // char.h:156 (el show def — ventana)
    const POINT_MAGIC_ATT_GRADE: usize = 22; // char.h:158
    const POINT_MAGIC_DEF_GRADE: usize = 23; // char.h:159
    const POINT_WEAPON_MIN: usize = 29; // char.h:165 (daño min del arma — ventana)
    const POINT_WEAPON_MAX: usize = 30; // char.h:166 (daño max del arma — ventana)

/// Tabla `JobInitialPoints[JOB_MAX_NUM]` del C++ (`constants.cpp:18-24` —
/// por JOB: st, ht, dx, iq, max_hp, max_sp, hp_per_ht, sp_per_iq, ...,
/// max_stamina, stamina_per_con). Solo los campos del subset ComputePoints.
struct JobPoints {
    max_hp: i32,
    hp_per_ht: i32,
    max_sp: i32,
    sp_per_iq: i32,
    max_stamina: i32,
    stamina_per_con: i32,
}

const JOB_POINTS: [JobPoints; 4] = [
    // JOB_WARRIOR (0)
    JobPoints { max_hp: 600, hp_per_ht: 40, max_sp: 200, sp_per_iq: 20, max_stamina: 800, stamina_per_con: 5 },
    // JOB_ASSASSIN (1)
    JobPoints { max_hp: 650, hp_per_ht: 40, max_sp: 200, sp_per_iq: 20, max_stamina: 800, stamina_per_con: 5 },
    // JOB_SURA (2)
    JobPoints { max_hp: 650, hp_per_ht: 40, max_sp: 200, sp_per_iq: 20, max_stamina: 800, stamina_per_con: 5 },
    // JOB_SHAMAN (3)
    JobPoints { max_hp: 700, hp_per_ht: 40, max_sp: 200, sp_per_iq: 20, max_stamina: 800, stamina_per_con: 5 },
];

/// `RaceToJob` (`input_login.cpp:356-405` + `char.h:48-62`: MAIN_RACE_WARRIOR_M
/// =0, ASSASSIN_W=1, SURA_M=2, SHAMAN_W=3, WARRIOR_W=4, ASSASSIN_M=5,
/// SURA_W=6, SHAMAN_M=7, WOLFMAN_M=8): el `job` almacenado en el player ES el
/// race. Sin WOLFMAN en el subset (ningún personaje lo usa; si llega -> Err
/// documentado en el caller).
pub fn race_to_job(race: i16) -> Result<i16, String> {
    Ok(match race {
        0 | 4 => 0, // WARRIOR
        1 | 5 => 1, // ASSASSIN
        2 | 6 => 2, // SURA
        3 | 7 => 3, // SHAMAN
        other => return Err(format!("race_to_job: race {other} fuera del subset (0..7)")),
    })
}

/// Subset de `ComputePoints` (`char.cpp:2228-2232` + `:2245-2248`, PC branch):
/// los máximos y las speeds base — las fórmulas EXACTAS del C++:
/// `iMaxHP = JobInitialPoints[job].max_hp + random_hp + ht * hp_per_ht`
/// `iMaxSP = JobInitialPoints[job].max_sp + random_sp + iq * sp_per_iq`
/// `iMaxStamina = JobInitialPoints[job].max_stamina + ht * stamina_per_con`
/// `MOV_SPEED = ATT_SPEED = CASTING_SPEED = 100`
/// (el bonus SKILL_ADD_HP y el resto de derivados — defensas, regens,
/// resistencias — quedan fuera: requieren skills/items, son F5).
pub fn compute_max_points(row: &PlayerRow) -> Result<[i32; 3], String> {
    let job = race_to_job(row.job)?;
    let jp = &JOB_POINTS[job as usize];
    let max_hp = jp.max_hp + i32::from(row.random_hp) + i32::from(row.ht) * jp.hp_per_ht;
    let max_sp = jp.max_sp + i32::from(row.random_sp) + i32::from(row.iq) * jp.sp_per_iq;
    let max_stamina = jp.max_stamina + i32::from(row.ht) * jp.stamina_per_con;
    Ok([max_hp, max_sp, max_stamina])
}

/// Battle points del jugador (parity `ComputeBattlePoints`, char.cpp:2051-2152
/// — subset PC sin montura/bonos): ataque = level×2 + stat del job (el arma
/// NO entra — su daño va en `weapon_min/max` para la ventana); defensa =
/// level + HT/1.25 + armadura; grades mágicos. El caller los computa con los
/// protos cargados (async) y la sesión los CACHEA (`Session::battle` — el
/// `points_packet` los lee en todos los caminos sin reload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BattlePoints {
    /// POINT_ATT_GRADE (18) — el ATK base (combate).
    pub attack_grade: i32,
    /// POINT_DEF_GRADE (16) — el DEF real (combate).
    pub def_grade: i32,
    /// POINT_CLIENT_DEF_GRADE (20) — el "show def" (level + HT + armor) —
    /// lo que la ventana del personaje del cliente muestra (DEF_GRADE S3ll).
    pub client_def_grade: i32,
    /// POINT_MAGIC_ATT_GRADE (22).
    pub magic_att_grade: i32,
    /// POINT_MAGIC_DEF_GRADE (23).
    pub magic_def_grade: i32,
    /// POINT_WEAPON_MIN (29) — daño min del arma (`value3` del item —
    /// `GetValue(3)`, battle.cpp:460).
    pub weapon_min: i32,
    /// POINT_WEAPON_MAX (30) — daño max del arma (`value4` del item —
    /// `GetValue(4)`, battle.cpp:461).
    pub weapon_max: i32,
}

/// `ComputeBattlePoints` PC (char.cpp:2051-2152, sin montura/bonos — los
/// bonus y la montura son F5):
/// - ataque: `level*2 + iStatAtk` (WARRIOR/SURA 2×ST; ASSASSIN
///   `(4×ST+2×DX)/3`; SHAMAN `(4×ST+2×IQ)/3` — char.cpp:2064-2092).
/// - defensa: `level + (int)(HT/1.25)` + `iArmor` (la suma value1+2×value5
///   de los ARMOR BODY/HEAD/FOOTS/SHIELD — `equipped_armor` del canal,
///   char.cpp:2119-2127); `client_def` = `(level+HT+armor) - def` (el show
///   def de la ventana — char.cpp:2146-2147).
/// - mágicos: `level*2 + IQ*2` / `level + (IQ*3+HT)/3 + armor/2`
///   (char.cpp:2150-2151).
/// - `weapon_min/max` = value3/value4 del arma equipada (el daño físico del
///   arma — `GetValue(3)/GetValue(4)`, battle.cpp:460-461; el cliente lee los
///   mismos values en `__SetWeaponPower`, PythonNetworkStreamPhaseGameActor.
///   cpp:50-51). El C++ NO los llena (POINT_WEAPON_MIN/MAX quedan 0 —
///   char.h:165-166): el ATT_MIN/ATT_MAX visible del cliente se calcula
///   LOCALMENTE desde el part del arma del GC_CHARACTER_UPDATE, así que el
///   rewrite los llena solo como información del GC_POINTS.
pub fn compute_battle_points(
    row: &PlayerRow,
    weapon: Option<&database::item::ProtoItem>,
    armor_sum: i32,
) -> BattlePoints {
    let st = i32::from(row.st);
    let dx = i32::from(row.dx);
    let iq = i32::from(row.iq);
    let ht = i32::from(row.ht);
    let level = i32::from(row.level);
    let stat_atk = match race_to_job(row.job).unwrap_or(0) {
        1 => (4 * st + 2 * dx) / 3, // ASSASSIN (char.cpp:2071-2073)
        3 => (4 * st + 2 * iq) / 3, // SHAMAN (char.cpp:2075-2077)
        _ => 2 * st,                // WARRIOR/SURA/default (char.cpp:2066-2069)
    };
    let attack_grade = level * 2 + stat_atk;
    let def_real = level + (ht * 4) / 5; // (int)(HT / 1.25) — char.cpp:2114
    let def_grade = def_real + armor_sum;
    let client_def_grade = level + ht + armor_sum - def_grade; // char.cpp:2147
    let magic_att_grade = level * 2 + iq * 2; // char.cpp:2150
    let magic_def_grade = level + (iq * 3 + ht) / 3 + armor_sum / 2; // char.cpp:2151
    let weapon_min = weapon.map(|w| w.values[3]).unwrap_or(0);
    let weapon_max = weapon.map(|w| w.values[4]).unwrap_or(0);
    BattlePoints {
        attack_grade,
        def_grade,
        client_def_grade,
        magic_att_grade,
        magic_def_grade,
        weapon_min,
        weapon_max,
    }
}

/// `PlayerRow` -> `TPacketGCPoints` (1021 B, header 16).
///
/// Parity `char.cpp:1553-1581` (PointsPacket): los puntos DIRECTOS del row
/// (level/exp/hp/sp/stamina/gold/st/ht/dx/iq/level_step/stat/skill points/
/// playtime) + `POINT_VOICE` + los MAXIMOS y speeds del subset
/// `ComputePoints` (`compute_max_points`, char.cpp:2228-2248) + `NEXT_EXP`
/// (del caller — `exp_table[level]`, char.cpp:7190) + los `BattlePoints`
/// (ComputeBattlePoints — la sesión los cachea al entry/equip/unequip; el
/// resto de derivados — regens/resistencias/bonos — siguen a 0, F5).
pub fn points_packet(row: &PlayerRow, next_exp: i64, battle: &BattlePoints) -> TPacketGCPoints {
    let mut p = TPacketGCPoints { header: TPacketGCPoints::HEADER, points: [0; 255] };
    p.points[POINT_LEVEL] = i32::from(row.level);
    p.points[POINT_VOICE] = i32::from(row.voice);
    p.points[POINT_EXP] = row.exp;
    p.points[POINT_NEXT_EXP] = next_exp as i32;
    p.points[POINT_HP] = row.hp;
    p.points[POINT_SP] = row.mp;
    p.points[POINT_STAMINA] = i32::from(row.stamina);
    p.points[POINT_GOLD] = row.gold;
    p.points[POINT_ST] = i32::from(row.st);
    p.points[POINT_HT] = i32::from(row.ht);
    p.points[POINT_DX] = i32::from(row.dx);
    p.points[POINT_IQ] = i32::from(row.iq);
    p.points[POINT_ATT_SPEED] = 100; // parity char.cpp:2246
    p.points[POINT_MOV_SPEED] = 100; // parity char.cpp:2245
    p.points[POINT_CASTING_SPEED] = 100; // parity char.cpp:2248
    p.points[POINT_LEVEL_STEP] = i32::from(row.level_step);
    p.points[POINT_STAT] = i32::from(row.stat_point);
    p.points[POINT_SUB_SKILL] = i32::from(row.sub_skill_point);
    p.points[POINT_SKILL] = i32::from(row.skill_point);
    p.points[POINT_PLAYTIME] = row.playtime;
    // Battle points (ComputeBattlePoints — la sesión los cachea).
    p.points[POINT_ATT_GRADE] = battle.attack_grade;
    p.points[POINT_DEF_GRADE] = battle.def_grade;
    p.points[POINT_CLIENT_DEF_GRADE] = battle.client_def_grade;
    p.points[POINT_MAGIC_ATT_GRADE] = battle.magic_att_grade;
    p.points[POINT_MAGIC_DEF_GRADE] = battle.magic_def_grade;
    p.points[POINT_WEAPON_MIN] = battle.weapon_min;
    p.points[POINT_WEAPON_MAX] = battle.weapon_max;
    // Máximos del ComputePoints subset (char.cpp:2228-2232). Si el race es
    // inválido los puntos quedan en 0 (defensivo — el row viene del PG).
    if let Ok([max_hp, max_sp, max_stamina]) = compute_max_points(row) {
        p.points[POINT_MAX_HP] = max_hp;
        p.points[POINT_MAX_SP] = max_sp;
        p.points[POINT_MAX_STAMINA] = max_stamina;
    }
    p
}

/// `player.skill_level` (bytea, 255 × `TPlayerSkill` x86 6 B) ->
/// `TPacketGCSkillLevel` (1531 B, header 76).
///
/// Parity `char_skill.cpp:184-194` (SkillLevelPacket — el C++ copia el array
/// tal cual). El bytea del PG ES la serie cruda (verificado: 1530 B para los
/// personajes reales). Un bytea de tamaño distinto (None/corto — defensivo)
/// produce skills zeroed (el C++ con tabla vacía manda todo 0).
pub fn skill_level_packet(skill_level: Option<&Vec<u8>>) -> TPacketGCSkillLevel {
    let mut p = TPacketGCSkillLevel {
        header: TPacketGCSkillLevel::HEADER,
        skills: [TPlayerSkill { b_master_type: 0, b_level: 0, t_next_read: 0 }; 255],
    };
    if let Some(blob) = skill_level
        && blob.len() == 255 * TPlayerSkill::SIZE {
            for (i, s) in p.skills.iter_mut().enumerate() {
                let Ok(skill) = TPlayerSkill::from_bytes(&blob[i * 6..(i + 1) * 6]) else {
                    break;
                };
                *s = skill;
            }
        }
    p
}

/// `PlayerRow` -> `TPacketGCMainCharacter` (47 B, header **15** — layout del
/// CLIENTE, `Packet.h:1347-1350`; el C++ manda este struct cuando el mapa NO
/// tiene BGM configurado, `char.cpp:1536-1550`; con BGM manda 137/138 — GAP
/// documentado: el runtime actual no configura BGM por mapa).
///
/// Parity `char.cpp:1539-1549`: vid = row.id, wRaceNum = job (GetRaceNum),
/// name, lx/ly/lz = x/y/z UNITS, skill_group del row.
///
/// ⚠️ NO lleva empire y NO es 113: el 113 del cliente es la variante 48 B con
/// empire (`Packet.h:251,1376-1385`) — emitir 113 con 47 B desalinea el
/// stream 1 byte (ver doc de la struct en `protocol::world`).
pub fn main_character(row: &PlayerRow) -> TPacketGCMainCharacter {
    TPacketGCMainCharacter {
        header: TPacketGCMainCharacter::HEADER,
        dw_vid: row.id as u32,
        w_race_num: row.job as u32,
        sz_name: from_cstr(&row.name),
        lx: row.x,
        ly: row.y,
        lz: row.z,
        skill_group: row.skill_group as u8,
    }
}

/// `LandRow` -> `TLandPacketElement` (24 B — el wire trunca a DWORD/long;
/// los valores del runtime caben: ids 201..218, cells 4600..77000).
fn land_element(l: &LandRow) -> TLandPacketElement {
    TLandPacketElement {
        dw_id: l.id as u32,
        x: l.x as i32,
        y: l.y as i32,
        width: l.width as i32,
        height: l.height as i32,
        dw_guild_id: l.guild_id as u32,
    }
}

/// `Vec<LandRow>` -> paquete `TPacketGCLandList` (3 B + 24 B×N, header 130).
/// Parity `building.cpp:931-979` (SendLandList — el game manda SOLO los lands
/// del mapa del ch; con 0 lands el C++ no manda el paquete — el caller decide).
pub fn land_list(lands: &[LandRow]) -> Vec<u8> {
    let elements: Vec<TLandPacketElement> = lands.iter().map(land_element).collect();
    wire_land_list(&elements)
}

// ---------------------------------------------------------------------------
// F4 slice 3.3: quickslots / items / affects del entry (el diff completo)
// ---------------------------------------------------------------------------

/// `player.quickslot` (bytea, 36 × `TQuickslot` 2 B) -> los 36 paquetes
/// `GC_QUICKSLOT_ADD` (4 B c/u, header 28).
///
/// Parity `input_db.cpp:455-456` (el PlayerLoad hace `SetQuickslot` por slot —
/// y `SetQuickslot` manda el paquete SIEMPRE, `char_quickslot.cpp:96-103`).
/// Un bytea de tamaño raro (None/corto — defensivo) produce 36 slots vacíos
/// (el C++ con tabla memset manda los 36 con type 0 — el cliente los ignora).
pub fn quickslot_packets(quickslot: Option<&Vec<u8>>) -> Vec<Vec<u8>> {
    let slots: [TQuickslot; TPacketGCQuickSlotAdd::QUICKSLOT_MAX_NUM] = match quickslot {
        Some(blob) if blob.len() == 36 * TQuickslot::SIZE => {
            let mut out = [TQuickslot { slot_type: 0, pos: 0 }; 36];
            for (i, s) in out.iter_mut().enumerate() {
                if let Ok(q) = TQuickslot::from_bytes(&blob[i * 2..(i + 1) * 2]) {
                    *s = q;
                }
            }
            out
        }
        _ => [TQuickslot { slot_type: 0, pos: 0 }; 36],
    };
    slots
        .iter()
        .enumerate()
        .map(|(i, s)| TPacketGCQuickSlotAdd::new(i as u8, *s).to_bytes().to_vec())
        .collect()
}

/// `Vec<ItemRow>` (los 4 windows del load) -> los paquetes `GC_ITEM_SET`
/// (58 B c/u, header 21) en el orden del repo (id).
///
/// Parity `input_db.cpp:1453-1561` (ItemLoad → AddToCharacter → paquete por
/// item). El `window` TEXT del PG se convierte al índice del enum wire
/// (`GameType.h:175-186`): INVENTORY=1, EQUIPMENT=2, DRAGON_SOUL=5, BELT=6.
/// GAP del slice: `flags`/`anti_flags`/`highlight` a 0 (el C++ los lee del
/// item_proto — `item->GetFlags()`; el cliente no los exige para pintar el
/// slot) y `count` truncado a BYTE (parity del struct wire).
pub fn item_set_packets(items: &[ItemRow]) -> Vec<Vec<u8>> {
    items.iter().map(item_set_packet).collect()
}

fn item_set_packet(it: &ItemRow) -> Vec<u8> {
    let window = match it.window.as_str() {
        "INVENTORY" => TItemPos::WINDOW_INVENTORY,
        "EQUIPMENT" => TItemPos::WINDOW_EQUIPMENT,
        "DRAGON_SOUL_INVENTORY" => TItemPos::WINDOW_DRAGON_SOUL,
        "BELT_INVENTORY" => TItemPos::WINDOW_BELT,
        // Defensivo: un window fuera de los 4 del load no llega nunca (la
        // query los filtra); si llegara, se descarta el paquete.
        _ => return Vec::new(),
    };
    TPacketGCItemSet {
        header: TPacketGCItemSet::HEADER,
        cell: TItemPos { window, cell: it.pos as u16 },
        vnum: it.vnum as u32,
        count: it.count as u8,
        flags: 0,
        anti_flags: 0,
        highlight: 0,
        sockets: it.sockets,
        attrs: it.attrs,
    }
    .to_bytes()
    .to_vec()
}

/// `Vec<AffectRow>` -> los paquetes `GC_AFFECT_ADD` (22 B c/u, header 126).
///
/// Parity `input_db.cpp:1563-1583` (AffectLoad → LoadAffect → AddAffect →
/// paquete por affect). El `b_type` del row (i32) es el `dwType` del wire;
/// `dwFlag`/`lApplyValue`/`lDuration`/`lSPCost` directos.
pub fn affect_add_packets(affects: &[AffectRow]) -> Vec<Vec<u8>> {
    affects
        .iter()
        .map(|a| {
            TPacketGCAffectAdd::new(TPacketAffectElement {
                dw_type: a.b_type as u32,
                b_apply_on: a.b_apply_on as u8,
                l_apply_value: a.l_apply_value,
                dw_flag: a.dw_flag as u32,
                l_duration: a.l_duration,
                l_sp_cost: a.l_sp_cost,
            })
            .to_bytes()
            .to_vec()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::land::LandRow;
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
    /// `ClientManagerLogin.cpp:324-383`) — con la dirección del server de
    /// juego del DirectEnter (lAddr = inet_addr network order; wPort host).
    #[test]
    fn summary_to_simple_player_fields_and_size() {
        let ip = ip_to_inet_addr("172.25.104.175").expect("ip");
        assert_eq!(ip, 0xAF_68_19_AC, "inet_addr('172.25.104.175') — network byte order");
        let p = summary_to_simple_player(&summary(), ip, 30003);
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
        // lAddr@64 (4 B) + wPort@68 (2 B): el DirectEnter conecta a esta
        // dirección (PythonNetworkStream.cpp:458-469). Bytes EXACTOS: el
        // cliente decodifica el IP desde el byte bajo (NetStream.cpp:467-473)
        // y el puerto en host order (NetAddress.cpp:79-82 hace htons).
        assert_eq!(&b[64..68], &[172, 25, 104, 175], "lAddr = inet_addr network order (bytes LE)");
        assert_eq!(&b[68..70], &30003u16.to_le_bytes(), "wPort host order (30003 = 0x7533)");
        // Bytes spot en el wire (LE): dwID@0, name@4, byJob@29, x@56.
        assert_eq!(&b[0..4], &[2, 0, 0, 0]);
        assert_eq!(&b[4..10], b"ninja\0");
        assert_eq!(b[29], 1);
        assert_eq!(&b[56..60], &969600u32.to_le_bytes(), "x=969600 @56");
        assert_eq!(&b[60..64], &278400u32.to_le_bytes(), "y=278400 @60");
    }

    /// ip_to_inet_addr: validación + formato inet_addr (el valor que en
    /// memoria LE tiene los bytes [a, b, c, d]).
    #[test]
    fn ip_to_inet_addr_format_and_errors() {
        assert_eq!(ip_to_inet_addr("127.0.0.1").unwrap(), 0x0100_007F, "memoria LE: [127, 0, 0, 1]");
        assert_eq!(ip_to_inet_addr("172.25.104.175").unwrap(), 0xAF68_19AC);
        assert_eq!(ip_to_inet_addr("0.0.0.0").unwrap(), 0);
        assert!(ip_to_inet_addr("172.25.104").is_err(), "3 octetos");
        assert!(ip_to_inet_addr("a.b.c.d").is_err());
        assert!(ip_to_inet_addr("300.1.1.1").is_err(), "octeto > 255");
    }

    /// login_success: 449 B, slots None -> zeroed, handle/random_key en los
    /// offsets del spec (desc.cpp:955-988) + lAddr/wPort del server real.
    #[test]
    fn login_success_size_slots_and_tail() {
        let mut slots: [Option<PlayerSummary>; 5] = [None, None, None, None, None];
        slots[0] = Some(summary());
        let ip = ip_to_inet_addr("172.25.104.175").unwrap();
        let p = login_success(&slots, 0xDEAD_BEEF, 0xCAFE_BABE, ip, 30003);
        let b = p.to_bytes();
        assert_eq!(b.len(), TPacketGCLoginSuccess::SIZE, "449 B packed");
        assert_eq!(b[0], TPacketGCLoginSuccess::HEADER, "header 0x20");
        // Slot 0 con datos; slots 1..4 zeroed (dwID=0).
        assert_eq!(p.players[0].dw_id, 2);
        for i in 1..5 {
            assert_eq!(p.players[i].dw_id, 0, "slot {i} vacio -> zeroed");
        }
        // lAddr/wPort del slot 0 (offsets del TSimplePlayer: 64/68) — el
        // DirectEnter del cliente conecta a esta dirección.
        assert_eq!(&b[1 + 64..1 + 68], &[172, 25, 104, 175], "lAddr slot 0");
        assert_eq!(&b[1 + 68..1 + 70], &30003u16.to_le_bytes(), "wPort slot 0");
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
        assert_eq!((p.b_moving_speed, p.b_attack_speed, p.b_state_flag), (100, 100, 0), "parity char.cpp:2245-2246; state flag runtime GAP");
        assert_eq!(p.dw_affect_flag, [0, 0], "runtime GAP");
        // wRaceNum@22 (LE) en el wire.
        assert_eq!(&b[22..26], &[1, 0, 0, 0]);
    }

    /// character_additional_info: 70 B y campos spot (parity char.cpp:924-948).
    /// Sin items equipados: `PART_MAIN` = 0 (se setea al equipar — item.cpp:833;
    /// el `part_base` del row es la apariencia base, bBasePart char.cpp:1709);
    /// `PART_HAIR` = part_hair persistido (char.cpp:1710).
    #[test]
    fn character_additional_info_fields_and_size() {
        let p = character_additional_info(&row(), 3);
        let b = p.to_bytes();
        assert_eq!(b.len(), TPacketGCCharacterAdditionalInfo::SIZE, "70 B");
        assert_eq!(b[0], TPacketGCCharacterAdditionalInfo::HEADER, "header 136");
        assert_eq!(p.dw_vid, 2);
        assert_eq!(p.name(), "ninja");
        assert_eq!(p.aw_part[EQUIPPART_ARMOR], 0, "PART_MAIN = 0 sin items (se setea al equipar)");
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

    /// `equipped_parts` (F5.3 ComputeParts): el part de un item = su VNUM;
    /// el slot del equip se deduce del cell wire (`EQUIP_CELL_BASE + wear`,
    /// length.h:827). WEAR_BODY=0→ARMOR, WEAR_HEAD=1→HEAD, WEAR_WEAPON=4→
    /// WEAPON (length.h:101-111); HAIR del row persistido; ACCE=0 (GAP).
    #[test]
    fn equipped_parts_from_inventory() {
        let r = row();
        // Sin equipo: solo HAIR del row (part_hair persistido), resto 0.
        let p = equipped_parts(&r, &[]);
        assert_eq!(p[EQUIPPART_HAIR], r.part_hair as u32, "HAIR del row");
        assert_eq!(p[EQUIPPART_ARMOR], 0);
        assert_eq!(p[EQUIPPART_WEAPON], 0);
        assert_eq!(p[EQUIPPART_HEAD], 0);
        assert_eq!(p[EQUIPPART_ACCE], 0);

        // Items EQUIPMENT: BODY (cell 180) → ARMOR, HEAD (181) → HEAD,
        // WEAPON (184) → WEAPON — el part = vnum del item (parity
        // item.cpp:793,833 `SetPart(PART_WEAPON/MAIN, GetVnum())`).
        let items = vec![
            ItemRow {
                id: 10,
                window: "EQUIPMENT".into(),
                pos: EQUIP_CELL_BASE as i32 + 0, // WEAR_BODY
                count: 1,
                vnum: 101_001, // armadura
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            },
            ItemRow {
                id: 11,
                window: "EQUIPMENT".into(),
                pos: EQUIP_CELL_BASE as i32 + 1, // WEAR_HEAD
                count: 1,
                vnum: 102_002, // casco
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            },
            ItemRow {
                id: 12,
                window: "EQUIPMENT".into(),
                pos: EQUIP_CELL_BASE as i32 + 4, // WEAR_WEAPON
                count: 1,
                vnum: 103_003, // espada
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            },
            // Un item del inventario normal NO alimenta los parts.
            ItemRow {
                id: 13,
                window: "INVENTORY".into(),
                pos: 5,
                count: 1,
                vnum: 104_004,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            },
        ];
        let p = equipped_parts(&r, &items);
        assert_eq!(p[EQUIPPART_ARMOR], 101_001, "vnum del BODY");
        assert_eq!(p[EQUIPPART_HEAD], 102_002, "vnum del HEAD");
        assert_eq!(p[EQUIPPART_WEAPON], 103_003, "vnum del WEAPON");
        assert_eq!(p[EQUIPPART_HAIR], r.part_hair as u32, "HAIR intacto");
        assert_eq!(p[EQUIPPART_ACCE], 0, "GAP documentado");
    }

    /// `find_equip_cell` (F5.3 parity item.cpp:509-623): el slot del equip
    /// según los bits WEARABLE_* del wearflag; el orden de los else-if del
    /// C++ (item.cpp:568-592) decide cuando hay varios bits.
    #[test]
    fn find_equip_cell_matches_cpp_order() {
        use database::item::ProtoItem;
        let p = |wear_flag: u32| ProtoItem {
            b_type: 1,
            b_sub_type: 0,
            values: [0; 6],
            wear_flag: i64::from(wear_flag),
            weight: 0,
        };
        // Bits individuales -> slots (length.h:99-119).
        assert_eq!(find_equip_cell(&p(wearable::BODY)), Some(0), "WEAR_BODY");
        assert_eq!(find_equip_cell(&p(wearable::HEAD)), Some(1), "WEAR_HEAD");
        assert_eq!(find_equip_cell(&p(wearable::FOOTS)), Some(2), "WEAR_FOOTS");
        assert_eq!(find_equip_cell(&p(wearable::WRIST)), Some(3), "WEAR_WRIST");
        assert_eq!(find_equip_cell(&p(wearable::WEAPON)), Some(4), "WEAR_WEAPON");
        assert_eq!(find_equip_cell(&p(wearable::SHIELD)), Some(10), "WEAR_SHIELD");
        assert_eq!(find_equip_cell(&p(wearable::NECK)), Some(5), "WEAR_NECK");
        assert_eq!(find_equip_cell(&p(wearable::EAR)), Some(6), "WEAR_EAR");
        assert_eq!(find_equip_cell(&p(wearable::ARROW)), Some(9), "WEAR_ARROW");
        assert_eq!(find_equip_cell(&p(wearable::UNIQUE)), Some(7), "WEAR_UNIQUE1");
        assert_eq!(find_equip_cell(&p(wearable::ABILITY)), Some(11), "WEAR_ABILITY1");
        // Sin wearflag -> None (item.cpp:511-519 — no equipable).
        assert_eq!(find_equip_cell(&p(0)), None);
        // Solo HAIR/PENDANT/GLOVE -> None (GAP documentado — el C++ los
        // gestiona por otros paths).
        assert_eq!(find_equip_cell(&p(wearable::HAIR)), None);
        // Varios bits: gana el PRIMERO del orden del C++ (item.cpp:568-592).
        assert_eq!(find_equip_cell(&p(wearable::WEAPON | wearable::BODY)), Some(0), "BODY antes que WEAPON");
        assert_eq!(find_equip_cell(&p(wearable::SHIELD | wearable::NECK)), Some(10), "SHIELD antes que NECK");
    }

    /// Los 5 slots con datos -> 5×71 B ocupados (tamaño total 449).
    #[test]
    fn login_success_full_slots_size() {
        let s = summary();
        let (a, b, c) = (summary(), summary(), summary());
        let slots = [Some(s.clone()), Some(s), Some(a), Some(b), Some(c)];
        let p = login_success(&slots, 1, 2, 0, 0);
        assert_eq!(p.to_bytes().len(), 449);
        assert!(p.players.iter().all(|pl| pl.dw_id == 2));
    }

    // ---------------------------------------------------------- slice 3: entry

    /// points_packet: 1021 B, los puntos del row + los MÁXIMOS del subset
    /// ComputePoints + los BattlePoints (parity char.cpp:1553-1581 +
    /// 2228-2248 + 2051-2152).
    #[test]
    fn points_packet_fields_and_size() {
        let p = points_packet(&row(), 300, &BattlePoints::default());
        let b = p.to_bytes();
        assert_eq!(b.len(), TPacketGCPoints::SIZE, "1021 B");
        assert_eq!(b[0], TPacketGCPoints::HEADER, "header 16");
        assert_eq!(p.points[POINT_LEVEL], 5, "level del row");
        assert_eq!(p.points[POINT_HP], 100);
        assert_eq!(p.points[POINT_SP], 100, "mp -> POINT_SP");
        assert_eq!(p.points[POINT_STAMINA], 100);
        assert_eq!(p.points[POINT_GOLD], 0);
        assert_eq!((p.points[POINT_ST], p.points[POINT_HT], p.points[POINT_DX], p.points[POINT_IQ]), (30, 30, 30, 30));
        assert_eq!(p.points[POINT_EXP], 0);
        assert_eq!(p.points[POINT_NEXT_EXP], 300, "exp_table[level] del caller");
        assert_eq!(p.points[POINT_PLAYTIME], 0);
        assert_eq!((p.points[POINT_MOV_SPEED], p.points[POINT_ATT_SPEED], p.points[POINT_CASTING_SPEED]), (100, 100, 100), "parity char.cpp:2245-2248");
        // El dummy row es job=1 (ASSASSIN_W -> ASSASSIN): ht=30, iq=30,
        // random_hp/sp=0 -> 650+1200=1850 / 200+600=800 / 800+150=950.
        assert_eq!(p.points[POINT_MAX_HP], 1850, "650 + 30×40");
        assert_eq!(p.points[POINT_MAX_SP], 800, "200 + 30×20");
        assert_eq!(p.points[POINT_MAX_STAMINA], 950, "800 + 30×5");
        // Wire spot: level@5 (1 + 4×1), hp@21 (1 + 4×5), max_hp@25.
        assert_eq!(&b[5..9], &5i32.to_le_bytes());
        assert_eq!(&b[21..25], &100i32.to_le_bytes());
        assert_eq!(&b[25..29], &1850i32.to_le_bytes());
    }

    /// `compute_battle_points` — parity `ComputeBattlePoints` (char.cpp:2051-
    /// 2152, PC sin montura/bonos): ataque = level×2 + stat del job
    /// (ASSASSIN `(4×ST+2×DX)/3`); defensa = level + (int)(HT/1.25) + armor;
    /// client_def (el show de la ventana) = (level+HT+armor) − def; mágicos;
    /// la ventana del cliente (WEAPON_MIN/MAX) = el daño del arma.
    #[test]
    fn compute_battle_points_parity() {
        let mut r = row();
        r.job = 5; // ASSASSIN_M -> race_to_job 1 (ASSASSIN)
        r.level = 5;
        r.st = 30;
        r.dx = 30;
        r.ht = 30;
        r.iq = 30;
        let weapon = database::item::ProtoItem {
            b_type: 1,
            b_sub_type: 0,
            // El daño del arma vive en value3/value4 (GetValue(3)/(4) —
            // battle.cpp:460-461; el cliente lee los mismos en
            // `__SetWeaponPower`, PythonNetworkStreamPhaseGameActor.cpp:50-51).
            values: [0, 0, 0, 12, 15, 0],
            wear_flag: 16,
            weight: 0,
        };
        let b = compute_battle_points(&r, Some(&weapon), 25);
        // Ataque: 5×2 + (4×30+2×30)/3 = 10 + 60 (char.cpp:2061-2092).
        assert_eq!(b.attack_grade, 70, "level×2 + stat del job");
        // Defensa: 5 + (int)(30/1.25) + 25 = 5 + 24 + 25 (char.cpp:2113-2146).
        assert_eq!(b.def_grade, 54, "level + HT/1.25 + armor");
        // Show def (la ventana): (5 + 30 + 25) − 54 (char.cpp:2146-2147).
        assert_eq!(b.client_def_grade, 6, "show def de la ventana");
        // Mágicos (char.cpp:2150-2151).
        assert_eq!(b.magic_att_grade, 70, "level×2 + IQ×2");
        assert_eq!(b.magic_def_grade, 57, "level + (IQ×3+HT)/3 + armor/2");
        // La ventana del cliente: el daño del arma (value3/value4).
        assert_eq!((b.weapon_min, b.weapon_max), (12, 15));
        // Sin arma: 0/0 (manos vacías).
        let bare = compute_battle_points(&r, None, 0);
        assert_eq!((bare.weapon_min, bare.weapon_max), (0, 0));
        // El GUERRERO (job 0): stat del job = 2×ST.
        r.job = 0;
        let w = compute_battle_points(&r, None, 0);
        assert_eq!(w.attack_grade, 5 * 2 + 2 * 30, "WARRIOR 2×ST");
    }

    /// `character_update_with_parts` — el paquete que el C++ manda al
    /// equipar/desequipar (`UpdatePacket` char.cpp:1017-1052 — el cliente
    /// recalcula ATT_MIN/ATT_MAX y refresca la ventana): 51 B, parts del
    /// equipo, speeds 100 y los campos del row (vid/alignment).
    #[test]
    fn character_update_with_parts_fields_and_size() {
        let mut r = row();
        r.alignment = 1234;
        let parts = [0x1001, 0x1002, 0x1003, 0x1004, 0x1005];
        let p = character_update_with_parts(&r, &parts, 42);
        let b = p.to_bytes();
        assert_eq!(b.len(), TPacketGCCharacterUpdate::SIZE, "51 B");
        assert_eq!(b[0], TPacketGCCharacterUpdate::HEADER, "header 19");
        assert_eq!(p.dw_vid, r.id as u32);
        assert_eq!(p.aw_part, parts, "parts del equipo (el arma en WEAPON)");
        assert_eq!((p.b_moving_speed, p.b_attack_speed), (100, 100), "GetLimitPoint — char.cpp:1025-1026");
        assert_eq!(p.b_state_flag, 0);
        assert_eq!(p.dw_affect_flag, [0, 0], "sin affects (F5)");
        assert_eq!(p.dw_guild_id, 0);
        assert_eq!(p.s_alignment, 123, "row.alignment / 10 — char.cpp:1034");
        assert_eq!(p.b_pk_mode, 0);
        assert_eq!(p.dw_mount_vnum, 0);
        assert_eq!(p.dw_arrow, 42, "flechas equipadas (QUIVER)");
        // Wire spot: vid@1, weapon part@9 (1+4+4), arrow@47.
        assert_eq!(&b[1..5], &(r.id as u32).to_le_bytes());
        assert_eq!(&b[9..13], &0x1002u32.to_le_bytes());
        assert_eq!(&b[47..51], &42u32.to_le_bytes());
    }

    /// ComputePoints subset — vectores REALES del runtime (4 personajes):
    /// el hp/mp/stamina almacenados en el row SON los máximos que el C++
    /// calculó en el último login (parity empírica de char.cpp:2230-2232).
    #[test]
    fn compute_max_points_real_vectors() {
        // lkjsnlfknlsk: job=5 (ASSASSIN_M -> ASSASSIN), lvl 1, ht=3, iq=3.
        let mut a = row();
        a.job = 5;
        a.ht = 3;
        a.iq = 3;
        a.random_hp = 0;
        a.random_sp = 0;
        assert_eq!(race_to_job(5).unwrap(), 1, "RaceToJob: ASSASSIN_M -> JOB_ASSASSIN");
        assert_eq!(compute_max_points(&a).unwrap(), [770, 260, 815], "= hp/mp/stamina reales del row (650+3×40 / 200+3×20 / 800+3×5)");
        // ninja: job=1 (ASSASSIN_W), lvl 12, ht=8, iq=3, random_hp=430, random_sp=214.
        let mut n = row();
        n.job = 1;
        n.ht = 8;
        n.iq = 3;
        n.random_hp = 430;
        n.random_sp = 214;
        assert_eq!(compute_max_points(&n).unwrap(), [1400, 474, 840], "650+430+8×40 / 200+214+3×20 / 800+8×5");
        // Chaman: job=7 (SHAMAN_M -> SHAMAN), ht=4, iq=6.
        let mut c = row();
        c.job = 7;
        c.ht = 4;
        c.iq = 6;
        assert_eq!(compute_max_points(&c).unwrap(), [860, 320, 820], "700+4×40 / 200+6×20 / 800+4×5");
        // hol: job=6 (SURA_W -> SURA), ht=3, iq=5.
        let mut h = row();
        h.job = 6;
        h.ht = 3;
        h.iq = 5;
        assert_eq!(compute_max_points(&h).unwrap(), [770, 300, 815], "650+3×40 / 200+5×20 / 800+3×5");
        // Race fuera del subset -> Err (defensivo).
        assert!(race_to_job(9).is_err());
        assert!(compute_max_points(&{ let mut r = row(); r.job = 9; r }).is_err());
    }

    /// skill_level_packet: el bytea 1530 B -> 255 skills (bMasterType/bLevel/
    /// tNextRead); None o tamaño raro -> zeroed (defensivo).
    #[test]
    fn skill_level_packet_parses_bytea() {
        let mut blob = vec![0u8; 255 * 6];
        blob[0] = 1; // skill 0: master type
        blob[1] = 20; // skill 0: level
        blob[2..6].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        blob[254 * 6] = 2; // skill 254
        blob[254 * 6 + 1] = 40;
        let p = skill_level_packet(Some(&blob));
        let b = p.to_bytes();
        assert_eq!(b.len(), TPacketGCSkillLevel::SIZE, "1531 B");
        assert_eq!(b[0], TPacketGCSkillLevel::HEADER, "header 76");
        assert_eq!(p.skills[0].b_master_type, 1);
        assert_eq!(p.skills[0].b_level, 20);
        assert_eq!(p.skills[0].t_next_read, 0xDEAD_BEEF);
        assert_eq!(p.skills[254].b_level, 40);
        // Wire spot: skill 0 @ [1..7].
        assert_eq!(&b[1..7], &blob[0..6]);
        // None y bytea corto -> zeroed.
        let p = skill_level_packet(None);
        assert!(p.skills.iter().all(|s| *s == TPlayerSkill { b_master_type: 0, b_level: 0, t_next_read: 0 }));
        let p = skill_level_packet(Some(&vec![1, 2, 3]));
        assert_eq!(p.skills[0].b_level, 0, "bytea corto -> zeroed (defensivo)");
    }

    /// main_character: 47 B (layout del CLIENTE — sin empire), campos spot
    /// (parity char.cpp:1539-1549 + Packet.h:1349-1357).
    #[test]
    fn main_character_fields_and_size() {
        let p = main_character(&row());
        let b = p.to_bytes();
        assert_eq!(b.len(), TPacketGCMainCharacter::SIZE, "47 B (layout del cliente)");
        assert_eq!(b[0], TPacketGCMainCharacter::HEADER, "header 15 (MAIN_CHARACTER sin BGM — Packet.h:160)");
        assert_eq!(p.dw_vid, 2);
        assert_eq!(p.w_race_num, 1, "GetRaceNum() = job");
        assert_eq!(p.name(), "ninja");
        assert_eq!((p.lx, p.ly, p.lz), (969600, 278400, 0), "UNITS");
        assert_eq!(p.skill_group, 3);
        // Wire: lx@34; skill_group@46 (el empire del server NO existe en el
        // cliente — el byte 46 del wire ES el skill_group).
        assert_eq!(&b[34..38], &969600i32.to_le_bytes());
        assert_eq!(b[46], 3, "skill_group@46");
    }

    /// land_list: 18 lands del mapa 41 -> 435 B (3 + 18×24 — parity log del
    /// core "elem_size: 432"); 0 lands -> 3 B (el C++ no manda el paquete).
    #[test]
    fn land_list_map_41_size() {
        let lands: Vec<LandRow> = (201..=218)
            .map(|id| LandRow {
                id,
                map_index: 41,
                x: 66100 + (id - 201) * 100,
                y: 9400,
                width: 3000,
                height: 3000,
                guild_id: 0,
            })
            .collect();
        let bytes = land_list(&lands);
        assert_eq!(bytes.len(), 435, "3 + 18×24 (parity log del core)");
        assert_eq!(bytes[0], 130, "header GC_LAND_LIST");
        assert_eq!(u16::from_le_bytes([bytes[1], bytes[2]]), 435, "size WORD");
        // Elemento 0: dwID@3 = 201, x@7 = 66100 (cells crudas).
        assert_eq!(u32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]), 201);
        assert_eq!(i32::from_le_bytes([bytes[7], bytes[8], bytes[9], bytes[10]]), 66100);
        assert_eq!(u32::from_le_bytes([bytes[27], bytes[28], bytes[29], bytes[30]]), 202, "2º dwID @27");
        // Vacío -> solo el header (el caller decide no mandarlo).
        assert_eq!(land_list(&[]), [130, 3, 0]);
    }

    // ------------------------------------------------------ slice 3.3: cola

    /// quickslot_packets: el bytea 72 B -> 36 paquetes de 4 B en orden
    /// (parity input_db.cpp:455-456 + char_quickslot.cpp:96-103).
    #[test]
    fn quickslot_packets_36_slots() {
        let mut blob = vec![0u8; 36 * 2];
        blob[0] = 0; // slot 0: type ITEM
        blob[1] = 5; // slot 0: pos 5
        blob[2] = 1; // slot 1: type SKILL
        blob[3] = 12;
        let pkts = quickslot_packets(Some(&blob));
        assert_eq!(pkts.len(), 36, "QUICKSLOT_MAX_NUM");
        assert_eq!(pkts[0], [28, 0, 0, 5], "slot 0: header+pos+type+pos");
        assert_eq!(pkts[1], [28, 1, 1, 12], "slot 1");
        assert_eq!(pkts[35], [28, 35, 0, 0], "slot 35 vacío");
        assert!(pkts.iter().all(|p| p.len() == 4));
        // None / bytea corto -> 36 vacíos (defensivo).
        let pkts = quickslot_packets(None);
        assert_eq!(pkts.len(), 36);
        assert!(pkts.iter().all(|p| p[2] == 0 && p[3] == 0));
    }

    /// item_set_packets: 58 B por item, window -> índice wire (GameType.h),
    /// sockets/attrs directos del ItemRow.
    #[test]
    fn item_set_packets_fields_and_windows() {
        use database::item::ItemRow;
        let items = vec![
            ItemRow {
                id: 100,
                window: "INVENTORY".into(),
                pos: 3,
                count: 1,
                vnum: 27001,
                sockets: [0xDEAD_BEEF, 0, 0],
                attrs: [(1, 100), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
            },
            ItemRow {
                id: 101,
                window: "EQUIPMENT".into(),
                pos: 1,
                count: 1,
                vnum: 19001,
                sockets: [0, 0, 0],
                attrs: [(0, 0); 7],
            },
        ];
        let pkts = item_set_packets(&items);
        assert_eq!(pkts.len(), 2);
        assert_eq!(pkts[0].len(), 51, "TPacketGCItemSet::SIZE (packed)");
        assert_eq!(pkts[0][0], 21, "header GC_ITEM_SET");
        assert_eq!(&pkts[0][1..4], &[1, 3, 0], "window INVENTORY=1, cell=3");
        assert_eq!(&pkts[0][4..8], &27001u32.to_le_bytes(), "vnum");
        assert_eq!(pkts[0][8], 1, "count BYTE");
        assert_eq!(&pkts[0][18..26], &0xDEAD_BEEFi64.to_le_bytes(), "socket0");
        assert_eq!(&pkts[1][1..4], &[2, 1, 0], "window EQUIPMENT=2, cell=1");
        // Window fuera del load -> paquete vacío (defensivo).
        let bad = ItemRow {
            id: 102,
            window: "SAFEBOX".into(),
            pos: 0,
            count: 1,
            vnum: 1,
            sockets: [0, 0, 0],
            attrs: [(0, 0); 7],
        };
        assert!(item_set_packets(&[bad]).iter().all(|p| p.is_empty()), "SAFEBOX fuera del load");
    }

    /// affect_add_packets: 22 B por affect con el mapeo del row (b_type ->
    /// dwType; parity input_db.cpp:1563-1583 + tables.h:808-816).
    #[test]
    fn affect_add_packets_mapping() {
        use database::affect::AffectRow;
        let affects = vec![AffectRow {
            dw_pid: 1,
            b_type: 5,
            b_apply_on: 2,
            l_apply_value: 100,
            dw_flag: 0xDEAD_BEEF,
            l_duration: 60,
            l_sp_cost: 0,
        }];
        let pkts = affect_add_packets(&affects);
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0].len(), 22, "TPacketGCAffectAdd::SIZE");
        assert_eq!(pkts[0][0], TPacketGCAffectAdd::HEADER, "header GC_AFFECT_ADD");
        assert_eq!(&pkts[0][1..5], &5u32.to_le_bytes(), "dwType = b_type");
        assert_eq!(pkts[0][5], 2, "bApplyOn");
        assert_eq!(&pkts[0][6..10], &100i32.to_le_bytes(), "lApplyValue");
        assert_eq!(&pkts[0][10..14], &0xDEAD_BEEFu32.to_le_bytes(), "dwFlag");
        assert_eq!(&pkts[0][14..18], &60i32.to_le_bytes(), "lDuration");
        assert_eq!(pkts[0][18..22], [0, 0, 0, 0], "lSPCost");
    }
}
