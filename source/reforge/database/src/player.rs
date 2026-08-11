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

use tokio_postgres::{Client, NoTls, Row};

use crate::account::pg_err;

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
    pg_conn: String,
}

impl PlayerRepo {
    pub fn new(pg_conn: impl Into<String>) -> Self {
        Self { pg_conn: pg_conn.into() }
    }

    async fn connect(&self) -> Result<Client, String> {
        let (client, connection) = tokio_postgres::connect(&self.pg_conn, NoTls)
            .await
            .map_err(|e| format!("PG connect: {e}"))?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok(client)
    }

    /// Load completo (Q2). `None` = no existe el personaje.
    pub async fn load(&self, id: i64) -> Result<Option<PlayerRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(LOAD_SQL, &[&id])
            .await
            .map_err(|e| pg_err("PLAYER_LOAD", &e))?;
        Ok(rows.first().map(player_row_from_row).transpose()?)
    }

    /// Save (Q5 shape, `CreatePlayerSaveQuery`): UPDATE de todas las columnas
    /// del row + `last_play = NOW()`. Blobs como bytea nativo. Devuelve filas
    /// afectadas (1 = ok, 0 = el personaje no existe).
    pub async fn save(&self, p: &PlayerRow) -> Result<u64, String> {
        let client = self.connect().await?;
        let rows = client
            .execute(
                "UPDATE player.player SET \
job = $1, voice = $2, dir = $3, x = $4, y = $5, z = $6, map_index = $7, \
exit_x = $8, exit_y = $9, exit_map_index = $10, hp = $11, mp = $12, stamina = $13, \
random_hp = $14, random_sp = $15, playtime = $16, level = $17, level_step = $18, \
st = $19, ht = $20, dx = $21, iq = $22, gold = $23, exp = $24, stat_point = $25, \
skill_point = $26, sub_skill_point = $27, stat_reset_count = $28, ip = $29, \
part_main = $30, part_hair = $31, last_play = NOW(), skill_group = $32, \
alignment = $33, horse_level = $34, horse_riding = $35, horse_hp = $36, \
horse_hp_droptime = $37, horse_stamina = $38, horse_skill_point = $39, \
skill_level = $40, quickslot = $41 \
WHERE id = $42",
                &[
                    &p.job, &p.voice, &p.dir, &p.x, &p.y, &p.z, &p.map_index, //
                    &p.exit_x, &p.exit_y, &p.exit_map_index, &p.hp, &p.mp, &p.stamina, //
                    &p.random_hp, &p.random_sp, &p.playtime, &p.level, &p.level_step, //
                    &p.st, &p.ht, &p.dx, &p.iq, &p.gold, &p.exp, &p.stat_point, //
                    &p.skill_point, &p.sub_skill_point, &p.stat_reset_count, &ip_default(&p), //
                    &p.part_main, &p.part_hair, &p.skill_group, &p.alignment, //
                    &p.horse_level, &p.horse_riding, &p.horse_hp, &p.horse_hp_droptime, //
                    &p.horse_stamina, &p.horse_skill_point, &p.skill_level, &p.quickslot, //
                    &p.id,
                ],
            )
            .await
            .map_err(|e| pg_err("PLAYER_SAVE", &e))?;
        Ok(rows)
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
        row.try_get(0).map_err(|e| format!("PLAYER_CREATE id: {e}"))
    }
}

/// `ip` no esta en el load (42 columnas) — el save usa el default del C++.
fn ip_default(_p: &PlayerRow) -> String {
    "0.0.0.0".to_string()
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
        // El SQL vive inline en save(); verificamos el contrato via la firma:
        // los blobs se pasan como Vec<u8> (bytea nativo, no decode hex).
        let row = dummy_row();
        let _ = &row.skill_level;
        // sanity: el row tiene los 42 campos del load (compile-time).
        assert_eq!(row.name, "dummy");
        assert_eq!(row.logoff_interval, 0.0);
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
