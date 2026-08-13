//! `channel/entry.rs` — las fases de ENTRADA del canal (R-s2 del refactor):
//! handshake → login → select → player load → ENTERGAME → world join.
//!
//! Antes vivían en `connection_inner` (channel.rs:291-570) con ~20 locales;
//! desde R-s1 operan sobre `session::Session` y desde R-s2 viven en este
//! módulo — el loop de juego (game.rs) solo corre con la sesión llena.
//!
//! Paridad con el canal C++ (`input_login.cpp` + `input_db.cpp` + `desc.cpp`):
//! 1. Handshake server-side (`network::handshake` — igual que el auth).
//! 2. `GC_PHASE(LOGIN)` — parity `input.cpp:194-196`.
//! 3. `CG_LOGIN3` (65 B — el framer con rol Channel ya lo entrega así).
//! 4. Validaciones en orden (parity `input_login.cpp:97-147` + `db.cpp:244-365`).
//! 5. Credenciales vs PG (`AccountRepo::login` — 13 columnas).
//! 6. `GC_EMPIRE` + `GC_PHASE(SELECT)` + `GC_LOGIN_SUCCESS_NEWSLOT` (449 B).
//! 7. `CG_PLAYER_SELECT` → `WorldStore::select_player` → PLAYER LOAD → wait
//!    CG_ENTERGAME → ENTERGAME → join al MUNDO COMPARTIDO (ADR-0010).

use database::account::AccountRepo;
use database::affect::AffectRepo;
use database::common::CommonRepo;
use database::item::ItemRepo;
use database::land::LandRepo;
use network::handshake::HandshakeError;
use network::{handshake, Connection};
use protocol::world::{TPacketGCChannel, TPacketGCTime};
use protocol::{
    header, phase, TPacketCGLogin3, TPacketCGPlayerSelect, TPacketGCEmpire, TPacketGCLoginFailure,
    TPacketGCLoginSuccess, TPacketGCPhase, PLAYER_PER_ACCOUNT,
};
use game_core::ecs::{Intent, PlayerJoin};
use game_core::packets;
use game_core::world::WorldStore;
use tokio::net::TcpStream;

use crate::auth::{is_valid_login_string, normalize_login};
use crate::channel::session::{ChannelLoginGuard, LeaveGuard, Session};
use crate::channel::{equipped_armor, now_ms, parse_listen};

