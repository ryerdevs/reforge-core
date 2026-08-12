//! F3 (ADR-0008): dominio world — `ItemRepo` (schema `player`).
//!
//! Contrato portado de los QIDs de item legacy:
//! - `load_by_owner` = QID_ITEM (`ClientManagerPlayer.cpp:385-387`): las 23
//!   columnas del `TPlayerItem` con `"window"` citado (en PG `window` es
//!   palabra reservada; el `window+0` del C++/proxy no aplica aqui — la
//!   columna PG es TEXT con el NOMBRE del window, no el indice ENUM).
//! - `upsert` = `QUERY_ITEM_SAVE` (`ClientManager.cpp:1425-1452`): el
//!   `INSERT ... ON DUPLICATE KEY UPDATE` de MySQL -> `ON CONFLICT (id) DO
//!   UPDATE` (insert y update son la MISMA operacion en el legacy). `id == 0`
//!   -> `DEFAULT` (identity BY DEFAULT, regla B5 del proxy).
//! - `delete` = `QUERY_ITEM_DESTROY` (`ClientManager.cpp:1702`).
//! - `item_award` = `RequestLoad`/`Taken` (`ItemAwardManager.cpp:59-69` y
//!   `:166-168`; E2E Q6 `scripts/gpg/e2e_db.sh:148`).
//!
//! Tipos PG reales (verificados en el esquema): id bigint identity BY DEFAULT,
//! owner_id bigint, window text (check de los 7 windows), pos integer,
//! count/vnum/socket* bigint, attr*/attrvalue* smallint.

use tokio_postgres::{Client, NoTls};

use crate::account::pg_err;

/// Item del inventario (23 columnas del load QID_ITEM; los atributos son
/// pares (tipo, valor) — parity `TPlayerItem`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRow {
    pub id: i64,
    /// Nombre del window en PG ("INVENTORY", "EQUIPMENT", ...) — en el C++ es
    /// el indice ENUM; la conversion vive en la capa de protocolo.
    pub window: String,
    pub pos: i32,
    pub count: i64,
    pub vnum: i64,
    pub sockets: [i64; 3],
    pub attrs: [(i16, i16); 7],
}

/// Load QID_ITEM: 23 columnas en el orden de `ClientManagerPlayer.cpp:385-387`.
const LOAD_SQL: &str = "\
SELECT id, \"window\", pos, count, vnum, socket0, socket1, socket2, \
attrtype0, attrvalue0, attrtype1, attrvalue1, attrtype2, attrvalue2, \
attrtype3, attrvalue3, attrtype4, attrvalue4, attrtype5, attrvalue5, \
attrtype6, attrvalue6 \
FROM player.item WHERE owner_id = $1 AND \"window\" IN \
('INVENTORY','EQUIPMENT','DRAGON_SOUL_INVENTORY','BELT_INVENTORY')";

/// Upsert con id explicito (el id lo asigna el game desde ITEM_ID_RANGE —
/// `ItemIDRangeManager.cpp:93,121`; el E2E Q8 sondea el rango 100M-200M).
const UPSERT_SQL: &str = "\
INSERT INTO player.item \
(id, owner_id, \"window\", pos, count, vnum, socket0, socket1, socket2, \
attrtype0, attrvalue0, attrtype1, attrvalue1, attrtype2, attrvalue2, \
attrtype3, attrvalue3, attrtype4, attrvalue4, attrtype5, attrvalue5, \
attrtype6, attrvalue6) \
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, \
$10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23) \
ON CONFLICT (id) DO UPDATE SET \
owner_id = EXCLUDED.owner_id, \"window\" = EXCLUDED.\"window\", \
pos = EXCLUDED.pos, count = EXCLUDED.count, vnum = EXCLUDED.vnum, \
socket0 = EXCLUDED.socket0, socket1 = EXCLUDED.socket1, socket2 = EXCLUDED.socket2, \
attrtype0 = EXCLUDED.attrtype0, attrvalue0 = EXCLUDED.attrvalue0, \
attrtype1 = EXCLUDED.attrtype1, attrvalue1 = EXCLUDED.attrvalue1, \
attrtype2 = EXCLUDED.attrtype2, attrvalue2 = EXCLUDED.attrvalue2, \
attrtype3 = EXCLUDED.attrtype3, attrvalue3 = EXCLUDED.attrvalue3, \
attrtype4 = EXCLUDED.attrtype4, attrvalue4 = EXCLUDED.attrvalue4, \
attrtype5 = EXCLUDED.attrtype5, attrvalue5 = EXCLUDED.attrvalue5, \
attrtype6 = EXCLUDED.attrtype6, attrvalue6 = EXCLUDED.attrvalue6 \
RETURNING id";

