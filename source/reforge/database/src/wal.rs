//! F3 phase 2 (ADR-0008): pipeline WAL local — `mutation_id` uuidv7 + batcher
//! durable (batch <=100ms -> UNA transaccion -> replay idempotente + audit).
//!
//! Contrato durable del ADR-0008: durable = batch transaccional <=100ms; el
//! replay idempotente (`ON CONFLICT DO NOTHING`) garantiza que re-aplicar una
//! mutation (crash entre commit local y central) no duplica estado.
//!
//! El DDL del audit es una CONST exportada (no se aplica aqui — el lane del
//! harness lo aplica a la PG viva; los tests de integracion usan un schema
//! `e2e_wal` temporal y lo limpian).

use std::error::Error;
use std::fmt;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;
use tokio_postgres::types::{IsNull, ToSql, Type};

/// `bytes::BytesMut` re-exportado por tokio-postgres (ruta del trait ToSql).
use tokio_postgres::types::private::BytesMut;

use crate::pool::{Client, PgPool};

// ---------------------------------------------------------------------------
// uuidv7 (RFC 9562): 48-bit ms timestamp | version 7 | rand_a | variant + rand
// ---------------------------------------------------------------------------

/// `[0..6]` timestamp ms big-endian, byte 6 = version 7 | rand_a high,
/// byte 8 = variant `10xx` — comparacion lexicografica de los bytes == orden
/// cronologico (los 6 primeros bytes dominan).
fn uuidv7_from(ts_ms: u64, rand: u64) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0..6].copy_from_slice(&ts_ms.to_be_bytes()[2..]); // 48 bits
    let r0 = (rand >> 40) as u8;
    let r1 = (rand >> 32) as u8;
    b[6] = 0x70 | (r0 >> 4); // version 7
    b[7] = (r0 << 4) | (r1 >> 4);
    b[8] = 0x80 | (r1 & 0x0f); // variant 10xx
    b[9..].copy_from_slice(&rand.to_be_bytes()[1..]); // 7 bytes de rand
    b
}

/// Rand determinista sin dependencias: contador + nanos mezclados (patron del
/// nonce del handshake y de `unique_login_key` del auth).
fn rand64() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    nanos ^ c.wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// uuidv7 nuevo (timestamp actual + rand).
pub fn uuidv7() -> [u8; 16] {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    uuidv7_from(ts, rand64())
}

/// uuidv7 -> string canónico `8-4-4-4-12` (lowercase hex).
pub fn uuidv7_string(u: &[u8; 16]) -> String {
    let h: String = u.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

// ---------------------------------------------------------------------------
// Mutation + Param
// ---------------------------------------------------------------------------

/// Parametro tipado de una mutation (ToSql delegado + payload jsonb simple).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Param {
    Text(String),
    Int(i64),
    Bytes(Vec<u8>),
    /// SQL NULL (necesario en el write path: `player.skill_level`/`quickslot`
    /// son bytea nullable y el save del C++ puede escribir NULL).
    Null,
}

impl ToSql for Param {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        match self {
            Param::Text(s) => s.to_sql(ty, out),
            // OJO: i64 solo implementa ToSql para INT8 en postgres-types 0.2
            // (`simple_to!(i64, int8_to_sql, INT8)`) — enviar 8 bytes a una
            // columna int2/int4 da 22P03 en el server. Aqui codificamos por
            // el tipo destino (parity de las columnas mixtas del save).
            Param::Int(i) => match *ty {
                Type::INT2 => {
                    out.extend_from_slice(&(*i as i16).to_be_bytes());
                    Ok(IsNull::No)
                }
                Type::INT4 => {
                    out.extend_from_slice(&(*i as i32).to_be_bytes());
                    Ok(IsNull::No)
                }
                Type::INT8 => {
                    out.extend_from_slice(&i.to_be_bytes());
                    Ok(IsNull::No)
                }
                _ => i.to_sql(ty, out),
            },
            Param::Bytes(b) => b.to_sql(ty, out),
            Param::Null => Ok(IsNull::Yes),
        }
    }

    fn to_sql_checked(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        match self {
            Param::Null => Ok(IsNull::Yes),
            _ => self.to_sql(ty, out),
        }
    }

    fn accepts(ty: &Type) -> bool {
        String::accepts(ty)
            || i64::accepts(ty)
            || i32::accepts(ty)
            || i16::accepts(ty)
            || Vec::<u8>::accepts(ty)
    }
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Param::Text(s) => write!(
                f,
                "\"{}\"",
                s.replace('\\', "\\\\").replace('"', "\\\"")
            ),
            Param::Int(i) => write!(f, "{i}"),
            Param::Bytes(b) => write!(
                f,
                // `\\x` ESCAPADO (json válido): el parser inverso desescapa
                // `\\` -> `\` y reconoce el prefijo `\x` + hex como Bytes.
                "\"\\\\x{}\"",
                b.iter().map(|x| format!("{x:02x}")).collect::<String>()
            ),
            Param::Null => write!(f, "null"),
        }
    }
}

/// Mutation durable: sql idempotente (`ON CONFLICT DO NOTHING`) + params.
/// El `id` es el `mutation_id` que tambien se escribe en el audit (misma tx).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutation {
    pub id: [u8; 16],
    pub sql: String,
    pub params: Vec<Param>,
}

