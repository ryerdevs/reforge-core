//! F5.2: el CORE del combate (server-authoritative) — parity `battle.cpp` /
//! `char_battle.cpp` / `char.cpp` (el subset base SIN skills/items).
//!
//! # Subset base documentado (el "oráculo" con file:line)
//!
//! **Daño melee** (PC atacando, sin arma, sin items/skills — `battle.cpp:464-638`
//! `CalcMeleeDamage` + `:199-206` `CalcBattleDamage` + `:731-755` `battle_hit`):
//! 1. `ATT_GRADE` del atacante (`char.cpp:2059-2092` ComputeBattlePoints):
//!    `level*2 + statAtk`; `statAtk` por job (`:2064-2087`): warrior/sura
//!    `2*st`, assassin `(4*st + 2*dx)/3`, shaman `(4*st + 2*iq)/3` (división
//!    ENTERA, truncación C++).
//! 2. `DEF_GRADE` de la víctima mob (`char.cpp:2156-2158`): `level + ht + wDef`
//!    (el mob de `mob_proto`: bLevel/bCon/wDef — `tables.h:448/463/457`).
//! 3. `CalcAttackRating` (`battle.cpp:227-251`): `iARSrc = MIN(90, (dx_a*4 +
//!    lv_a*2)/6)`, `iERSrc = MIN(90, (dx_v*4 + lv_v*2)/6)`,
//!    `fAR = (iARSrc+210)/300`, `fER = ((iERSrc*2+5)/(iERSrc+95)) * 3/10`
//!    (la división de fER es ENTERA), `fAR -= fER`.
//! 4. Sin arma → rango de daño 0..1 (`Item_GetDamage` con item null,
//!    `battle.cpp:442-462`); `iDam = number(0,1) * 2` (`:533`).
//! 5. `iAtk = (ATT_GRADE + iDam - lv*2) * fAR + lv*2` (truncación en el
//!    producto, `:542-544`); + `arma.Value(5)*2` = 0 sin arma (`:546-553`);
//!    + party bonus = 0 (`:555`); × `(100 + ATT_BONUS + MELEE_MAGIC_ATT_BONUS_PER)
//!    /100` = ×1 (`:556`); `CalcAttBonus` = identidad en el subset base
//!    (`:305-440` — todos los términos base valen 0).
//! 6. `iDef = DEF_GRADE * (100 + DEF_BONUS)/100` = DEF_GRADE (`:564`,
//!    DEF_BONUS = 0).
//! 7. `iDam = MAX(0, iAtk - iDef)` (`:573`); `CalcBattleDamage` (`:199-206`):
//!    `if (iDam < 3) iDam = number(1, 5)` (el CALCULATE_DAMAGE_LVDELTA está
//!    comentado — el daño pasa igual).
//! 8. `battle_hit` (`:736-749`): `CalcDamBonus` = identidad (sin arma,
//!    `:265-303`); `attMul = 1.0` (`char.cpp:411`) → `iDam = 1.0*iDam + 0.5`
//!    truncado = iDam (sin cambio en el subset).
//!
//! **Intervalo / cooldown** (`battle.cpp:757-782` `GET_ATTACK_SPEED` +
//! `ani.cpp:341-351` + `config.cpp:101`): sin arma → `ani_speed = 1000` ms;
//! `real_speed = ani_speed*100 / (SPEEDHACK_LIMIT_BONUS(80) + ATT_SPEED(0) +
//! riding(0))` = **1250 ms** (daga/garra → /2, `:774-779` — fuera del subset).
//! Enforcement `IS_SPEED_HACK` (`:808-838`): rechazo si `now - last < speed`
//! atacando al MISMO objetivo; el rechazo IGUAL actualiza el timer (`:833`).
//! Nota: el C++ solo lo aplica con `gHackCheckEnable` (config, default
//! `false` — `config.cpp:127`); el core Rust lo aplica SIEMPRE (el server es
//! la autoridad — decisión documentada).
//!
//! **Rango** (`battle.cpp:127-167` battle_melee_attack): `distance =
//! DISTANCE_APPROX(|dx|,|dy|)` (`utils.h:19-43` = `(246*max + 102*min) >> 8`);
//! atacante PC: `max = 300` UNITS (3 m), salvo víctima mob MELEE con más
//! alcance: `max = MAX(300, (int)(wAttackRange * 1.15f))` (`:150-158`).
//! `distance > max` → BATTLE_NONE (sin golpe).
//!
//! **LoS**: sin mapa de colisión por ahora (SDB vacío — parity del runtime
//! actual): la walkability del mapa decide; no hay obstáculos que ocluyan.
//!
//! # Paquetes (ver `protocol::combat` para los layouts)
//!
//! Golpe con daño → `[GC_ATTACK (12), GC_DAMAGE_INFO (135)]` (el segundo es
//! el que el cliente muestra — `AddDamageEffect`); golpe sin daño → solo
//! `GC_ATTACK`. El C++ manda SOLO el GC_DAMAGE_INFO (`char_battle.cpp:1508-1530`,
//! a atacante y víctima-PC); el GC_ATTACK se incluye por contrato wire
//! (observadores futuros — el cliente v24 lo ignora, ver `protocol::combat`).

use protocol::combat::{damage_flag, CgAttack, GcDamageInfo};

// ---------------------------------------------------------------------------
// Estados (dominio — los construye el canal)
// ---------------------------------------------------------------------------

/// El estado de combate MUTABLE de un jugador (el canal guarda uno por
/// conexión, junto al `PlayerMotion` de F5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombatState {
    /// Reloj del server del último CG_ATTACK (parity `m_kAttackLog.dwTime`,
    /// `battle.cpp:784-794`). El C++ lo actualiza incluso en rechazos.
    pub last_attack_time: u64,
    /// VID del último objetivo atacado (parity `m_kAttackLog.dwVID` — el
    /// cooldown solo aplica contra el MISMO objetivo).
    pub last_attack_vid: u32,
}

impl CombatState {
    /// Estado inicial (el C++ zero-inicializa el `m_kAttackLog`).
    pub fn new() -> Self {
        Self { last_attack_time: 0, last_attack_vid: 0 }
    }
}

/// La vista del ATACANTE (jugador) para un ataque. Inmutable por llamada; el
/// canal la construye del `PlayerRow` + `PlayerMotion` (F5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerState {
    /// VID del jugador (un PC: su player id — `packets.rs:166`).
    pub vid: u32,
    pub x: i32,
    pub y: i32,
    pub level: i32,
    /// `JOB_*`: warrior 0, assassin 1, sura 2, shaman 3 (`constants.cpp:18-21`).
    pub job: u8,
    pub st: i32,
    pub ht: i32,
    pub dx: i32,
    pub iq: i32,
    /// Intervalo entre ataques en ms (`GET_ATTACK_SPEED`, battle.cpp:757-782).
    pub attack_speed_ms: u32,
    /// Bonus de ATT_GRADE de los buffs (parity `POINT_ATT_GRADE_BONUS` —
    /// char.h:95; el total ATT_GRADE del legacy = base + bonus). 0 sin buffs.
    pub att_grade_bonus: i32,
    /// % de crítico REAL del jugador (parity `POINT_CRITICAL_PCT`
    /// procesado — 0..100; el roll `number(1,100) ≤ pct` duplica el daño).
    pub critical_pct: i32,
}

impl PlayerState {
    /// Construye el estado desde el row del player + el motion F5.1. El
    /// `attack_speed_ms` = `default_attack_speed()` (sin arma — 1250 ms); el
    /// lane de items lo reemplazará con la velocidad del arma equipada.
    pub fn from_row(
        row: &database::player::PlayerRow,
        motion: &crate::movement::PlayerMotion,
    ) -> Self {
        Self {
            vid: row.id as u32,
            x: motion.x,
            y: motion.y,
            level: i32::from(row.level),
            job: row.job as u8,
            st: i32::from(row.st),
            ht: i32::from(row.ht),
            dx: i32::from(row.dx),
            iq: i32::from(row.iq),
            attack_speed_ms: default_attack_speed(),
            att_grade_bonus: 0,
            critical_pct: 0,
        }
    }
}

/// La vista del OBJETIVO (mob/NPC). El lane de NPCs (F5) lo construye del
/// `mob_proto` (`tables.h:440-470` — bLevel/bStr/bDex/bCon/wDef/wAttackRange/
/// bBattleType).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcState {
    pub vid: u32,
    pub x: i32,
    pub y: i32,
    pub level: i32,
    pub dx: i32,
    pub ht: i32,
    /// `wDef` del mob_proto (DEF base del mob).
    pub wdef: i32,
    /// `bBattleType`: MELEE = 0 (extiende el rango del atacante PC — ver
    /// `melee_max_range`). `battle.h:6`, `constants.cpp:46`.
    pub battle_type: u8,
    /// `wAttackRange` del mob_proto (UNITS; p.ej. mob 101 = 175).
    pub attack_range: u32,
}