/// Upsert con `id = DEFAULT` (identity BY DEFAULT — regla B5).
const UPSERT_DEFAULT_ID_SQL: &str = "\
INSERT INTO player.item \
(owner_id, \"window\", pos, count, vnum, socket0, socket1, socket2, \
attrtype0, attrvalue0, attrtype1, attrvalue1, attrtype2, attrvalue2, \
attrtype3, attrvalue3, attrtype4, attrvalue4, attrtype5, attrvalue5, \
attrtype6, attrvalue6) \
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, \
$9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22) \
ON CONFLICT (id) DO UPDATE SET \
owner_id = EXCLUDED.owner_id, \"window\" = EXCLUDED.\"window\", \
pos = EXCLUDED.pos, count = EXCLUDED.count, vnum = EXCLUDED.vnum, \
socket0 = EXCLUDED.socket0, socket1 = EXCLUDED.socket1, socket2 = EXCLUDED.socket2, \
attrtype0 = EXCLUDED.attrtype0, attrvalue0 = EXCLUDED.attrvalue0, \
attrtype1 = EXCLUDED.attrtype1, attrvalue1 = EXCLUDED.attrvalue1, \
attrtype2 = EXCLUDED.attrtype2, attrvalue2 = EXCLUDED.attrvalue2, \
attrtype3 = EXCLUDED.attrtype3, attrvalue3 = EXCLUDED.attrvalue3, \
attrtype4 = EXCLUDED.attrtype4, attrvalue4 = EXCLUDED.attrvalue4, \
attrtype5 = EXCLUDED.attrtype5, attrvalue5 = EXCLUDED.attrvalue5, \
attrtype6 = EXCLUDED.attrtype6, attrvalue6 = EXCLUDED.attrvalue6 \
RETURNING id";

/// Delete del destroy (`ClientManager.cpp:1702`).
const DELETE_SQL: &str = "DELETE FROM player.item WHERE id = $1";

/// Repositorio del dominio world (item). Conexion por llamada (ADR-0008).
pub struct ItemRepo {
    pg_conn: String,
}

/// Fila del item_proto (subset uso+combate): `type`/`sub_type` (el
/// `bType`/`bSubType` del TItemTable — ITEM_TYPE_WEAPON=1, ITEM_TYPE_ARMOR=2,
/// ARMOR_BODY=0/HEAD=1/SHIELD=2/FOOTS=4, ItemData.h:71-74,169-185) +
/// `value0..5` (`alValues`). El combate usa value3/4 (daño del arma) y
/// value5 (bonus); la armadura value1 + 2×value5; las pociones value0/1/3/4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtoItem {
    pub b_type: i16,
    pub b_sub_type: i16,
    pub values: [i32; 6],
}

impl ItemRepo {
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

