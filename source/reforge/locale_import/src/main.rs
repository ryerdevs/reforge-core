//! F1 â€” Importer del locale (plan `docs/plans/locale-redesign.md`): un
//! subcomando POR DOMINIO, cada uno independiente y re-ejecutable
//! (idempotente: borra las filas del dominio antes de insertar).
//!
//! Fuentes (SOLO lectura):
//! - Dumps de DumpProto en el repo: `source/tools/proto/<lang>/{mob,item}_names.txt`
//! - Pack locale (fuente de los epk): `source/tools/pack/locale/locale/<lang>/`
//!   (itemdesc.txt, skilldesc.txt, locale_interface.txt)
//! - Runtime del servidor en WSL (UNC): `\\wsl$\Debian-M2\home\m2\source\
//!   metin2_svfiles\main\srv1\share\locale\spain\` (locale_string_XX.txt,
//!   map/ â€” verificado 2026-08-12 que la vista UNC es legible desde Rust)
//!
//! Subcomandos:
//! - `import-mobs <lang>`   â€” common.mob_names (dump mob_names.txt)
//! - `import-items <lang>`  â€” common.item_names + item_descriptions
//! - `import-skills <lang>` â€” common.skill_names (skilldesc.txt col 2)
//! - `import-ui <lang>`     â€” common.ui_texts (locale_interface.txt)
//! - `import-messages`      â€” common.message_texts (16 Ã— locale_string_XX.txt)
//! - `import-maps`          â€” world.maps (index + Setting + Town)
//! - `import-spawns`        â€” world.spawns (vÃ­a game_core::npc::load_map_spawns)
//!
//! NOTA: `common.map_names` e `common.item_icons` NO tienen fuente en el
//! runtime (los nombres de mapa son imÃ¡genes en el pack; los iconos son
//! TGA dentro de icon.epk) â€” quedan vacÃ­as (ver reporte).
//!
//! PG: `host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2`
//! (misma cadena que el crate `database`). Todos los valores se insertan
//! como texto: PG coacciona los parÃ¡metros unknown al tipo de la columna.

mod parse;

use std::fs;

use game_core::npc::{load_map_spawns, SpawnEntry, SpawnKind};
use tokio_postgres::{types::ToSql, Client, NoTls};

use parse::{
    decode_key, decode_lang, parse_interface, parse_itemdesc, parse_locale_string,
    parse_map_index, parse_names_dump, parse_setting_base, parse_skilldesc, parse_town_spawn,
};

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";
const DEFAULT_PROTO_DIR: &str = r"C:\projects\Metin2\source\tools\proto";
const DEFAULT_PACK_LOCALE_DIR: &str = r"C:\projects\Metin2\source\tools\pack\locale\locale";
const DEFAULT_LOCALE_STRINGS_DIR: &str =
    r"\\wsl$\Debian-M2\home\m2\source\metin2_svfiles\main\srv1\share\locale\spain";
const DEFAULT_MAP_PATH: &str =
    r"\\wsl$\Debian-M2\home\m2\source\metin2_svfiles\main\srv1\share\locale\spain\map";

/// Los 16 idiomas del Language System (locale_service.cpp:20-24) â€” los
/// archivos del runtime son `locale_string_XX.txt` con XX en MAYÃšSCULAS.
const MESSAGE_LANGS: [&str; 16] = [
    "AE", "CZ", "DE", "DK", "EN", "ES", "FR", "GR", "HU", "IT", "NL", "PL", "PT", "RO", "RU",
    "TR",
];

struct Opts {
    sub: String,
    lang: Option<String>,
    pg: String,
    proto_dir: String,
    pack_locale_dir: String,
    locale_strings_dir: String,
    map_path: String,
}

fn usage() -> ! {
    eprintln!(
        "locale_import: importer F1 del locale (plan locale-redesign.md)

USO: locale_import <subcomando> [lang] [flags]

Subcomandos (uno por dominio, re-ejecutables â€” borran sus filas antes):
  import-mobs <lang>    common.mob_names      (dump source/tools/proto/<lang>/mob_names.txt)
  import-items <lang>   common.item_names + item_descriptions (dump + itemdesc.txt)
  import-skills <lang>  common.skill_names    (pack <lang>/skilldesc.txt, col 2)
  import-ui <lang>      common.ui_texts       (pack <lang>/locale_interface.txt)
  import-messages       common.message_texts (16 x runtime locale_string_XX.txt)
  import-maps           world.maps            (runtime map/index + Setting + Town)
  import-spawns         world.spawns          (runtime, via game_core::npc::load_map_spawns)

Flags:
  --pg <conn>           cadena PG (default: host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2)
  --proto-dir <dir>     dir de los dumps (default: source\\tools\\proto del repo)
  --pack-locale <dir>   dir de los packs (default: source\\tools\\pack\\locale\\locale)
  --locale-strings <dir> dir del runtime con locale_string_XX.txt (default: UNC WSL spain)
  --map-path <dir>      dir map/ del runtime (default: UNC WSL spain\\map)"
    );
    std::process::exit(1);
}