/// Resultado de `handle_attack` — lo que el canal envía al cliente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombatResult {
    /// Paquetes S→C ya encodados (`GcAttack` y/o `GcDamageInfo`) — el canal
    /// los manda por la conexión del atacante (y a los observadores cuando
    /// existan). Vacío si el ataque se rechazó (cooldown/rango/sin objetivo).
    pub packets: Vec<Vec<u8>>,
    /// Daño aplicado al objetivo (0 si no hubo golpe). El canal lo aplica al
    /// HP del mundo (NPCs — lane F5) y decide el `GC_DEAD`/`GC_CHARACTER_DEL`.
    pub damage: i32,
}

impl CombatResult {
    fn empty() -> Self {
        Self { packets: Vec::new(), damage: 0 }
    }
}

// ---------------------------------------------------------------------------
// Constantes (parity)
// ---------------------------------------------------------------------------

/// Jobs (`constants.cpp:18-21` — orden del array `JobInitialPoints`).
pub mod job {
    pub const WARRIOR: u8 = 0;
    pub const ASSASSIN: u8 = 1;
    pub const SURA: u8 = 2;
    pub const SHAMAN: u8 = 3;
}

/// `BATTLE_TYPE_MELEE` = 0 (`battle.h:6`).
pub const BATTLE_TYPE_MELEE: u8 = 0;

/// `BATTLE_TYPE_RANGE` = 1 / `BATTLE_TYPE_MAGIC` = 2 (`battle.h:7-8`).
pub const BATTLE_TYPE_RANGE: u8 = 1;
pub const BATTLE_TYPE_MAGIC: u8 = 2;

/// `POINT_BOW_DISTANCE` default = 300 (`char.cpp:2010-2020` — GetMobAttackRange
/// suma este bonus al rango de los mobs RANGE/MAGIC).
pub const BOW_DISTANCE_DEFAULT: i32 = 300;

/// Rango base melee del atacante PC: 300 UNITS = 3 m (`battle.cpp:148`).
pub const MELEE_RANGE_UNITS: i32 = 300;

/// `SPEEDHACK_LIMIT_BONUS` = 80 (`config.cpp:101`).
const SPEEDHACK_LIMIT_BONUS: u32 = 80;

/// `ani_attack_speed` sin arma = 1000 ms (`ani.cpp:341-351` — item null →
/// 1000; los defaults del ANI también son 1000, `ani.cpp:121-122`).
const ANI_SPEED_BARE_HAND_MS: u32 = 1000;

// ---------------------------------------------------------------------------
// La fórmula (sub-funciones con parity file:line — unit-testables)
// ---------------------------------------------------------------------------

/// El intervalo entre ataques del C++ (`GET_ATTACK_SPEED`, `battle.cpp:757-782`)
/// para el subset base: sin arma → `1000*100 / (80 + 0 + 0)` = **1250 ms**.
/// (Daga/garra → /2, `:774-779`; arma con animación → data-driven .msa,
/// fuera del subset.)
pub fn default_attack_speed() -> u32 {
    (ANI_SPEED_BARE_HAND_MS * 100) / SPEEDHACK_LIMIT_BONUS
}

/// Subtipos de arma (`WEAPON_*` — ani.cpp:37-49: SWORD=0, DAGGER=1, BOW=2,
/// TWO_HANDED=3, BELL=4, FAN=5, ARROW=6, MOUNT_SPEAR=7, CLAW=8, QUIVER=9).
pub mod weapon_subtype {
    pub const DAGGER: i16 = 1;
    pub const CLAW: i16 = 8;
}

/// `GET_ATTACK_SPEED` con el arma EQUIPADA (battle.cpp:757-782): el
/// `ani_speed` base del ANI es 1000 ms (ani.cpp:121 — el constructor; la
/// tabla real `.msa` del pack por raza/arma es GAP documentado) →
/// `real_speed = 1000*100/(80 + 0 + 0)` = 1250 ms; DAGGER y CLAW → /2
/// (battle.cpp:774-779). `weapon = None` → 1250 (manos desnudas).
pub fn attack_speed_for_weapon(weapon: Option<&database::item::ProtoItem>) -> u32 {
    attack_speed_for_weapon_bonus(weapon, 0)
}

/// `GET_ATTACK_SPEED` (battle.cpp:757-782): `(ani_speed × 100) / (
/// SPEEDHACK_LIMIT_BONUS + POINT_ATT_SPEED + riding_bonus)` — el buff
/// ATT_SPEED (POINT_ATT_SPEED) SUMA al denominador y ACELERA el ataque
/// (parity `PointChange(POINT_ATT_SPEED, +x)` de las pociones/skills).
pub fn attack_speed_for_weapon_bonus(
    weapon: Option<&database::item::ProtoItem>,
    att_speed_bonus: i32,
) -> u32 {
    const ANI_SPEED_MS: u32 = 1000; // default del constructor ANI (ani.cpp:121)
    let denom = (SPEEDHACK_LIMIT_BONUS as i32 + att_speed_bonus).max(1) as u32;
    let mut real = (ANI_SPEED_MS * 100) / denom;
    if let Some(w) = weapon
        && w.b_type == 1 /* ITEM_WEAPON (ItemData.h:72) */
            && (w.b_sub_type == weapon_subtype::DAGGER || w.b_sub_type == weapon_subtype::CLAW)
        {
            real /= 2;
        }
    real
}

/// `statAtk` por job (`char.cpp:2064-2087`) — la parte de stats del
/// `POINT_ATT_GRADE`. División ENTERA (truncación C++).
pub fn job_stat_attack(job: u8, st: i32, dx: i32, iq: i32) -> i32 {
    match job {
        job::ASSASSIN => (4 * st + 2 * dx) / 3,
        job::SHAMAN => (4 * st + 2 * iq) / 3,
        // WARRIOR/SURA (+ el default del C++ para job inválido).
        _ => 2 * st,
    }
}

/// `POINT_ATT_GRADE` del atacante PC (`char.cpp:2059-2092`), subset base:
/// `level*2 + statAtk` (sin montura, sin `ATT_GRADE_BONUS`).
pub fn attack_grade(level: i32, job: u8, st: i32, dx: i32, iq: i32) -> i32 {
    level * 2 + job_stat_attack(job, st, dx, iq)
}

/// `POINT_DEF_GRADE` de la víctima MOB (`char.cpp:2156-2158`):
/// `level + ht + wDef` (el mob de mob_proto; para PCs como víctima la fórmula
/// difiere — `char.cpp:2113-2114` — fuera del subset).
pub fn def_grade_npc(level: i32, ht: i32, wdef: i32) -> i32 {
    level + ht + wdef
}

/// `POINT_DEF_GRADE` del PC como VÍCTIMA (`char.cpp:2112-2114`):
/// `iDef = level + (int)(ht / 1.25)` (la división es f64, truncada a int).
/// F5.3 (items): + `iArmor` — la suma de `value1 + 2×value5` de los items
/// ARMOR equipados (char.cpp:2124-2125; el canal la calcula de los
/// `ProtoItem` de los items EQUIPMENT).
pub fn player_def_grade(level: i32, ht: i32, i_armor: i32) -> i32 {
    level + (ht as f64 / 1.25) as i32 + i_armor
}

/// `PK_PROTECT_LEVEL` del runtime spain = 15 (`__LocaleService_Init_spain`,
/// locale_service.cpp:602): los jugadores por debajo entran en
/// `PK_MODE_PROTECT` automático (`SetLevel`, char.cpp:1674-1675; load,
/// :1785-1786) → no atacan ni son atacables (`CanAttack`, pvp.cpp:421-429).
pub const PK_PROTECT_LEVEL: i32 = 15;

/// `PK_MODE_*` (char.h:359-363). Runtime de esta variante = `Peace` (el
/// cliente nunca envía CG_PVP — channel/pvp.rs); los modos habilitan la
/// paridad del gate `battle_is_attackable` (pvp.cpp:467-506).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PkMode {
    #[default]
    Peace,   // PK_MODE_PEACE (char.h:359)
    Revenge, // PK_MODE_REVENGE (:360)
    Free,    // PK_MODE_FREE (:361)
    Protect, // PK_MODE_PROTECT (:362)
    Guild,   // PK_MODE_GUILD (:363)
}

impl PkMode {
    /// El índice `PK_MODE_*` del wire (CG_PVP bMode) → modo; inválido → None.
    pub fn from_u8(b: u8) -> Option<PkMode> {
        Some(match b {
            0 => PkMode::Peace,
            1 => PkMode::Revenge,
            2 => PkMode::Free,
            3 => PkMode::Protect,
            4 => PkMode::Guild,
            _ => return None,
        })
    }
}

