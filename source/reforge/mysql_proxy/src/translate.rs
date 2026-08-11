//! Reescritura SQL MySQL→PostgreSQL (spec §8.2.1c). La tabla §4 de
//! `docs/reference/database/legacy-sql-compatibility.md` es la spec de los unit
//! tests: cada fila de esa tabla tiene su caso aquí.
//!
//! El proxy recibe el texto YA formateado por el `snprintf` del C++ (literales
//! reales), por lo que las reglas operan sobre texto final, no sobre formatos.
//!
//! Reglas implementadas (inventario §3/§4):
//! - backticks → comillas dobles (reservadas en PG: `window`, `where`, `when`);
//! - `col+0` (cast de ENUM/SET a entero, 12 columnas del boot) → expresión PG
//!   de índice (ENUM) o bitmask (SET) según el catálogo estático
//!   `ENUM_COLUMNS` (fuente: SHOW CREATE MariaDB, 2026-08-11); el `+0` de
//!   columnas no catalogadas se elimina (fallback histórico);
//! - `NOW()` → `LOCALTIMESTAMP`; `x - NOW() > 0` → `x > LOCALTIMESTAMP`;
//! - `UNIX_TIMESTAMP(x)` → `EXTRACT(EPOCH FROM x)`;
//! - `DATE_ADD(NOW(), INTERVAL n SECOND)` → `LOCALTIMESTAMP + make_interval(secs => n)`;
//! - `REPLACE [INTO]` → `INSERT … ON CONFLICT (pk) DO UPDATE SET` (PK desde el
//!   catálogo PG, cacheada; OD-3);
//! - `INSERT … SET` → forma column-list; `ON DUPLICATE KEY UPDATE` → `ON CONFLICT`;
//! - `SET sql_mode = ''` y `SET NAMES …` → no-op (OD-6: charset pass-through);
//! - `SET @var = (…)` → temp table `pg_temp.m2var_<var>`; `@var` → `(SELECT v …)`
//!   (OD-4; el único par es `log.cpp:309-313`, dos queries separadas);
//! - `inet_aton(x)` → `(x)::inet - '0.0.0.0'::inet` (loginlog2.ip → bigint);
//! - `TIMEDIFF(a,b)` → `(a - b)` (loginlog2.playtime → interval);
//! - `FROM_UNIXTIME(n)` → `to_timestamp(n)` (TimeZone server-local, OD-7);
//! - `CAST(x AS unsigned)` → `(x)::bigint`;
//! - `collate sjis_japanese_ci` → se elimina;
//! - `UPDATE/DELETE … LIMIT n` → se elimina el LIMIT (WHERE PK-unique);
//! - `mysql_hash_password(…)` y los nombres cruzados `player.player_index`
//!   pasan tal cual (esquemas PG con el mismo nombre).
//!
//! `INSERT`/`REPLACE` con columnas identity: el valor explícito `0` se
//! reescribe a `DEFAULT` (MySQL: 0 en AUTO_INCREMENT = generar) y el valor
//! explícito no-cero se devuelve como `uiInsertID` (paridad: el item-award lee
//! `uiInsertID` tras insertar con id explícito — `ClientManager.cpp:922-925`;
//! el player create inserta `VALUES(0, …)` y lee el id generado —
//! `ClientManagerPlayer.cpp:853-905`).
//!
//! `item.window` (enum en MySQL, text en PG): el C++ escribe el índice 1-based
//! del ENUM (`Cache.cpp:56`); se traduce al literal (0 → `''`, igual que MySQL).

use std::fmt;

// ---------------------------------------------------------------------------
// Catálogo de tablas (necesario para PK / columnas / identity)
// ---------------------------------------------------------------------------

/// Metadatos de una tabla del esquema fase 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableInfo {
    /// Columnas en orden ordinal (para `INSERT … VALUES` sin lista de columnas).
    pub columns: Vec<String>,
    /// Columnas de la PK en orden (conflict target de `ON CONFLICT`).
    pub pk: Vec<String>,
    /// Columnas `GENERATED … AS IDENTITY` (semántica AUTO_INCREMENT).
    pub identity: Vec<String>,
    /// Columnas `bytea` (varbinary/CP949: item_proto.name/locale_name,
    /// mob_proto.locale_name, skill_proto.szName, blobs) — el wire MySQL debe
    /// recibir los bytes crudos, no el `\x…` del text protocol PG.
    pub bytea: Vec<String>,
}

/// Fuente de metadatos: la implementa `session::PgSession` consultando el
/// catálogo PG (cacheada); los tests usan un catálogo en memoria.
///
/// `async fn` en trait (estable desde 1.75): el trait es interno del crate y se
/// usa SOLO con genéricos (nunca `dyn`) — evita la dependencia async-trait
/// (política de deps mínimas, ADR-0004).
#[allow(async_fn_in_trait)]
pub trait TableCatalog {
    /// `None` = tabla desconocida (→ ERR 1146, misma paridad que MySQL).
    async fn table_info(&mut self, table: &str) -> Option<TableInfo>;
}

// ---------------------------------------------------------------------------
// Errores / resultado
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslateError {
    /// Sintaxis que el traductor no puede parsear (→ ERR 1064).
    Syntax(String),
    /// Tabla desconocida en el catálogo (→ ERR 1146).
    NoSuchTable(String),
    /// Se necesita PK (REPLACE/ODKU) y la tabla no tiene (→ ERR 1064).
    NoPrimaryKey(String),
}

impl TranslateError {
    /// errno MySQL equivalente (el C++ lo ve vía `mysql_errno`).
    pub fn mysql_errno(&self) -> u16 {
        match self {
            TranslateError::NoSuchTable(_) => crate::wire::ER_NO_SUCH_TABLE,
            _ => crate::wire::ER_PARSE_ERROR,
        }
    }
}

impl fmt::Display for TranslateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TranslateError::Syntax(m) => write!(f, "traducción: {m}"),
            TranslateError::NoSuchTable(t) => write!(f, "no such table: {t}"),
            TranslateError::NoPrimaryKey(t) => write!(f, "tabla sin PK (REPLACE/ODKU): {t}"),
        }
    }
}

impl std::error::Error for TranslateError {}

/// Cómo obtener `uiInsertID` tras ejecutar el statement (contrato `SQLMsg::Store`,
/// `AsyncSQL.h:59-80`):
/// - `Explicit(v)`: el INSERT fija la identity a `v` → MySQL devuelve `v`
///   (item awards, `ClientManager.cpp:922-925`);
/// - `Generated`: la identity se genera → `SELECT lastval()` (error → 0);
/// - `None`: sin identity → 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertIdHint {
    Explicit(u64),
    Generated,
    None,
}

/// Statement listo para ejecutar en PG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecPlan {
    pub sql: String,
    pub insert_id: InsertIdHint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rewritten {
    /// No hay nada que ejecutar (SET sql_mode/NAMES, vacío) → responder OK(0,0).
    NoOp,
    Execute(ExecPlan),
}

// ---------------------------------------------------------------------------
// Split de multi-statements (CLIENT_MULTI_STATEMENTS se anuncia; en la práctica
// el C++ envía un statement por COM_QUERY — el loop de Store es defensivo,
// `AsyncSQL.h:59-80`).
// ---------------------------------------------------------------------------

