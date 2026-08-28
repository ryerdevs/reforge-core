//! F3 (ADR-0008): dominio social — messenger + guildas (schema `player`).
//!
//! # Tabla de paridad QID (legacy → Rust)
//!
//! | QID / query legacy | file:line | Metodo Rust | SQL / semantica |
//! |---|---|---|---|
//! | Messenger LoadList | `messenger_manager.cpp:57-58` | `MessengerRepo::list` | 2 columnas (`account, companion`), sin ORDER BY (el C++ construye sets). E2E Q6 `scripts/gpg/e2e_db.sh:149`. |
//! | Messenger Add | `messenger_manager.cpp:214` | `MessengerRepo::add` / `add_mutated` | INSERT idempotente (`ON CONFLICT (account, companion) DO NOTHING` — PK natural; ver messenger.rs). |
//! | Messenger Remove | `messenger_manager.cpp:273-274` | `MessengerRepo::remove` | DELETE por PK. |
//! | Guild Load (boot) | `db/src/GuildManager.cpp:161` (`CGuildManager::Initialize`) | `GuildRepo::load_all` | 8 columnas (`id, name, ladder_point, win, draw, loss, gold, level`) — `ParseResult` del boot del db. |
//! | Guild Load (uno) | `GuildManager.cpp:191` (`CGuildManager::Load`) | `GuildRepo::load_guild` | Mismas 8 columnas + `WHERE id = $1`. |
//! | QID_GUILD_RANKING (20) | `GuildManager.cpp:201` (`QueryRanking`) | `GuildRepo::ranking` | `SELECT id, name, ladder_point ... ORDER BY ladder_point DESC LIMIT 20` (top 20). |
//! | Grade upsert (slice F4) | `guild.cpp:104-111,799,838` | `GuildRepo::upsert_grade` | `player.guild_grade` (PK guild_id+grade): INSERT + `ON CONFLICT DO UPDATE` name/auth; auth serializado a SET textual (el legacy escribia `%d`, MariaDB canonicalizaba). |
//! | Guild save (upsert) | `GuildManager.cpp:340/354/368/727/762` + `guild.cpp:632` | `GuildRepo::save_guild` | 8 columnas del load: INSERT + `ON CONFLICT (id) DO UPDATE` — consolida los UPDATEs parciales legacy (win/loss/draw/ladder_point/gold/level) en una sentencia idempotente. |
//!
//! # Estado de migracion (verificado en pg_catalog 2026-08-13)
//!
//! `player.guild` (14 columnas, PK id), `player.guild_war_reservation` y —
//! desde 2026-08-13 — `player.guild_member`, `player.guild_grade`,
//! `player.guild_comment` estan migrados a PG (`scripts/gpg/
//! migrate_guild_tables.sql`; fuente: dump MariaDB archivado, tablas vacias:
//! 0 filas en las 3; huerfanos guild_id->guild: 0). La adaptacion sigue las
//! convenciones G-PG (int unsigned->bigint, set->text+CHECK con el typo
//! legacy `REMOVE_MEMEBER` preservado, `guild_comment.id` identity por el
//! INSERT sin id de `guild.cpp:1014`). El slice de guildas futuro puede portar
//! los QIDs de miembro/expulsion/disolucion (`ClientManagerGuild.cpp:31-117`)
//! sobre estas tablas. Los metodos de `GuildRepo` aqui solo tocan
//! `player.guild` (migrado).

use crate::pool::{Client, PgPool};

use crate::account::pg_err;

/// Re-export del messenger (modulo propio F3, antes de la creacion del
/// modulo social): list/add/remove viven en `messenger.rs`.
pub use crate::messenger::{MessengerRepo, MessengerRow};

/// Fila del load de guild (8 columnas, `GuildManager.cpp:161/191`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildRow {
    pub id: i64,
    pub name: String,
    pub ladder_point: i32,
    pub win: i32,
    pub draw: i32,
    pub loss: i32,
    pub gold: i32,
    pub level: i16,
}

