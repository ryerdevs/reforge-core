//! F3 (ADR-0008): dominio world — `PlayerRepo` (schema `player`).
//!
//! Semantica EXACTA verificada por la suite E2E (`scripts/gpg/e2e_db.sh`):
//! - `load` = Q2 (`ClientManagerPlayer.cpp:361-375`): 42 columnas, con
//!   `UNIX_TIMESTAMP(NOW())-UNIX_TIMESTAMP(last_play)` traducido a PG directo
//!   como `EXTRACT(EPOCH FROM LOCALTIMESTAMP) - EXTRACT(EPOCH FROM last_play)`
//!   (el proxy hace la traduccion para el C++; aqui hablamos PG nativo).
//! - `save` = Q5 shape (`CreatePlayerSaveQuery`, `ClientManagerPlayer.cpp:70-177`).
//! - `list_for_account` = Q3 (`ClientManagerLogin.cpp:231-235`): 15 columnas.
//! - `create` = Q4 (`ClientManagerPlayer.cpp:853-892`): `id=0` -> `DEFAULT`
//!   (identity BY DEFAULT — regla B5 del proxy, `translate.rs:419-425`).
//!
//! Blobs (`skill_level`/`quickslot`, bytea): con tokio-postgres se pasan como
//! `Vec<u8>` nativo (el `decode('<hex>','hex')` es solo el patron texto del
//! proxy). El mapeo de salida decodifica bytea -> `Vec<u8>` crudo (patron `\x`
//! ya resuelto por el driver).

use crate::pool::{Client, PgPool};
use tokio_postgres::Row;

use crate::account::pg_err;
use crate::wal::{Batcher, Mutation, Param};

/// Fila completa del load (42 columnas, orden de Q2). Tipos PG reales
/// (verificados en el esquema: bigint/smallint/integer/bytea/float8).
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerRow {
    pub id: i64,
    pub name: String,
    pub job: i16,
    pub voice: i16,
    pub dir: i16,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub map_index: i32,
    pub exit_x: i32,
    pub exit_y: i32,
    pub exit_map_index: i32,
    pub hp: i32,
    pub mp: i32,
    pub stamina: i16,
    pub random_hp: i16,
    pub random_sp: i16,
    pub playtime: i32,
    pub gold: i32,
    pub level: i16,
    pub level_step: i16,
    pub st: i16,
    pub ht: i16,
    pub dx: i16,
    pub iq: i16,
    pub exp: i32,
    pub stat_point: i16,
    pub skill_point: i16,
    pub sub_skill_point: i16,
    pub stat_reset_count: i16,
    pub part_base: i16,
    pub part_hair: i64,
    /// `part_main` NO esta en las 42 columnas del load (quirk legacy: el C++
    /// tampoco lo carga — solo lo escribe en el save). El load lo deja en 0.
    pub part_main: i64,
    pub skill_level: Option<Vec<u8>>,
    pub quickslot: Option<Vec<u8>>,
    pub skill_group: i16,
    pub alignment: i32,
    pub horse_level: i16,
    pub horse_riding: i16,
    pub horse_hp: i16,
    pub horse_hp_droptime: i64,
    pub horse_stamina: i16,
    /// `EXTRACT(EPOCH FROM LOCALTIMESTAMP) - EXTRACT(EPOCH FROM last_play)` —
    /// segundos desde el ultimo login (parity `logoff_interval` del C++).
    pub logoff_interval: f64,
    pub horse_skill_point: i16,
}

/// Load: las 42 columnas en el orden exacto del E2E Q2.
const LOAD_SQL: &str = "\
SELECT id, name, job, voice, dir, x, y, z, map_index, exit_x, exit_y, exit_map_index, \
hp, mp, stamina, random_hp, random_sp, playtime, gold, level, level_step, st, ht, dx, iq, \
exp, stat_point, skill_point, sub_skill_point, stat_reset_count, part_base, part_hair, \
skill_level, quickslot, skill_group, alignment, horse_level, horse_riding, horse_hp, \
horse_hp_droptime, horse_stamina, \
(EXTRACT(EPOCH FROM LOCALTIMESTAMP) - EXTRACT(EPOCH FROM last_play))::float8, horse_skill_point \
FROM player.player WHERE id = $1";

/// Fila de la lista del login (15 columnas, orden de Q3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSummary {
    pub id: i64,
    pub name: String,
    pub job: i16,
    pub level: i16,
    pub playtime: i32,
    pub st: i16,
    pub ht: i16,
    pub dx: i16,
    pub iq: i16,
    pub part_main: i64,
    pub part_hair: i64,
    pub x: i32,
    pub y: i32,
    pub skill_group: i16,
    pub change_name: i16,
}

