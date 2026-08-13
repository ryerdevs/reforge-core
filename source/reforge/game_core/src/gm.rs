//! F4 slice GM (2026-08-13): el dominio PURO de los comandos de GM — la
//! tabla de comandos, el parseo del chat y el gate de permisos. Sin I/O:
//! la lectura del `common.gmlist` (PG) vive en `database::common`
//! (`CommonRepo::gm_authority`) y el dispatch wire en `channel/gm.rs`.
//!
//! # Parity legacy (verificado 2026-08-13)
//!
//! - Entrada: el cliente manda `CG_CHAT` con '/' al inicio y el server llama
//!   `interpret_command(ch, buf+1, ...)` (input_main.cpp:661-665) — ANTES del
//!   echo y del anti-spam (el comando no se muestra).
//! - Tabla: `cmd_info[]` (cmd.cpp:276+) — cada entrada tiene el nivel GM
//!   mínimo (columna `gm_level`).
//! - Permisos: `gm_get_level(name, host, account)` (gm.cpp:50-105) lee el
//!   `common.gmlist` cargado en el boot (`__GetAdminInfo`,
//!   ClientManager.cpp:3476-3526: mName = clave, mAccount DEBE coincidir,
//!   mServerIP 'ALL' o la IP del canal; el texto mAuthority se mapea a
//!   EGMLevels). Gate: `cmd_info.gm_level > GetGMLevel() || GM_DISABLE` →
//!   rechazo con "그런 명령어가 없습니다" (cmd.cpp:710-714).
//! - Niveles (length.h:300-309): GM_PLAYER=0, GM_LOW_WIZARD=1, GM_WIZARD=2,
//!   GM_HIGH_WIZARD=3, GM_GOD=4, GM_IMPLEMENTOR=5, GM_DISABLE=6.
//! - `gPlayerMaxLevel = 99` (config.cpp:123); `g_bItemCountLimit = 200`
//!   (config.cpp:39) — los clamps del item/level.

/// `EGMLevels` (length.h:300-309): el nivel GM de `common.gmlist`.
pub mod gm_level {
    pub const PLAYER: i16 = 0;
    pub const LOW_WIZARD: i16 = 1;
    pub const WIZARD: i16 = 2;
    pub const HIGH_WIZARD: i16 = 3;
    pub const GOD: i16 = 4;
    pub const IMPLEMENTOR: i16 = 5;
    pub const DISABLE: i16 = 6;
}

/// `g_bItemCountLimit = 200` (config.cpp:39) — clamp del count de `/item`.
pub const ITEM_COUNT_LIMIT: u32 = 200;

/// `gPlayerMaxLevel = 99` (config.cpp:123) — clamp del `/level`.
pub const PLAYER_MAX_LEVEL: i32 = 99;

/// Comandos de GM implementados (subset ponytail del `cmd_info[]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GmCommand {
    /// `warp <x metros> <y metros>` → `x*100, y*100` unidades (parity
    /// do_warp, cmd_gm.cpp:319-387: `WarpSet(x*100, y*100)` → GC_WARP, el
    /// cliente RECONECTA con el DirectEnter). La forma `warp <nombre>` (warp
    /// a OTRO jugador) queda fuera: necesita el registro nombre→vid.
    Warp { x: i32, y: i32 },
    /// `item <vnum> [count]` → item nuevo en el primer slot libre del
    /// inventario (parity do_item, cmd_gm.cpp:398-448: CreateItem + count
    /// MINMAX(1, count, g_bItemCountLimit)).
    GiveItem { vnum: u32, count: u32 },
    /// `notice <texto>` → GC_CHAT tipo CHAT_TYPE_NOTICE (parity do_notice,
    /// cmd_gm.cpp:1354+ → BroadcastNotice). Subset: se manda al GM (el
    /// broadcast a TODOS los jugadores necesita el task del canal — GAP
    /// documentado en channel/gm.rs).
    Notice { text: String },
    /// `level <nivel>` → nivel del personaje con clamp 1..99 (parity
    /// do_level, cmd_gm.cpp:2423-2441: ResetPoint(MINMAX(1, level,
    /// gPlayerMaxLevel))). El recálculo de stat/skill points del ResetPoint
    /// queda fuera (GAP documentado).
    SetLevel { level: i32 },
}