    /// Load del QID_ITEM: los items de los 4 windows del personaje.
    pub async fn load_by_owner(&self, owner_id: i64) -> Result<Vec<ItemRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(LOAD_SQL, &[&owner_id])
            .await
            .map_err(|e| pg_err("ITEM_LOAD", &e))?;
        rows.iter().map(item_from_row).collect()
    }

    /// Upsert (insert y update son la MISMA operacion en el legacy —
    /// `INSERT ... ON DUPLICATE KEY UPDATE`, `ClientManager.cpp:1451`).
    /// `row.id == 0` -> `DEFAULT` (identity). `owner_id` es del lado de
    /// escritura (el load QID_ITEM no lo selecciona). Devuelve el id efectivo
    /// (explicito o generado).
    pub async fn upsert(&self, row: &ItemRow, owner_id: i64) -> Result<i64, String> {
        let client = self.connect().await?;
        let (sql, params): (&str, Vec<&(dyn tokio_postgres::types::ToSql + Sync)>) = if row.id == 0 {
            (UPSERT_DEFAULT_ID_SQL, item_params_without_id(row, &owner_id))
        } else {
            (UPSERT_SQL, item_params_with_id(row, &owner_id))
        };
        let r = client
            .query_one(sql, &params)
            .await
            .map_err(|e| pg_err("ITEM_UPSERT", &e))?;
        r.try_get(0).map_err(|e| format!("ITEM_UPSERT id: {e}"))
    }

    /// Delete del destroy (`ClientManager.cpp:1702`). Devuelve filas borradas.
    pub async fn delete(&self, id: i64) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(DELETE_SQL, &[&id])
            .await
            .map_err(|e| pg_err("ITEM_DESTROY", &e))
    }

    /// Valores de USO y COMBATE del item_proto (`player.item_proto` —
    /// `alValues` + `type`/`sub_type` del TItemTable del C++). El efecto de
    /// las pociones (`UseItemEx` → USE_POTION, char_item.cpp:4172-4204):
    /// `value0` = HP flat, `value1` = SP flat, `value3` = HP % (del máximo),
    /// `value4` = SP % (del máximo). El combate (`Item_GetDamage`,
    /// battle.cpp:442-462 + CalcMeleeDamage:533,548): arma →
    /// `value3`/`value4` = daño min/max, `value5` = bonus ×2; armadura
    /// (char.cpp:2124-2125): `value1` + `2×value5`. `None` = el vnum no
    /// existe en item_proto. SQL inline (sin dependencia del mapeo de filas).
    pub async fn load_proto_use_values(
        &self,
        vnum: i64,
    ) -> Result<Option<ProtoItem>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(
                "SELECT type, sub_type, value0, value1, value2, value3, value4, value5 \
                 FROM player.item_proto WHERE vnum = $1",
                &[&vnum],
            )
            .await
            .map_err(|e| pg_err("ITEM_PROTO_USE", &e))?;
        let Some(r) = rows.first() else {
            return Ok(None);
        };
        let mut values = [0i32; 6];
        for (i, slot) in values.iter_mut().enumerate() {
            *slot = r.try_get(2 + i).map_err(|e| format!("item_proto.value{i}: {e}"))?;
        }
        Ok(Some(ProtoItem {
            b_type: r.try_get(0).map_err(|e| format!("item_proto.type: {e}"))?,
            b_sub_type: r.try_get(1).map_err(|e| format!("item_proto.sub_type: {e}"))?,
            values,
        }))
    }

    /// Probe del rango de ids (`ItemIDRangeManager.cpp:93,121` — E2E Q8):
    /// `MAX(id)` dentro de [min, max]. `None` = rango vacio.
    pub async fn max_id_in_range(&self, min: i64, max: i64) -> Result<Option<i64>, String> {
        let client = self.connect().await?;
        let row = client
            .query_one(
                "SELECT MAX(id) FROM player.item WHERE id >= $1 AND id <= $2",
                &[&min, &max],
            )
            .await
            .map_err(|e| pg_err("ITEM_ID_RANGE", &e))?;
        row.try_get(0).map_err(|e| format!("ITEM_ID_RANGE: {e}"))
    }

    // ------------------------------------------------------------------ item_award

    /// Award pendiente (23 columnas de `RequestLoad`, `ItemAwardManager.cpp:59-69`).
    pub async fn load_pending_awards(&self, last_cached_id: i64) -> Result<Vec<ItemAward>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(ITEM_AWARD_LOAD_SQL, &[&last_cached_id])
            .await
            .map_err(|e| pg_err("ITEM_AWARD_LOAD", &e))?;
        rows.iter().map(item_award_from_row).collect()
    }

    /// Marcar tomado (`Taken`, `ItemAwardManager.cpp:166-168`): idempotente por
    /// el `AND taken_time IS NULL`. Devuelve filas afectadas (1 = tomado ahora,
    /// 0 = ya tomado o no existe).
    pub async fn take_award(&self, award_id: i64, item_id: i64) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(
                "UPDATE player.item_award SET taken_time = NOW(), item_id = $2 \
WHERE id = $1 AND taken_time IS NULL",
                &[&award_id, &item_id],
            )
            .await
            .map_err(|e| pg_err("ITEM_AWARD_TAKEN", &e))
    }
}

/// Fila de item_award pendiente (23 columnas, E2E Q6 `e2e_db.sh:148`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemAward {
    pub id: i64,
    pub login: String,
    pub vnum: i64,
    pub count: i64,
    pub sockets: [i64; 3],
    pub attrs: [(i16, i16); 7],
    pub mall: i16,
    /// `why` es nullable en PG (varchar(128)).
    pub why: Option<String>,
}