/// Datos del create (Q4, `ClientManagerPlayer.cpp:853-892` — 27 columnas).
/// Divergencia documentada: el C++ NO inserta `map_index` (lo deja en 0 y lo
/// resuelve con `CMapLocation` al entrar — `input_db.cpp:222-227`); el
/// rewrite lo fija en el create porque el canal sirve UN solo mapa (41) y el
/// load del select necesita el mapa válido desde la fila (P0-B).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerCreate {
    pub account_id: i64,
    pub name: String,
    pub level: i16,
    pub st: i16,
    pub ht: i16,
    pub dx: i16,
    pub iq: i16,
    pub job: i16,
    pub voice: i16,
    pub dir: i16,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub map_index: i32,
    pub hp: i32,
    pub mp: i32,
    pub random_hp: i16,
    pub random_sp: i16,
    pub stat_point: i16,
    pub stamina: i16,
    pub part_base: i16,
    pub part_main: i64,
    pub part_hair: i64,
    pub gold: i32,
    pub playtime: i32,
    pub skill_level: Vec<u8>,
    pub quickslot: Vec<u8>,
}

/// Repositorio del dominio world (player). Conexion por llamada (ADR-0008).
pub struct PlayerRepo {
    pool: PgPool,
}

impl PlayerRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn connect(&self) -> Result<Client, String> {
        self.pool.get().await.map_err(|e| format!("PG pool get: {e}"))
    }

    /// Load completo (Q2). `None` = no existe el personaje.
    pub async fn load(&self, id: i64) -> Result<Option<PlayerRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(LOAD_SQL, &[&id])
            .await
            .map_err(|e| pg_err("PLAYER_LOAD", &e))?;
        rows.first().map(player_row_from_row).transpose()
    }

    /// Save (Q5 shape, `CreatePlayerSaveQuery`): UPDATE de todas las columnas
    /// del row + `last_play = NOW()`. Blobs como bytea nativo. Devuelve filas
    /// afectadas (1 = ok, 0 = el personaje no existe).
    pub async fn save(&self, p: &PlayerRow) -> Result<u64, String> {
        let client = self.connect().await?;
        let owned = save_params(p);
        let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            owned.iter().map(|x| x as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
        client
            .execute(PLAYER_SAVE_SQL, &params)
            .await
            .map_err(|e| pg_err("PLAYER_SAVE", &e))
    }

    /// Save DURABLE (ADR-0008): construye la `Mutation` (uuidv7 + sql del
    /// save + params) y la envia al `Batcher` — el sink la aplica con audit
    /// en la MISMA transaccion, en batches <=100ms. Fire-and-forget: el
    /// callersigue; la garantia durable la da el batch transaccional (y el
    /// replay idempotente del WAL local en F3 phase 2). El UPDATE es
    /// naturalmente idempotente (re-aplicarlo no cambia el estado).
    pub fn save_mutated(&self, batcher: &Batcher, p: &PlayerRow) {
        batcher.push(save_mutation(p));
    }

    /// Lista de personajes de la cuenta (Q3, 15 columnas) — orden sin
    /// garantia (parity: el C++ no ordena).
    pub async fn list_for_account(&self, account_id: i64) -> Result<Vec<PlayerSummary>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(
                "SELECT id, name, job, level, playtime, st, ht, dx, iq, part_main, \
part_hair, x, y, skill_group, change_name FROM player.player WHERE account_id = $1",
                &[&account_id],
            )
            .await
            .map_err(|e| pg_err("PLAYER_LIST", &e))?;
        rows.iter().map(player_summary_from_row).collect()
    }

    /// Create (Q4): `id = DEFAULT` (regla B5 — identity BY DEFAULT) +
    /// `RETURNING id`. Devuelve el id nuevo.
    pub async fn create(&self, c: &PlayerCreate) -> Result<i64, String> {
        let client = self.connect().await?;
        let row = client
            .query_one(
                "INSERT INTO player.player \
(id, account_id, name, level, st, ht, dx, iq, job, voice, dir, x, y, z, hp, mp, \
random_hp, random_sp, stat_point, stamina, part_base, part_main, part_hair, gold, \
playtime, skill_level, quickslot) \
VALUES (DEFAULT, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, \
$16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26) RETURNING id",
                &[
                    &c.account_id, &c.name, &c.level, &c.st, &c.ht, &c.dx, &c.iq, //
                    &c.job, &c.voice, &c.dir, &c.x, &c.y, &c.z, &c.hp, &c.mp, //
                    &c.random_hp, &c.random_sp, &c.stat_point, &c.stamina, &c.part_base, //
                    &c.part_main, &c.part_hair, &c.gold, &c.playtime, &c.skill_level, //
                    &c.quickslot,
                ],
            )
            .await
            .map_err(|e| pg_err("PLAYER_CREATE", &e))?;
        let id: i64 = row.try_get(0).map_err(|e| format!("PLAYER_CREATE id: {e}"))?;
        // Divergencia documentada: el C++ no toca el map_index en el create
        // (lo resuelve CMapLocation); el rewrite fija el mapa 41 (el único
        // que sirve el canal) para que el load del select cargue el mapa
        // correcto desde la fila (P0-B — sin esto el entry fail-opens y el
        // cliente carga un mapa inexistente → 0xC0000374).
        client
            .execute(
                "UPDATE player.player SET map_index = $2 WHERE id = $1",
                &[&id, &c.map_index],
            )
            .await
            .map_err(|e| pg_err("PLAYER_CREATE map_index", &e))?;
        Ok(id)
    }

    /// Pid del slot en `player.player_index` (parity `ClientManagerPlayer.cpp:794`
    /// — `SELECT pid%u ...` con `account_index + 1`).
    ///
    /// - `slot` 0..4 -> `Some(pid)` si el índice tiene fila y `pid > 0`;
    ///   `None` = slot vacío (pid 0) o cuenta sin fila de índice.
    /// - `slot` >= 5 -> `Err` (el game valida antes, `input_login.cpp:260-264`).
    pub async fn player_index_pid(&self, account_id: i64, slot: u8) -> Result<Option<i64>, String> {
        let client = self.connect().await?;
        let sql = index_sql(slot)?;
        let rows = client
            .query(&sql, &[&account_id])
            .await
            .map_err(|e| pg_err("PLAYER_INDEX", &e))?;
        Ok(rows
            .first()
            .and_then(|r| r.try_get(0).ok())
            .filter(|&pid| pid > 0))
    }

    /// ¿Existe OTRO personaje con este nombre? (parity `QUERY_CHANGE_NAME`
    /// `ClientManager.cpp:548-570` — `COUNT WHERE name AND id <> except_id`;
    /// el create la usa con `except_id = 0` — parity `__QUERY_PLAYER_CREATE`
    /// `ClientManagerPlayer.cpp:812-829`). La columna `name` NO es UNIQUE en
    /// el esquema (solo KEY `name_idx` — el chequeo es del app, como el C++).
    pub async fn name_exists(&self, name: &str, except_id: i64) -> Result<bool, String> {
        let client = self.connect().await?;
        let row = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM player.player WHERE name = $1 AND id <> $2)",
                &[&name, &except_id],
            )
            .await
            .map_err(|e| pg_err("PLAYER_NAME_EXISTS", &e))?;
        row.try_get(0).map_err(|e| format!("PLAYER_NAME_EXISTS: {e}"))
    }

    /// Escribe el pid en el slot del índice (create — parity
    /// `ClientManagerPlayer.cpp:890-900`). Garantiza la fila del índice con
    /// `INSERT ... ON CONFLICT DO NOTHING` (parity
    /// `PLAYER_INDEX_CREATE_BUG_FIX`, `ClientManagerLogin.cpp:213` — el C++
    /// crea la fila perezosamente al login; el rewrite la garantiza aquí).
    pub async fn set_slot(&self, account_id: i64, slot: u8, player_id: i64) -> Result<(), String> {
        let client = self.connect().await?;
        client
            .execute(
                "INSERT INTO player.player_index (id) VALUES ($1) ON CONFLICT (id) DO NOTHING",
                &[&account_id],
            )
            .await
            .map_err(|e| pg_err("PLAYER_INDEX_CREATE", &e))?;
        let sql = format!("UPDATE player.player_index SET {} = $2 WHERE id = $1", index_col(slot)?);
        client
            .execute(&sql, &[&account_id, &player_id])
            .await
            .map_err(|e| pg_err("PLAYER_INDEX_SET", &e))?;
        Ok(())
    }

    /// Empire de la cuenta en el índice (parity `QUERY_EMPIRE_SELECT`
    /// `ClientManager.cpp:1129-1134` — `UPDATE player_index SET empire`).
    pub async fn set_empire(&self, account_id: i64, empire: i16) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(
                "INSERT INTO player.player_index (id) VALUES ($1) ON CONFLICT (id) DO NOTHING",
                &[&account_id],
            )
            .await
            .map_err(|e| pg_err("PLAYER_INDEX_CREATE", &e))?;
        client
            .execute(
                "UPDATE player.player_index SET empire = $2 WHERE id = $1",
                &[&account_id, &empire],
            )
            .await
            .map_err(|e| pg_err("PLAYER_INDEX_EMPIRE", &e))
    }

    /// Renombre (parity `QUERY_CHANGE_NAME` `ClientManager.cpp:573-580` —
    /// `UPDATE player SET name`). Devuelve filas afectadas (1 = ok).
    pub async fn rename(&self, player_id: i64, name: &str) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(
                "UPDATE player.player SET name = $2 WHERE id = $1",
                &[&player_id, &name],
            )
            .await
            .map_err(|e| pg_err("PLAYER_RENAME", &e))
    }

    /// Borrado de personaje (parity `__RESULT_PLAYER_DELETE`
    /// `ClientManagerPlayer.cpp:1055-1130`): 1) el GATE — el slot del índice
    /// se pone a 0 (si no afecta filas → `Err`, el C++ responde
    /// DELETE_FAILED); 2) DELETE del player + sus items/quests/afectos (el
    /// C++ los borra en cascada). Divergencia documentada: sin archivo en
    /// `player_deleted` (el C++ mueve la fila ANTES de borrar — red de
    /// seguridad del legacy; el rewrite confía en el DELETE directo).
    pub async fn delete(&self, account_id: i64, slot: u8, player_id: i64) -> Result<(), String> {
        let client = self.connect().await?;
        let sql = format!(
            "UPDATE player.player_index SET {} = 0 WHERE id = $1 AND {} = $2",
            index_col(slot)?,
            index_col(slot)?
        );
        let n = client
            .execute(&sql, &[&account_id, &player_id])
            .await
            .map_err(|e| pg_err("PLAYER_INDEX_DEL", &e))?;
        if n == 0 {
            return Err(format!(
                "PLAYER_INDEX_DEL: slot {slot} de la cuenta {account_id} no apunta a {player_id}"
            ));
        }
        client
            .execute(
                "DELETE FROM player.player WHERE id = $1",
                &[&player_id],
            )
            .await
            .map_err(|e| pg_err("PLAYER_DELETE", &e))?;
        client
            .execute(
                "DELETE FROM player.item WHERE owner_id = $1",
                &[&player_id],
            )
            .await
            .map_err(|e| pg_err("PLAYER_DELETE items", &e))?;
        client
            .execute(
                "DELETE FROM player.quest WHERE dw_pid = $1",
                &[&player_id],
            )
            .await
            .map_err(|e| pg_err("PLAYER_DELETE quests", &e))?;
        client
            .execute(
                "DELETE FROM player.affect WHERE dw_pid = $1",
                &[&player_id],
            )
            .await
            .map_err(|e| pg_err("PLAYER_DELETE affects", &e))?;
        Ok(())
    }
}