/// Load de todas las guildas (boot del db, `GuildManager.cpp:161`).
const GUILD_LOAD_ALL_SQL: &str = "\
SELECT id, name, ladder_point, win, draw, loss, gold, level FROM player.guild";

/// Load de una guild (`GuildManager.cpp:191`).
const GUILD_LOAD_SQL: &str = "\
SELECT id, name, ladder_point, win, draw, loss, gold, level FROM player.guild WHERE id = $1";

/// Ranking (QID_GUILD_RANKING, `GuildManager.cpp:201`): top 20.
const GUILD_RANKING_SQL: &str = "\
SELECT id, name, ladder_point FROM player.guild ORDER BY ladder_point DESC LIMIT 20";

/// Upsert de grade (PK `guild_id+grade`): INSERT + ON CONFLICT DO UPDATE —
/// cubre el CREATE de grades (guild.cpp:104-111) y los UPDATE de name/auth
/// (guild.cpp:799/838) en una sola sentencia idempotente.
const GUILD_GRADE_UPSERT_SQL: &str = "\
INSERT INTO player.guild_grade (guild_id, grade, name, auth) VALUES ($1, $2, $3, $4)
ON CONFLICT (guild_id, grade) DO UPDATE SET name = EXCLUDED.name, auth = EXCLUDED.auth";

/// Upsert de guild (PK id): INSERT + ON CONFLICT (id) DO UPDATE sobre las 8
/// columnas del load — consolida los UPDATEs parciales legacy (win/loss/draw
/// `GuildManager.cpp:340/354/368`, ladder_point :727, gold :762,
/// level/exp/skill_point `guild.cpp:632`) en una sentencia idempotente. El
/// resto de columnas (master/sp/skill/token): intactas en UPDATE, defaults en
/// INSERT (crear con master = slice futuro, `guild.cpp:84-99`).
const GUILD_UPSERT_SQL: &str = "\
INSERT INTO player.guild (id, name, ladder_point, win, draw, loss, gold, level)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
ON CONFLICT (id) DO UPDATE SET
  name = EXCLUDED.name, ladder_point = EXCLUDED.ladder_point, win = EXCLUDED.win,
  draw = EXCLUDED.draw, loss = EXCLUDED.loss, gold = EXCLUDED.gold, level = EXCLUDED.level";

/// Literales SET en orden de definicion (canonicalizacion de MariaDB del SET;
/// parity bitmask guild.h:92-95). El typo `REMOVE_MEMEBER` es legacy
/// (guild.cpp:106/838) y el CHECK de guild_grade lo exige literal.
const GUILD_AUTH_LITERALS: [(u8, &str); 4] =
    [(1, "ADD_MEMBER"), (2, "REMOVE_MEMEBER"), (4, "NOTICE"), (8, "USE_SKILL")];

/// Bitmask de auth -> SET textual; 0 -> `''` (ambos aceptados por el CHECK).
fn auth_to_set(auth: u8) -> String {
    GUILD_AUTH_LITERALS
        .iter()
        .filter(|(b, _)| auth & *b != 0)
        .map(|(_, s)| *s)
        .collect::<Vec<_>>()
        .join(",")
}

/// Repositorio del dominio social (guildas). Conexion por llamada (ADR-0008).
pub struct GuildRepo {
    pool: PgPool,
}