/// El contexto PvP de un jugador para el gate `can_attack` (parity
/// `GetPKMode()`/`GetParty()`/`GetGuild()`/`GetAlignment()`/`GetLevel()`/
/// `IsDead()` + el `ATTR_BANPK` del sectree — battle.cpp:91-101). Lo
/// construye el mundo de los componentes del jugador (`Pvp` + `Hp` +
/// `Player`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PvpContext {
    /// PK mode del jugador (CG_PVP 41 — flag de sesión; parity `GetPKMode`).
    pub pk_mode: PkMode,
    /// Party del jugador (None sin party — parity `GetParty()`).
    pub party_id: Option<u32>,
    /// Guild del jugador (None sin guild — parity `GetGuild()`).
    /// GAP (2026-08-27): el mundo aún no la sincroniza (el lane de guildas
    /// mandará un `SetGuild` — hoy el check queda inerte hasta entonces).
    pub guild_id: Option<u32>,
    /// Alineación (parity `GetAlignment` — char.h:1360, −200000..200000).
    /// La usa el modo REVENGE. GAP (2026-08-28): el lane de persistencia
    /// la alimentará — hoy 0 (neutral).
    pub alignment: i32,
    /// HP actual (el muerto no ataca ni es atacable — parity `IsDead()`).
    pub hp: i32,
    /// Nivel del jugador (parity `GetLevel()` — la protección PK).
    pub level: i32,
    /// El jugador está en una celda `ATTR_BANPK` del mapa (parity
    /// `sectree->IsAttr(..., ATTR_BANPK)`, battle.cpp:91-101). GAP
    /// (2026-08-27): el mundo no tiene el grid del mapa — false hasta que
    /// el lane de mapas lo alimente.
    pub safe_zone: bool,
}

/// `battle_is_attackable` (battle.cpp:107-139 + `CPVPManager::CanAttack`,
/// pvp.cpp:373-507) — el núcleo `is_attackable` del PvP. El C++ es
/// ATACANTE-céntrico: solo el modo del atacante importa (:467-506).
/// 1. Muerto (atacante o víctima) → false (battle.cpp:116-118).
/// 2. Misma party → false (pvp.cpp:446-450 — "any pvp model").
/// 3. PROTECT (atacante o víctima) → false (:424-428 — el C++ protege solo
///    en imperio PROPIO; sin imperios en el subset, conservador).
/// 4. GAP `IsKillerMode` de la víctima (:453): el killer-STATE no existe en
///    el realm — el modo no-PEACE de la víctima lo aproxima (en el C++ un
///    FREE/GUILD que ataca recibe el flag :491-504 → atacable por
///    cualquiera).
/// 5. Switch del modo del ATACANTE:
///    - PEACE → false (:469-471 — el duelo de :509-518 no existe).
///    - REVENGE → true SOLO con alineaciones opuestas (:475-484: asesino
///      <0 vs inocente ≥0, o inocente ≥0 vs asesino <0); misma guild → false
///      (:472-474).
///    - GUILD → true salvo misma guild (:489-497 — sin guild → ataca).
///    - FREE → true SIEMPRE, incluso la guild propia (:500-505).
pub fn battle_is_attackable(attacker: &PvpContext, victim: &PvpContext) -> bool {
    if attacker.hp <= 0 || victim.hp <= 0 {
        return false;
    }
    if attacker.party_id.is_some() && attacker.party_id == victim.party_id {
        return false;
    }
    if attacker.pk_mode == PkMode::Protect || victim.pk_mode == PkMode::Protect {
        return false;
    }
    if victim.pk_mode != PkMode::Peace {
        return true; // proxy del killer-STATE (GAP 4)
    }
    let same_guild = attacker.guild_id.is_some() && attacker.guild_id == victim.guild_id;
    match attacker.pk_mode {
        PkMode::Peace => false,
        PkMode::Revenge => {
            !same_guild
                && ((attacker.alignment < 0 && victim.alignment >= 0)
                    || (attacker.alignment >= 0 && victim.alignment < 0))
        }
        PkMode::Free => true,
        PkMode::Guild => !same_guild,
        PkMode::Protect => false,
    }
}

/// `can_attack` — el gate PvP COMPLETO PC→PC: zona segura, nivel
/// (`PK_PROTECT_LEVEL`) y `battle_is_attackable` (que incluye los checks
/// de guild POR MODO — pvp.cpp:472-497; FREE ataca la guild propia,
/// :500). Orden del C++ (`battle_is_attackable` battle.cpp:83-125 +
/// `CanAttack` pvp.cpp:373-522; el gate se evalúa antes del cooldown,
/// char_battle.cpp:205-210):
/// 1. ATTR_BANPK (battle.cpp:91-101) — atacante o víctima en zona segura.
/// 2. `PK_PROTECT_LEVEL` (pvp.cpp:421-429 — PROTECT auto por nivel,
///    char.cpp:1674/1785 — el subset aplica el nivel directamente).
/// 3. `battle_is_attackable` — muerto/misma party → false; el switch de
///    modos decide (PEACE/REVENGE/GUILD/FREE/PROTECT).
pub fn can_attack(attacker: &PvpContext, victim: &PvpContext) -> bool {
    if attacker.safe_zone || victim.safe_zone {
        return false;
    }
    if attacker.level < PK_PROTECT_LEVEL || victim.level < PK_PROTECT_LEVEL {
        return false;
    }
    battle_is_attackable(attacker, victim)
}

/// `CalcAttackRating` (`battle.cpp:227-251`) en f32. OJO: `(iERSrc*2 + 5) /
/// (iERSrc + 95)` es división ENTERA en el C++ (los dos operandos son int).
pub fn calc_attack_rating(attacker_dx: i32, attacker_lv: i32, victim_dx: i32, victim_lv: i32) -> f32 {
    let ar_src = (attacker_dx * 4 + attacker_lv * 2) / 6;
    let ar_src = ar_src.min(90);
    let er_src = (victim_dx * 4 + victim_lv * 2) / 6;
    let er_src = er_src.min(90);
    let f_ar = (ar_src as f32 + 210.0) / 300.0;
    let f_er = ((er_src * 2 + 5) / (er_src + 95)) as f32 * 3.0 / 10.0;
    f_ar - f_er
}

/// `DISTANCE_APPROX` (`utils.h:19-43`): `(246*max + 102*min) >> 8` con los
/// shifts EXACTOS del C++ (misma aritmética i32).
pub fn distance_approx(dx: i32, dy: i32) -> i32 {
    let dx = dx.abs();
    let dy = dy.abs();
    let (min, max) = if dx < dy { (dx, dy) } else { (dy, dx) };
    ((max << 8) + (max << 3) - (max << 4) - (max << 1)
        + (min << 7) - (min << 5) + (min << 3) - (min << 1))
        >> 8
}

/// El rango máximo del ataque melee del PC (`battle.cpp:144-167`): el PC
/// ataca a 300 UNITS; si la víctima es un mob MELEE con más alcance, se usa
/// el suyo (`MAX(300, (int)(wAttackRange * 1.15f))` — `:156-158`).
pub fn melee_max_range(victim: &NpcState) -> i32 {
    let mut max = MELEE_RANGE_UNITS;
    if victim.battle_type == BATTLE_TYPE_MELEE {
        max = max.max((victim.attack_range as f32 * 1.15) as i32);
    }
    max
}

/// `GetMobAttackRange()` del mob (char.cpp:2010-2020): RANGE/MAGIC suman
/// POINT_BOW_DISTANCE (300 default); el resto usa wAttackRange. Base SIN el
/// ×1.15 del ataque (C31) y SIN el floor del PC.
pub fn mob_attack_range_base(mob: &NpcState) -> i32 {
    if matches!(mob.battle_type, BATTLE_TYPE_RANGE | BATTLE_TYPE_MAGIC) {
        mob.attack_range as i32 + BOW_DISTANCE_DEFAULT
    } else {
        mob.attack_range as i32
    }
}

/// El rango máximo del ataque del MOB contra el PC (`battle.cpp:147-152`):
/// un atacante NO-PC usa SOLO su `GetMobAttackRange() * 1.15f` — SIN el
/// floor de 300 del PC. C31: el rewrite usaba `melee_max_range` (con floor
/// 300) → los mobs con `attack_range < 261` atacaban desde 300 en vez de su
/// rango real (mob 101 con range 175 → golpea desde ~201), y los mobs
/// RANGE/MAGIC atacaban a 300 en vez de `(wAttackRange + BOW_DISTANCE)*1.15`
/// (char.cpp:2010-2020 — GetMobAttackRange añade POINT_BOW_DISTANCE).
pub fn mob_attack_max_range(mob: &NpcState) -> i32 {
    (mob_attack_range_base(mob) as f32 * 1.15) as i32
}

