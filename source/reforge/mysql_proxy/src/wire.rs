//! Codec del protocolo wire de MySQL v10 (spec §8.2.1c) — implementado a mano,
//! sin dependencia de wire.
//!
//! Layouts según la documentación oficial (dev.mysql.com, `Protocol::HandshakeV10`
//! / `Protocol::HandshakeResponse41`). Superficie mínima porque el único cliente
//! es el baseline C++ (`libsql`, `AsyncSQL.cpp`): handshake + `mysql_native_password`
//! + COM_QUERY/COM_QUIT/COM_PING. Sin prepared statements (`CStmt` 0 call sites —
//!   `legacy-sql-compatibility.md` §2.1).
//!
//! Vectores de test: (a) golden del ejemplo documentado de HandshakeResponse41
//! (MySQL 5.5.8, usuario `pam`, db `test`); (b) token `mysql_native_password`
//! cruzado con .NET SHA1 (scramble del ejemplo clásico de la documentación);
//! (c) vectores FIPS 180-1 para SHA-1 (módulo `sha1`).

use crate::sha1;

// ---------------------------------------------------------------------------
// Capacidades / flags / comandos / tipos (constantes del protocolo)
// ---------------------------------------------------------------------------

pub const CAP_LONG_PASSWORD: u32 = 0x0000_0001;
pub const CAP_FOUND_ROWS: u32 = 0x0000_0002;
pub const CAP_CONNECT_WITH_DB: u32 = 0x0000_0008;
pub const CAP_TRANSACTIONS: u32 = 0x0000_2000;
pub const CAP_PROTOCOL_41: u32 = 0x0000_0200;
pub const CAP_SECURE_CONNECTION: u32 = 0x0000_8000;
pub const CAP_MULTI_STATEMENTS: u32 = 0x0001_0000;
pub const CAP_PLUGIN_AUTH: u32 = 0x0008_0000;

/// Capacidades que anuncia el proxy (spec §8.2.1c) = 0x0009_A208.
pub const CAP_SERVER_CAPS: u32 = CAP_PROTOCOL_41
    | CAP_PLUGIN_AUTH
    | CAP_SECURE_CONNECTION
    | CAP_CONNECT_WITH_DB
    | CAP_MULTI_STATEMENTS
    | CAP_TRANSACTIONS;

pub const STATUS_AUTOCOMMIT: u16 = 0x0002;
/// `SERVER_MORE_RESULTS_EXISTS` — lo consume el `do…while (mysql_next_result)`
/// de `SQLMsg::Store` (`AsyncSQL.h:59-80`).
pub const STATUS_MORE_RESULTS: u16 = 0x0008;

pub const COM_QUIT: u8 = 0x01;
pub const COM_QUERY: u8 = 0x03;
pub const COM_PING: u8 = 0x0e;

/// Charset id del protocolo: utf8mb4_general_ci = 45, binary = 63.
pub const CHARSET_UTF8MB4_GENERAL_CI: u8 = 45;
pub const CHARSET_BINARY: u8 = 63;

// Column flags (mysql_com.h).
pub const NOT_NULL_FLAG: u16 = 0x0001;
pub const BLOB_FLAG: u16 = 0x0010;
pub const BINARY_FLAG: u16 = 0x0080;
pub const NUM_FLAG: u16 = 0x8000;

// MYSQL_TYPE_* (mysql_com.h).
pub const MYSQL_TYPE_TINY: u8 = 0x01;
pub const MYSQL_TYPE_SHORT: u8 = 0x02;
pub const MYSQL_TYPE_LONG: u8 = 0x03;
pub const MYSQL_TYPE_FLOAT: u8 = 0x04;
pub const MYSQL_TYPE_DOUBLE: u8 = 0x05;
pub const MYSQL_TYPE_DATE: u8 = 0x0a;
pub const MYSQL_TYPE_TIME: u8 = 0x0b;
pub const MYSQL_TYPE_DATETIME: u8 = 0x0c;
pub const MYSQL_TYPE_LONGLONG: u8 = 0x08;
pub const MYSQL_TYPE_BLOB: u8 = 0xfc;
pub const MYSQL_TYPE_VAR_STRING: u8 = 0xfd;
pub const MYSQL_TYPE_STRING: u8 = 0xfe;
pub const MYSQL_TYPE_NEWDECIMAL: u8 = 0xf6;

// Errores MySQL que mapeamos (el C++ los compara vía `mysql_errno`).
pub const ER_ACCESS_DENIED: u16 = 1045;
pub const ER_NO_DB: u16 = 1049;
pub const ER_UNKNOWN_COM: u16 = 1047;
pub const ER_PARSE_ERROR: u16 = 1064;
pub const ER_DUP_ENTRY: u16 = 1062;
pub const ER_BAD_NULL: u16 = 1048;
pub const ER_BAD_FIELD: u16 = 1054;
pub const ER_NO_SUCH_TABLE: u16 = 1146;
pub const ER_WRONG_VALUE: u16 = 1292;
pub const ER_UNKNOWN: u16 = 1105;

/// OIDs de PostgreSQL que el proxy conoce (bytea = 17).
pub const OID_BYTEA: u32 = 17;

// ---------------------------------------------------------------------------
// Errores
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// Payload vacío o sin el byte de comando.
    EmptyPacket,
    /// COM_QUERY con bytes que no son UTF-8 (literales CP949 en la query —
    /// fuera del alcance fase 1; se rechaza en vez de corromper).
    NonUtf8Query,
    /// HandshakeResponse41 malformado.
    BadHandshakeResponse(String),
    /// HandshakeV10 malformado.
    BadHandshakeV10(String),
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::EmptyPacket => write!(f, "paquete vacío"),
            WireError::NonUtf8Query => write!(
                f,
                "COM_QUERY no es UTF-8 (literales no-ASCII fuera de alcance fase 1)"
            ),
            WireError::BadHandshakeResponse(m) => write!(f, "HandshakeResponse41: {m}"),
            WireError::BadHandshakeV10(m) => write!(f, "HandshakeV10: {m}"),
        }
    }
}

