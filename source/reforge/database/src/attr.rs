//! Atributos aleatorios + sockets de items al CREAR (parity
//! `item_attribute.cpp` / `item_manager.cpp` — lane 2026-08-16).
//!
//! # Parity C++ (source/server — congelado)
//!
//! - Tablas: `player.item_attr` (54 filas) e `player.item_attr_rare` (20
//!   filas) — el load de `ClientManagerBoot.cpp:578-600 / 700-720`
//!   (`InitializeItemAttrTable`/`InitializeItemRareTable`): columnas
//!   `apply, apply+0, prob, lv1..lv5, weapon..ear`. El `apply+0` del MySQL
//!   (índice 1-based del ENUM) es el `dwApplyIndex` = valor `EApplyTypes`
//!   (length.h:350+); en PG `apply` es TEXT → índice por posición en
//!   `APPLY_NAMES` (el MISMO orden del catálogo `ENUM_COLUMNS` del
//!   mysql_proxy, translate.rs:597 — fuente: SHOW CREATE MariaDB).
//! - `GetAttributeSetIndex` (item_attribute.cpp:15-57): weapon → set 0
//!   (salvo ARROW), armor → body 1 / wrist 2 / foots 3 / neck 4 / head 5 /
//!   shield 6 / ear 7; el resto → sin set (None).
//! - `AlterToMagicItem` (item.cpp:1300-1348) + `PutAttribute`/
//!   `PutAttributeWithLevel` (item_attribute.cpp:163-232): el primer attr
//!   con la tabla `aiItemMagicAttributePercentHigh` {30,40,20,8,2} y el
//!   ​​segundo/tercero con `Low` {50,40,10,0,0} con prob 20%/5% (arma) o
//!   10%/1-2% (armadura); cada attr se elige ponderado por `prob` entre los
//!   disponibles del set (bMaxLevelBySet[set] > 0, sin repetir), nivel
//!   clamp a `bMaxLevelBySet[set]`, valor = `lValues[min(4, nivel-1)]`.
//! - `AddRareAttribute` (item_attribute.cpp:447-480): un attr ALEATORIO de
//!   la tabla RARE entre los disponibles del set (sin repetir en los slots
//!   raros 5-6), nivel = MIN(5, bMaxLevelBySet[set]), valor =
//!   `lValues[nivel-1]`, slot `5 + rare_count`.
//! - Sockets: `CreateItem` id==0 (item_manager.cpp:308-312) →
//!   `AlterToSocketItem(bGainSocketPct)` (item.cpp:1288-1298): los
//!   primeros `socket_pct` sockets a 1 (engarce abierto). Aplica a TODO
//!   item nuevo (con o sin bTryMagic).
//! - `CreateItem` con bTryMagic (item_manager.cpp:301-306): si
//!   `number(1,100) <= magic_pct` → `AlterToMagicItem()`. Drops de mob
//!   (:933) y GM `item` (cmd_gm.cpp:437) usan bTryMagic=true; quests
//!   (questlua_game.cpp:110) false → solo sockets.
//! - NOTA (gap controlado, instrucción del lane): el C++ de esta variante
//!   solo añade RARE vía GM/quest/stone; el lane pide rareza en los drops —
//!   cuando el roll mágico del drop acierta se añade además 1 attr RARE
//!   (`add_rare_attribute`). Documentado como extensión del subset.

/// Las 8 posiciones de `bMaxLevelBySet` (EAttributeSet — length.h:659-682;
/// sin costume/pendant/glove: los datos de item_attr van a 0 ahí).
pub const SET_WEAPON: usize = 0;
pub const SET_BODY: usize = 1;
pub const SET_WRIST: usize = 2;
pub const SET_FOOTS: usize = 3;
pub const SET_NECK: usize = 4;
pub const SET_HEAD: usize = 5;
pub const SET_SHIELD: usize = 6;
pub const SET_EAR: usize = 7;
pub const SET_MAX_NUM: usize = 8;

/// `ITEM_ATTRIBUTE_NORM_NUM` / `ITEM_ATTRIBUTE_RARE_NUM` / `_START`
/// (item_length.h:19-25): 5 normales (pos 0-4) + 2 raras (pos 5-6).
pub const NORM_ATTR_NUM: usize = 5;
pub const RARE_ATTR_NUM: usize = 2;
pub const RARE_START: usize = NORM_ATTR_NUM;
/// `ITEM_ATTRIBUTE_MAX_LEVEL` = 5 (item_length.h:26).
pub const MAX_ATTR_LEVEL: usize = 5;