/// El `iAtk` del melee — la parte de ATAQUE de `melee_damage` SIN la DEF
/// del objetivo (parity `CalcMeleeDamage` con `bIgnoreDefense=true`,
/// battle.cpp:74-183 — el `atk` que las fórmulas de skill consumen; incluye
/// el `att_grade_bonus` de los buffs, parity `POINT_ATT_GRADE` total).
pub fn attack_power(
    attacker: &PlayerState,
    victim_dx: i32,
    victim_level: i32,
    weapon: Option<&database::item::ProtoItem>,
    roll: &mut dyn FnMut(i32, i32) -> i32,
) -> i32 {
    // Sin arma → 0..1 (Item_GetDamage con item null, battle.cpp:442-462 +
    // 521-526 + :533). Con arma → value3/value4 (battle.cpp:460-461).
    let i_dam = match weapon {
        Some(w) => roll(w.values[3], w.values[4]) * 2,
        None => roll(0, 1) * 2,
    };
    let f_ar = calc_attack_rating(attacker.dx, attacker.level, victim_dx, victim_level);

    // iAtk = (ATT_GRADE + iDam - lv*2) * fAR + lv*2 (battle.cpp:542-544) —
    // el ATT_GRADE total incluye el bonus de los buffs.
    let att_grade = attack_grade(attacker.level, attacker.job, attacker.st, attacker.dx, attacker.iq)
        + attacker.att_grade_bonus;
    let mut i_atk = ((att_grade + i_dam - attacker.level * 2) as f32 * f_ar) as i32;
    i_atk += attacker.level * 2;
    // Con arma: + arma.Value(5)*2 (battle.cpp:546-553).
    if let Some(w) = weapon {
        i_atk += w.values[5] * 2;
    }
    i_atk
}

/// El daño melee del subset base (`CalcMeleeDamage` + `CalcBattleDamage` +
/// `battle_hit` — ver la cabecera del módulo para el desglose con file:line).
///
/// F5.3 (items): `weapon` = el `ProtoItem` del arma equipada (`WEAR_WEAPON`).
/// Con arma: `iDamMin/Max = value3/value4` (`Item_GetDamage`,
/// battle.cpp:460-461) → `iDam = number(min,max) × 2` (battle.cpp:533) y
/// `iAtk += value5 × 2` (battle.cpp:548). Sin arma: `roll(0,1) × 2` y sin
/// bonus (subset original).
///
/// `roll(min, max)` = el `number(min, max)` del C++ (inclusive; `number(0,1)`
/// del arma y el floor `number(1,5)` de `CalcBattleDamage`). El canal provee
/// uno con su RNG; los tests uno fijo (determinismo byte-exacto).
/// Daño melee + si hubo CRÍTICO (parity `battle_melee_attack` +
/// `CalcBattleDamage` — battle.cpp:199-206 + el flag en char_battle.cpp:
/// 2117-2120: `IsCritical → damageFlag |= DAMAGE_CRITICAL`). Devuelve
/// `(daño, crítico)` — el canal pone el flag del wire.
pub fn melee_damage(
    attacker: &PlayerState,
    victim: &NpcState,
    weapon: Option<&database::item::ProtoItem>,
    roll: &mut dyn FnMut(i32, i32) -> i32,
) -> (i32, bool) {
    let i_atk = attack_power(attacker, victim.dx, victim.level, weapon, roll);

    // iDef = DEF_GRADE * (100 + DEF_BONUS)/100 = DEF_GRADE (battle.cpp:564).
    let i_def = def_grade_npc(victim.level, victim.ht, victim.wdef);

    let mut i_dam = (i_atk - i_def).max(0);
    // CalcBattleDamage (battle.cpp:199-206): floor aleatorio 1..5 si < 3
    // (el CALCULATE_DAMAGE_LVDELTA está comentado — :204).
    if i_dam < 3 {
        i_dam = roll(1, 5);
    }
    // CRÍTICO (parity char_battle.cpp:1661-1675): `number(1,100) ≤ pct`
    // → `dam *= 2`. El pct ya viene PROCESADO (5+(pct-10)/4 o pct/2) por
    // `Affects::critical_pct`; el RESIST_CRITICAL del objetivo es 0 en el
    // subset de mobs (no tienen la resistencia en el rewrite).
    let mut critical = false;
    if attacker.critical_pct > 0 && roll(1, 100) <= attacker.critical_pct {
        i_dam *= 2;
        critical = true;
    }
    // battle_hit (:736-749): CalcDamBonus identidad (sin arma, :265-303);
    // attMul = 1.0 (char.cpp:411) → iDam = 1.0*iDam + 0.5 → trunc = iDam.
    (i_dam, critical)
}

// ---------------------------------------------------------------------------
// El core: handle_attack
// ---------------------------------------------------------------------------

/// Procesa un `CG_ATTACK` (server-authoritative) — parity del flujo
/// `CHARACTER::Attack` (`char_battle.cpp:193-284`) + `battle_melee_attack`
/// (`battle.cpp:127-184`) + `battle_hit` (`:731-755`).
///
/// # Firma para el integrador (canal)
///
/// El canal (por conexión de jugador) guarda:
/// ```ignore
/// let mut combat = game_core::combat::CombatState::new();
/// // al recibir CG_ATTACK (header 2, 8 B):
/// let atk = protocol::combat::CgAttack::from_bytes(&pkt)?;       // ya parseado
/// let player = game_core::combat::PlayerState::from_row(&row, &motion); // ataque_speed default
/// let target = <npc lane: lookup por atk.victim_vid>;             // Option<&NpcState>
/// let weapon = <arma equipada: Option<&ProtoItem>>;               // F5.3 items
/// let result = game_core::combat::handle_attack(&mut combat, &atk, &player,
///                                           target, weapon, now_ms(), &mut roll);
/// for pkt in result.packets { conn.send(&pkt).await?; }
/// // result.damage → aplicar HP al mundo (lane NPCs, F5).
/// ```
/// `now_ms` = reloj del server en ms (el `now_ms()` u64 del canal; el C++
/// usa `get_dword_time` DWORD con wrap — u64 monotónico no envuelve, el
/// sáturating_sub del cooldown es equivalente para deltas sanos).
/// `roll` = el `number()` del C++ (inclusive [min, max]) — el canal pasa
/// `&mut |lo, hi| lo + (rand32() % (hi - lo + 1)) as i32` o similar.
///
/// # Comportamiento (orden del C++)
///
/// 1. `bType > 0` (skill) → resultado vacío: `ComputeSkill` es el lane de
///    skills (F5.x) — el canal NO debe enrutar skills aquí.
/// 2. `target: None` (no hay NPC en el mundo) → resultado vacío.
/// 3. Cooldown (`IS_SPEED_HACK`, battle.cpp:808-838): mismo objetivo y
///    `now - last < attack_speed` → rechazo; el timer se actualiza IGUAL
///    (parity `:833` — un rechazo también reinicia el intervalo).
/// 4. Rango (`battle.cpp:144-167`): `distance_approx > melee_max_range` →
///    sin golpe (BATTLE_NONE). El timer YA se actualizó (parity: el C++ llama
///    IS_SPEED_HACK antes de battle_melee_attack).
/// 5. Daño (`melee_damage`): golpe sin daño → solo `GcAttack` (el C++
///    devuelve BATTLE_DAMAGE sin SendDamagePacket — `battle.cpp:738-739`);
///    con daño → `[GcAttack, GcDamageInfo(NORMAL)]` (el flag del golpe normal,
///    `char_battle.cpp:2105-2117`).
///
/// LoS: sin obstáculos por ahora (SDB vacío — la walkability del mapa decide;
/// no hay mapa de colisión en el realm).
pub fn handle_attack(
    combat: &mut CombatState,
    attack: &CgAttack,
    attacker: &PlayerState,
    target: Option<&NpcState>,
    weapon: Option<&database::item::ProtoItem>,
    now_ms: u64,
    roll: &mut dyn FnMut(i32, i32) -> i32,
) -> CombatResult {
    // (1) Skills — ComputeSkill (char_battle.cpp:255-269): fuera del subset.
    if attack.b_type != CgAttack::TYPE_NORMAL {
        return CombatResult::empty();
    }
    // (2) Sin objetivo en el mundo.
    let Some(target) = target else {
        return CombatResult::empty();
    };

    // (3) Cooldown (IS_SPEED_HACK — battle.cpp:808-838). Solo contra el MISMO
    // objetivo; el rechazo actualiza el timer (parity battle.cpp:833-835).
    let speed = u64::from(attacker.attack_speed_ms);
    let delta = now_ms.saturating_sub(combat.last_attack_time);
    if combat.last_attack_vid == target.vid && delta < speed {
        combat.last_attack_time = now_ms;
        return CombatResult::empty();
    }
    combat.last_attack_time = now_ms;
    combat.last_attack_vid = target.vid;

    // (4) Rango (battle.cpp:144-167).
    let distance = distance_approx(attacker.x - target.x, attacker.y - target.y);
    if distance > melee_max_range(target) {
        return CombatResult::empty();
    }

    // (5) Daño (battle.cpp:731-755).
    let (damage, critical) = melee_damage(attacker, target, weapon, roll);
    // FIX 2026-08-14 (el cliente se cerraba al dañar a un mob): el `GcAttack`
    // (header 12) NO se manda — el cliente S3llMetin2 v24 NO tiene case para
    // HEADER_GC_ATTACK en su dispatch (PythonNetworkStreamPhaseGame.cpp —
    // solo HEADER_GC_DAMAGE_INFO 135) y cierra la conexión al recibirlo
    // (CheckPacket). El C++ tampoco lo manda (SendDamagePacket,
    // char_battle.cpp:1508 — solo TPacketGCDamageInfo). El golpe =
    // [GcDamageInfo] (+ la animación FUNC_ATTACK via GC_MOVE la difunde el
    // canal para los MOBS).
    let mut packets = Vec::new();
    if damage > 0 {
        // Flag del wire: NORMAL + CRITICAL si el golpe fue crítico (parity
        // `damageFlag |= DAMAGE_CRITICAL`, char_battle.cpp:2117-2120).
        let flag = if critical {
            damage_flag::NORMAL | damage_flag::CRITICAL
        } else {
            damage_flag::NORMAL
        };
        packets.push(
            GcDamageInfo::new(target.vid, flag, damage).to_bytes().to_vec(),
        );
    }
    CombatResult { packets, damage }
}

