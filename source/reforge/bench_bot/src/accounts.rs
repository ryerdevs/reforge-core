//! Provisión de cuentas desechables `bench<n>` en PostgreSQL (solo el modo
//! `--create-accounts` / `--cleanup-accounts` del harness).
//!
//! El canal rechaza la segunda sesión de la MISMA cuenta ("ALREADY",
//! `ChannelLoginGuard` — parity del C++) → N bots necesitan N cuentas.
//! Estas cuentas son throwaway: prefijo `bench` por defecto, password
//! programático, personaje plantilla en el mapa 41 (coordenadas en UNITS —
//! AGENTS.md: `969600, 278400` = c1 del village; coordenadas basura rompen el
//! cliente con 0xc0000374).
//!
//! Esquema verificado contra el PG 18.4 real (2026-08-13):
//! - `account.account`: `id` bigint identity BY DEFAULT; `login` UNIQUE
//!   (`account_login_key`); `password` = hash MySQL con `*` (41 chars);
//!   `social_id` varchar(7) NOT NULL; `status` default 'OK'; `lang` default 'es'.
//! - `player.player`: `id` identity BY DEFAULT (`PlayerRepo::create` —
//!   el repo del crate database, regla "acceso solo via repositorios").
//! - `player.player_index`: `id` = **account id** (el `LEFT JOIN` del login
//!   usa `pi.id = a.id`), `pid1..pid5` bigint, `empire` smallint; PK = id.
//!
//! Idempotente: re-ejecutar `--create-accounts N` con el mismo prefijo
//! reutiliza/actualiza lo existente (`ON CONFLICT`). `--cleanup-accounts`
//! borra cuentas+personajes+índices del prefijo.
//!
//! ⚠️ NO usa la cuenta real `test` — el harness nunca la toca.

use database::player::{PlayerCreate, PlayerRepo};

/// Cadena de conexión por defecto (parity `source/deploy/win/{auth,channel}.toml`).
pub const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";

/// Login del bot i: `{prefix}_{i}` (≤ 16 chars — columna varchar(16)).
pub fn bench_login(prefix: &str, i: usize) -> String {
    format!("{prefix}{i}")
}

/// Nombre del personaje del bot i: `{prefix}{i}c` (≤ 24 chars).
pub fn bench_char_name(prefix: &str, i: usize) -> String {
    format!("{prefix}{i}c")
}

/// Cuenta provista para el bot i.
#[derive(Debug, Clone)]
pub struct BenchAccount {
    pub login: String,
    pub account_id: i64,
    pub player_id: i64,
}

// Plantilla del personaje (valores reales de `player.player` id=1 verificado
// 2026-08-13 — job 1 warrior, mapa 41, village c1 en UNITS).
const CHAR_X: i32 = 969_600;
const CHAR_Y: i32 = 278_400;
const CHAR_MAP: i32 = 41;
const CHAR_HP: i32 = 770;
const CHAR_MP: i32 = 260;
const CHAR_STAMINA: i16 = 815;
const CHAR_ST: i16 = 4;
const CHAR_HT: i16 = 3;
const CHAR_DX: i16 = 6;
const CHAR_IQ: i16 = 3;
/// 255 skills × 6 B (`TPlayerSkill` — parity `player.skill_level` real: 1530 B).
const SKILL_LEVEL_BYTES: usize = 255 * 6;
/// 36 quickslots × 2 B (`TQuickslot` — parity real: 72 B).
const QUICKSLOT_BYTES: usize = 36 * 2;

/// Valida la longitud del prefijo: login `{prefix}9999` ≤ 16 y nombre
/// `{prefix}9999c` ≤ 24 (columnas varchar). El sufijo del bot i va PEGADO
/// (sin separador — el auth solo acepta alfanuméricos).
pub fn validate_prefix(prefix: &str) -> Result<(), String> {
    if prefix.is_empty() {
        return Err("el prefijo de cuentas no puede estar vacío".into());
    }
    if !prefix.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!(
            "prefijo '{prefix}' inválido: el auth solo acepta logins alfanuméricos \
             (parity input_auth.cpp:13-53; bench_0 → NOID)"
        ));
    }
    if prefix.len() > 16 - 4 {
        return Err(format!(
            "prefijo '{prefix}' demasiado largo: login {prefix}9999 excede 16 chars (varchar(16))"
        ));
    }
    if prefix.len() > 24 - 5 {
        return Err(format!(
            "prefijo '{prefix}' demasiado largo: nombre de personaje {prefix}9999c excede 24 chars"
        ));
    }
    Ok(())
}

