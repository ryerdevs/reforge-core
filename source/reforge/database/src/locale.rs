//! F1 (ADR-0008/0009) — repositorio del locale: `LocaleRepo` lee las 6
//! tablas del dominio `common` (mob_names, item_names, item_descriptions,
//! skill_names, map_names, ui_texts) con el **fallback EN** (ADR-0009: una
//! clave presente en EN pero NO en el idioma pedido → se incluye el valor
//! EN; ausente en ambos → se omite; idioma == EN → sin merge).
//!
//! Patrón de los repos existentes (`npc.rs`/`account.rs`): conexión por
//! llamada (ADR-0008), errores `Result<_, String>` con `pg_err`.
//!
//! El tipo de salida es el `LocaleBundle` del crate `protocol` (el contrato
//! wire del GC_LOCALE — los ids numéricos viajan como texto ASCII, spec F1).
//! `message_texts` e `item_icons` NO se leen (server-side / panel — ADR-0009).

use crate::pool::{Client, PgPool};
use protocol::locale::LocaleBundle;
use tokio_postgres::Row;

use crate::account::pg_err;

/// SQL por tabla del dominio common: (clave texto, valor). Las tablas
/// numéricas castean su id a texto (`::text`) — el wire usa claves ASCII.
const MOB_SQL: &str = "SELECT vnum::text, name FROM common.mob_names WHERE lang = $1";
const ITEM_SQL: &str = "SELECT vnum::text, name FROM common.item_names WHERE lang = $1";
const ITEM_DESC_SQL: &str = "SELECT vnum::text, text FROM common.item_descriptions WHERE lang = $1";
const SKILL_SQL: &str = "SELECT skill_id::text, name FROM common.skill_names WHERE lang = $1";
const MAP_SQL: &str = "SELECT map_id::text, name FROM common.map_names WHERE lang = $1";
const UI_SQL: &str = "SELECT key, value FROM common.ui_texts WHERE lang = $1";

/// Repositorio del locale (ADR-0008): conexión por llamada.
pub struct LocaleRepo {
    pool: PgPool,
}

impl LocaleRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Conexión nueva por llamada (patrón `npc.rs` — coste local ~ms).
    async fn connect(&self) -> Result<Client, String> {
        self.pool
            .get()
            .await
            .map_err(|e| format!("PG pool get: {e}"))
    }

    /// Lee las 6 secciones del locale para `lang` con el fallback EN
    /// (ADR-0009). `lang == "en"` → sin merge (una sola consulta). Errores
    /// de DB → `Err` (el auth degrada: la conexión se cierra con log).
    pub async fn load_for_lang(&self, lang: &str) -> Result<LocaleBundle, String> {
        let client = self.connect().await?;
        let en = lang != "en";
        Ok(LocaleBundle {
            mob: section(&client, MOB_SQL, lang, en).await?,
            item: section(&client, ITEM_SQL, lang, en).await?,
            item_desc: section(&client, ITEM_DESC_SQL, lang, en).await?,
            skill: section(&client, SKILL_SQL, lang, en).await?,
            map: section(&client, MAP_SQL, lang, en).await?,
            ui: section(&client, UI_SQL, lang, en).await?,
        })
    }
}

/// Una sección: filas del idioma + (si `with_en`) las claves de EN que el
/// idioma no tiene (fallback ADR-0009).
async fn section(
    client: &Client,
    sql: &str,
    lang: &str,
    with_en: bool,
) -> Result<Vec<(String, String)>, String> {
    let lang_rows = load_table(client, sql, lang).await?;
    if !with_en {
        return Ok(lang_rows);
    }
    let en_rows = load_table(client, sql, "en").await?;
    Ok(merge_en(lang, &lang_rows, &en_rows))
}

async fn load_table(
    client: &Client,
    sql: &str,
    lang: &str,
) -> Result<Vec<(String, String)>, String> {
    let rows = client
        .query(sql, &[&lang])
        .await
        .map_err(|e| pg_err("LOCALE_LOAD", &e))?;
    Ok(rows.iter().map(row_pair).collect())
}

/// Mapeo (clave, valor) de la fila (el orden de columnas es el contrato de
/// los SQL — id texto + valor texto).
fn row_pair(r: &Row) -> (String, String) {
    (r.get::<_, String>(0), r.get::<_, String>(1))
}