/// `aiItemMagicAttributePercentHigh/Low` (constants.cpp:751-758) — la
/// distribución de NIVEL del attr mágico (percentiles acumulados 1..100).
pub const MAGIC_PERCENT_HIGH: [u32; MAX_ATTR_LEVEL] = [30, 40, 20, 8, 2];
pub const MAGIC_PERCENT_LOW: [u32; MAX_ATTR_LEVEL] = [50, 40, 10, 0, 0];

/// Fila de `item_attr`/`item_attr_rare` (parity `TItemAttrTable`,
/// tables.h:624-642 — `dwApplyIndex`, `dwProb`, `lValues[5]`,
/// `bMaxLevelBySet[8]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrRow {
    /// `dwApplyIndex` = índice EApplyTypes (1-based del ENUM MySQL).
    pub apply_index: i16,
    /// `dwProb` — peso del attr en `PutAttributeWithLevel`.
    pub prob: i32,
    /// `lValues[ITEM_ATTRIBUTE_MAX_LEVEL]` — valor por nivel.
    pub values: [i32; 5],
    /// `bMaxLevelBySet[ATTRIBUTE_SET_MAX_NUM]` — nivel máx por set (0 = no
    /// disponible para ese set).
    pub max_level_by_set: [i16; SET_MAX_NUM],
}

/// Las dos tablas cargadas (54 + 20 filas) — caché compartida del canal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttrTables {
    /// `item_attr` — attrs NORMALES (pos 0-4).
    pub normal: Vec<AttrRow>,
    /// `item_attr_rare` — attrs RAROS (pos 5-6).
    pub rare: Vec<AttrRow>,
}

/// Los literales del ENUM `apply` en ORDEN (el orden ES el índice — fuente:
/// catálogo `ENUM_COLUMNS` del mysql_proxy, translate.rs:597 — SHOW CREATE
/// MariaDB 2026-08-11; el C++ los recibe con `apply+0`).
pub const APPLY_NAMES: [&str; 118] = [
    "MAX_HP", "MAX_SP", "CON", "INT", "STR", "DEX", "ATT_SPEED", "MOV_SPEED",
    "CAST_SPEED", "HP_REGEN", "SP_REGEN", "POISON_PCT", "STUN_PCT", "SLOW_PCT",
    "CRITICAL_PCT", "PENETRATE_PCT", "ATTBONUS_HUMAN", "ATTBONUS_ANIMAL",
    "ATTBONUS_ORC", "ATTBONUS_MILGYO", "ATTBONUS_UNDEAD", "ATTBONUS_DEVIL",
    "STEAL_HP", "STEAL_SP", "MANA_BURN_PCT", "DAMAGE_SP_RECOVER", "BLOCK",
    "DODGE", "RESIST_SWORD", "RESIST_TWOHAND", "RESIST_DAGGER", "RESIST_BELL",
    "RESIST_FAN", "RESIST_BOW", "RESIST_FIRE", "RESIST_ELEC", "RESIST_MAGIC",
    "RESIST_WIND", "REFLECT_MELEE", "REFLECT_CURSE", "POISON_REDUCE",
    "KILL_SP_RECOVER", "EXP_DOUBLE_BONUS", "GOLD_DOUBLE_BONUS",
    "ITEM_DROP_BONUS", "POTION_BONUS", "KILL_HP_RECOVER", "IMMUNE_STUN",
    "IMMUNE_SLOW", "IMMUNE_FALL", "SKILL", "BOW_DISTANCE", "ATT_GRADE_BONUS",
    "DEF_GRADE_BONUS", "MAGIC_ATT_GRADE_BONUS", "MAGIC_DEF_GRADE_BONUS",
    "CURSE_PCT", "MAX_STAMINA", "ATT_BONUS_TO_WARRIOR", "ATT_BONUS_TO_ASSASSIN",
    "ATT_BONUS_TO_SURA", "ATT_BONUS_TO_SHAMAN", "ATT_BONUS_TO_MONSTER",
    "ATT_BONUS", "MALL_DEFBONUS", "MALL_EXPBONUS", "MALL_ITEMBONUS",
    "MALL_GOLDBONUS", "MAX_HP_PCT", "MAX_SP_PCT", "SKILL_DAMAGE_BONUS",
    "NORMAL_HIT_DAMAGE_BONUS", "SKILL_DEFEND_BONUS", "NORMAL_HIT_DEFEND_BONUS",
    "PC_BANG_EXP_BONUS", "PC_BANG_DROP_BONUS", "EXTRACT_HP_PCT",
    "RESIST_WARRIOR", "RESIST_ASSASSIN", "RESIST_SURA", "RESIST_SHAMAN",
    "ENERGY", "DEF_GRADE", "COSTUME_ATTR_BONUS", "MAGIC_ATT_BONUS_PER",
    "MELEE_MAGIC_ATT_BONUS_PER", "RESIST_ICE", "RESIST_EARTH", "RESIST_DARK",
    "RESIST_CRITICAL", "RESIST_PENETRATE", "BLEEDING_REDUCE", "BLEEDING_PCT",
    "ATT_BONUS_TO_WOLFMAN", "RESIST_WOLFMAN", "RESIST_CLAW", "ACCEDRAIN_RATE",
    "RESIST_MAGIC_REDUCTION", "ENCHANT_ELECT", "ENCHANT_FIRE", "ENCHANT_ICE",
    "ENCHANT_WIND", "ENCHANT_EARTH", "ENCHANT_DARK", "ATTBONUS_CZ",
    "ATTBONUS_INSECT", "ATTBONUS_DESERT", "ATTBONUS_SWORD", "ATTBONUS_TWOHAND",
    "ATTBONUS_DAGGER", "ATTBONUS_BELL", "ATTBONUS_FAN", "ATTBONUS_BOW",
    "ATTBONUS_CLAW", "RESIST_HUMAN", "RESIST_MOUNT_FALL", "UNK_117", "MOUNT",
];