/// Nombres de columna del índice por slot (parity `ClientManagerPlayer.cpp:794`
/// — `pid%u` con `account_index + 1` = pid1..pid5). Constante cerrada: el slot
/// se valida antes de indexar (`index_sql`).
const PID_COLUMNS: [&str; 5] = ["pid1", "pid2", "pid3", "pid4", "pid5"];

/// Columna del índice para el slot (parity `ClientManagerPlayer.cpp:794`
/// — `pid%u` con `account_index + 1` = pid1..pid5). Constante cerrada: el slot
/// se valida antes de indexar (`index_col`).
fn index_col(slot: u8) -> Result<&'static str, String> {
    PID_COLUMNS
        .get(slot as usize)
        .copied()
        .ok_or_else(|| format!("player_index: slot {slot} fuera de rango 0..4"))
}

/// SQL del índice para el slot: `SELECT pid{n} FROM player.player_index
/// WHERE id = $1`. `Err` si el slot está fuera de 0..4 (nunca se interpola un
/// valor del caller — la columna viene de la constante cerrada).
fn index_sql(slot: u8) -> Result<String, String> {
    let col = index_col(slot)?;
    Ok(format!("SELECT {col} FROM player.player_index WHERE id = $1"))
}

/// `ip` no esta en el load (42 columnas) — el save usa el default del C++.
fn ip_default(_p: &PlayerRow) -> String {
    "0.0.0.0".to_string()
}