impl Mutation {
    /// Mutation nueva con uuidv7 propio.
    pub fn new(sql: impl Into<String>, params: Vec<Param>) -> Self {
        Self { id: uuidv7(), sql: sql.into(), params }
    }

    /// Mutation con id fijo (tests de replay idempotente).
    pub fn with_id(id: [u8; 16], sql: impl Into<String>, params: Vec<Param>) -> Self {
        Self { id, sql: sql.into(), params }
    }

    /// Payload jsonb del audit (string json valido, sin serde).
    pub fn payload_json(&self) -> String {
        let params: Vec<String> = self.params.iter().map(|p| p.to_string()).collect();
        format!(
            "{{\"mutation_id\":\"{}\",\"sql\":{},\"params\":[{}]}}",
            uuidv7_string(&self.id),
            serde_json_lite_escape(&self.sql),
            params.join(",")
        )
    }
}

/// Escapado minimo de string a json (comillas y backslash).
fn serde_json_lite_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Audit (DDL exportado — NO se aplica aqui)
// ---------------------------------------------------------------------------

/// DDL del audit durable (misma tx que las mutations). La tabla vive en el
/// schema `log`; el harness de otro lane la aplica a la PG viva.
/// `payload` es TEXT (no jsonb): el plan §5.5 pide "append-only audit" sin
/// exigir jsonb, y el texto evita la dependencia serde_json; el payload es
/// un json valido (ver `Mutation::payload_json`) que el consumidor parsea.
pub const AUDIT_DDL: &str = "\
CREATE TABLE IF NOT EXISTS log.mutation_audit (\
mutation_id uuid PRIMARY KEY, \
applied_at timestamptz NOT NULL DEFAULT now(), \
payload text NOT NULL)";

/// Version parametrizable del DDL (tests: schema `e2e_wal` temporal).
pub fn audit_ddl(table: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {table} (\
mutation_id uuid PRIMARY KEY, \
applied_at timestamptz NOT NULL DEFAULT now(), \
payload text NOT NULL)"
    )
}

// ---------------------------------------------------------------------------
// Sink + Batcher
// ---------------------------------------------------------------------------

/// Aplicador de un batch: UNA transaccion para todo el batch (commit o
/// rollback total). El impl real es `PgMutationSink`; los tests inyectan un
/// sink contador (sin PG). RPITIT con `+ Send` explicito para que el worker
/// del Batcher (tokio::spawn) pueda esperar el future.
pub trait MutationSink: Send + 'static {
    fn apply(&mut self, batch: Vec<Mutation>) -> impl std::future::Future<Output = Result<(), String>> + Send;
}

/// Sink real: conexion PG por batch, transaccion, replay idempotente + audit
/// en la MISMA tx (ADR-0008: durable = batch transaccional <=100ms). El
/// pool es el del proceso (una conexion por batch ya NO - el cuello del
/// entry 2026-08-13 era el connect() por llamada).
pub struct PgMutationSink {
    pool: PgPool,
    /// Tabla de audit (default `log.mutation_audit`; los tests usan `e2e_wal.*`).
    audit_table: String,
}

impl PgMutationSink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, audit_table: "log.mutation_audit".to_string() }
    }

    pub fn with_audit_table(mut self, audit_table: impl Into<String>) -> Self {
        self.audit_table = audit_table.into();
        self
    }

    async fn connect(&self) -> Result<Client, String> {
        self.pool.get().await.map_err(|e| format!("PG pool get: {e}"))
    }
}

impl MutationSink for PgMutationSink {
    fn apply(&mut self, batch: Vec<Mutation>) -> impl std::future::Future<Output = Result<(), String>> + Send {
        async move {
            let mut client = self.connect().await?;
            let tx = client
                .transaction()
                .await
                .map_err(|e| format!("BEGIN: {e}"))?;
            for m in &batch {
                let params: Vec<&(dyn ToSql + Sync)> =
                    m.params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();
                tx.execute(&m.sql, &params)
                    .await
                    .map_err(|e| {
                        // Display corto ("db error") — el mensaje real del
                        // servidor vive en el DbError (mismo patron del audit).
                        let detail = e
                            .as_db_error()
                            .map(|d| d.message().to_string())
                            .unwrap_or_default();
                        format!("mutation {}: {e} ({detail})", uuidv7_string(&m.id))
                    })?;
                // Audit en la MISMA tx; replay idempotente: si el mutation_id
                // ya esta (re-aplicacion), el insert no hace nada.
                // $1 viaja como uuid nativo (feature with-uuid-1); $2 es text
                // (payload TEXT — ver AUDIT_DDL).
                let audit_sql = format!(
                    "INSERT INTO {} (mutation_id, payload) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                    self.audit_table
                );
                let id_uuid = uuid::Uuid::from_bytes(m.id);
                tx.execute(&audit_sql, &[&id_uuid, &m.payload_json()])
                    .await
                    .map_err(|e| {
                        // Display de tokio-postgres es corto ("db error") — el
                        // mensaje real del servidor vive en el DbError.
                        let detail = e
                            .as_db_error()
                            .map(|d| d.message().to_string())
                            .unwrap_or_default();
                        format!("audit {}: {e} ({detail})", uuidv7_string(&m.id))
                    })?;
            }
            tx.commit().await.map_err(|e| format!("COMMIT: {e}"))?;
            Ok(())
        }
    }
}

