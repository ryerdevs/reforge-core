//! F3 (ADR-0008): dominio world — `ItemRepo` (schema `player`).
//!
//! # Tabla de paridad QID (legacy → Rust)
//!
//! | QID / query legacy | file:line | Metodo Rust | SQL / semantica |
//! |---|---|---|---|
//! | QID_ITEM (1) | `ClientManagerPlayer.cpp:326-332` y `:385-388` | `load_by_owner` | 22 columnas (`id, "window", pos, count, vnum, socket0..2, attrtype0..attrvalue6`), `WHERE owner_id = $1 AND window IN ('INVENTORY','EQUIPMENT','DRAGON_SOUL_INVENTORY','BELT_INVENTORY')`. El `window+0` del C++ (indice ENUM) no aplica: la columna PG es TEXT con el NOMBRE (el proxy los mapea). |
//! | QID_ITEM_SAVE (10) | `ClientManager.cpp:1400-1466` (`QUERY_ITEM_SAVE`) + `Cache.cpp:82` (flush del cache) | `upsert` / `upsert_mutation` | `INSERT ... ON DUPLICATE KEY UPDATE` de MySQL → `ON CONFLICT (id) DO UPDATE` (insert y update son la MISMA operacion). `id == 0` → `DEFAULT` (identity BY DEFAULT, regla B5). NOTA de arquitectura: el legacy cachea INVENTORY/EQUIPMENT/DS/BELT en memoria (`PutItemCache`, `ClientManager.cpp:1461`) y escribe SAFEBOX/MALL al momento; `QUERY_ITEM_FLUSH` (`:1387`) vacia el cache. El Rust NO cachea: el `Batcher` (≤100ms, una tx, WAL + replay idempotente) es el equivalente moderno del flush. |
//! | QID_ITEM_DESTROY (11) | `ClientManager.cpp:1692-1717` (`QUERY_ITEM_DESTROY`) | `delete` | `DELETE FROM player.item WHERE id = $1` (el cache del C++ decide borrar o escribir; aqui el DELETE es directo). |
//! | QID_ITEM_AWARD_LOAD (18) / TAKEN (19) | `ItemAwardManager.cpp:59-69` / `:166-168` | `load_pending_awards` / `take_award` | 23 columnas (`id, login, vnum, count, socket0..2, attrtype0..attrvalue6, mall, why`), `WHERE taken_time IS NULL AND id > $1`; taken = `UPDATE ... SET taken_time = NOW() WHERE id = $1 AND taken_time IS NULL` (idempotente). E2E Q6 `scripts/gpg/e2e_db.sh:148`. |
//! | ITEM_ID_RANGE | `ItemIDRangeManager.cpp:93` (BuildRange) y `:121` | `max_id_in_range` | `SELECT MAX(id) FROM player.item WHERE id >= $1 AND id <= $2` — el rango 100M-200M lo sondea el E2E Q8; `cs_dwMinimumRemainCount` (`:110`) es decision del game. |
//! | item_proto (uso+combate) | `TItemTable` (`type`/`sub_type`/`alValues`/`wearflag`) | `load_proto_use_values` | Subset del `player.item_proto` por vnum (18 columnas) — el equivalente moderno de `PROTO_FROM_DB` (ver lib.rs). |
//! | Unidad ACID (materials → resultado → oro, una tx) | ROADMAP.md:172, ADR-0011 (dupe completion F5) | `exchange_mutated` / `exchange_mutations` | Skeleton: valores ABSOLUTOS (parity del save legacy) + guard de estado previo (`= $pre`) que hace el replay del WAL un no-op exacto (0 filas) y da anti doble-gasto. |
//!
//! Tipos PG reales (verificados en el esquema): id bigint identity BY DEFAULT,
//! owner_id bigint, window text (check de los 7 windows), pos integer,
//! count/vnum/socket* bigint, attr*/attrvalue* smallint.

use std::collections::HashMap;

use crate::pool::{Client, PgPool};

use crate::account::pg_err;
use crate::wal::{Batcher, Mutation, Param};

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

