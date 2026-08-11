//! Integration F4 slice 2 (HITO del slice): fake client legacy contra el
//! channel REAL con PostgreSQL de verdad — el flujo login→select→spawn
//! best-effort end-to-end. Gated con `#[ignore]` (requiere la PG de WSL).
//!
//! Ejecutar desde WSL:
//!
//! ```text
//! source ~/.cargo/env
//! cd /mnt/c/projects/Metin2/source/reforge
//! cargo test --package server_realms -- --ignored
//! ```
//!
//! Flujo verificado (parity input_login.cpp / input_db.cpp / desc.cpp):
//! handshake → GC_PHASE(LOGIN) → LOGIN3 65 B (test/1234) → GC_EMPIRE(3) →
//! GC_PHASE(SELECT) → 449 B (slots [1,3,5,0,2] de la cuenta test) →
//! CG_PLAYER_SELECT(0) → GC_PHASE(LOADING) + GC_CHARACTER_ADD +
//! GC_CHAR_ADDITIONAL_INFO.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use network::{read_exact_size, Connection};
use protocol::{
    phase, TPacketCGHandshake, TPacketCGLogin3, TPacketCGPlayerSelect, TPacketGCPhase,
    TPacketGCLoginSuccess,
};
use tokio::net::TcpStream;

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";

fn pg_conn() -> String {
    std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
}

fn write_temp_config(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("f4_channel_pg_{name}.toml"));
    let toml = format!(
        "listen = \"127.0.0.1:0\"\n\
         pg_conn = \"{}\"\n\
         timeout_ms = 15000\n\
         no_more_clients = false\n",
        pg_conn()
    );
    std::fs::write(&path, toml).expect("escribir config temporal");
    path
}

fn spawn_channel(config_path: &std::path::Path) -> (Child, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_server_realms"))
        .args(["--role", "channel", "--config"])
        .arg(config_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("ejecutar server_realms");
    let stdout = child.stdout.take().expect("stdout piped");
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            if let Some(addr) = line.trim().strip_prefix("server_realms: channel escuchando en ") {
                let _ = tx.send(addr.to_string());
                break;
            }
        }
    });
    let addr = rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("el channel no anunció el listener"));
    (child, addr)
}

async fn client_handshake_channel(conn: &mut Connection<TcpStream>) {
    let phase_pkt = read_exact_size(conn, TPacketGCPhase::SIZE).await.expect("GC_PHASE");
    assert_eq!(TPacketGCPhase::from_bytes(&phase_pkt).unwrap().phase, phase::HANDSHAKE);
    let hs_pkt = read_exact_size(conn, 13).await.expect("GC_HANDSHAKE");
    let nonce = u32::from_le_bytes([hs_pkt[1], hs_pkt[2], hs_pkt[3], hs_pkt[4]]);
    let dw_time = u32::from_le_bytes([hs_pkt[5], hs_pkt[6], hs_pkt[7], hs_pkt[8]]);
    conn.send(&TPacketCGHandshake::new(nonce, dw_time, 0).to_bytes())
        .await
        .expect("eco");
    let login_phase = read_exact_size(conn, TPacketGCPhase::SIZE)
        .await
        .expect("GC_PHASE(LOGIN)");
    assert_eq!(TPacketGCPhase::from_bytes(&login_phase).unwrap().phase, phase::LOGIN);
}