impl std::error::Error for WireError {}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

/// Cabecera de paquete: 3 bytes de longitud (LE) + 1 byte de sequence id.
/// `len` máximo 0xFFFFFF (un solo paquete; las respuestas del proxy caben).
fn write_packet(seq: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 4);
    let len = payload.len() as u32;
    out.push((len & 0xff) as u8);
    out.push(((len >> 8) & 0xff) as u8);
    out.push(((len >> 16) & 0xff) as u8);
    out.push(seq);
    out.extend_from_slice(payload);
    out
}

/// Divide un buffer en paquetes `(seq, payload)` (helper de tests/inspección).
pub fn parse_packets(buf: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= buf.len() {
        let len = (buf[i] as usize) | ((buf[i + 1] as usize) << 8) | ((buf[i + 2] as usize) << 16);
        let seq = buf[i + 3];
        let end = i + 4 + len;
        if end > buf.len() {
            break;
        }
        out.push((seq, buf[i + 4..end].to_vec()));
        i = end;
    }
    out
}

// ---------------------------------------------------------------------------
// length-encoded integers / strings
// ---------------------------------------------------------------------------

pub fn lenenc_int(n: u64) -> Vec<u8> {
    if n < 0xfb {
        vec![n as u8]
    } else if n <= 0xffff {
        let mut v = vec![0xfc];
        v.extend_from_slice(&(n as u16).to_le_bytes());
        v
    } else if n <= 0xff_ffff {
        let mut v = vec![0xfd];
        v.extend_from_slice(&(n as u32).to_le_bytes()[..3]);
        v
    } else {
        let mut v = vec![0xfe];
        v.extend_from_slice(&n.to_le_bytes());
        v
    }
}

pub fn lenenc_bytes(data: &[u8]) -> Vec<u8> {
    let mut v = lenenc_int(data.len() as u64);
    v.extend_from_slice(data);
    v
}

fn lenenc_str(s: &str) -> Vec<u8> {
    lenenc_bytes(s.as_bytes())
}

// ---------------------------------------------------------------------------
// HandshakeV10 (server → client)
// ---------------------------------------------------------------------------

/// Layout oficial (dev.mysql.com, Protocol::HandshakeV10). Devuelve el paquete
/// completo (seq 0).
pub fn encode_handshake(
    server_version: &str,
    conn_id: u32,
    scramble: &[u8; 20],
    capabilities: u32,
    charset: u8,
) -> Vec<u8> {
    let mut p = Vec::with_capacity(64);
    p.push(0x0a); // protocol version 10
    p.extend_from_slice(server_version.as_bytes());
    p.push(0);
    p.extend_from_slice(&conn_id.to_le_bytes());
    p.extend_from_slice(&scramble[..8]); // auth-plugin-data-part-1
    p.push(0); // filler
    p.extend_from_slice(&(capabilities as u16).to_le_bytes()); // capability_flags_1
    p.push(charset);
    p.extend_from_slice(&STATUS_AUTOCOMMIT.to_le_bytes()); // status_flags
    p.extend_from_slice(&((capabilities >> 16) as u16).to_le_bytes()); // capability_flags_2
    p.push(21); // auth_plugin_data_len (20 + NUL)
    p.extend_from_slice(&[0u8; 10]); // reserved
    p.extend_from_slice(&scramble[8..20]); // auth-plugin-data-part-2 (12)
    p.push(0); // NUL terminator de la scramble
    p.extend_from_slice(b"mysql_native_password\0");
    write_packet(0, &p)
}

/// Estructura decodificada de HandshakeV10 (para tests/inspección).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeV10 {
    pub server_version: String,
    pub conn_id: u32,
    pub scramble: [u8; 20],
    pub capabilities: u32,
    pub charset: u8,
}

/// Decodifica HandshakeV10 según el layout oficial (tolera el campo part-2
/// variable: `$len = MAX(13, auth_plugin_data_len - 8)`).
pub fn decode_handshake_v10(payload: &[u8]) -> Result<HandshakeV10, WireError> {
    let err = |m: &str| WireError::BadHandshakeV10(m.to_string());
    if payload.len() < 32 || payload[0] != 0x0a {
        return Err(err("protocol version != 10"));
    }
    let version_end = payload[1..]
        .iter()
        .position(|&b| b == 0)
        .map(|p| p + 1)
        .ok_or_else(|| err("server version sin NUL"))?;
    let server_version = std::str::from_utf8(&payload[1..version_end])
        .map_err(|_| err("server version no UTF-8"))?
        .to_string();
    // Layout oficial (Protocol::HandshakeV10), relativo al NUL de la versión:
    // conn id u32 LE, part-1 [8], filler, cap lower u16, charset u8, status u16,
    // cap upper u16, auth len u8, reserved [10], part-2.
    let conn_id = u32::from_le_bytes([
        payload[version_end + 1],
        payload[version_end + 2],
        payload[version_end + 3],
        payload[version_end + 4],
    ]);
    let part1 = &payload[version_end + 5..version_end + 13];
    let cap_lower = u16::from_le_bytes([payload[version_end + 14], payload[version_end + 15]]);
    let charset = payload[version_end + 16];
    let cap_upper = u16::from_le_bytes([payload[version_end + 19], payload[version_end + 20]]);
    let capabilities = ((cap_upper as u32) << 16) | cap_lower as u32;
    let auth_len = payload[version_end + 21] as usize;
    let part2_start = version_end + 22 + 10;
    let part2_len = auth_len.saturating_sub(8).max(13);
    let part2 = payload
        .get(part2_start..part2_start + part2_len)
        .unwrap_or(&[]);
    let mut scramble = [0u8; 20];
    scramble[..8].copy_from_slice(part1);
    let tail = part2.len().min(12);
    scramble[8..8 + tail].copy_from_slice(&part2[..tail]);
    Ok(HandshakeV10 {
        server_version,
        conn_id,
        scramble,
        capabilities,
        charset,
    })
}