/// Índice EApplyTypes del literal (parity `apply+0` de MySQL — 1-based;
/// no-enum → 0 como MySQL). `None` si el literal no está en el catálogo.
pub fn apply_index_by_name(name: &str) -> Option<i16> {
    APPLY_NAMES
        .iter()
        .position(|n| *n == name)
        .map(|i| i as i16 + 1)
}

/// `GetAttributeSetIndex` (item_attribute.cpp:15-57): el set del item según
/// type/subtype (ITEM_WEAPON=1, ITEM_ARMOR=2; ARROW=6). `None` = el item no
/// puede llevar attrs (flechas, usables, etc.).
pub fn attribute_set_index(b_type: i16, b_sub_type: i16) -> Option<usize> {
    match b_type {
        1 /* ITEM_WEAPON */ => {
            if b_sub_type == 6 /* WEAPON_ARROW */ {
                None
            } else {
                Some(SET_WEAPON)
            }
        }
        2 /* ITEM_ARMOR */ => Some(match b_sub_type {
            0 /* ARMOR_BODY */ => SET_BODY,
            3 /* ARMOR_WRIST */ => SET_WRIST,
            4 /* ARMOR_FOOTS */ => SET_FOOTS,
            5 /* ARMOR_NECK */ => SET_NECK,
            1 /* ARMOR_HEAD */ => SET_HEAD,
            2 /* ARMOR_SHIELD */ => SET_SHIELD,
            6 /* ARMOR_EAR */ => SET_EAR,
            _ => return None,
        }),
        _ => None,
    }
}

/// `number(min, max)` del C++ (rand.c:44-48 — rango INCLUSIVO).
fn number(rng: &mut dyn FnMut() -> u32, min: u32, max: u32) -> u32 {
    min + rng() % (max - min + 1).max(1)
}

/// ¿El apply `idx` ya está en los slots NORMALES (0-4)? (`HasAttr`,
/// item_attribute.cpp:129-139).
fn has_attr(attrs: &[(i16, i16); 7], apply_index: i16) -> bool {
    attrs[..NORM_ATTR_NUM].iter().any(|(t, _)| *t == apply_index)
}

/// ¿El apply `idx` ya está en los slots RAROS (5-6)? (`HasRareAttr`).
fn has_rare_attr(attrs: &[(i16, i16); 7], apply_index: i16) -> bool {
    attrs[RARE_START..].iter().any(|(t, _)| *t == apply_index)
}