impl GuildRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn connect(&self) -> Result<Client, String> {
        self.pool.get().await.map_err(|e| format!("PG pool get: {e}"))
    }

    /// Load de TODAS las guildas (boot del db — `GuildManager.cpp:161`,
    /// `CGuildManager::Initialize`). Vec vacio = sin guildas.
    pub async fn load_all(&self) -> Result<Vec<GuildRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(GUILD_LOAD_ALL_SQL, &[])
            .await
            .map_err(|e| pg_err("GUILD_LOAD_ALL", &e))?;
        rows.iter().map(guild_from_row).collect()
    }

    /// Load de una guild (`GuildManager.cpp:191`). `None` = no existe.
    pub async fn load_guild(&self, id: i64) -> Result<Option<GuildRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(GUILD_LOAD_SQL, &[&id])
            .await
            .map_err(|e| pg_err("GUILD_LOAD", &e))?;
        rows.first().map(guild_from_row).transpose()
    }

    /// Upsert de una guild (`GuildRow`, las 8 columnas del load): INSERT si
    /// `id` no existe, UPDATE de las 8 columnas si existe. Idempotente.
    pub async fn save_guild(&self, guild: &GuildRow) -> Result<(), String> {
        let client = self.connect().await?;
        let GuildRow { id, name, ladder_point, win, draw, loss, gold, level } = guild;
        client
            .execute(GUILD_UPSERT_SQL, &[id, name, ladder_point, win, draw, loss, gold, level])
            .await
            .map(|_| ())
            .map_err(|e| pg_err("GUILD_UPSERT", &e))
    }

    /// Ranking (QID_GUILD_RANKING, `GuildManager.cpp:201`): top 20 por
    /// ladder_point DESC — `(id, name, ladder_point)`.
    pub async fn ranking(&self) -> Result<Vec<(i64, String, i32)>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(GUILD_RANKING_SQL, &[])
            .await
            .map_err(|e| pg_err("GUILD_RANKING", &e))?;
        rows.iter()
            .map(|r| {
                Ok((
                    r.try_get(0).map_err(|e| format!("col0 id: {e}"))?,
                    r.try_get(1).map_err(|e| format!("col1 name: {e}"))?,
                    r.try_get(2).map_err(|e| format!("col2 ladder_point: {e}"))?,
                ))
            })
            .collect()
    }

    /// Upsert de un grade (`player.guild_grade`, PK guild_id+grade). `grade`
    /// 1..=15, `auth` bitmask (guild.h:92-95). Idempotente: vuelve a llamarlo
    /// para set_grade_auth (UPDATE name/auth via ON CONFLICT).
    pub async fn upsert_grade(
        &self,
        guild_id: i64,
        grade: u8,
        name: &str,
        auth: u8,
    ) -> Result<(), String> {
        let client = self.connect().await?;
        client
            .execute(
                GUILD_GRADE_UPSERT_SQL,
                &[&guild_id, &(grade as i16), &name, &auth_to_set(auth)],
            )
            .await
            .map(|_| ())
            .map_err(|e| pg_err("GUILD_GRADE_UPSERT", &e))
    }
}