fn parse_opts() -> Opts {
    let mut opts = Opts {
        sub: String::new(),
        lang: None,
        pg: DEFAULT_PG.to_string(),
        proto_dir: DEFAULT_PROTO_DIR.to_string(),
        pack_locale_dir: DEFAULT_PACK_LOCALE_DIR.to_string(),
        locale_strings_dir: DEFAULT_LOCALE_STRINGS_DIR.to_string(),
        map_path: DEFAULT_MAP_PATH.to_string(),
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--pg" | "--proto-dir" | "--pack-locale" | "--locale-strings" | "--map-path" => {
                let flag = args[i].clone();
                i += 1;
                let Some(value) = args.get(i) else {
                    eprintln!("{flag} requiere un valor");
                    usage();
                };
                match flag.as_str() {
                    "--pg" => opts.pg = value.clone(),
                    "--proto-dir" => opts.proto_dir = value.clone(),
                    "--pack-locale" => opts.pack_locale_dir = value.clone(),
                    "--locale-strings" => opts.locale_strings_dir = value.clone(),
                    "--map-path" => opts.map_path = value.clone(),
                    _ => unreachable!(),
                }
            }
            "-h" | "--help" => usage(),
            s if s.starts_with('-') => {
                eprintln!("flag desconocido: {s}");
                usage();
            }
            s => positional.push(s.to_string()),
        }
        i += 1;
    }
    opts.sub = positional.first().cloned().unwrap_or_else(|| usage());
    opts.lang = positional.get(1).cloned();
    opts
}

async fn connect(pg: &str) -> Result<Client, String> {
    let (client, connection) = tokio_postgres::connect(pg, NoTls)
        .await
        .map_err(|e| format!("PG connect: {e}"))?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    Ok(client)
}

type SqlVal = Box<dyn ToSql + Sync>;

/// Valores TIPEADOS para los parámetros (el prepared statement infiere el
/// tipo de la columna: bigint/int NO aceptan texto — error de serialización).
fn val_i64(v: i64) -> SqlVal { Box::new(v) }
fn val_i32(v: i32) -> SqlVal { Box::new(v) }
fn val_str(v: &str) -> SqlVal { Box::new(v.to_string()) }

/// Inserta filas en lotes multi-row (chunks de <= 30.000 parÃ¡metros â€” el
/// lÃ­mite prÃ¡ctico del protocolo PG). Valores como texto: PG coacciona los
/// parÃ¡metros unknown al tipo de la columna (patrÃ³n G-PG).
async fn insert_rows(
    client: &Client,
    table: &str,
    cols: &[&str],
    rows: &[Vec<SqlVal>],
) -> Result<u64, String> {
    const MAX_PARAMS: usize = 30_000;
    let ncols = cols.len();
    let col_list = cols.join(", ");
    let mut total = 0u64;
    let mut start = 0;
    while start < rows.len() {
        let mut end = start;
        let mut params = 0;
        while end < rows.len() && params + rows[end].len() <= MAX_PARAMS {
            params += rows[end].len();
            end += 1;
        }
        if end == start {
            end = start + 1; // fila Ãºnica mÃ¡s grande que el lÃ­mite (no ocurre)
        }
        let mut sql = format!("INSERT INTO {table} ({col_list}) VALUES ");
        for (i, _) in rows[start..end].iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push('(');
            for c in 0..ncols {
                if c > 0 {
                    sql.push_str(", ");
                }
                sql.push('$');
                // Los parámetros REINICIAN en $1 por chunk (cada chunk es su
                // propia sentencia): PG en modo fixedparams (protocolo
                // extendido) dimensiona el array de tipos hasta el número de
                // parámetro MÁS ALTO — si el chunk 2 empezara en $30001, los
                // huecos $1..$30000 quedarían sin tipo -> E42P18.
                sql.push_str(&((i * ncols + c + 1).to_string()));
            }
            sql.push(')');
        }
        let mut params_vec: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(params);
        for r in &rows[start..end] {
            params_vec.extend(r.iter().map(|v| v.as_ref()));
        }
        let stmt = client
            .prepare(&sql)
            .await
            .map_err(|e| format!("prepare insert {table}: {e:?} (sql len {})", sql.len()))?;
        total += client
            .execute(&stmt, &params_vec)
            .await
            .map_err(|e| format!("insert {table}: {e}"))?;
        start = end;
    }
    Ok(total)
}