/// `PutAttributeWithLevel` (item_attribute.cpp:163-215): elige un attr del
/// set ponderado por `prob` (sin repetir los ya puestos) y lo escribe con el
/// nivel clamp a `bMaxLevelBySet[set]` → valor `lValues[min(4, nivel-1)]`.
/// Devuelve true si escribió. `table` = tabla normal o rare (el C++ usa la
/// normal para `PutAttribute` y la rare para `AddRareAttribute`).
fn put_attribute_with_level(
    rng: &mut dyn FnMut() -> u32,
    table: &[AttrRow],
    set: usize,
    attrs: &mut [(i16, i16); 7],
    level: u32,
    rare_slot: bool,
) -> bool {
    let mut avail: Vec<usize> = Vec::new();
    let mut total: u32 = 0;
    for (i, r) in table.iter().enumerate() {
        if r.max_level_by_set[set] > 0 && !(if rare_slot { has_rare_attr(attrs, r.apply_index) } else { has_attr(attrs, r.apply_index) }) {
            avail.push(i);
            total = total.saturating_add(r.prob.max(0) as u32);
        }
    }
    if total == 0 || avail.is_empty() {
        return false;
    }
    let mut prob = number(rng, 1, total);
    let mut idx = avail[0];
    for &i in &avail {
        let r = &table[i];
        if prob <= r.prob.max(0) as u32 {
            idx = i;
            break;
        }
        prob -= r.prob.max(0) as u32;
    }
    let r = &table[idx];
    let level = level.min(r.max_level_by_set[set].max(0) as u32);
    let value = r.values[(level as usize - 1).min(MAX_ATTR_LEVEL - 1)];
    if value == 0 {
        return false; // parity AddAttr: `lVal` 0 → no escribe
    }
    let pos = if rare_slot { RARE_START + count_rare(attrs) } else { count_norm(attrs) };
    if pos >= 7 {
        return false;
    }
    attrs[pos] = (r.apply_index, value as i16);
    true
}

fn count_norm(attrs: &[(i16, i16); 7]) -> usize {
    attrs[..NORM_ATTR_NUM].iter().take_while(|(t, _)| *t != 0).count()
}

fn count_rare(attrs: &[(i16, i16); 7]) -> usize {
    attrs[RARE_START..].iter().take_while(|(t, _)| *t != 0).count()
}

/// `PutAttribute(percentTable)` (item_attribute.cpp:217-232): el nivel del
/// attr por la tabla de percentiles (roll 1..100) + `PutAttributeWithLevel`.
fn put_attribute(
    rng: &mut dyn FnMut() -> u32,
    table: &[AttrRow],
    set: usize,
    attrs: &mut [(i16, i16); 7],
    percent: &[u32; MAX_ATTR_LEVEL],
) -> bool {
    let mut roll = number(rng, 1, 100);
    let mut level = MAX_ATTR_LEVEL as u32;
    for (i, p) in percent.iter().enumerate() {
        if roll <= *p {
            level = i as u32 + 1;
            break;
        }
        roll -= *p;
    }
    put_attribute_with_level(rng, table, set, attrs, level, false)
}

/// `AlterToMagicItem` (item.cpp:1300-1348): 1 attr con la tabla HIGH + un
/// segundo/tercero con LOW según las probabilidades del tipo (arma 20%/5%,
/// armadura body 10%/2%, resto 10%/1%). Sin set (flechas/usables) → no-op.
pub fn alter_to_magic_item(
    rng: &mut dyn FnMut() -> u32,
    normal: &[AttrRow],
    b_type: i16,
    b_sub_type: i16,
    attrs: &mut [(i16, i16); 7],
) {
    let Some(set) = attribute_set_index(b_type, b_sub_type) else {
        return;
    };
    let (second, third) = match b_type {
        1 /* ITEM_WEAPON */ => (20u32, 5u32),
        2 /* ITEM_ARMOR */ if b_sub_type == 0 /* ARMOR_BODY */ => (10, 2),
        2 /* ITEM_ARMOR */ => (10, 1),
        _ => return,
    };
    put_attribute(rng, normal, set, attrs, &MAGIC_PERCENT_HIGH);
    if number(rng, 1, 100) <= second {
        put_attribute(rng, normal, set, attrs, &MAGIC_PERCENT_LOW);
    }
    if number(rng, 1, 100) <= third {
        put_attribute(rng, normal, set, attrs, &MAGIC_PERCENT_LOW);
    }
}

