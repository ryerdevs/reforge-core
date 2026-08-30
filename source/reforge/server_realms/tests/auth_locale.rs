//! Gated F1 — `CG_LOCALE_REQUEST`/`GC_LOCALE` contra el auth REAL
//! (`server_realms --role auth`) con la PostgreSQL real (WSL): handshake →
//! CG_LOCALE_REQUEST("es") → chunks del GC_LOCALE → reensamblado →
//! secciones del bundle (mob ≈ 2.876, merge EN: las 3 descripciones de item
//! que ES no tiene). Patrón auth_smoke.rs (spawn del binario + config
//! temporal) + channel_pg.rs (PG real, gated con `#[ignore]`).
//!
//! Ejecutar desde WSL:
//!
//! ```text
//! source ~/.cargo/env
//! cd /mnt/c/projects/Metin2/source/reforge
//! cargo test -p server_realms --test auth_locale -- --ignored
//! ```

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use network::{Connection, read_exact_size};
use protocol::locale::{CgLocaleRequest, HEADER_GC_LOCALE, decode_payload};
use protocol::{TPacketCGHandshake, TPacketGCPhase, phase};
use tokio::net::TcpStream;

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";

fn pg_conn() -> String {
    std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
}

fn write_temp_config(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("f1_auth_locale_{name}.toml"));
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

/// Arranca el auth y espera la línea `auth escuchando en addr` del stdout
/// (patrón auth_smoke.rs).
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
        .unwrap_or_else(|_| panic!("el auth no anunció el listener (stdout)"));
    (child, addr)
}

/// Lado cliente del handshake legacy (parity auth_smoke.rs): GC_PHASE +
/// GC_HANDSHAKE → eco alineado → GC_PHASE(AUTH).
async fn client_handshake(conn: &mut Connection<TcpStream>) {
    let phase_pkt = read_exact_size(conn, TPacketGCPhase::SIZE)
        .await
        .expect("GC_PHASE");
    assert_eq!(
        TPacketGCPhase::from_bytes(&phase_pkt)
            .expect("parse GC_PHASE")
            .phase,
        phase::HANDSHAKE,
        "phase HANDSHAKE"
    );
    let hs_pkt = read_exact_size(conn, 13).await.expect("GC_HANDSHAKE");
    assert_eq!(hs_pkt[0], 0xff, "header GC_HANDSHAKE");
    let nonce = u32::from_le_bytes([hs_pkt[1], hs_pkt[2], hs_pkt[3], hs_pkt[4]]);
    let dw_time = u32::from_le_bytes([hs_pkt[5], hs_pkt[6], hs_pkt[7], hs_pkt[8]]);
    conn.send(&TPacketCGHandshake::new(nonce, dw_time, 0).to_bytes())
        .await
        .expect("eco CG_HANDSHAKE");
    let auth_phase = read_exact_size(conn, TPacketGCPhase::SIZE)
        .await
        .expect("GC_PHASE(AUTH)");
    assert_eq!(
        TPacketGCPhase::from_bytes(&auth_phase)
            .expect("parse GC_PHASE(AUTH)")
            .phase,
        phase::AUTH,
        "phase AUTH"
    );
}

/// Pide el locale y lee los chunks del GC_LOCALE hasta el flag final (0).
/// Devuelve (chunks crudos, payload reensamblado).
async fn request_locale(
    conn: &mut Connection<TcpStream>,
    lang: &str,
) -> Result<(usize, Vec<u8>), String> {
    conn.send(&CgLocaleRequest::new(lang).to_bytes())
        .await
        .map_err(|e| format!("CG_LOCALE_REQUEST: {e}"))?;
    let mut payload = Vec::new();
    let mut n_chunks = 0usize;
    loop {
        let hdr = read_exact_size(conn, 1)
            .await
            .map_err(|e| format!("header chunk: {e}"))?;
        assert_eq!(hdr[0], HEADER_GC_LOCALE, "header GC_LOCALE (0x8c)");
        let len_b = read_exact_size(conn, 2)
            .await
            .map_err(|e| format!("len chunk: {e}"))?;
        let len = u16::from_le_bytes([len_b[0], len_b[1]]) as usize;
        let body = read_exact_size(conn, len)
            .await
            .map_err(|e| format!("cuerpo chunk: {e}"))?;
        let flag = body[0];
        assert!(flag <= 1, "chunk_flag 0/1");
        payload.extend_from_slice(&body[1..]);
        n_chunks += 1;
        if flag == 0 {
            return Ok((n_chunks, payload));
        }
    }
}