async fn delete_lang(client: &Client, table: &str, lang: &str) -> Result<(), String> {
    client
        .execute(&format!("DELETE FROM {table} WHERE lang = $1"), &[&lang])
        .await
        .map_err(|e| format!("DELETE {table}: {e}"))?;
    Ok(())
}

async fn truncate(client: &Client, table: &str) -> Result<(), String> {
    client
        .execute(&format!("TRUNCATE {table}"), &[])
        .await
        .map_err(|e| format!("TRUNCATE {table}: {e}"))?;
    Ok(())
}

fn read(path: &str) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("{path}: {e}"))
}

/// import-mobs <lang>: common.mob_names desde el dump del pack.
async fn import_mobs(client: &Client, lang: &str, proto_dir: &str) -> Result<String, String> {
    let path = format!("{proto_dir}/{lang}/mob_names.txt");
    let rows = parse_names_dump(&read(&path)?, lang);
    delete_lang(client, "common.mob_names", lang).await?;
    let data: Vec<Vec<SqlVal>> = rows
        .iter()
        .map(|(v, n)| vec![val_i64(*v), val_str(lang), val_str(n)])
        .collect();
    let inserted = insert_rows(client, "common.mob_names", &["vnum", "lang", "name"], &data).await?;
    Ok(format!(
        "import-mobs {lang}: {inserted} nombres en common.mob_names (fuente: {path})"
    ))
}

/// import-items <lang>: common.item_names (dump) + item_descriptions
/// (itemdesc.txt col 1).
async fn import_items(
    client: &Client,
    lang: &str,
    proto_dir: &str,
    pack_locale_dir: &str,
) -> Result<String, String> {
    let names_path = format!("{proto_dir}/{lang}/item_names.txt");
    let names = parse_names_dump(&read(&names_path)?, lang);

    let desc_path = format!("{pack_locale_dir}/{lang}/itemdesc.txt");
    let descs = parse_itemdesc(&read(&desc_path)?, lang);

    delete_lang(client, "common.item_names", lang).await?;
    delete_lang(client, "common.item_descriptions", lang).await?;
    let data: Vec<Vec<SqlVal>> = names
        .iter()
        .map(|(v, n)| vec![val_i64(*v), val_str(lang), val_str(n)])
        .collect();
    let n_names =
        insert_rows(client, "common.item_names", &["vnum", "lang", "name"], &data).await?;
    let data: Vec<Vec<SqlVal>> = descs
        .iter()
        .map(|(v, d)| vec![val_i64(*v), val_str(lang), val_str(d)])
        .collect();
    let n_desc =
        insert_rows(client, "common.item_descriptions", &["vnum", "lang", "text"], &data).await?;
    Ok(format!(
        "import-items {lang}: {n_names} nombres (common.item_names) + {n_desc} descripciones (common.item_descriptions)"
    ))
}

/// import-skills <lang>: common.skill_names desde skilldesc.txt (col 2).
async fn import_skills(
    client: &Client,
    lang: &str,
    pack_locale_dir: &str,
) -> Result<String, String> {
    let path = format!("{pack_locale_dir}/{lang}/skilldesc.txt");
    let rows = parse_skilldesc(&read(&path)?, lang);
    delete_lang(client, "common.skill_names", lang).await?;
    let data: Vec<Vec<SqlVal>> = rows
        .iter()
        .map(|(id, n)| vec![val_i32(*id), val_str(lang), val_str(n)])
        .collect();
    let inserted =
        insert_rows(client, "common.skill_names", &["skill_id", "lang", "name"], &data).await?;
    Ok(format!(
        "import-skills {lang}: {inserted} habilidades en common.skill_names (fuente: {path})"
    ))
}

