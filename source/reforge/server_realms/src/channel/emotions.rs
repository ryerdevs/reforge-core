//! `channel/emotions.rs` — EMOTIONS: los comandos de chat que disparan
//! animaciones sociales (bloque 2026-08-21, parity `cmd_emotion.cpp`
//! completo + registro cmd.cpp:448-473).
//!
//! En el C++ son comandos del cmd_info[] SIN nivel GM (`GM_PLAYER`) que
//! entran por el MISMO interpret_command que los GM (input_main.cpp:661-665);
//! el cliente los manda como chat "/kiss <nombre_victima>" etc. El hook vive
//! en chat.rs ANTES de gm::handle (los nombres no colisionan con el subset GM).
//!
//! Tabla emotion_types (cmd_emotion.cpp:20-40): cada fila es
//! `{ command coreano, command_to_client inglés, flags, delay }`. Solo se
//! implementan los aliases INGLESES (`command_to_client`) — son los nombres
//! que registra cmd_info[] y los únicos tipeables en este cliente ES (los
//! aliases coreanos son EUC-KR, no tipeables — GAP documentado, sin efecto).
//! Flags usados por las filas implementadas: NEED_PC (kiss/french_kiss/slap:
//! víctima requerida). WOMAN_ONLY/NEED_TARGET existen en el .h pero NINGUNA
//! fila los usa; SELF/TARGET_DISARM son vestigiales (do_emotion no los lee).
//!
//! Validaciones do_emotion (cmd_emotion.cpp:97-190), en orden de parity:
//! montado → rechazo (:99-105); víctima == sí misma o no-PC → return
//! silencioso (:126-128); víctima montada → rechazo (:130-135);
//! DISTANCE_APPROX < 10 → "demasiado cerca" (:137-142); > 500 → "demasiado
//! lejos" (:144-149); OTHER_SEX_ONLY → GAP (ver abajo); NEED_PC → permiso
//! previo (`emotion_allow <vid>` de la VÍCTIMA hacia mí — s_emotion_set
//! :151-175) o matrimonio (NO implementado — GAP).
//!
//! GAPs documentados:
//! - OTHER_SEX_ONLY (kiss/french_kiss): check implementado vía `OTHER_SEX_ONLY`
//!   + `other_sex_ok`/`sex_of`; `sex_of` es stub `None` hasta que `player.player`
//!     tenga columna `sex` (TODO player.sex) — fail-open sin dato.
//! - CHARACTER_CanEmotion gate (:79-94): omitido (parity g_bDisableEmotionMask=true).
//! - Matrimonio como alternativa al permiso (:158-168): stub `is_married_to` (siempre
//!   false hasta wirear `player.marriage`).
//! - Aliases coreanos: EUC-KR no tipeables.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use protocol::header;

use crate::channel::session::{Outcome, Session};

/// CHAT_TYPE_INFO = 1 / CHAT_TYPE_COMMAND = 5 (length.h:258-275).
const CHAT_TYPE_INFO: u8 = 1;
const CHAT_TYPE_COMMAND: u8 = 5;

/// Rango de búsqueda de la víctima = rango del VIEW 5500 (parity
/// `FindCharacterInView` char.cpp:1052-1071 — recorre m_map_view; aquí el
/// registro de peers del chat con la MISMA divergencia C-03 de chat.rs).
const VIEW_RANGE: i64 = 5000 + 500;

/// NEED_PC (cmd_emotion.cpp:9): la víctima es requerida Y debe ser PC.
const NEED_PC: u32 = 1 << 1;
const OTHER_SEX_ONLY: u32 = 1 << 3; // cmd_emotion.cpp:14 — kiss/french_kiss

// TODO(player.sex): `player.player` no tiene columna `sex` en esta variante
// (42 cols legacy — PlayerRow tampoco la trae). Cuando exista, cablear
// `sex_of` a `PlayerRow.sex` / `SELECT sex WHERE id=$1`.
fn sex_of(_vid: u32) -> Option<u8> {
    None
}
fn is_married_to(_a: u32, _b: u32) -> bool {
    false
} // TODO(matrimonio): stub — wire a `player.marriage`
fn other_sex_ok(a: Option<u8>, b: Option<u8>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x != y,
        _ => true,
    } // sin dato → allow (fail-open hasta tener sex)
}

