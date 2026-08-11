//! Loop de conexión del proxy (spec §8.2.1c): handshake v10 → auth
//! `mysql_native_password` → sesión PG 1:1 → bucle de comandos
//! (COM_QUERY/COM_QUIT/COM_PING; sin prepared statements).
//!
//! Contrato multi-result (`SQLMsg::Store`, `AsyncSQL.h:59-80`): un statement por
//! COM_QUERY en la práctica; el splitter defensivo maneja multi-statements con
//! `SERVER_MORE_RESULTS_EXISTS` para que `mysql_next_result` siga/termine.
//!
//! Debug: `--debug` o `MYSQL_PROXY_DEBUG=1` → log por conexión del search_path,
//! las SETs de init, cada COM_QUERY (truncado), errores PG (SQLSTATE+mensaje+
//! statement) y metadata de result sets (nunca contenido de filas).

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::config::Config;
use crate::debug;
use crate::session::{init_statements, PgError, PgSession};
use crate::translate::{self, InsertIdHint, Rewritten};
use crate::wire::{self, ClientCommand};

/// Sirve conexiones hasta que el proceso muera.
pub async fn serve(config: Config) -> std::io::Result<()> {
    let listener = TcpListener::bind(&config.listen).await?;
    eprintln!("mysql_proxy: escuchando en {} (→ PG: {})", config.listen, config.pg_conn);
    if debug::enabled() {
        eprintln!("mysql_proxy: debug ON (MYSQL_PROXY_DEBUG=1 / --debug)");
    }
    let mut conn_id: u32 = 1;
    loop {
        let (stream, _peer) = listener.accept().await?;
        let cfg = config.clone();
        let id = conn_id;
        conn_id = conn_id.wrapping_add(1);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, cfg, id).await {
                eprintln!("mysql_proxy: conexión {id}: {e}");
            }
        });
    }
}

/// Lee un paquete MySQL (4 B de cabecera + payload). `Ok(None)` = EOF del cliente.
async fn read_packet<R: AsyncRead + Unpin>(r: &mut R) -> std::io::Result<Option<(u8, Vec<u8>)>> {
    let mut hdr = [0u8; 4];
    match r.read_exact(&mut hdr).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = hdr[0] as usize | ((hdr[1] as usize) << 8) | ((hdr[2] as usize) << 16);
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload).await?;
    Ok(Some((hdr[3], payload)))
}

