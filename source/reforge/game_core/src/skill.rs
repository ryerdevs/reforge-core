//! F5 (skills): el dominio de las skills del jugador — `skill_proto` (PG),
//! el evaluador de POLYS (las fórmulas data-driven del legacy) y la cadena
//! de daño/efecto (parity `char_skill.cpp` / `skill.cpp` / `skill_power.cpp`).
//!
//! # Parity del legacy (file:line verificados 2026-08-13)
//!
//! - **Wire del uso**: `TPacketCGUseSkill` (header 52, 9 B — Packet.h:854:
//!   `bHeader + dwVnum + dwTargetVID`) → `CInputMain::UseSkill`
//!   (input_main.cpp:3050) → `CHARACTER::UseSkill` (char_skill.cpp:2399).
//! - **Cooldown**: `TSkillUseInfo::UseSkill` (char_skill.cpp:94-121): si
//!   `bUsed && dwNextSkillUsableTime > now` → rechazo silencioso; si no,
//!   `dwNextSkillUsableTime = now + cooldown_ms`. El legacy NO valida el
//!   cooldown contra el cliente (`ENABLE_SKILL_COOLDOWN_CHECK` ausente,
//!   char_skill.cpp:107) — el server decide (ADR-0011): el Rust lo aplica
//!   SIEMPRE.
//! - **SP cost**: `kSPCostPoly.Eval()` con vars `v` (SP actual), `maxv`
//!   (SP máx), `maxhp`; `GetSP() < iNeededSP` → rechazo; `PointChange(SP,-)`
//!   (char_skill.cpp:2509-2526). `USE_HP_AS_COST` → el cost se paga con HP
//!   (vars `v`=HP, `maxhp`).
//! - **Skill power `k`**: `k = GetSkillPower(vnum, level) * bMaxLevel / 100`
//!   (char_skill.cpp:1632, skill_power.cpp:37-52). La tabla real
//!   (`skill_power.txt`, por job/skillgroup/nivel) NO está en PG — el subset
//!   usa `k = level * max_level / 100` (desviación documentada, F6 balance).
//! - **El poly**: `kPointPoly.Eval()` con las vars `atk` (CalcMeleeDamage
//!   con bIgnoreDefense=true — battle.cpp:74-183: el ATAQUE sin la DEF del
//!   objetivo), `k`, `lv`, `iq`, `str`, `dex`, `con`, `maxhp`, `maxsp`,
//!   `ar` (CalcAttackRating), `def`/`odef` (DEF con/sin bonus) — y las
//!   funciones `number(a,b)` (int inclusivo) y `floor(x)`.
//! - **Daño final** (FuncSplashDamage::OnHit, char_skill.cpp:1143-1210):
//!   `iAmount = poly` (negativo en la DB) → `iAmount = -iAmount` →
//!   `CalcBattleDamage` (floor: `if (iDam < 3) iDam = number(1,5)`,
//!   battle.cpp:199-206) → ajustes por attr type: MELEE → `-= victim DEF_GRADE`;
//!   RANGE/MAGIC → resistencias (0 para mobs en el subset).
//! - **Buffs**: `AddAffect(skill_vnum, bPointOn, iAmount, dwAffectFlag,
//!   iDur, 0, true)` (char_skill.cpp:2118-2120) — el dwType del wire = el
//!   vnum del skill (el icono del cliente); `bPointIdxApplyOn` = el POINT_*
//!   (FindPointType, skill.cpp:84-96 — "DEF_GRADE" → POINT_DEF_GRADE_BONUS);
//!   `dwFlag` = el AFF_* de `setaffectflag` (EAffectBits, affect.h:140-181);
//!   `lDuration` = segundos del `kDurationPoly`.
//!
//! # Subset documentado
//!
//! - Skills de daño single-target (flags ATTACK + USE_MELEE_DAMAGE),
//!   self-buffs (SELFONLY) y el modo ÁREA (flag SPLASH — `iSplashRange` =
//!   radio, `lMaxHit` = máx víctimas, `kSplashAroundDamageAdjustPoly` =
//!   ajuste del daño de las víctimas alrededor — parity `ComputeSkill` +
//!   `FuncSplashDamage` + `ComputeSkillAtPosition`). PARTY, HORSE, arco
//!   (USE_ARROW_DAMAGE se resuelve con el ataque melee — desviación
//!   documentada) y el grand master (kMasterBonusPoly) quedan fuera.
//! - `ar` del poly = `combat::calc_attack_rating` del PC contra el mob.
//! - `def`/`odef` = la DEF del PC (`player_def_grade` + bonus / sin bonus).
//! - `skill_power.txt` no se carga — la fuente es PG (`common.locale`
//!   SKILL_POWER_BY_LEVEL*, parity config.cpp:532-613): el `k` del poly =
//!   `power(job, skillgroup, level) × max_level / 100` (tabla real, ver
//!   `database::skill_power`). Fail-open: sin tabla → `k = level ×
//!   max_level / 100` (aproximación — desviación documentada, F6 balance).