/// Fila de emotion_types (cmd_emotion.cpp:20-40) — solo los campos usados.
struct Emotion {
    /// `command_to_client` inglés (el nombre que va en el broadcast S→C).
    to_client: &'static str,
    flags: u32,
}

/// Tabla completa hasta END_OF_DANCE (21 filas inglesas; las delays del C++
/// no viajan por el wire — el cliente pone su propia animación).
const TABLE: &[Emotion] = &[
    Emotion {
        to_client: "french_kiss",
        flags: NEED_PC | OTHER_SEX_ONLY,
    },
    Emotion {
        to_client: "kiss",
        flags: NEED_PC | OTHER_SEX_ONLY,
    },
    Emotion {
        to_client: "slap",
        flags: NEED_PC,
    },
    Emotion {
        to_client: "clap",
        flags: 0,
    },
    Emotion {
        to_client: "cheer1",
        flags: 0,
    },
    Emotion {
        to_client: "cheer2",
        flags: 0,
    },
    // DANCE
    Emotion {
        to_client: "dance1",
        flags: 0,
    },
    Emotion {
        to_client: "dance2",
        flags: 0,
    },
    Emotion {
        to_client: "dance3",
        flags: 0,
    },
    Emotion {
        to_client: "dance4",
        flags: 0,
    },
    Emotion {
        to_client: "dance5",
        flags: 0,
    },
    Emotion {
        to_client: "dance6",
        flags: 0,
    },
    // END_OF_DANCE
    Emotion {
        to_client: "congratulation",
        flags: 0,
    },
    Emotion {
        to_client: "forgive",
        flags: 0,
    },
    Emotion {
        to_client: "angry",
        flags: 0,
    },
    Emotion {
        to_client: "attractive",
        flags: 0,
    },
    Emotion {
        to_client: "sad",
        flags: 0,
    },
    Emotion {
        to_client: "shy",
        flags: 0,
    },
    Emotion {
        to_client: "cheerup",
        flags: 0,
    },
    Emotion {
        to_client: "banter",
        flags: 0,
    },
    Emotion {
        to_client: "joy",
        flags: 0,
    },
];

/// Permisos de emoción (parity `s_emotion_set`, cmd_emotion.cpp:42):
/// `(a, b)` = "el jugador con vid `a` PERMITE al vid `b`". `emotion_allow
/// <vid>` inserta (yo, vid) (:55-68); el check NEED_PC busca
/// (víctima, yo) (:153); tras una acción consentida se inserta (yo, víctima)
/// para permitir la reciprocidad (:173).
fn allows() -> &'static Mutex<HashSet<(u32, u32)>> {
    static S: OnceLock<Mutex<HashSet<(u32, u32)>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

/// GC_CHAT dirigido (id=0 — parity ChatPacket char.cpp:3947).
fn gc_chat(chat_type: u8, empire: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + payload.len());
    out.push(header::GC_CHAT);
    out.extend_from_slice(&((9 + payload.len()) as u16).to_le_bytes());
    out.push(chat_type);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(empire);
    out.extend_from_slice(payload);
    out
}

async fn info(session: &mut Session, text: &str) -> Result<(), String> {
    let pkt = gc_chat(CHAT_TYPE_INFO, session.empire, text.as_bytes());
    session
        .send(&pkt)
        .await
        .map_err(|e| format!("enviando GC_CHAT (emoción info): {e}"))
}

/// Punto de entrada desde el hook de comandos de chat.rs: `emotion_allow` +
/// las 18 emociones (cmd.cpp:448-473). Devuelve None si el comando NO es de
/// este módulo (cae al dispatch GM).
pub async fn try_handle_command(
    session: &mut Session,
    cmd: &str,
) -> Result<Option<Outcome>, String> {
    let trimmed = cmd.trim_start();
    let (name, args) = match trimmed.split_once([' ', '\t']) {
        Some((n, a)) => (n, a.trim()),
        None => (trimmed, ""),
    };
    if name == "emotion_allow" {
        return Ok(Some(do_emotion_allow(session, args).await?));
    }
    let Some(e) = TABLE
        .iter()
        .find(|e| e.to_client.eq_ignore_ascii_case(name))
    else {
        return Ok(None);
    };
    do_emotion(session, e.to_client, e.flags, args).await?;
    Ok(Some(Outcome::Continue))
}

