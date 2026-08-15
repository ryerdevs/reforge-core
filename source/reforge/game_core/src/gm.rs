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
    Warp {
        x: i32,
        y: i32,
    },
    /// `item <vnum> [count]` → item nuevo en el primer slot libre del
    /// inventario (parity do_item, cmd_gm.cpp:398-448: CreateItem + count
    /// MINMAX(1, count, g_bItemCountLimit)).
    GiveItem {
        vnum: u32,
        count: u32,
    },
    /// `notice <texto>` → GC_CHAT tipo CHAT_TYPE_NOTICE (parity do_notice,
    /// cmd_gm.cpp:1354+ → BroadcastNotice). Subset: se manda al GM (el
    /// broadcast a TODOS los jugadores necesita el task del canal — GAP
    /// documentado en channel/gm.rs).
    Notice {
        text: String,
    },
    /// `level <nivel>` → nivel del personaje con clamp 1..99 (parity
    /// do_level, cmd_gm.cpp:2423-2441: ResetPoint(MINMAX(1, level,
    /// gPlayerMaxLevel))). El recálculo de stat/skill points del ResetPoint
    /// queda fuera (GAP documentado).
    SetLevel {
        level: i32,
    },
    /// Comandos GM_PLAYER (nivel 0 — accesibles a TODOS los jugadores, sin
    /// gmlist; parity cmd.cpp:340-347): el diálogo de muerte manda
    /// `/restart_here`/`/restart_town` (uirestart.py:56-59) y el menú
    /// `/logout`/`/phase_select`/`/quit` (PythonNetworkStream.cpp:203-240).
    /// El C++ los despacha en do_restart/do_cmd (cmd_general.cpp:323-360,
    /// 402-570) — el canal traduce a revive / cierre de conexión.
    RestartHere,
    RestartTown,
    /// `/logout` → cierre de conexión (do_cmd SCMD_LOGOUT → PHASE_CLOSE).
    Logout,
    /// `/quit` → cierre de conexión (do_cmd SCMD_QUIT → quit + disconnect).
    Quit,
    /// `/phase_select` → GC_PHASE(SELECT) + cierre (vuelta al selector de
    /// personajes; el cliente reconecta al channel).
    PhaseSelect,
    // === Lote 2 GM_PLAYER (lane B, 2026-08-15) — cmd.cpp:339-466 ===
    // Regla (4) del lane: nivel PLAYER (0) para TODOS — SIN gmlist. El C++
    // congelado tiene tres excepciones (safebox cmd.cpp:351, horse_*
    // 441-446 → GM_HIGH_WIZARD; observer 420 → GM_IMPLEMENTOR) — divergencia
    // deliberada del lane, documentada también en channel/gm.rs.
    //
    // Los SIN sistema subyacente en reforge (party/horse/emociones/
    // view_equip/observer/safebox/mount/pvp/gskillup) responden INFO
    // 'not implemented' (regla 3 del lane: el comando EXISTE en el
    // cmd_info[] — no es 'No such command').
    /// `/safebox [tamaño]` — tamaño de la safebox (parity do_safebox_size
    /// cmd_gm.cpp:1857-1871: arg 0..3 → ChangeSafeboxSize). Sin sistema
    /// (la safebox vive en tablas + paquetes propios — GAP) → INFO.
    Safebox,
    /// `/safebox_close` — cierra la safebox (do_safebox_close
    /// cmd_general.cpp:796-799 → CloseSafebox). INFO (GAP).
    SafeboxClose,
    /// `/mount` — parity do_mount cmd_general.cpp:381-383: STUB VACÍO en el
    /// C++ (no hace nada). INFO.
    Mount,
    /// `/horse_state|level|ride|summon|unsummon|set_stat` — el sistema del
    /// caballo (do_horse_* cmd_gm.cpp — packets HORSE_* + proto del
    /// caballo) no existe en reforge → INFO (GAP).
    HorseState,
    HorseLevel,
    HorseRide,
    HorseSummon,
    HorseUnsummon,
    HorseSetStat,
    /// `/party_request[|_accept|_deny]` — invitar/aceptar/rechazar party
    /// (do_party_request cmd_general.cpp:1274+, vid del otro jugador). El
    /// sistema de party (grupos + broadcast) no existe en reforge → INFO
    /// (GAP).
    PartyRequest,
    PartyRequestAccept,
    PartyRequestDeny,
    /// `/pvp <vid>` — modo PVP (do_pvp cmd_general.cpp:696+, vid). Sin
    /// sistema → INFO (GAP).
    Pvp,
    /// `/view_equip <vid>` — equipo de otro jugador (do_view_equip
    /// cmd_general.cpp:1250-1271 → SendEquipment). Necesita registro
    /// vid→equip + GC_ITEM_SET del otro → INFO (GAP).
    ViewEquip,
    /// `/observer [vid]` — modo observador (do_observer cmd_general.cpp:
    /// 1236+). Sin sistema → INFO (GAP).
    Observer,
    /// `/observer_exit` — salir del modo observador (do_observer_exit
    /// cmd_general.cpp:1225+). INFO (GAP).
    ObserverExit,
    /// `/set_walk_mode` — modo caminar (parity do_set_walk_mode
    /// cmd_general.cpp:927-931: SetNowWalking+SetWalking → GC_WALK_MODE
    /// char.cpp:5759-5780). REAL: estado del personaje (sesión) +
    /// GC_WALK_MODE (channel/gm.rs).
    SetWalkMode,
    /// `/set_run_mode` — modo correr (do_set_run_mode cmd_general.cpp:
    /// 933-937). REAL: igual que SetWalkMode con walk=false.
    SetRunMode,
    /// `/skillup <vnum>` — sube el skill (parity do_skillup
    /// cmd_general.cpp:754-793 → SkillLevelUp char_skill.cpp:641-760):
    /// gasta 1 skill_point del row, +1 nivel (cap 40 — SetSkillLevel
    /// MIN(40, bLev) char_skill.cpp:207), escribe el bMasterType del nivel
    /// nuevo (parity SetSkillLevel char_skill.cpp:207-217: 20/30/40 →
    /// MASTER/GRAND/PERFECT), GC_POINTS + GC_SKILL_LEVEL +
    /// save. Sin vnum (o no numérico) → no-op silencioso (parity
    /// `if (!*arg1) return;` y str_to_number→0) y vnum 0 → no-op (parity
    /// CanUseSkill char_skill.cpp:3572 `if (0 == dwSkillVnum) return false;`
    /// + el switch del do_skillup). REAL (channel/gm.rs).
    /// GAP: sin skill_proto en reforge (CanUseSkill/level-limit/pre-skill/
    /// learnability y los saltos master por RNG 17→20/30/40).
    SkillUp {
        vnum: Option<u32>,
    },
    /// `/gskillup` — skill de guild (do_guildskillup). Sin sistema de guild
    /// → INFO (GAP).
    GuildSkillUp,
    /// `/emotion_allow <0|1>` — permite/deniega emociones de otros hacia el
    /// personaje (do_emotion_allow cmd_emotion.cpp:55+). INFO (GAP).
    EmotionAllow,
    /// `/kiss|slap|french_kiss|clap|cheer1|cheer2|dance1-6|congratulation|
    /// forgive` — emociones (do_emotion cmd_emotion.cpp:96+: la emoción
    /// sale del NOMBRE del comando, no del argumento — `emotion_types[]`
    /// cmd_emotion.cpp:30-51). El sistema de emociones (GC emoticon a los
    /// cercanos) no existe en reforge → INFO (GAP).
    Kiss,
    Slap,
    FrenchKiss,
    Clap,
    Cheer1,
    Cheer2,
    Dance1,
    Dance2,
    Dance3,
    Dance4,
    Dance5,
    Dance6,
    Congratulation,
    Forgive,
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
            Some(GmCommand::Notice {
                text: text.to_string(),
            })
        }
        "level" => {
            let level: i32 = it.next()?.parse().ok()?;
            Some(GmCommand::SetLevel {
                level: level.clamp(1, PLAYER_MAX_LEVEL),
            })
        }
        // GM_PLAYER (nivel 0) — sin argumentos (parity cmd.cpp:340-347).
        "restart_here" => Some(GmCommand::RestartHere),
        "restart_town" => Some(GmCommand::RestartTown),
        "logout" => Some(GmCommand::Logout),
        "quit" => Some(GmCommand::Quit),
        "phase_select" => Some(GmCommand::PhaseSelect),
        // Lote 2 GM_PLAYER — nombres EXACTOS del cmd.cpp:339-466 (parity;
        // el lookup del C++ es strcmp case-sensitive). Argumentos extra se
        // ignoran (parity: do_emotion ni lee el argumento; los demás solo el
        // primero).
        "safebox" => Some(GmCommand::Safebox),
        "safebox_close" => Some(GmCommand::SafeboxClose),
        "mount" => Some(GmCommand::Mount),
        "horse_state" => Some(GmCommand::HorseState),
        "horse_level" => Some(GmCommand::HorseLevel),
        "horse_ride" => Some(GmCommand::HorseRide),
        "horse_summon" => Some(GmCommand::HorseSummon),
        "horse_unsummon" => Some(GmCommand::HorseUnsummon),
        "horse_set_stat" => Some(GmCommand::HorseSetStat),
        "party_request" => Some(GmCommand::PartyRequest),
        "party_request_accept" => Some(GmCommand::PartyRequestAccept),
        "party_request_deny" => Some(GmCommand::PartyRequestDeny),
        "pvp" => Some(GmCommand::Pvp),
        "view_equip" => Some(GmCommand::ViewEquip),
        "observer" => Some(GmCommand::Observer),
        "observer_exit" => Some(GmCommand::ObserverExit),
        "set_walk_mode" => Some(GmCommand::SetWalkMode),
        "set_run_mode" => Some(GmCommand::SetRunMode),
        "skillup" => {
            // Parity do_skillup cmd_general.cpp:757-761: el vnum es el primer
            // argumento; sin él → no-op silencioso (None). "abc" →
            // str_to_number da 0 → no-op (None — mismo observable).
            let vnum = it.next().and_then(|s| s.parse().ok());
            Some(GmCommand::SkillUp { vnum })
        }
        "gskillup" => Some(GmCommand::GuildSkillUp),
        "emotion_allow" => Some(GmCommand::EmotionAllow),
        "kiss" => Some(GmCommand::Kiss),
        "slap" => Some(GmCommand::Slap),
        "french_kiss" => Some(GmCommand::FrenchKiss),
        "clap" => Some(GmCommand::Clap),
        "cheer1" => Some(GmCommand::Cheer1),
        "cheer2" => Some(GmCommand::Cheer2),
        "dance1" => Some(GmCommand::Dance1),
        "dance2" => Some(GmCommand::Dance2),
        "dance3" => Some(GmCommand::Dance3),
        "dance4" => Some(GmCommand::Dance4),
        "dance5" => Some(GmCommand::Dance5),
        "dance6" => Some(GmCommand::Dance6),
        "congratulation" => Some(GmCommand::Congratulation),
        "forgive" => Some(GmCommand::Forgive),
        _ => None,
    }
}

