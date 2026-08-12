//! F2a — rol `auth`: primer servidor REAL del rewrite — login del cliente
//! legacy contra PostgreSQL (ROADMAP §F2a; G-PG cerrado).
//!
//! Paridad con el auth C++ (`input_auth.cpp` + `input_db.cpp:1697-1728`):
//!
//! 1. Handshake server-side (`network::handshake` — F1.5, validado contra el
//!    auth real en F1.6): `GC_PHASE` + `GC_HANDSHAKE` → eco `CG_HANDSHAKE`
//!    (nonce, bias ≤ 80 ms, retries).
//! 2. `CG_LOGIN3` (68 B al auth: 65 + `szLanguage[3]`; el framer con rol Auth
//!    ya entrega 68).
//! 3. Validaciones en orden (parity `input_auth.cpp:66-152`):
//!    - login string inválido → `GC_LOGIN_FAILURE "NOID"`;
//!    - `no_more_clients` → `GC_LOGIN_FAILURE "SHUTDOWN"`;
//!    - ya logueado → `GC_LOGIN_FAILURE "ALREADY"`;
//!    - `lang` válido → `UPDATE account.account SET lang` (ANTES de la
//!      validación, parity `input_auth.cpp:133-152`);
//!    - credenciales → `GC_AUTH_SUCCESS` con `bResult` 0/1 y `dwLoginKey`
//!      (¡el C++ NUNCA manda GC_LOGIN_FAILURE por password mala — manda
//!      GC_AUTH_SUCCESS con bResult=0 y key=0, `input_db.cpp:1719-1726`!).
//! 4. La key: `number(1, INT_MAX)` única por login (`desc_manager.cpp
//!    CreateLoginKey`); `dwPanamaKey = dwKey ^ adwClientKey[0..3]` (parity
//!    `input_auth.cpp:154-156`).
//! 5. Timeout global por conexión (deuda F1.5): una conexión silenciosa no
//!    puede vivir los ~17.6 s de retries del handshake — el config lo acota.
//!
//! Pendiente documentado (NO bloquea el hito): en login exitoso el C++ envía
//! ANTES de `GC_AUTH_SUCCESS` los paquetes PanamaPack 151 + hybrid-crypt
//! 152/153 (legacy-client-only — `protocol::legacy`, ADR-0006) — el auth Rust
//! no los envía todavía; riesgo del test híbrido documentado en el reporte.
//!
//! F5 (2026-08-11): en login exitoso el auth envía además el `GC_CHANNEL_LIST`
//! (164) — la lista de canales + manifest (rates exp/gold/drop) del config
//! (`channels`/`exp_rate`/`gold_rate`/`drop_rate`): el cliente conecta al
//! canal con ESTA lista (adiós al IP bakeado de serverinfo.py, ROADMAP F5).
//!
//! Driver de DB: **tokio-postgres** (decisión F2a, ver `lib.rs`).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use network::framer::{ConnectionRole, Framer};
use network::{handshake, Connection};
use protocol::legacy;
use protocol::locale::{encode_chunks, encode_payload, CgLocaleRequest};
use protocol::{header, phase, TPacketCGLogin3, TPacketGCAuthSuccess, TPacketGCLoginFailure, TPacketGCPhase};
use tokio::net::{TcpListener, TcpStream};

use crate::config::{ChannelCfg, Config};
// F3 (ADR-0008): consolidación — el SQL inline vive en el crate database.
use database::account::{hex16, AccountRepo};
// F1 (ADR-0009): el locale server-side se lee de PG con fallback EN.
use database::locale::LocaleRepo;

/// F1 — bytes de chunk por paquete wire del `GC_LOCALE` (spec: cada paquete
/// con payload ≤ 64.000 B → wire de 64.004 B; el buffer completo son ~1-2 MB
/// y excede u16, por eso va chunked).
pub const GC_LOCALE_MAX_CHUNK: usize = 64_000;

/// Datos legacy cargados del runtime (parity del cwd del auth C++): vacíos =
/// no se envían (el runtime srv1 actual no tiene los archivos).
#[derive(Debug, Clone, Default)]
pub struct LegacyData {
    pub panama: Vec<legacy::PanamaEntry>,
    pub hybrid: legacy::HybridData,
}

impl LegacyData {
    /// Carga `panama/` + `cshybridcrypt*` de `dir` (vacío = sin legacy).
    pub fn load(dir: &str) -> Self {
        if dir.is_empty() {
            return Self::default();
        }
        let dir = std::path::Path::new(dir);
        Self {
            panama: legacy::load_panama(&dir.join("panama")),
            hybrid: legacy::load_hybrid(dir),
        }
    }
}