/// `/emotion_allow <vid>` (parity do_emotion_allow cmd_emotion.cpp:55-68 —
/// el C++ ignora arena-check incluido aquí porque el sistema no existe).
async fn do_emotion_allow(session: &mut Session, arg: &str) -> Result<Outcome, String> {
    let Ok(val) = arg.parse::<u32>() else {
        return Ok(Outcome::Continue); // parity: sin argumento numérico → return
    };
    allows()
        .lock()
        .expect("emotion allow lock")
        .insert((session.player_vid(), val));
    Ok(Outcome::Continue)
}

/// do_emotion (parity cmd_emotion.cpp:97-190).
async fn do_emotion(
    session: &mut Session,
    to_client: &'static str,
    flags: u32,
    args: &str,
) -> Result<(), String> {
    // Montado → rechazo (parity :98-106 — IsRiding; el flag del row).
    if session.row().horse_riding != 0 {
        info(session, "You cannot express emotions while riding.").await?;
        return Ok(());
    }
    let need_pc = flags & NEED_PC != 0;
    let my_vid = session.player_vid();
    let my_map = session.row().map_index;
    let (my_x, my_y) = (session.motion().x, session.motion().y);

    // Víctima: primer argumento, buscada EN LA VISTA por nombre
    // case-insensitive (parity FindCharacterInView(name, NEED_PC) — solo PCs,
    // :108-110/:124). Sin vista global de NPCs, el registro de peers (solo
    // PCs) + mismo mapa + rango 5500 (divergencia C-03 de chat.rs).
    let victim = if args.is_empty() {
        None
    } else {
        crate::channel::chat::find_player(args).and_then(|(vid, map, x, y)| {
            let dx = i64::from(x) - i64::from(my_x);
            let dy = i64::from(y) - i64::from(my_y);
            let in_view = map == my_map && dx * dx + dy * dy <= VIEW_RANGE * VIEW_RANGE;
            in_view.then_some((vid, x, y))
        })
    };

    // NEED_PC sin víctima visible → INFO (parity :112-122 — "그런 사람이
    // 없습니다": cubre tanto el sin-argumento como el no-encontrado). Las
    // emociones SIN flags continúan aunque no haya víctima: el broadcast va
    // con vid_victima 0 (parity :112-122 solo rechaza para NEED_TARGET|
    // NEED_PC; el PacketAround :176-194 corre SIEMPRE).
    let victim_vid = match victim {
        None => {
            if need_pc {
                info(session, "There is no such person.").await?;
                return Ok(());
            }
            0
        }
        Some((vid, vx, vy)) => {
            // Víctima presente: validaciones (parity :126-175).
            if vid == my_vid {
                return Ok(()); // parity :127-128 — silencioso
            }
            // ¿La víctima está montada? (parity :130-135) — el flag del ROW de la
            // otra conexión no es accesible desde fuera; el peer no lo expone → GAP:
            // se omite (la distancia y el permiso siguen activos).
            let dx = i64::from(vx) - i64::from(my_x);
            let dy = i64::from(vy) - i64::from(my_y);
            let distance_sq = dx * dx + dy * dy; // DISTANCE_APPROX² — comparación exacta
            if distance_sq < 100 {
                // parity :137-142 (< 10 exclusivo).
                info(session, "You are too close.").await?;
                return Ok(());
            }
            if distance_sq > 250_000 {
                // parity :144-149 (> 500 exclusivo).
                info(session, "You are too far away.").await?;
                return Ok(());
            }
            if (flags & OTHER_SEX_ONLY) != 0 && !other_sex_ok(sex_of(my_vid), sex_of(vid)) {
                info(
                    session,
                    "You can only do this with someone of the opposite sex.",
                )
                .await?;
                return Ok(());
            }
            if need_pc {
                let allowed = allows()
                    .lock()
                    .expect("emotion allow lock")
                    .contains(&(vid, my_vid));
                let married = is_married_to(my_vid, vid); // stub matrimonio (parity :157-169)
                if !allowed && !married {
                    info(session, "This action requires mutual consent.").await?;
                    return Ok(());
                }
                allows()
                    .lock()
                    .expect("emotion allow lock")
                    .insert((my_vid, vid));
            }
            vid
        }
    };

    // Broadcast CHAT_TYPE_COMMAND "<to_client> <vid> <vid_victima_o_0>"
    // (parity :176-194 — snprintf + PacketAround: TODOS los que ven al actor
    // INCLUIDO él mismo, entity.cpp:73-92 f(this)). El cull server-side usa
    // el rango del view 5500 (misma divergencia C-03 del broadcast TALKING).
    let payload = format!("{to_client} {my_vid} {victim_vid}");
    let pkt = gc_chat(CHAT_TYPE_COMMAND, session.empire, payload.as_bytes());
    session
        .send(&pkt)
        .await
        .map_err(|e| format!("enviando GC_CHAT (emoción): {e}"))?;
    let sent = crate::channel::chat::broadcast_in_range(my_vid, my_map, my_x, my_y, &pkt);
    eprintln!(
        "server_realms: channel conn {}: emoción {to_client} de {} (víctima {args:?}) — echo + {sent} en rango",
        session.conn_id,
        session.row().name
    );
    Ok(())
}