/// Fases 1-7 de la conexión del canal (parity `input_login.cpp` +
/// `input_db.cpp` + `desc.cpp`): el flujo completo hasta que el jugador
/// entra al MUNDO COMPARTIDO. Al volver, la sesión está LLENA (row/store/
/// motion/leave seteados) y el game loop puede correr.
///
/// Retornos tempranos `Ok(())` = cierre limpio del protocolo (guild mark,
/// login rechazado, slot vacío) — el mismo comportamiento que tenía el
/// cuerpo único de `connection_inner`.
pub async fn run(session: &mut Session) -> Result<(), String> {
    // 1. Handshake server-side (F1.5, validado contra el canal real en F1.6).
    //    El cliente del GUILD MARK abre una conexión SEPARADA en paralelo al
    //    select y responde al handshake con CG_MARK_LOGIN (0x64) en vez del
    //    eco (`GuildMarkDownloader.cpp:213-229`). El canal normal
    //    (`guild_mark_server` OFF — config del runtime) cierra esa conexión
    //    sin responder (`input.cpp:560-572`) — el cliente NO lo interpreta
    //    como fallo (el mark es opcional; el select sigue en la otra conexión).
    let hs = match handshake::perform(&mut session.conn, &mut session.framer, now_ms()).await {
        Err(HandshakeError::MarkLogin(p)) => {
            eprintln!(
                "server_realms: channel conn {}: guild mark login (handle 0x{:08x}, \
                 random 0x{:08x}) — no mark server, cierre limpio (parity input.cpp:562-566)",
                session.conn_id, p.handle, p.random_key
            );
            return Ok(());
        }
        Err(e) => return Err(format!("handshake: {e}")),
        Ok(hs) => hs,
    };
    eprintln!(
        "server_realms: channel conn {}: handshake OK (delta {} ms)",
        session.conn_id, hs.delta
    );

    // 2. GC_PHASE(LOGIN) — el cliente responde con el LOGIN3 del canal (65 B).
    session
        .send(&TPacketGCPhase::new(phase::LOGIN).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_PHASE(LOGIN): {e}"))?;
    eprintln!("server_realms: channel conn {}: enviado GC_PHASE(LOGIN)", session.conn_id);

    // 3. LOGIN3 (65 B al canal — framer rol Channel).
    let login3 = loop {
        let pkt = session.recv_idle().await?;
        match pkt[0] {
            header::CG_TIME_SYNC | header::CG_PONG => continue, // keepalives (F1.4)
            header::CG_LOGIN3 => {
                break TPacketCGLogin3::from_bytes(&pkt).map_err(|e| format!("LOGIN3: {e}"))?
            }
            other => {
                return Err(format!(
                    "channel conn {}: header inesperado 0x{other:02x} tras el handshake",
                    session.conn_id
                ))
            }
        }
    };
    let login = normalize_login(&login3.login);
    let passwd = cstr(&login3.passwd).to_string();
    eprintln!("server_realms: channel conn {}: LOGIN3 login={login}", session.conn_id);

    // 4. Validaciones (parity input_login.cpp:97-147 + db.cpp:244-365).
    if !is_valid_login_string(&login) {
        send_login_failure(&mut session.conn, session.conn_id, "NOID").await?;
        return Ok(());
    }
    if session.config.no_more_clients {
        send_login_failure(&mut session.conn, session.conn_id, "SHUTDOWN").await?;
        return Ok(());
    }
    let Some(_guard) = ChannelLoginGuard::acquire(&login) else {
        send_login_failure(&mut session.conn, session.conn_id, "ALREADY").await?;
        return Ok(());
    };
    // El guard libera el login al CERRAR la conexión (vive en la sesión).
    session.login_guard = Some(_guard);

    // 5. Credenciales vs PG (QUERY_LOGIN — 13 columnas; el canal C++ hace
    //    GD_LOGIN → db → RESULT_LOGIN, `db.cpp:244-365`).
    let acc = match AccountRepo::new(&session.config.pg_conn).login(&login, &passwd).await {
        Ok(Some(acc)) => acc,
        Ok(None) => {
            eprintln!("server_realms: channel conn {}: NOID {login}", session.conn_id);
            send_login_failure(&mut session.conn, session.conn_id, "NOID").await?;
            return Ok(());
        }
        Err(e) => {
            // Divergencia documentada: DB caída -> NOTAVAIL (determinista).
            eprintln!(
                "server_realms: channel conn {}: PG falló para {login}: {e} — NOTAVAIL",
                session.conn_id
            );
            send_login_failure(&mut session.conn, session.conn_id, "NOTAVAIL").await?;
            return Ok(());
        }
    };
    if acc.status != "OK" {
        eprintln!(
            "server_realms: channel conn {}: status '{}' para {login}",
            session.conn_id, acc.status
        );
        send_login_failure(&mut session.conn, session.conn_id, &acc.status).await?;
        return Ok(());
    }
    eprintln!(
        "server_realms: channel conn {}: login OK {login} (id {}, empire {:?})",
        session.conn_id, acc.id, acc.empire
    );
    // El login de la cuenta queda en la sesión (el gmlist de GM — la pareja
    // mName/mAccount — lo consulta channel/gm.rs por comando).
    session.account_login = login.clone();

    // 6. WorldStore (repos + Batcher) + empire + paquete del select.
    let store = match WorldStore::new(&session.config.pg_conn).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("server_realms: channel conn {}: WorldStore: {e} — NOTAVAIL", session.conn_id);
            send_login_failure(&mut session.conn, session.conn_id, "NOTAVAIL").await?;
            return Ok(());
        }
    };
    session.store = Some(store);
    session.empire = empire_byte(acc.empire);
    eprintln!("server_realms: channel conn {}: empire={}", session.conn_id, session.empire);

    // GC_EMPIRE (0x5a) + GC_PHASE(SELECT) + 449 B (parity input_db.cpp:169-183).
    session
        .send(&TPacketGCEmpire::new(session.empire).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_EMPIRE: {e}"))?;
    session
        .send(&TPacketGCPhase::new(phase::SELECT).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_PHASE(SELECT): {e}"))?;
    let success = build_login_success(session.store(), acc.id, session.conn_id, &session.config.listen)
        .await?;
    let bytes = success.to_bytes();
    assert_eq!(bytes.len(), TPacketGCLoginSuccess::SIZE, "449 B (invariante wire)");
    session
        .send(&bytes)
        .await
        .map_err(|e| format!("enviando 449 B: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: enviado GC_EMPIRE + GC_PHASE(SELECT) + 449 B",
        session.conn_id
    );

    // 7. Select: CG_PLAYER_SELECT (2 B) → load → spawn best-effort.
    let select = loop {
        let pkt = session.recv_idle().await?;
        match pkt[0] {
            header::CG_TIME_SYNC | header::CG_PONG => continue,
            header::CG_CHARACTER_SELECT => {
                break TPacketCGPlayerSelect::from_bytes(&pkt)
                    .map_err(|e| format!("CG_PLAYER_SELECT: {e}"))?
            }
            other => {
                return Err(format!(
                    "channel conn {}: header inesperado 0x{other:02x} esperando el select",
                    session.conn_id
                ))
            }
        }
    };
    eprintln!(
        "server_realms: channel conn {}: CG_PLAYER_SELECT index={}",
        session.conn_id, select.index
    );

    let Some(row) = session.store().select_player(acc.id, select.index).await? else {
        // Parity input_login.cpp:266-271 ("player index not found" -> CLOSE).
        eprintln!("server_realms: channel conn {}: slot vacío/inválido — cierre", session.conn_id);
        return Ok(());
    };
    session.row = Some(row);
    // F5.1: el estado de movimiento del jugador (posición del load).
    session.motion = Some(game_core::movement::initial(session.row().x, session.row().y));
    eprintln!(
        "server_realms: channel conn {}: player_load {} id={} lvl={} x={} y={} map={}",
        session.conn_id,
        session.row().name,
        session.row().id,
        session.row().level,
        session.row().x,
        session.row().y,
        session.row().map_index
    );

    // ------------------------------------------------------------------
    // PLAYER LOAD (parity input_db.cpp:428-459 + los DG_* asíncronos del db):
    // GC_PHASE(LOADING) -> MainCharacter (15) -> [SDB 153: NO — runtime sin
    // package] -> 36×QUICKSLOT_ADD (28 — SetQuickslot por slot,
    // char_quickslot.cpp:96-103) -> Points (16, con los MÁXIMOS + NEXT_EXP)
    // -> Skills (76) -> N×ITEM_SET (21, ItemLoad input_db.cpp:1453-1561) ->
    // M×AFFECT_ADD (126, AffectLoad input_db.cpp:1563-1583).
    // ------------------------------------------------------------------
    // F5.3: next_exp MUTABLE — el level-up del kill lo recalcula por nivel.
    session.next_exp = CommonRepo::new(&session.config.pg_conn)
        .next_exp(session.row().level)
        .await
        .unwrap_or(0);
    // Inventario del jugador (F5.3): MUTABLE — el pickup (CG_ITEM_PICKUP)
    // busca el primer cell libre y añade el item recogido.
    session.inventory = ItemRepo::new(&session.config.pg_conn)
        .load_by_owner(session.row().id)
        .await?;
    session.affects = AffectRepo::new(&session.config.pg_conn).load(session.row().id).await?;
    for pkt in entry_packets(session.row(), session.next_exp, &session.inventory, &session.affects) {
        session.send(&pkt).await.map_err(|e| format!("enviando entry: {e}"))?;
    }
    eprintln!(
        "server_realms: channel conn {}: entry enviado (LOADING + MAIN_CHARACTER + {} quickslots + \
         POINTS + SKILLS + {} items + {} affects) — esperando CG_ENTERGAME del cliente",
        session.conn_id,
        packets::quickslot_packets(session.row().quickslot.as_ref()).len(),
        session.inventory.len(),
        session.affects.len()
    );

    // El cliente carga el mapa (Warp) y manda CG_ENTERGAME (10, 1 B) al
    // abrir la ventana del juego (game.py:206 SendEnterGamePacket). Antes
    // manda la VERSIÓN del cliente (0xf1, 67 B) — se ignora sin validar
    // (parity input.cpp:205-213).
    loop {
        let pkt = session.recv_idle().await?;
        match pkt[0] {
            header::CG_TIME_SYNC | header::CG_PONG => continue,
            header::CG_CLIENT_VERSION2 => {
                let name_end = pkt[1..34].iter().position(|&b| b == 0).unwrap_or(33);
                let ts_end = pkt[34..67].iter().position(|&b| b == 0).unwrap_or(33);
                eprintln!(
                    "server_realms: channel conn {}: VERSION {} {} — ignorado sin validar \
                     (parity input.cpp:205-213)",
                    session.conn_id,
                    String::from_utf8_lossy(&pkt[1..1 + name_end]),
                    String::from_utf8_lossy(&pkt[34..34 + ts_end])
                );
                continue;
            }
            header::CG_ENTERGAME => break,
            other => {
                return Err(format!(
                    "channel conn {}: header inesperado 0x{other:02x} esperando CG_ENTERGAME",
                    session.conn_id
                ))
            }
        }
    }
    eprintln!("server_realms: channel conn {}: CG_ENTERGAME recibido", session.conn_id);

    // ------------------------------------------------------------------
    // ENTERGAME (parity input_login.cpp:611-656): ADD (1) + INFO (136) via
    // Show()/EncodeInsertPacket -> GC_PHASE(GAME) -> LandList (130) ->
    // GC_TIME (106, get_global_time) -> GC_CHANNEL (121, g_bChannel).
    // ------------------------------------------------------------------
    let lands = LandRepo::new(&session.config.pg_conn)
        .load_by_map(i64::from(session.row().map_index))
        .await?;
    if lands.is_empty() {
        eprintln!(
            "server_realms: channel conn {}: mapa {} sin lands — el C++ no manda el paquete (building.cpp:969)",
            session.conn_id, session.row().map_index
        );
    }
    let mut enter = enter_packets(
        session.row(),
        session.empire,
        &lands,
        &packets::equipped_parts(session.row(), &session.inventory),
        super::equipped_arrow_index(&session.inventory)
            .map(|i| session.inventory[i].count as u32)
            .unwrap_or(0),
    );
    // Cola de entrada (parity input_login.cpp:648-656): TIME + CHANNEL tras
    // el land list — el reloj del server (get_global_time) y el canal.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0);
    enter.push(TPacketGCTime::new(now).to_bytes().to_vec());
    enter.push(TPacketGCChannel::new(session.config.channel).to_bytes().to_vec());
    for pkt in enter {
        session.send(&pkt).await.map_err(|e| format!("enviando enter: {e}"))?;
    }
    eprintln!(
        "server_realms: channel conn {}: ENTERGAME enviado (ADD + INFO + GC_PHASE(GAME) + {} lands \
         + GC_TIME + GC_CHANNEL {}) — el cliente está DENTRO del mapa",
        session.conn_id, lands.len(), session.config.channel
    );

    // ------------------------------------------------------------------
    // F5.3 (ADR-0010): el jugador entra al MUNDO COMPARTIDO del canal — la
    // conexión envía el intent `Join` por el mpsc (patrón Veloren) y recibe
    // los eventos S→C por su cola. El SPAWN DINÁMICO del mundo materializa
    // los mobs del mapa según la posición del jugador (el filtro estático
    // SPAWN_VIEW del entry se ELIMINÓ en este slice): los adds llegan por la
    // cola (eventos `Spawned`, parity `game_core::npc::entry_spawns`) en cuanto
    // el mundo procesa el join — mismo wire que el entry previo.
    // ------------------------------------------------------------------
    let max_points = packets::compute_max_points(session.row()).unwrap_or([100, 100, 0]);
    let armor = equipped_armor(&session.inventory, &session.config.pg_conn).await?;
    let join = Intent::Join {
        player: PlayerJoin {
            vid: session.player_vid(),
            map_index: session.row().map_index as u32,
            x: session.motion().x,
            y: session.motion().y,
            hp: session.row().hp,
            max_hp: max_points[0],
            mp: session.row().mp,
            max_mp: max_points[1],
            skill_level: session.row().skill_level.clone().unwrap_or_default(),
            level: i32::from(session.row().level),
            ht: i32::from(session.row().ht),
            armor,
            job: session.row().job as u8,
            st: i32::from(session.row().st),
            dx: i32::from(session.row().dx),
            iq: i32::from(session.row().iq),
        },
        out: session.event_tx.clone(),
    };
    session
        .intent_tx
        .send(join)
        .map_err(|e| format!("join al mundo compartido: {e}"))?;
    // RAII: al terminar la conexión (CUALQUIER return del handler) se limpia
    // la entidad del jugador del mundo (`Intent::Leave`).
    session.leave = Some(LeaveGuard {
        player_vid: session.player_vid(),
        tx: session.intent_tx.clone(),
    });
    eprintln!(
        "server_realms: channel conn {}: {} en el mundo compartido (mapa {}) — \
         los adds de los mobs visibles llegan por la cola",
        session.conn_id, session.row().name, session.row().map_index
    );
    Ok(())
}

/// Armado del 449 B: slots del índice (orden del player_index) emparejados con
/// los summaries del Q3 por id (parity `CreateAccountPlayerDataFromRes:315-317`
/// — el C++ empareja por dwID; un slot con pid pero sin fila Q3 queda como
/// TSimplePlayer zeroed, divergencia menor documentada: el C++ deja el dwID
/// puesto y stats 0, el Rust lo deja todo a 0).
///
/// El `lAddr`/`wPort` de cada slot = la dirección REAL del canal (del config
/// `listen`): el DirectEnter del cliente conecta ahí
/// (`introselect.cpp` → `ConnectGameServer`, `PythonNetworkStream.cpp:458-469`).
/// Con 0/0 el cliente conecta a `0.0.0.0:0` → OnConnectFailure → ClosePhase
/// (causa del cierre en el select, slice 3.5).
async fn build_login_success(
    store: &WorldStore,
    account_id: i64,
    handle: u32,
    listen: &str,
) -> Result<TPacketGCLoginSuccess, String> {
    let (ip, port) = parse_listen(listen)?;
    let server_ip = packets::ip_to_inet_addr(&ip)?;
    let slots = store.account_slots(account_id).await?;
    let summaries = store.list_characters(account_id).await?;
    let mut players: [Option<database::player::PlayerSummary>; PLAYER_PER_ACCOUNT] =
        [None, None, None, None, None];
    for (i, pid) in slots.iter().enumerate() {
        if let Some(pid) = pid {
            if let Some(s) = summaries.iter().find(|s| s.id == *pid) {
                players[i] = Some(s.clone());
            }
        }
    }
    eprintln!(
        "server_realms: 449 B con server de juego {}:{} (DirectEnter)",
        ip, port
    );
    Ok(packets::login_success(&players, handle, crate::channel::rand32(), server_ip, port))
}

/// `GC_LOGIN_FAILURE` con log (patrón del auth).
async fn send_login_failure(conn: &mut Connection<TcpStream>, conn_id: u32, status: &str) -> Result<(), String> {
    eprintln!("server_realms: channel: GC_LOGIN_FAILURE {status}");
    crate::channel::session::conn_send(conn, conn_id, &TPacketGCLoginFailure::new(status).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_LOGIN_FAILURE: {e}"))
}

/// C-string → `&str` (hasta el primer NUL; bytes no-UTF8 → vacío defensivo).
fn cstr(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// Empire del `GC_EMPIRE`: 1..3 de la cuenta -> su valor; `None`/0 -> random
/// 1..3 (parity `input_db.cpp:167-180`: `GetServerLocation(0)` falla y el C++
/// manda `number(1, 3)`).
fn empire_byte(empire: Option<i16>) -> u8 {
    match empire {
        Some(1..=3) => empire.unwrap() as u8,
        _ => (crate::channel::rand32() % 3) as u8 + 1,
    }
}

/// Paquetes del PLAYER LOAD (parity `input_db.cpp:428-459` + los DG_*
/// asíncronos del db — `ItemLoad`/`AffectLoad`): `GC_PHASE(LOADING)` ->
/// `TPacketGCMainCharacter` (15) -> 36×`TPacketGCQuickSlotAdd` (28) ->
/// `TPacketGCPoints` (16, con los MÁXIMOS del subset ComputePoints +
/// NEXT_EXP) -> `TPacketGCSkillLevel` (76) -> N×`TPacketGCItemSet` (21) ->
/// M×`TPacketGCAffectAdd` (126). El SDB (153) no se manda (runtime sin
/// package — parity del C++ actual). Función pura (testeable sin red).
fn entry_packets(
    row: &database::player::PlayerRow,
    next_exp: i64,
    items: &[database::item::ItemRow],
    affects: &[database::affect::AffectRow],
) -> Vec<Vec<u8>> {
    let mut out = vec![
        TPacketGCPhase::new(phase::LOADING).to_bytes().to_vec(),
        packets::main_character(row).to_bytes().to_vec(),
    ];
    out.extend(packets::quickslot_packets(row.quickslot.as_ref()));
    out.push(packets::points_packet(row, next_exp).to_bytes().to_vec());
    out.push(packets::skill_level_packet(row.skill_level.as_ref()).to_bytes().to_vec());
    out.extend(packets::item_set_packets(items));
    out.extend(packets::affect_add_packets(affects));
    out
}

/// Paquetes del ENTERGAME (parity `input_login.cpp:611-616,644`):
/// `TPacketGCCharacterAdd` (1) + `TPacketGCCharacterAdditionalInfo` (136)
/// [Show/EncodeInsertPacket, `char.cpp:876-948`] -> `GC_PHASE(GAME)` ->
/// `TPacketGCLandList` (130, `building.cpp:931-979`). Función pura.
/// F5.3: `parts` = los 5 parts COMPUTADOS del equipo (ComputeParts — el
/// personaje muestra el arma/armadura al entrar; `equipped_parts`).
/// `arrows` = count de flechas equipadas (dw_arrow — ENABLE_QUIVER_SYSTEM).
fn enter_packets(
    row: &database::player::PlayerRow,
    empire: u8,
    lands: &[database::land::LandRow],
    parts: &[u32; 5],
    arrows: u32,
) -> Vec<Vec<u8>> {
    let mut out = vec![
        packets::character_add(row).to_bytes().to_vec(),
        packets::character_additional_info_with_parts(row, empire, parts, arrows).to_bytes().to_vec(),
        TPacketGCPhase::new(phase::GAME).to_bytes().to_vec(),
    ];
    if !lands.is_empty() {
        out.push(packets::land_list(lands));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::player::PlayerRow;
    use protocol::world::{
        TPacketGCAffectAdd, TPacketGCItemSet, TPacketGCMainCharacter, TPacketGCPoints,
        TPacketGCSkillLevel,
    };
    use protocol::{
        TPacketGCCharacterAdd, TPacketGCCharacterAdditionalInfo, TPacketGCEmpire,
        TPacketGCLoginFailure, TPacketGCLoginSuccess,
    };

    /// Empire: Some(1..=3) se usa tal cual; None/0/4+ -> random 1..3 (parity
    /// input_db.cpp:167-180 — el random se cubre probando el rango).
    #[test]
    fn empire_byte_parity() {
        assert_eq!(empire_byte(Some(1)), 1);
        assert_eq!(empire_byte(Some(3)), 3);
        assert_eq!(empire_byte(Some(2)), 2);
        for src in [None, Some(0), Some(4), Some(-1)] {
            let e = empire_byte(src);
            assert!((1..=3).contains(&e), "empire random 1..3: {e} ({src:?})");
        }
        // El random no degenera en un solo valor.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            seen.insert(empire_byte(None));
        }
        assert!(seen.len() >= 2, "el random no degenera: {seen:?}");
    }

    /// Entry (PLAYER LOAD): la cola completa — LOADING + MAIN_CHARACTER +
    /// 36×QUICKSLOT + POINTS + SKILLS + items + affects (parity input_db.cpp
    /// 428-459 + ItemLoad/AffectLoad). Tamaños byte-exactos.
    #[test]
    fn entry_packets_order_and_sizes() {
        use database::affect::AffectRow;
        use database::item::ItemRow;
        let row = dummy_row();
        let items = vec![ItemRow {
            id: 100,
            window: "INVENTORY".into(),
            pos: 0,
            count: 1,
            vnum: 27001,
            sockets: [0, 0, 0],
            attrs: [(0, 0); 7],
        }];
        let affects = vec![AffectRow {
            dw_pid: 2,
            b_type: 1,
            b_apply_on: 2,
            l_apply_value: 3,
            dw_flag: 4,
            l_duration: 5,
            l_sp_cost: 6,
        }];
        let pkts = entry_packets(&row, 300, &items, &affects);
        // 2 (LOADING+15) + 36 quickslots + 2 (16+76) + 1 item + 1 affect.
        assert_eq!(pkts.len(), 42, "2 + 36 + 2 + 1 + 1");
        assert_eq!(pkts[0].len(), TPacketGCPhase::SIZE);
        assert_eq!(pkts[0][0], header::GC_PHASE);
        assert_eq!(pkts[0][1], phase::LOADING, "parity input_db.cpp:428");
        assert_eq!(pkts[1].len(), TPacketGCMainCharacter::SIZE, "47 B (layout del cliente, sin empire)");
        assert_eq!(pkts[1][0], TPacketGCMainCharacter::HEADER);
        // Quickslots: 36 × 4 B en orden (parity input_db.cpp:455-456).
        for (i, q) in pkts[2..38].iter().enumerate() {
            assert_eq!(q.len(), 4, "quickslot {i}");
            assert_eq!(q[0], 28, "header GC_QUICKSLOT_ADD");
            assert_eq!(q[1], i as u8, "pos del slot {i}");
        }
        assert_eq!(pkts[38].len(), TPacketGCPoints::SIZE, "16, 1021 B");
        assert_eq!(pkts[38][0], TPacketGCPoints::HEADER);
        assert_eq!(pkts[39].len(), TPacketGCSkillLevel::SIZE, "76, 1531 B");
        assert_eq!(pkts[39][0], TPacketGCSkillLevel::HEADER);
        assert_eq!(pkts[40].len(), TPacketGCItemSet::SIZE, "item set 51 B");
        assert_eq!(pkts[40][0], TPacketGCItemSet::HEADER, "header 21");
        assert_eq!(pkts[41].len(), TPacketGCAffectAdd::SIZE, "affect add 22 B");
        assert_eq!(pkts[41][0], TPacketGCAffectAdd::HEADER, "header 126");
        // MainCharacter: vid@1, lx@34 (spot).
        assert_eq!(u32::from_le_bytes([pkts[1][1], pkts[1][2], pkts[1][3], pkts[1][4]]), 2);
        assert_eq!(i32::from_le_bytes([pkts[1][34], pkts[1][35], pkts[1][36], pkts[1][37]]), 969600);
        // Points: level@5 = 5 (parity char.cpp:1562), NEXT_EXP@17 = 300,
        // MAX_HP@25 = 1850 (650 + 30×40 — dummy job=1/ASSASSIN, ht=30).
        assert_eq!(i32::from_le_bytes([pkts[38][5], pkts[38][6], pkts[38][7], pkts[38][8]]), 5);
        assert_eq!(i32::from_le_bytes([pkts[38][17], pkts[38][18], pkts[38][19], pkts[38][20]]), 300, "NEXT_EXP");
        assert_eq!(i32::from_le_bytes([pkts[38][25], pkts[38][26], pkts[38][27], pkts[38][28]]), 1850, "MAX_HP > 0 (ComputePoints subset)");
        // MOV_SPEED@77 (1 + 19×4) = 100 (parity char.cpp:2245).
        assert_eq!(i32::from_le_bytes([pkts[38][77], pkts[38][78], pkts[38][79], pkts[38][80]]), 100);
        // Sin items/affects: 40 paquetes (2 + 36 + 2).
        assert_eq!(entry_packets(&row, 300, &[], &[]).len(), 40);
    }

    /// Enter (ENTERGAME): ADD + INFO + GAME (+ land list si hay lands) —
    /// parity input_login.cpp:611-616,644.
    #[test]
    fn enter_packets_order_and_sizes() {
        use database::land::LandRow;
        let row = dummy_row();
        let lands: Vec<LandRow> = vec![LandRow {
            id: 201,
            map_index: 41,
            x: 66100,
            y: 9400,
            width: 3000,
            height: 3000,
            guild_id: 0,
        }];
        let parts = packets::equipped_parts(&row, &[]);
        let pkts = enter_packets(&row, 3, &lands, &parts, 0);
        assert_eq!(pkts.len(), 4, "ADD + INFO + GAME + LAND_LIST");
        assert_eq!(pkts[0].len(), TPacketGCCharacterAdd::SIZE);
        assert_eq!(pkts[0][0], header::GC_CHARACTER_ADD);
        assert_eq!(pkts[1].len(), TPacketGCCharacterAdditionalInfo::SIZE);
        assert_eq!(pkts[1][0], header::GC_CHAR_ADDITIONAL_INFO);
        assert_eq!(pkts[2].len(), TPacketGCPhase::SIZE);
        assert_eq!(pkts[2][0], header::GC_PHASE);
        assert_eq!(pkts[2][1], phase::GAME, "parity input_login.cpp:616");
        assert_eq!(pkts[3][0], 130, "GC_LAND_LIST");
        assert_eq!(u16::from_le_bytes([pkts[3][1], pkts[3][2]]), 27, "3 + 1×24");
        // Sin lands -> 3 paquetes (el C++ no manda el paquete vacío).
        assert_eq!(enter_packets(&row, 3, &[], &parts, 0).len(), 3);
    }

    /// Tamaños del wire del flujo select/spawn (invariante byte-exacto).
    #[test]
    fn select_spawn_wire_sizes() {
        assert_eq!(TPacketGCEmpire::SIZE, 2, "GC_EMPIRE 0x5a");
        assert_eq!(TPacketGCLoginSuccess::SIZE, 449, "0x20 NEWSLOT");
        assert_eq!(TPacketCGPlayerSelect::SIZE, 2, "CG_PLAYER_SELECT");
        assert_eq!(TPacketGCPhase::SIZE, 2);
        assert_eq!(TPacketGCLoginFailure::SIZE, 10, "GC_LOGIN_FAILURE");
    }

    /// Fila del player del harness (parity del dummy del channel_pg E2E).
    fn dummy_row() -> PlayerRow {
        PlayerRow {
            id: 2,
            name: "ninja".into(),
            job: 1,
            voice: 0,
            dir: 0,
            x: 969600,
            y: 278400,
            z: 0,
            map_index: 41,
            exit_x: 0,
            exit_y: 0,
            exit_map_index: 0,
            hp: 100,
            mp: 100,
            stamina: 100,
            random_hp: 0,
            random_sp: 0,
            playtime: 0,
            gold: 0,
            level: 5,
            level_step: 0,
            st: 30,
            ht: 30,
            dx: 30,
            iq: 30,
            exp: 0,
            stat_point: 0,
            skill_point: 0,
            sub_skill_point: 0,
            stat_reset_count: 0,
            part_base: 0,
            part_hair: 0xAABB_CCDD,
            part_main: 0x1122_3344,
            skill_level: None,
            quickslot: None,
            skill_group: 3,
            alignment: 1234,
            horse_level: 0,
            horse_riding: 0,
            horse_hp: 0,
            horse_hp_droptime: 0,
            horse_stamina: 0,
            logoff_interval: 0.0,
            horse_skill_point: 0,
        }
    }
}