// ---------------------------------------------------------------------------
// Constantes del wire/dominio (parity char.h / skill.h / affect.h)
// ---------------------------------------------------------------------------

/// `SKILL_FLAG_*` (ESkillFlags — skill.h): los bits de `setflag`.
pub mod skill_flag {
    pub const ATTACK: u32 = 1 << 0;
    pub const USE_MELEE_DAMAGE: u32 = 1 << 1;
    pub const COMPUTE_ATTGRADE: u32 = 1 << 2;
    pub const SELFONLY: u32 = 1 << 3;
    pub const USE_MAGIC_DAMAGE: u32 = 1 << 4;
    pub const USE_HP_AS_COST: u32 = 1 << 5;
    pub const COMPUTE_MAGIC_DAMAGE: u32 = 1 << 6;
    pub const SPLASH: u32 = 1 << 7;
    pub const USE_ARROW_DAMAGE: u32 = 1 << 9;
}

/// `POINT_*` (EPointTypes — char.h:134+): el `bPointIdxApplyOn` del wire y
/// el índice del punto que el buff modifica.
pub mod point {
    pub const NONE: u8 = 0;
    pub const HP: u8 = 5;
    pub const MAX_HP: u8 = 6;
    pub const SP: u8 = 7;
    pub const MAX_SP: u8 = 8;
    pub const DEF_GRADE: u8 = 16;
    pub const ATT_SPEED: u8 = 17;
    pub const ATT_GRADE: u8 = 18;
    pub const MOV_SPEED: u8 = 19;
    pub const CASTING_SPEED: u8 = 21;
    pub const CRITICAL_PCT: u8 = 40;
    pub const ATT_GRADE_BONUS: u8 = 95;
    pub const DEF_GRADE_BONUS: u8 = 96;
}

/// `SKILL_ATTR_TYPE_*` (el `eskilltype` de la DB: NORMAL/MELEE/RANGE/MAGIC —
/// el `bSkillAttrType` del TSkillTable). El ajuste del daño final y el flag
/// del GC_DAMAGE_INFO dependen de esto (char_skill.cpp:1209-1246).
pub mod attr_type {
    pub const NORMAL: u8 = 0;
    pub const MELEE: u8 = 1;
    pub const RANGE: u8 = 2;
    pub const MAGIC: u8 = 3;
}

/// `DAMAGE_TYPE_*` del wire del GC_DAMAGE_INFO (EDamageType — char.h):
/// NORMAL=1, NORMAL_RANGE=2, MELEE=3, RANGE=4, MAGIC=8. El ataque de skill
/// manda el tipo según su attr (char_skill.cpp:1209-1216).
pub mod damage_type {
    pub const NORMAL: u8 = 1;
    pub const MELEE: u8 = 3;
    pub const RANGE: u8 = 4;
    pub const MAGIC: u8 = 8;
}

/// `AFF_*` (EAffectBits — affect.h:140-181): el `dwFlag` del elemento de
/// afecto (el icono del buff del cliente). Solo los que usa el runtime.
pub mod aff {
    pub const YMIR: u32 = 1;
    pub const CHEONGEUN: u32 = 17;
    pub const GYEONGGONG: u32 = 18;
    pub const EUNHYUNG: u32 = 19;
    pub const RED_POSSESSION: u32 = 44;
    pub const BLUE_POSSESSION: u32 = 45;
}

// ---------------------------------------------------------------------------
// El proto de la skill (columnas de `player.skill_proto`)
// ---------------------------------------------------------------------------

/// El proto de una skill (el subset que el slice usa). La carga completa de
/// la tabla vive en `SkillRepo::load_all`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillProto {
    pub vnum: u32,
    /// `btype` — el SKILL_TYPE_* (HORSE=5 fuera del subset).
    pub b_type: u8,
    /// `blevelstep` — el paso de nivel del skill.
    pub level_step: u8,
    /// `bmaxlevel` — el nivel máximo (`bMaxLevel` del k del poly).
    pub max_level: u8,
    /// `szpointon` → `FindPointType` (skill.cpp:84-96) — el POINT_* que el
    /// efecto modifica (HP para daño; DEF_GRADE_BONUS/MOV_SPEED/... buffs).
    pub point_on: u8,
    /// `szpointpoly` — la fórmula del efecto (negativa en la DB para daño).
    pub point_poly: String,
    /// `szspcostpoly` — la fórmula del coste de SP (vars v/maxv/maxhp).
    pub sp_cost_poly: String,
    /// `szdurationpoly` — la fórmula de la duración del buff (segundos).
    pub duration_poly: String,
    /// `szcooldownpoly` — la fórmula del cooldown (segundos).
    pub cooldown_poly: String,
    /// `setflag` — los SKILL_FLAG_* bits.
    pub flag: u32,
    /// `setaffectflag` — el AFF_* (icono del buff).
    pub affect_flag: u32,
    /// `eskilltype` — SKILL_ATTR_TYPE_* (ajuste del daño + flag wire).
    pub attr_type: u8,
    /// `imaxhit` — hits máximos del splash (0 = SIN límite — parity
    /// `UseSkill`: `lMaxHit = pkSk->lMaxHit ? pkSk->lMaxHit : -1`,
    /// char_skill.cpp:2464-2465 — el `HitOnce` del TSkillUseInfo consume
    /// uno por víctima).
    pub max_hit: u16,
    /// `dwsplashrange` — el RADIO del modo SPLASH (`iSplashRange` — la
    /// distancia máxima del centro a cada víctima, `DISTANCE_APPROX`).
    pub splash_range: u32,
    /// `szsplasharounddamageadjustpoly` — el multiplicador del daño de las
    /// víctimas que NO son el main target (`kSplashAroundDamageAdjustPoly`,
    /// char_skill.cpp:1206-1209). Vacío → 1.0 (sin ajuste).
    pub splash_adjust_poly: String,
    /// `dwtargetrange` — rango del objetivo (0 = sin check — parity
    /// ComputeSkill: `dwTargetRange && dist >= range + 50` → rechazo).
    pub target_range: u32,
}