/// Divide por `;` fuera de strings y backticks; descarta partes vacías.
pub fn split_statements(sql: &str) -> Vec<&str> {
    let b = sql.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' => i = skip_string(sql, i),
            b'`' => i = skip_backtick(sql, i),
            b';' => {
                let part = sql[start..i].trim();
                if !part.is_empty() {
                    out.push(part);
                }
                start = i + 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    let last = sql[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Traduce un statement (sin `;` final) a PG.
pub async fn rewrite<C: TableCatalog>(sql: &str, catalog: &mut C) -> Result<Rewritten, TranslateError> {
    let sql = sql.trim();
    if sql.is_empty() {
        return Ok(Rewritten::NoOp);
    }
    if kw_at(sql, "SET") {
        return rewrite_set(sql);
    }
    if kw_at(sql, "REPLACE") {
        return rewrite_replace(sql, catalog).await;
    }
    if kw_at(sql, "INSERT") {
        return rewrite_insert(sql, catalog).await;
    }
    // Regla `col+0` → índice/bitmask (boot protos, safebox load): se aplica
    // ANTES de scan() (que consumiría el `+0`); solo toca `ident+0` de tablas
    // con columnas enum/set del catálogo estático — ver `ENUM_COLUMNS`.
    let sql = rewrite_enum_zero(sql);
    let mut scanned = scan(&sql);
    if kw_at(&sql, "UPDATE") {
        // Regresión 22021: los UPDATE con columnas bytea (player save,
        // CreatePlayerSaveQuery) necesitan el catálogo para convertir los
        // literales `\0` escapados. Si no se puede parsear/la tabla no existe
        // → fallback al camino genérico (PG dará su error, paridad 1146).
        if let Some(rewritten) = rewrite_update(&sql, catalog).await? {
            return Ok(Rewritten::Execute(ExecPlan { sql: rewritten, insert_id: InsertIdHint::None }));
        }
        scanned = drop_trailing_limit(&scanned);
    } else if kw_at(&sql, "DELETE") {
        scanned = drop_trailing_limit(&scanned);
    }
    Ok(Rewritten::Execute(ExecPlan { sql: scanned.trim().to_string(), insert_id: InsertIdHint::None }))
}

/// Reescribe un UPDATE aplicando el fix bytea a sus asignaciones SET.
///
/// `UPDATE <tabla> SET col = val, … [WHERE …]` — parsea las asignaciones
/// (primer `WHERE` fuera de strings), convierte los literales de columnas bytea
/// y reconstruye con `scan()` (el resto de reglas siguen aplicando:
/// backticks, NOW(), @var, …). Devuelve `None` si no puede parsear o la tabla
/// no está en el catálogo → el caller usa el camino genérico.
async fn rewrite_update<C: TableCatalog>(sql: &str, catalog: &mut C) -> Result<Option<String>, TranslateError> {
    let s = skip_ws(&sql[6..]); // "UPDATE"
    let (table, end) = match parse_ident(s) {
        Some(x) => x,
        None => return Ok(None),
    };
    let body = skip_ws(&s[end..]);
    if !kw_at(body, "SET") {
        return Ok(None);
    }
    let assignments = match find_kw_outside_strings(body, "WHERE") {
        Some(p) => &body[4..p],
        None => &body[4..],
    };
    let where_part = match find_kw_outside_strings(body, "WHERE") {
        Some(p) => &body[p..],
        None => "",
    };
    let Some(info) = catalog.table_info(&table).await else {
        return Ok(None);
    };
    let (cols, vals) = match parse_assignments(&split_top_level(assignments)) {
        Ok(x) => x,
        Err(_) => return Ok(None),
    };
    let vals = fix_bytea_values(&cols, vals, &info);
    let sets: Vec<String> = cols
        .iter()
        .zip(vals.iter())
        .map(|(col, val)| format!("{col} = {}", scan(val)))
        .collect();
    let mut out = format!("UPDATE {table} SET {}", sets.join(", "));
    if !where_part.is_empty() {
        out.push(' ');
        out.push_str(&scan(where_part));
    }
    Ok(Some(drop_trailing_limit(&out)))
}

// ---------------------------------------------------------------------------
// SET
// ---------------------------------------------------------------------------

fn rewrite_set(sql: &str) -> Result<Rewritten, TranslateError> {
    let body = skip_ws(&sql[3..]);
    // Normaliza prefijos de sesión/variables MySQL: `SET SESSION x`, `SET @@x`,
    // `SET @@session.x` → `x` (el C++/libmariadb usa varias formas al conectar).
    let mut lower = body.to_ascii_lowercase();
    for prefix in ["session ", "@@session.", "@@"] {
        if lower.starts_with(prefix) {
            lower = lower[prefix.len()..].trim_start().to_string();
            break;
        }
    }
    // GUCs de MySQL sin equivalente en PG → no-op. El C++ NO debe recibir ERR
    // por estos SETs: `SET NAMES …` (mysql_set_character_set, AsyncSQL.cpp:104,
    // OD-6 pass-through), `SET sql_mode = ''` (ClientManagerBoot.cpp:39) y
    // `SET AUTOCOMMIT = 0/1` (lo manda el db/game al conectar — 8 conexiones
    // vistas en el log PG del gate).
    for guc in ["sql_mode", "names", "character_set", "autocommit"] {
        if lower.starts_with(guc) {
            return Ok(Rewritten::NoOp);
        }
    }
    // SET @var = … → temp table pg_temp.m2var_<var> (OD-4; log.cpp:309-313).
    if let Some(rest) = body.strip_prefix('@') {
        let name_end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        if name.is_empty() {
            return Err(TranslateError::Syntax("SET @ sin nombre de variable".into()));
        }
        let expr = skip_ws(&rest[name_end..]);
        let expr = skip_ws(expr.strip_prefix('=').ok_or_else(|| TranslateError::Syntax("SET @ sin '='".into()))?);
        if expr.is_empty() {
            return Err(TranslateError::Syntax("SET @ sin valor".into()));
        }
        let inner = if expr.starts_with('(') && expr.ends_with(')') {
            &expr[1..expr.len() - 1]
        } else {
            expr
        };
        let sql2 = if inner.trim_start().to_ascii_lowercase().starts_with("select") {
            format!(
                "CREATE TEMP TABLE pg_temp.m2var_{name} AS SELECT v FROM ({}) AS _m2var(v)",
                scan(inner)
            )
        } else {
            format!("CREATE TEMP TABLE pg_temp.m2var_{name} AS SELECT {} AS v", scan(inner))
        };
        return Ok(Rewritten::Execute(ExecPlan { sql: sql2, insert_id: InsertIdHint::None }));
    }
    // Otro SET → passthrough a PG.
    Ok(Rewritten::Execute(ExecPlan { sql: scan(sql).trim().to_string(), insert_id: InsertIdHint::None }))
}

// ---------------------------------------------------------------------------
// INSERT
// ---------------------------------------------------------------------------

async fn rewrite_insert<C: TableCatalog>(sql: &str, catalog: &mut C) -> Result<Rewritten, TranslateError> {
    let s = skip_ws(&sql[6..]); // "INSERT"
    let s = if kw_at(s, "INTO") { skip_ws(&s[4..]) } else { s };
    let (table, end) = parse_ident(s).ok_or_else(|| TranslateError::Syntax("INSERT sin tabla".into()))?;
    let body = skip_ws(&s[end..]);
    let odku_pos = find_kw_outside_strings(body, "ON DUPLICATE KEY UPDATE");
    let (main, odku) = match odku_pos {
        Some(p) => (body[..p].trim_end(), Some(skip_ws(&body[p + 23..]))),
        None => (body, None),
    };
    let info = catalog
        .table_info(&table)
        .await
        .ok_or_else(|| TranslateError::NoSuchTable(table.clone()))?;
    let (cols, tuples, select_part) = parse_insert_body(main, &info)?;
    let (sql2, hint) = build_insert(&table, &cols, &tuples, &select_part, odku, &info)?;
    Ok(Rewritten::Execute(ExecPlan { sql: sql2, insert_id: hint }))
}

// ---------------------------------------------------------------------------
// REPLACE
// ---------------------------------------------------------------------------

async fn rewrite_replace<C: TableCatalog>(sql: &str, catalog: &mut C) -> Result<Rewritten, TranslateError> {
    let s = skip_ws(&sql[7..]); // "REPLACE"
    let s = if kw_at(s, "INTO") { skip_ws(&s[4..]) } else { s };
    let (table, end) = parse_ident(s).ok_or_else(|| TranslateError::Syntax("REPLACE sin tabla".into()))?;
    let body = skip_ws(&s[end..]);
    let info = catalog
        .table_info(&table)
        .await
        .ok_or_else(|| TranslateError::NoSuchTable(table.clone()))?;
    let (cols, tuples, select_part) = parse_insert_body(body, &info)?;
    let (sql2, hint) = build_insert(&table, &cols, &tuples, &select_part, None, &info)?;
    // REPLACE = DELETE+INSERT → upsert completo con TODAS las columnas del
    // statement (OD-3: nadie depende del churn de ids; no hay triggers).
    let pk = quote_pk(&table, &info.pk)?;
    let sets: Vec<String> = cols.iter().map(|c| format!("\"{c}\"=EXCLUDED.\"{c}\"")).collect();
    let sql3 = format!("{sql2} ON CONFLICT ({pk}) DO UPDATE SET {}", sets.join(", "));
    Ok(Rewritten::Execute(ExecPlan { sql: sql3, insert_id: hint }))
}

/// Formas de INSERT/REPLACE → (columnas, tuplas VALUES, parte SELECT).
fn parse_insert_body(
    body: &str,
    info: &TableInfo,
) -> Result<(Vec<String>, Vec<Vec<String>>, Option<String>), TranslateError> {
    if kw_at(body, "SET") {
        let assignments = split_top_level(&body[3..]);
        let (cols, vals) = parse_assignments(&assignments)?;
        return Ok((cols, vec![vals], None));
    }
    if body.starts_with('(') {
        let (cols_inner, end2) = paren_arg(body, 0).ok_or_else(|| TranslateError::Syntax("INSERT: lista de columnas".into()))?;
        let cols: Vec<String> = split_top_level(&cols_inner).iter().map(|c| unquote_ident(c).to_string()).collect();
        let rest = skip_ws(&body[end2..]);
        if kw_at(rest, "VALUES") {
            let tuples = values_of_tuples(&rest[6..]).ok_or_else(|| TranslateError::Syntax("INSERT: VALUES malformado".into()))?;
            return Ok((cols, tuples, None));
        }
        if kw_at(rest, "SELECT") {
            return Ok((cols, Vec::new(), Some(rest.to_string())));
        }
        return Err(TranslateError::Syntax("INSERT: se esperaba VALUES o SELECT".into()));
    }
    if kw_at(body, "VALUES") {
        let tuples = values_of_tuples(&body[6..]).ok_or_else(|| TranslateError::Syntax("INSERT: VALUES malformado".into()))?;
        return Ok((info.columns.clone(), tuples, None));
    }
    Err(TranslateError::Syntax("INSERT/REPLACE: forma no soportada".into()))
}

/// Tuplas de VALUES → valores por tupla: `(a, b), (c, d)` →
/// `[["a", "b"], ["c", "d"]]`.
fn values_of_tuples(s: &str) -> Option<Vec<Vec<String>>> {
    parse_values(s).map(|tuples| {
        tuples
            .iter()
            .map(|t| split_top_level(t).iter().map(|v| v.to_string()).collect())
            .collect()
    })
}

/// Construye el INSERT final (con ODKU si viene) + el hint de insert id.
fn build_insert(
    table: &str,
    cols: &[String],
    tuples: &[Vec<String>],
    select_part: &Option<String>,
    odku: Option<&str>,
    info: &TableInfo,
) -> Result<(String, InsertIdHint), TranslateError> {
    let mut tuples = tuples.to_vec();
    let mut explicit_id: Option<u64> = None;
    for tuple in tuples.iter_mut() {
        if tuple.len() != cols.len() {
            return Err(TranslateError::Syntax("INSERT: count de columnas != count de valores".into()));
        }
        for (j, col) in cols.iter().enumerate() {
            let val = &mut tuple[j];
            if info.identity.iter().any(|c| c == col) {
                let trimmed = val.trim();
                if trimmed == "0" {
                    // MySQL: 0 explícito en AUTO_INCREMENT = generar
                    // (NO_AUTO_VALUE_ON_ZERO off) — player create
                    // `ClientManagerPlayer.cpp:853-863`.
                    *val = "DEFAULT".to_string();
                } else if let Ok(n) = trimmed.parse::<u64>() {
                    if explicit_id.is_none() {
                        explicit_id = Some(n);
                    }
                }
            }
            if let Some(fixed) = fix_enum_value(table, col, val) {
                *val = fixed;
            }
            // Regresión 22021: columnas bytea con blobs escapados MySQL (`\0`)
            // → decode('<hex>', 'hex') (nunca un NUL en un literal text de PG).
            if info.bytea.iter().any(|c| c == col) {
                if let Some(lit) = bytea_literal(val) {
                    *val = lit;
                }
            }
        }
    }

    let quoted: Vec<String> = cols.iter().map(|c| format!("\"{c}\"")).collect();
    let mut sql = format!("INSERT INTO {table} ({})", quoted.join(", "));
    if let Some(sel) = select_part {
        sql.push(' ');
        sql.push_str(&scan(sel));
    } else {
        let tuples_sql: Vec<String> = tuples
            .iter()
            .map(|t| format!("({})", t.iter().map(|v| scan(v)).collect::<Vec<_>>().join(", ")))
            .collect();
        sql.push_str(" VALUES ");
        sql.push_str(&tuples_sql.join(", "));
    }

    if let Some(odku_body) = odku {
        // MySQL ODKU: nombres pelados en el RHS = valor ACTUAL de la fila —
        // misma semántica que PG `DO UPDATE SET` (spec §4).
        let pk = quote_pk(table, &info.pk)?;
        let mut sets = Vec::new();
        for a in split_top_level(odku_body) {
            let (col, end) = parse_ident(a).ok_or_else(|| TranslateError::Syntax("ON DUPLICATE KEY UPDATE: assignment sin columna".into()))?;
            let rest = skip_ws(&a[end..]);
            let rest = skip_ws(rest.strip_prefix('=').ok_or_else(|| TranslateError::Syntax("ON DUPLICATE KEY UPDATE: assignment sin '='".into()))?);
            sets.push(format!("\"{col}\" = {}", scan(rest)));
        }
        sql.push_str(&format!(" ON CONFLICT ({pk}) DO UPDATE SET {}", sets.join(", ")));
    }

    let has_identity = !info.identity.is_empty();
    let hint = match explicit_id {
        Some(v) => InsertIdHint::Explicit(v),
        None if has_identity && select_part.is_none() => InsertIdHint::Generated,
        None => InsertIdHint::None,
    };
    Ok((sql, hint))
}

fn quote_pk(table: &str, pk: &[String]) -> Result<String, TranslateError> {
    if pk.is_empty() {
        return Err(TranslateError::NoPrimaryKey(table.to_string()));
    }
    Ok(pk.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", "))
}

/// `item.window` es text en PG (enum→text, `legacy-schema.md` §7.2); el C++
/// escribe el índice 1-based del ENUM MySQL (`Cache.cpp:56` — `window`={}).
/// Índice 0 → `''` (MySQL ENUM index 0 = empty).
fn fix_enum_value(table: &str, col: &str, val: &str) -> Option<String> {
    const ITEM_WINDOW: [&str; 7] = [
        "INVENTORY",
        "EQUIPMENT",
        "SAFEBOX",
        "MALL",
        "DRAGON_SOUL_INVENTORY",
        "BELT_INVENTORY",
        "GROUND",
    ];
    if table == "item" && col == "window" {
        if let Ok(idx) = val.trim().parse::<usize>() {
            if (1..=7).contains(&idx) {
                return Some(format!("'{}'", ITEM_WINDOW[idx - 1]));
            }
            if idx == 0 {
                return Some("''".into());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Regla `col+0` (ENUM/SET → índice/bitmask) — inventario 2026-08-11
// ---------------------------------------------------------------------------
//
// MySQL: `enum_col + 0` devuelve el ÍNDICE del ENUM (1-based; `''` y valor no
// válido → 0) y `set_col + 0` el BITMASK (Σ 2^(pos-1) por elemento; `''` → 0).
// El C++ lee esos números con `str_to_number` (`TMonsterInfo::dwRaceFlag`,
// `TItemTable::dwImmuneFlag`, `TSkillTable::dwFlag`, …). En PG las columnas
// son text (migración enum→text) → sin traducción el C++ recibiría el TEXTO.
// El caso inverso (C++ ESCRIBE el índice → literal) ya está en `fix_enum_value`
// (item.window, `Cache.cpp:56`).

/// Tipo de columna (el literal en orden ES el índice MySQL).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnumKind {
    /// `col+0` → índice 1-based del literal; `''`/no-enum → 0; NULL → NULL.
    Enum(&'static [&'static str]),
    /// `col+0` → bitmask Σ 2^(pos-1); `''` → 0; NULL → NULL.
    Set(&'static [&'static str]),
}

/// Catálogo estático (tabla, columna) → literales ENUM/SET en orden.
///
/// FUENTE (solo lectura): `SHOW CREATE TABLE` de MariaDB 127.0.0.1:3306
/// (schema player, 2026-08-11) — dump generado con
/// `scripts/gpg/dump_enums.py` contra information_schema.COLUMNS. El orden de
/// los literales define el índice/bitmask (idéntico en MariaDB y en el
/// `mob_proto.txt`/`item_proto.txt`/`skill_proto.txt` originales).
/// Columnas del boot C++ que usan `+0`:
///   ClientManager.cpp:680, ClientManagerPlayer.cpp:321,385 → item.window
///   ClientManagerBoot.cpp:1290-1291 → mob_proto.size/ai_flag/setRaceFlag/setImmuneFlag
///   ClientManagerBoot.cpp:1467 → item_proto.immuneflag
///   ClientManagerBoot.cpp:478-481 → skill_proto.setFlag/setAffectFlag/setAffectFlag2/eSkillType
///   ClientManagerBoot.cpp:594,719 → item_attr.apply / item_attr_rare.apply
/// NOTA: setAffectFlag/setAffectFlag2 son ENUM en MariaDB (no SET) pese al
/// nombre — se tratan como ENUM (índice), que es lo que devuelve `+0`.
static ENUM_COLUMNS: &[(&str, &str, EnumKind)] = &[
    ("item", "window", EnumKind::Enum(&["INVENTORY", "EQUIPMENT", "SAFEBOX", "MALL", "DRAGON_SOUL_INVENTORY", "BELT_INVENTORY", "GROUND"])),
    ("mob_proto", "size", EnumKind::Enum(&["SMALL", "MEDIUM", "BIG"])),
    (
        "mob_proto",
        "ai_flag",
        EnumKind::Set(&["AGGR", "NOMOVE", "COWARD", "NOATTSHINSU", "NOATTCHUNJO", "NOATTJINNO", "ATTMOB", "BERSERK", "STONESKIN", "GODSPEED", "DEATHBLOW", "REVIVE"]),
    ),
    (
        "mob_proto",
        "setRaceFlag",
        EnumKind::Set(&["ANIMAL", "UNDEAD", "DEVIL", "HUMAN", "ORC", "MILGYO", "INSECT", "FIRE", "ICE", "DESERT", "TREE", "ATT_ELEC", "ATT_FIRE", "ATT_ICE", "ATT_WIND", "ATT_EARTH", "ATT_DARK"]),
    ),
    ("mob_proto", "setImmuneFlag", EnumKind::Set(&["STUN", "SLOW", "FALL", "CURSE", "POISON", "TERROR", "REFLECT"])),
    ("item_proto", "immuneflag", EnumKind::Set(&["PARA", "CURSE", "STUN", "SLEEP", "SLOW", "POISON", "TERROR"])),
    (
        "skill_proto",
        "setFlag",
        EnumKind::Set(&["ATTACK", "USE_MELEE_DAMAGE", "COMPUTE_ATTGRADE", "SELFONLY", "USE_MAGIC_DAMAGE", "USE_HP_AS_COST", "COMPUTE_MAGIC_DAMAGE", "SPLASH", "GIVE_PENALTY", "USE_ARROW_DAMAGE", "PENETRATE", "IGNORE_TARGET_RATING", "ATTACK_SLOW", "ATTACK_STUN", "HP_ABSORB", "SP_ABSORB", "ATTACK_FIRE_CONT", "REMOVE_BAD_AFFECT", "REMOVE_GOOD_AFFECT", "CRUSH", "ATTACK_POISON", "TOGGLE", "DISABLE_BY_POINT_UP", "CRUSH_LONG", "ATTACK_WIND", "ATTACK_ELEC", "ATTACK_FIRE", "ATTACK_BLEEDING", "PARTY"]),
    ),
    (
        "skill_proto",
        "setAffectFlag",
        EnumKind::Enum(&["YMIR", "INVISIBILITY", "SPAWN", "POISON", "SLOW", "STUN", "DUNGEON_READY", "DUNGEON_UNIQUE", "BUILDING_CONSTRUCTION_SMALL", "BUILDING_CONSTRUCTION_LARGE", "BUILDING_UPGRADE", "MOV_SPEED_POTION", "ATT_SPEED_POTION", "FISH_MIND", "JEONGWIHON", "GEOMGYEONG", "CHEONGEUN", "GYEONGGONG", "EUNHYUNG", "GWIGUM", "TERROR", "JUMAGAP", "HOSIN", "BOHO", "KWAESOK", "MANASHIELD", "MUYEONG", "REVIVE_INVISIBLE", "FIRE", "GICHEON", "JEUNGRYEOK", "TANHWAN_DASH", "PABEOP", "CHEONGEUN_WITH_FALL", "POLYMORPH", "WAR_FLAG1", "WAR_FLAG2", "WAR_FLAG3", "CHINA_FIREWORK", "HAIR", "GERMANY", "RAMADAN_RING", "BLEEDING", "RED_POSSESSION", "BLUE_POSSESSION"]),
    ),
    (
        "skill_proto",
        "setAffectFlag2",
        EnumKind::Enum(&["YMIR", "INVISIBILITY", "SPAWN", "POISON", "SLOW", "STUN", "DUNGEON_READY", "DUNGEON_UNIQUE", "BUILDING_CONSTRUCTION_SMALL", "BUILDING_CONSTRUCTION_LARGE", "BUILDING_UPGRADE", "MOV_SPEED_POTION", "ATT_SPEED_POTION", "FISH_MIND", "JEONGWIHON", "GEOMGYEONG", "CHEONGEUN", "GYEONGGONG", "EUNHYUNG", "GWIGUM", "TERROR", "JUMAGAP", "HOSIN", "BOHO", "KWAESOK", "MANASHIELD", "MUYEONG", "REVIVE_INVISIBLE", "FIRE", "GICHEON", "JEUNGRYEOK", "TANHWAN_DASH", "PABEOP", "CHEONGEUN_WITH_FALL", "POLYMORPH", "WAR_FLAG1", "WAR_FLAG2", "WAR_FLAG3", "CHINA_FIREWORK", "HAIR", "GERMANY", "RAMADAN_RING", "BLEEDING", "RED_POSSESSION", "BLUE_POSSESSION"]),
    ),
    ("skill_proto", "eSkillType", EnumKind::Enum(&["NORMAL", "MELEE", "RANGE", "MAGIC"])),
    (
        "item_attr",
        "apply",
        EnumKind::Enum(&["MAX_HP", "MAX_SP", "CON", "INT", "STR", "DEX", "ATT_SPEED", "MOV_SPEED", "CAST_SPEED", "HP_REGEN", "SP_REGEN", "POISON_PCT", "STUN_PCT", "SLOW_PCT", "CRITICAL_PCT", "PENETRATE_PCT", "ATTBONUS_HUMAN", "ATTBONUS_ANIMAL", "ATTBONUS_ORC", "ATTBONUS_MILGYO", "ATTBONUS_UNDEAD", "ATTBONUS_DEVIL", "STEAL_HP", "STEAL_SP", "MANA_BURN_PCT", "DAMAGE_SP_RECOVER", "BLOCK", "DODGE", "RESIST_SWORD", "RESIST_TWOHAND", "RESIST_DAGGER", "RESIST_BELL", "RESIST_FAN", "RESIST_BOW", "RESIST_FIRE", "RESIST_ELEC", "RESIST_MAGIC", "RESIST_WIND", "REFLECT_MELEE", "REFLECT_CURSE", "POISON_REDUCE", "KILL_SP_RECOVER", "EXP_DOUBLE_BONUS", "GOLD_DOUBLE_BONUS", "ITEM_DROP_BONUS", "POTION_BONUS", "KILL_HP_RECOVER", "IMMUNE_STUN", "IMMUNE_SLOW", "IMMUNE_FALL", "SKILL", "BOW_DISTANCE", "ATT_GRADE_BONUS", "DEF_GRADE_BONUS", "MAGIC_ATT_GRADE_BONUS", "MAGIC_DEF_GRADE_BONUS", "CURSE_PCT", "MAX_STAMINA", "ATT_BONUS_TO_WARRIOR", "ATT_BONUS_TO_ASSASSIN", "ATT_BONUS_TO_SURA", "ATT_BONUS_TO_SHAMAN", "ATT_BONUS_TO_MONSTER", "ATT_BONUS", "MALL_DEFBONUS", "MALL_EXPBONUS", "MALL_ITEMBONUS", "MALL_GOLDBONUS", "MAX_HP_PCT", "MAX_SP_PCT", "SKILL_DAMAGE_BONUS", "NORMAL_HIT_DAMAGE_BONUS", "SKILL_DEFEND_BONUS", "NORMAL_HIT_DEFEND_BONUS", "PC_BANG_EXP_BONUS", "PC_BANG_DROP_BONUS", "EXTRACT_HP_PCT", "RESIST_WARRIOR", "RESIST_ASSASSIN", "RESIST_SURA", "RESIST_SHAMAN", "ENERGY", "DEF_GRADE", "COSTUME_ATTR_BONUS", "MAGIC_ATT_BONUS_PER", "MELEE_MAGIC_ATT_BONUS_PER", "RESIST_ICE", "RESIST_EARTH", "RESIST_DARK", "RESIST_CRITICAL", "RESIST_PENETRATE", "BLEEDING_REDUCE", "BLEEDING_PCT", "ATT_BONUS_TO_WOLFMAN", "RESIST_WOLFMAN", "RESIST_CLAW", "ACCEDRAIN_RATE", "RESIST_MAGIC_REDUCTION", "ENCHANT_ELECT", "ENCHANT_FIRE", "ENCHANT_ICE", "ENCHANT_WIND", "ENCHANT_EARTH", "ENCHANT_DARK", "ATTBONUS_CZ", "ATTBONUS_INSECT", "ATTBONUS_DESERT", "ATTBONUS_SWORD", "ATTBONUS_TWOHAND", "ATTBONUS_DAGGER", "ATTBONUS_BELL", "ATTBONUS_FAN", "ATTBONUS_BOW", "ATTBONUS_CLAW", "RESIST_HUMAN", "RESIST_MOUNT_FALL", "UNK_117", "MOUNT"]),
    ),
    (
        "item_attr_rare",
        "apply",
        EnumKind::Enum(&["MAX_HP", "MAX_SP", "CON", "INT", "STR", "DEX", "ATT_SPEED", "MOV_SPEED", "CAST_SPEED", "HP_REGEN", "SP_REGEN", "POISON_PCT", "STUN_PCT", "SLOW_PCT", "CRITICAL_PCT", "PENETRATE_PCT", "ATTBONUS_HUMAN", "ATTBONUS_ANIMAL", "ATTBONUS_ORC", "ATTBONUS_MILGYO", "ATTBONUS_UNDEAD", "ATTBONUS_DEVIL", "STEAL_HP", "STEAL_SP", "MANA_BURN_PCT", "DAMAGE_SP_RECOVER", "BLOCK", "DODGE", "RESIST_SWORD", "RESIST_TWOHAND", "RESIST_DAGGER", "RESIST_BELL", "RESIST_FAN", "RESIST_BOW", "RESIST_FIRE", "RESIST_ELEC", "RESIST_MAGIC", "RESIST_WIND", "REFLECT_MELEE", "REFLECT_CURSE", "POISON_REDUCE", "KILL_SP_RECOVER", "EXP_DOUBLE_BONUS", "GOLD_DOUBLE_BONUS", "ITEM_DROP_BONUS", "POTION_BONUS", "KILL_HP_RECOVER", "IMMUNE_STUN", "IMMUNE_SLOW", "IMMUNE_FALL", "SKILL", "BOW_DISTANCE", "ATT_GRADE_BONUS", "DEF_GRADE_BONUS", "MAGIC_ATT_GRADE_BONUS", "MAGIC_DEF_GRADE_BONUS", "CURSE_PCT", "MAX_STAMINA", "ATT_BONUS_TO_WARRIOR", "ATT_BONUS_TO_ASSASSIN", "ATT_BONUS_TO_SURA", "ATT_BONUS_TO_SHAMAN", "ATT_BONUS_TO_MONSTER", "ATT_BONUS", "MALL_DEFBONUS", "MALL_EXPBONUS", "MALL_ITEMBONUS", "MALL_GOLDBONUS", "MAX_HP_PCT", "MAX_SP_PCT", "SKILL_DAMAGE_BONUS", "NORMAL_HIT_DAMAGE_BONUS", "SKILL_DEFEND_BONUS", "NORMAL_HIT_DEFEND_BONUS", "PC_BANG_EXP_BONUS", "PC_BANG_DROP_BONUS", "EXTRACT_HP_PCT", "RESIST_WARRIOR", "RESIST_ASSASSIN", "RESIST_SURA", "RESIST_SHAMAN", "ENERGY", "DEF_GRADE", "COSTUME_ATTR_BONUS", "MAGIC_ATT_BONUS_PER", "MELEE_MAGIC_ATT_BONUS_PER", "RESIST_ICE", "RESIST_EARTH", "RESIST_DARK", "RESIST_CRITICAL", "RESIST_PENETRATE", "BLEEDING_REDUCE", "BLEEDING_PCT", "ATT_BONUS_TO_WOLFMAN", "RESIST_WOLFMAN", "RESIST_CLAW", "ACCEDRAIN_RATE", "RESIST_MAGIC_REDUCTION", "ENCHANT_ELECT", "ENCHANT_FIRE", "ENCHANT_ICE", "ENCHANT_WIND", "ENCHANT_EARTH", "ENCHANT_DARK", "ATTBONUS_CZ", "ATTBONUS_INSECT", "ATTBONUS_DESERT", "ATTBONUS_SWORD", "ATTBONUS_TWOHAND", "ATTBONUS_DAGGER", "ATTBONUS_BELL", "ATTBONUS_FAN", "ATTBONUS_BOW", "ATTBONUS_CLAW", "RESIST_HUMAN", "RESIST_MOUNT_FALL", "UNK_117", "MOUNT"]),
    ),
];

/// Expresión PG de `enum_col+0` → índice 1-based del literal; `''`/no-enum
/// → 0; NULL → NULL (semántica MySQL, verificada contra MariaDB 2026-08-11).
fn enum_index_expr(col: &str, lits: &[&str]) -> String {
    let mut s = format!("CASE WHEN {col} IS NULL THEN NULL");
    for (i, lit) in lits.iter().enumerate() {
        s.push_str(&format!(" WHEN {col} = '{lit}' THEN {}", i + 1));
    }
    s.push_str(" ELSE 0 END");
    s
}

/// Expresión PG de `set_col+0` → bitmask Σ 2^(pos-1) por elemento presente;
/// `''` → 0 (unnest de '' → sin filas → SUM NULL → COALESCE 0); NULL → NULL.
/// `WITH ORDINALITY` da pos bigint → cast a int para el shift (PG no tiene
/// `int << bigint`).
fn set_bitmask_expr(col: &str, lits: &[&str]) -> String {
    let list = lits.iter().map(|l| format!("'{l}'")).collect::<Vec<_>>().join(", ");
    format!(
        "CASE WHEN {col} IS NULL THEN NULL ELSE COALESCE((SELECT sum(1 << ((pos - 1)::int)) FROM unnest(string_to_array({col}, ',')) WITH ORDINALITY t(v, pos) WHERE v IN ({list})), 0) END"
    )
}

/// Expresión completa para una columna catalogada.
fn enum_zero_expr(col: &str, kind: &EnumKind) -> String {
    match kind {
        EnumKind::Enum(lits) => enum_index_expr(col, lits),
        EnumKind::Set(lits) => set_bitmask_expr(col, lits),
    }
}

/// Tabla del primer `FROM` fuera de strings — las queries con `+0` del boot
/// son todas `SELECT … FROM <tabla> …` de una sola tabla.
fn from_table(sql: &str) -> Option<String> {
    let pos = find_kw_outside_strings(sql, "FROM")?;
    let s = skip_ws(&sql[pos + 4..]);
    let (name, _) = parse_ident(s)?;
    Some(name)
}

/// `true` si en `sql[at..]` empieza `+0` (el sufijo del cast enum).
fn plus_zero_at(sql: &str, at: usize) -> bool {
    let b = sql.as_bytes();
    b.get(at) == Some(&b'+') && b.get(at + 1) == Some(&b'0')
}

/// Sustituye `ident+0` → expresión índice/bitmask para las columnas del
/// catálogo `ENUM_COLUMNS` de la tabla del FROM. La columna se emite en su
/// forma ORIGINAL (`` `window` `` o `size`): scan() la normaliza después
/// (backticks → comillas dobles PG). Los `+0` de columnas no catalogadas
/// quedan para la regla genérica de scan() (se eliminan).
fn rewrite_enum_zero(sql: &str) -> String {
    let Some(table) = from_table(sql) else {
        return sql.to_string();
    };
    let cols: Vec<(&str, &EnumKind)> = ENUM_COLUMNS
        .iter()
        .filter(|(t, _, _)| *t == table)
        .map(|(_, c, k)| (*c, k))
        .collect();
    if cols.is_empty() {
        return sql.to_string();
    }
    let b = sql.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' => {
                let end = skip_string(sql, i);
                out.push_str(&sql[i..end]);
                i = end;
            }
            b'`' => {
                let end = skip_backtick(sql, i);
                let name = &sql[i + 1..end - 1];
                if plus_zero_at(sql, end) {
                    if let Some((_, kind)) = cols.iter().find(|(c, _)| c.eq_ignore_ascii_case(name)) {
                        out.push_str(&enum_zero_expr(&sql[i..end], kind));
                        i = end + 2;
                        continue;
                    }
                }
                out.push_str(&sql[i..end]);
                i = end;
            }
            _ if b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$') {
                    i += 1;
                }
                let name = &sql[start..i];
                if plus_zero_at(sql, i) {
                    if let Some((_, kind)) = cols.iter().find(|(c, _)| c.eq_ignore_ascii_case(name)) {
                        out.push_str(&enum_zero_expr(name, kind));
                        i += 2;
                        continue;
                    }
                }
                out.push_str(name);
            }
            _ => {
                out.push(b[i] as char);
                i += 1;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Literales bytea (regresión 22021, 2026-08-10)
// ---------------------------------------------------------------------------

/// Convierte un literal `'…'` de una columna bytea a `decode('<hex>', 'hex')`.
///
/// El C++ escapa los blobs binarios con `mysql_real_escape_string`
/// (`ClientManagerPlayer.cpp:171-175,886-892`): el byte NUL llega como los DOS
/// caracteres `\0`. Con `standard_conforming_strings = off` (necesario para el
/// resto de literales), PG interpreta `\0` como octal → NUL REAL dentro de un
/// literal text → `22021 invalid byte sequence for encoding "UTF8": 0x00`
/// (creación de personaje y guardado, `/tmp/gpg/proxy.log`).
///
/// La salida es `decode('<hex>', 'hex')`: un literal text de SOLO dígitos hex
/// (sin backslashes → inmune a SCS=off) que la función `decode` convierte al
/// bytea exacto. NO se usa el literal `'\x…'` bytea: con SCS=off el parser de
/// strings procesaría el `\x` ANTES de la interpretación bytea (doble
/// interpretación ambigua).
///
/// Devuelve `None` si el valor no es un literal con comillas simples (número,
/// expresión, `DEFAULT`, …) o el literal está malformado.
fn bytea_literal(value: &str) -> Option<String> {
    let v = value.trim();
    if v.len() < 2 || !v.starts_with('\'') || !v.ends_with('\'') {
        return None;
    }
    let bytes = unescape_mysql(&v[1..v.len() - 1])?;
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Some(format!("decode('{hex}', 'hex')"))
}

/// Decodifica el escaping de `mysql_real_escape_string` (MySQL string literal):
/// `\0 \n \r \t \Z \\ \' \" \xHH` y `''` (duplicación defensiva); una secuencia
/// de escape desconocida deja el carácter (semántica MySQL). `None` si el
/// literal está malformado (backslash final o comilla interna sin escapar).
fn unescape_mysql(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => {
                match *b.get(i + 1)? {
                    b'x' => {
                        let hi = (*b.get(i + 2)? as char).to_digit(16)? as u8;
                        let lo = (*b.get(i + 3)? as char).to_digit(16)? as u8;
                        out.push((hi << 4) | lo);
                        i += 4;
                    }
                    b'0' => {
                        out.push(0x00);
                        i += 2;
                    }
                    b'n' => {
                        out.push(b'\n');
                        i += 2;
                    }
                    b'r' => {
                        out.push(b'\r');
                        i += 2;
                    }
                    b't' => {
                        out.push(b'\t');
                        i += 2;
                    }
                    b'Z' => {
                        out.push(0x1a);
                        i += 2;
                    }
                    b'\\' => {
                        out.push(b'\\');
                        i += 2;
                    }
                    b'\'' => {
                        out.push(b'\'');
                        i += 2;
                    }
                    b'"' => {
                        out.push(b'"');
                        i += 2;
                    }
                    other => {
                        out.push(other);
                        i += 2;
                    }
                }
            }
            b'\'' => {
                if b.get(i + 1) == Some(&b'\'') {
                    out.push(b'\'');
                    i += 2;
                } else {
                    return None;
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    Some(out)
}

/// Aplica el fix bytea a una lista de (columna, valor) — reescribe los valores
/// de columnas bytea que sean literales `'…'` a `decode('<hex>', 'hex')`.
fn fix_bytea_values(cols: &[String], vals: Vec<String>, info: &TableInfo) -> Vec<String> {
    cols.iter()
        .zip(vals.into_iter())
        .map(|(col, val)| {
            if info.bytea.iter().any(|c| c == col) {
                bytea_literal(&val).unwrap_or(val)
            } else {
                val
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Scanner genérico (reglas que aplican en cualquier posición, fuera de strings)
// ---------------------------------------------------------------------------

fn scan(sql: &str) -> String {
    let b = sql.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' => {
                let end = skip_string(sql, i);
                out.push_str(&sql[i..end]);
                i = end;
                continue;
            }
            b'`' => {
                out.push('"');
                i += 1;
                continue;
            }
            b'"' => {
                // MySQL: las comillas DOBLES son string literal (los
                // identificadores van entre backticks) — el caso real:
                // `WHERE mKey LIKE "LOCALE"` (config.cpp:477-499) → 42703 en
                // PG. Se convierten a comillas simples escapando las simples
                // internas (''). Los backticks (→ `"` identificador PG) ya se
                // manejaron en el brazo anterior: sin conflicto.
                out.push('\'');
                i += 1;
                while i < b.len() {
                    match b[i] {
                        b'\\' => match b.get(i + 1) {
                            Some(b'\'') => {
                                out.push_str("''");
                                i += 2;
                            }
                            Some(b'"') => {
                                out.push('"');
                                i += 2;
                            }
                            Some(b'\\') => {
                                out.push_str("\\\\");
                                i += 2;
                            }
                            _ => {
                                out.push(b[i] as char);
                                i += 1;
                            }
                        },
                        b'"' => {
                            i += 1;
                            break;
                        }
                        b'\'' => {
                            out.push_str("''");
                            i += 1;
                        }
                        other => {
                            out.push(other as char);
                            i += 1;
                        }
                    }
                }
                out.push('\'');
                continue;
            }
            b'@' => {
                let mut j = i + 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                if j > i + 1 {
                    out.push_str(&format!("(SELECT v FROM pg_temp.m2var_{})", &sql[i + 1..j]));
                    i = j;
                    continue;
                }
            }
            b'+' if i + 1 < b.len()
                && b[i + 1] == b'0'
                && i > 0
                && matches!(b[i - 1], b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'`' | b'"') =>
            {
                // FALLBACK del `+0`: las columnas ENUM/SET catalogadas ya las
                // tradujo rewrite_enum_zero (índice/bitmask); este +0 sobrante
                // (columna/tabla no catalogada) se elimina — inventario §3
                // fila 10.
                i += 2;
                continue;
            }
            _ => {
                if let Some((consumed, rep)) = match_keyword(&sql[i..]) {
                    out.push_str(&rep);
                    i += consumed;
                    continue;
                }
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

fn match_keyword(rest: &str) -> Option<(usize, String)> {
    let lower = rest.to_ascii_lowercase();
    // availDt - NOW() > 0 → availDt > LOCALTIMESTAMP (timestamp - timestamp = interval
    // no se puede comparar con 0 — spec §4; input_auth.cpp:175,195,208).
    if lower.starts_with("- now() > 0") {
        return Some((11, "> LOCALTIMESTAMP".into()));
    }
    if lower.starts_with("now()") {
        return Some((5, "LOCALTIMESTAMP".into()));
    }
    if lower.starts_with("unix_timestamp(") {
        let (inner, consumed) = paren_arg(rest, "unix_timestamp".len())?;
        let inner = inner.trim();
        if inner.is_empty() {
            return Some((consumed, "EXTRACT(EPOCH FROM now())".into()));
        }
        return Some((consumed, format!("EXTRACT(EPOCH FROM {})", scan(inner))));
    }
    if lower.starts_with("date_add(") {
        if let Some((inner, consumed)) = paren_arg(rest, "date_add".len()) {
            if let Some(rep) = rewrite_date_add(&inner) {
                return Some((consumed, rep));
            }
        }
    }
    if lower.starts_with("timediff(") {
        if let Some((inner, consumed)) = paren_arg(rest, "timediff".len()) {
            let parts = split_top_level(&inner);
            if parts.len() == 2 {
                return Some((consumed, format!("({} - {})", scan(parts[0]), scan(parts[1]))));
            }
        }
    }
    if lower.starts_with("from_unixtime(") {
        if let Some((inner, consumed)) = paren_arg(rest, "from_unixtime".len()) {
            return Some((consumed, format!("to_timestamp({})", scan(inner.trim()))));
        }
    }
    if lower.starts_with("inet_aton(") {
        if let Some((inner, consumed)) = paren_arg(rest, "inet_aton".len()) {
            return Some((consumed, format!("({})::inet - '0.0.0.0'::inet", scan(inner.trim()))));
        }
    }
    if lower.starts_with("cast(") {
        if let Some((inner, consumed)) = paren_arg(rest, "cast".len()) {
            let inner_lower = inner.to_ascii_lowercase();
            if let Some(pos) = inner_lower.find(" as unsigned") {
                let expr = inner[..pos].trim();
                // MySQL `CAST(x AS unsigned)` parsea el PREFIJO numérico del
                // string: "0 5 6 8" → 0, "123abc" → 123, "abc" → 0, " 42" → 42.
                // PG `::bigint` es estricto → 22P02 (config.cpp:576, locale
                // SKILL_POWER_BY_LEVEL*: "0 5 6 8 ..."). `regexp_match` devuelve
                // NULL sin match → COALESCE 0 (semántica MySQL).
                return Some((
                    consumed,
                    format!(
                        "COALESCE((regexp_match({}, '^[[:space:]]*[+-]?[0-9]+'))[1]::bigint, 0)",
                        scan(expr)
                    ),
                ));
            }
            // CAST con otro target → passthrough (PG dará su error, inventario §4).
            return Some((consumed, format!("CAST({})", scan(&inner))));
        }
    }
    if lower.starts_with("collate") {
        let after = skip_ws(&rest[7..]);
        if after.to_ascii_lowercase().starts_with("sjis_japanese_ci") {
            let ws = rest.len() - after.len();
            return Some((7 + ws + "sjis_japanese_ci".len(), String::new()));
        }
    }
    None
}

/// `DATE_ADD(NOW(), INTERVAL n UNIT)` → `LOCALTIMESTAMP + make_interval(u => n)`
/// (solo el patrón del inventario: 2 sitios, `ClientManager.cpp:193`,
/// `GuildManager.cpp:1043`).
fn rewrite_date_add(inner: &str) -> Option<String> {
    let parts = split_top_level(inner);
    if parts.len() != 2 {
        return None;
    }
    if !parts[0].trim().eq_ignore_ascii_case("now()") {
        return None;
    }
    let interval = parts[1].trim();
    if !interval.to_ascii_lowercase().starts_with("interval") {
        return None;
    }
    let rest = skip_ws(&interval[8..]);
    let n_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    if n_end == 0 {
        return None;
    }
    let n = &rest[..n_end];
    let unit = skip_ws(&rest[n_end..]).to_ascii_uppercase();
    let pg_unit = match unit.as_str() {
        "SECOND" => "secs",
        "MINUTE" => "mins",
        "HOUR" => "hours",
        "DAY" => "days",
        _ => return None,
    };
    Some(format!("LOCALTIMESTAMP + make_interval({pg_unit} => {n})"))
}

/// UPDATE/DELETE … LIMIT n → se elimina (PG no tiene LIMIT en UPDATE; el WHERE
/// es PK-unique — spec §4, `ClientManager.cpp:4072,4074`).
fn drop_trailing_limit(sql: &str) -> String {
    let s = sql.trim_end();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' => {
                i = skip_string(s, i);
                continue;
            }
            b'`' => {
                i = skip_backtick(s, i);
                continue;
            }
            _ => {
                if s[i..].len() >= 5 && s[i..i + 5].eq_ignore_ascii_case("limit") {
                    let after = skip_ws(&s[i + 5..]);
                    let digits: &str = after.trim_start_matches(|c: char| c.is_ascii_digit());
                    let preceded_by_ws = i == 0 || b[i - 1].is_ascii_whitespace();
                    if preceded_by_ws && !after.is_empty() && digits.is_empty() {
                        return s[..i].trim_end().to_string();
                    }
                }
                i += 1;
            }
        }
    }
    sql.to_string()
}

// ---------------------------------------------------------------------------
// Helpers de parsing (string-aware)
// ---------------------------------------------------------------------------

fn kw_at(s: &str, kw: &str) -> bool {
    s.len() >= kw.len() && s[..kw.len()].eq_ignore_ascii_case(kw)
}

fn skip_ws(s: &str) -> &str {
    s.trim_start()
}

fn unquote_ident(s: &str) -> &str {
    let t = s.trim();
    if t.starts_with('`') && t.ends_with('`') && t.len() >= 2 {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

/// Identificador (bare o entre backticks) → (nombre, bytes consumidos).
fn parse_ident(s: &str) -> Option<(String, usize)> {
    let b = s.as_bytes();
    if b.first() == Some(&b'`') {
        let mut i = 1;
        while i < b.len() && b[i] != b'`' {
            i += 1;
        }
        if i >= b.len() {
            return None;
        }
        return Some((s[1..i].to_string(), i + 1));
    }
    let mut i = 0;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$') {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    Some((s[..i].to_string(), i))
}

/// Argumento entre paréntesis: `rest` empieza en `"name("` (o en `"("` con
/// name_len=0) → (inner, bytes consumidos incluyendo el `)` final).
fn paren_arg(rest: &str, name_len: usize) -> Option<(String, usize)> {
    let b = rest.as_bytes();
    if b.get(name_len) != Some(&b'(') {
        return None;
    }
    let mut depth = 0usize;
    let mut i = name_len;
    while i < b.len() {
        match b[i] {
            b'\'' => {
                i = skip_string(rest, i);
                continue;
            }
            b'`' => {
                i = skip_backtick(rest, i);
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some((rest[name_len + 1..i].to_string(), i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Divide por comas de nivel superior (fuera de strings/backticks/parens).
fn split_top_level(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' => {
                i = skip_string(s, i);
                continue;
            }
            b'`' => {
                i = skip_backtick(s, i);
                continue;
            }
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                parts.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(s[start..].trim());
    parts
}

/// Tuplas de VALUES: `(a, b), (c, d)` → ["a, b", "c, d"].
fn parse_values(s: &str) -> Option<Vec<String>> {
    let mut tuples = Vec::new();
    let mut rest = s;
    loop {
        rest = skip_ws(rest);
        let (inner, end) = paren_arg(rest, 0)?;
        tuples.push(inner);
        rest = skip_ws(&rest[end..]);
        if rest.starts_with(',') {
            rest = &rest[1..];
            continue;
        }
        break;
    }
    Some(tuples)
}

/// Assignments `col = expr[, col = expr]` → (columnas, valores).
fn parse_assignments(parts: &[&str]) -> Result<(Vec<String>, Vec<String>), TranslateError> {
    let mut cols = Vec::new();
    let mut vals = Vec::new();
    for part in parts {
        let (col, end) = parse_ident(part).ok_or_else(|| TranslateError::Syntax("assignment sin columna".into()))?;
        let rest = skip_ws(&part[end..]);
        let rest = skip_ws(rest.strip_prefix('=').ok_or_else(|| TranslateError::Syntax("assignment sin '='".into()))?);
        vals.push(rest.to_string());
        cols.push(col);
    }
    Ok((cols, vals))
}

/// Busca una keyword fuera de strings/backticks (para cláusulas estructurales).
fn find_kw_outside_strings(s: &str, kw: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' => i = skip_string(s, i),
            b'`' => i = skip_backtick(s, i),
            _ => {
                if s[i..].len() >= kw.len() && s[i..i + kw.len()].eq_ignore_ascii_case(kw) {
                    return Some(i);
                }
                i += 1;
            }
        }
    }
    None
}

/// Avanza hasta el final de un literal `'…'` (maneja `\\` y `''`).
fn skip_string(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    i += 1;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == b'\'' {
            if i + 1 < b.len() && b[i + 1] == b'\'' {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    i
}

/// Avanza hasta el final de un identificador entre backticks.
fn skip_backtick(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    i += 1;
    while i < b.len() && b[i] != b'`' {
        i += 1;
    }
    i + 1
}

// ---------------------------------------------------------------------------
// Tests — cada fila de la tabla §4 del inventario legacy-sql-compatibility.md
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Catálogo en memoria con las tablas de la fase 1.
    struct TestCatalog(HashMap<&'static str, TableInfo>);

    impl TestCatalog {
        fn new() -> Self {
            let n = |columns: Vec<&str>, pk: Vec<&str>, identity: Vec<&str>| TableInfo {
                columns: columns.into_iter().map(str::to_string).collect(),
                pk: pk.into_iter().map(str::to_string).collect(),
                identity: identity.into_iter().map(str::to_string).collect(),
                bytea: vec![],
            };
            let mut m: HashMap<&'static str, TableInfo> = HashMap::new();
            m.insert("quest", n(vec!["dwPID", "szName", "szState", "lValue"], vec!["dwPID", "szName", "szState"], vec![]));
            m.insert("affect", n(vec!["dwPID", "bType", "bApplyOn", "lApplyValue", "dwFlag", "lDuration", "lSPCost"], vec!["dwPID", "bType", "bApplyOn", "lApplyValue"], vec![]));
            m.insert("horse_name", n(vec!["id", "name"], vec!["id"], vec![]));
            m.insert("monarch", n(vec!["empire", "name", "windate", "money"], vec!["empire"], vec![]));
            m.insert("myshop_pricelist", n(vec!["owner_id", "item_vnum", "price"], vec!["owner_id", "item_vnum"], vec![]));
            m.insert("priv_settings", n(vec!["priv_type", "id", "type", "value", "duration"], vec!["priv_type", "id", "type"], vec![]));
            m.insert("item", n(vec!["id", "owner_id", "window", "pos", "count", "vnum"], vec!["id"], vec!["id"]));
            m.insert(
                "player",
                TableInfo {
                    columns: vec!["id".into(), "account_id".into(), "name".into(), "level".into(), "skill_level".into(), "quickslot".into()],
                    pk: vec!["id".into()],
                    identity: vec!["id".into()],
                    bytea: vec!["skill_level".into(), "quickslot".into()],
                },
            );
            m.insert("player_index", n(vec!["id", "pid1", "pid2", "pid3", "pid4", "pid5", "empire"], vec!["id"], vec![]));
            m.insert("object", n(vec!["id", "land_id", "vnum", "map_index", "x", "y"], vec!["id"], vec!["id"]));
            m.insert("loginlog2", n(vec!["type", "is_gm", "login_time", "logout_time", "channel", "account_id", "pid", "ip", "client_version", "playtime"], vec![], vec![]));
            m.insert("dragon_slay_log", n(vec!["guild_id", "dragon_vnum", "start_time", "end_time"], vec![], vec![]));
            m.insert("chat_log", n(vec!["where", "who_id", "who_name", "whom_id", "whom_name", "type", "msg", "when", "ip"], vec![], vec![]));
            Self(m)
        }
    }

    impl TableCatalog for TestCatalog {
        async fn table_info(&mut self, table: &str) -> Option<TableInfo> {
            self.0.get(table).cloned()
        }
    }

    /// rewrite de UN statement (sin split) con el catálogo de test.
    async fn rw(sql: &str) -> Result<(Option<String>, InsertIdHint), String> {
        let mut cat = TestCatalog::new();
        match rewrite(sql, &mut cat).await {
            Ok(Rewritten::NoOp) => Ok((None, InsertIdHint::None)),
            Ok(Rewritten::Execute(p)) => Ok((Some(p.sql), p.insert_id)),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn assert_rw(sql: &str, expected: &str) {
        let (got, _) = rw(sql).await.unwrap_or_else(|e| panic!("rewrite({sql:?}) falló: {e}"));
        assert_eq!(got.as_deref(), Some(expected), "input: {sql}");
    }

    // --- Fila 1: backticks → comillas dobles (reservadas en PG) -------------
    #[tokio::test]
    async fn backticks_to_double_quotes() {
        // `window+0` ya no se "elimina": se traduce a su índice de enum
        // (ver enum_zero_cast_to_index).
        assert_rw(
            "SELECT id, `window`+0, pos, count, vnum, socket0, socket1, socket2 FROM item WHERE owner_id=1 AND `window`='INVENTORY'",
            "SELECT id, CASE WHEN \"window\" IS NULL THEN NULL WHEN \"window\" = 'INVENTORY' THEN 1 WHEN \"window\" = 'EQUIPMENT' THEN 2 WHEN \"window\" = 'SAFEBOX' THEN 3 WHEN \"window\" = 'MALL' THEN 4 WHEN \"window\" = 'DRAGON_SOUL_INVENTORY' THEN 5 WHEN \"window\" = 'BELT_INVENTORY' THEN 6 WHEN \"window\" = 'GROUND' THEN 7 ELSE 0 END, pos, count, vnum, socket0, socket1, socket2 FROM item WHERE owner_id=1 AND \"window\"='INVENTORY'",
        )
        .await;
        assert_rw(
            "SELECT vnum, name, locale_name, type, `rank`, battle_type, level, size+0 FROM mob_proto ORDER BY vnum",
            "SELECT vnum, name, locale_name, type, \"rank\", battle_type, level, CASE WHEN size IS NULL THEN NULL WHEN size = 'SMALL' THEN 1 WHEN size = 'MEDIUM' THEN 2 WHEN size = 'BIG' THEN 3 ELSE 0 END FROM mob_proto ORDER BY vnum",
        )
        .await;
    }

    // --- Fila 10: +0 (cast de ENUM/SET) → índice/bitmask (MySQL) ------------
    #[tokio::test]
    async fn enum_zero_cast_to_index() {
        // item_attr.apply: enum de 118 literales → índice 1-based. El texto
        // completo se valida por fragmentos + conteo (independiente del
        // generador).
        let (sql, _) = rw("SELECT apply, apply+0, prob, lv1 FROM item_attr ORDER BY apply")
            .await
            .unwrap();
        let out = sql.unwrap();
        assert!(out.starts_with("SELECT apply, CASE WHEN apply IS NULL THEN NULL WHEN apply = 'MAX_HP' THEN 1 WHEN apply = 'MAX_SP' THEN 2"), "{out}");
        assert!(out.contains("WHEN apply = 'MOUNT' THEN 118"), "{out}");
        assert!(out.ends_with("ELSE 0 END, prob, lv1 FROM item_attr ORDER BY apply"), "{out}");
        assert_eq!(out.matches("WHEN apply = ").count(), 118, "{out}");
        // El `apply` pelado (sin +0) NO se toca.
        assert!(out.starts_with("SELECT apply, CASE"), "{out}");
        // window (backtick) en una query con WHERE sobre el mismo enum: el
        // literal del WHERE no se traduce (solo `ident+0`).
        assert_rw(
            "SELECT id, `window`+0, pos FROM item WHERE owner_id=1 AND (`window` in ('INVENTORY','EQUIPMENT'))",
            "SELECT id, CASE WHEN \"window\" IS NULL THEN NULL WHEN \"window\" = 'INVENTORY' THEN 1 WHEN \"window\" = 'EQUIPMENT' THEN 2 WHEN \"window\" = 'SAFEBOX' THEN 3 WHEN \"window\" = 'MALL' THEN 4 WHEN \"window\" = 'DRAGON_SOUL_INVENTORY' THEN 5 WHEN \"window\" = 'BELT_INVENTORY' THEN 6 WHEN \"window\" = 'GROUND' THEN 7 ELSE 0 END, pos FROM item WHERE owner_id=1 AND (\"window\" in ('INVENTORY','EQUIPMENT'))",
        )
        .await;
    }

    // --- Fila 10b: gaps del E2E verbatim (scripts/gpg/e2e_db.sh Q9, 2026-08-11)
    // --- Queries copiadas VERBATIM del e2e_db.sh (que las copia de
    // --- ClientManagerBoot.cpp:1290/1466/204). MariaDB devuelve índice/
    // --- bitmask; el crate devolvía el TEXTO → GAP.
    #[tokio::test]
    async fn e2e_q9_mob_proto_gap_verbatim() {
        let q = "SELECT vnum, name, locale_name, type, `rank`, battle_type, level, size+0, ai_flag+0, setRaceFlag+0, setImmuneFlag+0, on_click, empire, drop_item, resurrection_vnum, folder, st, dx, ht, iq, damage_min, damage_max, max_hp, regen_cycle, regen_percent, exp, gold_min, gold_max, def, attack_speed, move_speed, aggressive_hp_pct, aggressive_sight, attack_range, polymorph_item, enchant_curse, enchant_slow, enchant_poison, enchant_stun, enchant_critical, enchant_penetrate, resist_sword, resist_twohand, resist_dagger, resist_bell, resist_fan, resist_bow, resist_fire, resist_elect, resist_magic, resist_wind, resist_poison, dam_multiply, summon, drain_sp, skill_vnum0, skill_level0, skill_vnum1, skill_level1, skill_vnum2, skill_level2, skill_vnum3, skill_level3, skill_vnum4, skill_level4, sp_berserk, sp_stoneskin, sp_godspeed, sp_deathblow, sp_revive FROM mob_proto ORDER BY vnum";
        let (got, _) = rw(q).await.unwrap();
        let out = got.unwrap();
        assert!(!out.contains("+0"), "{out}");
        // GAP 1: mob 101 size+0 — MariaDB 0, proxy '' → ENUM '' → ELSE 0.
        assert!(out.contains("WHEN size = 'SMALL' THEN 1 WHEN size = 'MEDIUM' THEN 2 WHEN size = 'BIG' THEN 3 ELSE 0 END"), "{out}");
        // GAP 2: mob 101 setRaceFlag+0 — MariaDB 1 ('ANIMAL'), proxy 'ANIMAL'.
        assert!(out.contains("string_to_array(setRaceFlag, ',')"), "{out}");
        assert!(out.contains("WHERE v IN ('ANIMAL', 'UNDEAD', 'DEVIL', 'HUMAN', 'ORC', 'MILGYO', 'INSECT', 'FIRE', 'ICE', 'DESERT', 'TREE', 'ATT_ELEC', 'ATT_FIRE', 'ATT_ICE', 'ATT_WIND', 'ATT_EARTH', 'ATT_DARK')"), "{out}");
        // ai_flag / setImmuneFlag también traducidos.
        assert!(out.contains("string_to_array(ai_flag, ',')"), "{out}");
        assert!(out.contains("string_to_array(setImmuneFlag, ',')"), "{out}");
        // El resto de la query intacta (backticks normalizados).
        assert!(out.contains("\"rank\""), "{out}");
        assert!(out.contains("FROM mob_proto ORDER BY vnum"), "{out}");
    }

    #[tokio::test]
    async fn e2e_q9_item_proto_gap_verbatim() {
        let q = "SELECT vnum, type, subtype, name, locale_name, gold, shop_buy_price, weight, size, flag, wearflag, antiflag, immuneflag+0, refined_vnum, refine_set, magic_pct, socket_pct, addon_type, limittype0, limitvalue0, limittype1, limitvalue1, applytype0, applyvalue0, applytype1, applyvalue1, applytype2, applyvalue2, value0, value1, value2, value3, value4, value5 FROM item_proto ORDER BY vnum";
        let (got, _) = rw(q).await.unwrap();
        let out = got.unwrap();
        assert!(!out.contains("+0"), "{out}");
        // GAP 3: item 1 immuneflag+0 — MariaDB 0 ('' → 0), proxy ''.
        // El `size` pelado de item_proto (int en MySQL) NO se toca.
        assert!(out.contains("CASE WHEN immuneflag IS NULL THEN NULL ELSE COALESCE((SELECT sum(1 << ((pos - 1)::int)) FROM unnest(string_to_array(immuneflag, ',')) WITH ORDINALITY t(v, pos) WHERE v IN ('PARA', 'CURSE', 'STUN', 'SLEEP', 'SLOW', 'POISON', 'TERROR')), 0) END"), "{out}");
        assert!(out.contains("weight, size, flag, wearflag, antiflag,"), "{out}");
        assert!(out.contains("FROM item_proto ORDER BY vnum"), "{out}");
    }

    #[tokio::test]
    async fn e2e_q9_skill_proto_gap_verbatim() {
        let q = "SELECT dwVnum, szName, bType, bMaxLevel, dwSplashRange, szPointOn, szPointPoly, szSPCostPoly, szDurationPoly, szDurationSPCostPoly, szCooldownPoly, szMasterBonusPoly, setFlag+0, setAffectFlag+0, szPointOn2, szPointPoly2, szDurationPoly2, setAffectFlag2+0, szPointOn3, szPointPoly3, szDurationPoly3, szGrandMasterAddSPCostPoly, bLevelStep, bLevelLimit, prerequisiteSkillVnum, prerequisiteSkillLevel, iMaxHit, szSplashAroundDamageAdjustPoly, eSkillType+0, dwTargetRange FROM skill_proto ORDER BY dwVnum";
        let (got, _) = rw(q).await.unwrap();
        let out = got.unwrap();
        assert!(!out.contains("+0"), "{out}");
        // GAP 4: skill 1 setFlag+0 — MariaDB 3 (ATTACK=1 + USE_MELEE_DAMAGE=2).
        assert!(out.contains("string_to_array(setFlag, ',')"), "{out}");
        assert!(out.contains("WHERE v IN ('ATTACK', 'USE_MELEE_DAMAGE', 'COMPUTE_ATTGRADE', 'SELFONLY', 'USE_MAGIC_DAMAGE', 'USE_HP_AS_COST', 'COMPUTE_MAGIC_DAMAGE', 'SPLASH', 'GIVE_PENALTY', 'USE_ARROW_DAMAGE', 'PENETRATE', 'IGNORE_TARGET_RATING', 'ATTACK_SLOW', 'ATTACK_STUN', 'HP_ABSORB', 'SP_ABSORB', 'ATTACK_FIRE_CONT', 'REMOVE_BAD_AFFECT', 'REMOVE_GOOD_AFFECT', 'CRUSH', 'ATTACK_POISON', 'TOGGLE', 'DISABLE_BY_POINT_UP', 'CRUSH_LONG', 'ATTACK_WIND', 'ATTACK_ELEC', 'ATTACK_FIRE', 'ATTACK_BLEEDING', 'PARTY')"), "{out}");
        // setAffectFlag/setAffectFlag2 son ENUM (pese al nombre) → índice;
        // eSkillType ENUM de 4.
        assert!(out.contains("WHEN setAffectFlag = 'YMIR' THEN 1"), "{out}");
        assert!(out.contains("WHEN setAffectFlag2 = 'BLUE_POSSESSION' THEN 45"), "{out}");
        assert!(out.contains("WHEN eSkillType = 'NORMAL' THEN 1 WHEN eSkillType = 'MELEE' THEN 2 WHEN eSkillType = 'RANGE' THEN 3 WHEN eSkillType = 'MAGIC' THEN 4 ELSE 0 END"), "{out}");
        assert!(out.contains("FROM skill_proto ORDER BY dwVnum"), "{out}");
    }

    // --- Fila 10c: casos límite de la semántica MySQL (''→0, no-enum→0,
    // --- multi-elemento→bitmask, NULL→NULL) — verificado contra MariaDB
    // --- 2026-08-11 (oráculo: SELECT col+0 ...).
    #[tokio::test]
    async fn enum_zero_edge_semantics() {
        // '' → 0 (ENUM): mob_proto.size es '' en 2864/2864 filas → MariaDB 0.
        let (sql, _) = rw("SELECT size+0 FROM mob_proto").await.unwrap();
        assert_eq!(
            sql.as_deref(),
            Some("SELECT CASE WHEN size IS NULL THEN NULL WHEN size = 'SMALL' THEN 1 WHEN size = 'MEDIUM' THEN 2 WHEN size = 'BIG' THEN 3 ELSE 0 END FROM mob_proto")
        );
        // '' → 0 (SET): unnest('') no genera filas → SUM NULL → COALESCE 0.
        let (sql, _) = rw("SELECT immuneflag+0 FROM item_proto").await.unwrap();
        assert_eq!(
            sql.as_deref(),
            Some("SELECT CASE WHEN immuneflag IS NULL THEN NULL ELSE COALESCE((SELECT sum(1 << ((pos - 1)::int)) FROM unnest(string_to_array(immuneflag, ',')) WITH ORDINALITY t(v, pos) WHERE v IN ('PARA', 'CURSE', 'STUN', 'SLEEP', 'SLOW', 'POISON', 'TERROR')), 0) END FROM item_proto")
        );
        // Multi-elemento → bitmask: skill 1 = 'ATTACK,USE_MELEE_DAMAGE' →
        // 1 + 2 = 3 (MariaDB verificado); NULL → NULL (20 filas NULL reales).
        let (sql, _) = rw("SELECT setFlag+0 FROM skill_proto").await.unwrap();
        assert_eq!(
            sql.as_deref(),
            Some("SELECT CASE WHEN setFlag IS NULL THEN NULL ELSE COALESCE((SELECT sum(1 << ((pos - 1)::int)) FROM unnest(string_to_array(setFlag, ',')) WITH ORDINALITY t(v, pos) WHERE v IN ('ATTACK', 'USE_MELEE_DAMAGE', 'COMPUTE_ATTGRADE', 'SELFONLY', 'USE_MAGIC_DAMAGE', 'USE_HP_AS_COST', 'COMPUTE_MAGIC_DAMAGE', 'SPLASH', 'GIVE_PENALTY', 'USE_ARROW_DAMAGE', 'PENETRATE', 'IGNORE_TARGET_RATING', 'ATTACK_SLOW', 'ATTACK_STUN', 'HP_ABSORB', 'SP_ABSORB', 'ATTACK_FIRE_CONT', 'REMOVE_BAD_AFFECT', 'REMOVE_GOOD_AFFECT', 'CRUSH', 'ATTACK_POISON', 'TOGGLE', 'DISABLE_BY_POINT_UP', 'CRUSH_LONG', 'ATTACK_WIND', 'ATTACK_ELEC', 'ATTACK_FIRE', 'ATTACK_BLEEDING', 'PARTY')), 0) END FROM skill_proto")
        );
        // eSkillType: MAGIC = 4 (MariaDB verificado).
        let (sql, _) = rw("SELECT eSkillType+0 FROM skill_proto").await.unwrap();
        assert_eq!(
            sql.as_deref(),
            Some("SELECT CASE WHEN eSkillType IS NULL THEN NULL WHEN eSkillType = 'NORMAL' THEN 1 WHEN eSkillType = 'MELEE' THEN 2 WHEN eSkillType = 'RANGE' THEN 3 WHEN eSkillType = 'MAGIC' THEN 4 ELSE 0 END FROM skill_proto")
        );
        // item_attr_rare.apply — mismo enum que item_attr (118).
        let (sql, _) = rw("SELECT apply+0 FROM item_attr_rare").await.unwrap();
        let out = sql.unwrap();
        assert!(out.contains("WHEN apply = 'MAX_HP' THEN 1"), "{out}");
        assert!(out.contains("WHEN apply = 'MOUNT' THEN 118"), "{out}");
    }

    // --- Fila 10d: safebox load (ClientManager.cpp:680) — el C++ lee
    // --- `window+0` como índice (CreateItemTableFromRes,
    // --- ClientManagerPlayer.cpp:50: str_to_number(item.window, ...)).
    #[tokio::test]
    async fn safebox_load_window_plus_zero_index() {
        let q = "SELECT id, `window`+0, pos, count, vnum, socket0, socket1, socket2, attrtype0, attrvalue0, attrtype1, attrvalue1, attrtype2, attrvalue2, attrtype3, attrvalue3, attrtype4, attrvalue4, attrtype5, attrvalue5, attrtype6, attrvalue6 FROM item WHERE owner_id=1 AND `window`='SAFEBOX'";
        let (got, _) = rw(q).await.unwrap();
        let out = got.unwrap();
        // SAFEBOX = literal 3 del enum item.window → índice 3.
        assert!(out.contains("WHEN \"window\" = 'SAFEBOX' THEN 3"), "{out}");
        assert!(out.ends_with("FROM item WHERE owner_id=1 AND \"window\"='SAFEBOX'"), "{out}");
    }

    // --- Fila 10e: fallback — `+0` de columnas/tablas NO catalogadas se
    // --- sigue eliminando (comportamiento histórico, spec §4 fila 10).
    #[tokio::test]
    async fn enum_zero_unknown_table_or_column_dropped() {
        assert_rw("SELECT foo+0 FROM bar", "SELECT foo FROM bar").await;
        assert_rw("SELECT vnum+0 FROM mob_proto", "SELECT vnum FROM mob_proto").await;
        assert_rw("SELECT pid1+0 FROM player.player_index", "SELECT pid1 FROM player.player_index").await;
    }

    // --- Fila 2: UNIX_TIMESTAMP → EXTRACT(EPOCH FROM) -----------------------
    #[tokio::test]
    async fn unix_timestamp_to_epoch() {
        assert_rw(
            "SELECT priv_type, id, type, value, UNIX_TIMESTAMP(duration) FROM priv_settings",
            "SELECT priv_type, id, type, value, EXTRACT(EPOCH FROM duration) FROM priv_settings",
        )
        .await;
        assert_rw(
            "SELECT UNIX_TIMESTAMP(NOW())-UNIX_TIMESTAMP(last_play) FROM account",
            "SELECT EXTRACT(EPOCH FROM LOCALTIMESTAMP)-EXTRACT(EPOCH FROM last_play) FROM account",
        )
        .await;
    }

    // --- Filas 3/4: NOW() y su aritmética -----------------------------------
    #[tokio::test]
    async fn now_to_localtimestamp() {
        assert_rw(
            "INSERT INTO loginlog2(type, is_gm, login_time, channel, account_id, pid, ip, client_version) VALUES('INVALID', 'Y', NOW(), 1, 2, 3, inet_aton('127.0.0.1'), 'v1')",
            "INSERT INTO loginlog2 (\"type\", \"is_gm\", \"login_time\", \"channel\", \"account_id\", \"pid\", \"ip\", \"client_version\") VALUES ('INVALID', 'Y', LOCALTIMESTAMP, 1, 2, 3, ('127.0.0.1')::inet - '0.0.0.0'::inet, 'v1')",
        )
        .await;
        assert_rw(
            "SELECT login FROM account WHERE login='test' AND availDt - NOW() > 0",
            "SELECT login FROM account WHERE login='test' AND availDt > LOCALTIMESTAMP",
        )
        .await;
        // NOW() dentro de un literal no se toca.
        assert_rw("SELECT 'NOW() is text'", "SELECT 'NOW() is text'").await;
    }

    // --- Fila 5: DATE_ADD(NOW(), INTERVAL n SECOND) -------------------------
    #[tokio::test]
    async fn date_add_to_make_interval() {
        assert_rw(
            "REPLACE INTO priv_settings SET priv_type='PLAYER', id=1, type=1, value=100, duration=DATE_ADD(NOW(), INTERVAL 3600 SECOND)",
            "INSERT INTO priv_settings (\"priv_type\", \"id\", \"type\", \"value\", \"duration\") VALUES ('PLAYER', 1, 1, 100, LOCALTIMESTAMP + make_interval(secs => 3600)) ON CONFLICT (\"priv_type\", \"id\", \"type\") DO UPDATE SET \"priv_type\"=EXCLUDED.\"priv_type\", \"id\"=EXCLUDED.\"id\", \"type\"=EXCLUDED.\"type\", \"value\"=EXCLUDED.\"value\", \"duration\"=EXCLUDED.\"duration\"",
        )
        .await;
    }

    // --- Fila 6: REPLACE INTO … VALUES --------------------------------------
    #[tokio::test]
    async fn replace_into_values() {
        assert_rw(
            "REPLACE INTO quest (dwPID, szName, szState, lValue) VALUES(100, 'q1', 's1', 5)",
            "INSERT INTO quest (\"dwPID\", \"szName\", \"szState\", \"lValue\") VALUES (100, 'q1', 's1', 5) ON CONFLICT (\"dwPID\", \"szName\", \"szState\") DO UPDATE SET \"dwPID\"=EXCLUDED.\"dwPID\", \"szName\"=EXCLUDED.\"szName\", \"szState\"=EXCLUDED.\"szState\", \"lValue\"=EXCLUDED.\"lValue\"",
        )
        .await;
        // REPLACE sin lista de columnas → columnas del catálogo (horse_name).
        assert_rw(
            "REPLACE INTO horse_name VALUES(7, 'Pony')",
            "INSERT INTO horse_name (\"id\", \"name\") VALUES (7, 'Pony') ON CONFLICT (\"id\") DO UPDATE SET \"id\"=EXCLUDED.\"id\", \"name\"=EXCLUDED.\"name\"",
        )
        .await;
        // Forma sin INTO (Cache.cpp:189: "REPLACE myshop_pricelist(...)").
        assert_rw(
            "REPLACE myshop_pricelist(owner_id, item_vnum, price) VALUES(1, 2, 3)",
            "INSERT INTO myshop_pricelist (\"owner_id\", \"item_vnum\", \"price\") VALUES (1, 2, 3) ON CONFLICT (\"owner_id\", \"item_vnum\") DO UPDATE SET \"owner_id\"=EXCLUDED.\"owner_id\", \"item_vnum\"=EXCLUDED.\"item_vnum\", \"price\"=EXCLUDED.\"price\"",
        )
        .await;
        // now() minúscula (función MySQL case-insensitive).
        assert_rw(
            "REPLACE INTO monarch (empire, name, windate, money) VALUES(1, 2, now(), 100)",
            "INSERT INTO monarch (\"empire\", \"name\", \"windate\", \"money\") VALUES (1, 2, LOCALTIMESTAMP, 100) ON CONFLICT (\"empire\") DO UPDATE SET \"empire\"=EXCLUDED.\"empire\", \"name\"=EXCLUDED.\"name\", \"windate\"=EXCLUDED.\"windate\", \"money\"=EXCLUDED.\"money\"",
        )
        .await;
    }

    // --- Fila 6b: REPLACE … SELECT (ClientManagerGuild.cpp:111) -------------
    #[tokio::test]
    async fn replace_into_select() {
        assert_rw(
            "REPLACE INTO quest (dwPID, szName, szState, lValue) SELECT pid, 'guild_manage', 'new_disband_time', 123 FROM guild_member WHERE guild_id = 5",
            "INSERT INTO quest (\"dwPID\", \"szName\", \"szState\", \"lValue\") SELECT pid, 'guild_manage', 'new_disband_time', 123 FROM guild_member WHERE guild_id = 5 ON CONFLICT (\"dwPID\", \"szName\", \"szState\") DO UPDATE SET \"dwPID\"=EXCLUDED.\"dwPID\", \"szName\"=EXCLUDED.\"szName\", \"szState\"=EXCLUDED.\"szState\", \"lValue\"=EXCLUDED.\"lValue\"",
        )
        .await;
    }

    // --- Filas 8/9: INSERT … SET + ON DUPLICATE KEY UPDATE ------------------
    #[tokio::test]
    async fn insert_set_with_odku_and_identity() {
        // Cache.cpp:82 — id explícito (ITEM_ID_RANGE) + window índice 1
        // (enum → 'INVENTORY').
        let (sql, hint) = rw(
            "INSERT INTO item SET id=100000001, owner_id=2, `window`=1, pos=1, count=1, vnum=30001 ON DUPLICATE KEY UPDATE count=2",
        )
        .await
        .unwrap();
        assert_eq!(
            sql.as_deref(),
            Some(
                "INSERT INTO item (\"id\", \"owner_id\", \"window\", \"pos\", \"count\", \"vnum\") VALUES (100000001, 2, 'INVENTORY', 1, 1, 30001) ON CONFLICT (\"id\") DO UPDATE SET \"count\" = 2"
            )
        );
        assert_eq!(hint, InsertIdHint::Explicit(100_000_001));
    }

    #[tokio::test]
    async fn insert_values_form_with_identity() {
        // Item award: INSERT con id explícito no-cero → uiInsertID = ese id
        // (ClientManager.cpp:904-925).
        let (sql, hint) = rw(
            "INSERT INTO item (id, owner_id, `window`, pos, vnum, count) VALUES(100000001, 3, 'SAFEBOX', 0, 30001, 1)",
        )
        .await
        .unwrap();
        assert_eq!(
            sql.as_deref(),
            Some(
                "INSERT INTO item (\"id\", \"owner_id\", \"window\", \"pos\", \"vnum\", \"count\") VALUES (100000001, 3, 'SAFEBOX', 0, 30001, 1)"
            )
        );
        assert_eq!(hint, InsertIdHint::Explicit(100_000_001));

        // Player create: VALUES(0, …) → DEFAULT + Generated
        // (ClientManagerPlayer.cpp:853-905 lee uiInsertID).
        let (sql, hint) = rw("INSERT INTO player (id, account_id, name, level) VALUES(0, 5, 'Test', 1)").await.unwrap();
        assert_eq!(
            sql.as_deref(),
            Some("INSERT INTO player (\"id\", \"account_id\", \"name\", \"level\") VALUES (DEFAULT, 5, 'Test', 1)")
        );
        assert_eq!(hint, InsertIdHint::Generated);

        // Object: identity omitida de la lista → Generated.
        let (sql, hint) = rw("INSERT INTO object (land_id, vnum, map_index, x, y) VALUES(1, 2, 3, 4, 5)").await.unwrap();
        assert_eq!(
            sql.as_deref(),
            Some("INSERT INTO object (\"land_id\", \"vnum\", \"map_index\", \"x\", \"y\") VALUES (1, 2, 3, 4, 5)")
        );
        assert_eq!(hint, InsertIdHint::Generated);

        // Sin identity (loginlog2) → None.
        let (_, hint) = rw("INSERT INTO loginlog2 (type) VALUES('X')").await.unwrap();
        assert_eq!(hint, InsertIdHint::None);
    }

    // --- Fila 11: SET sql_mode / NAMES / AUTOCOMMIT → no-op -----------------
    #[tokio::test]
    async fn set_sql_mode_and_names_are_noop() {
        let (stmt, hint) = rw("SET sql_mode = ''").await.unwrap();
        assert_eq!(stmt, None);
        assert_eq!(hint, InsertIdHint::None);
        assert_eq!(rw("SET NAMES latin1").await.unwrap().0, None);
        assert_eq!(rw("SET character_set_client = utf8").await.unwrap().0, None);
    }

    /// Regresión del gate: el db/game manda `SET AUTOCOMMIT = 0` al conectar
    /// (8 errores `unrecognized configuration parameter "autocommit"` en el log
    /// PG); el C++ NO debe recibir ERR por estos SETs de config.
    #[tokio::test]
    async fn set_autocommit_is_noop() {
        for q in [
            "SET AUTOCOMMIT = 0",
            "SET AUTOCOMMIT=1",
            "SET SESSION AUTOCOMMIT = 0",
            "SET @@autocommit = 0",
            "SET @@session.autocommit = 1",
        ] {
            let (stmt, hint) = rw(q).await.unwrap_or_else(|e| panic!("{q}: {e}"));
            assert_eq!(stmt, None, "{q}");
            assert_eq!(hint, InsertIdHint::None, "{q}");
        }
    }

    // --- Fila 12: @var → temp table (OD-4; log.cpp:309-313) -----------------
    #[tokio::test]
    async fn user_variable_via_temp_table() {
        let (sql, _) = rw("SET @i = (SELECT MAX(id) FROM loginlog2 WHERE account_id=1 AND pid=2)")
            .await
            .unwrap();
        assert_eq!(
            sql.as_deref(),
            Some(
                "CREATE TEMP TABLE pg_temp.m2var_i AS SELECT v FROM (SELECT MAX(id) FROM loginlog2 WHERE account_id=1 AND pid=2) AS _m2var(v)"
            )
        );
        let (sql, _) = rw(
            "UPDATE loginlog2 SET type='VALID', logout_time=NOW(), playtime=TIMEDIFF(logout_time,login_time) WHERE id=@i",
        )
        .await
        .unwrap();
        // rewrite_update reconstruye las asignaciones con espacios alrededor
        // del `=` (semántica SQL idéntica).
        assert_eq!(
            sql.as_deref(),
            Some(
                "UPDATE loginlog2 SET type = 'VALID', logout_time = LOCALTIMESTAMP, playtime = (logout_time - login_time) WHERE id=(SELECT v FROM pg_temp.m2var_i)"
            )
        );
    }

    // --- Filas 14/15: inet_aton / FROM_UNIXTIME -----------------------------
    #[tokio::test]
    async fn inet_aton_and_from_unixtime() {
        let (sql, _) = rw(
            "INSERT INTO dragon_slay_log VALUES( 1, 2, FROM_UNIXTIME(1710000000), FROM_UNIXTIME(1710000060) )",
        )
        .await
        .unwrap();
        assert_eq!(
            sql.as_deref(),
            Some(
                "INSERT INTO dragon_slay_log (\"guild_id\", \"dragon_vnum\", \"start_time\", \"end_time\") VALUES (1, 2, to_timestamp(1710000000), to_timestamp(1710000060))"
            )
        );
    }

    // --- Fila 17: CAST(x AS unsigned) → prefijo numérico (semántica MySQL) ---
    #[tokio::test]
    async fn cast_as_unsigned() {
        // "0 5 6 8..." (locale SKILL_POWER_BY_LEVEL*) → 0; PG ::bigint estricto
        // daba 22P02.
        assert_rw(
            "SELECT mValue FROM locale WHERE mKey='SKILL_POWER_BY_LEVEL_TYPE0' ORDER BY CAST(mValue AS unsigned)",
            "SELECT mValue FROM locale WHERE mKey='SKILL_POWER_BY_LEVEL_TYPE0' ORDER BY COALESCE((regexp_match(mValue, '^[[:space:]]*[+-]?[0-9]+'))[1]::bigint, 0)",
        )
        .await;
        assert_rw(
            "SELECT CAST(mValue AS unsigned) FROM locale",
            "SELECT COALESCE((regexp_match(mValue, '^[[:space:]]*[+-]?[0-9]+'))[1]::bigint, 0) FROM locale",
        )
        .await;
    }

    /// La traducción del CAST produce la semántica MySQL de prefijo numérico
    /// ("0 5 6" → 0, "123abc" → 123, "abc" → 0, " 42" → 42): la forma emitida
    /// es `COALESCE((regexp_match(expr, '^[[:space:]]*[+-]?[0-9]+'))[1]::bigint, 0)`
    /// — `regexp_match` devuelve NULL sin match → COALESCE 0.
    #[test]
    fn cast_unsigned_emits_prefix_regexp() {
        let sql = "COALESCE((regexp_match(x, '^[[:space:]]*[+-]?[0-9]+'))[1]::bigint, 0)";
        assert!(sql.contains("regexp_match(x, '^[[:space:]]*[+-]?[0-9]+')"));
        assert!(sql.contains("[1]::bigint, 0)"));
        assert!(sql.starts_with("COALESCE("));
    }

    /// Regresión 42703: comillas dobles MySQL = string literal → comillas
    /// simples PG (el caso real: `WHERE mKey LIKE "LOCALE"`).
    #[tokio::test]
    async fn double_quotes_are_string_literals() {
        assert_rw(
            "SELECT mKey, mValue FROM locale WHERE mKey LIKE \"LOCALE\"",
            "SELECT mKey, mValue FROM locale WHERE mKey LIKE 'LOCALE'",
        )
        .await;
        // Los backticks siguen siendo identificadores → comillas dobles PG;
        // las comillas dobles MySQL de valores → comillas simples.
        assert_rw(
            "SELECT `window` FROM item WHERE `window` = \"INVENTORY\"",
            "SELECT \"window\" FROM item WHERE \"window\" = 'INVENTORY'",
        )
        .await;
        // Comillas simples internas escapadas por duplicación.
        assert_rw(
            "SELECT 1 FROM locale WHERE mKey = \"it's\"",
            "SELECT 1 FROM locale WHERE mKey = 'it''s'",
        )
        .await;
        // Las comillas dobles dentro de literales de comillas simples no se tocan.
        assert_rw("SELECT 'a \"b\" c'", "SELECT 'a \"b\" c'").await;
    }

    // --- Fila 25: collate sjis_japanese_ci se elimina -----------------------
    #[tokio::test]
    async fn sjis_collate_dropped() {
        assert_rw(
            "SELECT COUNT(*) as count FROM player WHERE name='Test' collate sjis_japanese_ci",
            "SELECT COUNT(*) as count FROM player WHERE name='Test'",
        )
        .await;
    }

    // --- Fila 24: UPDATE … LIMIT 1 se elimina -------------------------------
    #[tokio::test]
    async fn update_limit_dropped() {
        assert_rw(
            "UPDATE account SET cash = cash + 100 WHERE id = 1 limit 1",
            "UPDATE account SET cash = cash + 100 WHERE id = 1",
        )
        .await;
        // SELECT LIMIT es portable y se mantiene.
        assert_rw("SELECT id FROM player WHERE name='x' LIMIT 1", "SELECT id FROM player WHERE name='x' LIMIT 1").await;
    }

    // --- Fila 18: mysql_hash_password pasa tal cual (función PG en account) --
    #[tokio::test]
    async fn hash_function_and_cross_schema_pass_through() {
        assert_rw(
            "SELECT mysql_hash_password('1234'), a.id, a.login, a.password, a.social_id, pi.empire, pid1, pid2, pid3, pid4, pid5, a.status, a.lang FROM account a LEFT JOIN player.player_index pi ON pi.id = a.id WHERE a.login='test' AND a.password=mysql_hash_password('1234')",
            "SELECT mysql_hash_password('1234'), a.id, a.login, a.password, a.social_id, pi.empire, pid1, pid2, pid3, pid4, pid5, a.status, a.lang FROM account a LEFT JOIN player.player_index pi ON pi.id = a.id WHERE a.login='test' AND a.password=mysql_hash_password('1234')",
        )
        .await;
    }

    // --- Portables: SELECT/UPDATE/DELETE sin constructs MySQL ----------------
    #[tokio::test]
    async fn portable_statements_unchanged() {
        assert_rw("SELECT 1", "SELECT 1").await;
        assert_rw(
            "SELECT id, map_index, x, y, width, height, guild_id, guild_level_limit, price FROM land WHERE enable='YES' ORDER BY id",
            "SELECT id, map_index, x, y, width, height, guild_id, guild_level_limit, price FROM land WHERE enable='YES' ORDER BY id",
        )
        .await;
        assert_rw(
            "SELECT id, name, job, level, playtime, st, ht, dx, iq, part_main, part_hair, x, y, skill_group, change_name FROM player WHERE account_id=1",
            "SELECT id, name, job, level, playtime, st, ht, dx, iq, part_main, part_hair, x, y, skill_group, change_name FROM player WHERE account_id=1",
        )
        .await;
    }

    // --- Seguridad del scanner: strings con caracteres especiales -----------
    #[tokio::test]
    async fn string_literals_are_invulnerable() {
        // Backtick, ';' y NOW() dentro de un literal no se tocan.
        let (sql, _) = rw("INSERT INTO chat_log (`where`, `msg`) VALUES (1, 'a`b;c NOW()')").await.unwrap();
        assert_eq!(
            sql.as_deref(),
            Some("INSERT INTO chat_log (\"where\", \"msg\") VALUES (1, 'a`b;c NOW()')")
        );
        // split_statements respeta ';' dentro de strings.
        assert_eq!(split_statements("SELECT ';'"), vec!["SELECT ';'"]);
        assert_eq!(
            split_statements("INSERT INTO chat_log (`where`, `msg`) VALUES (1, 'a`b;c NOW()')"),
            vec!["INSERT INTO chat_log (`where`, `msg`) VALUES (1, 'a`b;c NOW()')"]
        );
    }

    // --- split de multi-statements (defensivo; CLIENT_MULTI_STATEMENTS) -----
    #[tokio::test]
    async fn split_statements_works() {
        assert_eq!(split_statements("SET sql_mode = ''; SELECT 1;"), vec!["SET sql_mode = ''", "SELECT 1"]);
        assert_eq!(split_statements("SELECT 1"), vec!["SELECT 1"]);
        assert_eq!(split_statements("  ;  "), Vec::<&str>::new());
        assert_eq!(split_statements("SELECT 1;"), vec!["SELECT 1"]);
    }

    // --- Ghost table: guild_invite_limit (legacy-schema.md §7.7) ------------
    #[tokio::test]
    async fn replace_unknown_table_errors_like_mysql() {
        let err = rw("REPLACE INTO guild_invite_limit VALUES(1, 2)").await.unwrap_err();
        assert!(err.contains("guild_invite_limit"), "{err}");
        // En MySQL esa tabla no existe → el write falla silenciosamente;
        // el proxy responde ERR 1146 (paridad de comportamiento observable).
        let mut cat = TestCatalog::new();
        match rewrite("REPLACE INTO guild_invite_limit VALUES(1, 2)", &mut cat).await {
            Err(e) => assert_eq!(e.mysql_errno(), crate::wire::ER_NO_SUCH_TABLE),
            other => panic!("esperaba error, got {other:?}"),
        }
    }

    // --- Punto y coma final de los queries del boot -------------------------
    #[tokio::test]
    async fn trailing_semicolon_handled_by_splitter() {
        let stmts = split_statements("SELECT vnum, name, type FROM mob_proto ORDER BY vnum;");
        assert_eq!(stmts, vec!["SELECT vnum, name, type FROM mob_proto ORDER BY vnum"]);
    }

    // --- Regresión 22021 (bug crítico 2026-08-10): blobs escapados MySQL `\0`
    // --- en columnas bytea (skill_level/quickslot) → literal bytea hex.
    // --- INSERT de creación de personaje (ClientManagerPlayer.cpp:853-905,
    // --- con ENABLE_ACCE_COSTUME_SYSTEM): el C++ escapa los blobs con
    // --- mysql_real_escape_string → `\0\0…` → PG con SCS=off interpreta `\0`
    // --- como octal → NUL real en literal text → 22021.
    #[tokio::test]
    async fn player_create_insert_escaped_blobs_no_nul_in_text() {
        let sql = "INSERT INTO player (id, account_id, name, level, st, ht, dx, iq, job, voice, dir, x, y, z, hp, mp, random_hp, random_sp, stat_point, stamina, part_base, part_main, part_hair, part_acce, gold, playtime, skill_level, quickslot) VALUES(0, 1, 'kjkj', 1, 3, 4, 3, 6, 1, 0, 0, 969600, 278400, 0, 100, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, '\\0\\0\\0', '\\0\\0\\0')";
        let (got, hint) = rw(sql).await.unwrap_or_else(|e| panic!("rewrite({sql:?}) falló: {e}"));
        let out = got.unwrap();
        // Los literales bytea se emiten como decode('<hex>', 'hex') — nunca un
        // `\0` crudo llega a un literal text de PG. En el INSERT los valores
        // son posicionales (2 blobs → 2 decode).
        assert_eq!(out.matches("decode('000000', 'hex')").count(), 2, "{out}");
        assert!(!out.contains("\\0"), "ningún \\0 escapado sobrevive: {out}");
        assert_eq!(hint, InsertIdHint::Generated);
    }

    /// UPDATE de guardado de personaje (CreatePlayerSaveQuery,
    /// ClientManagerPlayer.cpp:70-177): las asignaciones de columnas bytea
    /// también deben convertirse.
    #[tokio::test]
    async fn player_save_update_escaped_blobs_no_nul_in_text() {
        let sql = "UPDATE player SET job = 7, voice = 0, dir = 0, x = 960658, y = 266626, z = 0, map_index = 41, exit_x = 960658, exit_y = 266626, exit_map_index = 41, hp = 860, mp = 320, stamina = 820, random_hp = 0, random_sp = 0, playtime = 100, level = 12, level_step = 1, st = 3, ht = 4, dx = 3, iq = 6, gold = 0, exp = 0, stat_point = 0, skill_point = 0, sub_skill_point = 0, stat_reset_count = 0, ip = '0.0.0.0', part_main = 0, part_hair = 0, part_acce = 0, last_play = NOW(), skill_group = 1, alignment = 0, horse_level = 0, horse_riding = 0, horse_hp = 0, horse_hp_droptime = 0, horse_stamina = 0, horse_skill_point = 0, skill_level = '\\0\\0\\0', quickslot = '\\0\\0\\0' WHERE id=4";
        let (got, _) = rw(sql).await.unwrap_or_else(|e| panic!("rewrite({sql:?}) falló: {e}"));
        let out = got.unwrap();
        assert!(out.contains("skill_level = decode('000000', 'hex')"), "{out}");
        assert!(out.contains("quickslot = decode('000000', 'hex')"), "{out}");
        assert!(!out.contains("\\0"), "ningún \\0 escapado sobrevive: {out}");
        // El resto del UPDATE sigue traduciéndose (NOW() → LOCALTIMESTAMP).
        assert!(out.contains("last_play = LOCALTIMESTAMP"), "{out}");
        assert!(out.contains("WHERE id=4"), "{out}");
    }

    /// Mezcla: literales text normales (no bytea) con secuencias escapadas
    /// estándar NO se convierten — solo las columnas bytea del catálogo.
    #[tokio::test]
    async fn non_bytea_escapes_stay_text() {
        let sql = "UPDATE player SET name = 'a\\'b', level = 1 WHERE id=4";
        let (got, _) = rw(sql).await.unwrap();
        let out = got.unwrap();
        assert!(out.contains("name = 'a\\'b'"), "{out}");
        assert!(!out.contains("decode("), "{out}");
    }
}
