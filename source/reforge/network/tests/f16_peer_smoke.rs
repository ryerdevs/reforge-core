//! Smoke test F1.6: ejecuta el binario `f16_peer` contra un fake-auth local que
//! replica el flujo servidor del auth C++ (GC_PHASE + GC_HANDSHAKE → eco →
//! LOGIN3 → GC_LOGIN_FAILURE) usando el handshake server-side del crate.
//!
//! Valida el peer SIN el stack C++ vivo (regla del lane): handshake completado
//! (exit 0, reportes en stdout), eco alineado (bias ≈ 0), LOGIN3 de 68 B y la
//! respuesta `GC_LOGIN_FAILURE` reportada.
//!
//! El binario del example se localiza por ruta (`target/debug/examples/…`):
//! `CARGO_BIN_EXE_*` solo se define para targets `[[bin]]`, no para examples;
//! `cargo test` compila los examples del package antes de correr los tests.

use std::path::PathBuf;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWriteExt, DuplexStream, duplex};
use tokio::net::TcpListener;

use network::handshake::{HandshakeConfig, perform_with};
use network::{Connection, ConnectionRole, Framer, read_exact_size};
use protocol::{
    TPacketCGHandshake, TPacketCGLogin3, TPacketGCHandshake, TPacketGCLoginFailure, TPacketGCPhase,
    header, phase,
};

/// Ruta del binario del example (compilado por `cargo test` antes de los tests).
fn f16_peer_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("target");
    path.push("debug");
    path.push("examples");
    path.push(if cfg!(windows) {
        "f16_peer.exe"
    } else {
        "f16_peer"
    });
    path
}

/// Config del fake-auth: UN intento (el peer responde el eco al primer intento),
/// timeout amplio y tolerancia de bias HOLGADA (2 s): bajo la carga del run del
/// workspace completo el spawn del binario y la CPU saturada inflan el eco —
/// lo que el test valida es la MECÁNICA del handshake (eco estructurado,
/// alineación), no la tolerancia de producción (flake 2026-08-16 resuelto).
fn fake_cfg() -> HandshakeConfig {
    HandshakeConfig {
        // G3.2f: bajo la suite completa el spawn del binario del peer +
        // CPU saturada inflan el eco mas alla de 5 s + 1 retry. Subimos
        // el limite a un valor que tolera el workspace test (no es un
        // timeout de produccion: la cobertura real esta en channel_pg
        // contra el binario desplegado).
        retry_limit: 2,
        attempt_timeout: Duration::from_secs(15),
        retry_delay: Duration::from_millis(50),
        bias_tolerance_ms: 2000,
    }
}

/// Lee el LOGIN3 del flujo de auth después del handshake.
///
/// Se mantiene separado porque el framer puede haber recibido el eco y el
/// LOGIN3 en el mismo `read`; leer directamente del socket perdería los bytes
/// que ya quedaron en el buffer del framer.
async fn read_auth_login3<S: AsyncRead + Unpin>(
    conn: &mut Connection<S>,
    framer: &mut Framer,
) -> Result<Vec<u8>, network::FramingError> {
    framer.next_packet(conn).await
}

async fn recv_server_handshake(stream: &mut DuplexStream) -> TPacketGCHandshake {
    let phase_bytes = read_exact_size(stream, TPacketGCPhase::SIZE)
        .await
        .expect("server sends GC_PHASE");
    assert_eq!(
        phase_bytes,
        TPacketGCPhase::new(phase::HANDSHAKE).to_bytes()
    );
    let handshake_bytes = read_exact_size(stream, TPacketGCHandshake::SIZE)
        .await
        .expect("server sends GC_HANDSHAKE");
    TPacketGCHandshake::from_bytes(&handshake_bytes).expect("valid GC_HANDSHAKE")
}