/// Load de awards pendientes: `WHERE taken_time IS NULL AND id > $1`
/// (el C++ solo re-carga awards mas nuevos que el ultimo cacheado).
const ITEM_AWARD_LOAD_SQL: &str = "\
SELECT id, login, vnum, count, socket0, socket1, socket2, \
attrtype0, attrvalue0, attrtype1, attrvalue1, attrtype2, attrvalue2, \
attrtype3, attrvalue3, attrtype4, attrvalue4, attrtype5, attrvalue5, \
attrtype6, attrvalue6, mall, why \
FROM player.item_award WHERE taken_time IS NULL AND id > $1";

fn item_award_from_row(r: &tokio_postgres::Row) -> Result<ItemAward, String> {
    let g = |i: usize| -> Result<i64, String> { r.try_get(i).map_err(|e| format!("col{i}: {e}")) };
    let gs = |i: usize| -> Result<i16, String> { r.try_get(i).map_err(|e| format!("col{i}: {e}")) };
    let mut attrs = [(0i16, 0i16); 7];
    for (i, slot) in attrs.iter_mut().enumerate() {
        *slot = (gs(8 + 2 * i)?, gs(9 + 2 * i)?);
    }
    Ok(ItemAward {
        id: g(0)?,
        login: r.try_get(1).map_err(|e| format!("col1: {e}"))?,
        vnum: g(2)?,
        count: g(3)?,
        sockets: [g(4)?, g(5)?, g(6)?],
        attrs,
        mall: gs(21)?,
        why: r.try_get(22).map_err(|e| format!("col22: {e}"))?,
    })
}

/// Mapeo de las 22 columnas del load (orden QID_ITEM:
/// id, window, pos, count, vnum, socket0..2, attrtype0..6, attrvalue0..6).
fn item_from_row(r: &tokio_postgres::Row) -> Result<ItemRow, String> {
    let g = |i: usize| -> Result<i64, String> { r.try_get(i).map_err(|e| format!("col{i}: {e}")) };
    let gs = |i: usize| -> Result<i16, String> { r.try_get(i).map_err(|e| format!("col{i}: {e}")) };
    let mut attrs = [(0i16, 0i16); 7];
    for (i, slot) in attrs.iter_mut().enumerate() {
        *slot = (gs(8 + 2 * i)?, gs(9 + 2 * i)?);
    }
    Ok(ItemRow {
        id: g(0)?,
        window: r.try_get(1).map_err(|e| format!("col1: {e}"))?,
        pos: r.try_get(2).map_err(|e| format!("col2: {e}"))?,
        count: g(3)?,
        vnum: g(4)?,
        sockets: [g(5)?, g(6)?, g(7)?],
        attrs,
    })
}

fn item_params_with_id<'a>(
    row: &'a ItemRow,
    owner_id: &'a i64,
) -> Vec<&'a (dyn tokio_postgres::types::ToSql + Sync)> {
    vec![
        &row.id, owner_id, &row.window, &row.pos, &row.count, &row.vnum, //
        &row.sockets[0], &row.sockets[1], &row.sockets[2], //
        &row.attrs[0].0, &row.attrs[0].1, &row.attrs[1].0, &row.attrs[1].1, //
        &row.attrs[2].0, &row.attrs[2].1, &row.attrs[3].0, &row.attrs[3].1, //
        &row.attrs[4].0, &row.attrs[4].1, &row.attrs[5].0, &row.attrs[5].1, //
        &row.attrs[6].0, &row.attrs[6].1,
    ]
}