/// El nivel GM mínimo del comando (columna `gm_level` del `cmd_info[]` —
/// cmd.cpp:281 warp LOW_WIZARD, 283 notice HIGH_WIZARD, 297 level
/// LOW_WIZARD, 301 item GOD; los de jugador 339-466 GM_PLAYER=0 — lote 2
/// incluido, regla (4) del lane B: TODOS a PLAYER).
pub fn required_level(cmd: &GmCommand) -> i16 {
    match cmd {
        GmCommand::Warp { .. } | GmCommand::SetLevel { .. } => gm_level::LOW_WIZARD,
        GmCommand::GiveItem { .. } => gm_level::GOD,
        GmCommand::Notice { .. } => gm_level::HIGH_WIZARD,
        GmCommand::RestartHere
        | GmCommand::RestartTown
        | GmCommand::Logout
        | GmCommand::Quit
        | GmCommand::PhaseSelect
        // Lote 2 (lane B — regla 4: TODOS nivel PLAYER; el C++ congelado
        // tiene safebox/horse_*/observer en HIGH_WIZARD/IMPLEMENTOR —
        // divergencia deliberada documentada en el enum).
        | GmCommand::Safebox
        | GmCommand::SafeboxClose
        | GmCommand::Mount
        | GmCommand::HorseState
        | GmCommand::HorseLevel
        | GmCommand::HorseRide
        | GmCommand::HorseSummon
        | GmCommand::HorseUnsummon
        | GmCommand::HorseSetStat
        | GmCommand::PartyRequest
        | GmCommand::PartyRequestAccept
        | GmCommand::PartyRequestDeny
        | GmCommand::Pvp
        | GmCommand::ViewEquip
        | GmCommand::Observer
        | GmCommand::ObserverExit
        | GmCommand::SetWalkMode
        | GmCommand::SetRunMode
        | GmCommand::SkillUp { .. }
        | GmCommand::GuildSkillUp
        | GmCommand::EmotionAllow
        | GmCommand::Kiss
        | GmCommand::Slap
        | GmCommand::FrenchKiss
        | GmCommand::Clap
        | GmCommand::Cheer1
        | GmCommand::Cheer2
        | GmCommand::Dance1
        | GmCommand::Dance2
        | GmCommand::Dance3
        | GmCommand::Dance4
        | GmCommand::Dance5
        | GmCommand::Dance6
        | GmCommand::Congratulation
        | GmCommand::Forgive => gm_level::PLAYER,
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
        assert_eq!(
            parse_command("item 500"),
            Some(GmCommand::GiveItem {
                vnum: 500,
                count: 1
            })
        );
        assert_eq!(
            parse_command("item 500 5"),
            Some(GmCommand::GiveItem {
                vnum: 500,
                count: 5
            })
        );
        // MINMAX(1, count, g_bItemCountLimit) — cmd_gm.cpp:414.
        assert_eq!(
            parse_command("item 500 0"),
            Some(GmCommand::GiveItem {
                vnum: 500,
                count: 1
            })
        );
        assert_eq!(
            parse_command("item 500 999"),
            Some(GmCommand::GiveItem {
                vnum: 500,
                count: ITEM_COUNT_LIMIT
            })
        );
        assert_eq!(parse_command("item"), None, "sin vnum");
        assert_eq!(parse_command("item abc"), None, "vnum no numérico");
    }

    #[test]
    fn parse_notice_keeps_full_text() {
        assert_eq!(
            parse_command("notice hola mundo 123"),
            Some(GmCommand::Notice {
                text: "hola mundo 123".to_string()
            })
        );
        assert_eq!(parse_command("notice"), None, "sin texto");
        assert_eq!(parse_command("notice   "), None, "solo espacios");
    }

    #[test]
    fn parse_level_clamp() {
        assert_eq!(
            parse_command("level 5"),
            Some(GmCommand::SetLevel { level: 5 })
        );
        // MINMAX(1, level, gPlayerMaxLevel) — cmd_gm.cpp:2437.
        assert_eq!(
            parse_command("level 0"),
            Some(GmCommand::SetLevel { level: 1 })
        );
        assert_eq!(
            parse_command("level 500"),
            Some(GmCommand::SetLevel {
                level: PLAYER_MAX_LEVEL
            })
        );
        assert_eq!(parse_command("level"), None);
    }

    #[test]
    fn parse_unknown_and_empty_none() {
        assert_eq!(parse_command(""), None);
        assert_eq!(parse_command("   "), None);
        assert_eq!(parse_command("teleport 1 2"), None, "comando desconocido");
        assert_eq!(
            parse_command("mob 101"),
            None,
            "mob: fuera del subset (GAP)"
        );
        assert_eq!(
            parse_command("kill alguien"),
            None,
            "kill: fuera del subset (GAP)"
        );
    }

    /// Fix bug 4 (2026-08-15): los comandos del diálogo de muerte y del menú
    /// del cliente (`/restart_here`, `/restart_town`, `/logout`, `/quit`,
    /// `/phase_select` — uirestart.py:56-59, PythonNetworkStream.cpp:203-240)
    /// ahora se parsean; antes → "No such command". GM_PLAYER (nivel 0).
    #[test]
    fn parse_player_commands_gm_player() {
        assert_eq!(parse_command("restart_here"), Some(GmCommand::RestartHere));
        assert_eq!(parse_command("restart_town"), Some(GmCommand::RestartTown));
        assert_eq!(parse_command("logout"), Some(GmCommand::Logout));
        assert_eq!(parse_command("quit"), Some(GmCommand::Quit));
        assert_eq!(parse_command("phase_select"), Some(GmCommand::PhaseSelect));
        // Argumentos extra se ignoran (parity: do_restart/do_cmd NO leen el
        // argumento para SCMD_RESTART_*/LOGOUT/QUIT/PHASE_SELECT).
        assert_eq!(
            parse_command("restart_here x"),
            Some(GmCommand::RestartHere)
        );
        assert_eq!(parse_command("logout ahora"), Some(GmCommand::Logout));
        // Nivel 0 — accesibles a todos los jugadores sin gmlist.
        assert_eq!(required_level(&GmCommand::RestartHere), gm_level::PLAYER);
        assert_eq!(required_level(&GmCommand::RestartTown), gm_level::PLAYER);
        assert_eq!(required_level(&GmCommand::Logout), gm_level::PLAYER);
        assert_eq!(required_level(&GmCommand::Quit), gm_level::PLAYER);
        assert_eq!(required_level(&GmCommand::PhaseSelect), gm_level::PLAYER);
        // Cualquier jugador (nivel 0) tiene permitido.
        assert!(is_allowed(
            gm_level::PLAYER,
            required_level(&GmCommand::RestartHere)
        ));
    }

    /// Lote 2 (lane B): parseo de los 30 comandos nuevos con los nombres
    /// EXACTOS del cmd.cpp:339-466 (parity).
    #[test]
    fn parse_gm_player_batch_names() {
        assert_eq!(parse_command("safebox"), Some(GmCommand::Safebox));
        assert_eq!(
            parse_command("safebox_close"),
            Some(GmCommand::SafeboxClose)
        );
        assert_eq!(parse_command("mount"), Some(GmCommand::Mount));
        assert_eq!(parse_command("horse_state"), Some(GmCommand::HorseState));
        assert_eq!(parse_command("horse_level"), Some(GmCommand::HorseLevel));
        assert_eq!(parse_command("horse_ride"), Some(GmCommand::HorseRide));
        assert_eq!(parse_command("horse_summon"), Some(GmCommand::HorseSummon));
        assert_eq!(
            parse_command("horse_unsummon"),
            Some(GmCommand::HorseUnsummon)
        );
        assert_eq!(
            parse_command("horse_set_stat"),
            Some(GmCommand::HorseSetStat)
        );
        assert_eq!(
            parse_command("party_request"),
            Some(GmCommand::PartyRequest)
        );
        assert_eq!(
            parse_command("party_request_accept"),
            Some(GmCommand::PartyRequestAccept)
        );
        assert_eq!(
            parse_command("party_request_deny"),
            Some(GmCommand::PartyRequestDeny)
        );
        assert_eq!(parse_command("pvp"), Some(GmCommand::Pvp));
        assert_eq!(parse_command("view_equip"), Some(GmCommand::ViewEquip));
        assert_eq!(parse_command("observer"), Some(GmCommand::Observer));
        assert_eq!(
            parse_command("observer_exit"),
            Some(GmCommand::ObserverExit)
        );
        assert_eq!(parse_command("set_walk_mode"), Some(GmCommand::SetWalkMode));
        assert_eq!(parse_command("set_run_mode"), Some(GmCommand::SetRunMode));
        assert_eq!(parse_command("gskillup"), Some(GmCommand::GuildSkillUp));
        assert_eq!(
            parse_command("emotion_allow"),
            Some(GmCommand::EmotionAllow)
        );
        assert_eq!(parse_command("kiss"), Some(GmCommand::Kiss));
        assert_eq!(parse_command("slap"), Some(GmCommand::Slap));
        assert_eq!(parse_command("french_kiss"), Some(GmCommand::FrenchKiss));
        assert_eq!(parse_command("clap"), Some(GmCommand::Clap));
        assert_eq!(parse_command("cheer1"), Some(GmCommand::Cheer1));
        assert_eq!(parse_command("cheer2"), Some(GmCommand::Cheer2));
        assert_eq!(parse_command("dance1"), Some(GmCommand::Dance1));
        assert_eq!(parse_command("dance2"), Some(GmCommand::Dance2));
        assert_eq!(parse_command("dance3"), Some(GmCommand::Dance3));
        assert_eq!(parse_command("dance4"), Some(GmCommand::Dance4));
        assert_eq!(parse_command("dance5"), Some(GmCommand::Dance5));
        assert_eq!(parse_command("dance6"), Some(GmCommand::Dance6));
        assert_eq!(
            parse_command("congratulation"),
            Some(GmCommand::Congratulation)
        );
        assert_eq!(parse_command("forgive"), Some(GmCommand::Forgive));
    }

    /// Lote 2: argumentos extra ignorados (parity — do_emotion ni siquiera
    /// lee el argumento, cmd_emotion.cpp:96-110; los demás solo el primero)
    /// y case-sensitive (parity strcmp del interpret_command).
    #[test]
    fn parse_gm_player_batch_extra_args_ignored() {
        assert_eq!(parse_command("safebox 2"), Some(GmCommand::Safebox));
        assert_eq!(parse_command("kiss 7"), Some(GmCommand::Kiss));
        assert_eq!(parse_command("dance1 0"), Some(GmCommand::Dance1));
        assert_eq!(
            parse_command("party_request 123"),
            Some(GmCommand::PartyRequest)
        );
        assert_eq!(
            parse_command("set_walk_mode 1"),
            Some(GmCommand::SetWalkMode)
        );
        assert_eq!(
            parse_command("emotion_allow 1"),
            Some(GmCommand::EmotionAllow)
        );
        assert_eq!(
            parse_command("horse_set_stat 1 2 3"),
            Some(GmCommand::HorseSetStat)
        );
        assert_eq!(
            parse_command("KISS"),
            None,
            "case-sensitive (parity strcmp)"
        );
        assert_eq!(parse_command("Dance1"), None);
    }

    /// `/skillup` — el vnum es el primer argumento; sin él o no numérico →
    /// no-op silencioso (parity do_skillup cmd_general.cpp:757-761 y
    /// str_to_number→0); los extra se ignoran.
    #[test]
    fn parse_skillup_vnum_optional() {
        assert_eq!(
            parse_command("skillup 12"),
            Some(GmCommand::SkillUp { vnum: Some(12) })
        );
        assert_eq!(
            parse_command("skillup"),
            Some(GmCommand::SkillUp { vnum: None }),
            "sin vnum → no-op (parity)"
        );
        assert_eq!(
            parse_command("skillup abc"),
            Some(GmCommand::SkillUp { vnum: None }),
            "no numérico → str_to_number da 0 → no-op"
        );
        assert_eq!(
            parse_command("skillup 12 extra"),
            Some(GmCommand::SkillUp { vnum: Some(12) }),
            "args extra ignorados"
        );
    }

    /// Lote 2: TODOS nivel PLAYER (regla 4 del lane B) — accesibles sin
    /// gmlist. (El C++ congelado difiere en safebox/horse_*/observer —
    /// divergencia deliberada documentada en el enum.)
    #[test]
    fn gm_player_batch_required_level_and_gate() {
        for cmd in [
            GmCommand::Safebox,
            GmCommand::SafeboxClose,
            GmCommand::Mount,
            GmCommand::HorseState,
            GmCommand::HorseLevel,
            GmCommand::HorseRide,
            GmCommand::HorseSummon,
            GmCommand::HorseUnsummon,
            GmCommand::HorseSetStat,
            GmCommand::PartyRequest,
            GmCommand::PartyRequestAccept,
            GmCommand::PartyRequestDeny,
            GmCommand::Pvp,
            GmCommand::ViewEquip,
            GmCommand::Observer,
            GmCommand::ObserverExit,
            GmCommand::SetWalkMode,
            GmCommand::SetRunMode,
            GmCommand::SkillUp { vnum: Some(1) },
            GmCommand::GuildSkillUp,
            GmCommand::EmotionAllow,
            GmCommand::Kiss,
            GmCommand::Slap,
            GmCommand::FrenchKiss,
            GmCommand::Clap,
            GmCommand::Cheer1,
            GmCommand::Cheer2,
            GmCommand::Dance1,
            GmCommand::Dance2,
            GmCommand::Dance3,
            GmCommand::Dance4,
            GmCommand::Dance5,
            GmCommand::Dance6,
            GmCommand::Congratulation,
            GmCommand::Forgive,
        ] {
            assert_eq!(required_level(&cmd), gm_level::PLAYER, "{cmd:?}");
            assert!(
                is_allowed(gm_level::PLAYER, required_level(&cmd)),
                "{cmd:?} debe pasar con nivel 0"
            );
        }
    }

    #[test]
    fn required_levels_parity_cmd_table() {
        assert_eq!(
            required_level(&GmCommand::Warp { x: 0, y: 0 }),
            gm_level::LOW_WIZARD
        );
        assert_eq!(
            required_level(&GmCommand::SetLevel { level: 1 }),
            gm_level::LOW_WIZARD
        );
        assert_eq!(
            required_level(&GmCommand::Notice { text: "x".into() }),
            gm_level::HIGH_WIZARD
        );
        assert_eq!(
            required_level(&GmCommand::GiveItem { vnum: 1, count: 1 }),
            gm_level::GOD
        );
    }

    #[test]
    fn is_allowed_gate_parity() {
        // cmd.cpp:710 — el gate falla si required > player o required == DISABLE.
        assert!(!is_allowed(gm_level::PLAYER, gm_level::LOW_WIZARD), "0 < 1");
        assert!(!is_allowed(gm_level::LOW_WIZARD, gm_level::GOD), "1 < 4");
        assert!(
            is_allowed(gm_level::HIGH_WIZARD, gm_level::HIGH_WIZARD),
            "3 >= 3"
        );
        assert!(is_allowed(gm_level::IMPLEMENTOR, gm_level::GOD), "5 >= 4");
        assert!(
            !is_allowed(gm_level::IMPLEMENTOR, gm_level::DISABLE),
            "DISABLE rechaza a todos"
        );
    }

    #[test]
    fn gm_level_text_mapping_parity_boot() {
        assert_eq!(
            gm_level_from_text("IMPLEMENTOR"),
            Some(gm_level::IMPLEMENTOR)
        );
        assert_eq!(gm_level_from_text("GOD"), Some(gm_level::GOD));
        assert_eq!(
            gm_level_from_text("HIGH_WIZARD"),
            Some(gm_level::HIGH_WIZARD)
        );
        assert_eq!(gm_level_from_text("LOW_WIZARD"), Some(gm_level::LOW_WIZARD));
        assert_eq!(gm_level_from_text("WIZARD"), Some(gm_level::WIZARD));
        assert_eq!(
            gm_level_from_text("implementor"),
            Some(gm_level::IMPLEMENTOR),
            "case-insensitive"
        );
        assert_eq!(
            gm_level_from_text("PLAYER"),
            None,
            "el boot omite lo desconocido"
        );
        assert_eq!(gm_level_from_text("HACKER"), None);
        assert_eq!(gm_level_from_text(""), None);
    }
}