impl SkillProto {
    /// ¿Skill de daño? (parity: el branch de ataque de ComputeSkill exige
    /// SKILL_FLAG_ATTACK + punto HP — char_skill.cpp:2070-2096).
    pub fn is_attack(&self) -> bool {
        self.flag & skill_flag::ATTACK != 0 && self.point_on == point::HP
    }
}

/// `FindPointType` parity (skill.cpp:84-96): el texto de `szpointon` → el
/// POINT_* del wire. `None` = tipo desconocido (el legacy aborta el boot —
/// el subset ignora la fila).
pub fn point_from_text(name: &str) -> Option<u8> {
    Some(match name.to_ascii_uppercase().as_str() {
        "NONE" => point::NONE,
        "MAX_HP" => point::MAX_HP,
        "MAX_SP" => point::MAX_SP,
        "HP" => point::HP,
        "SP" => point::SP,
        "ATT_GRADE" => point::ATT_GRADE_BONUS,
        "DEF_GRADE" => point::DEF_GRADE_BONUS,
        "MOV_SPEED" => point::MOV_SPEED,
        "ATT_SPEED" => point::ATT_SPEED,
        "CASTING_SPEED" => point::CASTING_SPEED,
        "CRITICAL" => point::CRITICAL_PCT,
        _ => return None,
    })
}

/// Los `SKILL_FLAG_*` bits del `setflag` (texto "ATTACK,USE_MELEE_DAMAGE").
pub fn skill_flags_from_text(text: &str) -> u32 {
    let mut flags = 0u32;
    for name in text.split(',') {
        flags |= match name.trim().to_ascii_uppercase().as_str() {
            "ATTACK" => skill_flag::ATTACK,
            "USE_MELEE_DAMAGE" => skill_flag::USE_MELEE_DAMAGE,
            "COMPUTE_ATTGRADE" => skill_flag::COMPUTE_ATTGRADE,
            "SELFONLY" => skill_flag::SELFONLY,
            "USE_MAGIC_DAMAGE" => skill_flag::USE_MAGIC_DAMAGE,
            "USE_HP_AS_COST" => skill_flag::USE_HP_AS_COST,
            "COMPUTE_MAGIC_DAMAGE" => skill_flag::COMPUTE_MAGIC_DAMAGE,
            "SPLASH" => skill_flag::SPLASH,
            "USE_ARROW_DAMAGE" => skill_flag::USE_ARROW_DAMAGE,
            _ => 0,
        };
    }
    flags
}

/// El AFF_* del `setaffectflag` (EAffectBits — el icono del buff del
/// cliente). Desconocido → 0 (el cliente ignora dwFlag sin
/// ENABLE_PLAYER_CHECKAFFECT — el icono se resuelve por dwType = skill).
pub fn affect_flag_from_text(name: &str) -> u32 {
    match name.trim().to_ascii_uppercase().as_str() {
        "YMIR" => aff::YMIR,
        "CHEONGEUN" => aff::CHEONGEUN,
        "GYEONGGONG" => aff::GYEONGGONG,
        "EUNHYUNG" => aff::EUNHYUNG,
        "RED_POSSESSION" => aff::RED_POSSESSION,
        "BLUE_POSSESSION" => aff::BLUE_POSSESSION,
        _ => 0,
    }
}

/// El SKILL_ATTR_TYPE_* del `eskilltype` (el `bSkillAttrType` legacy).
pub fn attr_type_from_text(name: &str) -> u8 {
    match name.trim().to_ascii_uppercase().as_str() {
        "MELEE" => attr_type::MELEE,
        "RANGE" => attr_type::RANGE,
        "MAGIC" => attr_type::MAGIC,
        _ => attr_type::NORMAL,
    }
}

/// El nivel del jugador en la skill: `player.skill_level` es un blob de
/// `255 × TPlayerSkill` (bMasterType + bLevel + tNextRead = 6 B — Packet.h);
/// `bLevel` vive en el offset `skill_id * 6 + 1` (parity
/// `m_pSkillLevels[dwVnum].bLevel` — el vnum INDEXA el array).
pub fn skill_level_from_blob(blob: &[u8], skill_id: u32) -> u8 {
    let off = skill_id as usize * 6 + 1;
    blob.get(off).copied().unwrap_or(0)
}

