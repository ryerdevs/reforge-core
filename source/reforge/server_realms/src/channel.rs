//! F4 slice 2 — rol `channel`: el canal Rust sirviendo el flujo real
//! login→select (y spawn best-effort) contra PostgreSQL DIRECTO (los repos
//! son PG-native — sin proxy, ADR-0008).
//!
//! Paridad con el canal C++ (`input_login.cpp` + `input_db.cpp` + `desc.cpp`):
//!
//! 1. Handshake server-side (`network::handshake` — igual que el auth):
//!    `GC_PHASE(HANDSHAKE)` + `GC_HANDSHAKE` → eco `CG_HANDSHAKE`.
//! 2. `GC_PHASE(LOGIN)` — parity `input.cpp:194-196` (`g_bAuthServer ? PHASE_AUTH
//!    : PHASE_LOGIN`): el cliente entra en `SetLoginPhase` y manda el LOGIN3 del
//!    canal (`PythonNetworkStream.cpp:597-599` → 65 B, sin lang).
//! 3. `CG_LOGIN3` (65 B — el framer con rol Channel ya lo entrega así).
//! 4. Validaciones en orden (parity `input_login.cpp:97-147` + `db.cpp:244-365`):
//!    login inválido → `GC_LOGIN_FAILURE "NOID"`; `no_more_clients` →
//!    `"SHUTDOWN"`; ya logueado → `"ALREADY"`; credenciales vs PG →
//!    `AccountRepo::login` (13 columnas, hash MySQL en Rust); status != "OK" →
//!    `GC_LOGIN_FAILURE(status)`.
//! 5. `GC_EMPIRE` (0x5a, 2 B — empire de la cuenta; 0 → random 1..3, parity
//!    `input_db.cpp:167-180` GetServerLocation) + `GC_PHASE(SELECT)` +
//!    `GC_LOGIN_SUCCESS_NEWSLOT` (0x20, 449 B — `realm::packets::login_success`
//!    con los 5 slots de `WorldStore::account_slots`; handle = conn_id,
//!    random_key = rand32 — parity `desc.cpp:955-988`).
//! 6. `CG_PLAYER_SELECT` (2 B) → `WorldStore::select_player` (índice → Q2 load)
//!    → **spawn best-effort**: `GC_PHASE(LOADING)` + `TPacketGCCharacterAdd`
//!    (37 B) + `TPacketGCCharacterAdditionalInfo` (70 B) — los GAPs del spawn
//!    completo están documentados en `realm::packets` (mapa/sectree,
//!    affects→flags, items→parts, speeds, PointsPacket/SkillLevelPacket, SDB).
//!    Si el cliente no completa la entrada al mapa, el SELECT es el hito.
//!
//! Divergencias deliberadas (documentadas):
//! - DB caída en el LOGIN3 → `GC_LOGIN_FAILURE "NOTAVAIL"` (el C++ con el db
//!   caído no responde nada — la query se pierde; el Rust degrada a un status
//!   válido del protocolo, determinista; mismo espíritu que el auth bResult=0).
//! - `WorldStore` por conexión (sanity + Batcher por login): el pool y el
//!   Batcher compartido se deciden con el pipeline WAL (ADR-0008).
//! - `test_server` gate (`input_login.cpp:108-114`, "VERSION"): el Rust no lo
//!   aplica (el runtime real del C++ lo tiene desactivado).
//! - `last_play`/`g_iUserLimit` (FULL) del canal C++: no implementados
//!   (analytics / config no expuesto — YAGNI, se reportan como GAP).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use database::account::AccountRepo;
use network::framer::{ConnectionRole, Framer};
use network::{handshake, Connection};
use protocol::{
    header, phase, TPacketCGLogin3, TPacketCGPlayerSelect, TPacketGCEmpire, TPacketGCLoginFailure,
    TPacketGCLoginSuccess, TPacketGCPhase, PLAYER_PER_ACCOUNT,
};
use realm::packets;
use realm::world::WorldStore;
use tokio::net::{TcpListener, TcpStream};

use crate::auth::{is_valid_login_string, normalize_login};
use crate::config::Config;