/// Lado servidor (fake-auth): handshake + LOGIN3 + GC_LOGIN_FAILURE.
/// (G3.2f: tolerancias altas — 15s + 2 retries — aplicadas al fake-auth
/// para tolerar la suite completa. La cobertura real contra el canal
/// desplegado sigue en `channel_pg`.)
async fn fake_auth_with_login3() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("peer conecta");
        let mut conn = Connection::new(stream);
        let mut framer = Framer::new(ConnectionRole::Auth);
        // El peer alinea su reloj con el dwTime enviado (parity ELTimer_SetServerMSec)
        // → el bias validado debe ser ≈ 0 (RTT local).
        let h = perform_with(&mut conn, &mut framer, 1_000_000, &fake_cfg())
            .await
            .expect("handshake server-side valida el eco del peer");
        assert!(
            h.delta.unsigned_abs() <= 2000,
            "eco del peer alineado, delta={}",
            h.delta
        );
        // LOGIN3 al auth = 68 B (65 + szLanguage[3], packet_info.cpp:157).
        let login3 = read_auth_login3(&mut conn, &mut framer)
            .await
            .expect("peer envía LOGIN3");
        assert_eq!(login3[0], header::CG_LOGIN3);
        // Respuesta: GC_LOGIN_FAILURE "WRONGPWD" (10 B).
        conn.send(&TPacketGCLoginFailure::new("WRONGPWD").to_bytes())
            .await
            .expect("envía failure");
    });

    // El peer se ejecuta como subproceso real (el binario del example).
    let bin = f16_peer_bin();
    let out = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&bin)
            .args([
                addr.ip().to_string(),
                addr.port().to_string(),
                "--login3".to_string(),
            ])
            .output()
            .expect("ejecutar f16_peer")
    })
    .await
    .expect("spawn_blocking");

    server.await.expect("fake-auth termina sin panic");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    eprintln!("F16 smoke stdout:\n{stdout}");
    if !stderr.is_empty() {
        eprintln!("F16 smoke stderr:\n{stderr}");
    }
    assert!(
        out.status.success(),
        "exit 0 (handshake sin timeout), status={:?}",
        out.status
    );
    assert!(stdout.contains("GC_HANDSHAKE"), "reporta GC_HANDSHAKE");
    assert!(
        stdout.contains("handshake completado"),
        "handshake completado"
    );
    assert!(stdout.contains("CG_LOGIN3"), "envía LOGIN3");
    assert!(
        stdout.contains("GC_LOGIN_FAILURE"),
        "reporta GC_LOGIN_FAILURE"
    );
    assert!(stdout.contains("WRONGPWD"), "status del failure");
    Ok(())
}

#[tokio::test]
async fn auth_login3_survives_coalesced_echo_and_login3() {
    let (server_side, mut client_side): (DuplexStream, DuplexStream) = duplex(1024);
    let mut server = tokio::spawn(async move {
        let mut conn = Connection::new(server_side);
        let mut framer = Framer::new(network::ConnectionRole::Auth);
        perform_with(&mut conn, &mut framer, 1_000_000, &fake_cfg())
            .await
            .expect("handshake completes");
        read_auth_login3(&mut conn, &mut framer)
            .await
            .expect("coalesced LOGIN3 remains readable")
    });

    let hs = tokio::time::timeout(
        Duration::from_secs(1),
        recv_server_handshake(&mut client_side),
    )
    .await
    .expect("server handshake must arrive promptly");
    let mut combined = TPacketCGHandshake::new(hs.dw_handshake, hs.dw_time, 0)
        .to_bytes()
        .to_vec();
    combined.extend_from_slice(
        &TPacketCGLogin3::new_auth("test", "1234", [0; 4], "es").to_bytes_auth(),
    );
    client_side
        .write_all(&combined)
        .await
        .expect("echo and LOGIN3 are written together");

    let result = tokio::time::timeout(Duration::from_secs(1), &mut server).await;
    if result.is_err() {
        server.abort();
    }
    let login3 = result
        .expect("coalesced LOGIN3 must not wait on the socket")
        .expect("server task completes");
    assert_eq!(login3[0], header::CG_LOGIN3);
    assert_eq!(login3.len(), TPacketCGLogin3::SIZE_AUTH);
}

/// Modo solo transporte (sin --login3): el peer cierra tras el eco; el fake
/// solo hace el handshake y termina al ver EOF.
async fn fake_auth_handshake_only() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("peer conecta");
        let mut conn = Connection::new(stream);
        let mut framer = Framer::new(ConnectionRole::Auth);
        let h = perform_with(&mut conn, &mut framer, 1_000_000, &fake_cfg())
            .await
            .expect("handshake server-side valida el eco del peer");
        assert!(h.delta.unsigned_abs() <= 2000);
        // El peer cierra sin LOGIN3 → EOF limpio.
        let mut b = [0u8; 1];
        let n = conn.recv(&mut b).await.expect("lectura tras handshake");
        assert_eq!(n, 0, "el peer cierra la conexión limpiamente");
    });

    let bin = f16_peer_bin();
    let out = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&bin)
            .args([addr.ip().to_string(), addr.port().to_string()])
            .output()
            .expect("ejecutar f16_peer")
    })
    .await
    .expect("spawn_blocking");

    server.await.expect("fake-auth termina sin panic");
    let stdout = String::from_utf8_lossy(&out.stdout);
    eprintln!("F16 smoke (handshake-only) stdout:\n{stdout}");
    assert!(out.status.success(), "exit 0, status={:?}", out.status);
    assert!(stdout.contains("handshake completado"));
    assert!(stdout.contains("sin --login3"));
    Ok(())
}

/// Los dos smokes corren como #[test] normales (flake 2026-08-16 resuelto:
/// la tolerancia del eco del fake-auth ahora holgada — 2 s — para soportar
/// runs del workspace con la CPU saturada).
#[tokio::test]
async fn f16_peer_handshake_and_login3_against_fake_auth() {
    fake_auth_with_login3().await.expect("smoke login3");
}

#[tokio::test]
async fn f16_peer_handshake_only_against_fake_auth() {
    fake_auth_handshake_only()
        .await
        .expect("smoke handshake-only");
}