/// El HITO del slice: el cliente real llega al SELECT a través del canal Rust.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package server_realms -- --ignored"]
async fn channel_full_login_select_spawn_flow() {
    let config_path = write_temp_config("full");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr).await.map_err(|e| format!("connect: {e}"))?;
        let mut conn = Connection::new(stream);
        client_handshake_channel(&mut conn).await;

        // LOGIN3 del canal (65 B) — test/1234 (cuenta viva del E2E).
        conn.send(&TPacketCGLogin3::new_channel("test", "1234", [0; 4]).to_bytes_channel())
            .await
            .map_err(|e| format!("LOGIN3: {e}"))?;

        // GC_EMPIRE (2 B, 0x5a): empire de la cuenta test = 3 (E2E Q1).
        let empire_pkt = read_exact_size(&mut conn, 2).await.map_err(|e| format!("GC_EMPIRE: {e}"))?;
        assert_eq!(empire_pkt[0], 0x5a, "header GC_EMPIRE");
        assert_eq!(empire_pkt[1], 3, "empire de la cuenta test (pi.empire)");

        // GC_PHASE(SELECT) (2 B).
        let sel_phase = read_exact_size(&mut conn, 2).await.map_err(|e| format!("phase: {e}"))?;
        assert_eq!(sel_phase[0], 0xfd, "GC_PHASE");
        assert_eq!(sel_phase[1], phase::SELECT, "phase SELECT");

        // 449 B: slots del player_index [1, 3, 5, 0, 2] (E2E Q1).
        let mut pkt = read_exact_size(&mut conn, 1).await.map_err(|e| format!("hdr 449: {e}"))?;
        assert_eq!(pkt[0], TPacketGCLoginSuccess::HEADER, "header 0x20 (NEWSLOT)");
        let rest = read_exact_size(&mut conn, TPacketGCLoginSuccess::SIZE - 1)
            .await
            .map_err(|e| format!("449 B: {e}"))?;
        pkt.extend_from_slice(&rest);
        let success = TPacketGCLoginSuccess::from_bytes(&pkt).map_err(|e| e.to_string())?;
        assert_eq!(success.players[0].dw_id, 1, "slot 0 = pid1 (lkjsnlfknlsk)");
        assert_eq!(success.players[1].dw_id, 3, "slot 1 = pid2 (Chaman)");
        assert_eq!(success.players[2].dw_id, 5, "slot 2 = pid3");
        assert_eq!(success.players[3].dw_id, 0, "slot 3 vacio (pid4=0) -> zeroed");
        assert_eq!(success.players[4].dw_id, 2, "slot 4 = pid5 (ninja)");
        assert_eq!(success.players[0].name(), "lkjsnlfknlsk");
        assert_ne!(success.handle, 0, "handle = conn_id");
        assert_ne!(success.random_key, 0, "random_key (MakeRandomKey parity)");
        assert_eq!(success.players[0].w_main_part, 0, "sin parts en el summary? structure-only");

        // CG_PLAYER_SELECT slot 0 (2 B).
        conn.send(&TPacketCGPlayerSelect::new(0).to_bytes())
            .await
            .map_err(|e| format!("select: {e}"))?;

        // GC_PHASE(LOADING) (2 B).
        let loading = read_exact_size(&mut conn, 2).await.map_err(|e| format!("loading: {e}"))?;
        assert_eq!(loading[0], 0xfd, "GC_PHASE");
        assert_eq!(loading[1], phase::LOADING, "parity input_db.cpp:428");

        // GC_CHARACTER_ADD (37 B): vid = 1 (pid del slot 0), type PC (6),
        // race = job (structure-only el valor exacto).
        let add_pkt = read_exact_size(&mut conn, 37).await.map_err(|e| format!("ADD: {e}"))?;
        assert_eq!(add_pkt[0], 1, "header GC_CHARACTER_ADD");
        assert_eq!(u32::from_le_bytes([add_pkt[1], add_pkt[2], add_pkt[3], add_pkt[4]]), 1, "dwVID = pid");
        assert_eq!(add_pkt[21], 6, "bType = CHAR_TYPE_PC (length.h:330)");
        let x = i32::from_le_bytes([add_pkt[9], add_pkt[10], add_pkt[11], add_pkt[12]]);
        let y = i32::from_le_bytes([add_pkt[13], add_pkt[14], add_pkt[15], add_pkt[16]]);
        assert!(x > 0 && y > 0, "x/y UNITS: {x},{y}");

        // GC_CHAR_ADDITIONAL_INFO (70 B): vid = 1, name = lkjsnlfknlsk.
        let info_pkt = read_exact_size(&mut conn, 70).await.map_err(|e| format!("INFO: {e}"))?;
        assert_eq!(info_pkt[0], 136, "header GC_CHAR_ADDITIONAL_INFO");
        assert_eq!(u32::from_le_bytes([info_pkt[1], info_pkt[2], info_pkt[3], info_pkt[4]]), 1, "dwVID");
        let name_end = info_pkt[5..30].iter().position(|&b| b == 0).unwrap_or(25);
        assert_eq!(&info_pkt[5..5 + name_end], b"lkjsnlfknlsk", "name del personaje");
        assert_eq!(info_pkt[50], 3, "bEmpire = empire de la cuenta");
        let level = u32::from_le_bytes([info_pkt[55], info_pkt[56], info_pkt[57], info_pkt[58]]);
        assert!(level >= 1, "dwLevel del personaje: {level}");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("flujo login→select→spawn del canal contra PG real");
}

/// LOGIN3 con password mala → NOID (parity db.cpp:244-249).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package server_realms -- --ignored"]
async fn channel_wrong_password_noid() {
    let config_path = write_temp_config("noid");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr).await.map_err(|e| format!("connect: {e}"))?;
        let mut conn = Connection::new(stream);
        client_handshake_channel(&mut conn).await;
        conn.send(&TPacketCGLogin3::new_channel("test", "mala", [0; 4]).to_bytes_channel())
            .await
            .map_err(|e| format!("LOGIN3: {e}"))?;
        let hdr = read_exact_size(&mut conn, 1).await.map_err(|e| format!("header: {e}"))?;
        assert_eq!(hdr[0], 0x07, "GC_LOGIN_FAILURE");
        let rest = read_exact_size(&mut conn, 9).await.map_err(|e| format!("resto: {e}"))?;
        let status = rest.iter().take_while(|&&b| b != 0).copied().collect::<Vec<u8>>();
        assert_eq!(status, b"NOID", "password mala -> NOID");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("password mala -> NOID");
}

/// Select de un slot VACÍO (pid4=0) → cierre limpio (parity input_login.cpp:266-271).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package server_realms -- --ignored"]
async fn channel_select_empty_slot_closes() {
    let config_path = write_temp_config("empty");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr).await.map_err(|e| format!("connect: {e}"))?;
        let mut conn = Connection::new(stream);
        client_handshake_channel(&mut conn).await;
        conn.send(&TPacketCGLogin3::new_channel("test", "1234", [0; 4]).to_bytes_channel())
            .await
            .map_err(|e| format!("LOGIN3: {e}"))?;
        // Consume GC_EMPIRE + GC_PHASE(SELECT) + 449 B (flujo del login OK).
        let _ = read_exact_size(&mut conn, 2).await.map_err(|e| format!("empire: {e}"))?;
        let _ = read_exact_size(&mut conn, 2).await.map_err(|e| format!("phase: {e}"))?;
        let _ = read_exact_size(&mut conn, TPacketGCLoginSuccess::SIZE)
            .await
            .map_err(|e| format!("449: {e}"))?;

        // Select del slot 3 (pid4 = 0 — vacío).
        conn.send(&TPacketCGPlayerSelect::new(3).to_bytes())
            .await
            .map_err(|e| format!("select: {e}"))?;
        // El canal cierra sin paquetes de spawn (parity "player index not found").
        let mut b = [0u8; 1];
        let n = conn.recv(&mut b).await.map_err(|e| format!("recv: {e}"))?;
        assert_eq!(n, 0, "EOF limpio — slot vacío cierra la conexión");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("select de slot vacío cierra");
}