/// Load del SAFEBOX (parity `RESULT_SAFEBOX_LOAD`,
/// `ClientManager.cpp:686-693`): MISMAS 22 columnas del QID_ITEM pero con
/// `owner_id = $1 AND window = 'SAFEBOX'` — el owner de los items de la caja
/// es la CUENTA (no el personaje; el C++ pasa `pi->account_id`).
const SAFEBOX_LOAD_SQL: &str = "\
SELECT id, \"window\", pos, count, vnum, socket0, socket1, socket2, \
attrtype0, attrvalue0, attrtype1, attrvalue1, attrtype2, attrvalue2, \
attrtype3, attrvalue3, attrtype4, attrvalue4, attrtype5, attrvalue5, \
attrtype6, attrvalue6 \
FROM player.item WHERE owner_id = $1 AND \"window\" = 'SAFEBOX'";

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

/// Proto del safebox: `size` (celdas del grid legacy), `antiflag`/`flag`
/// (bits `ITEM_ANTIFLAG_*`/`ITEM_FLAG_*` del TItemTable) por vnums — batch
/// `ANY($1)` (los handlers del safebox piden el entrante + los guardados).
const SAFEBOX_PROTO_SQL: &str = "SELECT vnum, size, antiflag, flag \
FROM player.item_proto WHERE vnum = ANY($1)";

/// Repositorio del dominio world (item). Conexion por llamada (ADR-0008).
pub struct ItemRepo {
    pool: PgPool,
}

/// Fila del item_proto (subset uso+combate): `type`/`sub_type` (el
/// `bType`/`bSubType` del TItemTable — ITEM_TYPE_WEAPON=1, ITEM_TYPE_ARMOR=2,
/// ARMOR_BODY=0/HEAD=1/SHIELD=2/FOOTS=4, ItemData.h:71-74,169-185) +
/// `value0..5` (`alValues`) + `wearflag` (los bits `WEARABLE_*` —
/// item_length.h:379-392, el slot del equip lo decide `FindEquipCell`,
/// item.cpp:509-623) + `weight` (columna `weight` del item_proto — el PESO
/// básico del lane D. La columna existe pero está a 0 en TODA la línea
/// verificada (PG 11 002 filas, dump MariaDB, pack del cliente — ver
/// weight.rs); el gate es fail-open hasta importar pesos clásicos). El
/// combate usa value3/4 (daño del arma)
/// y value5 (bonus); la armadura value1 + 2×value5; las pociones value0/1/3/4.
/// `applies` = los 3 pares (tipo, valor) del `aApplies[ITEM_APPLY_MAX_NUM]`
/// del TItemTable (tables.h:608-612 — columnas `applytype0..2`/
/// `applyvalue0..2`): el equip los aplica con `ModifyPoints` (item.cpp:
/// 718-735 — `ApplyPoint(aApplies[i].bType, ±lValue)`); el C27 (velocidad
/// de botas) lee el apply `APPLY_MOV_SPEED` (8) de aquí.
/// `magic_pct`/`socket_pct` = `bAlterToMagicItemPct`/`bGainSocketPct` del
/// TItemTable (columnas `magic_pct`/`socket_pct` — el lane de attrs
/// aleatorios los consume en `CreateItem`, item_manager.cpp:301-312).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtoItem {
    pub b_type: i16,
    pub b_sub_type: i16,
    /// Pares (bType, lValue) del `aApplies` — `APPLY_NONE` (0) = vacío.
    pub applies: [(i16, i32); 3],
    pub values: [i32; 6],
    pub wear_flag: i64,
    /// Peso del item (unidades crudas de la columna `weight`).
    pub weight: i64,
    /// `bAlterToMagicItemPct` — probabilidad % de attr mágico al crear
    /// (tinyint; 0 = nunca).
    pub magic_pct: i16,
    /// `bGainSocketPct` — nº de sockets abiertos al crear (tinyint).
    pub socket_pct: i16,
}

/// Proto del safebox por vnum (`load_safebox_proto` — columnas del
/// TItemTable legacy): `size` en CELDAS del grid (1×1..3×3, la columna 0
/// degenera a 1), `antiflag` = bits `ITEM_ANTIFLAG_*` (SAFEBOX = 1<<17,
/// STACK = 1<<15, item_length.h:331-377) y `flag` = bits `ITEM_FLAG_*`
/// (STACKABLE = 1<<2, :337).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SafeboxProto {
    pub size: u16,
    pub antiflag: i64,
    pub flag: i64,
}

