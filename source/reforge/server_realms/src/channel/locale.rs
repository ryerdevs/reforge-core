//! `channel/locale.rs` — F1 (ADR-0009): push del locale del jugador al
//! conectar (lengua = columna `lang` del QUERY_LOGIN — parity input_db.cpp:
//! 150-164): bundle `GC_LOCALE` (140) chunked — TPacketGCLocale, parity
//! `AccountConnector.cpp:288-320`; el cliente legacy lo descarta
//! (PythonNetworkStream.cpp:159-163); el data channel lo aplica.
//! FAIL-OPEN: error de PG → log y SIN push (el AUTH ya sirvió el bundle).

use database::locale::LocaleRepo;
use protocol::locale::{encode_chunks, encode_payload, LocaleBundle};

use crate::auth::GC_LOCALE_MAX_CHUNK;
use crate::channel::session::Session;

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
        "server_realms: channel conn {}: GC_LOCALE push lang={lang} (idx {}) — {} pares, {} B, {} chunks",
        session.conn_id, lang_index(lang), bundle.len(),
        chunks.iter().map(|c| c.len() - 4).sum::<usize>(), chunks.len()
    );
    for chunk in chunks {
        session.send(&chunk).await.map_err(|e| format!("enviando GC_LOCALE: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::locale::{decode_chunks, decode_payload};

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
}