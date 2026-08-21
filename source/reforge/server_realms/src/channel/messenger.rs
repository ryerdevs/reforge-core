//! `channel/messenger.rs` — MESSENGER (amigos): CG_MESSENGER (67) + el
//! comando de chat `messenger_auth` (bloque 2026-08-21, parity
//! `input_main.cpp:927-1037` + `messenger_manager.cpp` +
//! `cmd_general.cpp:1167-1189` do_messenger_auth).
//!
//! Flujo legacy (verificado contra el oráculo congelado):
//! 1. INVITAR — ADD_BY_VID/ADD_BY_NAME → `RequestToAdd`
//!    (messenger_manager.cpp:157-176): al DESTINO se le envía un GC_CHAT
//!    CHAT_TYPE_COMMAND "messenger_auth <nombre_del_que_invita>"
//!    (:174); el cliente muestra el diálogo y responde
//!    `/messenger_auth y|n <nombre>` por SendChatPacket (game.py:1007-1013).
//! 2. ACEPTAR — `AuthToAdd` (:179-204): borra la petición pendiente y hace
//!    `AddToList` EN AMBAS DIRECCIONES (:200-201) — la tabla
//!    `player.messenger_list` guarda NOMBRES DE PERSONAJE en ambas columnas
//!    (parity AddToList(ch->GetName(), ...)). `__AddToList` (:206-227) avisa
//!    por INFO a cada lado online + manda GC LOGIN del otro si está online
//!    (o LOGOUT si no).
//! 3. RECHAZAR — cmd_general.cpp:1184-1192: INFO al invitador si está online
//!    ("%s rejected your friend request").
//! 4. BORRAR — REMOVE (input_main.cpp:1015-1030): DELETE en DB en AMBAS
//!    direcciones (@fixme183) + sync GC REMOVE_FRIEND al otro lado
//!    (`ENABLE_MESSENGER_REMOVE_SYNC` activo en server Y cliente:
//!    common/CommonDefines.h:55 + UserInterface/Locale_inc.h:59).
//! 5. LISTA AL ENTRAR — input_login.cpp:639 (`MessengerManager::Login` en
//!    ENTERGAME) → LoadList + SendList (messenger_manager.cpp:44-141):
//!    UN paquete GC LIST con `connected` según sesión online del companion;
//!    con 0 filas NO se envía nada.
//!
//! El registro de peticiones pendientes (`requests()`) es el equivalente del
//! `m_set_requestToAdd` del C++ (:24, keyed por CRC de la pareja): SIN él,
//! cualquiera podría auto-agregar a su lista a otro jugador sin su consentimiento
//! escribiendo "messenger_auth y <víctima>" (el check de AuthToAdd :186-191 lo
//! impide). En memoria del canal (las peticiones son efímeras — igual que el
//! set del C++, que muere con el proceso).
//!
//! GAPs documentados (subset):
//! - Textos INFO en EN (sin locale system — divergencia documentada igual
//!   que gm.rs/party.rs; el C++ usa LC_TEXT coreano).
//! - BLOCK_MESSENGER_INVITE (block-mode) y observer mode: sistemas ausentes
//!   en reforge → checks omitidos.
//! - Quest-running gate de RequestToAdd (messenger_manager.cpp:160-168):
//!   el estado "quest en curso" del engine Rust no se consulta aquí.
//! - P2P entre canales (HEADER_GG_MESSENGER_ADD/REMOVE, p2p.cpp): runtime
//!   de UN canal — los paquetes GG no existen; el sync intra-canal lo hacen
//!   los outbox directos.
//! - Logout broadcast (char.cpp:1360 `Logout` al desconectar): los amigos
//!   ven el estado stale hasta el próximo LIST/login event (GAP; requiere
//!   teardown async — lane futura).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use database::messenger::MessengerRepo;
use game_core::gm::{self};
use protocol::header;
use protocol::social as psocial;

use crate::channel::session::{Outcome, Session};