/// Parseo del texto tras el '/': `parse_command("warp 100 200")` →
/// `Warp { x: 100, y: 200 }`. `None` = comando desconocido o argumentos
/// inválidos (parity: el C++ compara el primer token contra `cmd_info[]` y
/// responde "그런 명령어가 없습니다" — el mensaje lo manda el canal).
pub fn parse_command(cmd: &str) -> Option<GmCommand> {
    let cmd = cmd.trim();
    let mut it = cmd.split_whitespace();
    let name = it.next()?;
    match name {
        "warp" => {
            let x: i32 = it.next()?.parse().ok()?;
            let y: i32 = it.next()?.parse().ok()?;
            Some(GmCommand::Warp { x, y })
        }
        "item" => {
            let vnum: u32 = it.next()?.parse().ok()?;
            let count = it
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1)
                .clamp(1, ITEM_COUNT_LIMIT);
            Some(GmCommand::GiveItem { vnum, count })
        }
        "notice" => {
            let text = cmd[name.len()..].trim();
            if text.is_empty() {
                return None;
            }
            Some(GmCommand::Notice { text: text.to_string() })
        }
        "level" => {
            let level: i32 = it.next()?.parse().ok()?;
            Some(GmCommand::SetLevel { level: level.clamp(1, PLAYER_MAX_LEVEL) })
        }
        _ => None,
    }
}

/// El nivel GM mínimo del comando (columna `gm_level` del `cmd_info[]` —
/// cmd.cpp:281 warp LOW_WIZARD, 283 notice HIGH_WIZARD, 297 level
/// LOW_WIZARD, 301 item GOD).
pub fn required_level(cmd: &GmCommand) -> i16 {
    match cmd {
        GmCommand::Warp { .. } | GmCommand::SetLevel { .. } => gm_level::LOW_WIZARD,
        GmCommand::GiveItem { .. } => gm_level::GOD,
        GmCommand::Notice { .. } => gm_level::HIGH_WIZARD,
    }
}

/// Gate de permisos (parity cmd.cpp:710-714): `gm_level && (gm_level >
/// GetGMLevel() || gm_level == GM_DISABLE)` → rechazo. El nivel del GM viene
/// de `common.gmlist` (mapeado por `gm_level_from_text`).
pub fn is_allowed(player_level: i16, required: i16) -> bool {
    required != gm_level::DISABLE && player_level >= required
}

