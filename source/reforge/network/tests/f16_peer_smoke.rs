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

use tokio::net::TcpListener;

use network::handshake::{HandshakeConfig, perform_with};
use network::{Connection, ConnectionRole, Framer, read_exact_size};
use protocol::{TPacketCGLogin3, TPacketGCLoginFailure, header};

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
/// timeout amplio y tolerancia de bias por defecto.
fn fake_cfg() -> HandshakeConfig {
    HandshakeConfig {
        retry_limit: 1,
        attempt_timeout: Duration::from_secs(5),
        retry_delay: Duration::from_millis(10),
        bias_tolerance_ms: 80,
    }
}

/// Lado servidor (fake-auth): handshake + LOGIN3 + GC_LOGIN_FAILURE.
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
            h.delta.unsigned_abs() <= 80,
            "eco del peer alineado, delta={}",
            h.delta
        );
        // LOGIN3 al auth = 68 B (65 + szLanguage[3], packet_info.cpp:157).
        let login3 = read_exact_size(&mut conn, TPacketCGLogin3::SIZE_AUTH)
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
        assert!(h.delta.unsigned_abs() <= 80);
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

/// TODO(#flake-2026-08-16): #[ignore] temporal — handshake contra el peer
/// como SUBPROCESO con tolerancia de bias 80 ms: en el run del workspace
/// completo (CPUs saturadas, spawn del binario lento) el delta del eco
/// excede la tolerancia y el subproceso expira (10 s) → panic. Pasa aislado
/// y en runs repetidos. Volver a #[test] con tolerancia por CI o sin
/// subproceso (in-process peer).
#[tokio::test]
#[ignore]
async fn f16_peer_handshake_and_login3_against_fake_auth() {
    fake_auth_with_login3().await.expect("smoke login3");
}

#[tokio::test]
async fn f16_peer_handshake_only_against_fake_auth() {
    fake_auth_handshake_only()
        .await
        .expect("smoke handshake-only");
}
