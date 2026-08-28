//! `channel/locale.rs` — F1 (ADR-0009): push del locale del jugador al
//! conectar (lengua = columna `lang` del QUERY_LOGIN — parity input_db.cpp:
//! 150-164): bundle `GC_LOCALE` (140) chunked — TPacketGCLocale, parity
//! `AccountConnector.cpp:288-320`; el cliente legacy lo descarta
//! (PythonNetworkStream.cpp:159-163); el data channel lo aplica.
//! FAIL-OPEN: error de PG → log y SIN push (el AUTH ya sirvió el bundle).
//!
//! PULL (locale-redesign §hot reload): `CG_LOCALE_REQUEST` (132, 4 B) en
//! fase game → `handle` responde el bundle de la lengua PEDIDA (stateless —
//! ambos roles sirven el par; el auth cierra, el canal ignora con log, C6a).

use database::locale::LocaleRepo;
use protocol::locale::{encode_chunks, encode_payload, CgLocaleRequest, LocaleBundle};

use crate::auth::{extract_lang, GC_LOCALE_MAX_CHUNK};
use crate::channel::session::{Outcome, Session};

/// Nombres de las 16 lenguas (parity locale.cpp:20-24, UPPERCASE — `strcasecmp`).
const LANG_NAMES: [&str; 16] = [
    "AE", "CZ", "DE", "DK", "EN", "ES", "FR", "GR", "HU", "IT", "NL", "PL", "PT", "RO", "RU", "TR",
];

/// Índice de la lengua — "es" → 5 (input_db.cpp:150-158); desconocida = ES.
pub fn lang_index(lang: &str) -> u8 {
    LANG_NAMES.iter().position(|n| n.eq_ignore_ascii_case(lang)).map_or(5, |i| i as u8)
}

/// Chunks wire del bundle — el camino EXACTO del push.
fn locale_chunks(bundle: &LocaleBundle) -> Vec<Vec<u8>> {
    encode_chunks(&encode_payload(bundle), GC_LOCALE_MAX_CHUNK)
}

/// Handler del push AL CONECTAR (fin del entry): `GC_LOCALE` (140) chunked
/// de la lengua de la cuenta; `Ok(())` siempre (fail-open).
pub async fn send_player_locale(session: &mut Session, lang: &str) -> Result<(), String> {
    let bundle = match LocaleRepo::new(session.pool.clone()).load_for_lang(lang).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("server_realms: channel conn {}: locale {lang}: {e} — push omitido (fail-open)", session.conn_id);
            return Ok(());
        }
    };
    let chunks = locale_chunks(&bundle);
    eprintln!(
        "server_realms: channel conn {}: GC_LOCALE envío lang={lang} (idx {}) — {} pares, {} B, {} chunks",
        session.conn_id, lang_index(lang), bundle.len(),
        chunks.iter().map(|c| c.len() - 4).sum::<usize>(), chunks.len()
    );
    for chunk in chunks {
        session.send(&chunk).await.map_err(|e| format!("enviando GC_LOCALE: {e}"))?;
    }
    Ok(())
}

