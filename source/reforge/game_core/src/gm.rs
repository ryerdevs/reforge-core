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
    /// cmd_gm.cpp:1857-1871: arg 0..3, clamp a 0 — GM_HIGH_WIZARD en el
    /// cmd.cpp:351). REAL (channel/safebox.rs: GC_SAFEBOX_SIZE + grid +
    /// SafeboxRepo::set_size — divergencia: el C++ GM command no persiste).
    Safebox {
        size: u8,
    },
    /// `/safebox_password <password>` — abrir la safebox (parity
    /// do_safebox_password cmd_general.cpp:805-810 → ReqSafeboxLoad —
    /// GM_PLAYER, cmd.cpp:354). REAL (channel/safebox.rs: validación de la
    /// password + GC_SAFEBOX_SIZE + GC_SAFEBOX_SET por item).
    SafeboxPassword {
        password: String,
    },
    /// `/safebox_close` — cierra la safebox (do_safebox_close
    /// cmd_general.cpp:796-799 → CloseSafebox). REAL (channel/safebox.rs:
    /// oro persistido + CHAT COMMAND "CloseSafebox").
    SafeboxClose,
    /// `/safebox_change_password <old> <new>` — cambiar la password de la
    /// caja (parity do_safebox_change_password cmd_general.cpp:812-838 →
    /// GD_SAFEBOX_CHANGE_PASSWORD → RESULT_SAFEBOX_CHANGE_PASSWORD
    /// ClientManager.cpp:991-1053; GM_PLAYER, cmd.cpp:355 — el cliente lo
    /// manda desde el diálogo de la caja, uisafebox.py:178). REAL
    /// (channel/safebox.rs + SafeboxRepo: old incorrecto → INFO; sin fila
    /// → INSERT con la nueva).
    SafeboxChangePassword {
        old: String,
        new: String,
    },
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
    // === Lote 3 (lane 2026-08-17) — los commands de mayor valor jugable ===
    // do_mob/do_kill/do_purge/do_goto/do_stat (cmd_gm.cpp + cmd_general.cpp).
    /// `/mob <vnum> [count]` — spawn `count` (clamp 1..20 — parity
    /// MINMAX(1, iCount, 20) cmd_gm.cpp:662) copias del mob en un rect
    /// aleatorio ±(200..750) alrededor del GM (parity do_mob cmd_gm.cpp:
    /// 630-700 → SpawnMobRange). GM_HIGH_WIZARD (cmd.cpp:310). GAP: el
    /// nombre del mob no se resuelve (solo vnum numérico — el lookup por
    /// nombre del C++ `CMobManager::Get(arg1, true)` necesita un índice
    /// nombre→vnum que reforge no tiene).
    Mob {
        vnum: u32,
        count: u32,
    },
    /// `/kill` — mata el TARGET del jugador (CG_TARGET) si es un mob
    /// (parity do_kill cmd_gm.cpp:1505+ → `SetDead` directo: SIN drop ni
    /// exp; PC → no-op). GM_HIGH_WIZARD (cmd.cpp:314). DIVERGENCIA
    /// deliberada del lane: el C++ congelado mata a un JUGADOR por nombre
    /// (`FindByCharacterName` → `Dead()`) — el rewrite solo apunta mobs
    /// (el handler de comandos no alcanza el registro nombre→vid; la
    /// variante jugador necesita el sistema de muerte de PCs del PvP).
    Kill,
    /// `/purge [all]` — mata los mobs del área (sin `all`: radio 1000
    /// units — parity `FuncPurge` cmd_gm.cpp:757; con `all`: todo el mapa).
    /// Sin drop ni exp (`M2_DESTROY_CHARACTER` directo). GM_WIZARD
    /// (cmd.cpp:292).
    Purge {
        all: bool,
    },
    /// `/goto <nombre>` — teletransporta al GM a la posición del jugador
    /// nombrado (parity do_goto → WarpSet; el C++ congelado es
    /// `goto <x y>`/`goto <mapname> [empire]` — DIVERGENCIA deliberada del
    /// lane: la forma jugador es la de mayor valor jugable y el registro de
    /// sesiones del channel (chat.rs) la permite; la de coordenadas ya
    /// existe como `warp`). GM_LOW_WIZARD (cmd.cpp:296).
    Goto {
        name: String,
    },
    /// `/stat <st|dx|ht|iq> [cantidad]` — asigna puntos de stat gastando
    /// POINT_STAT, SIN cap (divergencia deliberada 2026-08-27: el C++
    /// capaba en MAX_STAT = 90 — `GetRealPoint(idx) >= MAX_STAT`, parity
    /// do_stat cmd_general.cpp:644-702 y g_iStatusPointSetMaxValue
    /// config.cpp:48; el rewrite da 5 puntos por nivel sin límite → stats
    /// infinitos). La cantidad es una EXTENSIÓN del lane (el C++ solo
    /// hace +1; el cliente manda `/stat st` sin cantidad → 1). El
    /// recálculo de MAX_HP/MAX_SP del C++ (PointChange(POINT_MAX_HP/SP, 0))
    /// lo refleja el GC_POINTS del rewrite vía `compute_max_points`
    /// (max_hp = f(ht), max_sp = f(iq) — parity char.cpp:2230-2231).
    /// GM_PLAYER (cmd.cpp:324 — el cliente lo usa para asignar stats).
    Stat {
        point: StatPoint,
        amount: i32,
    },
    /// `/stat- <st|dx|ht|iq> [cantidad]` — devuelve puntos de stat (parity
    /// do_stat_minus cmd_general.cpp:577-643: floor de los iniciales del job
    /// — JobInitialPoints constants.cpp:6-15; `PointChange(POINT_STAT, +1)`).
    /// DIVERGENCIA deliberada (2026-08-27): el C++ gasta
    /// POINT_STAT_RESET_COUNT por punto devuelto — el rewrite no lo
    /// requiere (puntos infinitos, sin reset limitado). GM_PLAYER
    /// (cmd.cpp:325).
    StatMinus {
        point: StatPoint,
        amount: i32,
    },
    /// `/emotion_allow <vid>` — permite emociones de otros hacia el
    /// personaje (do_emotion_allow cmd_emotion.cpp:55+). REAL desde el
    /// bloque messenger+emotions 2026-08-21: el hook social de chat.rs
    /// (channel/emotions.rs) lo intercepta ANTES de gm::handle — estas
    /// variantes quedan sombra (solo por exhaustividad del match).
    EmotionAllow,
    /// `/kiss|slap|french_kiss|clap|cheer1|cheer2|dance1-6|congratulation|
    /// forgive` — emociones (do_emotion cmd_emotion.cpp:96+: la emoción
    /// sale del NOMBRE del comando, no del argumento — `emotion_types[]`
    /// cmd_emotion.cpp:30-51). REAL desde 2026-08-21: las maneja
    /// channel/emotions.rs vía el hook de chat.rs (estas variantes quedan
    /// sombra — exhaustividad).
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
    /// `/polymorph <vnum>` — transformación (parity do_polymorph
    /// cmd_gm.cpp:2736-2761 — LOW_WIZARD; vnum 0 quita el poly).
    Polymorph { vnum: u32 },
    /// `/setskill <vnum> <level>` — nivel de skill (parity do_setskill
    /// cmd_gm.cpp:2302-2336 — LOW_WIZARD; level cap 40).
    SetSkill { vnum: u32, level: u8 },
}

