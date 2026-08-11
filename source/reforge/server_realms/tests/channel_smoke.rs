//! Smoke F4 slice 2: fake client legacy contra el channel REAL
//! (`server_realms --role channel`) como subproceso — valida el wire del
//! flujo login SIN PostgreSQL (el config apunta a un puerto muerto → la
//! validación de credenciales devuelve GC_LOGIN_FAILURE "NOTAVAIL",
//! divergencia documentada del canal Rust: el C++ con el db caído no responde).
//!
//! Flujo verificado: handshake (GC_PHASE + GC_HANDSHAKE → eco CG_HANDSHAKE)
//! → GC_PHASE(LOGIN) → CG_LOGIN3 (65 B, canal) → GC_LOGIN_FAILURE (10 B,
//! status "NOTAVAIL").

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use network::{read_exact_size, Connection};
use protocol::{phase, TPacketCGHandshake, TPacketCGLogin3, TPacketGCPhase};
use tokio::net::TcpStream;

/// Config temporal POR TEST (patrón auth_smoke — los tests del bin corren en
/// paralelo; un archivo común sería un race).
fn write_temp_config(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("f4_channel_smoke_{name}.toml"));
    // PG caído a propósito: 127.0.0.1:1 (nadie escucha) → NOTAVAIL.
    let toml = "listen = \"127.0.0.1:0\"\n\
                pg_conn = \"host=127.0.0.1 port=1 user=mt2 password=mt2 dbname=metin2\"\n\
                timeout_ms = 15000\n\
                no_more_clients = false\n";
    std::fs::write(&path, toml).expect("escribir config temporal");
    path
}

/// Arranca el channel y espera la línea `channel escuchando en addr` (stdout).
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
        .unwrap_or_else(|_| panic!("el channel no anunció el listener (stdout): ¿arrancó?"));
    (child, addr)
}

/// Lado cliente del handshake + fase LOGIN del canal: recibe GC_PHASE +
/// GC_HANDSHAKE, responde el eco con el reloj alineado, y recibe
/// GC_PHASE(LOGIN) — el paquete que dispara el LOGIN3 del cliente
/// (PythonNetworkStream.cpp:597-599).
async fn client_handshake_channel(conn: &mut Connection<TcpStream>) {
    let phase_pkt = read_exact_size(conn, TPacketGCPhase::SIZE).await.expect("GC_PHASE");
    let gc_phase = TPacketGCPhase::from_bytes(&phase_pkt).expect("parse GC_PHASE");
    assert_eq!(gc_phase.phase, phase::HANDSHAKE, "phase HANDSHAKE");

    let hs_pkt = read_exact_size(conn, 13).await.expect("GC_HANDSHAKE");
    assert_eq!(hs_pkt[0], 0xff, "header GC_HANDSHAKE");
    let nonce = u32::from_le_bytes([hs_pkt[1], hs_pkt[2], hs_pkt[3], hs_pkt[4]]);
    let dw_time = u32::from_le_bytes([hs_pkt[5], hs_pkt[6], hs_pkt[7], hs_pkt[8]]);
    conn.send(&TPacketCGHandshake::new(nonce, dw_time, 0).to_bytes())
        .await
        .expect("eco CG_HANDSHAKE");

    // Tras el eco, el canal manda GC_PHASE(LOGIN) (parity input.cpp:194-196).
    let login_phase = read_exact_size(conn, TPacketGCPhase::SIZE)
        .await
        .expect("GC_PHASE(LOGIN) tras el eco");
    let gc_login_phase = TPacketGCPhase::from_bytes(&login_phase).expect("parse GC_PHASE(LOGIN)");
    assert_eq!(gc_login_phase.phase, phase::LOGIN, "phase LOGIN");
}

#[tokio::test]
async fn channel_handles_login3_with_db_down() {
    let config_path = write_temp_config("login3");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr).await.map_err(|e| format!("connect {addr}: {e}"))?;
        let mut conn = Connection::new(stream);
        client_handshake_channel(&mut conn).await;

        // LOGIN3 al canal = 65 B (sin lang; framer rol Channel).
        let login3 = TPacketCGLogin3::new_channel("test", "1234", [0; 4]).to_bytes_channel();
        assert_eq!(login3.len(), 65, "LOGIN3 del canal");
        conn.send(&login3).await.map_err(|e| format!("LOGIN3: {e}"))?;

        // Respuesta: GC_LOGIN_FAILURE (10 B) con la DB caída — NOTAVAIL
        // (divergencia documentada del canal Rust).
        let hdr = read_exact_size(&mut conn, 1).await.map_err(|e| format!("header: {e}"))?;
        assert_eq!(hdr[0], 0x07, "GC_LOGIN_FAILURE (0x07)");
        let rest = read_exact_size(&mut conn, 9).await.map_err(|e| format!("resto: {e}"))?;
        let mut pkt = hdr;
        pkt.extend_from_slice(&rest);
        let status = pkt[1..].iter().take_while(|&&b| b != 0).copied().collect::<Vec<u8>>();
        assert_eq!(status, b"NOTAVAIL", "DB caída -> NOTAVAIL (determinista)");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("flujo channel con DB caída");
}