/// CHAT_TYPE_INFO = 1 / CHAT_TYPE_COMMAND = 5 (length.h:258-275).
const CHAT_TYPE_INFO: u8 = 1;
const CHAT_TYPE_COMMAND: u8 = 5;

/// Nivel GM del texto gmlist → i16 helper local (PLAYER si no hay fila).
fn level_of(auth: Option<String>) -> i16 {
    auth.and_then(|a| gm::gm_level_from_text(&a)).unwrap_or(gm::gm_level::PLAYER)
}

/// Peticiones de amistad pendientes: `(inviter, accepter)` en minúsculas
/// (parity `m_set_requestToAdd`, messenger_manager.cpp:23/:159-172/:185-195).
fn requests() -> &'static Mutex<HashSet<(String, String)>> {
    static R: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashSet::new()))
}

/// GC_CHAT dirigido (id=0 — parity ChatPacket char.cpp:3947; bEmpire = el
/// del RECEPTOR — char.cpp:3948). Payload crudo SIN NUL.
fn gc_chat(chat_type: u8, empire: u8, payload: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + payload.len());
    out.push(header::GC_CHAT);
    out.extend_from_slice(&((9 + payload.len()) as u16).to_le_bytes());
    out.push(chat_type);
    out.extend_from_slice(&0u32.to_le_bytes()); // id=0 (ChatPacket)
    out.push(empire);
    out.extend_from_slice(payload.as_bytes());
    out
}

async fn info(session: &mut Session, text: &str) -> Result<(), String> {
    let pkt = gc_chat(CHAT_TYPE_INFO, session.empire, text);
    session.send(&pkt).await.map_err(|e| format!("enviando GC_CHAT (messenger info): {e}"))
}

/// CG_MESSENGER (67) — dispatch por subheader (parity Messenger()
/// input_main.cpp:927-1037). El framer ya entregó el paquete COMPLETO
/// (variable por subheader); subheader desconocido → log + descarte SIN
/// cerrar (parity sys_err + break, :1031-1035).
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() < psocial::CG_FIXED {
        eprintln!(
            "server_realms: channel conn {}: CG_MESSENGER malformado ({})",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    }
    match pkt[1] {
        psocial::SUB_CG_ADD_BY_VID => {
            if pkt.len() < psocial::CG_ADD_BY_VID_TOTAL {
                return Ok(Outcome::Continue);
            }
            let vid = u32::from_le_bytes([pkt[2], pkt[3], pkt[4], pkt[5]]);
            add_by_vid(session, vid).await?;
        }
        psocial::SUB_CG_ADD_BY_NAME => add_by_name(session, &name_at(pkt)).await?,
        psocial::SUB_CG_REMOVE => remove(session, &name_at(pkt)).await?,
        other => {
            // parity input_main.cpp:1031-1035: sys_err + break — el handler
            // devuelve 0 SIN cerrar la conexión.
            eprintln!(
                "server_realms: channel conn {}: CInputMain::Messenger : Unknown subheader {other} : {}",
                session.conn_id,
                session.row().name
            );
        }
    }
    Ok(Outcome::Continue)
}

/// Nombre crudo del payload (24 B desde el offset 2, strlcpy parity — corta
/// en el primer NUL).
fn name_at(pkt: &[u8]) -> String {
    let raw = &pkt[psocial::CG_FIXED..psocial::CG_NAME_TOTAL.min(pkt.len())];
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).into_owned()
}

/// ADD_BY_VID (parity input_main.cpp:939-969): Find(vid) → staff-check →
/// self-check → RequestToAdd.
async fn add_by_vid(session: &mut Session, vid: u32) -> Result<(), String> {
    // Find(vid) — solo sesiones CONECTADAS (desc) tienen peer; sin peer =
    // `!ch_companion` o `!d` del C++ → return silencioso (:943/:957).
    let Some(companion) = crate::channel::chat::peer_name(vid) else {
        return Ok(());
    };
    if vid == session.player_vid() {
        // parity `ch->GetDesc() == d` (:962) — auto-invite silencioso.
        return Ok(());
    }
    // Staff-check (parity :956-960): un JUGADOR no puede invitar a un GM.
    if !staff_check(session, &companion).await? {
        return Ok(());
    }
    request_to_add(
        session,
        &companion,
        vid,
        crate::channel::chat::peer_empire(vid),
    )
    .await
}

