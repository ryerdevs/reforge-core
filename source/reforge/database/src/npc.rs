//! F5 (ADR-0008): dominio world — `MobRepo` (schema `player`, tabla
//! `mob_proto`). Lectura de la tabla de mobs para el spawn del mapa (F5):
//! el subset que el spawn necesita.
//!
//! Tipos PG reales (verificados contra el esquema migrado por G-PG,
//! 2026-08-11):
//! - `vnum` bigint (PK) — ES el race del wire: el cliente resuelve el modelo
//!   y el nombre del mob por `wRaceNum` (= vnum) desde SU pack (multilang
//!   §17 — `CPythonNonPlayer::LoadNonPlayerData`).
//! - `name` varchar(24) / `locale_name` bytea — el nombre del mob; el C++
//!   usa `locale_name` como `GetName()` del spawn (`char_manager.cpp:309/409`
//!   `CreateCharacter(pkMob->m_table.szLocaleName)`).
//! - `type`/`battle_type` **smallint** (NO ENUM): índices numéricos del
//!   legacy (`tables.h:445-447` `bType`/`bBattleType`). El wire los usa tal
//!   cual: `bType` del `TPacketGCCharacterAdd` = `GetCharType()` =
//!   `m_bCharType` = `mob_proto.type` (`char.cpp SetProto`).
//! - `level` integer — `bLevel` de la tabla.
//! - `size`/`ai_flag` **TEXT** (las ENUM/SET del legacy migradas a texto) —
//!   el spawn del cliente NO las necesita (renderiza con race+folder del
//!   pack; el C++ las usa para combat/AI — F5, fuera del wire del spawn).
//! - `folder` varchar(100) — el motion del cliente; no participa en el wire.

use tokio_postgres::{Client, NoTls, Row};

use crate::account::pg_err;

/// Fila del subset de spawn (`mob_proto`). Tipos PG reales
/// (bigint/varchar/bytea/smallint/integer/text — verificados 2026-08-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MobRow {
    /// `vnum` (bigint, PK) — el race del wire (`wRaceNum`).
    pub vnum: i64,
    /// `name` (varchar 24) — nombre original (CP949 en el legacy; el dump ES
    /// del 2026-08-08 lo dejó en español). No participa en el wire del spawn.
    pub name: String,
    /// `locale_name` (bytea) — bytes CRUDOS del nombre (CP949 en el runtime);
    /// el C++ los manda como `GetName()` del spawn. El cliente los usa SOLO
    /// como fallback para NPCs (multilang §17 — su pack manda).
    pub locale_name: Vec<u8>,
    /// `type` (smallint) — `bType` de la tabla: 0 MONSTER, 1 NPC, 2 STONE,
    /// 3 WARP, 4 DOOR, 5 BUILDING, 6 PC, 9 GOTO (`length.h:330`). Es el
    /// `bType` del `TPacketGCCharacterAdd` (parity `GetCharType()`).
    pub b_type: i16,
    /// `battle_type` (smallint) — MELEE/RANGE/etc (`tables.h:447`); combat F5.
    pub battle_type: i16,
    /// `level` (integer) — `bLevel` de la tabla; el C++ lo manda en el
    /// addInfo SOLO con ENABLE_SHOWNPCLEVEL (off en este build -> 0 para NPCs).
    pub level: i32,
    /// `size` (TEXT — la ENUM legacy SMALL/MEDIUM/LARGE migrada a texto).
    /// No participa en el wire del spawn (GAP documentado).
    pub size: String,
    /// `ai_flag` (TEXT — el SET legacy migrado a texto, p.ej. "COWARD").
    /// No participa en el wire del spawn (combat F5).
    pub ai_flag: Option<String>,
    /// `folder` (varchar 100) — el motion del cliente (p.ej. "blacksmith").
    /// No participa en el wire del spawn (el cliente lo resuelve del pack).
    pub folder: String,
    // ---- F5.2 (combate): las columnas que el combate necesita (verificadas
    // contra el schema PG real — el G-PG las migró con OTROS nombres que el
    // C++: la `bCon` del mob (`tables.h:448`) es la columna `ht`; la `wDef`
    // (`tables.h:463`) es la columna `def`). ----
    /// `ht` (integer) — la `bCon` del mob en el C++ (`tables.h:448`): la
    /// "con" del mob para la DEF (el combate la usa como `NpcState.ht`).
    pub ht: i32,
    /// `def` (integer) — la `wDef` del mob (`tables.h:463`).
    pub def: i32,
    /// `max_hp` (bigint) — el HP máximo (el mundo lo gestiona en runtime).
    pub max_hp: i64,
    /// `attack_range` (integer) — UNITS (p.ej. mob 101 = 175).
    pub attack_range: i32,
    // ---- F5.3 (recompensas del kill): las columnas del reward. Tipos PG
    // (legacy-schema.md §4.6): `exp` int(10) unsigned -> bigint; `gold_min`/
    // `gold_max` int(11) -> integer. ----
    /// `exp` (bigint) — la exp que da el mob al morir.
    pub exp: i64,
    /// `gold_min` (integer) — el gold mínimo que da el mob.
    pub gold_min: i32,
    /// `gold_max` (integer) — el gold máximo (el C++ sortea `number(min,max)`).
    pub gold_max: i32,
}