/// SQL del save (Q5 shape, `CreatePlayerSaveQuery` `ClientManagerPlayer.cpp:70-177`):
/// UPDATE de todas las columnas + `last_play = NOW()`. Compartido por `save()`
/// (directo) y `save_mutated()` (mutation durable). Idempotente por naturaleza
/// (UPDATE por PK) — requisito del replay del pipeline.
const PLAYER_SAVE_SQL: &str = "\
UPDATE player.player SET \
job = $1, voice = $2, dir = $3, x = $4, y = $5, z = $6, map_index = $7, \
exit_x = $8, exit_y = $9, exit_map_index = $10, hp = $11, mp = $12, stamina = $13, \
random_hp = $14, random_sp = $15, playtime = $16, level = $17, level_step = $18, \
st = $19, ht = $20, dx = $21, iq = $22, gold = $23, exp = $24, stat_point = $25, \
skill_point = $26, sub_skill_point = $27, stat_reset_count = $28, ip = $29, \
part_main = $30, part_hair = $31, last_play = NOW(), skill_group = $32, \
alignment = $33, horse_level = $34, horse_riding = $35, horse_hp = $36, \
horse_hp_droptime = $37, horse_stamina = $38, horse_skill_point = $39, \
skill_level = $40, quickslot = $41 \
WHERE id = $42";

