//! `channel/entry.rs` — las fases de ENTRADA del canal (R-s2 del refactor):
//! login DIRECTO → select → player load → ENTERGAME → world join.
//!
//! Antes vivían en `connection_inner` (channel.rs:291-570) con ~20 locales;
//! desde R-s1 operan sobre `session::Session` y desde R-s2 viven en este
//! módulo — el loop de juego (game.rs) solo corre con la sesión llena.
//!
//! Paridad con el canal C++ (`input_login.cpp` + `input_db.cpp` + `desc.cpp`):
//! 1. `GC_PHASE(LOGIN)` DIRECTO al aceptar (divergencia deliberada 2026-08-14:
//!    el C++ del canal handshakeaba — `desc.cpp:258`; el rewrite entra
//!    directo a Login porque el cliente legacy conecta con `Connect()` crudo
//!    (KEEP_ACCOUNT_CONNETION_ENABLE=1) y dispara su LOGIN3 al procesar la
//!    fase — sin handshake la carrera intermitente (ni eco ni LOGIN3 tras
//!    32 intentos + 45 s de leniencia) NO PUEDE ocurrir; el reloj del cliente
//!    queda alineado con el AUTH (ambos procesos arrancan juntos) y el canal
//!    manda GC_TIME al entrar al mundo — ADR-0010/0011, rediseño).
//! 2. `CG_LOGIN3` (65 B — el framer con rol Channel ya lo entrega así) o
//!    `CG_MARK_LOGIN` (0x64, 9 B — la conexión paralela del guild mark:
//!    cierre limpio sin responder, parity `input.cpp:560-572`).
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
use database::player::{PlayerCreate, PlayerSummary};
use network::Connection;
use protocol::world::{TPacketGCChannel, TPacketGCTime};
use protocol::world::TPacketCGMarkLogin;
use protocol::{
    header, phase, TPacketCGChangeName, TPacketCGEmpire, TPacketCGLogin3, TPacketCGPlayerCreate,
    TPacketCGPlayerDelete, TPacketCGPlayerSelect, TPacketGCDestroyCharacterSuccess,
    TPacketGCChangeName, TPacketGCCreateFailure, TPacketGCEmpire, TPacketGCLoginFailure,
    TPacketGCLoginSuccess, TPacketGCPhase, TPacketGCPlayerCreateSuccess, PLAYER_PER_ACCOUNT,
};
use game_core::ecs::{Intent, PlayerJoin, QuestIntent};
use game_core::packets;
use game_core::world::WorldStore;
use tokio::net::TcpStream;

use crate::auth::{is_valid_login_string, normalize_login};
use crate::channel::session::{ChannelLoginGuard, LeaveGuard, Session};
use crate::channel::{equipped_armor, now32, parse_listen};