/// `AddRareAttribute` (item_attribute.cpp:447-480): un attr ALEATORIO de la
/// tabla RARE entre los disponibles del set (sin repetir en slots raros),
/// nivel = MIN(5, maxBySet), valor = `lValues[nivel-1]`, slot `5 + count`.
/// Devuelve true si escribió (false = sin set, slots llenos o sin attrs
/// disponibles).
pub fn add_rare_attribute(
    rng: &mut dyn FnMut() -> u32,
    rare: &[AttrRow],
    b_type: i16,
    b_sub_type: i16,
    attrs: &mut [(i16, i16); 7],
) -> bool {
    if count_rare(attrs) >= RARE_ATTR_NUM {
        return false;
    }
    let Some(set) = attribute_set_index(b_type, b_sub_type) else {
        return false;
    };
    let avail: Vec<usize> = rare
        .iter()
        .enumerate()
        .filter(|(_, r)| r.apply_index != 0 && r.max_level_by_set[set] > 0 && !has_rare_attr(attrs, r.apply_index))
        .map(|(i, _)| i)
        .collect();
    if avail.is_empty() {
        return false;
    }
    let r = &rare[avail[number(rng, 0, avail.len() as u32 - 1) as usize]];
    let level = (MAX_ATTR_LEVEL as i16).min(r.max_level_by_set[set]);
    let value = r.values[(level as usize - 1).min(MAX_ATTR_LEVEL - 1)];
    if value == 0 {
        return false;
    }
    let pos = RARE_START + count_rare(attrs);
    if pos >= 7 {
        return false;
    }
    attrs[pos] = (r.apply_index, value as i16);
    true
}

/// Bonus de CREACIÓN del item (parity `CreateItem` id==0,
/// item_manager.cpp:292-313):
/// 1. sockets: los primeros `socket_pct` sockets → 1 (`AlterToSocketItem`).
/// 2. mágico: si `roll(1,100) <= magic_pct` → `AlterToMagicItem` + (per
///    instrucción del lane) 1 attr RARE (`AddRareAttribute`).
///
/// `magic_pct == 0` → sin attrs mágicos (parity quests — bTryMagic=false;
/// los sockets SÍ aplican). Los attrs/sockets previos se conservan (los
/// items ya mágicos no se re-rollean). La ÚNICA función pública del slice
/// (ponytail: 1 función, no las 3 clases del C++).
pub fn roll_attrs(
    rng: &mut dyn FnMut() -> u32,
    magic_pct: i16,
    socket_pct: i16,
    tables: &AttrTables,
    b_type: i16,
    b_sub_type: i16,
    sockets: &mut [i64; 3],
    attrs: &mut [(i16, i16); 7],
) {
    // Sockets: `AlterToSocketItem(bGainSocketPct)` — item.cpp:1288-1298.
    for (i, s) in sockets.iter_mut().enumerate() {
        if (i as i16) < socket_pct && *s == 0 {
            *s = 1;
        }
    }
    if magic_pct <= 0 || number(rng, 1, 100) > magic_pct as u32 {
        return;
    }
    if count_norm(attrs) > 0 {
        return; // ya mágico (re-roll no aplica — parity CreateItem)
    }
    alter_to_magic_item(rng, &tables.normal, b_type, b_sub_type, attrs);
    // Rareza en drops (extensión documentada del lane — ver header).
    add_rare_attribute(rng, &tables.rare, b_type, b_sub_type, attrs);
}

/// SQL del load de `item_attr`/`item_attr_rare` (parity
/// `ClientManagerBoot.cpp:594-600 / 719-725` — `SELECT apply, apply+0, prob,
/// lv1..lv5, weapon..ear FROM item_attr ORDER BY apply`). En PG el índice
/// del ENUM se deriva del literal en Rust (`apply_index_by_name` — el
/// `apply+0` del C++ llega vía mysql_proxy).
const ATTR_TABLE_SQL: &str = "\
SELECT apply, prob, lv1, lv2, lv3, lv4, lv5, \
weapon, body, wrist, foots, neck, head, shield, ear \
FROM player.{table} ORDER BY apply";

/// Fila de attr desde el row PG (columnas 1-14 del SQL de arriba; la 0 es el
/// literal `apply`, que se mapea al índice EApplyTypes en Rust).
/// OJO tipos PG (verificado 2026-08-16 contra el esquema real): TODAS las
/// columnas numéricas son BIGINT (el int(11) UNSIGNED de MariaDB → bigint,
/// regla del G-PG) — leerlas como i32/i16 da "error deserializing column"
/// (mismo trap documentado del `weight` en item.rs). Se leen i64 y se
/// castean a los tipos del TItemAttrTable (long/BYTE en el C++).
fn attr_row_from_row(r: &tokio_postgres::Row) -> Result<AttrRow, String> {
    let g = |i: usize| -> Result<i64, String> { r.try_get(i).map_err(|e| format!("attr col{i}: {e}")) };
    let mut values = [0i32; MAX_ATTR_LEVEL];
    for (i, v) in values.iter_mut().enumerate() {
        *v = g(2 + i)? as i32;
    }
    let mut max_level_by_set = [0i16; SET_MAX_NUM];
    for (i, m) in max_level_by_set.iter_mut().enumerate() {
        *m = g(7 + i)? as i16;
    }
    let name: String = r.try_get(0).map_err(|e| format!("attr apply: {e}"))?;
    Ok(AttrRow {
        apply_index: apply_index_by_name(&name).unwrap_or(0),
        prob: g(1)? as i32,
        values,
        max_level_by_set,
    })
}

