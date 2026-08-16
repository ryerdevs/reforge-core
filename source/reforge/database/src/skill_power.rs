//! F5 (skills) — la tabla REAL de poder de skills (`SKILL_POWER_BY_LEVEL*`
//! de `common.locale`): parity `config.cpp:532-613` (el C++ la lee de la DB
//! al boot) + `skill_power.cpp` (`CTableBySkill`). Sustituye la aproximación
//! `k = level × max_level / 100` (desviación documentada en game_core) por
//! `k = GetSkillPower(vnum, level) × bMaxLevel / 100` — el poder REAL por
//! job/skillgroup/nivel (char_skill.cpp:1632, char.cpp:7200).
//!
//! # Parity del legacy (file:line verificados 2026-08-13)
//!
//! - Base `SKILL_POWER_BY_LEVEL` (config.cpp:536): 41 números espacio-
//!   separados (niveles 0..=40 — `SKILL_MAX_LEVEL = 40`, length.h:63). El
//!   C++ es la fuente de fallback de los tipos sin fila.
//! - Por tipo `SKILL_POWER_BY_LEVEL_TYPE<job>` (config.cpp:576-611), job
//!   0..<JOB_MAX_NUM×2 — el fork congelado tiene 9 filas verificadas en PG
//!   (TYPE0..TYPE8). Fila ausente → se usa la base (config.cpp:579-583).
//! - Lookup `GetSkillPowerByLevelFromType(job, skillgroup, level, bMob)`
//!   (skill_power.cpp:31-41): `bMob` → tabla [0]; `job >= JOB_MAX_NUM ||
//!   skillgroup == 0` → 0; si no, `idx = job×2 + (skillgroup-1)`; el nivel
//!   se clampa a [0, SKILL_MAX_LEVEL] (char.cpp:7200 `MINMAX`).
//!
//! `JOB_MAX_NUM = 4` (warrior/assassin/sura/shaman — sin wolfman en este
//! fork; jobs de PC 0..3).

use crate::account::pg_err;
use crate::pool::{Client, PgPool};

/// `SKILL_MAX_LEVEL` (length.h:63) — el índice máximo de la tabla (41
/// valores: niveles 0..=40).
pub const SKILL_MAX_LEVEL: usize = 40;

/// `JOB_MAX_NUM` (length.h:199, sin ENABLE_WOLFMAN_CHARACTER) — jobs de PC
/// 0..3; `job >= JOB_MAX_NUM` → poder 0 (parity skill_power.cpp:36).
pub const JOB_MAX_NUM: u8 = 4;

/// Filas de tipo que el fork congelado tiene verificadas en PG:
/// `SKILL_POWER_BY_LEVEL_TYPE0..8` (job 0..8; el C++ itera
/// `< JOB_MAX_NUM×2 = 8` — la extra va en la carga, el lookup nunca llega).
const TYPE_ROWS: usize = 9;

/// La tabla real de poder por nivel (parity `m_aiSkillPowerByLevelFromType`
/// de `CTableBySkill` — skill_power.h:32): `rows[idx]` = la tabla del
/// `SKILL_POWER_BY_LEVEL_TYPE<idx>` (o la base cuando el tipo no existe).
/// `rows[0]` = la tabla base de los mobs (`bMob` → skill_power.cpp:32-35).
/// `rows` VACÍO = tabla no cargada (fail-open: el caller cae a la
/// aproximación documentada con log, NO rompe el server).
#[derive(Debug, Clone, Default)]
pub struct SkillPowerTable {
    rows: Vec<[i32; SKILL_MAX_LEVEL + 1]>,
}

impl SkillPowerTable {
    /// Constructor para tests/harness: `rows[idx]` = la tabla del tipo idx
    /// (0 = la base de los mobs). El runtime la obtiene de
    /// `SkillPowerRepo::load` — filas vacías = tabla no cargada.
    pub fn from_rows(rows: Vec<[i32; SKILL_MAX_LEVEL + 1]>) -> Self {
        Self { rows }
    }

    /// ¿Tabla cargada? (fail-open: `false` → el runtime usa la
    /// aproximación `k = level × max_level / 100`).
    pub fn loaded(&self) -> bool {
        !self.rows.is_empty()
    }

    /// `GetSkillPowerByLevelFromType(job, skillgroup, level, bMob)` parity
    /// (skill_power.cpp:31-41 + char.cpp:7200). Los jobs de PC son 0..3;
    /// `b_mob` → la tabla base (rows[0]); `job >= JOB_MAX_NUM` o
    /// `skillgroup == 0` → 0; si no, `idx = job×2 + (skillgroup-1)`. El
    /// nivel se clampa a [0, SKILL_MAX_LEVEL]. Sin tabla (no cargada) →
    /// 0 (el caller ya decidió la aproximación con `loaded()`).
    pub fn skill_power(&self, job: u8, skill_group: i16, level: i32, b_mob: bool) -> i32 {
        // Sin tabla (fail-open) → 0 (el caller decidió la aproximación con
        // `loaded()` antes de llamar).
        let Some(base) = self.rows.first() else { return 0 };
        let lv = level.clamp(0, SKILL_MAX_LEVEL as i32) as usize;
        if b_mob {
            return base[lv];
        }
        if job >= JOB_MAX_NUM || skill_group <= 0 {
            return 0;
        }
        let idx = (job as usize) * 2 + (skill_group as usize - 1);
        // Defensivo: idx fuera del rango cargado → la base (el C++ nunca
        // indexa fuera — job < 4 y group 1..2 dan idx ≤ 7).
        let row = self.rows.get(idx).unwrap_or(base);
        row[lv]
    }
}