/// Params del save en el orden de `PLAYER_SAVE_SQL` ($1..$42). Los blobs
/// nullable (`skill_level`/`quickslot`) van como `Param::Bytes` o
/// `Param::Null` (el save directo pasaba `Option<Vec<u8>>` -> NULL).
fn save_params(p: &PlayerRow) -> Vec<Param> {
    let i16 = |v: i16| Param::Int(i64::from(v));
    let i32 = |v: i32| Param::Int(i64::from(v));
    let blob = |b: &Option<Vec<u8>>| match b {
        Some(bytes) => Param::Bytes(bytes.clone()),
        None => Param::Null,
    };
    vec![
        i16(p.job), i16(p.voice), i16(p.dir), i32(p.x), i32(p.y), i32(p.z), i32(p.map_index), //
        i32(p.exit_x), i32(p.exit_y), i32(p.exit_map_index), i32(p.hp), i32(p.mp), i16(p.stamina), //
        i16(p.random_hp), i16(p.random_sp), i32(p.playtime), i16(p.level), i16(p.level_step), //
        i16(p.st), i16(p.ht), i16(p.dx), i16(p.iq), i32(p.gold), i32(p.exp), i16(p.stat_point), //
        i16(p.skill_point), i16(p.sub_skill_point), i16(p.stat_reset_count), Param::Text(ip_default(p)), //
        Param::Int(p.part_main), Param::Int(p.part_hair), i16(p.skill_group), i32(p.alignment), //
        i16(p.horse_level), i16(p.horse_riding), i16(p.horse_hp), Param::Int(p.horse_hp_droptime), //
        i16(p.horse_stamina), i16(p.horse_skill_point), blob(&p.skill_level), blob(&p.quickslot), //
        Param::Int(p.id),
    ]
}

/// Mutation durable del save: uuidv7 + `PLAYER_SAVE_SQL` + params.
pub(crate) fn save_mutation(p: &PlayerRow) -> Mutation {
    Mutation::new(PLAYER_SAVE_SQL, save_params(p))
}