impl crate::item::ItemRepo {
    /// Carga las dos tablas de attrs (`item_attr` + `item_attr_rare` — 54 +
    /// 20 filas; parity `InitializeItemAttrTable`/`InitializeItemRareTable`,
    /// ClientManagerBoot.cpp:578-600/700-720). El canal las cachea en el
    /// arranque (`AttrTables`) y las comparte entre conexiones.
    pub async fn load_attr_tables(&self) -> Result<AttrTables, String> {
        let client = self.connect().await?;
        let mut tables = AttrTables::default();
        for (table, out) in [("item_attr", &mut tables.normal), ("item_attr_rare", &mut tables.rare)] {
            let sql = ATTR_TABLE_SQL.replace("{table}", table);
            let rows = client
                .query(&sql, &[])
                .await
                .map_err(|e| crate::account::pg_err(&format!("ATTR_{table}"), &e))?;
            let mut v = rows.iter().map(attr_row_from_row).collect::<Result<Vec<_>, _>>()?;
            // El C++ indexa por dwApplyIndex (`g_map_itemAttr[dwApplyIndex]`,
            // input_db.cpp:652) → el orden de iteración de avail es por apply
            // ASCENDENTE; el ORDER BY apply del SQL es alfabético (text en
            // PG) → reordenar para el mismo tie-breaking del ponderado.
            v.sort_by_key(|r| r.apply_index);
            *out = v;
        }
        Ok(tables)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply literal → índice EApplyTypes (parity `apply+0` del ENUM MySQL):
    /// 'MAX_HP' = APPLY_MAX_HP = 1 (length.h:352), 'MAX_SP' = 2, ... y el
    /// orden coincide con la cabecera del C++.
    #[test]
    fn apply_index_matches_eapplytypes_order() {
        assert_eq!(apply_index_by_name("MAX_HP"), Some(1), "APPLY_MAX_HP");
        assert_eq!(apply_index_by_name("MAX_SP"), Some(2), "APPLY_MAX_SP");
        assert_eq!(apply_index_by_name("CON"), Some(3), "APPLY_CON");
        assert_eq!(apply_index_by_name("DEX"), Some(6), "APPLY_DEX");
        assert_eq!(apply_index_by_name("MOV_SPEED"), Some(8), "APPLY_MOV_SPEED");
        assert_eq!(apply_index_by_name("MOUNT"), Some(118), "último literal");
        assert_eq!(apply_index_by_name("NO_EXISTE"), None, "no-enum → 0");
        assert_eq!(APPLY_NAMES.len(), 118, "catálogo del proxy (translate.rs:597)");
    }

    /// `GetAttributeSetIndex` parity (item_attribute.cpp:15-57): arma → 0
    /// (la flecha no), armadura por subtipo, el resto → None.
    #[test]
    fn attribute_set_index_parity() {
        assert_eq!(attribute_set_index(1, 0), Some(SET_WEAPON), "espada");
        assert_eq!(attribute_set_index(1, 6), None, "flecha — sin set");
        assert_eq!(attribute_set_index(2, 0), Some(SET_BODY));
        assert_eq!(attribute_set_index(2, 3), Some(SET_WRIST));
        assert_eq!(attribute_set_index(2, 4), Some(SET_FOOTS));
        assert_eq!(attribute_set_index(2, 5), Some(SET_NECK));
        assert_eq!(attribute_set_index(2, 1), Some(SET_HEAD));
        assert_eq!(attribute_set_index(2, 2), Some(SET_SHIELD));
        assert_eq!(attribute_set_index(2, 6), Some(SET_EAR));
        assert_eq!(attribute_set_index(2, 7), None, "pendant — sin datos");
        assert_eq!(attribute_set_index(3, 0), None, "usable");
    }

    /// RNG determinista de los tests: secuencia fija.
    struct SeqRng {
        vals: Vec<u32>,
        i: usize,
    }
    impl SeqRng {
        fn new(vals: Vec<u32>) -> Self {
            Self { vals, i: 0 }
        }
    }
    fn seq(vals: Vec<u32>) -> Box<dyn FnMut() -> u32> {
        let mut r = SeqRng::new(vals);
        Box::new(move || {
            let v = r.vals[r.i % r.vals.len()];
            r.i += 1;
            v
        })
    }

    /// `number(min, max)` es INCLUSIVO (parity rand.c:44-48): rng 0 → min;
    /// rng u32::MAX → min + (u32::MAX % span) — el módulo del C++ tampoco
    /// alcanza el max (rand() % span), siempre en rango.
    #[test]
    fn number_is_inclusive() {
        let mut rng = seq(vec![0]);
        assert_eq!(number(&mut rng, 1, 100), 1, "mínimo inclusive");
        let mut rng = seq(vec![u32::MAX]);
        assert_eq!(number(&mut rng, 1, 100), 96, "1 + (u32::MAX % 100)");
        let mut rng = seq(vec![u32::MAX]);
        let v = number(&mut rng, 1, 100);
        assert!((1..=100).contains(&v), "siempre en rango: {v}");
    }

    fn row(apply: i16, prob: i32, values: [i32; 5], sets: [i16; 8]) -> AttrRow {
        AttrRow { apply_index: apply, prob, values, max_level_by_set: sets }
    }

    /// `alter_to_magic_item` escribe 1..3 attrs en los slots NORMALES (0-4)
    /// con el valor del nivel (`lValues[min(4, nivel-1)]` — con los rolls en
    /// el mínimo el nivel es 1 → values[0]).
    #[test]
    fn alter_to_magic_item_writes_normal_attrs() {
        let normal = vec![
            row(1, 10, [10, 20, 30, 40, 50], [5, 0, 0, 0, 0, 0, 0, 0]), // MAX_HP (weapon)
            row(6, 10, [1, 2, 3, 4, 5], [5, 0, 0, 0, 0, 0, 0, 0]),      // DEX
        ];
        let mut attrs = [(0i16, 0i16); 7];
        // Rolls: HIGH nivel → 1 (roll 2 ≤ 30), ponderado → MAX_HP (prob 2 ≤
        // 10); segundo 20% → SÍ (roll 2 ≤ 20); LOW nivel → 1 (roll 2 ≤ 50),
        // ponderado → DEX (MAX_HP ya usado); tercero 5% → NO (roll 100).
        let mut rng = seq(vec![1, 1, 1, 1, 1, 99]);
        alter_to_magic_item(&mut rng, &normal, 1, 0, &mut attrs);
        assert_eq!(attrs[0], (1, 10), "MAX_HP lv1 = values[0]");
        assert_eq!(attrs[1], (6, 1), "DEX lv1 (Low)");
        assert_eq!(attrs[2], (0, 0), "tercero no (5% fallado)");
        assert_eq!(attrs[5], (0, 0), "slots raros intactos");
    }

    /// Flecha/usables → `alter_to_magic_item` es no-op (sin set).
    #[test]
    fn alter_to_magic_item_noop_without_set() {
        let normal = vec![row(1, 10, [10, 0, 0, 0, 0], [5, 0, 0, 0, 0, 0, 0, 0])];
        let mut attrs = [(0i16, 0i16); 7];
        let mut rng = seq(vec![1, 1, 1]);
        alter_to_magic_item(&mut rng, &normal, 1, 6, &mut attrs);
        assert_eq!(attrs, [(0, 0); 7], "flecha — sin attrs");
        let mut rng = seq(vec![1, 1, 1]);
        alter_to_magic_item(&mut rng, &normal, 3, 0, &mut attrs);
        assert_eq!(attrs, [(0, 0); 7], "usable — sin attrs");
    }

    /// Sin attrs disponibles en el set (max_level_by_set 0) → no escribe.
    #[test]
    fn put_attribute_no_available_attr_for_set() {
        let normal = vec![row(1, 10, [10, 0, 0, 0, 0], [0, 5, 0, 0, 0, 0, 0, 0])];
        let mut attrs = [(0i16, 0i16); 7];
        let mut rng = seq(vec![1]);
        assert!(!put_attribute_with_level(&mut rng, &normal, SET_WEAPON, &mut attrs, 1, false));
        assert_eq!(attrs, [(0, 0); 7]);
    }

    /// `add_rare_attribute` parity: slot 5 + count, nivel MIN(5, maxBySet),
    /// valor `lValues[nivel-1]`; sin repetir; slots llenos → false.
    #[test]
    fn add_rare_attribute_parity() {
        let rare = vec![
            row(53, 0, [1, 2, 3, 4, 5], [3, 0, 0, 0, 0, 0, 0, 0]), // ATT_GRADE_BONUS — max 3
            row(54, 0, [10, 20, 30, 40, 50], [5, 0, 0, 0, 0, 0, 0, 0]), // DEF_GRADE_BONUS — max 5
        ];
        let mut attrs = [(0i16, 0i16); 7];
        // number(0, 1) con rng 0 → índice 0 → ATT_GRADE_BONUS lv3 = values[2].
        let mut rng = seq(vec![0, 0]);
        assert!(add_rare_attribute(&mut rng, &rare, 1, 0, &mut attrs));
        assert_eq!(attrs[5], (53, 3), "slot raro 0: ATT_GRADE_BONUS lv3");
        // Segundo rare: rng 0 → ATT_GRADE_BONUS YA está → no repetible → el
        // avail restante es DEF_GRADE_BONUS (índice 1); number(0,0) = 0 → 1.
        let mut rng = seq(vec![0]);
        assert!(add_rare_attribute(&mut rng, &rare, 1, 0, &mut attrs));
        assert_eq!(attrs[6], (54, 50), "slot raro 1: DEF_GRADE_BONUS lv5");
        // Slots raros llenos → false.
        let mut rng = seq(vec![0]);
        assert!(!add_rare_attribute(&mut rng, &rare, 1, 0, &mut attrs));
        // Sin set → false.
        let mut attrs = [(0i16, 0i16); 7];
        let mut rng = seq(vec![0]);
        assert!(!add_rare_attribute(&mut rng, &rare, 3, 0, &mut attrs));
    }

    /// `roll_attrs` parity CreateItem id==0: sockets = socket_pct
    /// abiertos SIEMPRE (con o sin magic_pct); el mágico solo si el roll
    /// acierta magic_pct.
    #[test]
    fn roll_attrs_sockets_and_magic() {
        let tables = AttrTables {
            normal: vec![row(1, 10, [10, 20, 30, 40, 50], [5, 0, 0, 0, 0, 0, 0, 0])],
            rare: vec![row(53, 0, [1, 2, 3, 4, 5], [3, 0, 0, 0, 0, 0, 0, 0])],
        };
        // socket_pct=1, magic_pct=0 → socket abierto, sin attrs (parity quest).
        let mut sockets = [0i64; 3];
        let mut attrs = [(0i16, 0i16); 7];
        let mut rng = seq(vec![0]);
        roll_attrs(&mut rng, 0, 1, &tables, 1, 0, &mut sockets, &mut attrs);
        assert_eq!(sockets, [1, 0, 0]);
        assert_eq!(attrs, [(0, 0); 7], "magic_pct 0 → sin attrs mágicos");
        // socket_pct=3 → los 3 abiertos (AlterToSocketItem sin clamp del C++).
        let mut sockets = [0i64; 3];
        roll_attrs(&mut rng, 0, 3, &tables, 1, 0, &mut sockets, &mut attrs);
        assert_eq!(sockets, [1, 1, 1]);
        // magic_pct=100, roll 1 → acierta: attr normal + attr RARE.
        let mut sockets = [0i64; 3];
        let mut attrs = [(0i16, 0i16); 7];
        let mut rng = seq(vec![1, 1, 1, 1, 99, 0]);
        roll_attrs(&mut rng, 100, 0, &tables, 1, 0, &mut sockets, &mut attrs);
        assert_eq!(attrs[0], (1, 10), "attr normal MAX_HP lv1");
        assert_eq!(attrs[5], (53, 3), "attr RARE en el slot 5");
        // magic_pct=0 pero roll alto → nada.
        let mut attrs = [(0i16, 0i16); 7];
        let mut rng = seq(vec![99]);
        roll_attrs(&mut rng, 50, 0, &tables, 1, 0, &mut sockets, &mut attrs);
        assert_eq!(attrs, [(0, 0); 7], "roll 99 > 50 → sin attrs");
        // Items YA mágicos no se re-rollean (parity CreateItem).
        let mut attrs = [(1i16, 10i16), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0)];
        let mut rng = seq(vec![1]);
        roll_attrs(&mut rng, 100, 0, &tables, 1, 0, &mut sockets, &mut attrs);
        assert_eq!(attrs[0], (1, 10), "intacto");
        assert_eq!(attrs[1], (0, 0), "sin segundo attr");
    }
}