/// `one_argument` + `atoi` del C++ (config.cpp:560-571): parsea los 41
/// números espacio-separados del mValue. El C++ sale con exit(1) si no hay
/// exactamente 41 — aquí `Err` (el canal hace fail-open a la aproximación
/// con log). Un token no numérico → `Err` (desviación defensiva documentada:
/// el `atoi` del C++ daría 0 en silencio).
pub fn parse_power_row(mvalue: &str) -> Result<[i32; SKILL_MAX_LEVEL + 1], String> {
    let mut out = [0i32; SKILL_MAX_LEVEL + 1];
    let mut n = 0;
    for tok in mvalue.split_whitespace() {
        if n > SKILL_MAX_LEVEL {
            return Err(format!(
                "skill_power: fila con más de {} valores ('{mvalue}')",
                SKILL_MAX_LEVEL + 1
            ));
        }
        out[n] = tok
            .parse()
            .map_err(|_| format!("skill_power: token '{tok}' no numérico en '{mvalue}'"))?;
        n += 1;
    }
    if n != SKILL_MAX_LEVEL + 1 {
        return Err(format!(
            "skill_power: {} valores, se esperaban {} ('{mvalue}')",
            n,
            SKILL_MAX_LEVEL + 1
        ));
    }
    Ok(out)
}

/// Repositorio del `SKILL_POWER_BY_LEVEL*` (schema `common` — misma tabla
/// `common.locale` mkey/mvalue que el resto del locale server-side).
/// Conexión por llamada (ADR-0008 — patrón `LocaleRepo`).
pub struct SkillPowerRepo {
    pool: PgPool,
}

