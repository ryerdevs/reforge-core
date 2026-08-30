//! Smoke F2a: fake client legacy contra el auth REAL (`server_realms --role
//! auth`) como subproceso — valida el wire end-to-end SIN PostgreSQL (el
//! config apunta a un puerto muerto → la validación de credenciales devuelve
//! bResult=0, que es el comportamiento del C++ ante DB fallida).
//!
//! Flujo verificado: handshake (GC_PHASE + GC_HANDSHAKE → eco CG_HANDSHAKE,
//! igual que f16_peer) → CG_LOGIN3 (68 B con lang) → GC_AUTH_SUCCESS (6 B,
//! bResult=0, key=0 — parity input_db.cpp:1719-1726).

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use network::{Connection, read_exact_size};
use protocol::{TPacketCGHandshake, TPacketCGLogin3, TPacketGCAuthSuccess, TPacketGCPhase, phase};
use tokio::net::TcpStream;

/// Config temporal POR TEST (los tests corren en paralelo y comparten el
/// temp dir — un archivo común sería un race: un test lo sobrescribe mientras
/// el otro lo usa).
fn write_temp_config(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("f2a_auth_smoke_{name}.toml"));
    // PG caído a propósito: 127.0.0.1:1 (nadie escucha) → bResult=0.
    let toml = "listen = \"127.0.0.1:0\"\n\
                pg_conn = \"host=127.0.0.1 port=1 user=mt2 password=mt2 dbname=metin2\"\n\
                timeout_ms = 15000\n\
                no_more_clients = false\n";
    std::fs::write(&path, toml).expect("escribir config temporal");
    path
}

/// Arranca el auth y espera la línea `auth escuchando en addr` del stdout.
fn spawn_auth(config_path: &std::path::Path) -> (Child, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_server_realms"))
        .args(["--role", "auth", "--config"])
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
            if let Some(addr) = line
                .trim()
                .strip_prefix("server_realms: auth escuchando en ")
            {
                let _ = tx.send(addr.to_string());
                break;
            }
        }
    });
    let addr = rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("el auth no anunció el listener (stdout): ¿arrancó?"));
    (child, addr)
}

/// Lado cliente del handshake legacy (parity f16_peer): recibe GC_PHASE +
/// GC_HANDSHAKE, responde el eco con el reloj alineado, y recibe
/// GC_PHASE(PHASE_AUTH) — el paquete que dispara el LOGIN3 del cliente
/// (AccountConnector.cpp `__AuthState_RecvPhase`; bug del test híbrido
/// 2026-08-11: sin él el cliente nunca manda LOGIN3).
async fn client_handshake(conn: &mut Connection<TcpStream>) {
    let phase_pkt = read_exact_size(conn, TPacketGCPhase::SIZE)
        .await
        .expect("GC_PHASE");
    let gc_phase = TPacketGCPhase::from_bytes(&phase_pkt).expect("parse GC_PHASE");
    assert_eq!(gc_phase.phase, phase::HANDSHAKE, "phase HANDSHAKE");

    let hs_pkt = read_exact_size(conn, 13).await.expect("GC_HANDSHAKE");
    assert_eq!(hs_pkt[0], 0xff, "header GC_HANDSHAKE");
    let nonce = u32::from_le_bytes([hs_pkt[1], hs_pkt[2], hs_pkt[3], hs_pkt[4]]);
    let dw_time = u32::from_le_bytes([hs_pkt[5], hs_pkt[6], hs_pkt[7], hs_pkt[8]]);
    // Eco con el reloj ALINEADO al servidor (parity ELTimer_SetServerMSec) —
    // el handshake server-side valida el bias.
    conn.send(&TPacketCGHandshake::new(nonce, dw_time, 0).to_bytes())
        .await
        .expect("eco CG_HANDSHAKE");

    // Tras el eco, el servidor manda GC_PHASE(PHASE_AUTH) — el cliente lo
    // necesita para enviar el LOGIN3.
    let auth_phase = read_exact_size(conn, TPacketGCPhase::SIZE)
        .await
        .expect("GC_PHASE(AUTH) tras el eco");
    let gc_auth_phase = TPacketGCPhase::from_bytes(&auth_phase).expect("parse GC_PHASE(AUTH)");
    assert_eq!(gc_auth_phase.phase, phase::AUTH, "phase AUTH");
}