/// Servidor channel: listener + tarea por conexión (patrón del auth).
pub async fn run(config: Config) -> std::io::Result<()> {
    let listener = TcpListener::bind(&config.listen).await?;
    println!("server_realms: channel escuchando en {}", listener.local_addr()?);
    let mut conn_id: u32 = 1;
    loop {
        let (stream, _peer) = listener.accept().await?;
        let cfg = config.clone();
        let id = conn_id;
        conn_id = conn_id.wrapping_add(1);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, cfg, id).await {
                eprintln!("server_realms: channel conn {id}: {e}");
            }
        });
    }
}

/// Conexión channel con timeout global (patrón del auth — deuda F1.5).
async fn handle_connection(stream: TcpStream, config: Config, conn_id: u32) -> Result<(), String> {
    match tokio::time::timeout(config.timeout, connection_inner(stream, &config, conn_id)).await {
        Err(_) => Err(format!(
            "channel conn {conn_id}: timeout global de {} ms — conexión cerrada",
            config.timeout.as_millis()
        )),
        Ok(r) => r,
    }
}

async fn connection_inner(stream: TcpStream, config: &Config, conn_id: u32) -> Result<(), String> {
    let mut conn = Connection::new(stream);
    let mut framer = Framer::new(ConnectionRole::Channel);

    // 1. Handshake server-side (F1.5, validado contra el canal real en F1.6).
    let hs = handshake::perform(&mut conn, &mut framer, now_ms())
        .await
        .map_err(|e| format!("handshake: {e}"))?;
    eprintln!("server_realms: channel conn {conn_id}: handshake OK (delta {} ms)", hs.delta);

    // 2. GC_PHASE(LOGIN) — el cliente responde con el LOGIN3 del canal (65 B).
    conn.send(&TPacketGCPhase::new(phase::LOGIN).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_PHASE(LOGIN): {e}"))?;
    eprintln!("server_realms: channel conn {conn_id}: enviado GC_PHASE(LOGIN)");

    // 3. LOGIN3 (65 B al canal — framer rol Channel).
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
            other => {
                return Err(format!(
                    "channel conn {conn_id}: header inesperado 0x{other:02x} tras el handshake"
                ))
            }
        }
    };
    let login = normalize_login(&login3.login);
    let passwd = cstr(&login3.passwd).to_string();
    eprintln!("server_realms: channel conn {conn_id}: LOGIN3 login={login}");

    // 4. Validaciones (parity input_login.cpp:97-147 + db.cpp:244-365).
    if !is_valid_login_string(&login) {
        send_login_failure(&mut conn, "NOID").await?;
        return Ok(());
    }
    if config.no_more_clients {
        send_login_failure(&mut conn, "SHUTDOWN").await?;
        return Ok(());
    }
    let Some(_guard) = ChannelLoginGuard::acquire(&login) else {
        send_login_failure(&mut conn, "ALREADY").await?;
        return Ok(());
    };

    // 5. Credenciales vs PG (QUERY_LOGIN — 13 columnas; el canal C++ hace
    //    GD_LOGIN → db → RESULT_LOGIN, `db.cpp:244-365`).
    let acc = match AccountRepo::new(&config.pg_conn).login(&login, &passwd).await {
        Ok(Some(acc)) => acc,
        Ok(None) => {
            eprintln!("server_realms: channel conn {conn_id}: NOID {login}");
            send_login_failure(&mut conn, "NOID").await?;
            return Ok(());
        }
        Err(e) => {
            // Divergencia documentada: DB caída -> NOTAVAIL (determinista).
            eprintln!("server_realms: channel conn {conn_id}: PG falló para {login}: {e} — NOTAVAIL");
            send_login_failure(&mut conn, "NOTAVAIL").await?;
            return Ok(());
        }
    };
    if acc.status != "OK" {
        eprintln!("server_realms: channel conn {conn_id}: status '{}' para {login}", acc.status);
        send_login_failure(&mut conn, &acc.status).await?;
        return Ok(());
    }
    eprintln!(
        "server_realms: channel conn {conn_id}: login OK {login} (id {}, empire {:?})",
        acc.id, acc.empire
    );

    // 6. WorldStore (repos + Batcher) + empire + paquete del select.
    let store = match WorldStore::new(&config.pg_conn).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("server_realms: channel conn {conn_id}: WorldStore: {e} — NOTAVAIL");
            send_login_failure(&mut conn, "NOTAVAIL").await?;
            return Ok(());
        }
    };
    let empire = empire_byte(acc.empire);
    eprintln!("server_realms: channel conn {conn_id}: empire={empire}");

    // GC_EMPIRE (0x5a) + GC_PHASE(SELECT) + 449 B (parity input_db.cpp:169-183).
    conn.send(&TPacketGCEmpire::new(empire).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_EMPIRE: {e}"))?;
    conn.send(&TPacketGCPhase::new(phase::SELECT).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_PHASE(SELECT): {e}"))?;
    let success = build_login_success(&store, acc.id, conn_id).await?;
    let bytes = success.to_bytes();
    assert_eq!(bytes.len(), TPacketGCLoginSuccess::SIZE, "449 B (invariante wire)");
    conn.send(&bytes).await.map_err(|e| format!("enviando 449 B: {e}"))?;
    eprintln!("server_realms: channel conn {conn_id}: enviado GC_EMPIRE + GC_PHASE(SELECT) + 449 B");

    // 7. Select: CG_PLAYER_SELECT (2 B) → load → spawn best-effort.
    let select = loop {
        let pkt = framer
            .next_packet(&mut conn)
            .await
            .map_err(|e| format!("framer (select): {e}"))?;
        match pkt[0] {
            header::CG_TIME_SYNC | header::CG_PONG => continue,
            header::CG_CHARACTER_SELECT => {
                break TPacketCGPlayerSelect::from_bytes(&pkt)
                    .map_err(|e| format!("CG_PLAYER_SELECT: {e}"))?
            }
            other => {
                return Err(format!(
                    "channel conn {conn_id}: header inesperado 0x{other:02x} esperando el select"
                ))
            }
        }
    };
    eprintln!("server_realms: channel conn {conn_id}: CG_PLAYER_SELECT index={}", select.index);

    let Some(row) = store.select_player(acc.id, select.index).await? else {
        // Parity input_login.cpp:266-271 ("player index not found" -> CLOSE).
        eprintln!("server_realms: channel conn {conn_id}: slot vacío/inválido — cierre");
        return Ok(());
    };
    eprintln!(
        "server_realms: channel conn {conn_id}: player_load {} id={} lvl={} x={} y={}",
        row.name, row.id, row.level, row.x, row.y
    );

    // Spawn best-effort: GC_PHASE(LOADING) + ADD + ADDITIONAL_INFO (los GAPs
    // del spawn completo en realm::packets — mapa/sectree, affects→flags,
    // items→parts, PointsPacket, SkillLevelPacket, SDB).
    for pkt in spawn_packets(&row, empire) {
        conn.send(&pkt).await.map_err(|e| format!("enviando spawn: {e}"))?;
    }
    eprintln!(
        "server_realms: channel conn {conn_id}: spawn best-effort enviado \
         (LOADING + ADD + ADDITIONAL_INFO) — el mundo completo (mapa/afects/items) es el siguiente slice"
    );

    // 8. Keepalive loop: el mundo real no existe todavía — la conexión se
    //    mantiene viva leyendo hasta EOF (el cliente fallará cargando el mapa;
    //    el hito del slice es el SELECT).
    loop {
        let pkt = framer
            .next_packet(&mut conn)
            .await
            .map_err(|e| format!("framer (keepalive): {e}"))?;
        match pkt[0] {
            header::CG_TIME_SYNC | header::CG_PONG => continue,
            other => {
                eprintln!(
                    "server_realms: channel conn {conn_id}: post-spawn header 0x{other:02x} \
                     ignorado (mundo no implementado)"
                );
            }
        }
    }
}

/// Los paquetes del spawn best-effort en orden wire (parity
/// `input_db.cpp:428-429` SetPhase(PHASE_LOADING) + `char.cpp:876-948`
/// EncodeInsertPacket): `GC_PHASE(LOADING)` → `TPacketGCCharacterAdd` →
/// `TPacketGCCharacterAdditionalInfo`. Función pura (testeable sin red).
fn spawn_packets(row: &database::player::PlayerRow, empire: u8) -> Vec<Vec<u8>> {
    vec![
        TPacketGCPhase::new(phase::LOADING).to_bytes().to_vec(),
        packets::character_add(row).to_bytes().to_vec(),
        packets::character_additional_info(row, empire).to_bytes().to_vec(),
    ]
}

/// Armado del 449 B: slots del índice (orden del player_index) emparejados con
/// los summaries del Q3 por id (parity `CreateAccountPlayerDataFromRes:315-317`
/// — el C++ empareja por dwID; un slot con pid pero sin fila Q3 queda como
/// TSimplePlayer zeroed, divergencia menor documentada: el C++ deja el dwID
/// puesto y stats 0, el Rust lo deja todo a 0).
async fn build_login_success(
    store: &WorldStore,
    account_id: i64,
    handle: u32,
) -> Result<TPacketGCLoginSuccess, String> {
    let slots = store.account_slots(account_id).await?;
    let summaries = store.list_characters(account_id).await?;
    let mut players: [Option<database::player::PlayerSummary>; PLAYER_PER_ACCOUNT] =
        [None, None, None, None, None];
    for (i, pid) in slots.iter().enumerate() {
        if let Some(pid) = pid {
            if let Some(s) = summaries.iter().find(|s| s.id == *pid) {
                players[i] = Some(s.clone());
            }
        }
    }
    Ok(packets::login_success(&players, handle, rand32()))
}

/// Empire del `GC_EMPIRE`: 1..3 de la cuenta -> su valor; `None`/0 -> random
/// 1..3 (parity `input_db.cpp:167-180`: `GetServerLocation(0)` falla y el C++
/// manda `number(1, 3)`).
fn empire_byte(empire: Option<i16>) -> u8 {
    match empire {
        Some(1..=3) => empire.unwrap() as u8,
        _ => (rand32() % 3) as u8 + 1,
    }
}

/// `GC_LOGIN_FAILURE` con log (patrón del auth).
async fn send_login_failure(conn: &mut Connection<TcpStream>, status: &str) -> Result<(), String> {
    eprintln!("server_realms: channel: GC_LOGIN_FAILURE {status}");
    conn.send(&TPacketGCLoginFailure::new(status).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_LOGIN_FAILURE: {e}"))
}

/// C-string → `&str` (hasta el primer NUL; bytes no-UTF8 → vacío defensivo).
fn cstr(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// `now_ms` — reloj del servidor en ms desde boot (parity `get_dword_time`).
fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Rand 32-bit determinista sin dependencias (nanos + contador — patrón del
/// nonce del handshake y de `unique_login_key` del auth). Uso: `random_key`
/// del 449 B (parity `DESC_MANAGER::MakeRandomKey`) y empire aleatorio.
fn rand32() -> u32 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    (nanos ^ c.wrapping_mul(0x9E37_79B9_7F4A_7C15)) as u32
}

/// Logins con sesión activa en el canal (parity `DESC_MANAGER::FindByLoginName`,
/// `db.cpp:354-359` — el C++ rechaza un segundo login del mismo nombre). El
/// guard libera al cerrar la conexión. Independiente del guard del auth (otro
/// proceso).
fn channel_logins() -> &'static Mutex<std::collections::HashSet<String>> {
    static M: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

struct ChannelLoginGuard {
    login: String,
}

impl ChannelLoginGuard {
    fn acquire(login: &str) -> Option<Self> {
        let mut set = channel_logins().lock().expect("channel_logins lock");
        if set.contains(login) {
            return None;
        }
        set.insert(login.to_string());
        Some(Self { login: login.to_string() })
    }
}

impl Drop for ChannelLoginGuard {
    fn drop(&mut self) {
        channel_logins()
            .lock()
            .expect("channel_logins lock")
            .remove(&self.login);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::player::PlayerRow;
    use protocol::{
        TPacketGCCharacterAdd, TPacketGCCharacterAdditionalInfo, TPacketGCEmpire,
        TPacketGCLoginFailure, TPacketGCLoginSuccess,
    };

    /// Empire: Some(1..=3) se usa tal cual; None/0/4+ -> random 1..3 (parity
    /// input_db.cpp:167-180 — el random se cubre probando el rango).
    #[test]
    fn empire_byte_parity() {
        assert_eq!(empire_byte(Some(1)), 1);
        assert_eq!(empire_byte(Some(3)), 3);
        assert_eq!(empire_byte(Some(2)), 2);
        for src in [None, Some(0), Some(4), Some(-1)] {
            let e = empire_byte(src);
            assert!((1..=3).contains(&e), "empire random 1..3: {e} ({src:?})");
        }
        // El random no degenera en un solo valor.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            seen.insert(empire_byte(None));
        }
        assert!(seen.len() >= 2, "el random no degenera: {seen:?}");
    }

    /// Spawn best-effort: 3 paquetes en orden wire con tamanos byte-exactos.
    #[test]
    fn spawn_packets_order_and_sizes() {
        let row = dummy_row();
        let pkts = spawn_packets(&row, 3);
        assert_eq!(pkts.len(), 3, "LOADING + ADD + ADDITIONAL_INFO");
        assert_eq!(pkts[0].len(), TPacketGCPhase::SIZE);
        assert_eq!(pkts[0][0], header::GC_PHASE);
        assert_eq!(pkts[0][1], phase::LOADING, "parity input_db.cpp:428 SetPhase(PHASE_LOADING)");
        assert_eq!(pkts[1].len(), TPacketGCCharacterAdd::SIZE);
        assert_eq!(pkts[1][0], header::GC_CHARACTER_ADD);
        assert_eq!(pkts[2].len(), TPacketGCCharacterAdditionalInfo::SIZE);
        assert_eq!(pkts[2][0], header::GC_CHAR_ADDITIONAL_INFO);
    }

    /// Tamano del wire del flujo select/spawn (invariante byte-exacto).
    #[test]
    fn select_spawn_wire_sizes() {
        assert_eq!(TPacketGCEmpire::SIZE, 2, "GC_EMPIRE 0x5a");
        assert_eq!(TPacketGCLoginSuccess::SIZE, 449, "0x20 NEWSLOT");
        assert_eq!(TPacketCGPlayerSelect::SIZE, 2, "CG_PLAYER_SELECT");
        assert_eq!(TPacketGCPhase::SIZE, 2);
        assert_eq!(TPacketGCLoginFailure::SIZE, 10, "GC_LOGIN_FAILURE");
    }

    /// random_key nunca es 0 (parity MakeRandomKey: number(1, INT_MAX)).
    #[test]
    fn rand32_never_zero() {
        for _ in 0..100 {
            assert_ne!(rand32(), 0);
        }
    }

    fn dummy_row() -> PlayerRow {
        PlayerRow {
            id: 2,
            name: "ninja".into(),
            job: 1,
            voice: 0,
            dir: 0,
            x: 969600,
            y: 278400,
            z: 0,
            map_index: 41,
            exit_x: 0,
            exit_y: 0,
            exit_map_index: 0,
            hp: 100,
            mp: 100,
            stamina: 100,
            random_hp: 0,
            random_sp: 0,
            playtime: 0,
            gold: 0,
            level: 5,
            level_step: 0,
            st: 30,
            ht: 30,
            dx: 30,
            iq: 30,
            exp: 0,
            stat_point: 0,
            skill_point: 0,
            sub_skill_point: 0,
            stat_reset_count: 0,
            part_base: 0,
            part_hair: 0xAABB_CCDD,
            part_main: 0x1122_3344,
            skill_level: None,
            quickslot: None,
            skill_group: 3,
            alignment: 1234,
            horse_level: 0,
            horse_riding: 0,
            horse_hp: 0,
            horse_hp_droptime: 0,
            horse_stamina: 0,
            logoff_interval: 0.0,
            horse_skill_point: 0,
        }
    }
}