/// Fases 1-7 de la conexión del canal (parity `input_login.cpp` +
/// `input_db.cpp` + `desc.cpp`): el flujo completo hasta que el jugador
/// entra al MUNDO COMPARTIDO. Al volver, la sesión está LLENA (row/store/
/// motion/leave seteados) y el game loop puede correr.
///
/// Retornos tempranos `Ok(())` = cierre limpio del protocolo (guild mark,
/// login rechazado, slot vacío) — el mismo comportamiento que tenía el
/// cuerpo único de `connection_inner`.
pub async fn run(session: &mut Session) -> Result<(), String> {
    // 1. GC_PHASE(LOGIN) DIRECTO — SIN handshake del canal (SOLUCIÓN
    //    DEFINITIVA 2026-08-14 del handshake silencioso intermitente):
    //    el cliente legacy conecta con Connect() crudo tras el auth
    //    (KEEP_ACCOUNT_CONNETION_ENABLE=1, AccountConnector.cpp:468-472) y
    //    manda su LOGIN3 al procesar la fase Login — ya en ella o al recibir
    //    este paquete (PythonNetworkStreamPhaseLogin.cpp:85-138). El AUTH
    //    MANTIENE su handshake (siempre funciona); el canal ya no lo tiene:
    //    la carrera ("el cliente conecta y no envía nada — 32 intentos +
    //    45 s de leniencia → cierre") no puede ocurrir. Divergencia
    //    deliberada documentada: el C++ del canal handshakeaba
    //    (`desc.cpp:258`); el rewrite entra directo — el reloj del cliente
    //    queda alineado con el AUTH (misma base Instant al arrancar ambos) y
    //    el canal manda GC_TIME al entrar al mundo (ADR-0010/0011).
    session
        .send(&TPacketGCPhase::new(phase::LOGIN).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_PHASE(LOGIN): {e}"))?;
    eprintln!("server_realms: channel conn {}: enviado GC_PHASE(LOGIN)", session.conn_id);

    // 2. Primer paquete: el LOGIN3 del canal (65 B) o el CG_MARK_LOGIN
    //    (0x64, 9 B) de la conexión paralela del guild mark — sin handshake
    //    el mark manda su login directo (GuildMarkDownloader.cpp:213-229);
    //    el canal normal (`guild_mark_server` OFF) lo cierra sin responder
    //    (parity `input.cpp:560-572` — el cliente no lo interpreta como fallo).
    let login3 = match recv_login3(session).await? {
        Some(l) => l,
        None => return Ok(()), // guild mark: cierre limpio
    };
    login_flow(session, login3).await
}

/// Flujo de login tras el LOGIN3 — compartido por el camino NORMAL (handshake
/// OK → GC_PHASE(LOGIN) → LOGIN3) y el camino `LoginEarly` (el LOGIN3 llegó
/// antes del eco y se procesa directo, sin reenviar GC_PHASE(LOGIN)).
/// Validaciones de la fase 4 en adelante (parity `input_login.cpp:97-147` +
/// `db.cpp:244-365`): login → WorldStore → GC_EMPIRE + SELECT + 449 B →
/// select → PLAYER LOAD → ENTERGAME → join al mundo.
async fn login_flow(session: &mut Session, login3: TPacketCGLogin3) -> Result<(), String> {
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
    let acc = match AccountRepo::new(session.pool.clone()).login(&login, &passwd).await {
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
    // El resto del AccountLogin del canal: los handlers de la fase select
    // (create/delete/empire/change-name) trabajan contra el player_index y
    // la social_id de la cuenta (parity `TAccountTable` del desc C++).
    session.account_id = acc.id;
    session.social_id = acc.social_id.clone();
    session.account_empire = acc.empire;

    // 6. WorldStore (repos sobre el pool COMPARTIDO + el Batcher UNICO del
    //    canal - ya no un Batcher por jugador) + empire + paquete del select.
    //    El sanity y el replay del WAL ya ocurrieron en el arranque del canal.
    session.store = Some(WorldStore::new(session.pool.clone(), session.batcher.clone()));
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

    // 7. Select: CG_PLAYER_SELECT (2 B) → load → spawn best-effort. Los
    // handlers de la fase select (crear/borrar/imperio/renombrar — lane E)
    // se atienden AQUÍ sin cerrar la conexión (parity `input_login.cpp`
    // `Analyze` — el cliente legacy los manda en PhaseSelect).
    let select = loop {
        let pkt = session.recv_idle().await?;
        match select_kind(pkt[0]) {
            SelectKind::Keepalive => continue,
            SelectKind::Create => {
                select_create(session, &pkt).await?;
            }
            SelectKind::Delete => {
                select_delete(session, &pkt).await?;
            }
            SelectKind::Empire => {
                select_empire(session, &pkt).await?;
            }
            SelectKind::ChangeName => {
                select_change_name(session, &pkt).await?;
            }
            SelectKind::Select => {
                break TPacketCGPlayerSelect::from_bytes(&pkt)
                    .map_err(|e| format!("CG_PLAYER_SELECT: {e}"))?
            }
            SelectKind::Other => {
                return Err(format!(
                    "channel conn {}: header inesperado 0x{:02x} esperando el select",
                    session.conn_id, pkt[0]
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
    // P0-B (2026-08-14): validación de la posición del load — parity
    // `GetValidLocation` del C++ (sectree_manager). Sin esto, un row con
    // coords FUERA del mapa crashea el CLIENTE con 0xC0000374 al cargar el
    // mapa (probado 2×: 08-09 y hoy — reparado con UPDATE manual). Fuera de
    // límites → primera celda movible (fallback determinista); mapa no
    // cargable → fail-open (el movimiento también fail-opens). Leniente a
    // propósito: las celdas del pueblo son ATTR_BLOCK legítimas (wave 45) —
    // solo se corrige lo que rompería al cliente (out of bounds).
    if let Some((fx, fy)) = validated_load_position(session) {
        let (ox, oy) = (session.row().x, session.row().y);
        session.row_mut().x = fx;
        session.row_mut().y = fy;
        eprintln!(
            "server_realms: channel conn {}: posición ({ox},{oy}) inválida en el mapa {} — \
             fallback a la primera celda movible ({fx},{fy})",
            session.conn_id, session.row().map_index
        );
    }
    // F5.1: el estado de movimiento del jugador (posición del load). El ANCLA
    // del anti-speedhack = el reloj del server AHORA (parity
    // `DESC::m_dwClientTime` — el reloj del server en el handshake del canal,
    // desc.cpp:714): el gate de 7 s (input_main.cpp:1496) y el `iServerDelta`
    // (input_main.cpp:1501) se miden desde aquí; con el canal sin handshake
    // (2026-08-14) el reloj del cliente queda anclado al AUTH y el desfase de
    // arranque entre procesos queda dentro del umbral.
    session.motion = Some(game_core::movement::initial(session.row().x, session.row().y));
    session
        .motion_mut()
        .anchor_server_time = now32();
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
    session.next_exp = CommonRepo::new(session.pool.clone())
        .next_exp(session.row().level)
        .await
        .unwrap_or(0);
    // Inventario del jugador (F5.3): MUTABLE — el pickup (CG_ITEM_PICKUP)
    // busca el primer cell libre y añade el item recogido.
    session.inventory = ItemRepo::new(session.pool.clone())
        .load_by_owner(session.row().id)
        .await?;
    session.affects = AffectRepo::new(session.pool.clone()).load(session.row().id).await?;
    // Battle points (ComputeBattlePoints — char.cpp:2051-2152): el arma
    // equipada (daño value0/value1 → la ventana del cliente) + la armadura
    // (el iArmor). Se cachean en la sesión (los GC_POINTS de todos los caminos
    // los leen) y van en el POINTS del entry.
    let weapon_proto = super::equipped_weapon_proto(&session.pool, &session.inventory).await?;
    let armor_sum = super::equipped_armor(&session.inventory, &session.pool).await?;
    session.battle = packets::compute_battle_points(session.row(), weapon_proto.as_ref(), armor_sum);
    for pkt in entry_packets(
        session.row(),
        session.next_exp,
        &session.inventory,
        &session.affects,
        &session.battle,
    ) {
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
    let lands = LandRepo::new(session.pool.clone())
        .load_by_map(i64::from(session.row().map_index))
        .await?;
    if lands.is_empty() {
        eprintln!(
            "server_realms: channel conn {}: mapa {} sin lands — el C++ no manda el paquete (building.cpp:969)",
            session.conn_id, session.row().map_index
        );
    }
    // C27 (velocidad de botas): la velocidad computada del personaje — el
    // b_moving_speed del ADD/INFO viaja con la bota (parity
    // `GetLimitPoint(POINT_MOV_SPEED)` char.cpp:896 — ModifyPoints aplica
    // el APPLY_MOV_SPEED de la bota al equipar, item.cpp:718-735).
    let boots = super::equipped_boots_proto(&session.pool, &session.inventory).await?;
    let mov_speed = packets::mov_speed_for_boots(boots.as_ref());
    session.mov_speed = mov_speed;
    let mut enter = enter_packets(
        session.row(),
        session.empire,
        &lands,
        &packets::equipped_parts(session.row(), &session.inventory),
        super::equipped_arrow_index(&session.inventory)
            .map(|i| session.inventory[i].count as u32)
            .unwrap_or(0),
        mov_speed,
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
    // MESSENGER (bloque 2026-08-21): la lista de amigos al entrar al mundo
    // (parity input_login.cpp:639 — `MessengerManager::Login(ch->GetName())`
    // dentro de ENTERGAME → LoadList + SendList). Con 0 filas NO se envía
    // nada (parity messenger_manager.cpp:341-343). El C++ lo manda ANTES del
    // LandList; aquí va después del lote ENTERGAME (el cliente bufferiza —
    // mismo resultado visible).
    crate::channel::messenger::send_login_list(session).await?;

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
    let armor = equipped_armor(&session.inventory, &session.pool).await?;
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
            skill_group: session.row().skill_group,
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
    // F6 social (gap-lane-C): registro del peer de CHAT del jugador — el
    // broadcast (GC_CHAT en rango) y el whisper (destino por nombre) usan
    // este registro; se libera al cerrar la conexión (RAII — el guard vive
    // en la sesión). La posición inicial es la del entry; el MOVE la
    // sincroniza (movement.rs).
    session.chat_guard = Some(crate::channel::chat::register_peer(
        session.player_vid(),
        session.row().name.clone(),
        session.row().map_index,
        session.motion().x,
        session.motion().y,
        session.empire,
        session.chat_tx.clone(),
    ));
    // PARTY (lane 2026-08-16): registro del peer de party del jugador — las
    // invitaciones (por vid), los chequeos de imperio/nivel y el outbox de
    // mensajes del party (GC_PARTY_* / exp compartida); se libera al cerrar
    // la conexión (RAII — el guard vive en la sesión; el líder desconectado
    // disuelve la party, parity P2PQuit). La posición la sincroniza el MOVE
    // (movement.rs).
    session.party_guard = Some(crate::channel::party::register_session(
        session.player_vid(),
        session.row().id as u32,
        session.row().name.clone(),
        session.row().level,
        session.empire,
        session.row().map_index,
        session.motion().x,
        session.motion().y,
        crate::channel::party::hp_percent(session.row()),
        session.party_tx.clone(),
    ));
    eprintln!(
        "server_realms: channel conn {}: {} en el mundo compartido (mapa {}) — \
         los adds de los mobs visibles llegan por la cola",
        session.conn_id, session.row().name, session.row().map_index
    );

    // F5 quests (wiring 2026-08-13): el runtime de quests del jugador - las
    // filas persistidas (player.quest) alimentan el engine (flags +
    // {quest}.__status). Sin filas o con error: runtime vacio (fail-open -
    // las quests de chat siguen disponibles, sin estado previo).
    match database::quest::QuestRepo::new(session.pool.clone()).load(session.row().id).await {
        Ok(rows) => {
            let flags: Vec<game_core::quest::PersistedFlag> = rows
                .into_iter()
                .map(|r| game_core::quest::PersistedFlag {
                    quest: r.sz_name,
                    flag: r.sz_state,
                    value: i64::from(r.l_value),
                })
                .collect();
            session.intent(Intent::Quest(QuestIntent::Init {
                player_vid: session.player_vid(),
                rows: flags.clone(),
            }))?;
            eprintln!(
                "server_realms: channel conn {}: quest runtime init ({} flags)",
                session.conn_id,
                flags.len()
            );
        }
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: quest flags: {e} - sin runtime de quests",
                session.conn_id
            );
        }
    }
    // F1 (ADR-0009): push del locale del jugador AL CONECTAR — el canal ya
    // conoce la lengua (columna `lang` del QUERY_LOGIN — parity input_db.cpp:
    // 150-164). El bundle GC_LOCALE (140) chunked viaja al final del entry
    // (channel/locale.rs); fail-open: el AUTH ya sirvió el bundle.
    crate::channel::locale::send_player_locale(session, &acc.lang).await?;
    Ok(())
}

/// Lee el primer paquete del canal filtrando keepalives (0xfc/0xfe):
/// `Some(LOGIN3)` = conexión NORMAL (login); `None` = CG_MARK_LOGIN (0x64)
/// o CG_MARK_IDXLIST (0x68) de la conexión paralela del guild mark → cierre
/// limpio sin responder (parity `input.cpp:560-566`), o CG_STATE_CHECKER
/// (0xce) del chequeo de estado del canal → se RESPONDE con el estado
/// (parity `input.cpp:573-589` + `input_db.cpp:2433-2461`). El timeout lo
/// pone el recv_idle del config.
async fn recv_login3(session: &mut Session) -> Result<Option<TPacketCGLogin3>, String> {
    let pkt = loop {
        let pkt = session.recv_idle().await?;
        match pkt[0] {
            header::CG_TIME_SYNC | header::CG_PONG => continue, // keepalives (F1.4)
            header::CG_MARK_LOGIN => {
                let m = TPacketCGMarkLogin::from_bytes(&pkt)
                    .map_err(|e| format!("CG_MARK_LOGIN: {e}"))?;
                eprintln!(
                    "server_realms: channel conn {}: guild mark login (handle 0x{:08x}, \
                     random 0x{:08x}) — no mark server, cierre limpio (parity input.cpp:562-566)",
                    session.conn_id, m.handle, m.random_key
                );
                return Ok(None);
            }
            // FIX 2026-08-14 (canal sin handshake): el mark downloader recibe
            // GC_PHASE(LOGIN) directo (ya no hay GC_HANDSHAKE) y su primer
            // paquete es CG_MARK_IDXLIST (0x68) en vez del CG_MARK_LOGIN
            // (GuildMarkDownloader.cpp `__LoginState_RecvPhase` → TODO_RECV_MARK
            // → `__SendMarkIDXList`). Sin mark server → cierre limpio sin
            // responder (parity input.cpp:560-566 — el cliente no lo interpreta
            // como fallo; el mark es opcional).
            header::CG_MARK_IDXLIST => {
                eprintln!(
                    "server_realms: channel conn {}: guild mark index list (0x68, sin handshake) \
                     — no mark server, cierre limpio (parity input.cpp:562-566)",
                    session.conn_id
                );
                return Ok(None);
            }
            // Chequeo de estado del canal (ServerStateChecker.cpp:43-69 — el
            // cliente abre UNA conexión paralela al seleccionar el servidor y
            // manda 0xce como PRIMER paquete). Parity input.cpp:573-589 +
            // input_db.cpp:2433-2461: responder GC_RESPOND_CHANNELSTATUS
            // (0xd2): [bHeader][nSize:4][TChannelStatus port:2+status:1][0x01]
            // — el cliente hace Initialize()/Disconnect al recibirla.
            header::CG_STATE_CHECKER => {
                let resp = channel_status_packet(&session.config.listen, session.config.no_more_clients);
                session
                    .send(&resp)
                    .await
                    .map_err(|e| format!("respondiendo CG_STATE_CHECKER: {e}"))?;
                eprintln!(
                    "server_realms: channel conn {}: estado del canal respondido ({}) — cierre",
                    session.conn_id, session.config.listen
                );
                return Ok(None);
            }
            header::CG_LOGIN3 => break pkt,
            other => {
                return Err(format!(
                    "channel conn {}: header inesperado 0x{other:02x} esperando el LOGIN3",
                    session.conn_id
                ))
            }
        }
    };
    TPacketCGLogin3::from_bytes(&pkt).map(Some).map_err(|e| format!("LOGIN3: {e}"))
}

// ---------------------------------------------------------------------------
// Fase select (lane E — parity `input_login.cpp:203-245,460-571,806-830` +
// `input_db.cpp:188-340` + `ClientManagerPlayer.cpp:774-1130`): crear,
// borrar, elegir imperio y renombrar personaje. Contrato de los handlers:
// - `Err` → CIERRE de la conexión (parity de los `SetPhase(PHASE_CLOSE)`).
// - `Ok(())` → se responde (o no-op, parity) y el loop del select SIGUE
//   esperando el CG_PLAYER_SELECT — el cliente actualiza su ventana con la
//   respuesta recibida y reintenta el select.
// ---------------------------------------------------------------------------

/// Paquetes de la fase select (dispatch del loop): los headers de
/// crear/borrar/imperio/renombrar se ATIENDEN dentro del loop; solo `Other`
/// cierra la conexión ("header inesperado" — parity `input_login.cpp:1124`
/// "login phase does not handle this packet").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectKind {
    /// CG_TIME_SYNC (0xfc) / CG_PONG (0xfe) — keepalives, se ignoran.
    Keepalive,
    /// CG_CHARACTER_CREATE (4).
    Create,
    /// CG_CHARACTER_DELETE (5).
    Delete,
    /// CG_EMPIRE (90).
    Empire,
    /// CG_CHANGE_NAME (106).
    ChangeName,
    /// CG_CHARACTER_SELECT (6) — el que rompe el loop.
    Select,
    /// Cualquier otro header → cierre.
    Other,
}

/// Dispatch del loop del select (función pura — testeable sin red).
fn select_kind(h: u8) -> SelectKind {
    match h {
        header::CG_TIME_SYNC | header::CG_PONG => SelectKind::Keepalive,
        header::CG_CHARACTER_CREATE => SelectKind::Create,
        header::CG_CHARACTER_DELETE => SelectKind::Delete,
        header::CG_EMPIRE => SelectKind::Empire,
        header::CG_CHANGE_NAME => SelectKind::ChangeName,
        header::CG_CHARACTER_SELECT => SelectKind::Select,
        _ => SelectKind::Other,
    }
}

/// `EMPIRE_MAX_NUM` (constants.h): el CG_EMPIRE valida `bEmpire < 4`
/// (parity `input_login.cpp:809-812`).
const EMPIRE_MAX_NUM: u8 = 4;
/// Mapa del create: 41 — el ÚNICO que sirve el canal (parity
/// `g_start_map[3]` del C++ — start_position.cpp:12-16).
const CREATE_MAP: i32 = 41;
/// Posición del create en UNITS — NO celdas (el cliente divide por 100;
/// CRITICAL AGENTS.md: coords inválidas crashean el cliente 0xC0000374 — el
/// P0-B del load valida contra el mapa igualmente). La aldea de Shinsoo
/// (parity `g_start_position[3]` = 969600/278400; el C++ usa
/// `CREATE_START_X/Y` con ±300 de jitter — el rewrite fija la aldea,
/// determinista).
const CREATE_X: i32 = 969600;
const CREATE_Y: i32 = 278400;

/// `CG_CHARACTER_CREATE` (4, 34 B) — parity `input_login.cpp:460-538` +
/// `ClientManagerPlayer.cpp:774-913` + `input_db.cpp:201-248`: validaciones
/// en el orden del C++, create en PG (INSERT + slot del índice) y respuesta
/// `TPacketGCPlayerCreateSuccess` (73 B — el cliente rellena el slot del
/// select con el TSimplePlayer, `__RecvPlayerCreateSuccessPacket`).
async fn select_create(session: &mut Session, pkt: &[u8]) -> Result<(), String> {
    let p = TPacketCGPlayerCreate::from_bytes(pkt)
        .map_err(|e| format!("CG_CHARACTER_CREATE: {e}"))?;
    let name = cstr(&p.name).to_string();
    eprintln!(
        "server_realms: channel conn {}: CG_CHARACTER_CREATE index={} name={name} job={} shape={}",
        session.conn_id, p.index, p.job, p.shape
    );
    // 1. Slot fuera de rango → cierre (parity @fixme190 input_login.cpp:472-476).
    if p.index >= PLAYER_PER_ACCOUNT as u8 {
        return Err(format!(
            "channel conn {}: create index overflow {} (parity PHASE_CLOSE)",
            session.conn_id, p.index
        ));
    }
    // 2. Nombre inválido o shape > 1 → failure bType=0 (parity
    //    input_login.cpp:488-492).
    if !check_name(&name) || p.shape > 1 {
        eprintln!(
            "server_realms: channel conn {}: create rechazado (nombre inválido o shape {})",
            session.conn_id, p.shape
        );
        send_create_failure(session, 0).await?;
        return Ok(());
    }
    // 3. Raza → job (parity RaceToJob input_login.cpp:356-380 + NewPlayerTable2
    //    :407-458): raza fuera de 0..7 → failure bType=0 (el C++ sin wolfman
    //    también la rechaza — MAIN_RACE_MAX_NUM=8).
    let Some(job) = race_to_job(p.job) else {
        eprintln!(
            "server_realms: channel conn {}: create rechazado (raza {} fuera de rango)",
            session.conn_id, p.job
        );
        send_create_failure(session, 0).await?;
        return Ok(());
    };
    // 4. Nombre == login de la cuenta → failure bType=1 (parity
    //    input_login.cpp:503-509 — strcmp del login contra el nombre).
    if session.account_login == name {
        eprintln!(
            "server_realms: channel conn {}: create rechazado (nombre igual al login)",
            session.conn_id
        );
        send_create_failure(session, 1).await?;
        return Ok(());
    }
    // 5. Slot ocupado → failure bType=1 (parity DG_PLAYER_CREATE_ALREADY).
    if session.store().account_slots(session.account_id).await?[p.index as usize].is_some() {
        eprintln!(
            "server_realms: channel conn {}: create rechazado (slot {} ocupado)",
            session.conn_id, p.index
        );
        send_create_failure(session, 1).await?;
        return Ok(());
    }
    // 6. Nombre ya existe → failure bType=1 (parity __QUERY_PLAYER_CREATE
    //    ClientManagerPlayer.cpp:812-829 — COUNT por nombre).
    if database::player::PlayerRepo::new(session.pool.clone())
        .name_exists(&name, 0)
        .await?
    {
        eprintln!(
            "server_realms: channel conn {}: create rechazado (nombre {name} ya existe)",
            session.conn_id
        );
        send_create_failure(session, 1).await?;
        return Ok(());
    }
    // 7. Create: stats iniciales (parity NewPlayerTable2 input_login.cpp:434-441
    //    + JobInitialPoints constants.cpp:18-21) y posición del mapa 41 en
    //    UNITS (ver CREATE_X/Y). El C++ persiste la RAZA en `job` (el campo
    //    `byJob` del TSimplePlayer = raza — el cliente pinta el modelo).
    let (st, ht, dx, iq, max_hp, max_sp, hp_per_ht, sp_per_iq, max_stamina) =
        job_initial_points(job);
    let create = PlayerCreate {
        account_id: session.account_id,
        name,
        level: 1,
        st,
        ht,
        dx,
        iq,
        job: p.job as i16,
        voice: 0,
        dir: 0,
        x: CREATE_X,
        y: CREATE_Y,
        z: 0,
        map_index: CREATE_MAP,
        hp: max_hp + i32::from(ht) * hp_per_ht,
        mp: max_sp + i32::from(iq) * sp_per_iq,
        random_hp: 0,
        random_sp: 0,
        stat_point: 0,
        stamina: max_stamina,
        part_base: i16::from(p.shape),
        part_main: i64::from(p.shape), // parity: el INSERT del C++ pone part_main = part_base
        part_hair: 0,
        gold: 0,
        playtime: 0,
        skill_level: Vec::new(),
        quickslot: Vec::new(),
    };
    let pid = match session.store().create_character(&create, p.index).await {
        Ok(pid) => pid,
        Err(e) => {
            // El C++ responde bType=1 en los fallos del INSERT/índice
            // (DG_PLAYER_CREATE_ALREADY en todos los caminos de error).
            eprintln!(
                "server_realms: channel conn {}: create falló en PG: {e}",
                session.conn_id
            );
            send_create_failure(session, 1).await?;
            return Ok(());
        }
    };
    // 8. Respuesta: TPacketGCPlayerCreateSuccess (73 B) con el TSimplePlayer
    //    (parity input_db.cpp:234-247 — los campos del DG success,
    //    ClientManagerPlayer.cpp:889-903; lAddr/wPort = el canal real, como
    //    en el 449 B del select).
    let (ip, port) = parse_listen(&session.config.listen)?;
    let server_ip = packets::ip_to_inet_addr(&ip)?;
    let summary = PlayerSummary {
        id: pid,
        name: create.name.clone(),
        job: create.job,
        level: 1,
        playtime: 0,
        st: create.st,
        ht: create.ht,
        dx: create.dx,
        iq: create.iq,
        part_main: create.part_main,
        part_hair: 0,
        x: create.x,
        y: create.y,
        skill_group: 0,
        change_name: 0,
    };
    let simple = packets::summary_to_simple_player(&summary, server_ip, port);
    session
        .send(&TPacketGCPlayerCreateSuccess::new(p.index, simple).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_CHARACTER_CREATE_SUCCESS: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: personaje {} creado (pid {pid}, slot {})",
        session.conn_id, create.name, p.index
    );
    Ok(())
}

/// `CG_CHARACTER_DELETE` (5, 10 B) — parity `input_login.cpp:539-571` +
/// `ClientManagerPlayer.cpp:950-1130` + `input_db.cpp:289-311`: slot vacío o
/// social_id incorrecta → GC_CHARACTER_DELETE_WRONG_SOCIAL_ID (0x0b, 1 B);
/// ok → borrado + GC_CHARACTER_DELETE_SUCCESS (0x0a) + account_index.
/// Divergencia documentada: sin límites de nivel del borrado (el C++ los
/// lee de conf.txt — PLAYER_DELETE_LEVEL_LIMIT[_LOWER]; con los defaults
/// 251/0 cualquier nivel es borrable — el rewrite no tiene el config).
async fn select_delete(session: &mut Session, pkt: &[u8]) -> Result<(), String> {
    let p = TPacketCGPlayerDelete::from_bytes(pkt)
        .map_err(|e| format!("CG_CHARACTER_DELETE: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: CG_CHARACTER_DELETE index={} private_code={}",
        session.conn_id,
        p.index,
        cstr(&p.private_code)
    );
    // Index overflow: el C++ solo loguea y sigue (input_login.cpp:552-556);
    // el rewrite responde WRONG_SOCIAL_ID para que el cliente no se quede
    // esperando (divergencia menor — un índice inválido nunca llega de un
    // cliente sano).
    if p.index >= PLAYER_PER_ACCOUNT as u8 {
        eprintln!(
            "server_realms: channel conn {}: delete index overflow {}",
            session.conn_id, p.index
        );
        send_delete_wrong_social_id(session).await?;
        return Ok(());
    }
    // Slot vacío → WRONG_SOCIAL_ID (parity input_login.cpp:558-564).
    let slots = session.store().account_slots(session.account_id).await?;
    let Some(pid) = slots[p.index as usize] else {
        eprintln!(
            "server_realms: channel conn {}: delete: slot {} vacío",
            session.conn_id, p.index
        );
        send_delete_wrong_social_id(session).await?;
        return Ok(());
    };
    // Confirmación: los ÚLTIMOS 7 chars de la social_id vs los primeros 7
    // del private_code (parity ClientManagerPlayer.cpp:972-977 — strncmp de
    // 7; un código más corto falla, como el strncmp con NUL).
    let social = session.social_id.as_bytes();
    let code = cstr(&p.private_code).as_bytes();
    if social.len() < 7 || code.get(..7) != Some(&social[social.len() - 7..]) {
        eprintln!(
            "server_realms: channel conn {}: delete: social_id no coincide (slot {})",
            session.conn_id, p.index
        );
        send_delete_wrong_social_id(session).await?;
        return Ok(());
    }
    // Borrado + respuesta (parity input_db.cpp:294-298: BufferedPacket(0x0a)
    // + account_index — el cliente los lee como UN paquete de 2 B).
    if let Err(e) = session
        .store()
        .delete_character(session.account_id, p.index, pid)
        .await
    {
        eprintln!(
            "server_realms: channel conn {}: delete: {e}",
            session.conn_id
        );
        send_delete_wrong_social_id(session).await?;
        return Ok(());
    }
    session
        .send(&TPacketGCDestroyCharacterSuccess::new(p.index).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_CHARACTER_DELETE_SUCCESS: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: personaje pid {pid} borrado (slot {})",
        session.conn_id, p.index
    );
    Ok(())
}

/// `CG_EMPIRE` (90, 2 B) — parity `input_login.cpp:806-830` +
/// `ClientManager.cpp:1129-1200` + `input_db.cpp:1250-1265`: valida el
/// imperio, lo persiste en `player_index`, reposiciona los personajes a la
/// aldea y responde GC_EMPIRE (2 B). El cliente legacy solo muestra la
/// ventana de imperio con bEmpire=0 (el login manda un imperio RANDOM —
/// `empire_byte`); el handler existe por parity con el C++.
async fn select_empire(session: &mut Session, pkt: &[u8]) -> Result<(), String> {
    let p = TPacketCGEmpire::from_bytes(pkt).map_err(|e| format!("CG_EMPIRE: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: CG_EMPIRE bEmpire={}",
        session.conn_id, p.b_empire
    );
    // bEmpire >= EMPIRE_MAX_NUM → cierre (parity input_login.cpp:809-812).
    if p.b_empire >= EMPIRE_MAX_NUM {
        return Err(format!(
            "channel conn {}: empire {} fuera de rango (parity PHASE_CLOSE)",
            session.conn_id, p.b_empire
        ));
    }
    // Cuenta con imperio Y personajes → cierre (parity input_login.cpp:814-823:
    // el imperio se elige ANTES de crear personajes; sin personajes se puede
    // re-elegir).
    if session.account_empire.is_some_and(|e| e > 0) {
        let slots = session.store().account_slots(session.account_id).await?;
        if slots.iter().any(Option::is_some) {
            return Err(format!(
                "channel conn {}: empire select fallido — la cuenta ya tiene \
                 imperio y personajes (parity input_login.cpp:816-822)",
                session.conn_id
            ));
        }
    }
    // Persiste el imperio + mueve los personajes a la aldea (WorldStore::
    // set_empire — mapa 41/UNITS 969600-278400, el único mapa del canal).
    if let Err(e) = session.store().set_empire(session.account_id, p.b_empire).await {
        // Fail-open: sin respuesta, el cliente reintenta (parity — el C++
        // tampoco responde si el db falla: no llega el DG_EMPIRE_SELECT).
        eprintln!(
            "server_realms: channel conn {}: empire select: {e}",
            session.conn_id
        );
        return Ok(());
    }
    session.account_empire = Some(i16::from(p.b_empire));
    session.empire = p.b_empire;
    session
        .send(&TPacketGCEmpire::new(p.b_empire).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_EMPIRE: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: imperio {} elegido",
        session.conn_id, p.b_empire
    );
    Ok(())
}

/// `CG_CHANGE_NAME` (106, 27 B) — parity `input_login.cpp:203-245` +
/// `ClientManager.cpp:548-588` + `input_db.cpp:313-340`: solo si el
/// personaje tiene el flag `bChangeName`; nombre inválido → failure bType=0;
/// nombre tomado → failure bType=1; ok → GC_CHANGE_NAME (107, 30 B).
async fn select_change_name(session: &mut Session, pkt: &[u8]) -> Result<(), String> {
    let p = TPacketCGChangeName::from_bytes(pkt)
        .map_err(|e| format!("CG_CHANGE_NAME: {e}"))?;
    let name = cstr(&p.name).to_string();
    eprintln!(
        "server_realms: channel conn {}: CG_CHANGE_NAME index={} name={name}",
        session.conn_id, p.index
    );
    // Index overflow → cierre (parity @fixme190 input_login.cpp:211-216).
    if p.index >= PLAYER_PER_ACCOUNT as u8 {
        return Err(format!(
            "channel conn {}: change_name index overflow {} (parity PHASE_CLOSE)",
            session.conn_id, p.index
        ));
    }
    // Slot sin personaje → cierre (parity input_login.cpp:218-223).
    let slots = session.store().account_slots(session.account_id).await?;
    let Some(pid) = slots[p.index as usize] else {
        return Err(format!(
            "channel conn {}: change_name player index not found (parity PHASE_CLOSE)",
            session.conn_id
        ));
    };
    // Sin flag bChangeName → no-op sin respuesta (parity input_login.cpp:228-229).
    let summaries = session.store().list_characters(session.account_id).await?;
    let Some(summary) = summaries.iter().find(|s| s.id == pid) else {
        return Ok(());
    };
    if summary.change_name == 0 {
        return Ok(());
    }
    // Nombre inválido → failure bType=0 (parity input_login.cpp:231-238).
    if !check_name(&name) {
        send_create_failure(session, 0).await?;
        return Ok(());
    }
    // Nombre tomado por OTRO personaje → failure bType=1 (parity
    // QUERY_CHANGE_NAME ClientManager.cpp:548-570 — `AND id <> pid`).
    if database::player::PlayerRepo::new(session.pool.clone())
        .name_exists(&name, pid)
        .await?
    {
        eprintln!(
            "server_realms: channel conn {}: change_name: nombre {name} ya existe",
            session.conn_id
        );
        send_create_failure(session, 1).await?;
        return Ok(());
    }
    // Renombre + respuesta GC_CHANGE_NAME (parity input_db.cpp:325-338 — el
    // cliente matchea el pid contra sus slots y actualiza el nombre).
    if let Err(e) = session.store().rename_character(pid, &name).await {
        eprintln!(
            "server_realms: channel conn {}: change_name: {e}",
            session.conn_id
        );
        send_create_failure(session, 0).await?;
        return Ok(());
    }
    session
        .send(&TPacketGCChangeName::new(pid as u32, &name).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_CHANGE_NAME: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: personaje pid {pid} renombrado a {name}",
        session.conn_id
    );
    Ok(())
}

/// `GC_CHARACTER_CREATE_FAILURE` (2 B) con log (parity `input_db.cpp:188-199`
/// — `PlayerCreateFailure`).
async fn send_create_failure(session: &mut Session, b_type: u8) -> Result<(), String> {
    eprintln!(
        "server_realms: channel conn {}: GC_CHARACTER_CREATE_FAILURE bType={b_type}",
        session.conn_id
    );
    session
        .send(&TPacketGCCreateFailure::new(b_type).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_CHARACTER_CREATE_FAILURE: {e}"))
}

/// `GC_CHARACTER_DELETE_WRONG_SOCIAL_ID` (1 B — parity `input_db.cpp:302-310`
/// `PlayerDeleteFail`; el cliente lo lee como `TPacketGCBlank` de 1 B).
async fn send_delete_wrong_social_id(session: &mut Session) -> Result<(), String> {
    eprintln!(
        "server_realms: channel conn {}: GC_CHARACTER_DELETE_WRONG_SOCIAL_ID",
        session.conn_id
    );
    session
        .send(&[header::GC_CHARACTER_DELETE_WRONG_SOCIAL_ID])
        .await
        .map_err(|e| format!("enviando GC_CHARACTER_DELETE_WRONG_SOCIAL_ID: {e}"))
}

/// `check_name` del rewrite (parity `check_name_alphabet` — locale spain,
/// `locale_service.cpp:311-326`): 2..=24 chars ASCII alfanuméricos.
/// Divergencia documentada: sin banword ni tabla de nombres de mob (el
/// rewrite no carga esos datos en runtime; el trigger `MakeCharacter` del PG
/// valida `^[A-Za-z0-9]+$` como red de seguridad).
fn check_name(name: &str) -> bool {
    (2..=protocol::CHARACTER_NAME_MAX_LEN).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric())
}

/// `RaceToJob` (input_login.cpp:356-380): la raza del cliente (0..7 — el
/// campo `job` del TPacketCGPlayerCreate) → índice del job (0..3) para los
/// stats iniciales. `None` = raza fuera de rango (8+ — el C++ sin wolfman
/// también la rechaza: MAIN_RACE_MAX_NUM=8 → NewPlayerTable2 false).
fn race_to_job(race: u16) -> Option<u16> {
    match race {
        0 | 4 => Some(0), // MAIN_RACE_WARRIOR_M/W → JOB_WARRIOR
        1 | 5 => Some(1), // MAIN_RACE_ASSASSIN_W/M → JOB_ASSASSIN
        2 | 6 => Some(2), // MAIN_RACE_SURA_M/W → JOB_SURA
        3 | 7 => Some(3), // MAIN_RACE_SHAMAN_W/M → JOB_SHAMAN
        _ => None,
    }
}

/// `JobInitialPoints` (constants.cpp:18-21) — `(st, ht, dx, iq, max_hp,
/// max_sp, hp_per_ht, sp_per_iq, max_stamina)`: los stats INICIALES del
/// create (parity `NewPlayerTable2` input_login.cpp:434-441 — hp = max_hp +
/// ht×hp_per_ht, mp = max_sp + iq×sp_per_iq, stamina = max_stamina).
fn job_initial_points(job: u16) -> (i16, i16, i16, i16, i32, i32, i32, i32, i16) {
    match job {
        0 => (6, 4, 3, 3, 600, 200, 40, 20, 800), // JOB_WARRIOR
        1 => (4, 3, 6, 3, 650, 200, 40, 20, 800), // JOB_ASSASSIN
        2 => (5, 3, 3, 5, 650, 200, 40, 20, 800), // JOB_SURA
        _ => (3, 4, 3, 6, 700, 200, 40, 20, 800), // JOB_SHAMAN (3 — el C++ valida antes)
    }
}

/// `GC_RESPOND_CHANNELSTATUS` (0xd2, parity `input_db.cpp:2433-2461`):
/// `[bHeader=0xd2][nSize: i32 LE][nPort: u16 LE][bStatus: u8][bSuccess=0x01]`.
/// El C++ responde con el estado CACHEADO del canal (input.cpp:573-589 +
/// desc_client.cpp:291-295: `g_bNoMoreClient → 0`, si no → por ocupación);
/// el Rust: 1 canal = este proceso — puerto del config, status 1
/// (recomendado/verde — `STATE_DICT[1]` del serverinfo.py) u 0 (offline) con
/// `no_more_clients`. El cliente matchea por puerto
/// (`ServerStateChecker::Update` — `channelStatus.nPort == it->uPort`).
fn channel_status_packet(listen: &str, no_more_clients: bool) -> Vec<u8> {
    let port = parse_listen(listen).map(|(_, p)| p).unwrap_or(0);
    let mut out = Vec::with_capacity(9);
    out.push(header::GC_RESPOND_CHANNELSTATUS); // 0xd2 = 210
    out.extend_from_slice(&1i32.to_le_bytes()); // nSize: 1 canal
    out.extend_from_slice(&port.to_le_bytes()); // TChannelStatus.nPort
    out.push(u8::from(!no_more_clients)); // TChannelStatus.bStatus: 1 = recomendado
    out.push(1u8); // bSuccess (parity input_db.cpp:2457 — el cliente lo ignora)
    out
}

/// Valida la posición cargada contra el mapa (P0-B 2026-08-14 — parity
/// `GetValidLocation` del C++): `None` = posición válida (o mapa no
/// cargable — fail-open, la posición cargada se mantiene); `Some((x, y))` =
/// fallback determinista a la primera celda movible del mapa (un row con
/// coords fuera del mapa crashea el CLIENTE con 0xC0000374 al cargar el
/// mapa — probado 2×: 08-09 y hoy). Leniente a propósito: celdas bloqueadas
/// in-bounds NO se corrigen (las celdas del pueblo son ATTR_BLOCK
/// legítimas — wave 45).
fn validated_load_position(session: &Session) -> Option<(i32, i32)> {
    let mut store = session.map_store.lock().unwrap();
    if let Err(e) = store.load(&session.config.map_path, session.row().map_index) {
        eprintln!(
            "server_realms: channel conn {}: walkability no disponible (mapa {}): {e} — \
             fail-open (la posición cargada se mantiene)",
            session.conn_id, session.row().map_index
        );
        return None;
    }
    let Some(map) = store.get(session.row().map_index) else {
        return None; // fail-open defensivo
    };
    let (x, y) = (session.row().x, session.row().y);
    // GUARD de posición (2026-08-16): si la celda NO es movible (fuera de
    // límites O bloqueada/agua — no solo out-of-bounds), hacer fallback al
    // spawn del mapa. El cliente legacy carga el mapa desde SU pack
    // (maps.epk), que puede diferir del server_attr del runtime: una
    // posición "in-bounds pero rara" (ej. 987103,314720 en el mapa 41)
    // rompía LoadMap del cliente → PostQuitMessage(0) → "se cierra al
    // entrar" (0xc0000374 / diálogo VC++ Runtime eran SÍNTOMAS del mapa
    // no cargado, no el crash raíz). Parity: el C++ GetValidLocation
    // (sectree_manager.cpp:790-837) falla al spawn (EMPIRE_START) cuando
    // el árbol no encuentra la celda.
    if map.is_movable(x, y) {
        return None; // posición OK (movible) — mantener
    }
    match map.first_movable() {
        Some(f) => Some(f),
        None => {
            eprintln!(
                "server_realms: channel conn {}: mapa {} sin celdas movibles — \
                 la posición cargada se mantiene",
                session.conn_id, session.row().map_index
            );
            None
        }
    }
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
        if let Some(pid) = pid
            && let Some(s) = summaries.iter().find(|s| s.id == *pid) {
                players[i] = Some(s.clone());
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
    battle: &packets::BattlePoints,
) -> Vec<Vec<u8>> {
    let mut out = vec![
        TPacketGCPhase::new(phase::LOADING).to_bytes().to_vec(),
        packets::main_character(row).to_bytes().to_vec(),
    ];
    out.extend(packets::quickslot_packets(row.quickslot.as_ref()));
    out.push(packets::points_packet(row, next_exp, battle).to_bytes().to_vec());
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
/// `mov_speed` = la velocidad computada (C27 — `mov_speed_for_boots`).
fn enter_packets(
    row: &database::player::PlayerRow,
    empire: u8,
    lands: &[database::land::LandRow],
    parts: &[u32; 5],
    arrows: u32,
    mov_speed: u8,
) -> Vec<Vec<u8>> {
    let mut out = vec![
        packets::character_add(row, mov_speed).to_bytes().to_vec(),
        // El ADDITIONAL_INFO (136) NO lleva b_moving_speed (packet.h:
        // 1348-1368 — la velocidad con botas viaja en el ADD y el UPDATE).
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
        let pkts = entry_packets(&row, 300, &items, &affects, &packets::BattlePoints::default());
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
        assert_eq!(entry_packets(&row, 300, &[], &[], &packets::BattlePoints::default()).len(), 40);
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
        let pkts = enter_packets(&row, 3, &lands, &parts, 0, 100);
        assert_eq!(pkts.len(), 4, "ADD + INFO + GAME + LAND_LIST");
        assert_eq!(pkts[0].len(), TPacketGCCharacterAdd::SIZE);
        assert_eq!(pkts[0][0], header::GC_CHARACTER_ADD);
        assert_eq!(pkts[0][26], 100, "b_moving_speed sin botas (C27)");
        assert_eq!(pkts[1].len(), TPacketGCCharacterAdditionalInfo::SIZE);
        assert_eq!(pkts[1][0], header::GC_CHAR_ADDITIONAL_INFO);
        assert_eq!(pkts[2].len(), TPacketGCPhase::SIZE);
        assert_eq!(pkts[2][0], header::GC_PHASE);
        assert_eq!(pkts[2][1], phase::GAME, "parity input_login.cpp:616");
        assert_eq!(pkts[3][0], 130, "GC_LAND_LIST");
        assert_eq!(u16::from_le_bytes([pkts[3][1], pkts[3][2]]), 27, "3 + 1×24");
        // Sin lands -> 3 paquetes (el C++ no manda el paquete vacío).
        assert_eq!(enter_packets(&row, 3, &[], &parts, 0, 100).len(), 3);
        // Con botas (+10%): el ADD del enter lleva 110 (C27); el
        // ADDITIONAL_INFO (136) NO lleva speed (packet.h:1348-1368 — solo
        // ADD y UPDATE tienen b_moving_speed).
        let pkts = enter_packets(&row, 3, &[], &parts, 0, 110);
        assert_eq!(pkts[0][26], 110, "b_moving_speed@26 del ADD (C27 botas +10)");
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

    /// Dispatch del loop del select (lane E): los headers de
    /// crear/borrar/imperio/renombrar NUNCA caen en `Other` (el cierre por
    /// "header inesperado") — se atienden y el loop sigue esperando el
    /// CG_PLAYER_SELECT.
    #[test]
    fn select_kind_accepts_lane_e_headers() {
        assert_eq!(select_kind(header::CG_CHARACTER_CREATE), SelectKind::Create);
        assert_eq!(select_kind(header::CG_CHARACTER_DELETE), SelectKind::Delete);
        assert_eq!(select_kind(header::CG_EMPIRE), SelectKind::Empire);
        assert_eq!(select_kind(header::CG_CHANGE_NAME), SelectKind::ChangeName);
        assert_eq!(select_kind(header::CG_CHARACTER_SELECT), SelectKind::Select);
        assert_eq!(select_kind(header::CG_TIME_SYNC), SelectKind::Keepalive);
        assert_eq!(select_kind(header::CG_PONG), SelectKind::Keepalive);
        assert_eq!(select_kind(0x7f), SelectKind::Other, "solo headers desconocidos cierran");
    }

    /// `check_name` del rewrite (parity `check_name_alphabet` + trigger
    /// `MakeCharacter`): 2..=24 chars ASCII alfanuméricos.
    #[test]
    fn check_name_validation() {
        assert!(check_name("Warrior"), "alfanumérico");
        assert!(check_name("a1B2"), "mezcla");
        assert!(check_name(&"x".repeat(24)), "máximo 24");
        assert!(!check_name(""), "vacío");
        assert!(!check_name("a"), "menor a 2 (parity strlen < 2)");
        assert!(!check_name(&"x".repeat(25)), "mayor a 24 (el buffer es [25])");
        assert!(!check_name("Warrior!"), "símbolo");
        assert!(!check_name("War rior"), "espacio");
        assert!(!check_name("ñandu"), "no-ASCII (parity isalpha/isdigit)");
    }

    /// `RaceToJob` (input_login.cpp:356-380): raza 0..7 → job 0..3;
    /// 8+ → None (el C++ sin wolfman la rechaza igual).
    #[test]
    fn race_to_job_parity() {
        assert_eq!(race_to_job(0), Some(0), "WARRIOR_M");
        assert_eq!(race_to_job(4), Some(0), "WARRIOR_W");
        assert_eq!(race_to_job(1), Some(1), "ASSASSIN_W");
        assert_eq!(race_to_job(5), Some(1), "ASSASSIN_M");
        assert_eq!(race_to_job(2), Some(2), "SURA_M");
        assert_eq!(race_to_job(6), Some(2), "SURA_W");
        assert_eq!(race_to_job(3), Some(3), "SHAMAN_W");
        assert_eq!(race_to_job(7), Some(3), "SHAMAN_M");
        assert_eq!(race_to_job(8), None, "wolfman/out-of-range");
        assert_eq!(race_to_job(255), None);
    }

    /// Stats iniciales del create (parity JobInitialPoints constants.cpp:18-21
    /// + NewPlayerTable2 input_login.cpp:434-441): hp = max_hp + ht×hp_per_ht,
    /// mp = max_sp + iq×sp_per_iq, stamina = max_stamina.
    #[test]
    fn job_initial_points_parity() {
        // JOB_WARRIOR: st6 ht4 dx3 iq3, 600/200, 40/20, 800.
        let (st, ht, dx, iq, max_hp, max_sp, hph, sph, stamina) = job_initial_points(0);
        assert_eq!((st, ht, dx, iq), (6, 4, 3, 3));
        assert_eq!(max_hp + i32::from(ht) * hph, 760, "hp del warrior (600 + 4×40)");
        assert_eq!(max_sp + i32::from(iq) * sph, 260, "mp del warrior (200 + 3×20)");
        assert_eq!(stamina, 800, "stamina inicial (max_stamina)");
        // JOB_ASSASSIN.
        let (st, ht, dx, iq, max_hp, max_sp, hph, sph, _) = job_initial_points(1);
        assert_eq!((st, ht, dx, iq), (4, 3, 6, 3));
        assert_eq!(max_hp + i32::from(ht) * hph, 770, "hp del assassin (650 + 3×40)");
        assert_eq!(max_sp + i32::from(iq) * sph, 260, "mp del assassin (200 + 3×20)");
        // JOB_SURA.
        let (st, ht, dx, iq, max_hp, _, hph, _, _) = job_initial_points(2);
        assert_eq!((st, ht, dx, iq), (5, 3, 3, 5));
        assert_eq!(max_hp + i32::from(ht) * hph, 770, "hp del sura (650 + 3×40)");
        // JOB_SHAMAN (default del match).
        let (st, ht, dx, iq, max_hp, _, hph, _, _) = job_initial_points(3);
        assert_eq!((st, ht, dx, iq), (3, 4, 3, 6));
        assert_eq!(max_hp + i32::from(ht) * hph, 860, "hp del shaman (700 + 4×40)");
    }

    /// Estado del canal (GC_RESPOND_CHANNELSTATUS 0xd2 — parity
    /// input_db.cpp:2433-2461): [0xd2][nSize=1 LE i32][port LE u16][status
    /// u8][bSuccess 0x01] = 9 B. El cliente matchea por puerto
    /// (ServerStateChecker::Update — channelStatus.nPort == uPort).
    #[test]
    fn channel_status_packet_wire() {
        let pkt = channel_status_packet("127.0.0.1:30003", false);
        assert_eq!(pkt.len(), 9, "1 + 4 + 2 + 1 + 1");
        assert_eq!(pkt[0], header::GC_RESPOND_CHANNELSTATUS, "0xd2");
        assert_eq!(i32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]), 1, "nSize 1");
        assert_eq!(u16::from_le_bytes([pkt[5], pkt[6]]), 30003, "puerto del canal");
        assert_eq!(pkt[7], 1, "status recomendado (STATE_DICT[1])");
        assert_eq!(pkt[8], 1, "bSuccess (parity — el cliente lo ignora)");
        // no_more_clients → status 0 (offline, parity desc_client.cpp:294-295).
        let off = channel_status_packet("127.0.0.1:30003", true);
        assert_eq!(off[7], 0, "status offline");
        // Listen inválido → puerto 0 (defensivo, nunca en runtime).
        assert_eq!(channel_status_packet("", false)[5..7], [0, 0]);
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