/// import-ui <lang>: common.ui_texts desde locale_interface.txt.
async fn import_ui(client: &Client, lang: &str, pack_locale_dir: &str) -> Result<String, String> {
    let path = format!("{pack_locale_dir}/{lang}/locale_interface.txt");
    let rows = parse_interface(&read(&path)?, lang);
    delete_lang(client, "common.ui_texts", lang).await?;
    let data: Vec<Vec<SqlVal>> = rows
        .iter()
        .map(|(k, v)| vec![val_str(k), val_str(lang), val_str(v)])
        .collect();
    let inserted = insert_rows(client, "common.ui_texts", &["key", "lang", "value"], &data).await?;
    Ok(format!(
        "import-ui {lang}: {inserted} claves en common.ui_texts (fuente: {path})"
    ))
}

/// import-messages: common.message_texts desde los 16 locale_string_XX.txt
/// del runtime (lang = XX en minÃºsculas). TRUNCATE del dominio completo.
async fn import_messages(client: &Client, locale_strings_dir: &str) -> Result<String, String> {
    truncate(client, "common.message_texts").await?;
    let mut data: Vec<Vec<SqlVal>> = Vec::new();
    let mut missing = Vec::new();
    for xx in MESSAGE_LANGS {
        let path = format!("{locale_strings_dir}/locale_string_{xx}.txt");
        let Ok(bytes) = read(&path) else {
            missing.push(xx);
            continue;
        };
        let lang = xx.to_ascii_lowercase();
        for (key, value) in parse_locale_string(&bytes) {
            data.push(vec![val_str(&decode_key(&key)), val_str(&lang), val_str(&decode_lang(&value, &lang))]);
        }
    }
    let inserted = insert_rows(client, "common.message_texts", &["key", "lang", "value"], &data).await?;
    let note = if missing.is_empty() {
        String::new()
    } else {
        format!(" (archivos ausentes: {})", missing.join(", "))
    };
    Ok(format!(
        "import-messages: {inserted} pares en common.message_texts (langs: {}){note}",
        MESSAGE_LANGS.len() - missing.len()
    ))
}

/// import-maps: world.maps desde index + Setting.txt (BasePosition) +
/// Town.txt (posSpawn = base + town*100) de TODOS los mapas del index.
async fn import_maps(client: &Client, map_path: &str) -> Result<String, String> {
    let index = parse_map_index(&read(&format!("{map_path}/index"))?);
    let mut data: Vec<Vec<SqlVal>> = Vec::new();
    let mut skipped = 0usize;
    for (map_id, name) in &index {
        let dir = format!("{map_path}/{name}");
        let Some(base) = parse_setting_base(&read(&format!("{dir}/Setting.txt"))?) else {
            eprintln!("import-maps: {dir}/Setting.txt sin BasePosition â€” mapa omitido");
            skipped += 1;
            continue;
        };
        let town = read(&format!("{dir}/Town.txt")).ok();
        let spawn = parse_town_spawn(town.as_deref(), base);
        data.push(vec![val_i32(*map_id), val_str(name), val_i32(base.0), val_i32(base.1), val_i32(spawn.0), val_i32(spawn.1)]);
    }
    truncate(client, "world.maps").await?;
    let inserted =
        insert_rows(client, "world.maps", &["map_id", "name", "base_x", "base_y", "spawn_x", "spawn_y"], &data)
            .await?;
    Ok(format!(
        "import-maps: {inserted} mapas en world.maps (index: {}; omitidos: {skipped})",
        index.len()
    ))
}

/// import-spawns: world.spawns desde el runtime para TODOS los mapas del
/// index, reutilizando el parser verificado `game_core::npc::load_map_spawns`
/// (expansiÃ³n de grupos incluida â€” el resultado solo contiene Mob/Anywhere).
async fn import_spawns(client: &Client, map_path: &str) -> Result<String, String> {
    let index = parse_map_index(&read(&format!("{map_path}/index"))?);
    let mut data: Vec<Vec<SqlVal>> = Vec::new();
    let mut skipped = Vec::new();
    let mut total_mobs: u64 = 0;
    for (map_id, name) in &index {
        match load_map_spawns(*map_id as u32, map_path) {
            Ok(entries) => {
                total_mobs += entries.iter().map(|e| e.count as u64).sum::<u64>();
                for e in &entries {
                    data.push(spawn_row(*map_id, e));
                }
            }
            Err(err) => {
                eprintln!("import-spawns: mapa {map_id} ({name}) omitido: {err}");
                skipped.push(name.clone());
            }
        }
    }
    truncate(client, "world.spawns").await?;
    let inserted =
        insert_rows(client, "world.spawns", &["map_id", "vnum", "x", "y", "count", "kind"], &data)
            .await?;
    let note = if skipped.is_empty() {
        String::new()
    } else {
        format!("; omitidos: {}", skipped.join(", "))
    };
    Ok(format!(
        "import-spawns: {inserted} entradas en world.spawns (Î£ count {total_mobs} mobs, {} mapas del index){note}",
        index.len()
    ))
}