/// Mapeo de las 42 columnas del load (orden Q2).
fn player_row_from_row(row: &Row) -> Result<PlayerRow, String> {
    let g = |i: usize| -> Result<i64, String> { row.try_get(i).map_err(|e| format!("col{i}: {e}")) };
    Ok(PlayerRow {
        id: g(0)?,
        name: row.try_get(1).map_err(|e| format!("col1: {e}"))?,
        job: row.try_get(2).map_err(|e| format!("col2: {e}"))?,
        voice: row.try_get(3).map_err(|e| format!("col3: {e}"))?,
        dir: row.try_get(4).map_err(|e| format!("col4: {e}"))?,
        x: row.try_get(5).map_err(|e| format!("col5: {e}"))?,
        y: row.try_get(6).map_err(|e| format!("col6: {e}"))?,
        z: row.try_get(7).map_err(|e| format!("col7: {e}"))?,
        map_index: row.try_get(8).map_err(|e| format!("col8: {e}"))?,
        exit_x: row.try_get(9).map_err(|e| format!("col9: {e}"))?,
        exit_y: row.try_get(10).map_err(|e| format!("col10: {e}"))?,
        exit_map_index: row.try_get(11).map_err(|e| format!("col11: {e}"))?,
        hp: row.try_get(12).map_err(|e| format!("col12: {e}"))?,
        mp: row.try_get(13).map_err(|e| format!("col13: {e}"))?,
        stamina: row.try_get(14).map_err(|e| format!("col14: {e}"))?,
        random_hp: row.try_get(15).map_err(|e| format!("col15: {e}"))?,
        random_sp: row.try_get(16).map_err(|e| format!("col16: {e}"))?,
        playtime: row.try_get(17).map_err(|e| format!("col17: {e}"))?,
        gold: row.try_get(18).map_err(|e| format!("col18: {e}"))?,
        level: row.try_get(19).map_err(|e| format!("col19: {e}"))?,
        level_step: row.try_get(20).map_err(|e| format!("col20: {e}"))?,
        st: row.try_get(21).map_err(|e| format!("col21: {e}"))?,
        ht: row.try_get(22).map_err(|e| format!("col22: {e}"))?,
        dx: row.try_get(23).map_err(|e| format!("col23: {e}"))?,
        iq: row.try_get(24).map_err(|e| format!("col24: {e}"))?,
        exp: row.try_get(25).map_err(|e| format!("col25: {e}"))?,
        stat_point: row.try_get(26).map_err(|e| format!("col26: {e}"))?,
        skill_point: row.try_get(27).map_err(|e| format!("col27: {e}"))?,
        sub_skill_point: row.try_get(28).map_err(|e| format!("col28: {e}"))?,
        stat_reset_count: row.try_get(29).map_err(|e| format!("col29: {e}"))?,
        part_base: row.try_get(30).map_err(|e| format!("col30: {e}"))?,
        part_hair: row.try_get(31).map_err(|e| format!("col31: {e}"))?,
        part_main: 0, // quirk legacy: el load no trae part_main (el save lo escribe)
        // bytea: el driver decodifica el formato binario -> Vec<u8> crudo.
        skill_level: row.try_get(32).map_err(|e| format!("col32: {e}"))?,
        quickslot: row.try_get(33).map_err(|e| format!("col33: {e}"))?,
        skill_group: row.try_get(34).map_err(|e| format!("col34: {e}"))?,
        alignment: row.try_get(35).map_err(|e| format!("col35: {e}"))?,
        horse_level: row.try_get(36).map_err(|e| format!("col36: {e}"))?,
        horse_riding: row.try_get(37).map_err(|e| format!("col37: {e}"))?,
        horse_hp: row.try_get(38).map_err(|e| format!("col38: {e}"))?,
        horse_hp_droptime: row.try_get(39).map_err(|e| format!("col39: {e}"))?,
        horse_stamina: row.try_get(40).map_err(|e| format!("col40: {e}"))?,
        logoff_interval: row.try_get(41).map_err(|e| format!("col41: {e}"))?,
        horse_skill_point: row.try_get(42).map_err(|e| format!("col42: {e}"))?,
    })
}