/// Mapeo de las 8 columnas del load (orden `GuildManager.cpp:161`).
fn guild_from_row(r: &tokio_postgres::Row) -> Result<GuildRow, String> {
    Ok(GuildRow {
        id: r.try_get(0).map_err(|e| format!("col0 id: {e}"))?,
        name: r.try_get(1).map_err(|e| format!("col1 name: {e}"))?,
        ladder_point: r.try_get(2).map_err(|e| format!("col2 ladder_point: {e}"))?,
        win: r.try_get(3).map_err(|e| format!("col3 win: {e}"))?,
        draw: r.try_get(4).map_err(|e| format!("col4 draw: {e}"))?,
        loss: r.try_get(5).map_err(|e| format!("col5 loss: {e}"))?,
        gold: r.try_get(6).map_err(|e| format!("col6 gold: {e}"))?,
        level: r.try_get(7).map_err(|e| format!("col7 level: {e}"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Load all: 8 columnas en el orden del boot (`GuildManager.cpp:161`).
    #[test]
    fn guild_load_all_8_columns_in_contract_order() {
        let cols: Vec<&str> = GUILD_LOAD_ALL_SQL
            .split_once(" FROM ")
            .expect("FROM")
            .0
            .trim_start_matches("SELECT")
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(
            cols,
            ["id", "name", "ladder_point", "win", "draw", "loss", "gold", "level"]
        );
        assert!(GUILD_LOAD_ALL_SQL.contains("FROM player.guild"));
        assert!(!GUILD_LOAD_ALL_SQL.contains("WHERE"), "boot carga todas");
        assert!(!GUILD_LOAD_ALL_SQL.contains("ORDER BY"), "sin orden (parity)");
    }

    /// Load uno: mismas 8 columnas + WHERE id = $1 (`GuildManager.cpp:191`).
    #[test]
    fn guild_load_sql_where_id() {
        let select = GUILD_LOAD_SQL.split_once(" FROM ").expect("FROM").0;
        assert_eq!(
            select.trim_start_matches("SELECT").split(',').count(),
            8,
            "8 columnas del load"
        );
        assert!(GUILD_LOAD_SQL.contains("WHERE id = $1"), "bind del id");
    }

    /// Ranking: 3 columnas + ORDER BY ladder_point DESC LIMIT 20
    /// (QID_GUILD_RANKING, `GuildManager.cpp:201`).
    #[test]
    fn guild_ranking_sql_order_and_limit() {
        let select = GUILD_RANKING_SQL.split_once(" FROM ").expect("FROM").0;
        let cols: Vec<&str> = select
            .trim_start_matches("SELECT")
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(cols, ["id", "name", "ladder_point"]);
        assert!(
            GUILD_RANKING_SQL.contains("ORDER BY ladder_point DESC LIMIT 20"),
            "top 20 por ladder_point"
        );
    }

    /// El re-export del messenger sigue disponible desde el modulo social.
    #[test]
    fn social_reexports_messenger_repo() {
        let _ = MessengerRepo::new(crate::pool::new_pool("host=127.0.0.1 port=1 user=x password=x dbname=x", 2).expect("pool"));
        let row = MessengerRow { account: "a".into(), companion: "b".into() };
        assert_eq!(row.account, "a");
    }

    /// SET serialization: bitmask -> literales en orden de definicion (parity
    /// canonicalizacion MariaDB; guild.h:92-95). 0 -> ''.
    #[test]
    fn grade_auth_serializes_to_set_literals() {
        assert_eq!(auth_to_set(0), "");
        assert_eq!(auth_to_set(1), "ADD_MEMBER");
        assert_eq!(auth_to_set(6), "REMOVE_MEMEBER,NOTICE");
        assert_eq!(auth_to_set(10), "REMOVE_MEMEBER,USE_SKILL");
        assert_eq!(auth_to_set(15), "ADD_MEMBER,REMOVE_MEMEBER,NOTICE,USE_SKILL");
    }

    /// Verifier live-PG (gated, patrón locale.rs): save→load roundtrip sobre
    /// las 8 columnas y el camino ON CONFLICT (2º save con valores distintos
    /// → load refleja el UPDATE, no una 2ª fila). FALLA si se revierte el
    /// save_guild/load_guild o si el upsert no actualiza.
    #[tokio::test]
    #[ignore = "requiere PG real: cargo test --package database -- --ignored"]
    async fn save_guild_roundtrip_live_pg() {
        let pg = std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| {
            "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2".to_string()
        });
        let repo = GuildRepo::new(crate::pool::new_pool(&pg, 2).expect("pool"));
        let id = 9_000_000_001; // rango de test (ids reales u32 << 9e9)
        let g = GuildRow { id, name: "Verifier".into(), ladder_point: 7, win: 3, draw: 1, loss: 2, gold: 500, level: 3 };
        repo.save_guild(&g).await.expect("save INSERT");
        assert_eq!(repo.load_guild(id).await.expect("load"), Some(g.clone()));
        let g2 = GuildRow { ladder_point: 9, gold: 600, ..g };
        repo.save_guild(&g2).await.expect("save ON CONFLICT");
        assert_eq!(repo.load_guild(id).await.expect("load"), Some(g2));
        repo.connect().await.expect("pool")
            .execute("DELETE FROM player.guild WHERE id = $1", &[&id]).await.expect("cleanup");
    }
}