/// Mensaje interno del worker del Batcher: mutation normal o flush forzado
/// con ack (el ack recibe el resultado de la tx del batch).
enum Msg {
    M(Mutation),
    Flush(tokio::sync::oneshot::Sender<Result<(), String>>),
}

/// Batcher: recoge mutations y las flushea en batches — flush por tamaño
/// (`max_batch`) o por tiempo (`flush_interval` desde la PRIMERA mutation del
/// batch, <=100ms en produccion). El worker es una task tokio; el sender se
/// clona para pushear desde cualquier task. `flush()` fuerza el cierre del
/// batch actual en UNA transaccion y espera el commit (unidad ACID).
pub struct Batcher {
    tx: mpsc::UnboundedSender<Msg>,
}

impl Batcher {
    /// Arranca el worker; `sink` aplica los batches (UNA transaccion cada uno).
    pub fn spawn(flush_interval: Duration, max_batch: usize, sink: impl MutationSink) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
        tokio::spawn(async move {
            let mut sink = sink;
            loop {
                // Primera mutation del batch: espera indefinida (el canal se
                // cierra al dropear todos los senders -> fin del worker).
                let Some(first) = rx.recv().await else { break };
                let mut batch = Vec::new();
                let mut flush_ack: Option<tokio::sync::oneshot::Sender<Result<(), String>>> = None;
                match first {
                    Msg::M(m) => batch.push(m),
                    // Flush sin batch pendiente: no-op con ack Ok.
                    Msg::Flush(ack) => {
                        let _ = ack.send(Ok(()));
                        continue;
                    }
                }
                // Acumula hasta max_batch, hasta que pase flush_interval, o
                // hasta un flush explicito (el batch se cierra YA).
                while batch.len() < max_batch {
                    match tokio::time::timeout(flush_interval, rx.recv()).await {
                        Ok(Some(Msg::M(m))) => batch.push(m),
                        Ok(Some(Msg::Flush(ack))) => {
                            flush_ack = Some(ack);
                            break;
                        }
                        _ => break, // timeout o canal cerrado -> flush ya
                    }
                }
                // UNA transaccion para todo el batch. El flush explicito
                // recibe el resultado (Ok = commit; Err = el WAL local lo
                // re-aplicara al arrancar).
                let result = sink.apply(batch).await;
                if let Err(e) = &result {
                    eprintln!("database: wal: batch falló: {e} — el WAL local lo re-aplicará al arrancar");
                }
                if let Some(ack) = flush_ack {
                    let _ = ack.send(result);
                }
            }
        });
        Self { tx }
    }

    /// Push de una mutation (unbounded: nunca bloquea al caller).
    pub fn push(&self, m: Mutation) {
        let _ = self.tx.send(Msg::M(m));
    }

    /// Flush explicito: cierra el batch actual (todo lo pusheado ANTES de
    /// esta llamada) y espera el commit — UNA transaccion + audit. Devuelve
    /// `Ok` si el sink confirmo; `Err` si el batch fallo (el WAL local
    /// conserva el archivo para el replay del proximo arranque, ADR-0008).
    ///
    /// Contrato de la unidad ACID (ADR-0011 "items as ACID units"): el caller
    /// pushea TODAS las mutations de la unidad y llama a `flush()` sin pausas
    /// mayores a `flush_interval` entre pushes y con la unidad por debajo de
    /// `max_batch` — entonces la unidad entera commit en una sola tx (o no
    /// commit nada).
    pub async fn flush(&self) -> Result<(), String> {
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        let _ = self.tx.send(Msg::Flush(ack_tx));
        ack_rx.await.map_err(|e| format!("batcher worker caido: {e}"))?
    }
}

// ---------------------------------------------------------------------------
// WAL local a disco (F3 phase 2 — ADR-0008): durable-first por batch
// ---------------------------------------------------------------------------

/// Envelope durable-first: persiste el batch en disco ANTES de aplicarlo a PG
/// y borra el archivo SOLO tras el COMMIT. Si el proceso muere a mitad de
/// batch, el archivo queda en disco y el replay del siguiente arranque lo
/// re-aplica (idempotente — ver la auditoría abajo).
///
/// # Auditoría de idempotencia (qué se re-aplica sin duplicar estado)
///
/// Todas las rutas cableadas al Batcher hoy son idempotentes en resultado
/// (re-aplicar no duplica estado):
/// - `PlayerRepo::save_mutated` (`PLAYER_SAVE_SQL`): UPDATE por PK — el
///   resultado es idéntico; `last_play = NOW()` se re-escribe (inofensivo,
///   documentado — "idempotente salvo columnas NOW()").
/// - `ItemRepo::upsert`: `ON CONFLICT (id) DO UPDATE` — idempotente.
/// - `QuestRepo::save`: `ON CONFLICT (dwPID, szName, szState)` + DELETE —
///   idempotente.
/// - `AffectRepo::save/remove`: `ON CONFLICT (dwPID, bType, bApplyOn,
///   lApplyValue)` + DELETE — idempotente.
/// - `ItemRepo::take_award`: UPDATE con `taken_time = NOW()` — idempotente
///   salvo la columna NOW().
/// - `SafeboxRepo::set_size` (`set_size_mutated`): rama `size == 1` INSERT
///   con `ON CONFLICT (account_id) DO NOTHING` (la PK); rama UPDATE por PK —
///   idempotente. El quirk legacy "size==1 -> INSERT" se conserva.
/// - `MessengerRepo::add` (`add_mutated`): INSERT con
///   `ON CONFLICT (account, companion) DO NOTHING` (la PK natural) —
///   idempotente (un par repetido devuelve 0 en vez de 23505).
///
/// Los dos INSERTs que fueron un gap documentado (ver ADR-0011/ROADMAP) —
/// `SafeboxRepo::set_size` (rama `size == 1`) y `MessengerRepo::add` — ya
/// están cableados con `ON CONFLICT DO NOTHING` sobre su PK natural. La regla
/// para cablear una ruta nueva al Batcher: el SQL debe ser idempotente bajo
/// re-aplicación (upsert por PK o conflict target), o no se cablea.
pub struct WalSink<S: MutationSink> {
    inner: S,
    wal_dir: String,
}