/// PULL (F1, ADR-0009): `CG_LOCALE_REQUEST` (132, 4 B — el framer ya lo
/// entrega entero en ambos roles) → `GC_LOCALE` (140) chunked de la lengua
/// PEDIDA (stateless, parity auth). Lang inválida → log + Continue (el canal
/// no cierra — divergencia del auth, C6a); PG caída → fail-open.
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let Ok(req) = CgLocaleRequest::from_bytes(pkt) else {
        eprintln!(
            "server_realms: channel conn {}: CG_LOCALE_REQUEST malformado ({})",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    };
    let Some(lang) = extract_lang(&req.lang) else {
        eprintln!(
            "server_realms: channel conn {}: CG_LOCALE_REQUEST lang inválida — ignorado",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    };
    send_player_locale(session, &lang).await?;
    Ok(Outcome::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::locale::{decode_chunks, decode_payload, HEADER_GC_LOCALE};

    /// Verifier del push: lo que `send_player_locale` envía para "es" — un
    /// TPacketGCLocale cuyo payload reensambla al bundle ES intacto ("Perro
    /// Salvaje" — AGENTS §16; `decode_chunks` valida header 0x8c/len/flag).
    #[test]
    fn push_sends_es_bundle() {
        let bundle = LocaleBundle {
            mob: vec![("101".into(), "Perro Salvaje".into())],
            ..LocaleBundle::default()
        };
        let chunks = locale_chunks(&bundle);
        let payload = decode_chunks(&[chunks[0].as_slice()]).expect("reensamblar");
        assert!(payload.windows(13).any(|w| w == b"Perro Salvaje"), "el nombre ES viaja");
        assert_eq!(decode_payload(&payload).expect("parsear"), bundle);
        assert_eq!(lang_index("es"), 5, "es → índice 5 (input_db.cpp:150-158)");
    }

    /// Índice: coincidencia case-insensitive; desconocida → default ES.
    #[test]
    fn lang_index_parity() {
        assert_eq!(lang_index("ES"), 5, "strcasecmp parity");
        assert_eq!(lang_index("en"), 4);
        assert_eq!(lang_index("xx"), 5, "default ES (locale.hpp:40)");
    }

    /// VERIFIER del PULL (gated — PG real): `CG_LOCALE_REQUEST`("es") sobre
    /// una Session REAL en loopback → `handle` responde `GC_LOCALE` chunked →
    /// reensamblado → bundle ES intacto (mob 2.876 — el mismo dump del auth).
    #[tokio::test]
    #[ignore = "requiere la PG real (host=127.0.0.1:5432, bd metin2)"]
    async fn pull_request_to_response() {
        use tokio::io::AsyncReadExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let mut peer = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (server_side, _) = listener.accept().await.expect("accept");
        let pool = database::pool::new_pool(
            &std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| {
                "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2".into()
            }),
            2,
        )
        .expect("pool (lazy)");
        let wal = std::env::temp_dir()
            .join(format!("locale_pull_wal_{}", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let batcher = std::sync::Arc::new(database::wal::Batcher::spawn(
            std::time::Duration::from_millis(100),
            64,
            database::wal::WalSink::new(database::wal::PgMutationSink::new(pool.clone()), wal),
        ));
        let (intent_tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut s = Session::new(
            server_side,
            crate::config::Config::default(),
            1,
            intent_tx,
            std::sync::Arc::new(std::sync::Mutex::new(game_core::map::MapStore::new())),
            pool,
            batcher,
            std::sync::Arc::new(database::attr::AttrTables::default()),
        );
        handle(&mut s, &CgLocaleRequest::new("es").to_bytes()).await.expect("pull responde");
        // Lee los chunks del GC_LOCALE hasta el flag final (0) — layout
        // 0x8c + u16 payload_len (flag + chunk) — parity request_locale.
        let mut payload = Vec::new();
        loop {
            let mut hdr = [0u8; 3];
            peer.read_exact(&mut hdr).await.expect("hdr GC_LOCALE");
            assert_eq!(hdr[0], HEADER_GC_LOCALE, "header 0x8c");
            let n = u16::from_le_bytes([hdr[1], hdr[2]]) as usize;
            let mut body = vec![0u8; n];
            peer.read_exact(&mut body).await.expect("cuerpo");
            let flag = body[0];
            assert!(flag <= 1, "chunk_flag 0/1");
            payload.extend_from_slice(&body[1..]);
            if flag == 0 {
                break;
            }
        }
        let bundle = decode_payload(&payload).expect("payload ES");
        assert_eq!(bundle.mob.len(), 2_876, "mob ES completo (dump 2026-08-12)");
        assert_eq!(bundle.item.len(), 11_427, "item ES");
    }
}
