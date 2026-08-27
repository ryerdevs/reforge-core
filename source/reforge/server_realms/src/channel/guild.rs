//! `channel/guild.rs` — GUILD slice 2026-08-27: `CG_GUILD_CREATE` (80/1) →
//! `game_core::guild::create_guild` + ack `GC_GUILD` INFO (parity
//! `SendGuildInfoPacket` guild.cpp:867-897) + `GC_CHAT` INFO ok/error
//! (parity `AnswerMakeGuild` input_main.cpp:2364-2385 y errores de
//! `CreateGuild` guild_manager.cpp:77-107).
//!
//! Persistencia: SOLO MEMORIA (registro del proceso — el `database/src/
//! guild.rs` del slice no existe; el INSERT a `player.guild` PG es un slice
//! futuro — las guildas mueren con el canal, GAP documentado). El store es
//! el equivalente de `m_mapGuild` de `CGuildManager` (guild_manager.cpp:112).
//! DIVERGENCIA del wire (documentada en `protocol::guild`): sub 1 = CREATE
//! (el legacy lo usa para REMOVE_MEMBER; la creación legacy va por 82/81).
//! Textos INFO en EN (divergencia establecida como gm.rs/messenger.rs).

use std::sync::{Mutex, OnceLock};

use game_core::guild::{create_guild, Guild, GuildError};
use protocol::{guild as pg, header};

use crate::channel::session::{Outcome, Session};

/// Registro de guildas del PROCESO (m_mapGuild — guild_manager.cpp:112).
fn guilds() -> &'static Mutex<Vec<Guild>> {
    static G: OnceLock<Mutex<Vec<Guild>>> = OnceLock::new();
    G.get_or_init(|| Mutex::new(Vec::new()))
}

/// GC_CHAT dirigido (id=0 — parity ChatPacket char.cpp:3947-3948).
fn gc_chat(empire: u8, text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + text.len());
    out.push(header::GC_CHAT);
    out.extend_from_slice(&((9 + text.len()) as u16).to_le_bytes());
    out.push(1); // CHAT_TYPE_INFO (length.h:258-275)
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(empire);
    out.extend_from_slice(text.as_bytes());
    out
}

/// CG_GUILD (80) — dispatch por subheader. El framer ya entregó el paquete
/// completo; subheader sin handler → log + Continue (patrón messenger).
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() < pg::CG_FIXED {
        return Ok(Outcome::Continue);
    }
    match pkt[1] {
        pg::SUB_CG_CREATE => create(session, pkt).await?,
        other => eprintln!(
            "server_realms: channel conn {}: guild subheader {other} sin handler (slice)",
            session.conn_id
        ),
    }
    Ok(Outcome::Continue)
}