#[tokio::test]
async fn auth_handles_login3_with_db_down() {
    let config_path = write_temp_config("login3");
    let (mut child, addr) = spawn_auth(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("connect {addr}: {e}"))?;
        let mut conn = Connection::new(stream);
        client_handshake(&mut conn).await;

        // LOGIN3 al auth = 68 B (65 + lang[3], packet_info.cpp:157).
        let login3 = TPacketCGLogin3::new_auth("test", "1234", [0; 4], "es").to_bytes_auth();
        conn.send(&login3)
            .await
            .map_err(|e| format!("LOGIN3: {e}"))?;

        // Respuesta: GC_AUTH_SUCCESS (6 B) — con la DB caída, bResult=0 y
        // key=0 (parity input_db.cpp:1719-1726: el C++ nunca manda
        // GC_LOGIN_FAILURE por credenciales).
        let hdr = read_exact_size(&mut conn, 1)
            .await
            .map_err(|e| format!("header respuesta: {e}"))?;
        assert_eq!(hdr[0], 0x96, "GC_AUTH_SUCCESS (0x96)");
        let rest = read_exact_size(&mut conn, 5)
            .await
            .map_err(|e| format!("resto: {e}"))?;
        let mut pkt = hdr;
        pkt.extend_from_slice(&rest);
        let auth = TPacketGCAuthSuccess::from_bytes(&pkt).map_err(|e| e.to_string())?;
        assert_eq!(auth.b_result, 0, "bResult=0 con la DB caída");
        assert_eq!(auth.dw_login_key, 0, "key=0 en fallo");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);

    result.expect("flujo auth con DB caída");
}

/// F2b: LOGIN3 de 88 B con version BUENA (40999) + hwid → el auth lo acepta y
/// sigue (con la DB caída → bResult=0).
#[tokio::test]
async fn auth_accepts_full_login3_with_good_version() {
    let config_path = write_temp_config("goodver");
    let (mut child, addr) = spawn_auth(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("connect {addr}: {e}"))?;
        let mut conn = Connection::new(stream);
        client_handshake(&mut conn).await;

        let login3 = TPacketCGLogin3::new_auth("test", "1234", [0; 4], "es")
            .to_bytes_auth_with(Some(40999), Some([0xAB; 16]));
        assert_eq!(login3.len(), 88, "LOGIN3 auth completo (F2b)");
        conn.send(&login3)
            .await
            .map_err(|e| format!("LOGIN3: {e}"))?;

        // El version gate pasa → el auth responde GC_AUTH_SUCCESS (bResult=0
        // por la DB caída) — no un cierre por versión.
        let hdr = read_exact_size(&mut conn, 1)
            .await
            .map_err(|e| format!("header: {e}"))?;
        assert_eq!(
            hdr[0], 0x96,
            "GC_AUTH_SUCCESS — la version buena no se rechaza"
        );
        let rest = read_exact_size(&mut conn, 5)
            .await
            .map_err(|e| format!("resto: {e}"))?;
        let mut pkt = hdr;
        pkt.extend_from_slice(&rest);
        let auth = TPacketGCAuthSuccess::from_bytes(&pkt).map_err(|e| e.to_string())?;
        assert_eq!(auth.b_result, 0, "bResult=0 (DB caída)");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("login3 88 B con version buena");
}

/// F2b: LOGIN3 con version MALA → cierre limpio (EOF, sin paquetes) — el
/// version gate rechaza antes de cualquier respuesta.
#[tokio::test]
async fn auth_rejects_bad_version_with_clean_close() {
    let config_path = write_temp_config("badver");
    let (mut child, addr) = spawn_auth(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("connect {addr}: {e}"))?;
        let mut conn = Connection::new(stream);
        client_handshake(&mut conn).await;

        // version 99999 ≠ 40999 (default del config).
        let login3 = TPacketCGLogin3::new_auth("test", "1234", [0; 4], "es")
            .to_bytes_auth_with(Some(99_999), Some([0xCD; 16]));
        assert_eq!(login3.len(), 88);
        conn.send(&login3)
            .await
            .map_err(|e| format!("LOGIN3: {e}"))?;

        // El auth cierra sin responder → EOF limpio.
        let mut b = [0u8; 1];
        let n = conn.recv(&mut b).await.map_err(|e| format!("recv: {e}"))?;
        assert_eq!(n, 0, "EOF limpio — el version gate cierra sin paquetes");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("rechazo limpio por version mala");
}

/// El timeout global de conexión cierra una conexión muda (deuda F1.5).
#[tokio::test]
async fn auth_times_out_silent_connection() {
    let config_path = write_temp_config("timeout");
    // Config propio con timeout corto (1 s) para que el test sea rápido.
    let _ = std::fs::write(
        &config_path,
        "listen = \"127.0.0.1:0\"\n\
         pg_conn = \"host=127.0.0.1 port=1 user=mt2 password=mt2 dbname=metin2\"\n\
         timeout_ms = 1000\n\
         no_more_clients = false\n",
    );
    let (mut child, addr) = spawn_auth(&config_path);

    let stream = TcpStream::connect(&addr).await.expect("connect");
    // Leemos lo que el auth envía (GC_PHASE + GC_HANDSHAKE) y NO respondemos.
    let mut conn = Connection::new(stream);
    let mut buf = [0u8; 2];
    let _ = tokio::time::timeout(Duration::from_secs(3), conn.recv(&mut buf)).await;

    // El auth debe cerrar la conexión (timeout global de 1 s tras el
    // handshake... el handshake intenta 32×500ms — el timeout global lo corta).
    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        let mut b = [0u8; 1];
        loop {
            match conn.recv(&mut b).await {
                Ok(0) => return true, // EOF: el auth cerró
                Ok(_) => continue,
                Err(_) => return true,
            }
        }
    })
    .await;
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    assert!(
        closed.unwrap_or(false),
        "el auth debe cerrar la conexión muda por timeout"
    );
}