// ---------------------------------------------------------------------------
// F5 — GC_CHANNEL_LIST (164): lista de canales + manifest (rates) desde el
// auth (ROADMAP F5 "channel list from the auth — goodbye baked IP").
//
// Wire (little-endian, packed, tamaño FIJO 152 B):
//   BYTE header (164); BYTE count; WORD wExpRate; WORD wGoldRate;
//   WORD wDropRate; TChannelInfo aChannels[4] (36 B c/u) — slots no usados
//   (>= count) a cero. TChannelInfo = char szName[16]; char szIP[16];
//   WORD wPort; WORD wPlayers.
//
// Tamaño fijo a propósito (ponytail): el cliente legacy registra el paquete
// como STATIC_SIZE en `CMainPacketHeaderMap` y `__AnalyzePacket` despacha
// solo con el tamaño completo en el buffer — un array variable rompería esa
// garantía (race de paquete parcial → login roto). `count` es semántico: el
// cliente procesa solo las `count` primeras entradas.
// ---------------------------------------------------------------------------
pub const GC_CHANNEL_LIST: u8 = 164;
/// Slots del wire (fijos). El runtime srv1 tiene 4 canales (30003-30015).
pub const GC_CHANNEL_LIST_MAX_CHANNELS: usize = 4;
/// Tamaño total del paquete: header + count + rates(6) + 4×36 = 152 B.
pub const GC_CHANNEL_LIST_SIZE: usize = 1 + 1 + 6 + GC_CHANNEL_LIST_MAX_CHANNELS * 36;

/// Serializa el `GC_CHANNEL_LIST` (152 B). `channels` se trunca a 4; los
/// strings a 15 bytes + NUL (parity de los buffers `char[16]` del cliente).
pub fn encode_channel_list(channels: &[ChannelCfg], exp_rate: u16, gold_rate: u16, drop_rate: u16) -> [u8; GC_CHANNEL_LIST_SIZE] {
    let mut b = [0u8; GC_CHANNEL_LIST_SIZE];
    let count = channels.len().min(GC_CHANNEL_LIST_MAX_CHANNELS);
    b[0] = GC_CHANNEL_LIST;
    b[1] = count as u8;
    b[2..4].copy_from_slice(&exp_rate.to_le_bytes());
    b[4..6].copy_from_slice(&gold_rate.to_le_bytes());
    b[6..8].copy_from_slice(&drop_rate.to_le_bytes());
    for (i, ch) in channels.iter().take(count).enumerate() {
        let base = 8 + i * 36;
        let name = ch.name.as_bytes();
        let n = name.len().min(15);
        b[base..base + n].copy_from_slice(&name[..n]);
        let ip = ch.ip.as_bytes();
        let m = ip.len().min(15);
        b[base + 16..base + 16 + m].copy_from_slice(&ip[..m]);
        b[base + 32..base + 34].copy_from_slice(&ch.port.to_le_bytes());
        b[base + 34..base + 36].copy_from_slice(&ch.players.to_le_bytes());
    }
    b
}

/// F5 — envía el `GC_CHANNEL_LIST` (164) con la lista de canales + manifest
/// del config. Solo en login exitoso; con `channels` vacío no se envía nada
/// (parity de comportamiento con el auth C++: no manda paquetes nuevos).
async fn send_channel_list<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    conn: &mut Connection<S>,
    config: &Config,
) -> Result<(), String> {
    if config.channels.is_empty() {
        return Ok(());
    }
    eprintln!(
        "server_realms: auth: F5 GC_CHANNEL_LIST: {} canal(es), rates exp {} gold {} drop {}",
        config.channels.len(),
        config.exp_rate,
        config.gold_rate,
        config.drop_rate
    );
    conn.send(&encode_channel_list(
        &config.channels,
        config.exp_rate,
        config.gold_rate,
        config.drop_rate,
    ))
    .await
    .map_err(|e| format!("enviando GC_CHANNEL_LIST: {e}"))
}

/// Servidor auth: listener + tarea por conexión (patrón `network::serve`).
pub async fn run(config: Config) -> std::io::Result<()> {
    let listener = TcpListener::bind(&config.listen).await?;
    // El puerto real (relevante con `listen = "127.0.0.1:0"` en tests).
    println!("server_realms: auth escuchando en {}", listener.local_addr()?);
    let legacy = LegacyData::load(&config.legacy_dir);
    if !legacy.panama.is_empty() || !legacy.hybrid.keys_stream.is_empty() {
        eprintln!(
            "server_realms: auth: legacy cargado (panama {} entradas, hybrid keys {} B)",
            legacy.panama.len(),
            legacy.hybrid.keys_stream.len()
        );
    }
    let mut conn_id: u32 = 1;
    loop {
        let (stream, _peer) = listener.accept().await?;
        let cfg = config.clone();
        let legacy = legacy.clone();
        let id = conn_id;
        conn_id = conn_id.wrapping_add(1);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, cfg, legacy, id).await {
                eprintln!("server_realms: auth conn {id}: {e}");
            }
        });
    }
}