/// Fallback EN (ADR-0009) — función pura (testeable sin PG): las filas del
/// idioma primero; las claves que EN tiene y el idioma NO → valor EN al
/// final. `lang == "en"` → sin merge. Claves ausentes en ambos → omitidas.
/// El PK (clave, lang) de las tablas garantiza claves únicas por idioma (la
/// primera ocurrencia gana, defensivo).
pub fn merge_en(
    lang: &str,
    lang_rows: &[(String, String)],
    en_rows: &[(String, String)],
) -> Vec<(String, String)> {
    if lang == "en" {
        return lang_rows.to_vec();
    }
    let mut out = lang_rows.to_vec();
    let has: std::collections::HashSet<&str> = lang_rows.iter().map(|(k, _)| k.as_str()).collect();
    for (k, v) in en_rows {
        if !has.contains(k.as_str()) {
            out.push((k.clone(), v.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture del EN y de un idioma parcial (las claves del idioma ganan;
    /// las que EN tiene de más se añaden al final; ausentes en ambos → fuera).
    #[test]
    fn merge_en_fallback() {
        let en = vec![
            ("101".to_string(), "Wild Dog".to_string()),
            ("2101".to_string(), "Desert Fox".to_string()),
            ("9001".to_string(), "Weapon Merchant".to_string()),
        ];
        let es = vec![
            ("101".to_string(), "Perro Salvaje".to_string()),
            ("2101".to_string(), "Zorro del Desierto".to_string()),
        ];
        let merged = merge_en("es", &es, &en);
        assert_eq!(
            merged,
            vec![
                ("101".to_string(), "Perro Salvaje".to_string()), // el idioma gana
                ("2101".to_string(), "Zorro del Desierto".to_string()),
                ("9001".to_string(), "Weapon Merchant".to_string()), // fallback EN
            ]
        );
    }

    /// `lang == "en"` → sin merge (las filas tal cual; el caller ni consulta
    /// EN dos veces).
    #[test]
    fn merge_en_noop_for_en() {
        let en = vec![("101".to_string(), "Wild Dog".to_string())];
        let merged = merge_en("en", &en, &[("999".to_string(), "x".to_string())]);
        assert_eq!(merged, en);
    }

    /// Idioma sin filas → todo el EN (fallback puro). Clave en ambos → el
    /// idioma; claves en ninguno → fuera.
    #[test]
    fn merge_en_empty_lang_rows() {
        let en = vec![
            ("101".to_string(), "Wild Dog".to_string()),
            ("102".to_string(), "Wolf".to_string()),
        ];
        let merged = merge_en("zz", &[], &en);
        assert_eq!(merged, en, "idioma inexistente → bundle EN puro");
        let merged = merge_en(
            "es",
            &[("101".to_string(), "Perro Salvaje".to_string())],
            &en,
        );
        assert_eq!(
            merged,
            vec![
                ("101".to_string(), "Perro Salvaje".to_string()),
                ("102".to_string(), "Wolf".to_string()),
            ]
        );
    }

    /// Contrato del SQL: el orden de columnas del mapeo (id texto + valor) —
    /// si alguien lo toca, `row_pair` se desalinea.
    #[test]
    fn sql_column_order() {
        assert_eq!(
            MOB_SQL,
            "SELECT vnum::text, name FROM common.mob_names WHERE lang = $1"
        );
        assert_eq!(
            ITEM_SQL,
            "SELECT vnum::text, name FROM common.item_names WHERE lang = $1"
        );
        assert_eq!(
            ITEM_DESC_SQL,
            "SELECT vnum::text, text FROM common.item_descriptions WHERE lang = $1"
        );
        assert_eq!(
            SKILL_SQL,
            "SELECT skill_id::text, name FROM common.skill_names WHERE lang = $1"
        );
        assert_eq!(
            MAP_SQL,
            "SELECT map_id::text, name FROM common.map_names WHERE lang = $1"
        );
        assert_eq!(
            UI_SQL,
            "SELECT key, value FROM common.ui_texts WHERE lang = $1"
        );
    }

    /// Live-PG (gated, patrón account_pg.rs): `load_for_lang` contra los datos
    /// reales importados (2026-08-12): ES con el merge EN (las 3 descripciones
    /// EN-only 31084/53526/71219) y un idioma inexistente → bundle EN puro.
    #[tokio::test]
    #[ignore = "requiere PG real (WSL): cargo test --package database -- --ignored"]
    async fn load_for_lang_live_pg() {
        let pg = std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| {
            "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2".to_string()
        });
        let repo = LocaleRepo::new(crate::pool::new_pool(&pg, 4).expect("pool"));
        let es = repo.load_for_lang("es").await.expect("load es");
        assert_eq!(es.mob.len(), 2_876, "mob ES (dump 2026-08-12)");
        assert_eq!(es.item.len(), 11_427, "item ES");
        assert_eq!(
            es.item_desc.len(),
            7_499,
            "7.496 ES + 3 EN-only (merge ADR-0009)"
        );
        assert!(
            es.item_desc.iter().any(|(k, _)| k == "31084"),
            "31084 (EN-only) presente"
        );
        assert!(
            es.item_desc.iter().any(|(k, _)| k == "53526"),
            "53526 (EN-only) presente"
        );
        assert!(
            es.item_desc.iter().any(|(k, _)| k == "71219"),
            "71219 (EN-only) presente"
        );
        assert_eq!(es.skill.len(), 134, "skill ES");
        assert_eq!(es.ui.len(), 1_301, "ui ES");
        assert!(
            es.map.is_empty(),
            "map_names sin fuente en el runtime (gap documentado)"
        );
        let zz = repo.load_for_lang("zz").await.expect("load zz");
        assert_eq!(zz.mob.len(), 2_876, "idioma inexistente → fallback EN puro");
        assert_eq!(zz.item.len(), 11_427);
        assert_eq!(
            zz.item_desc.len(),
            7_499,
            "las EN-only también en el fallback"
        );
        let en = repo.load_for_lang("en").await.expect("load en");
        assert_eq!(en.mob.len(), 2_876, "EN sin merge (una sola consulta)");
    }
}