/// ADD_BY_NAME (parity input_main.cpp:971-1000): staff-check por nombre →
/// FindPC → self-check → RequestToAdd.
async fn add_by_name(session: &mut Session, name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Ok(());
    }
    if !staff_check(session, name).await? {
        return Ok(());
    }
    // FindPC case-insensitive (char_manager.cpp:209-223) — find_player.
    let Some((vid, ..)) = crate::channel::chat::find_player(name) else {
        // parity :986-988: "%s is not connected".
        info(session, &format!("{name} is not connected.")).await?;
        return Ok(());
    };
    if vid == session.player_vid() {
        // parity tch == ch (:991) — silencioso.
        return Ok(());
    }
    request_to_add(session, name, vid, crate::channel::chat::peer_empire(vid)).await
}

/// Staff-check del invitador (parity input_main.cpp:956-960 VID /
/// :978-984 NAME): ch PLAYER && destino staff → INFO rechazo. Resuelve el
/// nivel del DESTINO por nombre (gm_authority_by_name — parity gm_get_level
/// con host/account NULL, gm.cpp:66-79) y el PROPIO con la pareja
/// nombre+cuenta (gm_authority — el GetGMLevel cacheado del CHARACTER).
/// Devuelve true si la invitación puede continuar.
///
/// Fail-open ante ERROR de PG (no ante fila ausente): el lookup del C++ es
/// sobre un map EN MEMORIA cargado al boot — si la DB no responde, tratarlo
/// como "no es GM" (default GM_PLAYER del C++, gm.cpp:60-62) y loguear;
/// bloquear invitaciones por una caída transitoria sería peor.
async fn staff_check(session: &mut Session, target: &str) -> Result<bool, String> {
    let my_level = level_of(
        self_gm_authority(session)
            .await
            .inspect_err(|e| {
                eprintln!("server_realms: channel conn {}: messenger staff-check (self): {e}", session.conn_id)
            })
            .unwrap_or(None),
    );
    if my_level != gm::gm_level::PLAYER {
        return Ok(true); // un GM sí puede invitar a cualquiera (parity)
    }
    let target_level = level_of(
        database::common::CommonRepo::new(session.pool.clone())
            .gm_authority_by_name(target)
            .await
            .inspect_err(|e| {
                eprintln!("server_realms: channel conn {}: messenger staff-check ({target}): {e}", session.conn_id)
            })
            .unwrap_or(None),
    );
    if target_level != gm::gm_level::PLAYER {
        info(
            session,
            "<Messenger> You cannot add a staff member to your messenger.",
        )
        .await?;
        return Ok(false);
    }
    Ok(true)
}

/// Nivel GM del PROPIO personaje (gmlist nombre+cuenta — el GetGMLevel
/// cacheado del CHARACTER C++).
async fn self_gm_authority(session: &Session) -> Result<Option<String>, String> {
    database::common::CommonRepo::new(session.pool.clone())
        .gm_authority(&session.row().name, &session.account_login)
        .await
}

/// RequestToAdd (parity messenger_manager.cpp:157-176): registra la petición
/// y envía al DESTINO el prompt "messenger_auth <inviter>" como GC_CHAT
/// CHAT_TYPE_COMMAND (:174 — ChatPacket del C++; id=0).
async fn request_to_add(
    session: &mut Session,
    companion: &str,
    companion_vid: u32,
    companion_empire: u8,
) -> Result<(), String> {
    let key = (
        session.row().name.to_ascii_lowercase(),
        companion.to_ascii_lowercase(),
    );
    requests()
        .lock()
        .expect("messenger requests lock")
        .insert(key);
    let prompt = gc_chat(
        CHAT_TYPE_COMMAND,
        companion_empire,
        &format!("messenger_auth {}", session.row().name),
    );
    if !crate::channel::chat::send_to_vid(companion_vid, &prompt) {
        eprintln!(
            "server_realms: channel conn {}: messenger: outbox de {companion} cerrado",
            session.conn_id
        );
    }
    eprintln!(
        "server_realms: channel conn {}: {} invitó a {companion} al messenger",
        session.conn_id,
        session.row().name
    );
    Ok(())
}