// ---------------------------------------------------------------------------
// F5.3: recompensa del kill (función pura — el canal solo la invoca)
// ---------------------------------------------------------------------------

/// Recompensa del kill de un mob (parity del reward del C++: exp del mob ×
/// rate; gold = `number(gold_min, gold_max)` × rate — el sorteo del gold usa
/// el mismo `number(min,max)` inclusive del C++, inyectado como `roll` para
/// determinismo en tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KillReward {
    pub exp_gain: i64,
    pub gold_gain: i64,
}

/// `kill_reward` — recompensa pura del kill. El `roll` es el `number()`
/// inclusive del C++: el canal pasa `&mut |lo, hi| lo + (rand32() % (hi - lo
/// + 1))`; los tests uno fijo (determinismo).
pub fn kill_reward(
    mob_exp: i64,
    gold_min: i32,
    gold_max: i32,
    exp_rate: u16,
    gold_rate: u16,
    roll: &mut dyn FnMut(i32, i32) -> i32,
) -> KillReward {
    let exp_gain = mob_exp.saturating_mul(i64::from(exp_rate)) / 100;
    let span = gold_max.saturating_sub(gold_min).max(0);
    // number(gold_min, gold_max) inclusive; min==max → el propio min.
    let gold_roll = if span > 0 { gold_min + roll(0, span) } else { gold_min };
    let gold_gain = i64::from(gold_roll).saturating_mul(i64::from(gold_rate)) / 100;
    KillReward { exp_gain, gold_gain }
}

/// `aiPercentByDeltaLev_euckr` (constants.cpp:235-266) — el factor de exp
/// por diferencia de nivel mob↔jugador (locale_service.cpp:1208 asigna esta
/// tabla para el runtime; índice = clamp(0, (mob_level + 15) - player_level,
/// 30) — `NEW_GET_LVDELTA`, char_battle.cpp:2210). Matar mobs mucho más
/// débiles da 1% (en vez de exp llena — C33); el mismo nivel da 100%;
/// mobs 15+ niveles superiores dan 170%.
const AI_PERCENT_BY_DELTA_LEV: [i32; 31] = [
    1, 5, 10, 20, 30, 50, 70, 80, 85, 90, 92, 94, 96, 98, 100, 100, 105,
    110, 115, 120, 125, 130, 135, 140, 145, 150, 155, 160, 165, 170, 170,
];

/// El factor de exp por level-delta (`NEW_GET_LVDELTA`, char_battle.cpp:2210).
pub fn exp_level_delta_factor(player_level: i32, mob_level: i32) -> i32 {
    let idx = ((mob_level + 15) - player_level).clamp(0, 30) as usize;
    AI_PERCENT_BY_DELTA_LEV[idx]
}