/// `k` del poly: `GetSkillPower(vnum, level) * bMaxLevel / 100`
/// (char_skill.cpp:1632). El runtime carga la tabla REAL por
/// job/skillgroup/nivel (`common.locale` SKILL_POWER_BY_LEVEL* — ver
/// `database::skill_power`) y el proceso_skill usa `power(job, group, level)
/// × max / 100`. Esta función es el FALLBACK fail-open cuando la tabla no
/// carga (aproximación `level × max_level / 100` — desviación documentada,
/// F6 balance; el server nunca rompe por eso).
pub fn k_value(skill_level: u8, max_level: u8) -> f64 {
    f64::from(skill_level) * f64::from(max_level) / 100.0
}

// ---------------------------------------------------------------------------
// El evaluador de POLYS (las fórmulas data-driven del skill_proto)
// ---------------------------------------------------------------------------

/// Evalúa una expresión poly con las vars del caller y el roll INCLUSIVE
/// del RNG del mundo (parity del `CPoly::Eval` del legacy + `number()`).
/// Funciones: `number(a, b)` (int inclusivo) y `floor(x)`.
pub fn eval_poly(
    expr: &str,
    var: &dyn Fn(&str) -> Option<f64>,
    roll: &mut dyn FnMut(i32, i32) -> i32,
) -> Result<f64, String> {
    let tokens = tokenize(expr)?;
    let mut p = Parser { tokens: &tokens, pos: 0, var, roll };
    let v = p.parse_expr()?;
    if p.pos != tokens.len() {
        return Err(format!("poly: tokens sobrantes tras '{expr}'"));
    }
    Ok(v)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char),
}

fn tokenize(expr: &str) -> Result<Vec<Tok>, String> {
    let b = expr.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '+' | '-' | '*' | '/' | '(' | ')' | ',' => {
                out.push(Tok::Op(c));
                i += 1;
            }
            '0'..='9' | '.' => {
                let start = i;
                while i < b.len() && ((b[i] as char).is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                let s: String = expr[start..i].chars().collect();
                let v: f64 = s.parse().map_err(|_| format!("poly: número inválido '{s}'"))?;
                out.push(Tok::Num(v));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let start = i;
                while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                out.push(Tok::Ident(expr[start..i].to_string()));
            }
            other => return Err(format!("poly: token inesperado '{other}'")),
        }
    }
    Ok(out)
}

struct Parser<'a> {
    tokens: &'a [Tok],
    pos: usize,
    var: &'a dyn Fn(&str) -> Option<f64>,
    roll: &'a mut dyn FnMut(i32, i32) -> i32,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&Tok> {
        let t = self.tokens.get(self.pos);
        self.pos += 1;
        t
    }

    fn parse_expr(&mut self) -> Result<f64, String> {
        let mut v = self.parse_term()?;
        loop {
            match self.peek() {
                Some(Tok::Op('+')) => {
                    self.next();
                    v += self.parse_term()?;
                }
                Some(Tok::Op('-')) => {
                    self.next();
                    v -= self.parse_term()?;
                }
                _ => return Ok(v),
            }
        }
    }

    fn parse_term(&mut self) -> Result<f64, String> {
        let mut v = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(Tok::Op('*')) => {
                    self.next();
                    v *= self.parse_unary()?;
                }
                Some(Tok::Op('/')) => {
                    self.next();
                    v /= self.parse_unary()?;
                }
                _ => return Ok(v),
            }
        }
    }

    fn parse_unary(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.next();
                Ok(-self.parse_unary()?)
            }
            Some(Tok::Op('+')) => {
                self.next();
                self.parse_unary()
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<f64, String> {
        let Some(tok) = self.next().cloned() else {
            return Err("poly: fin inesperado de la expresión".into());
        };
        match tok {
            Tok::Num(n) => Ok(n),
            Tok::Ident(name) => {
                if matches!(self.peek(), Some(Tok::Op('('))) {
                    self.next();
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Tok::Op(')'))) {
                        loop {
                            args.push(self.parse_expr()?);
                            match self.peek() {
                                Some(Tok::Op(',')) => {
                                    self.next();
                                }
                                _ => break,
                            }
                        }
                    }
                    if !matches!(self.next(), Some(Tok::Op(')'))) {
                        return Err(format!("poly: falta ')' tras {name}("));
                    }
                    match name.as_str() {
                        // `number(a,b)` — el número INCLUSIVE del C++
                        // (utils.h `number()`); el poly lo usa para rangos
                        // aleatorios de daño (p.ej. skill 31: number(500,700)).
                        "number" => {
                            if args.len() != 2 {
                                return Err("poly: number() necesita 2 args".into());
                            }
                            let (lo, hi) = (args[0] as i32, args[1] as i32);
                            Ok(f64::from((self.roll)(lo, hi)))
                        }
                        "floor" => {
                            if args.len() != 1 {
                                return Err("poly: floor() necesita 1 arg".into());
                            }
                            Ok(args[0].floor())
                        }
                        other => Err(format!("poly: función desconocida '{other}'")),
                    }
                } else {
                    (self.var)(&name).ok_or_else(|| format!("poly: var desconocida '{name}'"))
                }
            }
            Tok::Op('(') => {
                let v = self.parse_expr()?;
                if !matches!(self.next(), Some(Tok::Op(')'))) {
                    return Err("poly: falta ')'".into());
                }
                Ok(v)
            }
            other => Err(format!("poly: token inesperado {other:?}")),
        }
    }
}