impl SkillPowerRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn connect(&self) -> Result<Client, String> {
        self.pool.get().await.map_err(|e| format!("PG pool get: {e}"))
    }

    /// Carga la tabla completa (parity config.cpp:532-613): la base
    /// `SKILL_POWER_BY_LEVEL` (obligatoria — el C++ sale del boot sin ella)
    /// + las `SKILL_POWER_BY_LEVEL_TYPE0..8` (fila ausente → la base como
    /// fallback, config.cpp:579-583). Errores → `Err` (el canal hace
    /// fail-open a la aproximación con log — no rompe el server).
    pub async fn load(&self) -> Result<SkillPowerTable, String> {
        let client = self.connect().await?;
        let base = self
            .row_for(&client, "SKILL_POWER_BY_LEVEL")
            .await?
            .ok_or_else(|| "SKILL_POWER_BY_LEVEL ausente en common.locale".to_string())?;
        let mut rows = Vec::with_capacity(TYPE_ROWS);
        for idx in 0..TYPE_ROWS {
            let key = format!("SKILL_POWER_BY_LEVEL_TYPE{idx}");
            rows.push(self.row_for(&client, &key).await?.unwrap_or(base));
        }
        Ok(SkillPowerTable { rows })
    }

    /// La fila (41 valores) de un mKey del `common.locale`, o `None` si el
    /// mKey no existe (parity `uiNumRows == 0` → fallback a la base).
    async fn row_for(
        &self,
        client: &Client,
        key: &str,
    ) -> Result<Option<[i32; SKILL_MAX_LEVEL + 1]>, String> {
        let row = client
            .query_opt("SELECT mvalue FROM common.locale WHERE mkey = $1", &[&key])
            .await
            .map_err(|e| pg_err("SKILL_POWER_LOAD", &e))?;
        let Some(row) = row else { return Ok(None) };
        let value: String = row.try_get(0).map_err(|e| format!("SKILL_POWER col0 ({key}): {e}"))?;
        parse_power_row(&value).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture: una fila válida de 41 valores (niveles 0..=40).
    fn fixture_row() -> String {
        (0..=40).map(|i| i.to_string()).collect::<Vec<_>>().join(" ")
    }

    /// El parseo (parity `one_argument`): 41 valores espacio-separados →
    /// el array indexado por nivel; separadores múltiples/espacios
    /// tolerados (el `one_argument` del C++ también).
    #[test]
    fn parse_power_row_ok() {
        let row = parse_power_row(&fixture_row()).expect("41 valores");
        assert_eq!(row[0], 0);
        assert_eq!(row[5], 5);
        assert_eq!(row[40], 40);
        assert_eq!(row.len(), SKILL_MAX_LEVEL + 1);
        // Separadores mezclados (tabs/multi-espacio).
        let messy = "0 1\t2   3 4";
        assert!(parse_power_row(&messy).is_err(), "solo 5 valores");
        let mut v: Vec<String> = (0..=40).map(|i| i.to_string()).collect();
        v[3] = "3".to_string();
        let ok = parse_power_row(&v.join("  \t ")).expect("41 con separadores mixtos");
        assert_eq!(ok[3], 3);
    }

    /// Errores del parseo (el C++ haría exit(1); aquí Err → fail-open).
    #[test]
    fn parse_power_row_errors() {
        // 40 valores (faltan) → Err.
        let short = (0..40).map(|i| i.to_string()).collect::<Vec<_>>().join(" ");
        assert!(parse_power_row(&short).is_err(), "40 < 41");
        // 42 valores (sobran) → Err.
        let long = (0..=41).map(|i| i.to_string()).collect::<Vec<_>>().join(" ");
        assert!(parse_power_row(&long).is_err(), "42 > 41");
        // Token no numérico → Err (desviación defensiva vs atoi=0).
        assert!(parse_power_row(&format!("{} x", fixture_row())).is_err(), "token basura");
        // Vacío → Err.
        assert!(parse_power_row("").is_err());
    }

    /// El lookup completo (parity skill_power.cpp:31-41): bMob → base;
    /// job >= JOB_MAX_NUM o skillgroup 0 → 0; idx = job×2 + (skillgroup-1);
    /// nivel clamp a [0, 40].
    #[test]
    fn skill_power_lookup_matches_cpp() {
        // Filas diferenciadas por idx: fila i → valor 1000+i en cada nivel.
        let mut rows = Vec::new();
        for i in 0..9 {
            rows.push([1000 + i as i32; SKILL_MAX_LEVEL + 1]);
        }
        let t = SkillPowerTable { rows };
        // Mob → la fila 0 (base).
        assert_eq!(t.skill_power(0, 1, 1, true), 1000, "bMob → tabla base");
        assert_eq!(t.skill_power(3, 2, 40, true), 1000, "bMob ignora job/group");
        // Warrior (job 0): group 1 → idx 0; group 2 → idx 1.
        assert_eq!(t.skill_power(0, 1, 5, false), 1000);
        assert_eq!(t.skill_power(0, 2, 5, false), 1001);
        // Assassin (job 1): group 1 → idx 2; group 2 → idx 3.
        assert_eq!(t.skill_power(1, 1, 5, false), 1002);
        assert_eq!(t.skill_power(1, 2, 5, false), 1003);
        // Sura (job 2) / Shaman (job 3).
        assert_eq!(t.skill_power(2, 1, 5, false), 1004);
        assert_eq!(t.skill_power(3, 2, 5, false), 1007);
        // Guards del C++: job fuera del rango / skillgroup 0 → 0.
        assert_eq!(t.skill_power(4, 1, 5, false), 0, "job >= JOB_MAX_NUM");
        assert_eq!(t.skill_power(0, 0, 5, false), 0, "skillgroup == 0");
        // Nivel clamp (char.cpp:7200 MINMAX(0, level, SKILL_MAX_LEVEL)).
        assert_eq!(t.skill_power(0, 1, 99, false), 1000, "clamp a 40");
        assert_eq!(t.skill_power(0, 1, -3, false), 1000, "clamp a 0");
    }

    /// Tabla no cargada (fail-open): `loaded() == false` y el lookup da 0
    /// (el runtime decide la aproximación con `loaded()`).
    #[test]
    fn skill_power_table_empty_fail_open() {
        let t = SkillPowerTable::default();
        assert!(!t.loaded());
        assert_eq!(t.skill_power(0, 1, 5, false), 0);
        assert_eq!(t.skill_power(0, 1, 5, true), 0);
    }

    /// Live-PG (gated, patrón locale.rs): la tabla real del `common.locale`
    /// importado (2026-08-13): 9 filas de tipo + la base; el valor del
    /// warrior group 1 nivel 1 (idx 0) del runtime.
    #[tokio::test]
    #[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
    async fn load_live_pg() {
        let pg = std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| {
            "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2".to_string()
        });
        let repo = SkillPowerRepo::new(crate::pool::new_pool(&pg, 4).expect("pool"));
        let t = repo.load().await.expect("carga de skill_power");
        assert!(t.loaded());
        assert_eq!(t.skill_power(0, 1, 1, false), t.skill_power(0, 1, 1, false));
        // Sanity: el poder del warrior group 1 es > 0 en nivel 1 (una tabla
        // real nunca da 0 en todos los niveles — balance del juego).
        let p = t.skill_power(0, 1, 1, false);
        assert!(p > 0, "warrior group 1 nivel 1: {p}");
        // bMob usa la fila 0 (la misma base que el C++).
        assert_eq!(t.skill_power(5, 1, 1, true), t.skill_power(0, 1, 1, false));
    }
}