/// Load del subset por vnum (`SELECT ... FROM player.mob_proto WHERE vnum = $1`).
/// El orden de columnas ES el contrato del mapeo (`mob_row_from_row`).
const LOAD_SQL: &str = "\
SELECT vnum, name, locale_name, type, battle_type, level, size, ai_flag, folder, \
ht, def, max_hp, attack_range, exp, gold_min, gold_max \
FROM player.mob_proto WHERE vnum = $1";

/// Load por LOTE de vnums (la misma SELECT, `WHERE vnum = ANY($1::int8[])` —
/// el cast explicito fija el tipo del array; tokio-postgres serializa
/// `&[i64]` como array nativo). Una sola query para los N vnums.
const LOAD_BATCH_SQL: &str = "\
SELECT vnum, name, locale_name, type, battle_type, level, size, ai_flag, folder, \
ht, def, max_hp, attack_range, exp, gold_min, gold_max \
FROM player.mob_proto WHERE vnum = ANY($1::int8[])";

/// Repositorio del dominio world (mob_proto). Conexion por llamada (ADR-0008).
pub struct MobRepo {
    pg_conn: String,
}

impl MobRepo {
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

    /// Load del subset de spawn por vnum. `None` = el vnum no existe en
    /// `mob_proto` (o es un vnum de grupo — ver el TRAP en `realm::npc`).
    pub async fn load_by_vnum(&self, vnum: i64) -> Result<Option<MobRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(LOAD_SQL, &[&vnum])
            .await
            .map_err(|e| pg_err("MOB_LOAD", &e))?;
        rows.first().map(mob_row_from_row).transpose()
    }

    /// Load por LOTE de vnums en UNA query (`WHERE vnum = ANY($1::int8[])`):
    /// la resolución de spawns del mapa (117 vnums distintos para el 41) en
    /// una sola llamada, en vez de una conexión PG por vnum. Devuelve
    /// `HashMap<vnum, MobRow>` — los vnums sin fila en `mob_proto` NO
    /// aparecen (el C++ tampoco los spawnea: `SpawnMob` -> nullptr).
    pub async fn load_by_vnums(&self, vnums: &[i64]) -> Result<std::collections::HashMap<i64, MobRow>, String> {
        if vnums.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let client = self.connect().await?;
        let rows = client
            .query(LOAD_BATCH_SQL, &[&vnums])
            .await
            .map_err(|e| pg_err("MOB_LOAD_BATCH", &e))?;
        rows.iter()
            .map(|r| mob_row_from_row(r).map(|m| (m.vnum, m)))
            .collect()
    }
}