// ---------------------------------------------------------------------------
// HandshakeResponse41 (client → server)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeResponse {
    pub capabilities: u32,
    pub max_packet: u32,
    pub charset: u8,
    pub username: String,
    pub auth_response: Vec<u8>,
    pub database: Option<String>,
    pub plugin: Option<String>,
}

/// CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA (0x0200_0000) — auth response lenenc.
const CAP_PLUGIN_AUTH_LENENC_CLIENT_DATA: u32 = 0x0200_0000;

/// Decodifica HandshakeResponse41 (layout oficial). Defensivo: el cliente decide
/// qué campos incluye según SU máscara de capacidades; los atributos
/// (CLIENT_CONNECT_ATTRS) se toleran ausentes aunque la máscara los pida (el
/// ejemplo documentado de MySQL 5.5.8 los omite).
pub fn decode_handshake_response(payload: &[u8]) -> Result<HandshakeResponse, WireError> {
    let err = |m: &str| WireError::BadHandshakeResponse(m.to_string());
    if payload.len() < 32 {
        return Err(err("payload demasiado corto"));
    }
    let capabilities = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let max_packet = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
    let charset = payload[8];
    let mut pos = 9 + 23; // filler
    let username_end = payload[pos..]
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| err("username sin NUL"))?;
    let username = std::str::from_utf8(&payload[pos..pos + username_end])
        .map_err(|_| err("username no UTF-8"))?
        .to_string();
    pos += username_end + 1;
    let auth_response = if capabilities & CAP_PLUGIN_AUTH_LENENC_CLIENT_DATA != 0 {
        let (len, n) = read_lenenc(&payload[pos..]).ok_or_else(|| err("auth lenenc"))?;
        let len = len as usize;
        pos += n;
        let bytes = payload
            .get(pos..pos + len)
            .ok_or_else(|| err("auth response corta"))?
            .to_vec();
        pos += len;
        bytes
    } else {
        let len = *payload.get(pos).ok_or_else(|| err("auth len"))? as usize;
        pos += 1;
        let bytes = payload
            .get(pos..pos + len)
            .ok_or_else(|| err("auth response corta"))?
            .to_vec();
        pos += len;
        bytes
    };
    let mut database = None;
    if capabilities & CAP_CONNECT_WITH_DB != 0 {
        let end = payload[pos..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| err("db sin NUL"))?;
        database = Some(
            std::str::from_utf8(&payload[pos..pos + end])
                .map_err(|_| err("db no UTF-8"))?
                .to_string(),
        );
        pos += end + 1;
    }
    let mut plugin = None;
    if capabilities & CAP_PLUGIN_AUTH != 0 && pos < payload.len() {
        let end = payload[pos..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| err("plugin sin NUL"))?;
        plugin = Some(
            std::str::from_utf8(&payload[pos..pos + end])
                .map_err(|_| err("plugin no UTF-8"))?
                .to_string(),
        );
        // CLIENT_CONNECT_ATTRS: bloque lenenc opcional — se ignora (nada del
        // C++ depende de los atributos; el ejemplo documentado los omite).
    }
    Ok(HandshakeResponse {
        capabilities,
        max_packet,
        charset,
        username,
        auth_response,
        database,
        plugin,
    })
}

/// Lee un integer length-encoded al inicio de `b` → (valor, bytes consumidos).
fn read_lenenc(b: &[u8]) -> Option<(u64, usize)> {
    let first = *b.first()?;
    match first {
        0xfb => None, // NULL no aplica aquí
        0xfc => {
            let v = u16::from_le_bytes([*b.get(1)?, *b.get(2)?]) as u64;
            Some((v, 3))
        }
        0xfd => {
            let v = *b.get(1)? as u64 | ((*b.get(2)? as u64) << 8) | ((*b.get(3)? as u64) << 16);
            Some((v, 4))
        }
        0xfe => {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(b.get(1..9)?);
            Some((u64::from_le_bytes(arr), 9))
        }
        _ => Some((first as u64, 1)),
    }
}

// ---------------------------------------------------------------------------
// Respuestas server → client
// ---------------------------------------------------------------------------

/// OK packet (0x00): affected rows + last insert id + status + warnings.
pub fn encode_ok(seq: u8, affected: u64, insert_id: u64, status: u16) -> Vec<u8> {
    let mut p = vec![0x00];
    p.extend(lenenc_int(affected));
    p.extend(lenenc_int(insert_id));
    p.extend_from_slice(&status.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes()); // warnings
    write_packet(seq, &p)
}

/// ERR packet (0xff): código + SQLSTATE `#xxxxx` + mensaje.
pub fn encode_err(seq: u8, code: u16, sqlstate: &str, message: &str) -> Vec<u8> {
    let mut p = vec![0xff];
    p.extend_from_slice(&code.to_le_bytes());
    p.push(b'#');
    p.extend_from_slice(sqlstate.as_bytes());
    p.extend_from_slice(message.as_bytes());
    write_packet(seq, &p)
}

/// EOF packet (0xfe): warnings + status. Se usa al final de column defs y de rows.
pub fn encode_eof(seq: u8, status: u16) -> Vec<u8> {
    let mut p = vec![0xfe];
    p.extend_from_slice(&0u16.to_le_bytes()); // warnings
    p.extend_from_slice(&status.to_le_bytes());
    write_packet(seq, &p)
}

/// Definición de columna para el result set (Protocol::ColumnDefinition41).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    pub name: String,
    pub schema: String,
    pub table: String,
    pub charset: u8,
    pub column_length: u32,
    pub type_code: u8,
    pub flags: u16,
    pub decimals: u8,
}