// ---------------------------------------------------------------------------
// La cadena de daño/efecto (pura — parity char_skill.cpp FuncSplashDamage)
// ---------------------------------------------------------------------------

/// El daño final del skill (parity `FuncSplashDamage::OnHit`):
/// 1. `iAmount = -point_poly` (la DB da el daño en negativo).
/// 2. `CalcBattleDamage`: `if (iDam < 3) iDam = number(1, 5)` (battle.cpp:199).
/// 3. Ajustes por attr type (char_skill.cpp:1209-1246): MELEE →
///    `-= victim POINT_DEF_GRADE`; RANGE → `× (100 - RESIST_BOW)/100` y
///    MAGIC → `× (100 - RESIST_MAGIC)/100` (resistencias 0 para mobs en el
///    subset — los términos son identidad).
///
/// Devuelve el daño (≥ 0) o 0 si el efecto no es de daño.
pub fn skill_damage(attr: u8, amount: i32, victim_def: i32, roll: &mut dyn FnMut(i32, i32) -> i32) -> i32 {
    let mut dam = amount.max(0);
    if dam < 3 {
        dam = roll(1, 5); // CalcBattleDamage floor (parity battle.cpp:199-206)
    }
    if attr == attr_type::MELEE {
        dam = dam.saturating_sub(victim_def).max(0);
        // RANGE/MAGIC: resistencias 0 en el subset (mobs) — identidad.
    }
    dam
}

/// El flag del GC_DAMAGE_INFO para el golpe de skill (parity
/// char_skill.cpp:1209-1216 — el DAMAGE_TYPE del attr).
pub fn damage_flag_for_attr(attr: u8) -> u8 {
    match attr {
        attr_type::MELEE => damage_type::MELEE,
        attr_type::RANGE => damage_type::RANGE,
        attr_type::MAGIC => damage_type::MAGIC,
        _ => damage_type::NORMAL,
    }
}

// ---------------------------------------------------------------------------
// SkillRepo — la lectura del `player.skill_proto` (PG directo desde game_core,
// mismo patrón que el MobRepo del database crate; sin tocar database/)
// ---------------------------------------------------------------------------

/// Repositorio del `skill_proto` (PG). Conexión por llamada (ADR-0008).
pub struct SkillRepo {
    pool: database::pool::PgPool,
}

impl SkillRepo {
    pub fn new(pool: database::pool::PgPool) -> Self {
        Self { pool }
    }

    async fn connect(&self) -> Result<database::pool::Client, String> {
        self.pool.get().await.map_err(|e| format!("PG pool get: {e}"))
    }

    /// Carga TODAS las skills del `skill_proto` (la tabla es estática en el
    /// runtime — el mundo la cachea como recurso `SkillTable`). Las filas con
    /// un `szpointon` desconocido se omiten (el legacy abortaría el boot).
    pub async fn load_all(&self) -> Result<Vec<SkillProto>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(
                "SELECT dwvnum, btype, blevelstep, bmaxlevel, szpointon, szpointpoly, \
                 szspcostpoly, szdurationpoly, szcooldownpoly, setflag, setaffectflag, \
                 eskilltype, imaxhit, dwtargetrange, dwsplashrange, \
                 szsplasharounddamageadjustpoly \
                 FROM player.skill_proto ORDER BY dwvnum",
                &[],
            )
            .await
            .map_err(|e| format!("skill_proto: {e}"))?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            if let Some(p) = skill_proto_from_row(r) {
                out.push(p);
            }
        }
        Ok(out)
    }

    /// Carga UNA skill por vnum (el gate de flechas del canal — dw_arrow:
    /// las skills con flag USE_ARROW_DAMAGE exigen flechas equipadas antes
    /// del intent; mismas reglas de mapeo que `load_all`). `None` = no existe
    /// o `szpointon` desconocido.
    pub async fn load(&self, vnum: u32) -> Result<Option<SkillProto>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(
                "SELECT dwvnum, btype, blevelstep, bmaxlevel, szpointon, szpointpoly, \
                 szspcostpoly, szdurationpoly, szcooldownpoly, setflag, setaffectflag, \
                 eskilltype, imaxhit, dwtargetrange, dwsplashrange, \
                 szsplasharounddamageadjustpoly \
                 FROM player.skill_proto WHERE dwvnum = $1",
                &[&(vnum as i32)],
            )
            .await
            .map_err(|e| format!("skill_proto: {e}"))?;
        Ok(rows.first().and_then(skill_proto_from_row))
    }
}