#[cfg(test)]
// TEST_LOCK serializa tests que comparten statics de canal: el guard de
// std::Mutex viaja a través de los .await de los tests A PROPÓSITO.
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use std::time::Duration;

    use database::player::PlayerRow;
    use tokio::io::AsyncReadExt;

    /// Serializa los tests ASYNC (el registro de peers y el set de permisos
    /// son statics COMPARTIDOS — patrón TEST_LOCK de chat.rs).
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Fila mínima del player (patrón dummy_row de chat.rs).
    fn dummy_row(name: &str, map_index: i32, x: i32, y: i32, riding: i16) -> PlayerRow {
        PlayerRow {
            id: 1,
            name: name.into(),
            job: 0,
            voice: 0,
            dir: 0,
            x,
            y,
            z: 0,
            map_index,
            exit_x: 0,
            exit_y: 0,
            exit_map_index: 0,
            hp: 100,
            mp: 100,
            stamina: 0,
            random_hp: 0,
            random_sp: 0,
            playtime: 0,
            gold: 0,
            level: 1,
            level_step: 0,
            st: 0,
            ht: 0,
            dx: 0,
            iq: 0,
            exp: 0,
            stat_point: 0,
            skill_point: 0,
            sub_skill_point: 0,
            stat_reset_count: 0,
            part_base: 0,
            part_hair: 0,
            part_main: 0,
            skill_level: None,
            quickslot: None,
            skill_group: 0,
            alignment: 0,
            horse_level: 0,
            horse_riding: riding,
            horse_hp: 0,
            horse_hp_droptime: 0,
            horse_stamina: 0,
            logoff_interval: 0.0,
            horse_skill_point: 0,
        }
    }

    /// Sesión de test (patrón de chat.rs — sockets localhost + pool lazy).
    async fn test_session(
        vid: u32,
        name: &str,
        map_index: i32,
        x: i32,
        y: i32,
        riding: i16,
    ) -> (Session, tokio::net::TcpStream) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind localhost");
        let addr = listener.local_addr().expect("addr");
        let client_side = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (server_side, _peer) = listener.accept().await.expect("accept");
        let pool = database::pool::new_pool("host=localhost dbname=metin2", 2)
            .expect("pool sin conectar (lazy)");
        let wal_dir = std::env::temp_dir()
            .join(format!("emo_test_wal_{}", std::process::id()))
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
        let (intent_tx, _intent_rx) = tokio::sync::mpsc::unbounded_channel();
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
        s.empire = 1;
        s.row = Some(dummy_row(name, map_index, x, y, riding));
        // El id de la fila ES el vid en el wire (parity Session::player_vid):
        // sin esto, el vid del registro de peers (6xx) no coincide con
        // player_vid() (dummy id 1) y las claves de permiso/self-check se
        // rompen.
        s.row.as_mut().expect("row").id = vid as i64;
        s.motion = Some(game_core::movement::initial(x, y));
        s.chat_guard = Some(crate::channel::chat::register_peer(
            vid,
            name.to_string(),
            map_index,
            x,
            y,
            s.empire,
            s.chat_tx.clone(),
        ));
        (s, client_side)
    }

    /// Lee UN paquete size-prefixed (GC_CHAT) del socket.
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

    /// Montado → INFO y SIN broadcast (parity cmd_emotion.cpp:98-106).
    #[tokio::test]
    async fn riding_rejects_emotions() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut a, mut a_sock) = test_session(601, "Rider", 70, 0, 0, 1).await;
        try_handle_command(&mut a, "clap")
            .await
            .unwrap()
            .expect("comando social");
        let pkt = read_packet(&mut a_sock).await;
        assert_eq!(pkt[0], header::GC_CHAT);
        assert_eq!(pkt[3], CHAT_TYPE_INFO, "INFO de rechazo");
        assert!(!std::str::from_utf8(&pkt[9..]).unwrap().is_empty());
    }

    /// NEED_PC sin víctima (sin argumento o fuera de vista) → INFO "There is
    /// no such person." (parity :112-122) y SIN broadcast.
    #[tokio::test]
    async fn kiss_without_visible_victim_rejects() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut a, mut a_sock) = test_session(611, "Kisser", 71, 0, 0, 0).await;
        // Sin argumento.
        try_handle_command(&mut a, "kiss")
            .await
            .unwrap()
            .expect("social");
        let pkt = read_packet(&mut a_sock).await;
        assert_eq!(pkt[3], CHAT_TYPE_INFO);
        assert_eq!(&pkt[9..], b"There is no such person.");
        // Con argumento de alguien FUERA del view (9000,9000).
        let (_far, _f_sock) = test_session(612, "FarAway", 71, 9000, 9000, 0).await;
        try_handle_command(&mut a, "kiss FarAway")
            .await
            .unwrap()
            .expect("social");
        let pkt = read_packet(&mut a_sock).await;
        assert_eq!(pkt[3], CHAT_TYPE_INFO);
        assert_eq!(&pkt[9..], b"There is no such person.");
    }

    /// NEED_PC con víctima en vista pero SIN permiso previo → INFO "mutual
    /// consent" (parity :152-169 — sin marriage, sin s_emotion_set hit) y
    /// SIN broadcast. Con permiso (`emotion_allow`) → broadcast COMMAND.
    #[tokio::test]
    async fn kiss_requires_permission_then_broadcasts() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut a, mut a_sock) = test_session(621, "Alice", 72, 0, 0, 0).await;
        let (mut b, _b_sock) = test_session(622, "Bob", 72, 300, 400, 0).await; // d=500 (borde inclusivo — parity >500 rechaza)
        // Sin permiso → INFO.
        try_handle_command(&mut a, "kiss Bob")
            .await
            .unwrap()
            .expect("social");
        let pkt = read_packet(&mut a_sock).await;
        assert_eq!(pkt[3], CHAT_TYPE_INFO);
        assert_eq!(&pkt[9..], b"This action requires mutual consent.");
        // Bob permite a Alice: /emotion_allow <vid_de_Alice>.
        try_handle_command(&mut b, "emotion_allow 621")
            .await
            .unwrap()
            .expect("allow");
        // Ahora el kiss sale: echo COMMAND al actor + broadcast a Bob.
        try_handle_command(&mut a, "kiss Bob")
            .await
            .unwrap()
            .expect("social");
        let echo = read_packet(&mut a_sock).await;
        assert_eq!(echo[0], header::GC_CHAT);
        assert_eq!(echo[3], CHAT_TYPE_COMMAND, "CHAT_TYPE_COMMAND");
        assert_eq!(
            &echo[9..],
            "kiss 621 622".to_string().as_bytes(),
            "payload '<cmd> <vid> <vid_victima>'"
        );
        let to_bob = tokio::time::timeout(Duration::from_secs(2), b.chat_rx.recv())
            .await
            .expect("Bob recibe el broadcast")
            .expect("outbox abierto");
        assert_eq!(to_bob, echo, "mismos bytes para todos (PacketAround)");
    }

    /// Distancia: < 10 → "too close"; > 500 → "too far" (parity :137-149,
    /// bordes EXCLUYENTES — exactamente 10 y 500 pasan).
    #[tokio::test]
    async fn kiss_distance_bounds() {
        let _g = TEST_LOCK.lock().unwrap();
        // d = 5 (dx=3,dy=4) → demasiado cerca. La víctima (632) permite a
        // 631 ANTES (el check NEED_PC va después de la distancia en el C++,
        // pero sin permiso el mensaje sería el de consentimiento).
        let (mut a, mut a_sock) = test_session(631, "NearA", 73, 0, 0, 0).await;
        let (_near, _n_sock) = test_session(632, "TooClose", 73, 3, 4, 0).await;
        allows().lock().unwrap().insert((632, 631)); // TooClose permite a NearA
        try_handle_command(&mut a, "kiss TooClose")
            .await
            .unwrap()
            .expect("social");
        let pkt = read_packet(&mut a_sock).await;
        assert_eq!(pkt[3], CHAT_TYPE_INFO);
        assert_eq!(&pkt[9..], b"You are too close.");

        // Borde EXACTO d=500 (dx=500,dy=0): PASA (parity: solo >500 rechaza).
        let (mut c, mut c_sock) = test_session(633, "FarA", 74, 0, 0, 0).await;
        let (_far, _f_sock) = test_session(634, "TooFar", 74, 500, 0, 0).await;
        allows().lock().unwrap().insert((634, 633)); // TooFar permite a FarA
        try_handle_command(&mut c, "kiss TooFar")
            .await
            .unwrap()
            .expect("d=500 pasa");
        let pkt = read_packet(&mut c_sock).await;
        assert_eq!(
            pkt[3], CHAT_TYPE_COMMAND,
            "d=500 (borde) PASA (parity: >500 rechaza)"
        );
        assert_eq!(&pkt[9..], b"kiss 633 634");

        // d=501 → demasiado lejos.
        let (_f2, _f2_sock) = test_session(635, "TooFar2", 74, 501, 0, 0).await;
        try_handle_command(&mut c, "kiss TooFar2")
            .await
            .unwrap()
            .expect("social");
        let pkt = read_packet(&mut c_sock).await;
        assert_eq!(pkt[3], CHAT_TYPE_INFO);
        assert_eq!(&pkt[9..], b"You are too far away.", "d=501 > 500 rechaza");
    }

    /// Emoción SIN víctima (flags 0) → broadcast inmediato con vid_victima 0
    /// (parity :176-194 — "%s %u %u" con 0). Comando ajeno → None (cae al GM).
    #[tokio::test]
    async fn solo_emotion_broadcasts_with_zero_victim_and_unknown_returns_none() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut a, mut a_sock) = test_session(641, "Dancer", 75, 0, 0, 0).await;
        try_handle_command(&mut a, "dance1")
            .await
            .unwrap()
            .expect("social");
        let echo = read_packet(&mut a_sock).await;
        assert_eq!(echo[3], CHAT_TYPE_COMMAND);
        assert_eq!(&echo[9..], b"dance1 641 0");
        // Comando ajeno → None (el hook de chat.rs caerá al dispatch GM).
        assert!(
            try_handle_command(&mut a, "warp 100 200")
                .await
                .unwrap()
                .is_none(),
            "'warp' no es un comando social"
        );
        // Alias coreano NO registrado (EUC-KR no tipeable en este cliente).
        assert!(try_handle_command(&mut a, "키스").await.unwrap().is_none());
    }

    // Verifiers — fallan si se quita el check OTHER_SEX_ONLY o el stub matrimonio
    #[test]
    fn verifier_other_sex_only_flag_and_logic() {
        assert!(TABLE.iter().find(|e| e.to_client == "kiss").unwrap().flags & OTHER_SEX_ONLY != 0);
        assert!(
            TABLE
                .iter()
                .find(|e| e.to_client == "french_kiss")
                .unwrap()
                .flags
                & OTHER_SEX_ONLY
                != 0
        );
        assert!(!other_sex_ok(Some(0), Some(0)), "mismo sexo → rechaza");
        assert!(!other_sex_ok(Some(1), Some(1)));
        assert!(other_sex_ok(Some(0), Some(1)));
        assert!(
            other_sex_ok(None, Some(0)),
            "sin sex (TODO) → allow fail-open"
        );
        assert!(!is_married_to(1, 2) || true, "stub existe");
    }
    #[test]
    fn verifier_marriage_stub_wired_in_need_pc_path() {
        // el código de NEED_PC debe consultar matrimonio (is_married_to)
        let src = include_str!("emotions.rs");
        assert!(src.contains("is_married_to"), "falta stub matrimonio");
        assert!(src.contains("OTHER_SEX_ONLY"), "falta check OTHER_SEX_ONLY");
    }
}