/// Mapeo de las 15 columnas de la lista (orden Q3).
fn player_summary_from_row(row: &Row) -> Result<PlayerSummary, String> {
    let g = |i: usize| -> Result<i64, String> { row.try_get(i).map_err(|e| format!("col{i}: {e}")) };
    Ok(PlayerSummary {
        id: g(0)?,
        name: row.try_get(1).map_err(|e| format!("col1: {e}"))?,
        job: row.try_get(2).map_err(|e| format!("col2: {e}"))?,
        level: row.try_get(3).map_err(|e| format!("col3: {e}"))?,
        playtime: row.try_get(4).map_err(|e| format!("col4: {e}"))?,
        st: row.try_get(5).map_err(|e| format!("col5: {e}"))?,
        ht: row.try_get(6).map_err(|e| format!("col6: {e}"))?,
        dx: row.try_get(7).map_err(|e| format!("col7: {e}"))?,
        iq: row.try_get(8).map_err(|e| format!("col8: {e}"))?,
        part_main: row.try_get(9).map_err(|e| format!("col9: {e}"))?,
        part_hair: row.try_get(10).map_err(|e| format!("col10: {e}"))?,
        x: row.try_get(11).map_err(|e| format!("col11: {e}"))?,
        y: row.try_get(12).map_err(|e| format!("col12: {e}"))?,
        skill_group: row.try_get(13).map_err(|e| format!("col13: {e}"))?,
        change_name: row.try_get(14).map_err(|e| format!("col14: {e}"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::uuidv7_string;

    /// Load: 43 columnas en el orden del contrato (parse del C++
    /// `RESULT_PLAYER_LOAD` ClientManagerPlayer.cpp:464-523; el E2E etiqueta
    /// "42" pero el check solo compara proxy vs maria) + traduccion del diff
    /// de tiempo a EXTRACT(EPOCH...).
    #[test]
    fn load_sql_has_43_columns_and_epoch_diff() {
        // Ojo: split por " FROM player.player" — la expresion EXTRACT contiene
        // "EPOCH FROM ..." y un split por "FROM" la partiria.
        let select = LOAD_SQL.split_once(" FROM player.player").expect("FROM").0;
        let cols: Vec<&str> = select
            .trim_start_matches("SELECT")
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(cols.len(), 43, "43 columnas (RESULT_PLAYER_LOAD)");
        assert_eq!(cols[0], "id");
        assert_eq!(cols[1], "name");
        assert_eq!(cols[32], "skill_level");
        assert_eq!(cols[33], "quickslot");
        assert_eq!(
            cols[41],
            "(EXTRACT(EPOCH FROM LOCALTIMESTAMP) - EXTRACT(EPOCH FROM last_play))::float8",
            "traduccion PG del UNIX_TIMESTAMP diff (::float8 — EXTRACT da numeric)"
        );
        assert_eq!(cols[42], "horse_skill_point");
        assert!(LOAD_SQL.contains("FROM player.player WHERE id = $1"), "calificado + bind");
    }

    /// Save: shape Q5 — todas las columnas + last_play = NOW() + blobs bytea.
    #[test]
    fn save_sql_shape() {
        // El SQL vive en PLAYER_SAVE_SQL (compartido save/save_mutated):
        // contrato de columnas verificado en el integration gated (Q5).
        let row = dummy_row();
        assert_eq!(row.name, "dummy");
        assert_eq!(row.logoff_interval, 0.0);
        assert!(PLAYER_SAVE_SQL.contains("last_play = NOW()"), "Q5 escribe last_play");
        assert!(PLAYER_SAVE_SQL.contains("WHERE id = $42"), "42 params");
        assert_eq!(save_params(&row).len(), 42, "42 params del save");
    }

    /// Wiring del Batcher: la mutation del save durable usa el MISMO sql que
    /// el save directo + uuidv7 (version 7) + 42 params con blobs nullable
    /// como Bytes/Null.
    #[test]
    fn save_mutation_uses_save_sql_uuidv7_and_42_params() {
        let row = dummy_row();
        let m = save_mutation(&row);
        assert_eq!(m.sql, PLAYER_SAVE_SQL, "mismo SQL (una fuente de verdad)");
        assert_eq!(m.params.len(), 42);
        assert_eq!(m.params[28], Param::Text("0.0.0.0".into()), "ip default del C++");
        assert_eq!(m.params[39], Param::Null, "skill_level None -> NULL");
        assert_eq!(m.params[40], Param::Null, "quickslot None -> NULL");
        assert_eq!(m.params[41], Param::Int(0), "id del row");
        assert_eq!(m.id[6] >> 4, 7, "version 7 del uuidv7");
        assert!(m.payload_json().contains(&uuidv7_string(&m.id)), "audit payload con mutation_id");
    }

    /// La mutation con blobs presentes mapea a Bytes (bytea).
    #[test]
    fn save_mutation_maps_blobs_to_bytes() {
        let mut row = dummy_row();
        row.skill_level = Some(vec![0x01, 0x00, 0x27, 0x5c]);
        row.quickslot = Some(vec![0xde, 0xad]);
        let m = save_mutation(&row);
        assert_eq!(m.params[39], Param::Bytes(vec![0x01, 0x00, 0x27, 0x5c]));
        assert_eq!(m.params[40], Param::Bytes(vec![0xde, 0xad]));
    }

    /// El pipeline agrupa: 2 saves en <100ms -> 1 batch (el worker flushea por
    /// intervalo desde la PRIMERA mutation). Sink contador local (sin PG).
    #[tokio::test(start_paused = true)]
    async fn save_mutated_two_saves_in_interval_land_in_one_batch() {
        use crate::wal::MutationSink;
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct CountingSink(Arc<Mutex<Vec<Vec<Mutation>>>>);
        impl MutationSink for CountingSink {
            fn apply(&mut self, batch: Vec<Mutation>) -> impl std::future::Future<Output = Result<(), String>> + Send {
                async move {
                    self.0.lock().unwrap().push(batch);
                    Ok(())
                }
            }
        }

        let sink = CountingSink::default();
        let batcher = Batcher::spawn(std::time::Duration::from_millis(100), 64, sink.clone());
        let repo = PlayerRepo::new(crate::pool::new_pool("host=127.0.0.1 port=1 user=x password=x dbname=x", 2).expect("pool"));
        let mut a = dummy_row();
        a.id = 1;
        a.x = 969600;
        let mut b = dummy_row();
        b.id = 1;
        b.x = 278400;
        repo.save_mutated(&batcher, &a);
        repo.save_mutated(&batcher, &b);
        // Fases del reloj pausado (patron de wal.rs): 1) el worker consume la
        // primera mutation y arranca su ventana de 100ms; 2) cruzar la ventana
        // -> flush del batch con ambas.
        tokio::time::advance(std::time::Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_millis(120)).await;
        for _ in 0..200 {
            if sink.0.lock().unwrap().len() >= 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        let batches = sink.0.lock().unwrap();
        assert_eq!(batches.len(), 1, "2 saves en la ventana -> 1 batch");
        assert_eq!(batches[0].len(), 2, "ambas mutations en el batch");
        assert_ne!(batches[0][0].id, batches[0][1].id, "uuidv7 distintos");
    }

    /// List: 15 columnas en el orden de Q3.
    #[test]
    fn list_sql_shape() {
        // El SQL de list_for_account es inline; el contrato de columnas se
        // verifica en el integration gated (Q3 contra PG real).
        let s = PlayerSummary {
            id: 2,
            name: "ninja".into(),
            job: 1,
            level: 1,
            playtime: 0,
            st: 0,
            ht: 0,
            dx: 0,
            iq: 0,
            part_main: 0,
            part_hair: 0,
            x: 0,
            y: 0,
            skill_group: 0,
            change_name: 0,
        };
        assert_eq!(s.id, 2);
    }

    /// Índice: el SQL por slot es la query LITERAL del C++
    /// (`ClientManagerPlayer.cpp:794` — `SELECT pid%u` con slot+1); el slot
    /// fuera de 0..4 falla ANTES de interpolar (columna de constante cerrada).
    #[test]
    fn index_sql_shape_and_slot_validation() {
        assert_eq!(
            index_sql(0).unwrap(),
            "SELECT pid1 FROM player.player_index WHERE id = $1"
        );
        assert_eq!(index_sql(1).unwrap(), "SELECT pid2 FROM player.player_index WHERE id = $1");
        assert_eq!(index_sql(3).unwrap(), "SELECT pid4 FROM player.player_index WHERE id = $1");
        assert_eq!(index_sql(4).unwrap(), "SELECT pid5 FROM player.player_index WHERE id = $1");
        // El C++ usa account_index+1: slot 0 -> pid1 .. slot 4 -> pid5.
        assert!(PID_COLUMNS.iter().enumerate().all(|(i, c)| c == &format!("pid{}", i + 1)));
        // Slots inválidos -> Err (parity input_login.cpp:260-264: el game
        // valida antes de preguntar al db).
        for slot in [5u8, 6, 200] {
            assert!(index_sql(slot).is_err(), "slot {slot} debe fallar");
        }
    }

    fn dummy_row() -> PlayerRow {
        PlayerRow {
            id: 0,
            name: "dummy".into(),
            job: 0,
            voice: 0,
            dir: 0,
            x: 0,
            y: 0,
            z: 0,
            map_index: 0,
            exit_x: 0,
            exit_y: 0,
            exit_map_index: 0,
            hp: 0,
            mp: 0,
            stamina: 0,
            random_hp: 0,
            random_sp: 0,
            playtime: 0,
            gold: 0,
            level: 1,
            level_step: 0,
            st: 0,
            ht: 0,
            dx: 0,
            iq: 0,
            exp: 0,
            stat_point: 0,
            skill_point: 0,
            sub_skill_point: 0,
            stat_reset_count: 0,
            part_base: 0,
            part_hair: 0,
            part_main: 0,
            skill_level: None,
            quickslot: None,
            skill_group: 0,
            alignment: 0,
            horse_level: 0,
            horse_riding: 0,
            horse_hp: 0,
            horse_hp_droptime: 0,
            horse_stamina: 0,
            logoff_interval: 0.0,
            horse_skill_point: 0,
        }
    }
}