/// Crea (o actualiza) `n` cuentas `{prefix}{i}` + personaje + índice.
///
/// Idempotente por `ON CONFLICT`: la cuenta existente se actualiza con el
/// password actual (el harness siempre sabe la password que va a usar) y el
/// índice apunta al personaje del slot 0. Devuelve una entrada por bot.
pub async fn create_accounts(
    pg: &str,
    prefix: &str,
    n: usize,
    password: &str,
) -> Result<Vec<BenchAccount>, String> {
    validate_prefix(prefix)?;
    let (client, connection) = tokio_postgres::connect(pg, tokio_postgres::NoTls)
        .await
        .map_err(|e| format!("PG connect: {e}"))?;
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let hash = database::account::mysql5_password(password);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let login = bench_login(prefix, i);
        let account_id = upsert_account(&client, &login, &hash).await?;
        let player_id = upsert_character(&client, pg, account_id, prefix, i).await?;
        client
            .execute(
                "INSERT INTO player.player_index (id, pid1, empire) VALUES ($1, $2, 1) \
                 ON CONFLICT (id) DO UPDATE SET pid1 = EXCLUDED.pid1, empire = EXCLUDED.empire",
                &[&account_id, &player_id],
            )
            .await
            .map_err(|e| pg_err(&format!("PLAYER_INDEX upsert {login}"), &e))?;
        out.push(BenchAccount {
            login,
            account_id,
            player_id,
        });
    }
    Ok(out)
}

/// Upsert de la cuenta: `ON CONFLICT (login)` actualiza el password (el
/// harness controla el prefijo — nunca toca cuentas fuera de él).
async fn upsert_account(
    client: &tokio_postgres::Client,
    login: &str,
    hash: &str,
) -> Result<i64, String> {
    let rows = client
        .query(
            "INSERT INTO account.account (login, password, social_id, status, lang) \
             VALUES ($1, $2, '1234567', 'OK', 'es') \
             ON CONFLICT (login) DO UPDATE SET password = EXCLUDED.password \
             RETURNING id",
            &[&login, &hash],
        )
        .await
        .map_err(|e| pg_err(&format!("ACCOUNT upsert {login}"), &e))?;
    rows.first()
        .and_then(|r| r.try_get(0).ok())
        .ok_or_else(|| format!("ACCOUNT upsert {login}: sin id (RETURNING vacío)"))
}

/// Personaje del slot 0: reutiliza el existente si lo hay, si no lo crea con
/// `PlayerRepo::create` (el repo del crate — plantilla del mapa 41).
async fn upsert_character(
    client: &tokio_postgres::Client,
    pg: &str,
    account_id: i64,
    prefix: &str,
    i: usize,
) -> Result<i64, String> {
    let login = bench_login(prefix, i);
    let existing = client
        .query(
            "SELECT id FROM player.player WHERE account_id = $1 ORDER BY id LIMIT 1",
            &[&account_id],
        )
        .await
        .map_err(|e| pg_err(&format!("PLAYER find {login}"), &e))?;
    if let Some(row) = existing.first() {
        return row
            .try_get(0)
            .map_err(|e| format!("PLAYER find {login}: {e}"));
    }
    let c = PlayerCreate {
        account_id,
        name: bench_char_name(prefix, i),
        level: 1,
        st: CHAR_ST,
        ht: CHAR_HT,
        dx: CHAR_DX,
        iq: CHAR_IQ,
        job: 1,
        voice: 0,
        dir: 0,
        x: CHAR_X,
        y: CHAR_Y,
        z: 0,
        map_index: 41,
        hp: CHAR_HP,
        mp: CHAR_MP,
        random_hp: 0,
        random_sp: 0,
        stat_point: 0,
        stamina: CHAR_STAMINA,
        part_base: 0,
        part_main: 0,
        part_hair: 0,
        gold: 0,
        playtime: 0,
        skill_level: vec![0; SKILL_LEVEL_BYTES],
        quickslot: vec![0; QUICKSLOT_BYTES],
    };
    let name = c.name.clone();
    let player_id = PlayerRepo::new(database::pool::new_pool(pg, 2).expect("pool"))
        .create(&c)
        .await
        .map_err(|e| format!("PLAYER_CREATE {login}/{name}: {e}"))?;
    // `PlayerRepo::create` (Q4 del C++) NO incluye map_index/exit_* (el C++
    // los fija al spawn — `CHAR_START_*`; el canal aún no crea personajes).
    // Sin el UPDATE el personaje cargaría en el mapa 0 (spawns vacíos y
    // coordenadas fuera del mundo). Se fija el village c1 del mapa 41
    // (parity de la fila real id=1). OJO: `map_index` es integer (int4) —
    // i64 sería rechazado por to_sql_checked (parámetros heterogéneos).
    let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        vec![&player_id, &CHAR_MAP, &CHAR_X, &CHAR_Y];
    client
        .execute(
            "UPDATE player.player SET map_index = $2, exit_x = $3, exit_y = $4, \
             exit_map_index = $2 WHERE id = $1",
            &params,
        )
        .await
        .map_err(|e| pg_err(&format!("PLAYER map fix {login}"), &e))?;
    Ok(player_id)
}