/// Fila de world.spawns para una entrada EXPANDIDA (kind Mob/Anywhere â€” la
/// expansiÃ³n nunca emite grupos, doc game_core::npc).
fn spawn_row(map_id: i32, e: &SpawnEntry) -> Vec<SqlVal> {
    let kind = match e.kind {
        SpawnKind::Mob => "mob",
        SpawnKind::Anywhere => "anywhere",
        // Inalcanzable tras la expansiÃ³n (guarda defensiva).
        SpawnKind::Group | SpawnKind::GroupGroup => "group",
    };
    vec![
        val_i32(map_id),
        val_i64(i64::from(e.vnum)),
        val_i32(e.x),
        val_i32(e.y),
        val_i32(e.count as i32),
        val_str(kind),
    ]
}

#[cfg(test)]
mod pg_tests {
    use super::*;

    /// La cadena PG se puede sobreescribir con DATABASE_TEST_PG (patrón
    /// channel_pg.rs).
    fn pg_conn() -> String {
        std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
    }

    /// Integración contra la PG REAL de WSL — gated con `#[ignore]` (patrón
    /// channel_pg.rs): import-mobs es re-importa el dominio y verifica el
    /// conteo real del dump ES (2876 filas, verificado 2026-08-12).
    #[tokio::test]
    #[ignore = "requiere la PG de WSL (host=127.0.0.1:5432, bd metin2)"]
    async fn import_mobs_es_live_pg() {
        let client = connect(&pg_conn()).await.expect("connect");
        let summary = import_mobs(&client, "es", DEFAULT_PROTO_DIR).await.expect("import");
        eprintln!("{summary}");
        let n: i64 = client
            .query_one("SELECT count(*) FROM common.mob_names WHERE lang = 'es'", &[])
            .await
            .expect("count")
            .get(0);
        assert_eq!(n, 2876, "dump ES completo (2876 mobs, verificado 2026-08-12)");
    }

    /// Parity del spawn del mapa 41 contra el runtime: Σ count = 23.033 —
    /// el MISMO número del test F5 map41_spawns.rs (23033 mobs individuales).
    #[tokio::test]
    #[ignore = "requiere la PG de WSL + el runtime WSL (share/locale/spain/map)"]
    async fn spawns_map41_live_pg() {
        let client = connect(&pg_conn()).await.expect("connect");
        let summary = import_spawns(&client, DEFAULT_MAP_PATH).await.expect("import");
        eprintln!("{summary}");
        let n: i64 = client
            .query_one("SELECT sum(count) FROM world.spawns WHERE map_id = 41", &[])
            .await
            .expect("sum")
            .get(0);
        assert_eq!(n, 23_033, "fauna expandida del mapa 41 (parity F5, map41_spawns.rs)");
    }
}

#[tokio::main]
async fn main() {
    let opts = parse_opts();
    let client = match connect(&opts.pg).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let lang = || opts.lang.clone().unwrap_or_else(|| usage());
    let result = match opts.sub.as_str() {
        "import-mobs" => import_mobs(&client, &lang(), &opts.proto_dir).await,
        "import-items" => import_items(&client, &lang(), &opts.proto_dir, &opts.pack_locale_dir).await,
        "import-skills" => import_skills(&client, &lang(), &opts.pack_locale_dir).await,
        "import-ui" => import_ui(&client, &lang(), &opts.pack_locale_dir).await,
        "import-messages" => import_messages(&client, &opts.locale_strings_dir).await,
        "import-maps" => import_maps(&client, &opts.map_path).await,
        "import-spawns" => import_spawns(&client, &opts.map_path).await,
        other => {
            eprintln!("subcomando desconocido: {other}");
            usage();
        }
    };
    match result {
        Ok(summary) => println!("OK: {summary}"),
        Err(e) => {
            eprintln!("ERROR: {e}");
            std::process::exit(1);
        }
    }
}

