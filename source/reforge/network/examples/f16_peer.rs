//! F1.6 — peer de integración: CLIENTE del auth C++ legacy (hito F1.6, ROADMAP
//! §Phase 1: "la C++ auth binary se conecta con un peer Rust y viceversa sin
//! timeouts ni WRITE floods").
//!
//! Uso:
//! ```text
//! cargo run --example f16_peer -- <host> <port> [--login3] [--version <n>] [--hwid <hex32>]
//! ```
//! - `<host>`: `172.25.104.175` (IP WSL eth0) o `127.0.0.1` (peer dentro de WSL).
//! - `<port>`: `30001` (auth C++/Rust).
//! - `--login3`: además del handshake, envía `CG_LOGIN3` (test/1234) y reporta
//!   la respuesta del auth (informativo — el objetivo F1.6 es TRANSPORTE).
//! - `--version <n>` (F2b): añade el campo `version` (DWORD LE) al LOGIN3
//!   auth → 72 B. Default: sin version (68 B, cliente actual).
//! - `--hwid <hex32>` (F2b): añade 16 bytes de hwid → 88 B (requiere
//!   `--version`).
//!
//! Exit: `0` = el handshake completó sin timeout (aunque el login sea
//! rechazado — `GC_LOGIN_FAILURE` es un resultado de transporte válido);
//! `1` = timeout global (10 s) o error.
//!
//! # Qué reutiliza de `network` (crate F1.1–F1.5)
//!
//! - [`network::Connection`] — par read/write con la semántica del contrato
//!   legacy (`send` = write_all sin flood; `connection.rs:49-51`).
//! - [`network::read_exact_size`] — lectura de paquetes servidor→cliente
//!   (structs crudos sin prefijo; `framer.rs:200-204`).
//! - Structs de `protocol`: `TPacketGCPhase`, `TPacketGCHandshake`,
//!   `TPacketCGHandshake`, `TPacketCGLogin3`, `TPacketGCAuthSuccess`.
//!
//! El módulo [`network::handshake`] está orientado al SERVIDOR (envía
//! GC_PHASE+GC_HANDSHAKE y valida el eco, `handshake.rs:208-282`); el lado
//! CLIENTE (recibir GC_* y responder el eco) se implementa aquí con las mismas
//! primitivas públicas — sin cambios en `network`. El eco replica al cliente
//! legacy: reloj ALINEADO al `dwTime` del servidor (`ELTimer_SetServerMSec`) y
//! `l_delta = 0` — el auth C++ valida el bias de reloj en [0, 50] ms unilateral
//! (`desc.cpp:701`); con el reloj alineado el bias ≈ RTT.

use std::process::ExitCode;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::time::timeout;

use network::{Connection, read_exact_size};
use protocol::header;
use protocol::phase;
use protocol::{
    TPacketCGHandshake, TPacketCGLogin3, TPacketGCAuthSuccess, TPacketGCHandshake, TPacketGCPhase,
};

/// HWID como hex para el log (16 B → 32 chars).
fn hex16(h: &[u8; 16]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// Timeout global estricto de todo el peer (connect + handshake + respuesta).
const GLOBAL_TIMEOUT: Duration = Duration::from_secs(10);

struct Args {
    host: String,
    port: u16,
    login3: bool,
    version: Option<u32>,
    hwid: Option<[u8; 16]>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut it = args.iter();
    let host = it
        .next()
        .ok_or("uso: f16_peer <host> <port> [--login3] [--version <n>] [--hwid <hex32>]")?;
    let port = it
        .next()
        .ok_or("uso: f16_peer <host> <port> [--login3] [--version <n>] [--hwid <hex32>]")?
        .parse::<u16>()
        .map_err(|e| format!("puerto inválido: {e}"))?;
    let mut login3 = false;
    let mut version = None;
    let mut hwid = None;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--login3" => login3 = true,
            "--version" => {
                let v = it.next().ok_or("--version requiere un valor (u32)")?;
                version = Some(
                    v.parse::<u32>()
                        .map_err(|e| format!("version inválida: {e}"))?,
                );
            }
            "--hwid" => {
                let v = it.next().ok_or("--hwid requiere 32 chars hex (16 bytes)")?;
                let mut bytes = [0u8; 16];
                if v.len() != 32 {
                    return Err("--hwid debe tener 32 chars hex (16 bytes)".into());
                }
                for (i, chunk) in v.as_bytes().chunks_exact(2).enumerate() {
                    let hi = (chunk[0] as char).to_digit(16).ok_or("hwid no es hex")? as u8;
                    let lo = (chunk[1] as char).to_digit(16).ok_or("hwid no es hex")? as u8;
                    bytes[i] = (hi << 4) | lo;
                }
                hwid = Some(bytes);
            }
            other => return Err(format!("argumento desconocido: {other}")),
        }
    }
    if hwid.is_some() && version.is_none() {
        return Err("--hwid requiere --version (el hwid va tras la version en el LOGIN3)".into());
    }
    Ok(Args {
        host: host.clone(),
        port,
        login3,
        version,
        hwid,
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("F16: {e}");
            return ExitCode::from(2);
        }
    };
    match timeout(GLOBAL_TIMEOUT, run(&args)).await {
        Err(_) => {
            eprintln!(
                "F16: TIMEOUT global ({} s) — el auth no completó el handshake",
                GLOBAL_TIMEOUT.as_secs()
            );
            ExitCode::from(1)
        }
        Ok(Err(e)) => {
            eprintln!("F16: {e}");
            ExitCode::from(1)
        }
        Ok(Ok(())) => ExitCode::SUCCESS,
    }
}