impl<S: MutationSink> WalSink<S> {
    pub fn new(inner: S, wal_dir: impl Into<String>) -> Self {
        Self { inner, wal_dir: wal_dir.into() }
    }
}

impl<S: MutationSink> MutationSink for WalSink<S> {
    fn apply(&mut self, batch: Vec<Mutation>) -> impl std::future::Future<Output = Result<(), String>> + Send {
        async move {
            // 1. Durable-first: persiste el batch completo ANTES de tocar PG.
            let file = persist_batch(&self.wal_dir, &batch)
                .map_err(|e| format!("persistiendo WAL en {}: {e}", self.wal_dir))?;
            // 2. Aplica (UNA tx + audit — el sink interno).
            match self.inner.apply(batch).await {
                Ok(()) => {
                    // 3. Truncate SOLO post-COMMIT: el archivo ya está en PG.
                    if let Err(e) = std::fs::remove_file(&file) {
                        eprintln!("database: wal: no pude borrar {file:?} tras el commit: {e}");
                    }
                    Ok(())
                }
                Err(e) => {
                    // El archivo QUEDA en disco: el replay del siguiente
                    // arranque lo re-aplica (idempotente).
                    eprintln!("database: wal: batch con error — {file:?} queda para el replay: {e}");
                    Err(e)
                }
            }
        }
    }
}

/// Persiste un batch en un archivo nuevo `{wal_dir}/{uuidv7}.wal` (JSONL —
/// una `payload_json` por línea) + `sync_all` (garantía durable real: sin
/// fsync el OS puede perder el archivo en un crash).
fn persist_batch(wal_dir: &str, batch: &[Mutation]) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(wal_dir)?;
    let name = format!("{}.wal", uuidv7_string(&uuidv7()));
    let path = std::path::Path::new(wal_dir).join(name);
    let mut f = std::fs::File::create(&path)?;
    for m in batch {
        writeln!(f, "{}", m.payload_json())?;
    }
    f.sync_all()?;
    Ok(path)
}

/// Replay del WAL local: re-aplica cada archivo `*.wal` del directorio (en
/// orden cronológico — el uuidv7 del nombre ordena) como UN batch (una tx +
/// audit), y borra el archivo SOLO tras el commit. Devuelve cuántos archivos
/// se re-aplicaron. Función pura: los tests la invocan contra un dir temporal;
/// en producción la invoca `WorldStore::new` UNA vez por proceso (OnceLock).
pub async fn replay_wal(wal_dir: &str, pool: &PgPool) -> Result<usize, String> {
    let dir = std::path::Path::new(wal_dir);
    if !dir.is_dir() {
        return Ok(0); // sin WAL previo — nada que re-aplicar
    }
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("leyendo {wal_dir}: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "wal").unwrap_or(false))
        .collect();
    files.sort(); // uuidv7 cronológico
    let mut sink = PgMutationSink::new(pool.clone());
    let mut replayed = 0usize;
    for path in files {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("leyendo {path:?}: {e}"))?;
        let mut batch = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            batch.push(
                parse_payload_json(line)
                    .map_err(|e| format!("parseando {path:?}: {e}"))?,
            );
        }
        sink.apply(batch).await.map_err(|e| format!("replay de {path:?}: {e}"))?;
        std::fs::remove_file(&path).map_err(|e| format!("borrando {path:?}: {e}"))?;
        replayed += 1;
    }
    Ok(replayed)
}

/// Parse inverso de una línea `payload_json` -> `Mutation` (el formato es
/// cerrado y propio — ver `Mutation::payload_json`; sin serde, std only).
fn parse_payload_json(line: &str) -> Result<Mutation, String> {
    // {"mutation_id":"...","sql":"...","params":[...]}
    let mut rest = line.trim();
    rest = rest.strip_prefix('{').ok_or("sin {")?.trim_start();
    let mut id: Option<[u8; 16]> = None;
    let mut sql: Option<String> = None;
    let mut params: Option<Vec<Param>> = None;
    while !rest.is_empty() {
        let key = parse_json_string(rest).map_err(|e| format!("key: {e}"))?;
        rest = rest[key.1..].trim_start();
        rest = rest.strip_prefix(':').ok_or("sin : tras key")?.trim_start();
        match key.0.as_str() {
            "mutation_id" => {
                let (v, n) = parse_json_string(rest).map_err(|e| format!("mutation_id: {e}"))?;
                id = Some(parse_uuid(&v)?);
                rest = rest[n..].trim_start();
            }
            "sql" => {
                let (v, n) = parse_json_string(rest).map_err(|e| format!("sql: {e}"))?;
                sql = Some(v);
                rest = rest[n..].trim_start();
            }
            "params" => {
                let (v, n) = parse_json_array(rest).map_err(|e| format!("params: {e}"))?;
                params = Some(v);
                rest = rest[n..].trim_start();
            }
            _ => {
                // Clave desconocida (formato futuro): saltar el valor.
                let (_, n) = parse_json_value(rest).map_err(|e| format!("valor: {e}"))?;
                rest = rest[n..].trim_start();
            }
        }
        rest = rest.strip_prefix(',').map(|r| r.trim_start()).unwrap_or(rest);
        if rest.starts_with('}') {
            break;
        }
    }
    let id = id.ok_or("sin mutation_id")?;
    let sql = sql.ok_or("sin sql")?;
    let params = params.unwrap_or_default();
    Ok(Mutation { id, sql, params })
}