/// Conexión auth con timeout global del intento (deuda F1.5).
async fn handle_connection(
    stream: TcpStream,
    config: Config,
    legacy: LegacyData,
    conn_id: u32,
) -> Result<(), String> {
    match tokio::time::timeout(config.timeout, connection_inner(stream, &config, &legacy)).await {
        Err(_) => Err(format!(
            "auth conn {conn_id}: timeout global de {} ms — conexión cerrada",
            config.timeout.as_millis()
        )),
        Ok(r) => r,
    }
}

async fn connection_inner(stream: TcpStream, config: &Config, legacy: &LegacyData) -> Result<(), String> {
    let mut conn = Connection::new(stream);
    let mut framer = Framer::new(ConnectionRole::Auth);

    // 1. Handshake server-side (F1.5, validado contra el auth real en F1.6).
    let hs = handshake::perform(&mut conn, &mut framer, now_ms())
        .await
        .map_err(|e| format!("handshake: {e}"))?;
    eprintln!("server_realms: auth: handshake OK (delta {} ms)", hs.delta);

    // 1b. El cliente manda el LOGIN3 al recibir GC_PHASE(PHASE_AUTH)
    //     (AccountConnector.cpp `__AuthState_RecvPhase`); el C++ hace
    //     `SetPhase(PHASE_AUTH)` tras validar el eco (input.cpp
    //     `CInputProcessor::Handshake`). SIN este paquete el cliente espera
    //     hasta el timeout — el bug del test híbrido (2026-08-11).
    conn.send(&TPacketGCPhase::new(phase::AUTH).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_PHASE(AUTH): {e}"))?;
    eprintln!("server_realms: auth: enviado GC_PHASE(PHASE_AUTH)");

    // 2. LOGIN3 (68 B al auth — el framer con rol Auth ya entrega 68).
    //    F1: el cliente pide el locale AL CONECTAR (antes del LOGIN3) — el
    //    auth responde GC_LOCALE chunked y sigue esperando el login.
    let login3 = loop {
        let pkt = framer
            .next_packet(&mut conn)
            .await
            .map_err(|e| format!("framer: {e}"))?;
        match pkt[0] {
            header::CG_TIME_SYNC | header::CG_PONG => continue, // keepalives (F1.4)
            header::CG_LOGIN3 => {
                break TPacketCGLogin3::from_bytes(&pkt).map_err(|e| format!("LOGIN3: {e}"))?
            }
            header::CG_LOCALE_REQUEST => {
                handle_locale_request(&mut conn, config, &pkt).await?;
                continue;
            }
            other => {
                return Err(format!(
                    "auth: header inesperado 0x{other:02x} tras el handshake (parity input_auth.cpp:251-253)"
                ))
            }
        }
    };
    let login = normalize_login(&login3.login);
    let passwd = cstr(&login3.passwd).to_string();
    eprintln!(
        "server_realms: auth: LOGIN3 login={login} lang={:?} version={:?} hwid={:?}",
        extract_lang(&login3.sz_language),
        login3.version,
        login3.hwid.as_ref().map(|h| hex16(h))
    );

    // 2b. F2b: version gate — si el cliente manda `version` (72/88 B) y no
    //     coincide con la esperada → cierre limpio con log (el C++ no tiene
    //     version check en input_auth.cpp; decisión: sin GC_LOGIN_FAILURE —
    //     no hay status legacy para versión mala y un status arbitrario
    //     confundiría al cliente; el cliente nuevo maneja el EOF).
    if let Some(v) = login3.version {
        if v != config.expected_version {
            eprintln!(
                "server_realms: auth: VERSION MISMATCH login={login} got={v} expected={} — cierre limpio",
                config.expected_version
            );
            return Ok(());
        }
    }

    // 3. Validaciones (parity input_auth.cpp:66-152).
    if !is_valid_login_string(&login) {
        send_login_failure(&mut conn, "NOID").await?;
        return Ok(());
    }
    if config.no_more_clients {
        send_login_failure(&mut conn, "SHUTDOWN").await?;
        return Ok(());
    }
    let Some(_guard) = ActiveLoginGuard::acquire(&login) else {
        send_login_failure(&mut conn, "ALREADY").await?;
        return Ok(());
    };

    // 4. PG: validación de credenciales vía AccountRepo (F3/ADR-0008 — las
    //    queries viven en el crate database). Los errores de DB NO abortan la
    //    conexión: login denegado (bResult=0) con log — respuesta determinista
    //    para el cliente (el C++ con el db caído no puede servir; el Rust
    //    degrada a denegación, parity de resultado observable input_db.cpp:1719).
    let ok = match account_login(
        &config.pg_conn,
        &login,
        &passwd,
        extract_lang(&login3.sz_language),
        login3.hwid,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("server_realms: auth: PG falló para {login}: {e} — bResult=0");
            false
        }
    };

    // 7. Key (parity CreateLoginKey: única, 1..INT_MAX) + panama key (parity
    //    input_auth.cpp:154-156 — el 151 la usa para XOR-ear los IVs).
    let key = if ok { unique_login_key() } else { 0 };
    let panama_key = key
        ^ login3.adw_client_key[0]
        ^ login3.adw_client_key[1]
        ^ login3.adw_client_key[2]
        ^ login3.adw_client_key[3];
    if ok {
        eprintln!("server_realms: auth: login OK {login} key {key} panama 0x{panama_key:08x}");
    } else {
        eprintln!("server_realms: auth: login FALLIDO {login} (bResult=0, parity input_db.cpp:1719-1726)");
    }

    // 8. Respuesta: el C++ SIEMPRE responde GC_AUTH_SUCCESS (bResult 0/1) para
    //    credenciales; GC_LOGIN_FAILURE solo para NOID/ALREADY/SHUTDOWN.
    //    En login exitoso, ANTES del GC_AUTH_SUCCESS: PanamaPack 151 +
    //    hybrid-crypt 152/153 (parity input_db.cpp:1710-1716 — el runtime
    //    actual no tiene los archivos → no se envían, igual que el auth C++)
    //    y el GC_CHANNEL_LIST (164, F5 — lista de canales + manifest).
    //    NOTA de orden (F5): el 164 va ANTES del 150 en el byte stream — el
    //    cliente legacy consume UN paquete por frame en la fase auth y
    //    desconecta del auth al despachar el 150 (`__AuthState_RecvAuthSuccess`
    //    → `Disconnect()`); con el 164 después, nunca lo leería. Aditivo y
    //    solo en login exitoso: el wire del LOGIN3/auth no cambia.
    if ok {
        send_legacy_packets(&mut conn, legacy, panama_key).await?;
        send_channel_list(&mut conn, config).await?;
        conn.send(&TPacketGCAuthSuccess::new(key, 1).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_AUTH_SUCCESS: {e}"))?;
    } else {
        conn.send(&TPacketGCAuthSuccess::new(0, 0).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_AUTH_SUCCESS: {e}"))?;
    }
    Ok(())
}

/// Envía los paquetes legacy (151 × entradas panama, 152 keys, 153 SDB del
/// mapa "none" = MAPNAME_DEFAULT, input_db.cpp:46) — parity
/// `input_db.cpp:1710-1716` (en orden: Panama, Keys, SDB, luego el caller
/// envía GC_AUTH_SUCCESS).
async fn send_legacy_packets<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    conn: &mut Connection<S>,
    legacy: &LegacyData,
    panama_key: u32,
) -> Result<(), String> {
    for entry in &legacy.panama {
        conn.send(&legacy::PanamaPack::encode(&entry.name, entry.iv, panama_key))
            .await
            .map_err(|e| format!("enviando 151 (panama): {e}"))?;
    }
    if !legacy.hybrid.keys_stream.is_empty() {
        conn.send(&legacy::HybridCryptKeys::new(legacy.hybrid.keys_stream.clone()).to_bytes())
            .await
            .map_err(|e| format!("enviando 152 (hybrid keys): {e}"))?;
    }
    if let Some(sdb) = legacy.hybrid.sdb.get("none").filter(|s| !s.is_empty()) {
        conn.send(&legacy::PackageSDB::new(sdb.clone()).to_bytes())
            .await
            .map_err(|e| format!("enviando 153 (sdb): {e}"))?;
    }
    Ok(())
}

async fn send_login_failure(conn: &mut Connection<TcpStream>, status: &str) -> Result<(), String> {
    eprintln!("server_realms: auth: GC_LOGIN_FAILURE {status}");
    conn.send(&TPacketGCLoginFailure::new(status).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_LOGIN_FAILURE: {e}"))
}

/// F1 (ADR-0009) — `CG_LOCALE_REQUEST` (132): lee el bundle del idioma de PG
/// (con fallback EN) y responde `GC_LOCALE` (140) chunked (payload ≤
/// 64.000 B por paquete wire). Lang inválido (no 2 letras + NUL) → cierre
/// limpio con log (parity `extract_lang` del LOGIN3 — input_auth.cpp:119-131).
/// Stateless: el flujo de login sigue intacto (el caller continúa el loop).
async fn handle_locale_request<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    conn: &mut Connection<S>,
    config: &Config,
    pkt: &[u8],
) -> Result<(), String> {
    let req = CgLocaleRequest::from_bytes(pkt).map_err(|e| format!("CG_LOCALE_REQUEST: {e}"))?;
    let Some(lang) = extract_lang(&req.lang) else {
        return Err(format!(
            "auth: CG_LOCALE_REQUEST con lang inválido {:?} — cierre limpio",
            req.lang
        ));
    };
    eprintln!("server_realms: auth: CG_LOCALE_REQUEST lang={lang}");
    let bundle = LocaleRepo::new(&config.pg_conn)
        .load_for_lang(&lang)
        .await
        .map_err(|e| format!("locale {lang}: {e}"))?;
    let payload = encode_payload(&bundle);
    let chunks = encode_chunks(&payload, GC_LOCALE_MAX_CHUNK);
    eprintln!(
        "server_realms: auth: GC_LOCALE {lang}: mob {} item {} item_desc {} skill {} map {} ui {} ({} B, {} chunks)",
        bundle.mob.len(),
        bundle.item.len(),
        bundle.item_desc.len(),
        bundle.skill.len(),
        bundle.map.len(),
        bundle.ui.len(),
        payload.len(),
        chunks.len(),
    );
    for chunk in chunks {
        conn.send(&chunk)
            .await
            .map_err(|e| format!("enviando GC_LOCALE: {e}"))?;
    }
    Ok(())
}

/// Validación contra PG vía `AccountRepo` (F3/ADR-0008 — consolidación: el
/// SQL inline se movió al crate database; `hex16`/`mysql5_password` viven en
/// `database::account`). Orden parity `input_auth.cpp`:
/// (a) `set_lang` si el LOGIN3 trae lang válido — ANTES de validar
///     (input_auth.cpp:133-152);
/// (b) F2b: `set_hwid` si el LOGIN3 trae hwid — la columna la crea otro lane;
///     si aún no existe (42703, incluido en el error por `pg_err`) se loguea
///     y el login sigue (nunca romper el login por la hwid);
/// (c) `login` (QUERY_LOGIN — 13 columnas, hash MySQL calculado en Rust;
///     parity `utils.cpp:30-58`; el auth C++ NO llama a la función PG).
/// Los errores de DB se propagan (`Err` → el caller degrada a bResult=0).
async fn account_login(
    pg_conn: &str,
    login: &str,
    passwd: &str,
    lang: Option<String>,
    hwid: Option<[u8; 16]>,
) -> Result<bool, String> {
    let repo = AccountRepo::new(pg_conn);
    if let Some(lang) = lang {
        repo.set_lang(login, &lang).await?;
    }
    if let Some(hwid) = hwid {
        // F2b fix: el parámetro es el hex como TEXT (32 chars, cabe en
        // VARCHAR(64)). `[u8;16]` NO implementa ToSql ("error serializing
        // parameter 0") y `Vec<u8>`/bytea da 42804 contra VARCHAR.
        let hwid_hex = hex16(&hwid);
        match repo.set_hwid(login, &hwid_hex).await {
            Ok(_) => {}
            Err(e) => {
                if e.contains("42703") {
                    eprintln!("server_realms: auth: columna hwid aún no existe (42703, la crea otro lane) — login sigue");
                } else {
                    eprintln!("server_realms: auth: UPDATE hwid falló para {login}: {e} — login sigue");
                }
            }
        }
    }
    Ok(repo.login(login, passwd).await?.is_some())
}

// ---------------------------------------------------------------------------
// Funciones puras (testeables sin red)
// ---------------------------------------------------------------------------

/// `FN_IS_VALID_LOGIN_STRING` (input_auth.cpp:13-53): ≥2 chars, solo
/// alfanuméricos (sin `ENABLE_ACCOUNT_W_SPECIALCHARS`).
pub fn is_valid_login_string(login: &str) -> bool {
    if login.len() < 2 {
        return false;
    }
    login.chars().all(|c| c.is_ascii_alphanumeric())
}

/// `trim_and_lower` (input_auth.cpp:79): C-string → lowercase + trim.
pub fn normalize_login(raw: &[u8; 31]) -> String {
    cstr(raw).trim().to_ascii_lowercase()
}

/// Extrae `szLanguage[3]` del LOGIN3 auth: 2 letras (case-insensitive) + NUL →
/// lowercase ("es"). `None` si no es el formato del cliente recompilado
/// (parity input_auth.cpp:119-131).
pub fn extract_lang(sz_language: &[u8; 3]) -> Option<String> {
    let a0 = sz_language[0];
    let a1 = sz_language[1];
    let is_alpha = |c: u8| c.is_ascii_alphabetic();
    if is_alpha(a0) && is_alpha(a1) && sz_language[2] == 0 {
        Some(format!(
            "{}{}",
            (a0 as char).to_ascii_lowercase(),
            (a1 as char).to_ascii_lowercase()
        ))
    } else {
        None
    }
}

/// C-string → `&str` (hasta el primer NUL; bytes no-UTF8 → vacío defensivo).
fn cstr(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// `now_ms` — reloj del servidor en ms desde boot (parity `get_dword_time`,
/// `utils.c:445`); el handshake lo usa como `dwTime` (el cliente alinea su
/// reloj con él).
fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

// ---------------------------------------------------------------------------
// Estado del auth (por proceso)
// ---------------------------------------------------------------------------

/// Logins con sesión activa (parity `DESC_MANAGER::FindByLoginName`,
/// input_auth.cpp:107-111) — el guard libera al cerrar la conexión.
fn active_logins() -> &'static Mutex<HashSet<String>> {
    static M: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashSet::new()))
}