/// Comando de chat `messenger_auth y|n <nombre>` (do_messenger_auth,
/// cmd_general.cpp:1167-1189 — llega CON '/' desde el hook de chat.rs).
/// Devuelve None si el comando no es de este módulo (cae al dispatch GM).
pub async fn try_handle_command(
    session: &mut Session,
    cmd: &str,
) -> Result<Option<Outcome>, String> {
    let rest = match cmd.strip_prefix("messenger_auth") {
        Some(r) if r.is_empty() || r.starts_with(' ') || r.starts_with('\t') => r.trim(),
        _ => return Ok(None),
    };
    let (answer, inviter_raw) = match rest.split_once([' ', '\t']) {
        Some((a, b)) => (a, b.trim()),
        None => (rest, ""),
    };
    if answer.is_empty() || inviter_raw.is_empty() {
        // parity :1177-1179 — sin argumentos → return silencioso.
        return Ok(Some(Outcome::Continue));
    }
    let denied = !answer.starts_with('y'); // LOWER(*arg1) != 'y' (:1180-1181)
    let added = auth_to_add(session, inviter_raw, denied).await?;
    if added && denied {
        // parity :1184-1192: INFO al INVITADOR si está online.
        let text = format!(
            "{} rejected your friend request.",
            session.row().name
        );
        send_info_to_name(inviter_raw, &text);
    }
    Ok(Some(Outcome::Continue))
}