/// `"..."` con escapes `\"` y `\\` -> (valor sin desescapar, bytes consumidos).
fn parse_json_string(s: &str) -> Result<(String, usize), String> {
    let b = s.as_bytes();
    if b.first() != Some(&b'"') {
        return Err(format!("esperaba string, vi {:?}", s.chars().next()));
    }
    let mut out = String::new();
    let mut i = 1;
    while i < b.len() {
        match b[i] {
            b'"' => return Ok((out, i + 1)),
            b'\\' => {
                let nxt = *b.get(i + 1).ok_or("string sin cerrar")?;
                match nxt {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    other => return Err(format!("escape desconocido \\{other}")),
                }
                i += 2;
            }
            c => {
                // El resto del archivo es UTF-8 (los sql/params/keys del
                // payload); copiamos bytes crudos (los no-ASCII pasan tal cual).
                let ch_len = utf8_len(c);
                let end = (i + ch_len).min(b.len());
                out.push_str(&s[i..end]);
                i = end;
            }
        }
    }
    Err("string sin cerrar".into())
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// `[...]` de params — cada elemento es el Display de `Param`:
/// string json, número, `"\x.."` (Bytes) o `null`.
fn parse_json_array(s: &str) -> Result<(Vec<Param>, usize), String> {
    let mut rest = s.trim_start();
    rest = rest.strip_prefix('[').ok_or("sin [")?.trim_start();
    let mut out = Vec::new();
    if rest.starts_with(']') {
        return Ok((out, s.len() - rest.len() + 1));
    }
    loop {
        let (p, n) = parse_json_param(rest)?;
        out.push(p);
        rest = rest[n..].trim_start();
        if rest.starts_with(']') {
            return Ok((out, s.len() - rest.len() + 1));
        }
        rest = rest.strip_prefix(',').ok_or("sin , en array")?.trim_start();
    }
}

fn parse_json_param(s: &str) -> Result<(Param, usize), String> {
    let trimmed = s.trim_start();
    let off = s.len() - trimmed.len();
    if trimmed.starts_with("null") {
        return Ok((Param::Null, off + 4));
    }
    if trimmed.starts_with('"') {
        let (v, n) = parse_json_string(trimmed)?;
        // Bytes: el Display produce "\xHEX" — distinguir del Text escapado.
        if let Some(hex) = v.strip_prefix("\\x")
            && hex.len() % 2 == 0 && !hex.is_empty() && hex.bytes().all(|c| c.is_ascii_hexdigit()) {
                let bytes = (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex valido"))
                    .collect();
                return Ok((Param::Bytes(bytes), off + n));
            }
        return Ok((Param::Text(v), off + n));
    }
    // Número (Display de Int: i64 con signo).
    let end = trimmed
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(trimmed.len());
    let num = trimmed[..end]
        .parse::<i64>()
        .map_err(|e| format!("param numérico inválido: {e}"))?;
    Ok((Param::Int(num), off + end))
}

/// Valor json cualquiera (para saltar claves desconocidas del formato).
fn parse_json_value(s: &str) -> Result<(String, usize), String> {
    let trimmed = s.trim_start();
    let off = s.len() - trimmed.len();
    if trimmed.starts_with('"') {
        let (v, n) = parse_json_string(trimmed)?;
        Ok((v, off + n))
    } else if trimmed.starts_with('[') {
        let (_, n) = parse_json_array(trimmed)?;
        Ok((String::new(), off + n))
    } else if trimmed.starts_with("null") {
        Ok((String::new(), off + 4))
    } else {
        // número u otro token simple: hasta la coma o llave.
        let end = trimmed
            .find([',', '}'])
            .unwrap_or(trimmed.len());
        Ok((trimmed[..end].to_string(), off + end))
    }
}

/// `8-4-4-4-12` (lowercase) -> bytes del uuid.
fn parse_uuid(s: &str) -> Result<[u8; 16], String> {
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("uuid inválido: {s}"));
    }
    let mut out = [0u8; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| format!("uuid: {s}"))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    // ------------------------------------------------------------------ uuidv7

    #[test]
    fn uuidv7_format_version_and_variant() {
        let u = uuidv7();
        let s = uuidv7_string(&u);
        let mut parts = s.split('-');
        assert_eq!(parts.next().unwrap().len(), 8);
        assert_eq!(parts.next().unwrap().len(), 4);
        assert_eq!(parts.next().unwrap().len(), 4);
        assert_eq!(parts.next().unwrap().len(), 4);
        assert_eq!(parts.next().unwrap().len(), 12);
        // version 7 en el 3er grupo (primer hex = 7).
        assert!(s[14..15].starts_with('7'), "version 7: {s}");
        // variant 10xx en el 4o grupo (8,9,a,b).
        assert!(matches!(s[19..20].as_bytes()[0], b'8' | b'9' | b'a' | b'b'), "variant: {s}");
        assert!(s.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-'));
    }

    #[test]
    fn uuidv7_unique_10000() {
        let mut set = HashSet::with_capacity(10_000);
        for _ in 0..10_000 {
            set.insert(uuidv7());
        }
        assert_eq!(set.len(), 10_000, "sin colisiones");
    }

    #[test]
    fn uuidv7_chronological_order() {
        let a = uuidv7_from(1000, 0xDEADBEEF);
        let b = uuidv7_from(2000, 0xDEADBEEF);
        assert!(a < b, "bytes lexicograficos: ts menor -> uuid menor");
        assert!(uuidv7_string(&a) < uuidv7_string(&b), "string canónico también ordena");
        // Mismo ts, rand distinto: el orden NO es estricto (solo rand) — solo
        // se garantiza que version/variant se mantienen.
        let c = uuidv7_from(1000, 0x1111);
        let d = uuidv7_from(1000, 0x2222);
        assert_ne!(c, d);
        assert_eq!(c[6] >> 4, 7);
        assert_eq!(d[6] >> 4, 7);
    }

    #[test]
    fn uuidv7_timestamp_is_48bit_ms() {
        // bytes 0..6 = ts big-endian; ts=0x0000_0102_0304 -> 00 00 01 02 03 04
        let u = uuidv7_from(0x0000_0102_0304, 0);
        assert_eq!(&u[0..6], &[0x00, 0x00, 0x01, 0x02, 0x03, 0x04], "48-bit be");
        assert_eq!(u[0], 0x00);
        assert_eq!(u[5], 0x04);
    }

    // ------------------------------------------------------------------ params/payload

    #[test]
    fn mutation_payload_json_is_valid_shape() {
        let m = Mutation::new(
            "INSERT INTO t (id, v) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            vec![Param::Int(7), Param::Text("a\"b".into())],
        );
        let p = m.payload_json();
        assert!(p.starts_with("{\"mutation_id\":\""), "json: {p}");
        assert!(p.contains("\"sql\":\"INSERT INTO t (id, v)"), "sql escapado: {p}");
        assert!(p.contains("\\\""), "comillas escapadas: {p}");
        assert!(p.ends_with('}'));
        // El id del payload == el id de la mutation (misma string).
        assert!(p.contains(&uuidv7_string(&m.id)), "mutation_id en payload");
    }

    #[test]
    fn param_null_roundtrip() {
        // Display -> "null" (json valido); to_sql_checked -> IsNull::Yes.
        let m = Mutation::new(
            "INSERT INTO t (a, b) VALUES ($1, $2)",
            vec![Param::Null, Param::Bytes(vec![0x01, 0x00])],
        );
        assert!(m.payload_json().contains("null"), "payload: {}", m.payload_json());
        let mut out = BytesMut::new();
        assert!(matches!(
            Param::Null.to_sql_checked(&Type::INT4, &mut out).expect("to_sql"),
            IsNull::Yes
        ));
        assert!(out.is_empty(), "NULL no escribe bytes");
    }

    /// Param::Int codifica por el tipo destino: int2 (2 bytes), int4 (4),
    /// int8 (8) — el fix del 22P03 (i64 solo ToSql para INT8 en postgres-types
    /// 0.2; las columnas del save son smallint/integer/bigint mezcladas).
    #[test]
    fn param_int_encodes_by_target_type() {
        let mut out = BytesMut::new();
        Param::Int(5).to_sql(&Type::INT2, &mut out).expect("int2");
        assert_eq!(&out[..], &[0x00, 0x05], "int2 2 bytes");
        let mut out = BytesMut::new();
        Param::Int(5).to_sql(&Type::INT4, &mut out).expect("int4");
        assert_eq!(&out[..], &[0x00, 0x00, 0x00, 0x05], "int4 4 bytes");
        let mut out = BytesMut::new();
        Param::Int(5).to_sql(&Type::INT8, &mut out).expect("int8");
        assert_eq!(&out[..], &[0, 0, 0, 0, 0, 0, 0, 5], "int8 8 bytes");
        // Negativos (i16/i32 truncado) preservan el signo.
        let mut out = BytesMut::new();
        Param::Int(-1).to_sql(&Type::INT2, &mut out).expect("int2 neg");
        assert_eq!(&out[..], &[0xff, 0xff], "int2 -1");
    }

    // ------------------------------------------------------------------ batcher

    /// Sink contador (sin PG): registra los batches aplicados.
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

    fn batches(sink: &CountingSink) -> usize {
        sink.0.lock().unwrap().len()
    }

    /// Espera (yields) hasta que el worker haya aplicado `n` batches — con el
    /// reloj pausado, yield_now deja que el worker avance hasta bloquearse.
    async fn wait_for_batches(sink: &CountingSink, n: usize) {
        for _ in 0..200 {
            if batches(sink) >= n {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("timeout esperando {n} batches (tiene {})", batches(sink));
    }

    #[tokio::test(start_paused = true)]
    async fn flush_fires_within_interval_after_first_mutation() {
        let sink = CountingSink::default();
        let batcher = Batcher::spawn(Duration::from_millis(100), 64, sink.clone());
        batcher.push(Mutation::new("SELECT 1", vec![]));
        // El worker arranca su ventana de 100ms al recibir la primera mutation.
        tokio::time::advance(Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        assert_eq!(batches(&sink), 0, "50ms despues del poll: sin flush");
        // Cruzar la ventana (50 + 110 > 100) -> flush con 1 mutation.
        tokio::time::advance(Duration::from_millis(110)).await;
        wait_for_batches(&sink, 1).await;
        assert_eq!(sink.0.lock().unwrap()[0].len(), 1, "batch de 1");
    }

    #[tokio::test(start_paused = true)]
    async fn batch_accumulates_up_to_interval_or_max_size() {
        let sink = CountingSink::default();
        let batcher = Batcher::spawn(Duration::from_millis(100), 4, sink.clone());
        for i in 0..3 {
            batcher.push(Mutation::new(format!("SELECT {i}"), vec![]));
        }
        tokio::time::advance(Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        assert_eq!(batches(&sink), 0, "dentro de la ventana de 100ms");
        tokio::time::advance(Duration::from_millis(120)).await;
        wait_for_batches(&sink, 1).await;
        assert_eq!(sink.0.lock().unwrap()[0].len(), 3, "3 mutations -> 1 batch");
    }

    #[tokio::test(start_paused = true)]
    async fn max_batch_splits_without_waiting() {
        let sink = CountingSink::default();
        let batcher = Batcher::spawn(Duration::from_millis(1000), 2, sink.clone());
        for i in 0..5 {
            batcher.push(Mutation::new(format!("SELECT {i}"), vec![]));
        }
        tokio::time::advance(Duration::from_millis(10)).await;
        // 2+2 por tamaño (sin reloj); el ultimo batch parcial (1) espera el
        // intervalo (1000ms) — avanzamos el reloj para cerrarlo.
        wait_for_batches(&sink, 2).await;
        tokio::time::advance(Duration::from_millis(1100)).await;
        wait_for_batches(&sink, 3).await;
        let batches = sink.0.lock().unwrap();
        assert_eq!(batches.len(), 3, "5 -> 2+2+1");
        assert_eq!(batches.iter().map(|b| b.len()).sum::<usize>(), 5);
    }

    // ------------------------------------------------------------- flush (unidad ACID)

    /// `flush()` cierra el batch pendiente INMEDIATAMENTE (sin avanzar el
    /// reloj — el flush rompe la ventana de acumulacion) y espera el commit:
    /// las mutations pusheadas antes aplican en UNA tx.
    #[tokio::test(start_paused = true)]
    async fn flush_applies_pending_batch_immediately() {
        let sink = CountingSink::default();
        let batcher = Batcher::spawn(Duration::from_millis(1000), 64, sink.clone());
        batcher.push(Mutation::new("SELECT 1", vec![]));
        batcher.push(Mutation::new("SELECT 2", vec![]));
        // Sin advance del reloj: el flush fuerza el cierre del batch.
        batcher.flush().await.expect("flush ok");
        let batches = sink.0.lock().unwrap();
        assert_eq!(batches.len(), 1, "1 batch");
        assert_eq!(batches[0].len(), 2, "las 2 mutations del flush");
    }

    /// `flush()` con cola vacia: no-op con Ok (nada que aplicar).
    #[tokio::test(start_paused = true)]
    async fn flush_with_empty_queue_returns_ok() {
        let sink = CountingSink::default();
        let batcher = Batcher::spawn(Duration::from_millis(100), 64, sink.clone());
        batcher.flush().await.expect("flush vacio ok");
        assert_eq!(batches(&sink), 0, "sin batches aplicados");
    }

    /// `flush()` propaga el error del sink: el batch fallo -> Err (el WAL
    /// local conserva el archivo para el replay del proximo arranque).
    #[tokio::test(start_paused = true)]
    async fn flush_propagates_sink_error() {
        #[derive(Clone)]
        struct AlwaysFailSink;
        impl MutationSink for AlwaysFailSink {
            fn apply(&mut self, _batch: Vec<Mutation>) -> impl std::future::Future<Output = Result<(), String>> + Send {
                async move { Err("PG caída (simulado)".into()) }
            }
        }
        let batcher = Batcher::spawn(Duration::from_millis(100), 64, AlwaysFailSink);
        batcher.push(Mutation::new("SELECT 1", vec![]));
        let err = batcher.flush().await.expect_err("sink falla");
        assert!(err.contains("PG caída"), "error del sink propagado: {err}");
    }

    // ------------------------------------------------------------------ WAL local (F3 phase 2)

    /// Round-trip completo del JSONL: `payload_json` -> `parse_payload_json`
    /// devuelve la MISMA mutation (id, sql y params) — cubre Text con
    /// comillas/backslash/UTF-8, Bytes no-ASCII, Int negativo y Null.
    #[test]
    fn payload_json_roundtrip_all_param_types() {
        let m = Mutation::with_id(
            uuidv7_from(1234, 0xDEADBEEF),
            "INSERT INTO t (a, b, c, d) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
            vec![
                Param::Text("comilla \" y backslash \\ y ñ€".into()),
                Param::Int(-42),
                Param::Bytes(vec![0x00, 0xFF, 0x10]),
                Param::Null,
            ],
        );
        let p = m.payload_json();
        let parsed = parse_payload_json(&p).expect("parse payload");
        assert_eq!(parsed, m, "round-trip exacto");
    }

    /// El json parseado también es un json VÁLIDO (shape del audit) — el
    /// parser inverso no es un json parser completo, pero el payload propio
    /// siempre round-trip (formato cerrado).
    #[test]
    fn payload_json_roundtrip_empty_params() {
        let m = Mutation::with_id(uuidv7_from(9, 1), "SELECT 1", vec![]);
        let parsed = parse_payload_json(&m.payload_json()).expect("parse");
        assert_eq!(parsed, m);
    }

    /// `persist_batch` escribe un archivo `{uuid}.wal` en el dir (creándolo)
    /// con una línea por mutation; `replay`-parseable y con sync_all (el
    /// archivo está completo al volver). El dir temporal se limpia SIEMPRE.
    #[test]
    fn persist_batch_writes_jsonl_file() {
        let dir = std::env::temp_dir().join(format!("e2e_wal_unit_{}", rand64()));
        let m1 = Mutation::with_id(
            uuidv7_from(111, 7),
            "INSERT INTO t (id) VALUES ($1) ON CONFLICT DO NOTHING",
            vec![Param::Int(7)],
        );
        let m2 = Mutation::with_id(
            uuidv7_from(222, 8),
            "INSERT INTO t (id) VALUES ($1) ON CONFLICT DO NOTHING",
            vec![Param::Null],
        );
        let path = persist_batch(dir.to_str().expect("utf8"), &[m1.clone(), m2.clone()])
            .expect("persistir");
        assert!(path.extension().map(|e| e == "wal").unwrap_or(false), "extensión .wal");
        let content = std::fs::read_to_string(&path).expect("leer archivo");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2, "una línea por mutation");
        assert_eq!(parse_payload_json(lines[0]).expect("l1"), m1);
        assert_eq!(parse_payload_json(lines[1]).expect("l2"), m2);
        // Cleanup SIEMPRE (patrón e2e_wal_*).
        std::fs::remove_file(&path).expect("borrar archivo");
        std::fs::remove_dir_all(&dir).expect("borrar dir");
    }

    /// WalSink durable-first:
    /// - sink que SIEMPRE falla -> el archivo QUEDA en disco (el replay del
    ///   arranque lo re-aplicará — idempotente);
    /// - sink OK -> el archivo se BORRA post-commit (WAL vacío).
    #[tokio::test]
    async fn walsink_removes_on_success_keeps_on_error() {
        fn count_files(dir: &std::path::Path) -> usize {
            std::fs::read_dir(dir)
                .expect("read dir")
                .filter_map(|e| e.ok())
                .count()
        }

        // Escenario 1: sink que siempre falla (PG caída persistente).
        #[derive(Clone)]
        struct AlwaysFailSink;
        impl MutationSink for AlwaysFailSink {
            fn apply(&mut self, _batch: Vec<Mutation>) -> impl std::future::Future<Output = Result<(), String>> + Send {
                async move { Err("PG caída (simulado)".into()) }
            }
        }
        let dir1 = std::env::temp_dir().join(format!("e2e_wal_fail_{}", rand64()));
        std::fs::create_dir_all(&dir1).expect("mkdir");
        let m = Mutation::with_id(uuidv7_from(333, 9), "SELECT 1", vec![]);
        let mut sink1 = WalSink::new(AlwaysFailSink, dir1.to_str().expect("utf8"));
        assert!(sink1.apply(vec![m.clone()]).await.is_err(), "falla");
        assert_eq!(count_files(&dir1), 1, "el batch con error queda en el WAL");
        assert!(sink1.apply(vec![m]).await.is_err(), "falla otra vez");
        assert_eq!(count_files(&dir1), 2, "cada batch fallido acumula su archivo");
        std::fs::remove_dir_all(&dir1).expect("cleanup dir1");

        // Escenario 2: sink OK -> archivo borrado tras el commit.
        #[derive(Clone, Default)]
        struct OkSink;
        impl MutationSink for OkSink {
            fn apply(&mut self, _batch: Vec<Mutation>) -> impl std::future::Future<Output = Result<(), String>> + Send {
                async move { Ok(()) }
            }
        }
        let dir2 = std::env::temp_dir().join(format!("e2e_wal_ok_{}", rand64()));
        std::fs::create_dir_all(&dir2).expect("mkdir");
        let m = Mutation::with_id(uuidv7_from(334, 10), "SELECT 2", vec![]);
        let mut sink2 = WalSink::new(OkSink, dir2.to_str().expect("utf8"));
        assert!(sink2.apply(vec![m]).await.is_ok(), "commit OK");
        assert_eq!(count_files(&dir2), 0, "post-commit el WAL está vacío");
        std::fs::remove_dir_all(&dir2).expect("cleanup dir2");
    }

    /// uuid malformado en el json -> Err descriptivo (defensivo).
    #[test]
    fn parse_payload_json_rejects_bad_uuid() {
        let bad = r#"{"mutation_id":"zz","sql":"SELECT 1","params":[]}"#;
        assert!(parse_payload_json(bad).is_err(), "uuid inválido");
        let no_close = r#"{"mutation_id":"8-4-4-4-12","sql":"SELECT 1","params":[]"#;
        assert!(parse_payload_json(no_close).is_err(), "json sin cerrar");
    }
}