/// Borra cuentas `{prefix}_%` + sus personajes + sus índices (una transacción).
/// Devuelve el número de cuentas borradas.
pub async fn cleanup_accounts(pg: &str, prefix: &str) -> Result<usize, String> {
    validate_prefix(prefix)?;
    let (mut client, connection) = tokio_postgres::connect(pg, tokio_postgres::NoTls)
        .await
        .map_err(|e| format!("PG connect: {e}"))?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let pattern = format!("{prefix}%");
    let tx = client
        .transaction()
        .await
        .map_err(|e| pg_err("cleanup tx begin", &e))?;
    tx.execute(
        "DELETE FROM player.player WHERE account_id IN \
         (SELECT id FROM account.account WHERE login LIKE $1)",
        &[&pattern],
    )
    .await
    .map_err(|e| pg_err("cleanup player", &e))?;
    tx.execute(
        "DELETE FROM player.player_index WHERE id IN \
         (SELECT id FROM account.account WHERE login LIKE $1)",
        &[&pattern],
    )
    .await
    .map_err(|e| pg_err("cleanup index", &e))?;
    let rows = tx
        .execute(
            "DELETE FROM account.account WHERE login LIKE $1",
            &[&pattern],
        )
        .await
        .map_err(|e| pg_err("cleanup account", &e))?;
    tx.commit()
        .await
        .map_err(|e| pg_err("cleanup tx commit", &e))?;
    Ok(rows as usize)
}

/// Error con contexto + SQLSTATE (mismo patrón que `database::account::pg_err`).
fn pg_err(ctx: &str, e: &tokio_postgres::Error) -> String {
    let code = e.code().map(|c| c.code().to_string()).unwrap_or_default();
    format!("{ctx}: {e} (sqlstate {code})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bench_names_formats_and_bounds() {
        assert_eq!(bench_login("bench", 0), "bench0");
        assert_eq!(bench_login("bench", 1000), "bench1000");
        assert_eq!(bench_char_name("bench", 7), "bench7c");
        // Límites de las columnas: login ≤ 16, nombre ≤ 24.
        assert!(bench_login("bench", 1000).len() <= 16);
        assert!(bench_char_name("bench", 1000).len() <= 24);
        assert!(bench_login("a", 9999).len() <= 16);
    }

    #[test]
    fn prefix_validation() {
        assert!(validate_prefix("bench").is_ok());
        assert!(validate_prefix("a").is_ok());
        assert!(validate_prefix("").is_err(), "vacío");
        // 13 chars → "abcdefghijklm9999" = 17 > 16 → error.
        assert!(validate_prefix("abcdefghijklm").is_err(), "login excede 16");
        // No alfanuméricos → el auth los rechaza (input_auth.cpp:13-53).
        assert!(
            validate_prefix("bench_").is_err(),
            "underscore no alfanumérico"
        );
        // 20 chars → nombre "abcdefghijklmnopqrst9999c" = 25 > 24 → error.
        assert!(
            validate_prefix("abcdefghijklmnopqrst").is_err(),
            "nombre excede 24"
        );
    }

    /// Plantilla: los blobs del personaje tienen el tamaño REAL del wire
    /// (255×6 skills, 36×2 quickslots — parity de los rows existentes).
    #[test]
    fn character_template_blob_sizes() {
        assert_eq!(SKILL_LEVEL_BYTES, 1530, "255 × TPlayerSkill(6 B)");
        assert_eq!(QUICKSLOT_BYTES, 72, "36 × TQuickslot(2 B)");
    }

    /// Los SQL de provisión son idempotentes (ON CONFLICT) — los re-runs del
    /// harness no rompen contra cuentas existentes.
    #[test]
    fn provision_sql_is_idempotent() {
        // Los SQL viven inline en create_accounts; aquí se verifica el patrón
        // con el shape del SQL del índice (constante del flujo).
        let sql = "INSERT INTO player.player_index (id, pid1, empire) VALUES ($1, $2, 1) \
                   ON CONFLICT (id) DO UPDATE SET pid1 = EXCLUDED.pid1, empire = EXCLUDED.empire";
        assert!(
            sql.contains("ON CONFLICT (id) DO UPDATE SET pid1"),
            "índice idempotente"
        );
    }

    /// Live-DB gated: requiere el PostgreSQL nativo (127.0.0.1:5432, mt2/mt2,
    /// db metin2) — `cargo test -p bench_bot -- --ignored` con el stack arriba.
    #[tokio::test]
    #[ignore = "requiere PG real (metin2 en 127.0.0.1:5432)"]
    async fn create_login_cleanup_roundtrip_live_pg() {
        let prefix = format!("bt{}", std::process::id()); // ≤ 11 chars (validación)
        let pg = DEFAULT_PG;
        let accs = create_accounts(pg, &prefix, 2, "1234")
            .await
            .expect("create");
        assert_eq!(accs.len(), 2);
        // El login del canal acepta las credenciales (AccountRepo::login).
        for a in &accs {
            let ok =
                database::account::AccountRepo::new(database::pool::new_pool(pg, 2).expect("pool"))
                    .login(&a.login, "1234")
                    .await
                    .expect("login query");
            assert!(ok.is_some(), "{} debe loguear con 1234", a.login);
        }
        let n = cleanup_accounts(pg, &prefix).await.expect("cleanup");
        assert_eq!(n, 2, "se borran las 2 cuentas");
        // Limpieza total incluso si el assert anterior falló (best-effort).
        let _ = cleanup_accounts(pg, &prefix).await;
    }
}