struct ActiveLoginGuard {
    login: String,
}

impl ActiveLoginGuard {
    fn acquire(login: &str) -> Option<Self> {
        let mut set = active_logins().lock().expect("active_logins lock");
        if set.contains(login) {
            return None;
        }
        set.insert(login.to_string());
        Some(Self { login: login.to_string() })
    }
}

impl Drop for ActiveLoginGuard {
    fn drop(&mut self) {
        active_logins()
            .lock()
            .expect("active_logins lock")
            .remove(&self.login);
    }
}

/// Almacén `dwLoginKey` → login — esqueleto LOGIN_BY_KEY (ROADMAP F2a): el
/// flujo actual del cliente re-manda la password en el LOGIN3 del canal
/// (AGENTS.md §14), así que la validación por key sin password queda
/// documentada, no exigida por el flujo real todavía (F2b la consumirá).
fn login_keys() -> &'static Mutex<HashMap<u32, String>> {
    static M: OnceLock<Mutex<HashMap<u32, String>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Key de login única (parity `CreateLoginKey`: `number(1, INT_MAX)` sin
/// colisiones). Zero-deps: mezcla de nanos + contador (patrón del nonce del
/// handshake); nunca 0; reintenta si colisiona con una key viva.
fn unique_login_key() -> u32 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    loop {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mixed = nanos ^ counter.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let key = mixed as u32 & 0x7FFF_FFFF; // 1..INT_MAX (parity)
        if key == 0 {
            continue;
        }
        let mut map = login_keys().lock().expect("login_keys lock");
        if let std::collections::hash_map::Entry::Vacant(e) = map.entry(key) {
            e.insert(String::new()); // login se rellena en F2b
            return key;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Vector REAL del entorno: account test / 1234 →
    /// `*A4B6157319038724E3560894F7F932C8886EBFCF` (AGENTS.md). El hash vive
    /// en `database::account` (F3/ADR-0008) — su test: `mysql5_password_real_vector`.
    #[test]
    fn login_string_validation_parity() {
        assert!(is_valid_login_string("test"));
        assert!(is_valid_login_string("ab"));
        assert!(!is_valid_login_string("a"));
        assert!(!is_valid_login_string(""));
        assert!(!is_valid_login_string("a b"));
        assert!(!is_valid_login_string("test_"));
        assert!(!is_valid_login_string("tést"));
    }

    #[test]
    fn normalize_login_trim_and_lower() {
        let mut raw = [0u8; 31];
        raw[..6].copy_from_slice(b" Test ");
        assert_eq!(normalize_login(&raw), "test");
        let mut raw2 = [0u8; 31];
        raw2[..4].copy_from_slice(b"TEST");
        raw2[4] = 0;
        assert_eq!(normalize_login(&raw2), "test");
    }

    #[test]
    fn extract_lang_parity() {
        assert_eq!(extract_lang(b"es\0"), Some("es".into()));
        assert_eq!(extract_lang(b"ES\0"), Some("es".into()));
        assert_eq!(extract_lang(b"de\0"), Some("de".into()));
        assert_eq!(extract_lang(b"\0\0\0"), None);
        assert_eq!(extract_lang(b"a\0\0"), None);
        assert_eq!(extract_lang(b"esx"), None); // sin NUL final
        assert_eq!(extract_lang(b"e1\0"), None);
    }

    #[test]
    fn login_key_never_zero_and_unique() {
        let k1 = unique_login_key();
        let k2 = unique_login_key();
        assert_ne!(k1, 0);
        assert_ne!(k1, k2);
        assert!(k1 <= i32::MAX as u32, "1..INT_MAX (parity CreateLoginKey)");
    }

    /// F2b fix: el parámetro del `UPDATE account SET hwid` es el hex como
    /// TEXT — 32 chars, cabe en VARCHAR(64). Ni `[u8;16]` (no implementa
    /// ToSql → "error serializing parameter 0") ni `Vec<u8>`/bytea (42804
    /// contra VARCHAR) sirven. El smoke (auth_smoke.rs) NO cubre esto: usa
    /// una PG caída y el UPDATE nunca se ejecuta — se verifica end-to-end
    /// contra la PG real después del deploy.
    #[test]
    fn hwid_update_param_is_hex_text() {
        let hwid: [u8; 16] = [
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, //
            0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
        ];
        let hex = hex16(&hwid);
        assert_eq!(hex, "aabbccddeeff00112233445566778899");
        assert_eq!(hex.len(), 32, "16 bytes → 32 chars hex");
        assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()));

        // Compile-time: el valor que viaja al UPDATE es String (ToSql como
        // TEXT), igual que el login ($2) — no un array ni bytes.
        let _as_text: &(dyn tokio_postgres::types::ToSql + Sync) = &hex;
        let _login: &(dyn tokio_postgres::types::ToSql + Sync) = &"test";
    }

    #[test]
    fn config_parse_and_defaults() {
        let cfg = Config::parse("listen = \"127.0.0.1:30001\"\ntimeout_ms = 15000\n").unwrap();
        assert_eq!(cfg.timeout, Duration::from_secs(15));
        assert_eq!(cfg.legacy_dir, "", "default: sin legacy");
        let cfg = Config::parse("legacy_dir = \"/home/m2/source/metin2_svfiles/main/srv1/auth1\"\n").unwrap();
        assert_eq!(cfg.legacy_dir, "/home/m2/source/metin2_svfiles/main/srv1/auth1");
    }

    /// Orden del login exitoso (parity input_db.cpp:1710-1716): 151 panama →
    /// 152 keys → 153 sdb ("none" = MAPNAME_DEFAULT) → GC_AUTH_SUCCESS.
    #[tokio::test]
    async fn legacy_packets_sent_before_auth_success() {
        use tokio::io::AsyncReadExt;
        let (mut server, mut client) = tokio::io::duplex(4096);
        let legacy = LegacyData {
            panama: vec![legacy::PanamaEntry { name: "test.epk".into(), iv: [0xAA; 32] }],
            hybrid: legacy::HybridData {
                keys_stream: vec![1, 2, 3],
                sdb: [("none".to_string(), vec![0xBB])].into_iter().collect(),
            },
        };
        let handle = tokio::spawn(async move {
            let mut conn = Connection::new(&mut server);
            send_legacy_packets(&mut conn, &legacy, 0x1234).await.unwrap();
            conn.send(&TPacketGCAuthSuccess::new(7, 1).to_bytes()).await.unwrap();
        });
        // 151: 289 B (header + name + IV XOR-eado).
        let mut buf = [0u8; 289];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf[0], 151);
        assert_eq!(&buf[1..9], b"test.epk");
        // 152: 7 + 3 B.
        let mut buf = [0u8; 10];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, [152, 10, 0, 3, 0, 0, 0, 1, 2, 3]);
        // 153: 7 + 1 B.
        let mut buf = [0u8; 8];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, [153, 8, 0, 1, 0, 0, 0, 0xBB]);
        // 150: GC_AUTH_SUCCESS.
        let mut buf = [0u8; 6];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, TPacketGCAuthSuccess::new(7, 1).to_bytes());
        handle.await.unwrap();
    }

    /// Sin datos legacy (runtime actual) → no se envía nada antes del success.
    #[tokio::test]
    async fn legacy_packets_empty_sends_nothing() {
        use tokio::io::AsyncReadExt;
        let (mut server, mut client) = tokio::io::duplex(64);
        let legacy = LegacyData::default();
        let handle = tokio::spawn(async move {
            let mut conn = Connection::new(&mut server);
            send_legacy_packets(&mut conn, &legacy, 0).await.unwrap();
            conn.send(&TPacketGCAuthSuccess::new(0, 1).to_bytes()).await.unwrap();
        });
        let mut buf = [0u8; 6];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, TPacketGCAuthSuccess::new(0, 1).to_bytes());
        handle.await.unwrap();
    }

    // -----------------------------------------------------------------------
    // F5 — GC_CHANNEL_LIST (164)
    // -----------------------------------------------------------------------

    /// Wire exacto: header 164, count, rates LE, name/ip NUL-padded, port y
    /// players LE, slots no usados a cero. Vector REAL del runtime (ch1).
    #[test]
    fn channel_list_wire_exact() {
        let channels = vec![ChannelCfg {
            name: "CH-1".into(),
            ip: "172.25.104.175".into(),
            port: 30003,
            players: 7,
        }];
        let b = encode_channel_list(&channels, 100, 200, 300);
        assert_eq!(b.len(), 152, "tamaño fijo (parity TPacketGCChannelList C++)");
        assert_eq!(b[0], GC_CHANNEL_LIST, "header 164");
        assert_eq!(b[1], 1, "count");
        assert_eq!(u16::from_le_bytes([b[2], b[3]]), 100, "exp_rate");
        assert_eq!(u16::from_le_bytes([b[4], b[5]]), 200, "gold_rate");
        assert_eq!(u16::from_le_bytes([b[6], b[7]]), 300, "drop_rate");
        // Canal 0 en base 8: name[16] + ip[16] + port u16 + players u16.
        assert_eq!(&b[8..13], b"CH-1\0", "name NUL-padded");
        assert_eq!(&b[8 + 16..8 + 16 + 15], b"172.25.104.175\0", "ip NUL-padded");
        assert_eq!(u16::from_le_bytes([b[8 + 32], b[8 + 33]]), 30003, "port LE");
        assert_eq!(u16::from_le_bytes([b[8 + 34], b[8 + 35]]), 7, "players LE");
        // Slots 1..4 a cero.
        assert!(b[8 + 36..].iter().all(|&x| x == 0), "slots no usados a cero");
    }

    /// Truncamientos: count capado a 4, strings a 15 bytes.
    #[test]
    fn channel_list_truncates() {
        let channels = (0..6)
            .map(|i| ChannelCfg {
                name: format!("channel-number-{i}"), // > 15 chars
                ip: "172.255.255.255".into(),
                port: 30_000 + i as u16,
                players: 0,
            })
            .collect::<Vec<_>>();
        let b = encode_channel_list(&channels, 100, 100, 100);
        assert_eq!(b[1], 4, "count capado a 4");
        assert_eq!(&b[8..23], b"channel-number-", "name truncado a 15");
        // Canal 3 (último válido) presente, canal 4 a cero.
        assert_eq!(&b[8 + 3 * 36..8 + 3 * 36 + 7], b"channel", "canal 3 truncado");
        assert!(b[8 + 4 * 36..].iter().all(|&x| x == 0), "canal 4+ a cero");
    }

    /// Lista vacía → count 0 (el envío se omite en `send_channel_list`, pero
    /// el encoder sigue siendo determinista).
    #[test]
    fn channel_list_empty() {
        let b = encode_channel_list(&[], 100, 100, 100);
        assert_eq!(b[0], GC_CHANNEL_LIST);
        assert_eq!(b[1], 0);
        assert_eq!(u16::from_le_bytes([b[2], b[3]]), 100, "rates presentes");
        assert!(b[8..].iter().all(|&x| x == 0), "sin canales → zona de canales a cero");
    }

    /// F5: el 164 se envía ANTES del 150 en login exitoso (el cliente legacy
    /// consume un paquete por frame y desconecta del auth al despachar el
    /// 150 — con 164 después nunca lo leería).
    #[tokio::test]
    async fn channel_list_sent_before_auth_success() {
        use tokio::io::AsyncReadExt;
        let (mut server, mut client) = tokio::io::duplex(1024);
        let mut cfg = Config::default();
        cfg.channels = vec![ChannelCfg {
            name: "CH-1".into(),
            ip: "172.25.104.175".into(),
            port: 30003,
            players: 0,
        }];
        cfg.exp_rate = 100;
        cfg.gold_rate = 100;
        cfg.drop_rate = 100;
        let handle = tokio::spawn(async move {
            let mut conn = Connection::new(&mut server);
            send_channel_list(&mut conn, &cfg).await.unwrap();
            conn.send(&TPacketGCAuthSuccess::new(7, 1).to_bytes()).await.unwrap();
        });
        let mut buf = [0u8; GC_CHANNEL_LIST_SIZE];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf[0], GC_CHANNEL_LIST, "164 primero");
        let mut buf = [0u8; 6];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, TPacketGCAuthSuccess::new(7, 1).to_bytes(), "150 después");
        handle.await.unwrap();
    }

    /// F5: con `channels` vacío no se envía nada (parity auth C++).
    #[tokio::test]
    async fn channel_list_skipped_when_no_channels() {
        use tokio::io::AsyncReadExt;
        let (mut server, mut client) = tokio::io::duplex(64);
        let cfg = Config::default(); // channels vacío
        let handle = tokio::spawn(async move {
            let mut conn = Connection::new(&mut server);
            send_channel_list(&mut conn, &cfg).await.unwrap();
            conn.send(&TPacketGCAuthSuccess::new(0, 1).to_bytes()).await.unwrap();
        });
        let mut buf = [0u8; 6];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, TPacketGCAuthSuccess::new(0, 1).to_bytes(), "solo el 150");
        handle.await.unwrap();
    }
}