pub fn encode_column_def(seq: u8, def: &ColumnDef) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend(lenenc_str("def"));
    p.extend(lenenc_str(&def.schema));
    p.extend(lenenc_str(&def.table));
    p.extend(lenenc_str(&def.table)); // org_table
    p.extend(lenenc_str(&def.name));
    p.extend(lenenc_str(&def.name)); // org_name
    p.extend(lenenc_int(0x0c)); // length de los campos fijos
    p.extend_from_slice(&(def.charset as u16).to_le_bytes()); // character set (u16)
    p.extend_from_slice(&def.column_length.to_le_bytes());
    p.push(def.type_code);
    p.extend_from_slice(&def.flags.to_le_bytes());
    p.push(def.decimals);
    p.extend_from_slice(&0u16.to_le_bytes()); // filler
    write_packet(seq, &p)
}

/// Result set completo: column count + column defs + EOF + rows + EOF.
///
/// Contrato multi-result (`SQLMsg::Store`, `AsyncSQL.h:59-80`): `more` marca
/// `SERVER_MORE_RESULTS_EXISTS` en el EOF final (o en el OK) para que
/// `mysql_next_result` siga; con un solo statement el flag queda limpio.
///
/// Fila text (`ProtocolText::ResultsetRow`, dev.mysql.com): SOLO las celdas
/// lenenc — `0xFB` para NULL, `string<lenenc>` (length + data) para el resto.
/// NO lleva contador de celdas (eso es del protocolo binario): ponerlo hace que
/// el cliente lea el length del primer valor como el valor mismo (bug del gate:
/// `COUNT(*)`=2864 se mostraba como 0x04).
pub fn encode_result_set(
    seq0: u8,
    columns: &[ColumnDef],
    rows: &[Vec<Option<Vec<u8>>>],
    more: bool,
) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut seq = seq0;
    out.push(write_packet(seq, &lenenc_int(columns.len() as u64)));
    seq = seq.wrapping_add(1);
    for def in columns {
        out.push(encode_column_def(seq, def));
        seq = seq.wrapping_add(1);
    }
    out.push(encode_eof(seq, STATUS_AUTOCOMMIT));
    seq = seq.wrapping_add(1);
    for row in rows {
        let mut p = Vec::with_capacity(8);
        for cell in row {
            match cell {
                None => p.push(0xfb),
                Some(bytes) => {
                    p.extend(lenenc_int(bytes.len() as u64));
                    p.extend_from_slice(bytes);
                }
            }
        }
        out.push(write_packet(seq, &p));
        seq = seq.wrapping_add(1);
    }
    let final_status = if more {
        STATUS_AUTOCOMMIT | STATUS_MORE_RESULTS
    } else {
        STATUS_AUTOCOMMIT
    };
    out.push(encode_eof(seq, final_status));
    out
}

// ---------------------------------------------------------------------------
// Comandos client → server
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientCommand {
    Quit,
    Ping,
    Query(String),
    /// Comando no soportado (p.ej. COM_STMT_* — sin prepared statements).
    Unknown(u8),
}

pub fn decode_command(payload: &[u8]) -> Result<ClientCommand, WireError> {
    let Some(&cmd) = payload.first() else {
        return Err(WireError::EmptyPacket);
    };
    match cmd {
        COM_QUIT => Ok(ClientCommand::Quit),
        COM_PING => Ok(ClientCommand::Ping),
        COM_QUERY => {
            let text = std::str::from_utf8(&payload[1..]).map_err(|_| WireError::NonUtf8Query)?;
            Ok(ClientCommand::Query(text.to_string()))
        }
        other => Ok(ClientCommand::Unknown(other)),
    }
}

// ---------------------------------------------------------------------------
// Auth mysql_native_password
// ---------------------------------------------------------------------------

/// Token del cliente: `SHA1(password) XOR SHA1(scramble || SHA1(SHA1(password)))`.
pub fn scramble_response(password: &[u8], scramble: &[u8]) -> [u8; 20] {
    let stage1 = sha1::digest(password);
    let stage2 = sha1::digest(&stage1);
    let mut h = Vec::with_capacity(scramble.len() + stage2.len());
    h.extend_from_slice(scramble);
    h.extend_from_slice(&stage2);
    let x = sha1::digest(&h);
    let mut token = [0u8; 20];
    for i in 0..20 {
        token[i] = x[i] ^ stage1[i];
    }
    token
}

/// Valida el token del cliente contra la contraseña esperada (config).
pub fn validate_native_auth(password: &[u8], scramble: &[u8], token: &[u8]) -> bool {
    if token.len() != 20 {
        return false;
    }
    token == scramble_response(password, scramble)
}

// ---------------------------------------------------------------------------
// Mapeo de tipos PG → metadata MySQL (text protocol)
// ---------------------------------------------------------------------------

/// Partes de ColumnDefinition41 a partir del OID de PostgreSQL.
/// `bytea` → MYSQL_TYPE_BLOB con bytes crudos (el C++ lo necesita en
/// `questlua_global.cpp:1616-1624` y en el path de binarios escapados
/// `ClientManagerPlayer.cpp:171-175`); numéricos → IS_NUM; `tinyint(1)` nunca
/// es PG `bool` en el esquema fase 1 (paridad de text protocol, spec §8.2.1b).
///
/// NOTA: el simple query protocol de tokio-postgres solo expone NOMBRES de
/// columna; `session::PgSession` usa este mapeo solo para los nombres que sabe
/// bytea (vía catálogo) y reporta el resto como VAR_STRING — el C++ consume
/// todo como texto (`str_to_number`/`strlcpy`). El mapeo por OID queda
/// disponible para el path extendido (F3+).
pub fn mysql_column_parts(type_oid: u32) -> (u8, u8, u32, u16) {
    // (type_code, charset, column_length, flags)
    match type_oid {
        17 => (
            MYSQL_TYPE_BLOB,
            CHARSET_BINARY,
            65_535,
            BLOB_FLAG | BINARY_FLAG,
        ), // bytea
        21 => (MYSQL_TYPE_SHORT, CHARSET_BINARY, 6, NUM_FLAG), // int2
        23 => (MYSQL_TYPE_LONG, CHARSET_BINARY, 11, NUM_FLAG), // int4
        20 => (MYSQL_TYPE_LONGLONG, CHARSET_BINARY, 20, NUM_FLAG), // int8
        700 => (MYSQL_TYPE_FLOAT, CHARSET_BINARY, 12, NUM_FLAG), // float4
        701 => (MYSQL_TYPE_DOUBLE, CHARSET_BINARY, 22, NUM_FLAG), // float8
        1700 => (MYSQL_TYPE_NEWDECIMAL, CHARSET_BINARY, 20, NUM_FLAG), // numeric
        16 => (MYSQL_TYPE_TINY, CHARSET_BINARY, 1, NUM_FLAG),  // bool (no usado en fase 1)
        25 | 1042 | 1043 => (MYSQL_TYPE_VAR_STRING, CHARSET_UTF8MB4_GENERAL_CI, 255, 0), // text/varchar/bpchar
        1082 => (MYSQL_TYPE_DATE, CHARSET_UTF8MB4_GENERAL_CI, 10, 0),                    // date
        1083 => (MYSQL_TYPE_TIME, CHARSET_UTF8MB4_GENERAL_CI, 10, 0),                    // time
        1114 | 1184 => (MYSQL_TYPE_DATETIME, CHARSET_UTF8MB4_GENERAL_CI, 19, 0), // timestamp
        1186 => (MYSQL_TYPE_STRING, CHARSET_UTF8MB4_GENERAL_CI, 24, 0), // interval (loginlog2.playtime)
        869 => (MYSQL_TYPE_STRING, CHARSET_UTF8MB4_GENERAL_CI, 45, 0),  // inet
        _ => (MYSQL_TYPE_VAR_STRING, CHARSET_UTF8MB4_GENERAL_CI, 255, 0),
    }
}