/// Mapeo de la fila (orden exacto de `LOAD_SQL`). `locale_name` bytea ->
/// `Vec<u8>` crudo (el driver ya resuelve el patrón `\x`).
fn mob_row_from_row(r: &Row) -> Result<MobRow, String> {
    Ok(MobRow {
        vnum: r.try_get(0).map_err(|e| format!("mob_proto.vnum: {e}"))?,
        name: r.try_get(1).map_err(|e| format!("mob_proto.name: {e}"))?,
        locale_name: r.try_get(2).map_err(|e| format!("mob_proto.locale_name: {e}"))?,
        b_type: r.try_get(3).map_err(|e| format!("mob_proto.type: {e}"))?,
        battle_type: r.try_get(4).map_err(|e| format!("mob_proto.battle_type: {e}"))?,
        level: r.try_get(5).map_err(|e| format!("mob_proto.level: {e}"))?,
        size: r.try_get(6).map_err(|e| format!("mob_proto.size: {e}"))?,
        ai_flag: r.try_get(7).map_err(|e| format!("mob_proto.ai_flag: {e}"))?,
        folder: r.try_get(8).map_err(|e| format!("mob_proto.folder: {e}"))?,
        ht: r.try_get(9).map_err(|e| format!("mob_proto.ht: {e}"))?,
        def: r.try_get(10).map_err(|e| format!("mob_proto.def: {e}"))?,
        max_hp: r.try_get(11).map_err(|e| format!("mob_proto.max_hp: {e}"))?,
        attack_range: r.try_get(12).map_err(|e| format!("mob_proto.attack_range: {e}"))?,
        exp: r.try_get(13).map_err(|e| format!("mob_proto.exp: {e}"))?,
        gold_min: r.try_get(14).map_err(|e| format!("mob_proto.gold_min: {e}"))?,
        gold_max: r.try_get(15).map_err(|e| format!("mob_proto.gold_max: {e}"))?,
    })
}

/// `bType` (smallint de PG) -> BYTE del wire, con guarda de rango (el C++
/// lo trunca silenciosamente a BYTE; aqui falla el conversion en vez de
/// corromper el paquete). Los valores reales del runtime caben (0..9).
pub fn wire_b_type(b_type: i16) -> Result<u8, String> {
    u8::try_from(b_type).map_err(|_| format!("mob_proto.type {b_type} fuera de BYTE (0..255)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contrato del SQL: el orden de columnas del mapeo (si alguien lo toca,
    /// `mob_row_from_row` y el orden del wire se desalinean — el test lo fija).
    /// F5.2: el combate añadió las columnas `ht, def, max_hp, attack_range`.
    /// F5.3: las recompensas añadieron `exp, gold_min, gold_max`.
    /// La query batch comparte el MISMO orden (mismo mapeo).
    #[test]
    fn load_sql_column_order() {
        assert_eq!(
            LOAD_SQL,
            "SELECT vnum, name, locale_name, type, battle_type, level, size, ai_flag, folder, \
ht, def, max_hp, attack_range, exp, gold_min, gold_max \
FROM player.mob_proto WHERE vnum = $1"
        );
        assert_eq!(
            LOAD_BATCH_SQL,
            "SELECT vnum, name, locale_name, type, battle_type, level, size, ai_flag, folder, \
ht, def, max_hp, attack_range, exp, gold_min, gold_max \
FROM player.mob_proto WHERE vnum = ANY($1::int8[])"
        );
    }

    /// wire_b_type: BYTE 0..=9 (los valores del runtime) pasan; fuera de
    /// rango -> Err (defensivo, el C++ truncaria).
    #[test]
    fn wire_b_type_range() {
        for t in [0i16, 1, 2, 3, 4, 5, 6, 9] {
            assert_eq!(wire_b_type(t).unwrap(), t as u8, "type {t}");
        }
        assert!(wire_b_type(255).is_ok(), "255 = ultimo BYTE valido");
        assert!(wire_b_type(256).is_err(), "> 255 -> Err (el C++ truncaria)");
        assert!(wire_b_type(-1).is_err(), "negativo -> Err");
    }
}