async fn run(args: &Args) -> Result<(), String> {
    // 1) TCP connect (dentro del timeout global).
    let stream = TcpStream::connect((args.host.as_str(), args.port))
        .await
        .map_err(|e| format!("connect {}:{}: {e}", args.host, args.port))?;
    let mut conn = Connection::new(stream);
    println!("F16: conectado a {}:{}", args.host, args.port);

    // 2) S→C: GC_PHASE (2 B) + GC_HANDSHAKE (13 B) — structs crudos
    //    (spec §3: login-flow.md:68-69; parity desc.cpp:664-740).
    let phase_pkt = read_exact_size(&mut conn, TPacketGCPhase::SIZE)
        .await
        .map_err(|e| format!("leyendo GC_PHASE: {e}"))?;
    let gc_phase = TPacketGCPhase::from_bytes(&phase_pkt).map_err(|e| e.to_string())?;
    println!(
        "F16: <- GC_PHASE (0x{:02x}, {} B) phase={}",
        gc_phase.header,
        TPacketGCPhase::SIZE,
        gc_phase.phase
    );
    if gc_phase.phase != phase::HANDSHAKE {
        return Err(format!(
            "GC_PHASE inesperado: phase={} (esperado HANDSHAKE={})",
            gc_phase.phase,
            phase::HANDSHAKE
        ));
    }

    let hs_pkt = read_exact_size(&mut conn, TPacketGCHandshake::SIZE)
        .await
        .map_err(|e| format!("leyendo GC_HANDSHAKE: {e}"))?;
    let gc_hs = TPacketGCHandshake::from_bytes(&hs_pkt).map_err(|e| e.to_string())?;
    println!(
        "F16: <- GC_HANDSHAKE (0x{:02x}, {} B) nonce=0x{:08x} dwTime={} lDelta={}",
        gc_hs.header,
        TPacketGCHandshake::SIZE,
        gc_hs.dw_handshake,
        gc_hs.dw_time,
        gc_hs.l_delta
    );

    // 3) C→S: eco CG_HANDSHAKE (13 B, mismo layout). Reloj ALINEADO al del
    //    servidor (parity cliente legacy `ELTimer_SetServerMSec`) y l_delta=0:
    //    el auth valida `now - (dwTime + lDelta)` ∈ [0, 50] ms (desc.cpp:701).
    let echo = TPacketCGHandshake::new(gc_hs.dw_handshake, gc_hs.dw_time, 0).to_bytes();
    conn.send(&echo)
        .await
        .map_err(|e| format!("enviando CG_HANDSHAKE: {e}"))?;
    println!(
        "F16: -> CG_HANDSHAKE (0xff, {} B) nonce=0x{:08x} dwTime={} lDelta=0 (reloj alineado al servidor)",
        TPacketCGHandshake::SIZE,
        gc_hs.dw_handshake,
        gc_hs.dw_time
    );
    println!("F16: handshake completado (sin timeout, sin WRITE flood)");

    // 4) (opcional) LOGIN3 informativo.
    if !args.login3 {
        println!("F16: fin (sin --login3; el auth verá EOF limpio)");
        return Ok(());
    }
    // Al AUTH el LOGIN3 es 68 B = 65 + szLanguage[3] (spec §3:
    // login-flow.md:35,49; packet_info.cpp:157 registra 68 en auth). La tarea
    // citaba 65 B (el tamaño del canal); 65 contra el auth dejaría el paquete
    // incompleto (sin respuesta) — se envía la forma que el auth procesa.
    // F2b: `--version`/`--hwid` añaden los campos aditivos (72/88 B).
    let login3 = TPacketCGLogin3::new_auth("test", "1234", [0; 4], "es");
    let login3 = login3.to_bytes_auth_with(args.version, args.hwid);
    conn.send(&login3)
        .await
        .map_err(|e| format!("enviando CG_LOGIN3: {e}"))?;
    println!(
        "F16: -> CG_LOGIN3 (0x{:02x}, {} B) login=test pwd=1234 keys=0 lang=es version={:?} hwid={}",
        header::CG_LOGIN3,
        login3.len(),
        args.version,
        args.hwid.map(|h| hex16(&h)).unwrap_or_else(|| "-".into()),
    );

    // 5) Respuesta del auth: loop de headers (keepalives filtrados, F1.4).
    loop {
        let hdr_pkt = read_exact_size(&mut conn, 1)
            .await
            .map_err(|e| format!("leyendo respuesta del auth (EOF): {e}"))?;
        match hdr_pkt[0] {
            header::GC_AUTH_SUCCESS => {
                let rest = read_exact_size(&mut conn, TPacketGCAuthSuccess::SIZE - 1)
                    .await
                    .map_err(|e| format!("leyendo GC_AUTH_SUCCESS: {e}"))?;
                let mut pkt = hdr_pkt;
                pkt.extend_from_slice(&rest);
                let ok = TPacketGCAuthSuccess::from_bytes(&pkt).map_err(|e| e.to_string())?;
                println!(
                    "F16: <- GC_AUTH_SUCCESS (0x{:02x}, {} B) key=0x{:08x} result={} — LOGIN OK (transporte + auth completos)",
                    ok.header,
                    TPacketGCAuthSuccess::SIZE,
                    ok.dw_login_key,
                    ok.b_result
                );
                return Ok(());
            }
            header::GC_LOGIN_FAILURE => {
                // TPacketGCLoginFailure = header + szStatus[9] (10 B, packet.h;
                // input.cpp:215-226).
                let rest = read_exact_size(&mut conn, 9)
                    .await
                    .map_err(|e| format!("leyendo GC_LOGIN_FAILURE: {e}"))?;
                let status = String::from_utf8_lossy(&rest);
                println!(
                    "F16: <- GC_LOGIN_FAILURE (0x{:02x}, {} B) status={} — login rechazado (esperado: el objetivo es transporte)",
                    header::GC_LOGIN_FAILURE,
                    10,
                    status.trim_end_matches('\0')
                );
                return Ok(());
            }
            header::GC_PHASE => {
                // El auth manda GC_PHASE(PHASE_AUTH) tras el handshake (dispara
                // el LOGIN3 del cliente) y GC_PHASE(CLOSE) al cerrar — se
                // reporta y se sigue.
                let rest = read_exact_size(&mut conn, 1)
                    .await
                    .map_err(|e| format!("leyendo GC_PHASE: {e}"))?;
                println!(
                    "F16: <- GC_PHASE (0x{:02x}, {} B) phase={}",
                    header::GC_PHASE,
                    TPacketGCPhase::SIZE,
                    rest[0]
                );
                if rest[0] == phase::CLOSE {
                    println!("F16: el auth cerró la sesión (GC_PHASE CLOSE)");
                    return Ok(());
                }
            }
            // Keepalives (F1.4): time sync 13 B, pongs 1 B — se consumen y se
            // sigue esperando la respuesta.
            header::CG_TIME_SYNC => {
                let _ = read_exact_size(&mut conn, 12)
                    .await
                    .map_err(|e| format!("keepalive 0xfc: {e}"))?;
                println!("F16: <- CG_TIME_SYNC (0xfc, 13 B) — filtrado");
            }
            header::CG_PONG => {
                println!("F16: <- CG_PONG (0xfe, 1 B) — filtrado");
            }
            header::GC_PING => {
                println!(
                    "F16: <- GC_PING (0x{:02x}, 1 B) — filtrado",
                    header::GC_PING
                );
            }
            // PanamaPack 151 + hybrid-crypt 152/153: el auth los envía ANTES
            // del GC_AUTH_SUCCESS en login exitoso (spec login-flow.md:73,
            // input_db.cpp:1710-1716). Sin tamaños fijos (payload dinámico —
            // protocol::legacy, ADR-0006): se reportan y se SIGUE esperando
            // el GC_AUTH_SUCCESS (el auth no responde nada más después).
            151..=153 => {
                println!(
                    "F16: <- 0x{:02x} (PanamaPack/hybrid-crypt — login OK, auth completo)",
                    hdr_pkt[0]
                );
                // consumir el resto del paquete: u16 size + i32 len + stream.
                let size_hdr = read_exact_size(&mut conn, 6)
                    .await
                    .map_err(|e| format!("leyendo size de 0x{:02x}: {e}", hdr_pkt[0]))?;
                let size = u16::from_le_bytes([size_hdr[0], size_hdr[1]]) as usize;
                let stream_len =
                    i32::from_le_bytes([size_hdr[2], size_hdr[3], size_hdr[4], size_hdr[5]])
                        as usize;
                let _ = read_exact_size(&mut conn, stream_len)
                    .await
                    .map_err(|e| format!("leyendo stream de 0x{:02x}: {e}", hdr_pkt[0]))?;
                println!(
                    "F16:    (paquete 0x{:02x}: size {} stream_len {})",
                    hdr_pkt[0], size, stream_len
                );
            }
            other => {
                return Err(format!("header de respuesta desconocido: 0x{other:02x}"));
            }
        }
    }
}