/// El mapeo del texto `mAuthority` → EGMLevels (parity
/// ClientManager.cpp:3506-3517 — IMPLEMENTOR/GOD/HIGH_WIZARD/LOW_WIZARD/
/// WIZARD; cualquier otro valor se OMITE en el boot del C++). Case-insensitive
/// (el texto PG viene del enum de MariaDB, guardado en mayúsculas).
pub fn gm_level_from_text(auth: &str) -> Option<i16> {
    match auth.trim().to_ascii_uppercase().as_str() {
        "IMPLEMENTOR" => Some(gm_level::IMPLEMENTOR),
        "GOD" => Some(gm_level::GOD),
        "HIGH_WIZARD" => Some(gm_level::HIGH_WIZARD),
        "LOW_WIZARD" => Some(gm_level::LOW_WIZARD),
        "WIZARD" => Some(gm_level::WIZARD),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_warp_two_meters() {
        assert_eq!(
            parse_command("warp 100 200"),
            Some(GmCommand::Warp { x: 100, y: 200 })
        );
        assert_eq!(
            parse_command("  warp  -5  42  "),
            Some(GmCommand::Warp { x: -5, y: 42 }),
            "trim + negativos (parity str_to_number)"
        );
    }

    #[test]
    fn parse_warp_bad_args_none() {
        assert_eq!(parse_command("warp"), None, "sin argumentos");
        assert_eq!(parse_command("warp 100"), None, "solo x");
        assert_eq!(parse_command("warp a b"), None, "no numérico");
        assert_eq!(parse_command("warp 100 b"), None, "y no numérico");
    }

    #[test]
    fn parse_item_count_default_and_clamp() {
        assert_eq!(parse_command("item 500"), Some(GmCommand::GiveItem { vnum: 500, count: 1 }));
        assert_eq!(
            parse_command("item 500 5"),
            Some(GmCommand::GiveItem { vnum: 500, count: 5 })
        );
        // MINMAX(1, count, g_bItemCountLimit) — cmd_gm.cpp:414.
        assert_eq!(parse_command("item 500 0"), Some(GmCommand::GiveItem { vnum: 500, count: 1 }));
        assert_eq!(
            parse_command("item 500 999"),
            Some(GmCommand::GiveItem { vnum: 500, count: ITEM_COUNT_LIMIT })
        );
        assert_eq!(parse_command("item"), None, "sin vnum");
        assert_eq!(parse_command("item abc"), None, "vnum no numérico");
    }

    #[test]
    fn parse_notice_keeps_full_text() {
        assert_eq!(
            parse_command("notice hola mundo 123"),
            Some(GmCommand::Notice { text: "hola mundo 123".to_string() })
        );
        assert_eq!(parse_command("notice"), None, "sin texto");
        assert_eq!(parse_command("notice   "), None, "solo espacios");
    }

    #[test]
    fn parse_level_clamp() {
        assert_eq!(parse_command("level 5"), Some(GmCommand::SetLevel { level: 5 }));
        // MINMAX(1, level, gPlayerMaxLevel) — cmd_gm.cpp:2437.
        assert_eq!(parse_command("level 0"), Some(GmCommand::SetLevel { level: 1 }));
        assert_eq!(
            parse_command("level 500"),
            Some(GmCommand::SetLevel { level: PLAYER_MAX_LEVEL })
        );
        assert_eq!(parse_command("level"), None);
    }

    #[test]
    fn parse_unknown_and_empty_none() {
        assert_eq!(parse_command(""), None);
        assert_eq!(parse_command("   "), None);
        assert_eq!(parse_command("teleport 1 2"), None, "comando desconocido");
        assert_eq!(parse_command("mob 101"), None, "mob: fuera del subset (GAP)");
        assert_eq!(parse_command("kill alguien"), None, "kill: fuera del subset (GAP)");
    }

    #[test]
    fn required_levels_parity_cmd_table() {
        assert_eq!(required_level(&GmCommand::Warp { x: 0, y: 0 }), gm_level::LOW_WIZARD);
        assert_eq!(required_level(&GmCommand::SetLevel { level: 1 }), gm_level::LOW_WIZARD);
        assert_eq!(required_level(&GmCommand::Notice { text: "x".into() }), gm_level::HIGH_WIZARD);
        assert_eq!(required_level(&GmCommand::GiveItem { vnum: 1, count: 1 }), gm_level::GOD);
    }

    #[test]
    fn is_allowed_gate_parity() {
        // cmd.cpp:710 — el gate falla si required > player o required == DISABLE.
        assert!(!is_allowed(gm_level::PLAYER, gm_level::LOW_WIZARD), "0 < 1");
        assert!(!is_allowed(gm_level::LOW_WIZARD, gm_level::GOD), "1 < 4");
        assert!(is_allowed(gm_level::HIGH_WIZARD, gm_level::HIGH_WIZARD), "3 >= 3");
        assert!(is_allowed(gm_level::IMPLEMENTOR, gm_level::GOD), "5 >= 4");
        assert!(!is_allowed(gm_level::IMPLEMENTOR, gm_level::DISABLE), "DISABLE rechaza a todos");
    }

    #[test]
    fn gm_level_text_mapping_parity_boot() {
        assert_eq!(gm_level_from_text("IMPLEMENTOR"), Some(gm_level::IMPLEMENTOR));
        assert_eq!(gm_level_from_text("GOD"), Some(gm_level::GOD));
        assert_eq!(gm_level_from_text("HIGH_WIZARD"), Some(gm_level::HIGH_WIZARD));
        assert_eq!(gm_level_from_text("LOW_WIZARD"), Some(gm_level::LOW_WIZARD));
        assert_eq!(gm_level_from_text("WIZARD"), Some(gm_level::WIZARD));
        assert_eq!(gm_level_from_text("implementor"), Some(gm_level::IMPLEMENTOR), "case-insensitive");
        assert_eq!(gm_level_from_text("PLAYER"), None, "el boot omite lo desconocido");
        assert_eq!(gm_level_from_text("HACKER"), None);
        assert_eq!(gm_level_from_text(""), None);
    }
}