/// CREATE (80/1): nombre crudo de 13 B (NUL-stripped, strlcpy parity —
/// patrón `name_at` de messenger.rs) → `create_guild` contra los nombres
/// existentes → GC_GUILD INFO + GC_CHAT ok, o GC_CHAT error.
async fn create(session: &mut Session, pkt: &[u8]) -> Result<(), String> {
    let raw = &pkt[2..pg::CG_CREATE_TOTAL.min(pkt.len())];
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    let name = String::from_utf8_lossy(&raw[..end]).into_owned();
    // La decisión (valida + inserta) ocurre en un BLOQUE con el lock; los
    // sends van después — el guard de std Mutex no cruza await (la tarea de
    // conexión es tokio::spawn → future Send).
    let (guild, msg) = {
        let mut store = guilds().lock().expect("guild store lock");
        let existing: Vec<&str> = store.iter().map(|g| g.name.as_str()).collect();
        let result = match create_guild(store.len() as i64 + 1, &name, &existing) {
            Ok(g) => {
                let msg = format!("<Guild> [{}] guild has been created.", g.name);
                (Some(g), msg)
            }
            Err(GuildError::DuplicateName) => {
                (None, "<Guild> A guild with that name already exists.".to_string())
            }
            Err(_) => (None, "<Guild> The guild name is invalid.".to_string()),
        };
        if let Some(g) = &result.0 {
            store.push(g.clone());
        }
        result
    };
    if let Some(g) = &guild {
        // Ack GC_GUILD INFO (parity SendGuildInfoPacket — guild.cpp:867-897).
        let ack = pg::gc_info(g.id as u32, session.player_vid(), &g.name);
        session.send(&ack).await.map_err(|e| format!("GC_GUILD (info): {e}"))?;
        eprintln!(
            "server_realms: channel conn {}: guild {} '{}' creada por {}",
            session.conn_id,
            g.id,
            g.name,
            session.row().name
        );
    }
    // Ok/error SIEMPRE llevan GC_CHAT (parity guild_manager.cpp:79/99 y
    // AnswerMakeGuild:2367/2385 — el C++ responde por ChatPacket INFO).
    session
        .send(&gc_chat(session.empire, &msg))
        .await
        .map_err(|e| format!("GC_CHAT (guild): {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use database::player::PlayerRow;
    use tokio::io::AsyncReadExt;

    /// Serializa los tests async (el store es un static COMPARTIDO).
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Sesión de test (patrón messenger.rs — sockets localhost + pool lazy;
    /// el handler guild NO toca PG).
    async fn test_session(vid: u32, name: &str, empire: u8) -> (Session, tokio::net::TcpStream) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let client_side = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (server_side, _peer) = listener.accept().await.expect("accept");
        let pool = database::pool::new_pool("host=127.0.0.1 port=1 user=x password=x dbname=x", 2)
            .expect("pool lazy");
        let wal_dir = std::env::temp_dir()
            .join(format!("guild_test_wal_{}", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let batcher = std::sync::Arc::new(database::wal::Batcher::spawn(
            Duration::from_millis(100),
            64,
            database::wal::WalSink::new(database::wal::PgMutationSink::new(pool.clone()), wal_dir),
        ));
        let cfg = crate::config::Config {
            timeout: Duration::from_secs(5),
            ..crate::config::Config::default()
        };
        let (intent_tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut s = Session::new(
            server_side,
            cfg,
            vid,
            intent_tx,
            std::sync::Arc::new(std::sync::Mutex::new(game_core::map::MapStore::new())),
            pool,
            batcher,
            std::sync::Arc::new(database::attr::AttrTables::default()),
        );
        s.empire = empire;
        s.row = Some(PlayerRow { id: vid as i64, name: name.into(), ..Default::default() });
        (s, client_side)
    }

    async fn read_packet(sock: &mut tokio::net::TcpStream) -> Vec<u8> {
        let mut hdr = [0u8; 3];
        sock.read_exact(&mut hdr).await.expect("paquete del server");
        let size = u16::from_le_bytes([hdr[1], hdr[2]]) as usize;
        let mut body = vec![0u8; size - 3];
        sock.read_exact(&mut body).await.expect("cuerpo");
        let mut pkt = hdr.to_vec();
        pkt.extend_from_slice(&body);
        pkt
    }

    fn cg_guild_create(name: &str) -> Vec<u8> {
        let mut pkt = vec![header::CG_GUILD, pg::SUB_CG_CREATE];
        pkt.extend_from_slice(name.as_bytes());
        pkt.resize(pg::CG_CREATE_TOTAL, 0); // nombre NUL-padded a 13 B
        pkt
    }

    /// VERIFIER (requerido por el slice): crear una guild y reintentar con el
    /// MISMO nombre → el duplicado FALLA: solo GC_CHAT de error, sin segundo
    /// GC_GUILD y sin doble alta en el store.
    #[tokio::test]
    async fn create_with_duplicate_name_fails() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut s, mut sock) = test_session(900, "Master", 1).await;
        let before = guilds().lock().unwrap().len();

        handle(&mut s, &cg_guild_create("Valientes")).await.expect("create 1");
        let ack = read_packet(&mut sock).await;
        assert_eq!(ack[0], pg::GC_GUILD, "ack GC_GUILD");
        assert_eq!(ack[3], pg::SUB_GC_INFO);
        assert_eq!(ack.len(), 39, "sobre 4 B + payload 35 B");
        let chat = read_packet(&mut sock).await;
        assert_eq!(chat[0], header::GC_CHAT);
        assert!(String::from_utf8_lossy(&chat[9..]).contains("has been created"));

        handle(&mut s, &cg_guild_create("Valientes")).await.expect("create 2 (dup)");
        let err = read_packet(&mut sock).await;
        assert_eq!(err[0], header::GC_CHAT, "duplicado → solo GC_CHAT");
        assert!(String::from_utf8_lossy(&err[9..]).contains("already exists"));
        assert!(
            tokio::time::timeout(Duration::from_millis(200), sock.read(&mut [0u8; 1]))
                .await
                .is_err(),
            "el duplicado NO envía un segundo GC_GUILD"
        );
        assert_eq!(guilds().lock().unwrap().len(), before + 1, "el duplicado no se inserta");
    }

    /// Nombre inválido (corto) → GC_CHAT de error, sin GC_GUILD (parity
    /// guild_manager.cpp:79 — ChatPacket INFO y return).
    #[tokio::test]
    async fn invalid_name_sends_error_chat_only() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut s, mut sock) = test_session(901, "Shorty", 1).await;
        let before = guilds().lock().unwrap().len();
        handle(&mut s, &cg_guild_create("x")).await.expect("create inválido");
        let err = read_packet(&mut sock).await;
        assert_eq!(err[0], header::GC_CHAT);
        assert!(String::from_utf8_lossy(&err[9..]).contains("invalid"));
        assert!(
            tokio::time::timeout(Duration::from_millis(200), sock.read(&mut [0u8; 1]))
                .await
                .is_err(),
            "sin GC_GUILD para error"
        );
        assert_eq!(guilds().lock().unwrap().len(), before, "el inválido no se inserta");
    }
}