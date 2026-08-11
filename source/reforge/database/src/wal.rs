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
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;
use tokio_postgres::types::{IsNull, ToSql, Type};
use tokio_postgres::{Client, NoTls};
/// `bytes::BytesMut` re-exportado por tokio-postgres (ruta del trait ToSql).
use tokio_postgres::types::private::BytesMut;

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
            Param::Bytes(b) => write!(f, "\"\\x{}\"", b.iter().map(|x| format!("{x:02x}")).collect::<String>()),
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
/// en la MISMA tx (ADR-0008: durable = batch transaccional <=100ms).
pub struct PgMutationSink {
    pg_conn: String,
    /// Tabla de audit (default `log.mutation_audit`; los tests usan `e2e_wal.*`).
    audit_table: String,
}

impl PgMutationSink {
    pub fn new(pg_conn: impl Into<String>) -> Self {
        Self { pg_conn: pg_conn.into(), audit_table: "log.mutation_audit".to_string() }
    }

    pub fn with_audit_table(mut self, audit_table: impl Into<String>) -> Self {
        self.audit_table = audit_table.into();
        self
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

/// Batcher: recoge mutations y las flushea en batches — flush por tamaño
/// (`max_batch`) o por tiempo (`flush_interval` desde la PRIMERA mutation del
/// batch, <=100ms en produccion). El worker es una task tokio; el sender se
/// clona para pushear desde cualquier task.
pub struct Batcher {
    tx: mpsc::UnboundedSender<Mutation>,
}

impl Batcher {
    /// Arranca el worker; `sink` aplica los batches (UNA transaccion cada uno).
    pub fn spawn(flush_interval: Duration, max_batch: usize, sink: impl MutationSink) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<Mutation>();
        tokio::spawn(async move {
            let mut sink = sink;
            loop {
                // Primera mutation del batch: espera indefinida (el canal se
                // cierra al dropear todos los senders -> fin del worker).
                let Some(first) = rx.recv().await else { break };
                let mut batch = vec![first];
                // Acumula hasta max_batch o hasta que pase flush_interval.
                while batch.len() < max_batch {
                    match tokio::time::timeout(flush_interval, rx.recv()).await {
                        Ok(Some(m)) => batch.push(m),
                        _ => break, // timeout o canal cerrado -> flush ya
                    }
                }
                if let Err(e) = sink.apply(batch).await {
                    eprintln!("database: wal: batch falló: {e} — el WAL local (F3 phase 2 completo) lo re-aplicará");
                }
            }
        });
        Self { tx }
    }

    /// Push de una mutation (unbounded: nunca bloquea al caller).
    pub fn push(&self, m: Mutation) {
        let _ = self.tx.send(m);
    }
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
}