/// Valor de una celda bytea: el text protocol de PG la entrega como `\x…` hex;
/// el wire MySQL debe llevar los bytes crudos (OD-6: round-trip byte-exacto de
/// `item_proto.name`/`mob_proto.locale_name`/`skill_proto.szName`).
pub fn decode_bytea_text(text: &[u8]) -> Vec<u8> {
    if let Some(hex) = text.strip_prefix(b"\\x") {
        let mut out = Vec::with_capacity(hex.len() / 2);
        let mut i = 0;
        while i + 1 < hex.len() {
            let hi = (hex[i] as char).to_digit(16);
            let lo = (hex[i + 1] as char).to_digit(16);
            match (hi, lo) {
                (Some(h), Some(l)) => out.push(((h << 4) | l) as u8),
                _ => return text.to_vec(), // formato inesperado → passthrough
            }
            i += 2;
        }
        if i < hex.len() {
            return text.to_vec();
        }
        out
    } else {
        text.to_vec()
    }
}

/// Mapea SQLSTATE de PG al errno MySQL que espera el C++ (`mysql_errno`;
/// los retries de `AsyncSQL.cpp:548-571` comparan errno de conexión, no de query).
pub fn map_pg_sqlstate(state: &str) -> u16 {
    match state {
        "42P01" => ER_NO_SUCH_TABLE,                   // undefined_table
        "42703" => ER_BAD_FIELD,                       // undefined_column
        "42601" => ER_PARSE_ERROR,                     // syntax_error
        "23505" => ER_DUP_ENTRY,                       // unique_violation
        "23502" => ER_BAD_NULL,                        // not_null_violation
        "23503" => 1452,                               // foreign_key_violation
        "3D000" => ER_NO_DB,                           // invalid_catalog_name
        "22007" | "22008" | "22021" => ER_WRONG_VALUE, // datetime/encoding
        "28P01" => ER_ACCESS_DENIED,                   // invalid_password
        _ => ER_UNKNOWN,
    }
}