/// Exp de kill con el factor por level-delta aplicado (parity `GiveExp`:
/// `iExp *= NEW_GET_LVDELTA(...)/100` — char_battle.cpp:2219-2221). El canal
/// usa ESTA función en `apply_kill` (session.rs) — verifier en tests.
pub fn apply_exp_delta(exp_gain: i64, player_level: i32, mob_level: i32) -> i64 {
    exp_gain.saturating_mul(i64::from(exp_level_delta_factor(player_level, mob_level))) / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    /// kill_reward: rates 100 → exp del mob tal cual; gold = roll(min,max).
    #[test]
    fn reward_base_rates() {
        let mut roll = |lo: i32, _hi: i32| lo; // el mínimo del rango
        let r = kill_reward(22, 15, 45, 100, 100, &mut roll);
        assert_eq!(r.exp_gain, 22);
        assert_eq!(r.gold_gain, 15, "roll = min → gold_min");
    }

    /// C33: el factor de exp por level-delta (parity `aiPercentByDeltaLev`,
    /// constants.cpp:235-266 + NEW_GET_LVDELTA char_battle.cpp:2210):
    /// mismo nivel → 100%; mob 15+ niveles por encima → 170%; mob mucho más
    /// débil (8+ niveles abajo) → 1%.
    #[test]
    fn exp_level_delta_matches_cpp_table() {
        // Índice = (mob + 15) − player; mob 15 niveles ARRIBA del player →
        // (mob+15)−player = 15 → 100% (el mismo nivel).
        assert_eq!(exp_level_delta_factor(40, 40), 100, "mismo nivel");
        assert_eq!(exp_level_delta_factor(30, 45), 170, "mob 15+ niveles arriba → 170 (índice 30)");
        assert_eq!(exp_level_delta_factor(50, 50), 100, "mismo nivel alto");
        // Player 8 niveles por encima del mob → (mob+15)−player = 7 → 80.
        assert_eq!(exp_level_delta_factor(40, 32), 80, "índice 7 = 80 (constants.cpp:242)");
        // Player 15+ niveles por encima → (mob+15)−player ≤ 0 → 1%.
        assert_eq!(exp_level_delta_factor(40, 25), 1, "índice 0 = 1% — el mob es basura");
        assert_eq!(exp_level_delta_factor(40, 20), 1, "clamp inferior");
        // Mob muy superior (índice > 30) → clamp a 170.
        assert_eq!(exp_level_delta_factor(20, 50), 170, "clamp superior (índice 30)");
    }

    /// Verifier (regla 20): `apply_exp_delta` es el camino EXACTO que usa
    /// `apply_kill` (session.rs:520) — si el canal vuelve a dar exp llena
    /// con delta grande (mutation: quitar el factor), este test falla.
    #[test]
    fn exp_delta_low_mob_never_full() {
        // player 50 vs mob 30 → (30+15)−50 = −5 → clamp índice 0 → 1%.
        assert_eq!(apply_exp_delta(1000, 50, 30), 10, "1000 exp × 1%");
        assert!(apply_exp_delta(1000, 50, 30) < 1000, "delta grande → < 100%");
        // Contraste: mismo nivel → 100% sin castigo; mob superior → bono.
        assert_eq!(apply_exp_delta(1000, 50, 50), 1000);
        assert_eq!(apply_exp_delta(1000, 50, 65), 1700, "índice 30 = 170%");
    }

    /// Rates aplicados: exp_rate 150 → 22*1.5 = 33; gold_rate 200 → 30*2 = 60.
    #[test]
    fn reward_applies_rates() {
        let mut roll = |_lo: i32, hi: i32| hi; // el máximo del rango
        let r = kill_reward(22, 15, 45, 150, 200, &mut roll);
        assert_eq!(r.exp_gain, 33, "22*150/100");
        assert_eq!(r.gold_gain, 90, "45*200/100");
    }

    /// gold_min == gold_max (mob sin rango) → gold fijo sin roll.
    #[test]
    fn reward_fixed_gold() {
        let mut roll = |_lo: i32, _hi: i32| panic!("no debe sortear");
        let r = kill_reward(10, 40, 40, 100, 100, &mut roll);
        assert_eq!(r.gold_gain, 40);
    }

    /// mob sin recompensa (exp 0, gold 0) → todo 0 (parity: el C++ no da
    /// nada con exp/gold 0 del mob_proto).
    #[test]
    fn reward_zero_mob() {
        let mut roll = |_lo: i32, _hi: i32| 999;
        let r = kill_reward(0, 0, 0, 100, 100, &mut roll);
        assert_eq!(r, KillReward { exp_gain: 0, gold_gain: 0 });
    }

    /// La fórmula EXACTA del wire (channel.rs usa este módulo — el test
    /// fija el contrato que el canal verifica end-to-end).
    #[test]
    fn reward_matches_channel_contract() {
        let mut roll = |lo: i32, hi: i32| lo + (hi - lo) / 2;
        let r = kill_reward(22, 15, 45, 100, 100, &mut roll);
        assert_eq!(r.gold_gain, 30, "mid(15,45) = 30");
    }

    /// DEF del PC como víctima (char.cpp:2114): `level + (int)(ht / 1.25)`
    /// — la división es f64 truncada. ht=30 → 24 (30/1.25 = 24 exacto);
    /// ht=5 → 4 (5/1.25 = 4); ht=40 → 32. F5.3: + iArmor (los items ARMOR
    /// equipados — char.cpp:2124-2125).
    #[test]
    fn player_def_grade_matches_cpp() {
        assert_eq!(player_def_grade(5, 30, 0), 29, "5 + 24");
        assert_eq!(player_def_grade(1, 5, 0), 5, "1 + 4");
        assert_eq!(player_def_grade(50, 40, 0), 82, "50 + 32");
        assert_eq!(player_def_grade(0, 0, 0), 0);
        // Truncación f64: ht=6 → 6/1.25 = 4.8 → 4.
        assert_eq!(player_def_grade(1, 6, 0), 5, "1 + (int)4.8");
        // Con armadura: se suma el iArmor del equipo (char.cpp:2146).
        assert_eq!(player_def_grade(5, 30, 100), 129, "29 + 100");
        assert_eq!(player_def_grade(1, 5, 20), 25, "5 + 20");
    }

    /// El gate PvP (`battle_is_attackable` — battle.cpp:107-139 +
    /// CPVPManager::CanAttack, pvp.cpp:373-507): muerto → false; misma
    /// party → false; PEACE → false (el duelo no existe); FREE/GUILD →
    /// true; REVENGE → solo alineaciones opuestas; PROTECT → false.
    #[test]
    fn battle_is_attackable_gate_pvp() {
        let live = |mode: PkMode, party: Option<u32>| ctx(mode, party, 100);
        let dead = PvpContext { hp: 0, ..ctx(PkMode::Free, None, 100) };
        // Muerto: ni el atacante ataca ni la víctima es atacable (battle.cpp:116).
        assert!(!battle_is_attackable(&dead, &live(PkMode::Free, None)));
        assert!(!battle_is_attackable(&live(PkMode::Free, None), &dead));
        // Misma party → no (pvp.cpp:446-450 — "any pvp model").
        assert!(!battle_is_attackable(&live(PkMode::Free, Some(7)), &live(PkMode::Peace, Some(7))));
        assert!(battle_is_attackable(&live(PkMode::Free, Some(7)), &live(PkMode::Peace, Some(8))));
        // Víctima no-PEACE → atacable por cualquiera (proxy del killer-
        // STATE — pvp.cpp:453; el killer-flag del C++ se aproxima con el
        // modo no-Peace de la víctima).
        assert!(battle_is_attackable(&live(PkMode::Peace, None), &live(PkMode::Free, None)));
        // PEACE → false (el duelo de :509-518 no existe en el subset).
        assert!(!battle_is_attackable(&live(PkMode::Peace, None), &live(PkMode::Peace, None)));
        // FREE → true (pvp.cpp:500-505).
        assert!(battle_is_attackable(&live(PkMode::Free, None), &live(PkMode::Peace, None)));
        assert!(battle_is_attackable(&live(PkMode::Guild, None), &live(PkMode::Peace, None)));
        // PROTECT → false (pvp.cpp:424-428 — conservador: sin imperios).
        assert!(!battle_is_attackable(&live(PkMode::Protect, None), &live(PkMode::Peace, None)));
        assert!(!battle_is_attackable(&live(PkMode::Peace, None), &live(PkMode::Protect, None)));
    }

    /// Contexto PvP del test (nivel protegido, sin guild, sin zona segura).
    fn ctx(mode: PkMode, party: Option<u32>, hp: i32) -> PvpContext {
        PvpContext {
            pk_mode: mode,
            party_id: party,
            guild_id: None,
            alignment: 0,
            hp,
            level: PK_PROTECT_LEVEL,
            safe_zone: false,
        }
    }

    /// VERIFIER del slice PK modes (2026-08-28, pvp full): `PkMode::Free`
    /// ATACA a la guild propia (pvp.cpp:500 — quedó resuelta la desviación
    /// del flag único) y `PkMode::Guild` la rechaza (:489-497). FALLA si se
    /// vuelve al check ciego de guild de `can_attack` (mutation).
    #[test]
    fn can_attack_free_hits_same_guild_guild_mode_rejects() {
        let free = PvpContext { guild_id: Some(9), ..ctx(PkMode::Free, None, 100) };
        let mate = PvpContext { guild_id: Some(9), ..ctx(PkMode::Peace, None, 100) };
        let other = PvpContext { guild_id: Some(10), ..ctx(PkMode::Peace, None, 100) };
        assert!(can_attack(&free, &mate), "FREE ataca la guild propia (pvp.cpp:500)");
        let guild = PvpContext { guild_id: Some(9), ..ctx(PkMode::Guild, None, 100) };
        assert!(!can_attack(&guild, &mate), "GUILD rechaza la guild propia (:489)");
        assert!(can_attack(&guild, &other), "GUILD ataca a otra guild");
        // Sin guild: GUILD ataca a cualquiera (pvp.cpp:489 — !GetGuild()).
        let groupless = ctx(PkMode::Guild, None, 100);
        assert!(can_attack(&groupless, &mate), "sin guild → ataca");
    }

    /// REVENGE (pvp.cpp:475-484): true SOLO con alineaciones opuestas;
    /// ambos ≥ 0 o ambos < 0 → false (cae al duelo → false).
    #[test]
    fn revenge_requires_opposite_alignment() {
        let revenge = |al: i32| PvpContext { alignment: al, ..ctx(PkMode::Revenge, None, 100) };
        let clean = |al: i32| PvpContext { alignment: al, ..ctx(PkMode::Peace, None, 100) };
        assert!(can_attack(&revenge(-1), &clean(0)), "asesino negativo vs inocente");
        assert!(can_attack(&revenge(0), &clean(-1)), "inocente vs asesino negativo");
        assert!(!can_attack(&revenge(-1), &clean(-1)), "ambos negativos (:482-483)");
        assert!(!can_attack(&revenge(1), &clean(1)), "ambos positivos");
    }

    /// VERIFIER del slice (task 2026-08-27): `can_attack` RECHAZA a un
    /// miembro de la MISMA PARTY aunque el atacante tenga PK ON
    /// (pvp.cpp:446-450 — "Cannot attack same party on any pvp model").
    /// FALLA si se permite atacar a un compañero de party (mutation:
    /// quitar el check de party).
    #[test]
    fn can_attack_rejects_same_party_even_with_pk() {
        let attacker = ctx(PkMode::Free, Some(7), 100);
        let mate = ctx(PkMode::Peace, Some(7), 100);
        assert!(!can_attack(&attacker, &mate), "PK ON + misma party → rechazado");
        let stranger = ctx(PkMode::Peace, Some(8), 100);
        assert!(can_attack(&attacker, &stranger), "party distinta → atacable");
    }

    /// Nivel: por debajo de PK_PROTECT_LEVEL (15 — locale spain,
    /// locale_service.cpp:602) no se ataca ni se es atacable (parity
    /// PROTECT auto, char.cpp:1674/1785 + pvp.cpp:421-429).
    #[test]
    fn can_attack_rejects_below_protect_level() {
        let pk = |level: i32| PvpContext { level, ..ctx(PkMode::Free, None, 100) };
        let peace = |level: i32| PvpContext { level, ..ctx(PkMode::Peace, None, 100) };
        assert!(!can_attack(&pk(PK_PROTECT_LEVEL - 1), &peace(PK_PROTECT_LEVEL)));
        assert!(!can_attack(&pk(PK_PROTECT_LEVEL), &peace(PK_PROTECT_LEVEL - 1)));
        assert!(can_attack(&pk(PK_PROTECT_LEVEL), &peace(PK_PROTECT_LEVEL)));
    }

    /// Zona segura (ATTR_BANPK — battle.cpp:91-101): atacante o víctima en
    /// zona segura → nunca atacable, ni con PK ON.
    #[test]
    fn can_attack_rejects_safe_zone() {
        let in_zone = PvpContext { safe_zone: true, ..ctx(PkMode::Free, None, 100) };
        let outside = ctx(PkMode::Peace, None, 100);
        assert!(!can_attack(&in_zone, &outside), "atacante en zona segura");
        assert!(!can_attack(&outside, &in_zone), "víctima en zona segura");
    }

    /// Atacante del harness E2E (channel.rs `dummy_row` — ninja: job 1
    /// ASSASSIN, lvl 5, st/dx/ht/iq = 30, sin arma) contra el mob 101 del
    /// mob_proto (`tools/proto/mob_proto.txt:2` — lvl 1, dx 6, ht 5, DEF 4,
    /// MELEE, rango 175).
    fn ninja() -> PlayerState {
        PlayerState {
            vid: 2,
            x: 969600,
            y: 278400,
            level: 5,
            job: job::ASSASSIN,
            st: 30,
            ht: 30,
            dx: 30,
            iq: 30,
            attack_speed_ms: default_attack_speed(),
            att_grade_bonus: 0,
            critical_pct: 0,
        }
    }

    fn mob101() -> NpcState {
        NpcState {
            vid: 101,
            x: 969600,
            y: 278400,
            level: 1,
            dx: 6,
            ht: 5,
            wdef: 4,
            battle_type: BATTLE_TYPE_MELEE,
            attack_range: 175,
        }
    }

    fn roll_fixed(v: i32) -> impl FnMut(i32, i32) -> i32 {
        move |_min, _max| v
    }

    /// La fórmula EXACTA contra un vector calculado a mano del C++ (ver
    /// reporte del lane — cada paso con su file:line en la cabecera):
    /// ATT_GRADE = 70, DEF_GRADE(mob) = 10, fAR = 0.77; roll(0,1)=0 →
    /// iAtk = (int)(60*0.77)+10 = 56 → 56-10 = 46; roll=1 (iDam=2) →
    /// iAtk = (int)(62*0.77)+10 = 57 → 57-10 = 47.
    #[test]
    fn formula_matches_cpp_vector() {
        let a = ninja();
        let m = mob101();
        assert_eq!(attack_grade(a.level, a.job, a.st, a.dx, a.iq), 70, "5*2 + (120+60)/3");
        assert_eq!(def_grade_npc(m.level, m.ht, m.wdef), 10, "1+5+4");
        assert_eq!(job_stat_attack(job::ASSASSIN, 30, 30, 30), 60);
        // roll(0,1) = 0 → iDam = 0 → iAtk = (70+0-10)*0.77+10 = 56 → dam 46.
        let mut roll = roll_fixed(0);
        assert_eq!(melee_damage(&a, &m, None, &mut roll).0, 46);
        // roll(0,1) = 1 → iDam = 2 → iAtk = (62)*0.77+10 = 57 → dam 47.
        let mut roll = roll_fixed(1);
        assert_eq!(melee_damage(&a, &m, None, &mut roll).0, 47);
    }

    /// La división ENTERA del C++ se respeta: assassin st=1, dx=2 →
    /// (4+4)/3 = 2 (no 2.66); shaman (4*st + 2*iq)/3; warrior/sura = 2*st.
    #[test]
    fn job_stat_attack_int_division() {
        assert_eq!(job_stat_attack(job::WARRIOR, 10, 0, 0), 20);
        assert_eq!(job_stat_attack(job::SURA, 10, 0, 0), 20);
        assert_eq!(job_stat_attack(job::ASSASSIN, 1, 2, 0), 2, "(4+4)/3 truncado");
        assert_eq!(job_stat_attack(job::ASSASSIN, 30, 30, 0), 60);
        assert_eq!(job_stat_attack(job::SHAMAN, 1, 0, 2), 2, "(4+4)/3 truncado");
        assert_eq!(job_stat_attack(job::SHAMAN, 30, 0, 30), 60);
    }

    /// El floor de CalcBattleDamage (battle.cpp:199-206): daño < 3 →
    /// `number(1, 5)` (el CALCULATE_DAMAGE_LVDELTA está comentado).
    /// Caso: lvl 1 warrior st=4 dx=4 → ATT_GRADE = 10, fAR = 0.71 (iAR=3,
    /// iER=4), iAtk = (int)(8*0.71)+2 = 7 < iDef 10 → 0 → floor roll(1,5).
    #[test]
    fn calc_battle_damage_floor() {
        let a = PlayerState {
            vid: 9,
            x: 0,
            y: 0,
            level: 1,
            job: job::WARRIOR,
            st: 4,
            ht: 4,
            dx: 4,
            iq: 4,
            attack_speed_ms: default_attack_speed(),
            att_grade_bonus: 0,
            critical_pct: 0,
        };
        let m = mob101();
        let mut roll = roll_fixed(3);
        assert_eq!(melee_damage(&a, &m, None, &mut roll).0, 3, "0 < 3 → number(1,5) = 3");
        // El cálculo previo al floor: 7 - 10 = max(0, -3) = 0.
        let f_ar = calc_attack_rating(a.dx, a.level, m.dx, m.level);
        let att = attack_grade(a.level, a.job, a.st, a.dx, a.iq);
        assert_eq!(((att - a.level * 2) as f32 * f_ar) as i32 + a.level * 2, 7);
        assert_eq!(f_ar, 0.71, "(3+210)/300 - 0");
    }

    /// CRÍTICO (parity char_battle.cpp:1661-1675): `number(1,100) ≤ pct`
    /// → `dam *= 2`. El pct ya viene procesado por `Affects::critical_pct`
    /// (5+(pct-10)/4 si ≥10, pct/2 si no). roll_fixed(5) → number(1,100)
    /// = 5 ≤ 100 → crítico SIEMPRE con pct alto.
    #[test]
    fn critical_doubles_damage_when_roll_hits() {
        let mut a = ninja();
        a.critical_pct = 100; // el roll number(1,100) ≤ 100 siempre
        let m = mob101();
        let mut roll = roll_fixed(5);
        let normal = melee_damage(&PlayerState { critical_pct: 0, ..a }, &m, None, &mut roll).0;
        let mut roll = roll_fixed(5);
        let crit = melee_damage(&a, &m, None, &mut roll).0;
        assert_eq!(crit, normal * 2, "crítico ×2: {normal} → {crit}");
        // roll_fixed(5) → el roll(1,100)=5; el roll del floor (1,5)=5.
        assert!(normal > 0, "daño base > 0");
    }

    /// `Affects::critical_pct`: la fórmula del C++ (char_battle.cpp:
    /// 1665-1668) — pct ≥ 10 → 5+(pct-10)/4; pct < 10 → pct/2.
    #[test]
    fn critical_pct_formula_matches_cpp() {
        use crate::ecs::components::{Affect, Affects};
        let aff = |v: i32| Affect {
            skill_id: 19,
            point: crate::skill::point::CRITICAL_PCT,
            value: v,
            flag: 0,
            duration_ms: 96_000,
            sp_cost: 0,
        };
        let a = Affects(vec![aff(30)]);
        assert_eq!(a.critical_pct(), 5 + (30 - 10) / 4, "30 → 5+20/4 = 10");
        let a = Affects(vec![aff(8)]);
        assert_eq!(a.critical_pct(), 4, "8 → 8/2 = 4");
        let a = Affects(vec![aff(10)]);
        assert_eq!(a.critical_pct(), 5, "10 → 5+0/4 = 5");
        let a = Affects(vec![]);
        assert_eq!(a.critical_pct(), 0, "sin buff → 0");
        // Suma de múltiples buffs.
        let a = Affects(vec![aff(10), aff(20)]);
        assert_eq!(a.critical_pct(), 5 + (30 - 10) / 4, "10+20 = 30 → 10");
    }

    /// Cooldown (IS_SPEED_HACK parity, battle.cpp:808-838): intervalo base
    /// 1250 ms; ataque al MISMO objetivo dentro del intervalo → rechazado; el
    /// rechazo reinicia el timer (un ataque a t=2000 rechazado desplaza la
    /// ventana); un objetivo distinto NO dispara el cooldown (parity — el
    /// check es `dwVID == victim->GetVID()`).
    #[test]
    fn cooldown_rejects_within_interval() {
        assert_eq!(default_attack_speed(), 1250, "(1000*100)/(80+0+0)");
        let a = ninja();
        let m = mob101();
        let mut combat = CombatState::new();

        // t=1000: primer ataque → golpe (timer anclado).
        let mut roll = roll_fixed(0);
        let r = handle_attack(&mut combat, &atk(m.vid), &a, Some(&m), None, 1000, &mut roll);
        assert_eq!(r.damage, 46);
        assert_eq!(combat.last_attack_time, 1000);
        assert_eq!(combat.last_attack_vid, m.vid);

        // t=2000: mismo objetivo, delta 1000 < 1250 → rechazo (vacío), pero el
        // timer se actualiza a 2000 (parity battle.cpp:833).
        let r = handle_attack(&mut combat, &atk(m.vid), &a, Some(&m), None, 2000, &mut roll);
        assert_eq!(r, CombatResult::empty());
        assert_eq!(combat.last_attack_time, 2000);

        // t=3250: delta 1250 ≥ 1250 → golpe de nuevo.
        let r = handle_attack(&mut combat, &atk(m.vid), &a, Some(&m), None, 3250, &mut roll);
        assert_eq!(r.damage, 46);

        // Objetivo distinto dentro del intervalo → SIN cooldown (parity).
        let m2 = NpcState { vid: 102, ..m };
        let mut combat2 = CombatState::new();
        handle_attack(&mut combat2, &atk(m.vid), &a, Some(&m), None, 1000, &mut roll);
        let r = handle_attack(&mut combat2, &atk(m2.vid), &a, Some(&m2), None, 2000, &mut roll);
        assert_eq!(r.damage, 46, "target distinto: sin rechazo");
    }

    /// `attack_speed_for_weapon` (F5.3, battle.cpp:757-782 + ani.cpp:121):
    /// sin arma → 1250 ms (`(1000*100)/(80+0+0)`); arma normal → 1250 (el
    /// ANI default es 1000; la tabla .msa real del pack es GAP); DAGGER y
    /// CLAW → /2 = 625 (battle.cpp:774-779).
    #[test]
    fn attack_speed_for_weapon_matches_cpp() {
        use database::item::ProtoItem;
        assert_eq!(attack_speed_for_weapon(None), 1250, "manos desnudas");
        let sword = ProtoItem {
            b_type: 1, // ITEM_WEAPON
            b_sub_type: 0, // WEAPON_SWORD
            applies: [(0, 0); 3],
            values: [0; 6],
            wear_flag: 1 << 4, // WEARABLE_WEAPON
            weight: 0,
            magic_pct: 0,
            socket_pct: 0,
        };
        assert_eq!(attack_speed_for_weapon(Some(&sword)), 1250, "espada: ANI default 1000");
        let dagger = ProtoItem { b_sub_type: weapon_subtype::DAGGER, ..sword };
        assert_eq!(attack_speed_for_weapon(Some(&dagger)), 625, "daga: /2 (battle.cpp:774-779)");
        let claw = ProtoItem { b_sub_type: weapon_subtype::CLAW, ..sword };
        assert_eq!(attack_speed_for_weapon(Some(&claw)), 625, "garra: /2");
        // Un item NO-weapon (p.ej. ARMOR) equipado en el slot del arma no
        // aplica el /2 (el C++ comprueba GetSubType del arma real).
        let armor = ProtoItem {
            b_type: 2, // ITEM_ARMOR
            b_sub_type: 0,
            applies: [(0, 0); 3],
            values: [0; 6],
            wear_flag: 1 << 0,
            weight: 0,
            magic_pct: 0,
            socket_pct: 0,
        };
        assert_eq!(attack_speed_for_weapon(Some(&armor)), 1250, "no-weapon: sin /2");
    }

    /// El buff ATT_SPEED (POINT_ATT_SPEED) SUMA al denominador de
    /// GET_ATTACK_SPEED (battle.cpp:762): sin arma, bonus 20 →
    /// (1000×100)/(80+20) = 1000 ms (era 1250).
    #[test]
    fn attack_speed_bonus_accelerates() {
        assert_eq!(attack_speed_for_weapon_bonus(None, 0), 1250, "sin buff: (1000×100)/80");
        assert_eq!(attack_speed_for_weapon_bonus(None, 20), 1000, "bonus 20: (1000×100)/(80+20)");
        assert_eq!(attack_speed_for_weapon_bonus(None, 170), 400, "bonus 170: (1000×100)/250");
        // DAGGER con bonus: el /2 se aplica DESPUÉS del denominador.
        use database::item::ProtoItem;
        let dagger = ProtoItem {
            b_type: 1,
            b_sub_type: weapon_subtype::DAGGER,
            applies: [(0, 0); 3],
            values: [0; 6],
            wear_flag: 1 << 4,
            weight: 0,
            magic_pct: 0,
            socket_pct: 0,
        };
        assert_eq!(attack_speed_for_weapon_bonus(Some(&dagger), 20), 500, "daga: 1000/2");
    }

    /// Rango (battle.cpp:144-167): `distance_approx > 300` → sin golpe;
    /// el mob MELEE con más alcance extiende el rango (`MAX(300, r*1.15)`).
    #[test]
    fn range_rejects_far_targets() {
        assert_eq!(distance_approx(0, 400), 384, "(246*400)>>8");
        assert_eq!(distance_approx(200, 200), 271, "(246+102)*200>>8");
        let a = ninja();
        let m = mob101();
        let mut roll = roll_fixed(0);
        let mut combat = CombatState::new();

        // 400 UNITS (dist 384) > 300 → sin golpe. (vid 101 — timer anclado.)
        let far = NpcState { vid: 101, x: a.x + 400, y: a.y, ..m };
        let r = handle_attack(&mut combat, &atk(far.vid), &a, Some(&far), None, 1000, &mut roll);
        assert_eq!(r, CombatResult::empty());

        // 200,200 (dist 271) ≤ 300 → golpe. (vid distinto — el cooldown solo
        // aplica contra el MISMO objetivo, battle.cpp:812.)
        let near = NpcState { vid: 102, x: a.x + 200, y: a.y + 200, ..m };
        let r = handle_attack(&mut combat, &atk(near.vid), &a, Some(&near), None, 2000, &mut roll);
        assert_eq!(r.damage, 46);

        // Mob MELEE grande (rango 1000 → max = MAX(300, 1150)): 500 OK, 1200 no.
        let big = NpcState { vid: 103, x: a.x + 500, y: a.y, battle_type: BATTLE_TYPE_MELEE, attack_range: 1000, ..m };
        assert_eq!(melee_max_range(&big), 1150, "(int)(1000*1.15f)");
        let r = handle_attack(&mut combat, &atk(big.vid), &a, Some(&big), None, 3000, &mut roll);
        assert_eq!(r.damage, 46, "500 ≤ 1150");
        let far_big = NpcState { vid: 104, x: a.x + 1200, y: a.y, ..big };
        let r = handle_attack(&mut combat, &atk(far_big.vid), &a, Some(&far_big), None, 4000, &mut roll);
        assert_eq!(r, CombatResult::empty(), "1200 > 1150");
    }

    /// C31: el rango del ataque del MOB (battle.cpp:147-152) — SIN el floor
    /// 300 del PC: mob 101 (range 175 MELEE) golpea desde ~201, no 300;
    /// RANGE/MAGIC suman POINT_BOW_DISTANCE (300) al rango
    /// (char.cpp:2010-2020 — GetMobAttackRange).
    #[test]
    fn mob_attack_range_uses_own_no_floor() {
        let a = ninja();
        let m = mob101();
        // MELEE con range 175 → (int)(175×1.15) = 201 — NO 300.
        let mob = NpcState { vid: 101, x: a.x + 200, y: a.y, battle_type: BATTLE_TYPE_MELEE, attack_range: 175, ..m };
        assert_eq!(mob_attack_max_range(&mob), 201, "(int)(175×1.15) — sin floor");
        // MELEE con range 1000 → 1150 (igual que el PC→mob, aquí sin MAX).
        let big = NpcState { vid: 102, x: a.x, y: a.y, battle_type: BATTLE_TYPE_MELEE, attack_range: 1000, ..m };
        assert_eq!(mob_attack_max_range(&big), 1150, "(int)(1000×1.15)");
        // RANGE (1) con range 175 → (175 + 300 BOW) × 1.15 = 546.
        let bow = NpcState { vid: 103, x: a.x, y: a.y, battle_type: BATTLE_TYPE_RANGE, attack_range: 175, ..m };
        assert_eq!(mob_attack_max_range(&bow), 546, "(175+300)×1.15 — POINT_BOW_DISTANCE");
        // MAGIC (2) — mismo bonus.
        let mage = NpcState { vid: 104, x: a.x, y: a.y, battle_type: BATTLE_TYPE_MAGIC, attack_range: 175, ..m };
        assert_eq!(mob_attack_max_range(&mage), 546, "MAGIC igual que RANGE");
    }

    /// Sin objetivo (`None` — el mundo aún vacío) → resultado vacío; bType > 0
    /// (skill) → resultado vacío (el lane de skills lo procesa).
    #[test]
    fn no_target_and_skills_are_empty() {
        let a = ninja();
        let mut combat = CombatState::new();
        let mut roll = roll_fixed(0);
        assert_eq!(handle_attack(&mut combat, &atk(101), &a, None, None, 1000, &mut roll), CombatResult::empty());
        assert_eq!(combat.last_attack_time, 0, "sin objetivo: ni timer");
        let m = mob101();
        let mut atk_skill = atk(m.vid);
        atk_skill.b_type = 42;
        assert_eq!(handle_attack(&mut combat, &atk_skill, &a, Some(&m), None, 1000, &mut roll), CombatResult::empty());
    }

    /// Los paquetes del resultado: `[GcDamageInfo(135)]` SOLO (fix
    /// 2026-08-14: GcAttack 12 cerraba el cliente — sin case en su dispatch;
    /// el C++ tampoco lo manda, char_battle.cpp:1508) — con los campos
    /// exactos (VID víctima, flag NORMAL).
    #[test]
    fn result_packets_bytes() {
        let a = ninja();
        let m = mob101();
        let mut combat = CombatState::new();
        let mut roll = roll_fixed(0);
        let r = handle_attack(&mut combat, &atk(m.vid), &a, Some(&m), None, 1000, &mut roll);
        assert_eq!(r.damage, 46);
        assert_eq!(r.packets.len(), 1, "solo GcDamageInfo (fix 2026-08-14 — GcAttack 12 cerraba el cliente)");
        // GC_DAMAGE_INFO: header 135, dwVID=101, flag=NORMAL(1), damage=46.
        assert_eq!(r.packets[0], [135, 101, 0, 0, 0, 1, 46, 0, 0, 0]);
    }

    fn atk(victim_vid: u32) -> CgAttack {
        CgAttack {
            header: protocol::header::CG_ATTACK,
            b_type: 0,
            victim_vid,
            crc_proc: 0,
            crc_file: 0,
        }
    }
}