/// Receta de refine (parity `TRefineTable` — `tables.h:924-933` + el load
/// de `ClientManagerBoot.cpp:121`): la fila de `refine_proto` por
/// `refine_set`. `cost` = fee base (el C++ la multiplica ×5 para el refine
/// NORMAL — `ComputeRefineFee`, char.cpp:6598 — y la cobra SIN multiplicar
/// en el refine con scroll), `prob` = probabilidad de éxito en %, y los
/// materiales `(vnum, count)` hasta 5 slots (`REFINE_MATERIAL_MAX_NUM`,
/// item_length.h:29; los no usados van con vnum 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefineRecipe {
    pub cost: i32,
    pub prob: i32,
    pub materials: [(i64, i32); 5],
}

impl ItemRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// pub(crate): lo usa también el impl de `load_attr_tables` (attr.rs).
    pub(crate) async fn connect(&self) -> Result<Client, String> {
        self.pool.get().await.map_err(|e| format!("PG pool get: {e}"))
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

    /// Load de los items de la CAJA (parity `RESULT_SAFEBOX_LOAD`,
    /// `ClientManager.cpp:686-693`): `window = 'SAFEBOX'`, owner = la
    /// CUENTA (`account_id` — el C++ pasa `pi->account_id`, no el pid).
    pub async fn load_safebox(&self, owner_id: i64) -> Result<Vec<ItemRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(SAFEBOX_LOAD_SQL, &[&owner_id])
            .await
            .map_err(|e| pg_err("SAFEBOX_ITEM_LOAD", &e))?;
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
    /// `alValues` + `type`/`sub_type` + `aApplies` del TItemTable del C++).
    /// El efecto de las pociones (`UseItemEx` → USE_POTION,
    /// char_item.cpp:4172-4204): `value0` = HP flat, `value1` = SP flat,
    /// `value3` = HP % (del máximo), `value4` = SP % (del máximo). El
    /// combate (`Item_GetDamage`, battle.cpp:442-462 + CalcMeleeDamage:
    /// 533,548): arma → `value3`/`value4` = daño min/max, `value5` = bonus
    /// ×2; armadura (char.cpp:2124-2125): `value1` + `2×value5`. Los
    /// `applies` (applytype0..2/applyvalue0..2) = el `aApplies` que el
    /// equip aplica (`ModifyPoints`, item.cpp:718-735) — las botas llevan
    /// `APPLY_MOV_SPEED` (8) ahí. `None` = el vnum no existe en item_proto.
    /// SQL inline (sin dependencia del mapeo de filas).
    pub async fn load_proto_use_values(
        &self,
        vnum: i64,
    ) -> Result<Option<ProtoItem>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(
                "SELECT type, subtype, \
                 applytype0, applyvalue0, applytype1, applyvalue1, applytype2, applyvalue2, \
                 value0, value1, value2, value3, value4, value5, wearflag, weight, \
                 magic_pct, socket_pct \
                 FROM player.item_proto WHERE vnum = $1",
                &[&vnum],
            )
            .await
            .map_err(|e| pg_err("ITEM_PROTO_USE", &e))?;
        let Some(r) = rows.first() else {
            return Ok(None);
        };
        let mut applies = [(0i16, 0i32); 3];
        for (i, slot) in applies.iter_mut().enumerate() {
            slot.0 = r.try_get(2 + 2 * i).map_err(|e| format!("item_proto.applytype{i}: {e}"))?;
            slot.1 = r.try_get(3 + 2 * i).map_err(|e| format!("item_proto.applyvalue{i}: {e}"))?;
        }
        let mut values = [0i32; 6];
        for (i, slot) in values.iter_mut().enumerate() {
            *slot = r.try_get(8 + i).map_err(|e| format!("item_proto.value{i}: {e}"))?;
        }
        // weight es smallint (int2) en el esquema — cast DESPUÉS (patrón
        // del fix shop.rs:284-289: leer el tipo real, cast a i64 después;
        // leer int2 como i64 daba "error deserializing column 9").
        let weight: i16 = r.try_get(15).map_err(|e| format!("item_proto.weight: {e}"))?;
        Ok(Some(ProtoItem {
            b_type: r.try_get(0).map_err(|e| format!("item_proto.type: {e}"))?,
            b_sub_type: r.try_get(1).map_err(|e| format!("item_proto.sub_type: {e}"))?,
            applies,
            values,
            wear_flag: r.try_get(14).map_err(|e| format!("item_proto.wearflag: {e}"))?,
            weight: i64::from(weight),
            magic_pct: r.try_get(16).map_err(|e| format!("item_proto.magic_pct: {e}"))?,
            socket_pct: r.try_get(17).map_err(|e| format!("item_proto.socket_pct: {e}"))?,
        }))
    }

    /// Subset REFINE del item_proto (parity `TItemTable` del C++ —
    /// `GetRefineSet()` item.h:157 / `GetRefinedVnum()` item.h:137):
    /// `(refine_set, refined_vnum)` del vnum. `refine_set` = el id de la
    /// receta en `refine_proto` (`wRefineSet`); `refined_vnum` = el vnum del
    /// item +1 de refine (`dwRefinedVnum` — 0 = sin siguiente nivel).
    /// Columnas: `refine_set` smallint, `refined_vnum` int unsigned
    /// (legacy-schema.md:175-176). `None` = vnum inexistente.
    pub async fn load_refine_proto(&self, vnum: i64) -> Result<Option<(i64, i64)>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(
                "SELECT refine_set, refined_vnum FROM player.item_proto \
                 WHERE vnum = $1",
                &[&vnum],
            )
            .await
            .map_err(|e| pg_err("ITEM_PROTO_REFINE", &e))?;
        let Some(r) = rows.first() else {
            return Ok(None);
        };
        let refine_set: i16 = r.try_get(0).map_err(|e| format!("item_proto.refine_set: {e}"))?;
        let refined_vnum: i32 = r.try_get(1).map_err(|e| format!("item_proto.refined_vnum: {e}"))?;
        Ok(Some((i64::from(refine_set), i64::from(refined_vnum))))
    }

    /// Reverse del refine (parity `ITEM_MANAGER::GetRefineFromVnum`,
    /// item_manager.cpp:1494-1501 + :76 — el mapa `refined_vnum → vnum`
    /// construido de TODOS los protos): el vnum que refine HACIA el dado
    /// (el nivel anterior). Lo usa el FAIL del refine con scroll
    /// (`GetRefineFromVnum`, char_item.cpp:1349-1352) para BAJAR el item
    /// (`result_fail_vnum`) en vez de destruirlo. `None` = el vnum no es
    /// el resultado de ningún refine.
    pub async fn load_refine_from_vnum(&self, vnum: i64) -> Result<Option<i64>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(
                "SELECT vnum FROM player.item_proto WHERE refined_vnum = $1 \
                 LIMIT 1",
                &[&vnum],
            )
            .await
            .map_err(|e| pg_err("ITEM_PROTO_REFINE_FROM", &e))?;
        let Some(r) = rows.first() else {
            return Ok(None);
        };
        let v: i32 = r.try_get(0).map_err(|e| format!("item_proto.vnum: {e}"))?;
        Ok(Some(i64::from(v)))
    }

    /// Receta de refine por `refine_set` (parity `CRefineManager::
    /// GetRefineRecipe` + el load de `ClientManagerBoot.cpp:121`:
    /// `SELECT id, cost, prob, vnum0, count0, ..., vnum4, count4 FROM
    /// refine_proto`): id = refine_set del item, cost = fee base, prob =
    /// probabilidad de éxito (%), y los 5 slots de material (vnum, count).
    /// `None` = sin receta para ese set.
    pub async fn load_refine_recipe(&self, id: i64) -> Result<Option<RefineRecipe>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(
                "SELECT id, cost, prob, \
                 vnum0, count0, vnum1, count1, vnum2, count2, \
                 vnum3, count3, vnum4, count4 \
                 FROM player.refine_proto WHERE id = $1",
                &[&id],
            )
            .await
            .map_err(|e| pg_err("REFINE_PROTO", &e))?;
        let Some(r) = rows.first() else {
            return Ok(None);
        };
        let mut materials = [(0i64, 0i32); 5];
        for (i, m) in materials.iter_mut().enumerate() {
            m.0 = r.try_get(3 + 2 * i).map_err(|e| format!("refine_proto.vnum{i}: {e}"))?;
            m.1 = r.try_get(4 + 2 * i).map_err(|e| format!("refine_proto.count{i}: {e}"))?;
        }
        Ok(Some(RefineRecipe {
            cost: r.try_get(1).map_err(|e| format!("refine_proto.cost: {e}"))?,
            prob: r.try_get(2).map_err(|e| format!("refine_proto.prob: {e}"))?,
            materials,
        }))
    }

    /// Datos de la VENTA al shop (shop_buy_price + lag del item_proto -
    /// parity shop_manager.cpp:297-319): el SellProto del lane social.
    /// SQL directo que vivia en server_realms::channel::shop (violaba
    /// ADR-0008 2 - acceso SOLO via repos); enrutado al crate database
    /// (mismo pool compartido). Devuelve (shop_buy_price, flag).
    pub async fn load_sell_proto(&self, vnum: i64) -> Result<(i64, i64), String> {
        let client = self.connect().await?;
        let row = client
            .query_one(
                "SELECT shop_buy_price, flag FROM player.item_proto WHERE vnum = $1",
                &[&vnum],
            )
            .await
            .map_err(|e| pg_err("ITEM_PROTO_SELL", &e))?;
        let shop_buy_price = row.try_get(0).map_err(|e| format!("shop_buy_price: {e}"))?;
        let flag = row.try_get(1).map_err(|e| format!("flag: {e}"))?;
        Ok((shop_buy_price, flag))
    }

    /// Protos del SAFEBOX por vnum — batch (`vnum = ANY($1)`): los gates del
    /// safebox (channel/safebox.rs) piden el item entrante + los guardados
    /// juntos (caja ≤ 15 slots → una query). Columnas del TItemTable legacy:
    /// `size` (CELDAS del grid, 1..3; 0 → 1 — fail-safe), `antiflag` (bits
    /// `ITEM_ANTIFLAG_*`, item_length.h:331-377: SAFEBOX = 1<<17, STACK =
    /// 1<<15) y `flag` (bits `ITEM_FLAG_*`: STACKABLE = 1<<2, :337). Los
    /// vnums sin fila NO aparecen en el mapa (el llamador decide: size 1 /
    /// sin flags).
    pub async fn load_safebox_proto(
        &self,
        vnums: &[i64],
    ) -> Result<HashMap<i64, SafeboxProto>, String> {
        if vnums.is_empty() {
            return Ok(HashMap::new());
        }
        let client = self.connect().await?;
        let rows = client
            .query(SAFEBOX_PROTO_SQL, &[&vnums])
            .await
            .map_err(|e| pg_err("ITEM_PROTO_SAFEBOX", &e))?;
        let mut out = HashMap::with_capacity(rows.len());
        for r in rows {
            let vnum: i64 = r.try_get(0).map_err(|e| format!("item_proto.vnum: {e}"))?;
            // size es smallint (int2): cast después (patrón shop.rs:284-289).
            let size: i16 = r.try_get(1).map_err(|e| format!("item_proto.size: {e}"))?;
            out.insert(
                vnum,
                SafeboxProto {
                    size: size.max(1) as u16,
                    antiflag: r.try_get(2).map_err(|e| format!("item_proto.antiflag: {e}"))?,
                    flag: r.try_get(3).map_err(|e| format!("item_proto.flag: {e}"))?,
                },
            );
        }
        Ok(out)
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

    // ------------------------------------------------------------- unidad ACID (F5)

    /// Unidad ACID durable (ADR-0011 "items as ACID units", ROADMAP.md:172):
    /// pushea TODAS las mutations de la unidad y fuerza el `flush()` — el
    /// batch entero aplica en UNA transaccion (+ audit en la misma tx) y esta
    /// llamada devuelve el resultado del commit. A diferencia de `save_mutated`
    /// (fire-and-forget), el slice de trade/refine SABE cuando la unidad
    /// commitio (Ok) o cuando el sink fallo (Err — el WAL local conserva el
    /// archivo para el replay del proximo arranque; ver la semantica del
    /// guard en `ItemExchange`: 0 filas del guard NO es un Err).
    ///
    /// Precondicion documentada: la unidad debe caber en un batch (max_batch)
    /// y los pushes + `flush()` deben ir sin pausas > `flush_interval`.
    pub async fn exchange_mutated(&self, batcher: &Batcher, ex: &ItemExchange) -> Result<(), String> {
        for m in exchange_mutations(ex) {
            batcher.push(m);
        }
        batcher.flush().await
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

/// Params de la mutation del upsert (Param tipado, no `&dyn ToSql`) — orden
/// $1..$23/$1..$22 de `UPSERT_SQL`/`UPSERT_DEFAULT_ID_SQL`.
fn item_params_mutation(row: &ItemRow, owner_id: i64) -> Vec<Param> {
    let p = |v: i64| Param::Int(v);
    let mut params = Vec::with_capacity(23);
    if row.id != 0 {
        params.push(p(row.id));
    }
    params.push(Param::Int(owner_id));
    params.push(Param::Text(row.window.clone()));
    params.push(p(i64::from(row.pos)));
    params.push(p(row.count));
    params.push(p(row.vnum));
    for s in &row.sockets {
        params.push(p(*s));
    }
    for (t, v) in &row.attrs {
        params.push(Param::Int(i64::from(*t)));
        params.push(Param::Int(i64::from(*v)));
    }
    params
}

/// Mutation durable del upsert: uuidv7 + el MISMO sql que el camino directo
/// (una fuente de verdad) + params. Compartida por `upsert_mutated` futuro y
/// por la unidad ACID (`exchange_mutations`).
pub(crate) fn upsert_mutation(row: &ItemRow, owner_id: i64) -> Mutation {
    let sql = if row.id == 0 { UPSERT_DEFAULT_ID_SQL } else { UPSERT_SQL };
    Mutation::new(sql, item_params_mutation(row, owner_id))
}

/// Unidad ACID (skeleton para los slices de trade/refine — ROADMAP.md:172):
/// materials → resultado → oro en UNA transaccion.
///
/// Todos los valores son ABSOLUTOS (parity del save legacy: el cache flush
/// escribe la fila completa — `Cache.cpp:82`), con guard de estado previo:
/// - materials: `UPDATE ... SET count = $post WHERE id = $1 AND count = $pre`
///   (o `DELETE ... WHERE id = $1 AND count = $pre` si `post == 0`).
/// - resultado: `upsert_mutation` (ON CONFLICT (id) — idempotente por
///   naturaleza; si el resultado stackea sobre un item existente, el caller
///   lo pasa con su id y el count_post sumado).
/// - oro: `UPDATE player.player SET gold = $post WHERE id = $1 AND gold = $pre`
///   (mismo patron de guard; `player.gold` es integer).
///
/// # Semantica del guard (importante)
///
/// El guard `= $pre` hace el replay del WAL un no-op EXACTO: tras el primer
/// commit count/gold == post != pre → 0 filas → no se vuelve a consumir ni a
/// duplicar. ESO es lo que exige ADR-0008/0011 (idempotencia del replay).
/// NO es un mecanismo de rechazo por concurrencia: 0 filas del guard NO hace
/// fallar la tx (el sink aplica las mutations sin comprobar filas afectadas;
/// el batch commit igual). Bajo single-writer-per-region (ADR-0011 — un solo
/// canal escribe los items de un player) no existe writer concurrente, asi
/// que el guard solo puede dispararse en replay — donde el no-op es correcto.
/// La validacion de negocio ("tienes suficientes materials") es del slice:
/// parity C++ — el game comprueba `count >= need` EN MEMORIA antes de
/// construir la unidad. Si un futuro topologia multi-writer necesita rechazo
/// estricto, el slice debe usar una transaccion sincrona con chequeo de
/// filas afectadas (client.transaction) en vez del Batcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemExchange {
    /// Owner de los materials y del resultado (player.item.owner_id).
    pub owner_id: i64,
    /// Materiales a consumir: `(item_id, count_pre, count_post)`.
    /// `count_post == 0` -> DELETE (stack vacio), si no UPDATE absoluto.
    pub materials: Vec<(i64, i64, i64)>,
    /// Resultado: item a upsertar (id == 0 -> identity DEFAULT) + su owner.
    pub result: Option<(ItemRow, i64)>,
    /// Oro del personaje: `(gold_pre, gold_post)` absolutos.
    pub gold: Option<(i64, i64)>,
}

/// Construye las mutations de la unidad (funcion pura — testeable sin PG).
pub(crate) fn exchange_mutations(ex: &ItemExchange) -> Vec<Mutation> {
    let mut out = Vec::with_capacity(ex.materials.len() + 2);
    for (id, pre, post) in &ex.materials {
        if *post == 0 {
            out.push(Mutation::new(
                "DELETE FROM player.item WHERE id = $1 AND count = $2",
                vec![Param::Int(*id), Param::Int(*pre)],
            ));
        } else {
            out.push(Mutation::new(
                "UPDATE player.item SET count = $2 WHERE id = $1 AND count = $3",
                vec![Param::Int(*id), Param::Int(*post), Param::Int(*pre)],
            ));
        }
    }
    if let Some((row, owner)) = &ex.result {
        out.push(upsert_mutation(row, *owner));
    }
    if let Some((pre, post)) = &ex.gold {
        out.push(Mutation::new(
            "UPDATE player.player SET gold = $2 WHERE id = $1 AND gold = $3",
            vec![Param::Int(ex.owner_id), Param::Int(*post), Param::Int(*pre)],
        ));
    }
    out
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

    /// Load del SAFEBOX: las MISMAS 22 columnas del QID_ITEM (mismo mapeo
    /// `item_from_row`) pero con el filtro `window = 'SAFEBOX'` y el owner =
    /// la CUENTA (parity `RESULT_SAFEBOX_LOAD`, ClientManager.cpp:686-693).
    #[test]
    fn safebox_load_sql_has_22_columns_and_safebox_window() {
        let select = SAFEBOX_LOAD_SQL.split_once(" FROM ").expect("FROM").0;
        let cols: Vec<&str> = select
            .trim_start_matches("SELECT")
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(cols.len(), 22, "id+window+pos+count+vnum+3 sockets+14 attrs");
        assert_eq!(cols[0], "id");
        assert_eq!(cols[21], "attrvalue6");
        assert_eq!(
            select, "SELECT id, \"window\", pos, count, vnum, socket0, socket1, socket2, \
attrtype0, attrvalue0, attrtype1, attrvalue1, attrtype2, attrvalue2, \
attrtype3, attrvalue3, attrtype4, attrvalue4, attrtype5, attrvalue5, \
attrtype6, attrvalue6",
            "mismas columnas que el QID_ITEM"
        );
        assert!(
            SAFEBOX_LOAD_SQL.contains("\"window\" = 'SAFEBOX'"),
            "filtro exclusivo del window SAFEBOX"
        );
        assert!(
            SAFEBOX_LOAD_SQL.contains("WHERE owner_id = $1"),
            "bind del owner (cuenta)"
        );
    }

    /// Proto del safebox: las 4 columnas del TItemTable que usan los gates
    /// (size/antiflag/flag) + batch por vnums (`vnum = ANY($1)` — una query
    /// para el item entrante + los guardados).
    #[test]
    fn safebox_proto_sql_shape_and_batch() {
        let q = "SELECT vnum, size, antiflag, flag FROM player.item_proto \
                 WHERE vnum = ANY($1)";
        assert_eq!(
            SAFEBOX_PROTO_SQL, q,
            "columnas del TItemTable + batch ANY($1)"
        );
        assert!(SAFEBOX_PROTO_SQL.contains("FROM player.item_proto"), "PROTO_FROM_DB");
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

    /// La mutation del upsert usa el MISMO sql que el camino directo (una
    /// fuente de verdad) + params tipados en el orden de los $N — para las
    /// dos variantes (id explicito / DEFAULT).
    #[test]
    fn upsert_mutation_uses_shared_sql_and_params() {
        let row = dummy_item();
        let m = upsert_mutation(&row, 7);
        assert_eq!(m.sql, UPSERT_SQL, "mismo SQL (una fuente de verdad)");
        assert_eq!(m.params.len(), 23);
        assert_eq!(m.params[0], Param::Int(row.id), "$1 = id explicito");
        assert_eq!(m.params[1], Param::Int(7), "$2 = owner");
        assert_eq!(m.params[2], Param::Text("INVENTORY".into()), "$3 = window");
        assert_eq!(m.params[3], Param::Int(0), "$4 = pos");
        assert_eq!(m.id[6] >> 4, 7, "version 7 del uuidv7");

        let mut default_row = dummy_item();
        default_row.id = 0;
        let m = upsert_mutation(&default_row, 7);
        assert_eq!(m.sql, UPSERT_DEFAULT_ID_SQL, "id==0 -> DEFAULT");
        assert_eq!(m.params.len(), 22, "sin id");
        assert_eq!(m.params[0], Param::Int(7), "$1 = owner");
    }

    /// Unidad ACID — materials: UPDATE absoluto con guard `count = $pre`
    /// (replay no-op) o DELETE con el mismo guard cuando el stack se vacia.
    #[test]
    fn exchange_mutations_consume_materials_with_guards() {
        let ex = ItemExchange {
            owner_id: 7,
            materials: vec![(100, 5, 2), (200, 3, 0)],
            result: None,
            gold: None,
        };
        let ms = exchange_mutations(&ex);
        assert_eq!(ms.len(), 2, "2 materials");
        // (100, 5, 2): UPDATE absoluto con guard pre.
        assert_eq!(
            ms[0].sql,
            "UPDATE player.item SET count = $2 WHERE id = $1 AND count = $3"
        );
        assert_eq!(ms[0].params, vec![Param::Int(100), Param::Int(2), Param::Int(5)]);
        // (200, 3, 0): DELETE con guard pre (el replay no borra un stack que ya no existe).
        assert_eq!(ms[1].sql, "DELETE FROM player.item WHERE id = $1 AND count = $2");
        assert_eq!(ms[1].params, vec![Param::Int(200), Param::Int(3)]);
    }

    /// Unidad ACID — resultado (upsert compartido) + oro (guard pre).
    #[test]
    fn exchange_mutations_include_result_upsert_and_gold() {
        let result = ItemRow {
            id: 0,
            window: "INVENTORY".into(),
            pos: 3,
            count: 1,
            vnum: 30001,
            sockets: [0, 0, 0],
            attrs: [(0, 0); 7],
        };
        let ex = ItemExchange {
            owner_id: 7,
            materials: vec![(100, 5, 2)],
            result: Some((result.clone(), 7)),
            gold: Some((1_000, 1_200)),
        };
        let ms = exchange_mutations(&ex);
        assert_eq!(ms.len(), 3, "material + resultado + oro");
        assert_eq!(ms[1].sql, UPSERT_DEFAULT_ID_SQL, "resultado id==0 -> DEFAULT");
        assert_eq!(ms[1].params[0], Param::Int(7), "owner del resultado");
        assert_eq!(
            ms[2].sql,
            "UPDATE player.player SET gold = $2 WHERE id = $1 AND gold = $3"
        );
        assert_eq!(ms[2].params, vec![Param::Int(7), Param::Int(1_200), Param::Int(1_000)]);
    }

    /// Unidad vacia -> sin mutations (flush no-op con Ok).
    #[test]
    fn exchange_mutations_empty_unit_is_empty() {
        let ex = ItemExchange { owner_id: 7, materials: vec![], result: None, gold: None };
        assert!(exchange_mutations(&ex).is_empty(), "sin materials/resultado/oro");
    }

    /// Wiring del Batcher: `exchange_mutated` pushea TODAS las mutations y
    /// fuerza el flush — el sink recibe la unidad entera en UN batch (una tx).
    #[tokio::test(start_paused = true)]
    async fn exchange_mutated_wires_unit_to_single_batch() {
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
        let batcher = Batcher::spawn(std::time::Duration::from_millis(1000), 64, sink.clone());
        let repo = ItemRepo::new(crate::pool::new_pool("host=127.0.0.1 port=1 user=x password=x dbname=x", 2).expect("pool"));
        let ex = ItemExchange {
            owner_id: 7,
            materials: vec![(100, 5, 2), (200, 3, 0)],
            result: Some((dummy_item(), 7)),
            gold: Some((1_000, 1_200)),
        };
        repo.exchange_mutated(&batcher, &ex).await.expect("flush ok");
        let batches = sink.0.lock().unwrap();
        assert_eq!(batches.len(), 1, "la unidad entera en UN batch");
        assert_eq!(batches[0].len(), 4, "2 materials + resultado + oro");
        // El id es uuidv7 propio de cada push — comparamos sql+params.
        let expected = exchange_mutations(&ex);
        for (got, want) in batches[0].iter().zip(&expected) {
            assert_eq!(got.sql, want.sql, "sql de la mutation");
            assert_eq!(got.params, want.params, "params de la mutation");
            assert_eq!(got.id[6] >> 4, 7, "version 7 del uuidv7");
        }
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