/// Scramble aleatoria (xorshift64* sembrada con tiempo+pid). Dev tooling bound a
/// 127.0.0.1 (spec §8.2.1c) — sin dependencia de rand.
pub fn random_scramble() -> [u8; 20] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15);
    let mut s = nanos ^ (std::process::id() as u64).wrapping_mul(0x1000_0000_01b3);
    let mut out = [0u8; 20];
    for chunk in out.chunks_mut(8) {
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        let v = s.wrapping_mul(0x2545_F491_4F6C_DD1D);
        for (i, b) in chunk.iter_mut().enumerate() {
            *b = (v >> (8 * i)) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Golden: HandshakeResponse41 documentado (MySQL 5.5.8, usuario "pam").
    // Ejemplo oficial de Protocol::HandshakeResponse41:
    //   cap = 0x000fa68d (PROTOCOL_41|PLUGIN_AUTH|SECURE_CONNECTION|CONNECT_WITH_DB
    //         |MULTI_STATEMENTS|CONNECT_ATTRS|...), max_packet = 0x01000000,
    //   charset = 0x08 (latin1), user "pam", auth 20 B, db "test",
    //   plugin "mysql_native_password".
    // -----------------------------------------------------------------------
    const PAM_RESPONSE: &[u8] = &[
        0x54, 0x00, 0x00, 0x01, 0x8d, 0xa6, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x70, 0x61, 0x6d, 0x00, 0x14, 0xab, 0x09, 0xee, 0xf6,
        0xbc, 0xb1, 0x32, 0x3e, 0x61, 0x14, 0x38, 0x65, 0xc0, 0x99, 0x1d, 0x95, 0x7d, 0x75, 0xd4,
        0x47, 0x74, 0x65, 0x73, 0x74, 0x00, 0x6d, 0x79, 0x73, 0x71, 0x6c, 0x5f, 0x6e, 0x61, 0x74,
        0x69, 0x76, 0x65, 0x5f, 0x70, 0x61, 0x73, 0x73, 0x77, 0x6f, 0x72, 0x64, 0x00,
    ];

    #[test]
    fn golden_handshake_response_documented() {
        let (seq, payload) = {
            let mut packets = parse_packets(PAM_RESPONSE);
            assert_eq!(packets.len(), 1);
            packets.remove(0)
        };
        assert_eq!(seq, 1);
        let resp = decode_handshake_response(&payload).unwrap();
        assert_eq!(resp.capabilities, 0x000f_a68d);
        assert_eq!(resp.max_packet, 0x0100_0000);
        assert_eq!(resp.charset, 0x08);
        assert_eq!(resp.username, "pam");
        assert_eq!(resp.auth_response.len(), 20);
        assert_eq!(resp.database.as_deref(), Some("test"));
        assert_eq!(resp.plugin.as_deref(), Some("mysql_native_password"));
        // El ejemplo documentado marca CONNECT_ATTRS pero omite el bloque —
        // el decoder debe tolerarlo.
        assert_eq!(resp.capabilities & 0x0004_0000, 0x0004_0000);
    }

    #[test]
    fn handshake_roundtrip() {
        let mut scramble = [0u8; 20];
        for (i, b) in scramble.iter_mut().enumerate() {
            *b = i as u8;
        }
        let packet = encode_handshake(
            "5.7.44-m2-proxy",
            42,
            &scramble,
            CAP_SERVER_CAPS,
            CHARSET_UTF8MB4_GENERAL_CI,
        );
        let (seq, payload) = parse_packets(&packet)
            .into_iter()
            .next()
            .expect("un paquete");
        assert_eq!(seq, 0);
        assert_eq!(payload[0], 0x0a, "protocol version 10");
        let hs = decode_handshake_v10(&payload).unwrap();
        assert_eq!(hs.server_version, "5.7.44-m2-proxy");
        assert_eq!(hs.conn_id, 42);
        assert_eq!(hs.scramble, scramble);
        assert_eq!(hs.capabilities, CAP_SERVER_CAPS);
        assert_eq!(hs.charset, CHARSET_UTF8MB4_GENERAL_CI);
    }

    #[test]
    fn handshake_layout_official_positions() {
        // Layout oficial (Protocol::HandshakeV10): version en [0], server version
        // NUL-terminada, conn id u32 LE, part-1 8 B, filler, cap lower u16,
        // charset u8, status u16, cap upper u16, auth len u8, reserved 10, part-2.
        let packet = encode_handshake("v", 7, &[0u8; 20], CAP_SERVER_CAPS, 45);
        let (_, payload) = parse_packets(&packet).into_iter().next().unwrap();
        let version_end = 2; // "v\0"
        assert_eq!(payload[0], 0x0a);
        assert_eq!(&payload[1..version_end + 1], b"v\0");
        assert_eq!(
            u32::from_le_bytes([
                payload[version_end + 1],
                payload[version_end + 2],
                payload[version_end + 3],
                payload[version_end + 4]
            ]),
            7
        );
        let caps = CAP_SERVER_CAPS;
        let cap_lower_pos = version_end + 14;
        assert_eq!(
            u16::from_le_bytes([payload[cap_lower_pos], payload[cap_lower_pos + 1]]),
            (caps & 0xffff) as u16
        );
        assert_eq!(
            u16::from_le_bytes([payload[cap_lower_pos + 5], payload[cap_lower_pos + 6]]),
            (caps >> 16) as u16
        );
        assert_eq!(payload[cap_lower_pos + 7], 21, "auth_plugin_data_len");
    }

    #[test]
    fn lenenc_boundaries() {
        assert_eq!(lenenc_int(0), vec![0x00]);
        assert_eq!(lenenc_int(250), vec![0xfa]);
        assert_eq!(lenenc_int(251), vec![0xfc, 0xfb, 0x00]);
        assert_eq!(lenenc_int(0xffff), vec![0xfc, 0xff, 0xff]);
        assert_eq!(lenenc_int(0x10000), vec![0xfd, 0x00, 0x00, 0x01]);
        assert_eq!(lenenc_int(0xff_ffff), vec![0xfd, 0xff, 0xff, 0xff]);
        assert_eq!(
            lenenc_int(0x0100_0000),
            vec![0xfe, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(lenenc_bytes(b"ab"), vec![0x02, b'a', b'b']);
    }

    #[test]
    fn ok_err_eof_shapes() {
        let ok = encode_ok(1, 5, 42, STATUS_AUTOCOMMIT);
        let (seq, payload) = parse_packets(&ok).into_iter().next().unwrap();
        assert_eq!(seq, 1);
        assert_eq!(payload[0], 0x00);
        assert_eq!(payload[1], 5, "affected lenenc");
        assert_eq!(payload[2], 42, "insert_id lenenc");
        assert_eq!(
            u16::from_le_bytes([payload[3], payload[4]]),
            STATUS_AUTOCOMMIT
        );

        let err = encode_err(1, ER_NO_SUCH_TABLE, "42P01", "no such table: x");
        let (_, payload) = parse_packets(&err).into_iter().next().unwrap();
        assert_eq!(payload[0], 0xff);
        assert_eq!(
            u16::from_le_bytes([payload[1], payload[2]]),
            ER_NO_SUCH_TABLE
        );
        assert_eq!(&payload[3..9], b"#42P01");
        assert_eq!(&payload[9..], b"no such table: x");

        let eof = encode_eof(2, STATUS_AUTOCOMMIT | STATUS_MORE_RESULTS);
        let (seq, payload) = parse_packets(&eof).into_iter().next().unwrap();
        assert_eq!(seq, 2);
        assert_eq!(payload[0], 0xfe);
        assert_eq!(
            u16::from_le_bytes([payload[3], payload[4]]),
            STATUS_AUTOCOMMIT | STATUS_MORE_RESULTS
        );
    }

    #[test]
    fn result_set_with_blob_and_null() {
        let col = ColumnDef {
            name: "name".into(),
            schema: String::new(),
            table: String::new(),
            charset: CHARSET_BINARY,
            column_length: 65_535,
            type_code: MYSQL_TYPE_BLOB,
            flags: BLOB_FLAG | BINARY_FLAG | NOT_NULL_FLAG,
            decimals: 0,
        };
        let rows = vec![
            vec![Some(vec![0x62, 0x65, 0x00, 0xff])], // bytes crudos con NUL
            vec![None],                               // NULL = 0xfb
        ];
        let packets = encode_result_set(1, &[col], &rows, false);
        // 1 (count) + 1 (def) + 1 (EOF defs) + 2 (rows) + 1 (EOF rows) = 6
        assert_eq!(packets.len(), 6);
        // column count
        let (_, p0) = parse_packets(&packets[0]).into_iter().next().unwrap();
        assert_eq!(p0, vec![0x01]);
        // column def. Layout fijo de ColumnDefinition41: "def"(4) + schema/
        // table/org_table(3×lenenc-1) + name/org_name(2×(1+4)) = 17;
        // lenenc 0x0c (1) = 18; charset u16 = 20; length u32 = 24; type (1);
        // flags u16 = 27; decimals (1) = 28; filler u16.
        let (_, p1) = parse_packets(&packets[1]).into_iter().next().unwrap();
        assert_eq!(p1[0], 0x03, "lenenc 'def'");
        assert_eq!(u16::from_le_bytes([p1[18], p1[19]]), CHARSET_BINARY as u16);
        assert_eq!(p1[24], MYSQL_TYPE_BLOB, "type en offset fijo");
        assert_eq!(
            u16::from_le_bytes([p1[25], p1[26]]),
            BLOB_FLAG | BINARY_FLAG | NOT_NULL_FLAG
        );
        // EOF tras defs
        let (_, p2) = parse_packets(&packets[2]).into_iter().next().unwrap();
        assert_eq!(p2[0], 0xfe);
        // fila 1 (text row: SOLO celdas lenenc, sin contador — spec
        // ProtocolText::ResultsetRow): lenenc len + bytes.
        let (_, p3) = parse_packets(&packets[3]).into_iter().next().unwrap();
        assert_eq!(p3[0], 0x04, "len de la celda");
        assert_eq!(&p3[1..], &[0x62, 0x65, 0x00, 0xff]);
        // fila 2: NULL = 0xfb
        let (_, p4) = parse_packets(&packets[4]).into_iter().next().unwrap();
        assert_eq!(p4, vec![0xfb]);
        // EOF final sin MORE_RESULTS
        let (_, p5) = parse_packets(&packets[5]).into_iter().next().unwrap();
        assert_eq!(p5[0], 0xfe);
        assert_eq!(u16::from_le_bytes([p5[3], p5[4]]), STATUS_AUTOCOMMIT);
    }

    /// Regresión del gate: 2 columnas (varchar + bytea) — el cliente leía el
    /// contador de celdas como length del primer valor (ERR 2000 / valor 0x04).
    #[test]
    fn result_set_two_columns_roundtrip() {
        let cols = vec![
            ColumnDef {
                name: "mValue".into(),
                schema: String::new(),
                table: String::new(),
                charset: CHARSET_UTF8MB4_GENERAL_CI,
                column_length: 255,
                type_code: MYSQL_TYPE_VAR_STRING,
                flags: 0,
                decimals: 0,
            },
            ColumnDef {
                name: "mKey".into(),
                schema: String::new(),
                table: String::new(),
                charset: CHARSET_BINARY,
                column_length: 65_535,
                type_code: MYSQL_TYPE_BLOB,
                flags: BLOB_FLAG | BINARY_FLAG,
                decimals: 0,
            },
        ];
        let rows = vec![vec![Some(b"ab".to_vec()), Some(vec![0xde, 0xad])]];
        let packets = encode_result_set(1, &cols, &rows, false);
        assert_eq!(packets.len(), 6); // count + 2 defs + EOF + 1 row + EOF
        // fila: lenenc "ab" + lenenc 0xdead — SIN contador de celdas.
        let (_, row) = parse_packets(&packets[4]).into_iter().next().unwrap();
        assert_eq!(row, vec![0x02, b'a', b'b', 0x02, 0xde, 0xad]);
    }

    /// Regresión del gate: valor numérico de 4 bytes ("2864") — el cliente debe
    /// leer length=0x04 y el texto completo, nunca el byte 0x04 como valor.
    #[test]
    fn result_set_numeric_value_roundtrip() {
        let cols = vec![ColumnDef {
            name: "count".into(),
            schema: String::new(),
            table: String::new(),
            charset: CHARSET_UTF8MB4_GENERAL_CI,
            column_length: 255,
            type_code: MYSQL_TYPE_VAR_STRING,
            flags: 0,
            decimals: 0,
        }];
        let rows = vec![vec![Some(b"2864".to_vec())]];
        let packets = encode_result_set(1, &cols, &rows, false);
        let (_, row) = parse_packets(&packets[3]).into_iter().next().unwrap();
        assert_eq!(row, vec![0x04, b'2', b'8', b'6', b'4']);
    }

    /// Celdas >250 bytes: lenenc de 2/3 bytes (0xfc/0xfd); el caso 0xfe (8B)
    /// está cubierto por `lenenc_boundaries` (>= 0x1000000).
    #[test]
    fn result_set_large_cells_lenenc() {
        let cols = vec![ColumnDef {
            name: "v".into(),
            schema: String::new(),
            table: String::new(),
            charset: CHARSET_UTF8MB4_GENERAL_CI,
            column_length: 255,
            type_code: MYSQL_TYPE_VAR_STRING,
            flags: 0,
            decimals: 0,
        }];
        let big300 = vec![0x41u8; 300];
        let big65536 = vec![0x42u8; 65_536];
        let rows = vec![vec![Some(big300.clone())], vec![Some(big65536.clone())]];
        let packets = encode_result_set(1, &cols, &rows, false);
        let (_, row1) = parse_packets(&packets[3]).into_iter().next().unwrap();
        assert_eq!(&row1[..3], &[0xfc, 0x2c, 0x01], "lenenc 300 = fc 2c 01");
        assert_eq!(&row1[3..], &big300);
        let (_, row2) = parse_packets(&packets[4]).into_iter().next().unwrap();
        assert_eq!(
            &row2[..4],
            &[0xfd, 0x00, 0x00, 0x01],
            "lenenc 65536 = fd 00 00 01"
        );
        assert_eq!(row2.len(), 4 + 65_536);
    }

    #[test]
    fn result_set_marks_more_results() {
        let packets = encode_result_set(1, &[], &[], true);
        let (_, last) = parse_packets(packets.last().unwrap())
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(
            u16::from_le_bytes([last[3], last[4]]),
            STATUS_AUTOCOMMIT | STATUS_MORE_RESULTS
        );
        let ok = encode_ok(1, 1, 0, STATUS_AUTOCOMMIT | STATUS_MORE_RESULTS);
        let (_, p) = parse_packets(&ok).into_iter().next().unwrap();
        assert_eq!(
            u16::from_le_bytes([p[3], p[4]]),
            STATUS_AUTOCOMMIT | STATUS_MORE_RESULTS
        );
    }

    #[test]
    fn scramble_response_documented_vector() {
        // Scramble del ejemplo clásico de la documentación MySQL (native password
        // auth); token calculado de forma independiente con .NET SHA1 (2026-08-10).
        let scramble: &[u8] = &[
            0x3d, 0x67, 0x0c, 0x3f, 0x2e, 0x2c, 0x2b, 0x20, 0x3a, 0x30, 0x2c, 0x39, 0x29, 0x24,
            0x22, 0x33, 0x2e, 0x2f, 0x31, 0x24,
        ];
        let expected: &[u8] = &[
            0x8b, 0x38, 0x55, 0x12, 0xca, 0xa1, 0xcb, 0xf6, 0xff, 0xa0, 0xf2, 0x1a, 0xc9, 0x43,
            0x73, 0x3c, 0x46, 0x53, 0xb7, 0xc0,
        ];
        assert_eq!(scramble_response(b"foo", scramble), expected);
    }

    #[test]
    fn validate_native_auth_ok_and_fail() {
        let scramble = random_scramble();
        let token = scramble_response(b"1234", &scramble);
        assert!(validate_native_auth(b"1234", &scramble, &token));
        assert!(!validate_native_auth(b"wrong", &scramble, &token));
        assert!(!validate_native_auth(b"1234", &scramble, &token[..19]));
    }

    #[test]
    fn decode_commands() {
        assert_eq!(decode_command(&[COM_QUIT]), Ok(ClientCommand::Quit));
        assert_eq!(decode_command(&[COM_PING]), Ok(ClientCommand::Ping));
        assert_eq!(
            decode_command(&[COM_QUERY, b'S', b'E', b'L', b'E', b'C', b'T', b' ', b'1']),
            Ok(ClientCommand::Query("SELECT 1".into()))
        );
        assert_eq!(decode_command(&[0x16]), Ok(ClientCommand::Unknown(0x16)));
        assert_eq!(decode_command(&[]), Err(WireError::EmptyPacket));
        // COM_QUERY con bytes no-UTF8 → error explícito (nunca corrupción).
        assert_eq!(
            decode_command(&[COM_QUERY, 0xff, 0xfe]),
            Err(WireError::NonUtf8Query)
        );
    }

    #[test]
    fn column_parts_mapping() {
        let (t, c, _l, f) = mysql_column_parts(17);
        assert_eq!((t, c), (MYSQL_TYPE_BLOB, CHARSET_BINARY));
        assert!(f & BLOB_FLAG != 0);
        let (t, _, _, f) = mysql_column_parts(23);
        assert_eq!(t, MYSQL_TYPE_LONG);
        assert!(f & NUM_FLAG != 0);
        let (t, c, _, f) = mysql_column_parts(1043);
        assert_eq!((t, c), (MYSQL_TYPE_VAR_STRING, CHARSET_UTF8MB4_GENERAL_CI));
        assert_eq!(f, 0);
        let (t, _, _, _) = mysql_column_parts(1114);
        assert_eq!(t, MYSQL_TYPE_DATETIME);
        let (t, _, _, _) = mysql_column_parts(1186);
        assert_eq!(t, MYSQL_TYPE_STRING);
    }

    #[test]
    fn bytea_hex_decode_roundtrip() {
        let raw = [0x62, 0x65, 0x00, 0xff, 0x80];
        let text = format!(
            "\\x{}",
            raw.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        assert_eq!(decode_bytea_text(text.as_bytes()), raw);
        // Sin prefijo \x → passthrough.
        assert_eq!(decode_bytea_text(b"plain"), b"plain");
        // Hex impar → passthrough defensivo.
        assert_eq!(decode_bytea_text(b"\\xabc"), b"\\xabc");
    }

    /// Regresión del bug crítico 2026-08-10: el blob real de carga de personaje
    /// (skill_level, 192 bytes de ceros) llega de PG como `\x` + 384 hex chars
    /// y debe decodificarse a los 192 bytes crudos — nunca servirse como texto.
    #[test]
    fn bytea_full_blob_hex_decodes_to_raw_bytes() {
        let raw = vec![0x00u8; 192];
        let hex: String = raw.iter().map(|b| format!("{b:02x}")).collect();
        let text = format!("\\x{hex}");
        assert_eq!(text.len(), 386, "\\x + 384 hex chars");
        assert_eq!(decode_bytea_text(text.as_bytes()), raw);
        assert_ne!(
            decode_bytea_text(text.as_bytes()),
            text.as_bytes(),
            "nunca el texto hex como valor"
        );
    }

    #[test]
    fn pg_sqlstate_mapping() {
        assert_eq!(map_pg_sqlstate("42P01"), ER_NO_SUCH_TABLE);
        assert_eq!(map_pg_sqlstate("42703"), ER_BAD_FIELD);
        assert_eq!(map_pg_sqlstate("23505"), ER_DUP_ENTRY);
        assert_eq!(map_pg_sqlstate("42601"), ER_PARSE_ERROR);
        assert_eq!(map_pg_sqlstate("zzzzz"), ER_UNKNOWN);
    }

    #[test]
    fn scramble_is_20_bytes_and_varies() {
        let a = random_scramble();
        let b = random_scramble();
        assert_eq!(a.len(), 20);
        assert_ne!(a, b, "dos llamadas no deben repetir la scramble");
    }
}