/// AuthToAdd (parity messenger_manager.cpp:179-204): la petición debe existir;
/// al aceptar, AddToList EN AMBAS DIRECCIONES. Devuelve si la petición existía.
async fn auth_to_add(
    session: &mut Session,
    inviter_raw: &str,
    deny: bool,
) -> Result<bool, String> {
    let key = (inviter_raw.to_ascii_lowercase(), session.row().name.to_ascii_lowercase());
    // parity :185-195: sin petición previa → sys_log + false (silencioso —
    // cierra el exploit del auto-add sin consentimiento).
    if !requests().lock().expect("messenger requests lock").remove(&key) {
        eprintln!(
            "server_realms: channel conn {}: MessengerManager::AuthToAdd : \
             request not exist {} -> {}",
            session.conn_id, inviter_raw, session.row().name
        );
        return Ok(false);
    }
    let repo = MessengerRepo::new(session.pool.clone());
    if deny {
        return Ok(true);
    }
    let accepter = session.row().name.clone();
    let inviter = inviter_raw.to_string();
    // AddToList(inviter, accepter) + AddToList(accepter, inviter) (:200-201).
    repo.add(&inviter, &accepter).await?;
    repo.add(&accepter, &inviter).await?;
    // __AddToList(:206-227): INFO "<Messenger> %s has been added..." al lado
    // online + GC LOGIN del otro (o LOGOUT si offline).
    let inviter_online = crate::channel::chat::find_player(&inviter);
    if let Some((vid, ..)) = inviter_online {
        send_to_vid_info(
            vid,
            crate::channel::chat::peer_empire(vid),
            &format!("<Messenger> {accepter} has been added to your messenger."),
        );
        // SendLogin(inviter→about accepter): el accepter está online (acabó
        // de aceptar) — parity :222-225.
        send_status(vid, &psocial::login(&accepter));
    }
    // Al ACCEPTER (esta sesión): INFO + LOGIN del invitador si online,
    // LOGOUT si no (parity :222-226).
    info(session, &format!(
        "<Messenger> {inviter} has been added to your messenger."
    ))
    .await?;
    let status = if inviter_online.is_some() {
        psocial::login(&inviter)
    } else {
        psocial::logout(&inviter)
    };
    session
        .send(&status)
        .await
        .map_err(|e| format!("enviando GC MESSENGER status: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: {} aceptó a {inviter} en el messenger (deny={deny})",
        session.conn_id, accepter
    );
    Ok(true)
}

/// REMOVE (parity input_main.cpp:1015-1030): RemoveFromList en AMBAS
/// direcciones (@fixme183) — cada dirección borra su fila y sincroniza al
/// OTRO lado (__RemoveFromList :230-260 con REMOVE_SYNC activo).
async fn remove(session: &mut Session, target: &str) -> Result<(), String> {
    if target.is_empty() {
        return Ok(());
    }
    let repo = MessengerRepo::new(session.pool.clone());
    let me = session.row().name.clone();
    remove_one(session, &repo, &me, target).await?;
    remove_one(session, &repo, target, &me).await?;
    Ok(())
}

/// Una dirección de RemoveFromList(a, b): DELETE (a,b) + INFO a `a` si
/// online + GC REMOVE_FRIEND(nombre de `a`) a `b` si online (parity
/// __RemoveFromList messenger_manager.cpp:230-260).
async fn remove_one(
    session: &mut Session,
    repo: &MessengerRepo,
    a: &str,
    b: &str,
) -> Result<(), String> {
    repo.remove(a, b).await?;
    let is_me = a.eq_ignore_ascii_case(&session.row().name);
    if is_me {
        // INFO al propio jugador (por el socket directo).
        info(session, &format!(
            "<Messenger> {b} has been removed from your messenger."
        ))
        .await?;
    } else if let Some((vid, ..)) = crate::channel::chat::find_player(a) {
        send_to_vid_info(
            vid,
            crate::channel::chat::peer_empire(vid),
            &format!("<Messenger> {b} has been removed from your messenger."),
        );
    }
    if let Some((vid, ..)) = crate::channel::chat::find_player(b)
        && vid != session.player_vid() {
            send_status(vid, &psocial::remove_friend(a));
        }
    eprintln!(
        "server_realms: channel conn {}: messenger remove {a} -> {b}",
        session.conn_id
    );
    Ok(())
}

/// GC LIST al entrar al mundo (parity input_login.cpp:639 Login → LoadList →
/// SendList messenger_manager.cpp:44-141): UN paquete con connected según la
/// sesión online del companion; 0 filas → NADA (parity :341-343/:369-371).
/// Fallo PG → error arriba (el entry aborta el join — mismo contrato que las
/// demás queries del entry).
pub async fn send_login_list(session: &mut Session) -> Result<(), String> {
    let rows = MessengerRepo::new(session.pool.clone())
        .list(&session.row().name)
        .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let entries: Vec<psocial::ListEntry> = rows
        .iter()
        .map(|r| psocial::ListEntry {
            connected: crate::channel::chat::find_player(&r.companion).is_some(),
            name: r.companion.clone(),
        })
        .collect();
    let pkt = psocial::list(&entries);
    session
        .send(&pkt)
        .await
        .map_err(|e| format!("enviando GC MESSENGER LIST: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: messenger list de {} enviada ({} entradas)",
        session.conn_id,
        session.row().name,
        entries.len()
    );
    Ok(())
}

// --- helpers de entrega a OTRA sesión (outbox del chat — patrón whisper) ---

fn send_status(target_vid: u32, pkt: &[u8]) {
    if !crate::channel::chat::send_to_vid(target_vid, pkt) {
        eprintln!("server_realms: channel: messenger: outbox cerrado (status)");
    }
}

/// INFO GC_CHAT a OTRA sesión online (outbox), con el empire del receptor.
fn send_to_vid_info(target_vid: u32, target_empire: u8, text: &str) {
    let pkt = gc_chat(CHAT_TYPE_INFO, target_empire, text);
    if !crate::channel::chat::send_to_vid(target_vid, &pkt) {
        eprintln!("server_realms: channel: messenger: outbox cerrado (info)");
    }
}

/// INFO a una sesión por NOMBRE (para el rechazo — parity cmd_general.cpp:
/// 1184-1192, FindPC(arg2)); sin sesión online → nada.
fn send_info_to_name(name: &str, text: &str) {
    if let Some((vid, ..)) = crate::channel::chat::find_player(name) {
        send_to_vid_info(vid, crate::channel::chat::peer_empire(vid), text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use database::player::PlayerRow;
    use tokio::io::AsyncReadExt;

    /// Serializa los tests ASYNC (el registro de peers y el de peticiones son
    /// statics COMPARTIDOS — patrón TEST_LOCK de chat.rs).
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Fila mínima del player (patrón dummy_row de chat.rs).
    fn dummy_row(name: &str, map_index: i32, x: i32, y: i32) -> PlayerRow {
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
            horse_riding: 0,
            horse_hp: 0,
            horse_hp_droptime: 0,
            horse_stamina: 0,
            logoff_interval: 0.0,
            horse_skill_point: 0,
        }
    }

    /// Sesión de test (patrón de chat.rs — sockets localhost + pool lazy; los
    /// caminos probados aquí NO tocan PG — el staff-check es fail-open).
    async fn test_session(
        vid: u32,
        name: &str,
        account: &str,
        map_index: i32,
    ) -> (Session, tokio::net::TcpStream) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind localhost");
        let addr = listener.local_addr().expect("addr");
        let client_side = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (server_side, _peer) = listener.accept().await.expect("accept");
        let pool = database::pool::new_pool("host=127.0.0.1 port=1 user=x password=x dbname=x", 2)
            .expect("pool sin conectar (lazy)");
        let wal_dir = std::env::temp_dir()
            .join(format!("msg_test_wal_{}", std::process::id()))
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
        s.account_login = account.to_string();
        s.row = Some(dummy_row(name, map_index, 100, 100));
        // El id de la fila ES el vid en el wire (parity Session::player_vid):
        // sin esto el self-check del ADD_BY_VID (vid == player_vid) no ve el
        // propio registro y las claves de peticiones se desalinean.
        s.row.as_mut().expect("row").id = vid as i64;
        s.motion = Some(game_core::movement::initial(100, 100));
        s.chat_guard = Some(crate::channel::chat::register_peer(
            vid,
            name.to_string(),
            map_index,
            100,
            100,
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

    fn cg_messenger(sub: u8, payload: &[u8]) -> Vec<u8> {
        let mut pkt = vec![header::CG_MESSENGER, sub];
        pkt.extend_from_slice(payload);
        pkt
    }

    /// ADD_BY_VID válido → el DESTINO recibe el prompt GC_CHAT
    /// CHAT_TYPE_COMMAND "messenger_auth <inviter>" (parity :174) y la
    /// petición queda registrada (parity m_set_requestToAdd).
    #[tokio::test]
    async fn add_by_vid_prompts_target_with_messenger_auth() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut a, _a_sock) = test_session(701, "Alice", "acc_a", 80).await;
        let (mut b, _b_sock) = test_session(702, "Bob", "acc_b", 81).await;
        // staff-check fail-open (pool lazy sin PG): ambos PLAYER → continúa.
        let pkt = cg_messenger(psocial::SUB_CG_ADD_BY_VID, &702u32.to_le_bytes());
        handle(&mut a, &pkt).await.expect("ADD_BY_VID OK");
        let prompt = tokio::time::timeout(Duration::from_secs(2), b.chat_rx.recv())
            .await
            .expect("Bob recibe el prompt")
            .expect("outbox abierto");
        assert_eq!(prompt[0], header::GC_CHAT);
        assert_eq!(prompt[3], CHAT_TYPE_COMMAND);
        assert_eq!(&prompt[9..], b"messenger_auth Alice");
        assert!(
            requests()
                .lock()
                .unwrap()
                .contains(&("alice".into(), "bob".into())),
            "petición registrada (inviter, accepter)"
        );
    }

    /// ADD_BY_VID a sí mismo o a un vid desconectado → SILENCIO total
    /// (parity input_main.cpp:943/:957/:962 — returns tempranos sin INFO).
    #[tokio::test]
    async fn add_by_vid_self_and_offline_are_silent() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut a, _a_sock) = test_session(711, "Solo", "acc_s", 90).await;
        // Self.
        let self_pkt = cg_messenger(psocial::SUB_CG_ADD_BY_VID, &711u32.to_le_bytes());
        handle(&mut a, &self_pkt).await.expect("self OK");
        // Offline (vid 999 sin peer).
        let off_pkt = cg_messenger(psocial::SUB_CG_ADD_BY_VID, &999u32.to_le_bytes());
        handle(&mut a, &off_pkt).await.expect("offline OK");
        // Nada en la cola propia ni petición registrada para este par
        // (el registro es compartido entre tests — no se exige vacío).
        assert!(a.chat_rx.try_recv().is_err(), "sin eco");
        assert!(
            !requests().lock().unwrap().contains(&("solo".into(), "solo".into())),
            "self/offline NO registra petición"
        );
    }

    /// ADD_BY_NAME con nombre desconectado → INFO "%s is not connected."
    /// (parity :986-988). El staff-check es fail-open (sin PG).
    #[tokio::test]
    async fn add_by_name_unknown_sends_not_connected_info() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut a, mut a_sock) = test_session(721, "Seeker", "acc_k", 91).await;
        let mut name = [0u8; protocol::CHARACTER_NAME_MAX_LEN];
        name[..5].copy_from_slice(b"Ghost");
        let pkt = cg_messenger(psocial::SUB_CG_ADD_BY_NAME, &name);
        handle(&mut a, &pkt).await.expect("ADD_BY_NAME OK");
        let reply = read_packet(&mut a_sock).await;
        assert_eq!(reply[0], header::GC_CHAT);
        assert_eq!(reply[3], CHAT_TYPE_INFO);
        assert_eq!(&reply[9..], b"Ghost is not connected.");
    }

    /// AUTH sin petición previa → silencioso y SIN tocar DB (parity :185-195
    /// — sys_log + false; cierra el exploit del auto-add sin consentimiento).
    #[tokio::test]
    async fn auth_without_pending_request_is_silent() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut a, _a_sock) = test_session(731, "Victim", "acc_v", 92).await;
        let outcome = try_handle_command(&mut a, "messenger_auth y Stranger")
            .await
            .expect("comando OK")
            .expect("es comando social");
        assert_eq!(outcome, Outcome::Continue);
        assert!(a.chat_rx.try_recv().is_err(), "sin paquetes");
    }

    /// RECHAZO con petición previa → el INVITADOR online recibe el INFO
    /// "%s rejected your friend request." (parity cmd_general.cpp:1184-1192)
    /// y NO se toca la DB (deny no añade filas).
    #[tokio::test]
    async fn auth_deny_notifies_inviter_only_when_requested() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut inviter, _i_sock) = test_session(741, "Inviter", "acc_i", 93).await;
        let (mut accepter, _a_sock) = test_session(742, "Denier", "acc_d", 94).await;
        // Petición previa (como si Inviter hubiera invitado).
        requests()
            .lock()
            .unwrap()
            .insert(("inviter".into(), "denier".into()));
        try_handle_command(&mut accepter, "messenger_auth n Inviter")
            .await
            .expect("comando OK")
            .expect("social");
        let reply = tokio::time::timeout(Duration::from_secs(2), inviter.chat_rx.recv())
            .await
            .expect("el invitador recibe el rechazo")
            .expect("outbox abierto");
        assert_eq!(reply[3], CHAT_TYPE_INFO);
        assert_eq!(&reply[9..], b"Denier rejected your friend request.");
        assert!(
            !requests()
                .lock()
                .unwrap()
                .contains(&("inviter".into(), "denier".into())),
            "la petición se consume"
        );
    }

    /// Subheader desconocido → log + Continue SIN cerrar (parity :1031-1035)
    /// y parseo del nombre crudo con corte en NUL (strlcpy parity).
    #[tokio::test]
    async fn unknown_subheader_continues_and_name_at_strips_nul() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut a, _a_sock) = test_session(751, "Any", "acc_any", 95).await;
        let pkt = cg_messenger(0xEE, &[0u8; 24]);
        let outcome = handle(&mut a, &pkt).await.expect("desconocido OK");
        assert_eq!(outcome, Outcome::Continue, "sin cierre");
        // name_at: corta en el primer NUL y capa a 24 B.
        let mut raw = vec![header::CG_MESSENGER, psocial::SUB_CG_REMOVE];
        raw.extend_from_slice(b"Target\0junk");
        raw.resize(psocial::CG_NAME_TOTAL, 0xAB); // basura tras el NUL
        assert_eq!(name_at(&raw), "Target");
    }

    /// Prefijo del comando: "messenger_authX ..." NO es de este módulo (cae
    /// al dispatch GM) — strip_prefix + frontera de separador.
    #[tokio::test]
    async fn command_prefix_requires_word_boundary() {
        let _g = TEST_LOCK.lock().unwrap();
        let (mut a, _a_sock) = test_session(761, "Edge", "acc_e", 96).await;
        assert!(
            try_handle_command(&mut a, "messenger_authX y Bob")
                .await
                .unwrap()
                .is_none()
        );
        assert!(try_handle_command(&mut a, "messenger_authy").await.unwrap().is_none());
        // Sin argumentos → consumido pero silencioso (parity :1177-1179).
        assert_eq!(
            try_handle_command(&mut a, "messenger_auth").await.unwrap(),
            Some(Outcome::Continue)
        );
    }

    /// LIVE-PG (gated, patrón locale.rs): aceptar persiste AMBAS direcciones
    /// y remove borra AMBAS (@fixme183) — el contrato real contra
    /// player.messenger_list (nombres de personaje en ambas columnas).
    #[tokio::test]
    #[ignore = "requiere PG real: cargo test --package server_realms -- --ignored"]
    async fn accept_and_remove_persist_both_directions_live_pg() {
        let pg = std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| {
            "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2".to_string()
        });
        let repo = MessengerRepo::new(database::pool::new_pool(&pg, 2).expect("pool"));
        // Limpieza defensiva previa (idempotencia del test).
        let _ = repo.remove("MsgPairA", "MsgPairB").await;
        let _ = repo.remove("MsgPairB", "MsgPairA").await;

        // Accept: ambas direcciones (parity AuthToAdd :200-201).
        assert_eq!(repo.add("MsgPairA", "MsgPairB").await.expect("add 1"), 1);
        assert_eq!(repo.add("MsgPairB", "MsgPairA").await.expect("add 2"), 1);
        // Idempotencia (ON CONFLICT — replay del WAL).
        assert_eq!(repo.add("MsgPairA", "MsgPairB").await.expect("re-add"), 0);

        let list_a = repo.list("MsgPairA").await.expect("list A");
        assert_eq!(list_a.len(), 1);
        assert_eq!(list_a[0].companion, "MsgPairB", "columna companion = nombre");
        assert_eq!(list_a[0].account, "MsgPairA");

        // Remove en AMBAS direcciones (parity @fixme183).
        repo.remove("MsgPairA", "MsgPairB").await.expect("remove 1");
        repo.remove("MsgPairB", "MsgPairA").await.expect("remove 2");
        assert!(repo.list("MsgPairA").await.expect("list post").is_empty());
        assert!(repo.list("MsgPairB").await.expect("list post B").is_empty());
    }
}