/// Mapeo de una fila del `skill_proto` → `SkillProto` (compartido por
/// `load_all` y `load`). `None` si el `szpointon` es desconocido (parity: el
/// legacy abortaría el boot — el subset ignora la fila).
fn skill_proto_from_row(r: &tokio_postgres::Row) -> Option<SkillProto> {
    let point_on = point_from_text(r.get::<_, Option<String>>(4).as_deref().unwrap_or(""));
    let point_on = point_on?; // szpointon desconocido — fila ignorada (documentado)
    Some(SkillProto {
        vnum: r.get::<_, i32>(0) as u32,
        b_type: r.get::<_, i16>(1) as u8,
        level_step: r.get::<_, i16>(2) as u8,
        max_level: r.get::<_, i16>(3) as u8,
        point_on,
        point_poly: r.get::<_, Option<String>>(5).unwrap_or_default(),
        sp_cost_poly: r.get::<_, Option<String>>(6).unwrap_or_default(),
        duration_poly: r.get::<_, Option<String>>(7).unwrap_or_default(),
        cooldown_poly: r.get::<_, Option<String>>(8).unwrap_or_default(),
        flag: skill_flags_from_text(r.get::<_, Option<String>>(9).as_deref().unwrap_or("")),
        affect_flag: affect_flag_from_text(r.get::<_, Option<String>>(10).as_deref().unwrap_or("")),
        attr_type: attr_type_from_text(r.get::<_, Option<String>>(11).as_deref().unwrap_or("")),
        max_hit: r.get::<_, i16>(12) as u16,
        target_range: r.get::<_, i32>(13) as u32,
        splash_range: r.get::<_, i64>(14).max(0) as u32,
        splash_adjust_poly: r.get::<_, Option<String>>(15).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roll_fixed(v: i32) -> impl FnMut(i32, i32) -> i32 {
        move |_lo, _hi| v
    }

    fn vars<'a>(map: &'a [(&'a str, f64)]) -> impl Fn(&str) -> Option<f64> + 'a {
        move |name| map.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
    }

    /// La fórmula REAL del skill 1 (삼연참 — warrior) con atk=100, str=30,
    /// k=1: `-(1.1*100 + (0.5*100 + 1.5*30)*1)` = -(110 + 95) = -205.
    #[test]
    fn eval_skill1_poly_real_formula() {
        let v = vars(&[("atk", 100.0), ("str", 30.0), ("k", 1.0)]);
        let mut roll = roll_fixed(0);
        let r = eval_poly("-( 1.1*atk + (0.5*atk +  1.5 * str)*k)", &v, &mut roll).unwrap();
        assert_eq!(r, -205.0, "1.1*100 + (50 + 45)*1");
    }

    /// La fórmula REAL del skill 19 (천근 — Iron Skin): `(200 + str*0.2 +
    /// con*0.5)*k` con str=30, con=30, k=1 → (200 + 6 + 15) = 221.
    #[test]
    fn eval_skill19_buff_formula() {
        let v = vars(&[("str", 30.0), ("con", 30.0), ("k", 1.0)]);
        let mut roll = roll_fixed(0);
        let r = eval_poly("(200 + str*0.2 + con*0.5 ) *k", &v, &mut roll).unwrap();
        assert_eq!(r, 221.0);
    }

    /// `floor` (skill 46 — 화둔): `0.3*atk*floor(2+k*6)` con k=1 → floor(8)=8.
    #[test]
    fn eval_floor_function() {
        let v = vars(&[("atk", 100.0), ("k", 1.0)]);
        let mut roll = roll_fixed(0);
        let r = eval_poly("0.3*atk*floor(2+k*6)", &v, &mut roll).unwrap();
        assert_eq!(r, 240.0, "0.3*100*floor(8)");
    }

    /// `number(a,b)` — el random INCLUSIVE del C++ (el roll del mundo).
    #[test]
    fn eval_number_function_uses_roll() {
        let v = vars(&[]);
        let mut roll = |lo: i32, hi: i32| {
            assert_eq!((lo, hi), (500, 700));
            600
        };
        let r = eval_poly("number(500, 700)", &v, &mut roll).unwrap();
        assert_eq!(r, 600.0);
    }

    /// Operadores: unario, división, cadenas con y sin espacios.
    #[test]
    fn eval_operators_and_precedence() {
        let v = vars(&[("k", 2.0)]);
        let mut roll = roll_fixed(0);
        assert_eq!(eval_poly("60*k", &v, &mut roll).unwrap(), 120.0);
        assert_eq!(eval_poly("-2*k", &v, &mut roll).unwrap(), -4.0);
        assert_eq!(eval_poly("80-12*k", &v, &mut roll).unwrap(), 56.0);
        assert_eq!(eval_poly("10/2+3", &v, &mut roll).unwrap(), 8.0);
        assert_eq!(eval_poly("2*(3+4)", &v, &mut roll).unwrap(), 14.0);
        // maxhp*0.2*k (skill 152 — 보호의노래): 1000*0.2*1 = 200.
        let v2 = vars(&[("maxhp", 1000.0), ("k", 1.0)]);
        assert_eq!(eval_poly("maxhp*0.2*k", &v2, &mut roll).unwrap(), 200.0);
    }

    /// Errores: var desconocida, función desconocida, sintaxis rota.
    #[test]
    fn eval_errors() {
        let v = vars(&[("k", 1.0)]);
        let mut roll = roll_fixed(0);
        assert!(eval_poly("atk*k", &v, &mut roll).is_err(), "var sin setear");
        assert!(eval_poly("pow(2,3)", &v, &mut roll).is_err(), "función desconocida");
        assert!(eval_poly("(1+2", &v, &mut roll).is_err(), "falta ')'");
        assert!(eval_poly("1+", &v, &mut roll).is_err(), "token colgado");
        assert!(eval_poly("1 @ 2", &v, &mut roll).is_err(), "token inválido");
    }

    /// FindPointType parity (skill.cpp:84-96): los textos del runtime.
    #[test]
    fn point_from_text_matches_cpp() {
        assert_eq!(point_from_text("HP"), Some(point::HP));
        assert_eq!(point_from_text("DEF_GRADE"), Some(point::DEF_GRADE_BONUS));
        assert_eq!(point_from_text("ATT_GRADE"), Some(point::ATT_GRADE_BONUS));
        assert_eq!(point_from_text("MOV_SPEED"), Some(point::MOV_SPEED));
        assert_eq!(point_from_text("MAX_HP"), Some(point::MAX_HP));
        assert_eq!(point_from_text("MAX_SP"), Some(point::MAX_SP));
        assert_eq!(point_from_text("NONE"), Some(point::NONE));
        assert_eq!(point_from_text("hp"), Some(point::HP), "case-insensitive");
        assert_eq!(point_from_text("BOGUS"), None);
    }

    /// Los flags del setflag (texto del runtime).
    #[test]
    fn skill_flags_from_text_matches_cpp() {
        assert_eq!(skill_flags_from_text("ATTACK,USE_MELEE_DAMAGE"), 0b11);
        assert_eq!(
            skill_flags_from_text("ATTACK,USE_MELEE_DAMAGE,SELFONLY,SPLASH"),
            skill_flag::ATTACK | skill_flag::USE_MELEE_DAMAGE | skill_flag::SELFONLY | skill_flag::SPLASH
        );
        assert_eq!(skill_flags_from_text(""), 0);
        assert_eq!(skill_flags_from_text("SELFONLY"), skill_flag::SELFONLY);
    }

    /// Los AFF_* del setaffectflag (EAffectBits — affect.h).
    #[test]
    fn affect_flag_from_text_matches_cpp() {
        assert_eq!(affect_flag_from_text("YMIR"), aff::YMIR);
        assert_eq!(affect_flag_from_text("CHEONGEUN"), aff::CHEONGEUN);
        assert_eq!(affect_flag_from_text("GYEONGGONG"), aff::GYEONGGONG);
        assert_eq!(affect_flag_from_text("EUNHYUNG"), aff::EUNHYUNG);
        assert_eq!(affect_flag_from_text("RED_POSSESSION"), aff::RED_POSSESSION);
        assert_eq!(affect_flag_from_text("BLUE_POSSESSION"), aff::BLUE_POSSESSION);
        assert_eq!(affect_flag_from_text(""), 0);
        assert_eq!(affect_flag_from_text("BOGUS"), 0);
    }

    /// El nivel de la skill en el blob del player (255 × TPlayerSkill de
    /// 6 B — bLevel en el offset skill_id*6+1).
    #[test]
    fn skill_level_from_blob_layout() {
        let mut blob = vec![0u8; 255 * 6];
        blob[1 * 6 + 1] = 5; // skill 1 nivel 5
        blob[19 * 6 + 1] = 3; // skill 19 nivel 3
        assert_eq!(skill_level_from_blob(&blob, 1), 5);
        assert_eq!(skill_level_from_blob(&blob, 19), 3);
        assert_eq!(skill_level_from_blob(&blob, 0), 0);
        assert_eq!(skill_level_from_blob(&blob, 999), 0, "fuera de rango");
        assert_eq!(skill_level_from_blob(&[], 1), 0);
    }

    /// k = level × max_level / 100 (el FAILBACK fail-open del runtime
    /// cuando la tabla real de skill_power no carga).
    #[test]
    fn k_value_formula() {
        assert_eq!(k_value(5, 40), 2.0);
        assert_eq!(k_value(1, 40), 0.4);
        assert_eq!(k_value(0, 40), 0.0);
    }

    /// La cadena de daño (FuncSplashDamage::OnHit parity): el poly da -205 →
    /// daño 205; MELEE → -def del mob; floor < 3 → number(1,5).
    #[test]
    fn skill_damage_chain() {
        let mut roll = |lo: i32, hi: i32| match (lo, hi) {
            (1, 5) => 3,
            _ => panic!("roll inesperado ({lo},{hi})"),
        };
        // Skill 1 (MELEE) con mob DEF 10: 205 - 10 = 195.
        assert_eq!(skill_damage(attr_type::MELEE, 205, 10, &mut roll), 195);
        // Sin def: el daño pasa (CalcBattleDamage no toca ≥ 3).
        assert_eq!(skill_damage(attr_type::MELEE, 205, 0, &mut roll), 205);
        // Floor: daño 2 → number(1,5) = 3.
        assert_eq!(skill_damage(attr_type::MELEE, 2, 0, &mut roll), 3);
        // RANGE/MAGIC: resistencias 0 → identidad (subset documentado).
        assert_eq!(skill_damage(attr_type::RANGE, 205, 10, &mut roll), 205);
        assert_eq!(skill_damage(attr_type::MAGIC, 205, 10, &mut roll), 205);
        // Def mayor que el daño → 0 (el C++ clamp en Damage).
        assert_eq!(skill_damage(attr_type::MELEE, 5, 100, &mut roll), 0);
        // amount negativo (estado pre-negación por error del caller): el
        // floor de CalcBattleDamage aplica a CUALQUIER valor < 3 (incluido
        // el negativo — parity battle.cpp:199) → number(1,5).
        assert_eq!(skill_damage(attr_type::MELEE, -50, 0, &mut roll), 3);
    }

    /// El flag del GC_DAMAGE_INFO por attr (EDamageType).
    #[test]
    fn damage_flag_matches_attr() {
        assert_eq!(damage_flag_for_attr(attr_type::NORMAL), damage_type::NORMAL);
        assert_eq!(damage_flag_for_attr(attr_type::MELEE), damage_type::MELEE);
        assert_eq!(damage_flag_for_attr(attr_type::RANGE), damage_type::RANGE);
        assert_eq!(damage_flag_for_attr(attr_type::MAGIC), damage_type::MAGIC);
    }

    /// is_attack: flag ATTACK + punto HP (parity ComputeSkill).
    #[test]
    fn attack_skill_detection() {
        let atk = SkillProto {
            vnum: 1,
            b_type: 0,
            level_step: 1,
            max_level: 40,
            point_on: point::HP,
            point_poly: "-(1.1*atk + (0.5*atk + 1.5*str)*k)".into(),
            sp_cost_poly: "80+220*k".into(),
            duration_poly: String::new(),
            cooldown_poly: "12".into(),
            flag: skill_flag::ATTACK | skill_flag::USE_MELEE_DAMAGE,
            affect_flag: 0,
            attr_type: attr_type::MELEE,
            max_hit: 1,
            target_range: 0,
            splash_range: 0,
            splash_adjust_poly: String::new(),
        };
        assert!(atk.is_attack());
        let buff = SkillProto { flag: skill_flag::SELFONLY, point_on: point::DEF_GRADE_BONUS, ..atk };
        assert!(!buff.is_attack());
    }

    /// La lectura PG se valida en el E2E gated (realm_pg) — aquí el parseo
    /// de una fila real del runtime (skill 1 y 19 del dump de la DB).
    #[test]
    fn proto_parse_matches_db_rows() {
        // skill 1: MELEE, target_range 0, cooldown 12, flags ATTACK+MELEE.
        let s1 = SkillProto {
            vnum: 1,
            b_type: 0,
            level_step: 1,
            max_level: 40,
            point_on: point_from_text("HP").unwrap(),
            point_poly: "-( 1.1*atk + (0.5*atk +  1.5 * str)*k)".into(),
            sp_cost_poly: "80+220*k".into(),
            duration_poly: String::new(),
            cooldown_poly: "12".into(),
            flag: skill_flags_from_text("ATTACK,USE_MELEE_DAMAGE"),
            affect_flag: affect_flag_from_text(""),
            attr_type: attr_type_from_text("MELEE"),
            max_hit: 1,
            target_range: 0,
            splash_range: 0,
            splash_adjust_poly: String::new(),
        };
        assert!(s1.is_attack());
        assert_eq!(s1.attr_type, attr_type::MELEE);
        assert_eq!(s1.cooldown_poly, "12");
        // skill 19: self-buff de DEF_GRADE (CHEONGEUN), cooldown "63+90*k".
        let s19 = SkillProto {
            vnum: 19,
            b_type: 0,
            level_step: 1,
            max_level: 40,
            point_on: point_from_text("DEF_GRADE").unwrap(),
            point_poly: "(200 + str*0.2 + con*0.5 ) *k".into(),
            sp_cost_poly: "80+220*k".into(),
            duration_poly: "60+90*k".into(),
            cooldown_poly: "63+90*k".into(),
            flag: skill_flags_from_text("SELFONLY"),
            affect_flag: affect_flag_from_text("CHEONGEUN"),
            attr_type: attr_type_from_text("NORMAL"),
            max_hit: 1,
            target_range: 0,
            splash_range: 0,
            splash_adjust_poly: String::new(),
        };
        assert!(!s19.is_attack());
        assert_eq!(s19.point_on, point::DEF_GRADE_BONUS);
        assert_eq!(s19.affect_flag, aff::CHEONGEUN);
    }
}