fn item_params_without_id<'a>(
    row: &'a ItemRow,
    owner_id: &'a i64,
) -> Vec<&'a (dyn tokio_postgres::types::ToSql + Sync)> {
    vec![
        owner_id, &row.window, &row.pos, &row.count, &row.vnum, //
        &row.sockets[0], &row.sockets[1], &row.sockets[2], //
        &row.attrs[0].0, &row.attrs[0].1, &row.attrs[1].0, &row.attrs[1].1, //
        &row.attrs[2].0, &row.attrs[2].1, &row.attrs[3].0, &row.attrs[3].1, //
        &row.attrs[4].0, &row.attrs[4].1, &row.attrs[5].0, &row.attrs[5].1, //
        &row.attrs[6].0, &row.attrs[6].1,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Load: 22 columnas en el orden de `ClientManagerPlayer.cpp:385-387`
    /// (id+window+pos+count+vnum+3 sockets+14 attrs; window citado —
    /// reserved en PG).
    #[test]
    fn load_sql_has_22_columns_in_contract_order() {
        let select = LOAD_SQL.split_once(" FROM ").expect("FROM").0;
        let cols: Vec<&str> = select
            .trim_start_matches("SELECT")
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(cols.len(), 22, "id+window+pos+count+vnum+3 sockets+14 attrs");
        assert_eq!(cols[0], "id");
        assert_eq!(cols[1], "\"window\"", "citado (reserved en PG)");
        assert_eq!(cols[2], "pos");
        assert_eq!(cols[3], "count");
        assert_eq!(cols[4], "vnum");
        assert_eq!(cols[5..8], ["socket0", "socket1", "socket2"]);
        assert_eq!(cols[8], "attrtype0");
        assert_eq!(cols[9], "attrvalue0");
        assert_eq!(cols[20], "attrtype6");
        assert_eq!(cols[21], "attrvalue6");
        assert!(
            LOAD_SQL.contains("WHERE owner_id = $1"),
            "bind del owner (QID_ITEM)"
        );
        assert!(
            LOAD_SQL.contains("('INVENTORY','EQUIPMENT','DRAGON_SOUL_INVENTORY','BELT_INVENTORY')"),
            "filtro de windows"
        );
    }

    /// Upsert: ON CONFLICT (id) + variante DEFAULT para id==0; ambos con
    /// RETURNING id (el repo devuelve el id efectivo).
    #[test]
    fn upsert_sql_conflict_and_default_variant() {
        assert!(UPSERT_SQL.contains("ON CONFLICT (id) DO UPDATE"));
        assert!(UPSERT_SQL.contains("attrtype6 = EXCLUDED.attrtype6"));
        assert!(UPSERT_SQL.ends_with("RETURNING id"), "query_one necesita 1 fila");
        assert!(UPSERT_SQL.starts_with("INSERT INTO player.item"));
        assert!(UPSERT_DEFAULT_ID_SQL.starts_with("INSERT INTO player.item"));
        assert!(!UPSERT_DEFAULT_ID_SQL.contains("(id,"), "id por DEFAULT");
        assert!(UPSERT_DEFAULT_ID_SQL.ends_with("RETURNING id"));
        // delete: por id.
        assert_eq!(DELETE_SQL, "DELETE FROM player.item WHERE id = $1");
    }

    /// Param counts: 23 con id, 22 con DEFAULT.
    #[test]
    fn upsert_param_counts() {
        let row = dummy_item();
        let oid = 7i64;
        assert_eq!(item_params_with_id(&row, &oid).len(), 23);
        assert_eq!(item_params_without_id(&row, &oid).len(), 22);
    }

    /// item_award load: 23 columnas en el orden de `RequestLoad`
    /// (`ItemAwardManager.cpp:59-69`, E2E Q6 `e2e_db.sh:148`) + filtro
    /// `taken_time IS NULL AND id > $1`.
    #[test]
    fn item_award_load_sql_shape() {
        let cols: Vec<&str> = ITEM_AWARD_LOAD_SQL
            .split_once(" FROM ")
            .expect("FROM")
            .0
            .trim_start_matches("SELECT")
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(cols.len(), 23, "id+login+vnum+count+3 sockets+14 attrs+mall+why");
        assert_eq!(cols[0], "id");
        assert_eq!(cols[1], "login");
        assert_eq!(cols[2], "vnum");
        assert_eq!(cols[3], "count");
        assert_eq!(cols[4..7], ["socket0", "socket1", "socket2"]);
        assert_eq!(cols[7], "attrtype0");
        assert_eq!(cols[8], "attrvalue0");
        assert_eq!(cols[19], "attrtype6");
        assert_eq!(cols[20], "attrvalue6");
        assert_eq!(cols[21], "mall");
        assert_eq!(cols.last(), Some(&"why"), "why es la ultima columna");
        assert!(
            ITEM_AWARD_LOAD_SQL.contains("WHERE taken_time IS NULL AND id > $1"),
            "filtro pendientes + last-cached"
        );
    }

    fn dummy_item() -> ItemRow {
        ItemRow {
            id: 100_000_001,
            window: "INVENTORY".into(),
            pos: 0,
            count: 1,
            vnum: 1,
            sockets: [0, 0, 0],
            attrs: [(0, 0); 7],
        }
    }
}