async fn handle_connection(stream: TcpStream, config: Config, conn_id: u32) -> Result<(), String> {
    let (mut r, mut w) = stream.into_split();

    // 1) HandshakeV10 (seq 0).
    let scramble = wire::random_scramble();
    w.write_all(&wire::encode_handshake(
        "5.7.44-m2-proxy",
        conn_id,
        &scramble,
        wire::CAP_SERVER_CAPS,
        wire::CHARSET_UTF8MB4_GENERAL_CI,
    ))
    .await
    .map_err(|e| e.to_string())?;

    // 2) HandshakeResponse41.
    let Some((_, payload)) = read_packet(&mut r).await.map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let resp = wire::decode_handshake_response(&payload).map_err(|e| e.to_string())?;
    debug::log(format_args!("conn {conn_id}: handshake user={} db={:?} plugin={:?}", resp.username, resp.database, resp.plugin));

    // 3) Auth mysql_native_password contra las credenciales del config.
    if resp.username != config.mysql_user
        || !wire::validate_native_auth(config.mysql_password.as_bytes(), &scramble, &resp.auth_response)
    {
        w.write_all(&wire::encode_err(2, wire::ER_ACCESS_DENIED, "28000", "Access denied for user"))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    // 4) Slot: el nombre de db del handshake mapea el search_path (spec §8.2.1c).
    let db = resp
        .database
        .ok_or_else(|| "cliente sin db: el proxy necesita el nombre de db para mapear el slot".to_string())?;
    let search_path = config
        .search_path(&db)
        .ok_or_else(|| format!("db desconocida: {db} (revisar [slots] del config)"))?;
    debug::log(format_args!("conn {conn_id}: slot db={db} search_path={search_path}"));

    // 5) Sesión PG: connect + init ANTES del OK de auth. Si el init falla
    // (p.ej. search_path inválido), se aborta con ERR visible — nunca se sirve
    // una query con una sesión sin init (bug del gate: "relation does not exist").
    let mut pg = match PgSession::connect(&config.pg_conn, &search_path, &config.timezone).await {
        Ok(pg) => pg,
        Err(e) => {
            debug::log(format_args!("conn {conn_id}: PG init FAILED: {e}"));
            let msg = format!("PG session init failed: {e}");
            w.write_all(&wire::encode_err(2, wire::ER_UNKNOWN, "HY000", &msg))
                .await
                .map_err(|we| we.to_string())?;
            return Ok(());
        }
    };
    for s in init_statements(&search_path, &config.timezone) {
        debug::log(format_args!("conn {conn_id}: init: {s}"));
    }

    // 6) OK de auth (seq 2).
    w.write_all(&wire::encode_ok(2, 0, 0, wire::STATUS_AUTOCOMMIT))
        .await
        .map_err(|e| e.to_string())?;

    // 7) Bucle de comandos.
    loop {
        let Some((_client_seq, payload)) = read_packet(&mut r).await.map_err(|e| e.to_string())? else {
            break;
        };
        // La respuesta arranca en seq+1 (cada comando reinicia su contador).
        match wire::decode_command(&payload) {
            Ok(ClientCommand::Quit) => break,
            Ok(ClientCommand::Ping) => {
                w.write_all(&wire::encode_ok(1, 0, 0, wire::STATUS_AUTOCOMMIT))
                    .await
                    .map_err(|e| e.to_string())?;
            }
            Ok(ClientCommand::Query(sql)) => {
                debug::log(format_args!("conn {conn_id}: query: {}", debug::truncate(&sql, 200)));
                handle_query(&mut pg, &mut w, &sql).await?;
            }
            Ok(ClientCommand::Unknown(cmd)) => {
                debug::log(format_args!("conn {conn_id}: comando desconocido 0x{cmd:02x}"));
                w.write_all(&wire::encode_err(
                    1,
                    wire::ER_UNKNOWN_COM,
                    "08S01",
                    &format!("comando desconocido: 0x{cmd:02x}"),
                ))
                .await
                .map_err(|e| e.to_string())?;
            }
            Err(e) => {
                w.write_all(&wire::encode_err(1, wire::ER_UNKNOWN, "HY000", &e.to_string()))
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// Traduce y ejecuta un COM_QUERY (posiblemente multi-statement).
async fn handle_query<W: AsyncWrite + Unpin>(
    pg: &mut PgSession,
    w: &mut W,
    sql: &str,
) -> Result<(), String> {
    let statements = translate::split_statements(sql);
    if statements.is_empty() {
        w.write_all(&wire::encode_ok(1, 0, 0, wire::STATUS_AUTOCOMMIT))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let n = statements.len();
    for (idx, stmt) in statements.iter().enumerate() {
        let more = idx + 1 < n;
        let status = if more {
            wire::STATUS_AUTOCOMMIT | wire::STATUS_MORE_RESULTS
        } else {
            wire::STATUS_AUTOCOMMIT
        };
        let plan = match translate::rewrite(stmt, pg).await {
            Ok(Rewritten::NoOp) => {
                debug::log(format_args!("  → no-op (SET de config MySQL)"));
                w.write_all(&wire::encode_ok(1, 0, 0, status)).await.map_err(|e| e.to_string())?;
                continue;
            }
            Ok(Rewritten::Execute(plan)) => plan,
            Err(e) => {
                debug::log(format_args!("  → translate error: {e}"));
                w.write_all(&wire::encode_err(1, e.mysql_errno(), "42000", &e.to_string()))
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(()); // MySQL aborta el batch en el primer error
            }
        };
        match pg.execute(&plan.sql).await {
            Ok(outcome) => {
                let insert_id = match plan.insert_id {
                    InsertIdHint::Explicit(v) => v,
                    InsertIdHint::Generated => pg.last_insert_id().await,
                    InsertIdHint::None => 0,
                };
                debug::log(format_args!(
                    "  → PG ok: sql={} result_set={} cols={} rows={} affected={} insert_id={}",
                    debug::truncate(&plan.sql, 160),
                    outcome.is_result_set,
                    outcome.columns.len(),
                    outcome.rows.len(),
                    outcome.affected,
                    insert_id,
                ));
                if !outcome.columns.is_empty() {
                    let meta: Vec<String> = outcome
                        .columns
                        .iter()
                        .map(|c| format!("{}:type{}", c.name, c.type_code))
                        .collect();
                    debug::log(format_args!("  → columns: [{}]", meta.join(", ")));
                }
                if outcome.is_result_set {
                    let packets = wire::encode_result_set(1, &outcome.columns, &outcome.rows, more);
                    debug::log(format_args!(
                        "  → result set: {} paquetes, {} B (sin contenido de filas)",
                        packets.len(),
                        packets.iter().map(Vec::len).sum::<usize>(),
                    ));
                    for pkt in packets {
                        w.write_all(&pkt).await.map_err(|e| e.to_string())?;
                    }
                } else {
                    w.write_all(&wire::encode_ok(1, outcome.affected, insert_id, status))
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            Err(PgError { sqlstate, message }) => {
                let state = sqlstate.as_deref().unwrap_or("HY000");
                let code = wire::map_pg_sqlstate(state);
                debug::log(format_args!("  → PG error {state}: {message} | statement: {}", debug::truncate(&plan.sql, 160)));
                w.write_all(&wire::encode_err(1, code, state, &message))
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }
        }
    }
    Ok(())
}