/// POINT_ST/POINT_DX/POINT_HT/POINT_IQ del do_stat (cmd_general.cpp:664-671
/// — compara por nombre "st"/"dx"/"ht"/"iq", case-sensitive como el
/// strcmp del interpret_command).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatPoint {
    St,
    Dx,
    Ht,
    Iq,
}

impl StatPoint {
    /// El nombre del argumento del comando (parity strcmp del do_stat).
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "st" => Some(Self::St),
            "dx" => Some(Self::Dx),
            "ht" => Some(Self::Ht),
            "iq" => Some(Self::Iq),
            _ => None,
        }
    }

    /// El nombre canónico (logs del handler).
    pub fn name(self) -> &'static str {
        match self {
            Self::St => "st",
            Self::Dx => "dx",
            Self::Ht => "ht",
            Self::Iq => "iq",
        }
    }

    /// POINT_ST/POINT_HT/POINT_DX/POINT_IQ del wire (length.h:500-506 — el
    /// GC_POINTS los manda en estos índices).
    pub fn index(self) -> usize {
        match self {
            Self::St => 12,
            Self::Ht => 13,
            Self::Dx => 14,
            Self::Iq => 15,
        }
    }
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
        // primero). EXCEPCIONES: `safebox` (size — HIGH_WIZARD, cmd.cpp:351)
        // y `safebox_password` (password — el cliente abre la caja con
        // `/safebox_password <pass>`, cmd.cpp:354).
        "safebox" => {
            // Parity do_safebox_size cmd_gm.cpp:1857-1871: arg opcional
            // 0..3 (sin arg → 0); `size > 3 || size < 0 → 0` (NO clamp).
            let size = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let size = if size > 3 { 0 } else { size };
            Some(GmCommand::Safebox { size })
        }
        // Parity do_safebox_password cmd_general.cpp:805-806 (one_argument:
        // sin argumento → cadena vacía → el handler responde "wrong
        // password").
        "safebox_password" => Some(GmCommand::SafeboxPassword {
            password: it.next().unwrap_or("").to_string(),
        }),
        "safebox_close" => Some(GmCommand::SafeboxClose),
        // Parity do_safebox_change_password cmd_general.cpp:812-817
        // (two_arguments: old y new; faltante → cadena vacía → el handler
        // responde INFO "wrong password").
        "safebox_change_password" => Some(GmCommand::SafeboxChangePassword {
            old: it.next().unwrap_or("").to_string(),
            new: it.next().unwrap_or("").to_string(),
        }),
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
        // Lote 3 — parity de los nombres del cmd.cpp:292/296/310/314/324-325
        // (strcmp case-sensitive). `kill` ignora argumentos extra (el target
        // vive en la sesión, no en el texto).
        "mob" => {
            let vnum: u32 = it.next()?.parse().ok()?;
            let count = it
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1)
                .clamp(1, 20); // parity MINMAX(1, iCount, 20) cmd_gm.cpp:662
            Some(GmCommand::Mob { vnum, count })
        }
        "kill" => Some(GmCommand::Kill),
        "purge" => {
            // Parity do_purge cmd_gm.cpp:775-783: el flag `all` solo si el
            // primer argumento es exactamente "all"; cualquier otro valor
            // → radio 1000 (NO es error).
            let all = it.next().is_some_and(|s| s == "all");
            Some(GmCommand::Purge { all })
        }
        "goto" => {
            let name = it.next()?.to_string();
            if name.is_empty() {
                return None;
            }
            Some(GmCommand::Goto { name })
        }
        "stat" | "stat-" => {
            // Parity do_stat/do_stat_minus: el primer argumento es el punto
            // ("st"/"dx"/"ht"/"iq"); sin él → no-op. La CANTIDAD es una
            // extensión del lane (default 1 — el cliente manda `/stat st`);
            // cantidad <= 0 → None (parity del "잘못 입력하셨습니다" del
            // do_stat_plus_amount — el subset responde el INFO genérico).
            let point = StatPoint::from_name(it.next()?)?;
            let amount = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
            if amount <= 0 {
                return None;
            }
            if name == "stat-" {
                Some(GmCommand::StatMinus { point, amount })
            } else {
                Some(GmCommand::Stat { point, amount })
            }
        }
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
        "polymorph" => {
            let vnum: u32 = it.next()?.parse().ok()?;
            Some(GmCommand::Polymorph { vnum })
        }
        "setskill" => {
            let vnum: u32 = it.next()?.parse().ok()?;
            let level: u8 = it.next()?.parse().ok()?;
            Some(GmCommand::SetSkill { vnum, level: level.min(40) })
        }
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
        // Lote 3 (parity cmd.cpp): mob 310 HIGH_WIZARD, kill 314
        // HIGH_WIZARD, purge 292 WIZARD, goto 296 LOW_WIZARD; stat/stat-
        // son GM_PLAYER (cmd.cpp:324-325 — el cliente los usa sin gmlist).
        GmCommand::Polymorph { .. } | GmCommand::SetSkill { .. } => gm_level::LOW_WIZARD,
        GmCommand::Mob { .. } | GmCommand::Kill => gm_level::HIGH_WIZARD,
        GmCommand::Purge { .. } => gm_level::WIZARD,
        GmCommand::Goto { .. } => gm_level::LOW_WIZARD,
        // Safebox (tamaño): GM_HIGH_WIZARD (parity cmd.cpp:351).
        GmCommand::Safebox { .. } => gm_level::HIGH_WIZARD,
        GmCommand::RestartHere
        | GmCommand::RestartTown
        | GmCommand::Logout
        | GmCommand::Quit
        | GmCommand::PhaseSelect
        // Lote 2 (lane B — regla 4: TODOS nivel PLAYER; el C++ congelado
        // tiene horse_*/observer en HIGH_WIZARD/IMPLEMENTOR — divergencia
        // deliberada documentada en el enum). EXCEPCIONES REALES: `safebox`
        // (tamaño — HIGH_WIZARD, parity cmd.cpp:351) y
        // `safebox_password`/`safebox_close` (abrir/cerrar — PLAYER,
        // cmd.cpp:352,354).
        | GmCommand::SafeboxPassword { .. }
        | GmCommand::SafeboxClose
        | GmCommand::SafeboxChangePassword { .. }
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
        | GmCommand::Stat { .. }
        | GmCommand::StatMinus { .. }
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
        assert_eq!(parse_command("mob"), None, "mob sin vnum");
        assert_eq!(parse_command("mob abc"), None, "mob vnum no numérico");
        assert_eq!(parse_command("kill x"), Some(GmCommand::Kill), "kill ignora el arg");
        assert_eq!(parse_command("goto"), None, "goto sin nombre");
        assert_eq!(parse_command("stat"), None, "stat sin punto");
        assert_eq!(parse_command("stat foo"), None, "punto desconocido");
        assert_eq!(parse_command("stat st -3"), None, "cantidad negativa → no-op");
    }

    /// Lote 3: parseo de los 5 comandos nuevos (parity de los nombres del
    /// cmd.cpp — strcmp case-sensitive).
    #[test]
    fn parse_lote3_commands() {
        assert_eq!(
            parse_command("mob 101"),
            Some(GmCommand::Mob { vnum: 101, count: 1 }),
            "sin count → 1 (parity do_mob)"
        );
        assert_eq!(
            parse_command("mob 101 5"),
            Some(GmCommand::Mob { vnum: 101, count: 5 })
        );
        assert_eq!(
            parse_command("mob 101 0"),
            Some(GmCommand::Mob { vnum: 101, count: 1 }),
            "MINMAX(1, count, 20)"
        );
        assert_eq!(
            parse_command("mob 101 99"),
            Some(GmCommand::Mob { vnum: 101, count: 20 }),
            "cap 20 (@fixme339)"
        );
        assert_eq!(parse_command("kill"), Some(GmCommand::Kill));
        assert_eq!(parse_command("purge"), Some(GmCommand::Purge { all: false }));
        assert_eq!(
            parse_command("purge all"),
            Some(GmCommand::Purge { all: true })
        );
        assert_eq!(
            parse_command("purge xyz"),
            Some(GmCommand::Purge { all: false }),
            "arg != all → radio 1000 (parity, NO error)"
        );
        assert_eq!(
            parse_command("goto Pepe"),
            Some(GmCommand::Goto { name: "Pepe".into() })
        );
        assert_eq!(
            parse_command("goto  pepe  extra"),
            Some(GmCommand::Goto { name: "pepe".into() }),
            "solo el primer token (one_argument)"
        );
        assert_eq!(parse_command("MOB 101"), None, "case-sensitive");
        assert_eq!(parse_command("Purge"), None);
    }

    /// Lote 3: `/stat`/`/stat-` — el punto por nombre exacto, la cantidad
    /// opcional (default 1 — el cliente manda `/stat st` sin cantidad).
    #[test]
    fn parse_stat_point_and_amount() {
        assert_eq!(
            parse_command("stat st"),
            Some(GmCommand::Stat { point: StatPoint::St, amount: 1 })
        );
        assert_eq!(
            parse_command("stat dx 5"),
            Some(GmCommand::Stat { point: StatPoint::Dx, amount: 5 })
        );
        assert_eq!(
            parse_command("stat- ht 2"),
            Some(GmCommand::StatMinus { point: StatPoint::Ht, amount: 2 })
        );
        assert_eq!(
            parse_command("stat- iq"),
            Some(GmCommand::StatMinus { point: StatPoint::Iq, amount: 1 })
        );
        assert_eq!(parse_command("stat ST"), None, "case-sensitive (parity strcmp)");
        assert_eq!(parse_command("stat-"), None, "sin punto → no-op");
        assert_eq!(parse_command("stat st 0"), None, "cantidad 0 → no-op");
        assert_eq!(parse_command("stat- dx abc"), Some(GmCommand::StatMinus { point: StatPoint::Dx, amount: 1 }), "no numérico → default 1");
    }

    /// Lote 3: niveles GM de parity (cmd.cpp:292/296/310/314/324-325) y
    /// el gate para el GM mínimo.
    #[test]
    fn lote3_required_levels_parity() {
        assert_eq!(required_level(&GmCommand::Mob { vnum: 1, count: 1 }), gm_level::HIGH_WIZARD);
        assert_eq!(required_level(&GmCommand::Kill), gm_level::HIGH_WIZARD);
        assert_eq!(required_level(&GmCommand::Purge { all: false }), gm_level::WIZARD);
        assert_eq!(required_level(&GmCommand::Goto { name: "x".into() }), gm_level::LOW_WIZARD);
        assert_eq!(required_level(&GmCommand::Stat { point: StatPoint::St, amount: 1 }), gm_level::PLAYER);
        assert_eq!(required_level(&GmCommand::StatMinus { point: StatPoint::St, amount: 1 }), gm_level::PLAYER);
        // Gate: LOW_WIZARD no puede purge (1 < 2); WIZARD sí (2 >= 2).
        assert!(!is_allowed(gm_level::LOW_WIZARD, required_level(&GmCommand::Purge { all: false })));
        assert!(is_allowed(gm_level::WIZARD, required_level(&GmCommand::Purge { all: false })));
        assert!(is_allowed(gm_level::LOW_WIZARD, required_level(&GmCommand::Goto { name: "x".into() })));
        assert!(is_allowed(gm_level::PLAYER, required_level(&GmCommand::Stat { point: StatPoint::St, amount: 1 })), "stat es GM_PLAYER — sin gmlist");
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

    /// Lote 2 (lane B): parseo de los comandos nuevos con los nombres
    /// EXACTOS del cmd.cpp:339-466 (parity).
    #[test]
    fn parse_gm_player_batch_names() {
        assert_eq!(
            parse_command("safebox"),
            Some(GmCommand::Safebox { size: 0 }),
            "sin arg → 0 (parity do_safebox_size)"
        );
        assert_eq!(
            parse_command("safebox_password 1234"),
            Some(GmCommand::SafeboxPassword {
                password: "1234".into()
            }),
            "el cliente abre con /safebox_password <pass>"
        );
        assert_eq!(
            parse_command("safebox_password"),
            Some(GmCommand::SafeboxPassword {
                password: "".into()
            }),
            "sin password → cadena vacía (one_argument; el handler la rechaza)"
        );
        assert_eq!(
            parse_command("safebox_close"),
            Some(GmCommand::SafeboxClose)
        );
        assert_eq!(
            parse_command("safebox_change_password 1111 2222"),
            Some(GmCommand::SafeboxChangePassword {
                old: "1111".into(),
                new: "2222".into(),
            }),
            "el diálogo del cliente lo manda (uisafebox.py:178)"
        );
        assert_eq!(
            parse_command("safebox_change_password 1111"),
            Some(GmCommand::SafeboxChangePassword {
                old: "1111".into(),
                new: "".into(),
            }),
            "faltante → vacío (two_arguments; el handler lo rechaza)"
        );
        assert_eq!(
            required_level(&GmCommand::SafeboxChangePassword {
                old: "".into(),
                new: "".into()
            }),
            gm_level::PLAYER
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
    /// y case-sensitive (parity strcmp del interpret_command). EXCEPCIÓN:
    /// `safebox` (el arg ES el tamaño, cmd_gm.cpp:1857-1871) y
    /// `safebox_password` (el arg ES la password).
    #[test]
    fn parse_gm_player_batch_extra_args_ignored() {
        assert_eq!(
            parse_command("safebox 2"),
            Some(GmCommand::Safebox { size: 2 })
        );
        assert_eq!(
            parse_command("safebox 9"),
            Some(GmCommand::Safebox { size: 0 }),
            "clamp 0..3 (parity `size > 3 || size < 0 → 0`)"
        );
        assert_eq!(
            parse_command("safebox_password abc def"),
            Some(GmCommand::SafeboxPassword {
                password: "abc".into()
            }),
            "solo el primer token (one_argument)"
        );
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
    /// gmlist. (El C++ congelado difiere en horse_*/observer — divergencia
    /// deliberada documentada en el enum; safebox_password/safebox_close son
    /// PLAYER por parity cmd.cpp:352,354.)
    #[test]
    fn gm_player_batch_required_level_and_gate() {
        for cmd in [
            GmCommand::SafeboxClose,
            GmCommand::SafeboxPassword {
                password: "1234".into(),
            },
            GmCommand::Mount,
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

    #[test]
    fn verifier_polymorph_setskill() {
        assert_eq!(parse_command("polymorph 101"), Some(GmCommand::Polymorph { vnum: 101 }));
        assert_eq!(parse_command("polymorph"), None);
        assert_eq!(parse_command("setskill 42 20"), Some(GmCommand::SetSkill { vnum: 42, level: 20 }));
        assert_eq!(parse_command("setskill 42 99"), Some(GmCommand::SetSkill { vnum: 42, level: 40 }));
        assert_eq!(parse_command("setskill"), None);
        assert_eq!(required_level(&GmCommand::Polymorph { vnum: 1 }), gm_level::LOW_WIZARD);
        assert_eq!(required_level(&GmCommand::SetSkill { vnum: 1, level: 1 }), gm_level::LOW_WIZARD);
    }
}