/// Busca el par (clave, valor) en una sección.
fn has_pair(section: &[(String, String)], key: &str) -> bool {
    section.iter().any(|(k, _)| k == key)
}

/// F1 end-to-end: CG_LOCALE_REQUEST("es") → GC_LOCALE reensamblado con las
/// secciones reales de la PG (2.876 mobs ES) y el merge EN (las 3
/// descripciones de item que ES no tiene: 31084, 53526, 71219). Segunda
/// conexión con lang "zz" (inexistente) → bundle EN puro (fallback ADR-0009).
#[tokio::test]
#[ignore = "requiere la PG real de WSL (host=127.0.0.1:5432, bd metin2)"]
async fn auth_serves_locale_bundle_from_pg() {
    let config_path = write_temp_config("es");
    let (mut child, addr) = spawn_auth(&config_path);

    let result = async {
        // --- conexión 1: es (con merge EN) ---
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("connect {addr}: {e}"))?;
        let mut conn = Connection::new(stream);
        client_handshake(&mut conn).await;
        let (n_chunks, payload) = request_locale(&mut conn, "es").await?;
        eprintln!(
            "GC_LOCALE es: {n_chunks} chunks, {} B reensamblados",
            payload.len()
        );
        assert!(n_chunks > 1, "el payload real (~1 MB) exige chunking");
        let bundle = decode_payload(&payload).map_err(|e| format!("decode payload: {e:?}"))?;
        assert_eq!(bundle.mob.len(), 2_876, "mob ES completo (dump 2026-08-12)");
        assert_eq!(bundle.item.len(), 11_427, "item ES");
        assert_eq!(bundle.skill.len(), 134, "skill ES");
        assert_eq!(bundle.ui.len(), 1_301, "ui ES");
        // Merge EN (ADR-0009): las 3 descripciones que ES NO tiene → valores EN.
        assert_eq!(bundle.item_desc.len(), 7_499, "7.496 ES + 3 EN-only");
        assert!(
            has_pair(&bundle.item_desc, "31084"),
            "31084 Nimbus Tincture (EN-only)"
        );
        assert!(
            has_pair(&bundle.item_desc, "53526"),
            "53526 Pepita Can (EN-only)"
        );
        assert!(
            has_pair(&bundle.item_desc, "71219"),
            "71219 Invigorating Potion (EN-only)"
        );
        // map_names está vacía (sin fuente en el runtime — gap F1 documentado).
        assert!(bundle.map.is_empty(), "map_names vacía (gap documentado)");

        // --- conexión 2: lang inexistente → bundle EN puro (fallback) ---
        let stream2 = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("connect2 {addr}: {e}"))?;
        let mut conn2 = Connection::new(stream2);
        client_handshake(&mut conn2).await;
        let (_n2, payload2) = request_locale(&mut conn2, "zz").await?;
        let bundle2 = decode_payload(&payload2).map_err(|e| format!("decode payload2: {e:?}"))?;
        eprintln!(
            "GC_LOCALE zz: mob {} item {} item_desc {}",
            bundle2.mob.len(),
            bundle2.item.len(),
            bundle2.item_desc.len()
        );
        assert_eq!(bundle2.mob.len(), 2_876, "idioma inexistente → EN completo");
        assert_eq!(bundle2.item.len(), 11_427);
        assert_eq!(
            bundle2.item_desc.len(),
            7_499,
            "las EN-only también en el fallback"
        );
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("auth sirve el locale desde PG");
}

/// Lang inválido en el CG_LOCALE_REQUEST (no 2 letras + NUL) → cierre limpio
/// sin paquetes (EOF) — parity extract_lang del LOGIN3.
#[tokio::test]
#[ignore = "requiere la PG real de WSL (host=127.0.0.1:5432, bd metin2)"]
async fn auth_rejects_invalid_locale_lang_with_clean_close() {
    let config_path = write_temp_config("badlang");
    let (mut child, addr) = spawn_auth(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("connect {addr}: {e}"))?;
        let mut conn = Connection::new(stream);
        client_handshake(&mut conn).await;
        // "x" + NULs: 1 letra, no 2 → inválido (extract_lang).
        let mut req = CgLocaleRequest::new("x");
        req.lang = [b'x', 0, 0];
        conn.send(&req.to_bytes())
            .await
            .map_err(|e| format!("CG_LOCALE_REQUEST: {e}"))?;
        // Cierre limpio: EOF sin ningún paquete.
        let mut b = [0u8; 1];
        let n = conn.recv(&mut b).await.map_err(|e| format!("recv: {e}"))?;
        assert_eq!(n, 0, "EOF limpio — lang inválido cierra sin GC_LOCALE");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("rechazo limpio por lang inválido");
}
