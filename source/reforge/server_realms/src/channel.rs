//! F4 slice 2 — rol `channel`: el canal Rust sirviendo el flujo real
//! login→select (y spawn best-effort) contra PostgreSQL DIRECTO (los repos
//! son PG-native — sin proxy, ADR-0008).
//!
//! Paridad con el canal C++ (`input_login.cpp` + `input_db.cpp` + `desc.cpp`):
//!
//! 1. Handshake server-side (`network::handshake` — igual que el auth):
//!    `GC_PHASE(HANDSHAKE)` + `GC_HANDSHAKE` → eco `CG_HANDSHAKE`.
//! 2. `GC_PHASE(LOGIN)` — parity `input.cpp:194-196` (`g_bAuthServer ? PHASE_AUTH
//!    : PHASE_LOGIN`): el cliente entra en `SetLoginPhase` y manda el LOGIN3 del
//!    canal (`PythonNetworkStream.cpp:597-599` → 65 B, sin lang).
//! 3. `CG_LOGIN3` (65 B — el framer con rol Channel ya lo entrega así).
//! 4. Validaciones en orden (parity `input_login.cpp:97-147` + `db.cpp:244-365`):
//!    login inválido → `GC_LOGIN_FAILURE "NOID"`; `no_more_clients` →
//!    `"SHUTDOWN"`; ya logueado → `"ALREADY"`; credenciales vs PG →
//!    `AccountRepo::login` (13 columnas, hash MySQL en Rust); status != "OK" →
//!    `GC_LOGIN_FAILURE(status)`.
//! 5. `GC_EMPIRE` (0x5a, 2 B — empire de la cuenta; 0 → random 1..3, parity
//!    `input_db.cpp:167-180` GetServerLocation) + `GC_PHASE(SELECT)` +
//!    `GC_LOGIN_SUCCESS_NEWSLOT` (0x20, 449 B — `realm::packets::login_success`
//!    con los 5 slots de `WorldStore::account_slots`; handle = conn_id,
//!    random_key = rand32 — parity `desc.cpp:955-988`).
//! 6. `CG_PLAYER_SELECT` (2 B) → `WorldStore::select_player` (índice → Q2 load)
//!    → **spawn best-effort**: `GC_PHASE(LOADING)` + `TPacketGCCharacterAdd`
//!    (37 B) + `TPacketGCCharacterAdditionalInfo` (70 B) — los GAPs del spawn
//!    completo están documentados en `realm::packets` (mapa/sectree,
//!    affects→flags, items→parts, speeds, PointsPacket/SkillLevelPacket, SDB).
//!    Si el cliente no completa la entrada al mapa, el SELECT es el hito.
//!
//! Divergencias deliberadas (documentadas):
//! - DB caída en el LOGIN3 → `GC_LOGIN_FAILURE "NOTAVAIL"` (el C++ con el db
//!   caído no responde nada — la query se pierde; el Rust degrada a un status
//!   válido del protocolo, determinista; mismo espíritu que el auth bResult=0).
//! - `WorldStore` por conexión (sanity + Batcher por login): el pool y el
//!   Batcher compartido se deciden con el pipeline WAL (ADR-0008).
//! - `test_server` gate (`input_login.cpp:108-114`, "VERSION"): el Rust no lo
//!   aplica (el runtime real del C++ lo tiene desactivado).
//! - `last_play`/`g_iUserLimit` (FULL) del canal C++: no implementados
//!   (analytics / config no expuesto — YAGNI, se reportan como GAP).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use database::account::AccountRepo;
use database::affect::AffectRepo;
use database::common::CommonRepo;
use database::item::ItemRepo;
use database::land::LandRepo;
use network::framer::{ConnectionRole, Framer};
use network::handshake::HandshakeError;
use network::{handshake, Connection};
use protocol::world::{
    TPacketGCChannel, TPacketGCItemGroundAdd, TPacketGCItemGroundDel, TPacketGCItemOwnership,
    TPacketGCItemSet, TPacketGCTime, TItemPos,
};
use protocol::{
    header, phase, TPacketCGLogin3, TPacketCGPlayerSelect, TPacketGCEmpire, TPacketGCLoginFailure,
    TPacketGCLoginSuccess, TPacketGCPhase, PLAYER_PER_ACCOUNT,
};
use realm::packets;
use realm::world::WorldStore;
use tokio::net::{TcpListener, TcpStream};

use crate::auth::{is_valid_login_string, normalize_login};
use crate::config::Config;

/// Servidor channel: listener + tarea por conexión (patrón del auth).
pub async fn run(config: Config) -> std::io::Result<()> {
    let listener = TcpListener::bind(&config.listen).await?;
    // El puerto REAL del listener (relevante con `listen = "...:0"` en tests):
    // viaja en el `listen` del config clonado — el 449 B lo usa para el
    // `wPort` del DirectEnter (el cliente conecta a lAddr:wPort).
    let actual = listener.local_addr()?;
    println!("server_realms: channel escuchando en {actual}");
    let mut config = config;
    config.listen = actual.to_string();
    // Caché de mob_proto COMPARTIDA entre conexiones (F5 perf): la
    // resolución de spawns hace UNA query batch por los vnums que falten;
    // la caché evita el stall de minutos (10k conexiones por entrada).
    let spawn_cache = std::sync::Arc::new(tokio::sync::Mutex::new(realm::npc::MobCache::new()));
    let mut conn_id: u32 = 1;
    loop {
        let (stream, _peer) = listener.accept().await?;
        let cfg = config.clone();
        let id = conn_id;
        let cache = std::sync::Arc::clone(&spawn_cache);
        conn_id = conn_id.wrapping_add(1);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, cfg, id, cache).await {
                eprintln!("server_realms: channel conn {id}: {e}");
            }
        });
    }
}

/// Conexión channel — timeout de INACTIVIDAD (NO absoluto).
///
/// El C++ core no tiene timeout global: las sesiones viven hasta el
/// disconnect (la liveness la dan los keepalives 0xfc/0xfe del cliente).
/// El propósito original (deuda F1.5) era matar conexiones SILENCIOSAS — un
/// timeout ABSOLUTO de 15s mataba al cliente jugando (MOVE continuos — slice
/// 3.8). Cada lectura lleva su propio `config.timeout`: se resetea con
/// CUALQUIER paquete recibido (incluidos los ignorados de juego y los
/// keepalives) y solo dispara si no llega NADA durante `timeout_ms`.
async fn handle_connection(
    stream: TcpStream,
    config: Config,
    conn_id: u32,
    spawn_cache: std::sync::Arc<tokio::sync::Mutex<realm::npc::MobCache>>,
) -> Result<(), String> {
    connection_inner(stream, &config, conn_id, spawn_cache).await
}

/// Lee el siguiente paquete con timeout de inactividad: si no llega NADA en
/// `timeout`, la conexión se cierra (el paquete que llega resetea el timer —
/// el timeout se crea por lectura). El handshake (antes de este helper) tiene
/// sus propios retries internos (F1.5 — una conexión muda muere en ellos).
async fn recv_packet_idle<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    conn: &mut Connection<S>,
    framer: &mut Framer,
    timeout: std::time::Duration,
) -> Result<Vec<u8>, String> {
    tokio::time::timeout(timeout, framer.next_packet(conn))
        .await
        .map_err(|_| {
            format!(
                "timeout de inactividad de {} ms — sin paquetes del cliente, conexión cerrada",
                timeout.as_millis()
            )
        })?
        .map_err(|e| format!("framer: {e}"))
}

async fn connection_inner(
    stream: TcpStream,
    config: &Config,
    conn_id: u32,
    spawn_cache: std::sync::Arc<tokio::sync::Mutex<realm::npc::MobCache>>,
) -> Result<(), String> {
    let mut conn = Connection::new(stream);
    let mut framer = Framer::new(ConnectionRole::Channel);

    // 1. Handshake server-side (F1.5, validado contra el canal real en F1.6).
    //    El cliente del GUILD MARK abre una conexión SEPARADA en paralelo al
    //    select y responde al handshake con CG_MARK_LOGIN (0x64) en vez del
    //    eco (`GuildMarkDownloader.cpp:213-229`). El canal normal
    //    (`guild_mark_server` OFF — config del runtime) cierra esa conexión
    //    sin responder (`input.cpp:560-572`) — el cliente NO lo interpreta
    //    como fallo (el mark es opcional; el select sigue en la otra conexión).
    let hs = match handshake::perform(&mut conn, &mut framer, now_ms()).await {
        Err(HandshakeError::MarkLogin(p)) => {
            eprintln!(
                "server_realms: channel conn {conn_id}: guild mark login (handle 0x{:08x}, \
                 random 0x{:08x}) — no mark server, cierre limpio (parity input.cpp:562-566)",
                p.handle, p.random_key
            );
            return Ok(());
        }
        Err(e) => return Err(format!("handshake: {e}")),
        Ok(hs) => hs,
    };
    eprintln!("server_realms: channel conn {conn_id}: handshake OK (delta {} ms)", hs.delta);

    // 2. GC_PHASE(LOGIN) — el cliente responde con el LOGIN3 del canal (65 B).
    conn.send(&TPacketGCPhase::new(phase::LOGIN).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_PHASE(LOGIN): {e}"))?;
    eprintln!("server_realms: channel conn {conn_id}: enviado GC_PHASE(LOGIN)");

    // 3. LOGIN3 (65 B al canal — framer rol Channel).
    let login3 = loop {
        let pkt = recv_packet_idle(&mut conn, &mut framer, config.timeout).await?;
        match pkt[0] {
            header::CG_TIME_SYNC | header::CG_PONG => continue, // keepalives (F1.4)
            header::CG_LOGIN3 => {
                break TPacketCGLogin3::from_bytes(&pkt).map_err(|e| format!("LOGIN3: {e}"))?
            }
            other => {
                return Err(format!(
                    "channel conn {conn_id}: header inesperado 0x{other:02x} tras el handshake"
                ))
            }
        }
    };
    let login = normalize_login(&login3.login);
    let passwd = cstr(&login3.passwd).to_string();
    eprintln!("server_realms: channel conn {conn_id}: LOGIN3 login={login}");

    // 4. Validaciones (parity input_login.cpp:97-147 + db.cpp:244-365).
    if !is_valid_login_string(&login) {
        send_login_failure(&mut conn, "NOID").await?;
        return Ok(());
    }
    if config.no_more_clients {
        send_login_failure(&mut conn, "SHUTDOWN").await?;
        return Ok(());
    }
    let Some(_guard) = ChannelLoginGuard::acquire(&login) else {
        send_login_failure(&mut conn, "ALREADY").await?;
        return Ok(());
    };

    // 5. Credenciales vs PG (QUERY_LOGIN — 13 columnas; el canal C++ hace
    //    GD_LOGIN → db → RESULT_LOGIN, `db.cpp:244-365`).
    let acc = match AccountRepo::new(&config.pg_conn).login(&login, &passwd).await {
        Ok(Some(acc)) => acc,
        Ok(None) => {
            eprintln!("server_realms: channel conn {conn_id}: NOID {login}");
            send_login_failure(&mut conn, "NOID").await?;
            return Ok(());
        }
        Err(e) => {
            // Divergencia documentada: DB caída -> NOTAVAIL (determinista).
            eprintln!("server_realms: channel conn {conn_id}: PG falló para {login}: {e} — NOTAVAIL");
            send_login_failure(&mut conn, "NOTAVAIL").await?;
            return Ok(());
        }
    };
    if acc.status != "OK" {
        eprintln!("server_realms: channel conn {conn_id}: status '{}' para {login}", acc.status);
        send_login_failure(&mut conn, &acc.status).await?;
        return Ok(());
    }
    eprintln!(
        "server_realms: channel conn {conn_id}: login OK {login} (id {}, empire {:?})",
        acc.id, acc.empire
    );

    // 6. WorldStore (repos + Batcher) + empire + paquete del select.
    let store = match WorldStore::new(&config.pg_conn).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("server_realms: channel conn {conn_id}: WorldStore: {e} — NOTAVAIL");
            send_login_failure(&mut conn, "NOTAVAIL").await?;
            return Ok(());
        }
    };
    let empire = empire_byte(acc.empire);
    eprintln!("server_realms: channel conn {conn_id}: empire={empire}");

    // GC_EMPIRE (0x5a) + GC_PHASE(SELECT) + 449 B (parity input_db.cpp:169-183).
    conn.send(&TPacketGCEmpire::new(empire).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_EMPIRE: {e}"))?;
    conn.send(&TPacketGCPhase::new(phase::SELECT).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_PHASE(SELECT): {e}"))?;
    let success = build_login_success(&store, acc.id, conn_id, &config.listen).await?;
    let bytes = success.to_bytes();
    assert_eq!(bytes.len(), TPacketGCLoginSuccess::SIZE, "449 B (invariante wire)");
    conn.send(&bytes).await.map_err(|e| format!("enviando 449 B: {e}"))?;
    eprintln!("server_realms: channel conn {conn_id}: enviado GC_EMPIRE + GC_PHASE(SELECT) + 449 B");

    // 7. Select: CG_PLAYER_SELECT (2 B) → load → spawn best-effort.
    let select = loop {
        let pkt = recv_packet_idle(&mut conn, &mut framer, config.timeout).await?;
        match pkt[0] {
            header::CG_TIME_SYNC | header::CG_PONG => continue,
            header::CG_CHARACTER_SELECT => {
                break TPacketCGPlayerSelect::from_bytes(&pkt)
                    .map_err(|e| format!("CG_PLAYER_SELECT: {e}"))?
            }
            other => {
                return Err(format!(
                    "channel conn {conn_id}: header inesperado 0x{other:02x} esperando el select"
                ))
            }
        }
    };
    eprintln!("server_realms: channel conn {conn_id}: CG_PLAYER_SELECT index={}", select.index);

    let Some(mut row) = store.select_player(acc.id, select.index).await? else {
        // Parity input_login.cpp:266-271 ("player index not found" -> CLOSE).
        eprintln!("server_realms: channel conn {conn_id}: slot vacío/inválido — cierre");
        return Ok(());
    };
    // F5.1: el estado de movimiento del jugador (posición del load).
    let mut motion = realm::movement::initial(row.x, row.y);
    eprintln!(
        "server_realms: channel conn {conn_id}: player_load {} id={} lvl={} x={} y={} map={}",
        row.name, row.id, row.level, row.x, row.y, row.map_index
    );

    // ------------------------------------------------------------------
    // PLAYER LOAD (parity input_db.cpp:428-459 + los DG_* asíncronos del db):
    // GC_PHASE(LOADING) -> MainCharacter (113) -> [SDB 153: NO — runtime sin
    // package] -> 36×QUICKSLOT_ADD (28 — SetQuickslot por slot,
    // char_quickslot.cpp:96-103) -> Points (16, con los MÁXIMOS + NEXT_EXP)
    // -> Skills (76) -> N×ITEM_SET (21, ItemLoad input_db.cpp:1453-1561) ->
    // M×AFFECT_ADD (126, AffectLoad input_db.cpp:1563-1583).
    // ------------------------------------------------------------------
    // F5.3: next_exp MUTABLE — el level-up del kill lo recalcula por nivel.
    let mut next_exp = CommonRepo::new(&config.pg_conn).next_exp(row.level).await.unwrap_or(0);
    // Inventario del jugador (F5.3): MUTABLE — el pickup (CG_ITEM_PICKUP)
    // busca el primer cell libre y añade el item recogido.
    let mut inventory = ItemRepo::new(&config.pg_conn).load_by_owner(row.id).await?;
    let affects = AffectRepo::new(&config.pg_conn).load(row.id).await?;
    for pkt in entry_packets(&row, next_exp, &inventory, &affects) {
        conn.send(&pkt).await.map_err(|e| format!("enviando entry: {e}"))?;
    }
    eprintln!(
        "server_realms: channel conn {conn_id}: entry enviado (LOADING + MAIN_CHARACTER + {} quickslots + \
         POINTS + SKILLS + {} items + {} affects) — esperando CG_ENTERGAME del cliente",
        packets::quickslot_packets(row.quickslot.as_ref()).len(),
        inventory.len(),
        affects.len()
    );

    // El cliente carga el mapa (Warp) y manda CG_ENTERGAME (10, 1 B) al
    // abrir la ventana del juego (game.py:206 SendEnterGamePacket). Antes
    // manda la VERSIÓN del cliente (0xf1, 67 B) — se ignora sin validar
    // (parity input.cpp:205-213).
    loop {
        let pkt = recv_packet_idle(&mut conn, &mut framer, config.timeout).await?;
        match pkt[0] {
            header::CG_TIME_SYNC | header::CG_PONG => continue,
            header::CG_CLIENT_VERSION2 => {
                let name_end = pkt[1..34].iter().position(|&b| b == 0).unwrap_or(33);
                let ts_end = pkt[34..67].iter().position(|&b| b == 0).unwrap_or(33);
                eprintln!(
                    "server_realms: channel conn {conn_id}: VERSION {} {} — ignorado sin validar \
                     (parity input.cpp:205-213)",
                    String::from_utf8_lossy(&pkt[1..1 + name_end]),
                    String::from_utf8_lossy(&pkt[34..34 + ts_end])
                );
                continue;
            }
            header::CG_ENTERGAME => break,
            other => {
                return Err(format!(
                    "channel conn {conn_id}: header inesperado 0x{other:02x} esperando CG_ENTERGAME"
                ))
            }
        }
    }
    eprintln!("server_realms: channel conn {conn_id}: CG_ENTERGAME recibido");

    // ------------------------------------------------------------------
    // ENTERGAME (parity input_login.cpp:611-656): ADD (1) + INFO (136) via
    // Show()/EncodeInsertPacket -> GC_PHASE(GAME) -> LandList (130) ->
    // GC_TIME (106, get_global_time) -> GC_CHANNEL (121, g_bChannel).
    // ------------------------------------------------------------------
    let lands = LandRepo::new(&config.pg_conn).load_by_map(i64::from(row.map_index)).await?;
    if lands.is_empty() {
        eprintln!(
            "server_realms: channel conn {conn_id}: mapa {} sin lands — el C++ no manda el paquete (building.cpp:969)",
            row.map_index
        );
    }
    let mut enter = enter_packets(&row, empire, &lands);
    // Cola de entrada (parity input_login.cpp:648-656): TIME + CHANNEL tras
    // el land list — el reloj del server (get_global_time) y el canal.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0);
    enter.push(TPacketGCTime::new(now).to_bytes().to_vec());
    enter.push(TPacketGCChannel::new(config.channel).to_bytes().to_vec());
    for pkt in enter {
        conn.send(&pkt).await.map_err(|e| format!("enviando enter: {e}"))?;
    }
    eprintln!(
        "server_realms: channel conn {conn_id}: ENTERGAME enviado (ADD + INFO + GC_PHASE(GAME) + {} lands \
         + GC_TIME + GC_CHANNEL {}) — el cliente está DENTRO del mapa",
        lands.len(),
        config.channel
    );

    // ------------------------------------------------------------------
    // F5.2: el SPAWN de los NPCs del mapa (tras la cola del ENTERGAME — el
    // orden del C++: los NPCs del mapa se insertan en el sectree y sus
    // add/addInfo llegan contiguos por mob — `realm::npc::entry_spawns`).
    // Los VIDs de los NPCs: rango alto (10000+) — no colisionan con los PCs
    // (ids bajos 1..5; parity AllocVID del C++).
    // ------------------------------------------------------------------
    let spawns = match realm::npc::load_map_spawns(row.map_index as u32, &config.map_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("server_realms: channel conn {conn_id}: spawns del mapa {}: {e} — mundo vacío", row.map_index);
            Vec::new()
        }
    };
    let mut live_npcs: std::collections::HashMap<u32, LiveNpc> = std::collections::HashMap::new();
    if !spawns.is_empty() {
        // Resolución con CACHÉ COMPARTIDA + UNA query batch por los vnums
        // que falten (F5 perf — realm::npc::MobCache): la resolución previa
        // (10k × load_by_vnum con conexión PG por llamada) stallaba la
        // entrada ~3-4 min. Los grupos ya vienen expandidos de
        // load_map_spawns (kind Mob/Anywhere — la guarda vive en el cache).
        let repo = database::npc::MobRepo::new(&config.pg_conn);
        let mobs = spawn_cache
            .lock()
            .await
            .resolve(&repo, &spawns)
            .await
            .unwrap_or_default();
        let vid_base = next_npc_vid();
        let npc_packets = realm::npc::entry_spawns(row.map_index as u32, &mobs, vid_base);
        for pkt in &npc_packets {
            conn.send(pkt).await.map_err(|e| format!("enviando spawn: {e}"))?;
        }
        // La lista de NPCs vivos: vid -> estado (el combate la consulta).
        let mut vid = vid_base;
        for (entry, mob) in &mobs {
            for _ in 0..entry.count {
                live_npcs.insert(
                    vid,
                    LiveNpc {
                        state: realm::combat::NpcState {
                            vid,
                            x: entry.x,
                            y: entry.y,
                            level: mob.level,
                            dx: mob.ht, // la "dx" del mob: columna ht del PG
                            ht: mob.ht,
                            wdef: mob.def,
                            battle_type: mob.battle_type as u8,
                            attack_range: mob.attack_range as u32,
                        },
                        vnum: mob.vnum,
                        max_hp: mob.max_hp as i32,
                        hp: mob.max_hp as i32,
                        exp: mob.exp,
                        gold_min: mob.gold_min,
                        gold_max: mob.gold_max,
                        drop_item: mob.drop_item,
                        move_speed: mob.move_speed,
                        aggro: false,
                        damage_min: mob.damage_min,
                        damage_max: mob.damage_max,
                        home_x: entry.x,
                        home_y: entry.y,
                        nomove: mob
                            .ai_flag
                            .as_deref()
                            .is_some_and(|f| f.contains("NOMOVE")),
                    },
                );
                vid += 1;
            }
        }
        eprintln!(
            "server_realms: channel conn {conn_id}: spawn {} mobs del mapa {} ({:?} paquetes)",
            live_npcs.len(),
            row.map_index,
            npc_packets.len()
        );
    }

    // F5.2: el estado de combate del jugador (cooldown por objetivo).
    let mut combat = realm::combat::CombatState::new();

    // F5.3: items EN EL SUELO del mundo (vid -> item). El pickup
    // (CG_ITEM_PICKUP) los consume; el `next_item_vid` global no colisiona
    // con los NPCs (10 000+) ni los PCs (ids bajos).
    let mut live_items: std::collections::HashMap<u32, LiveGroundItem> = std::collections::HashMap::new();

    // 8. Loop de juego (estático): el mundo no tiene NPCs/mobs todavía (F5) —
    //    la conexión se mantiene viva. El HEARTBEAT es del SERVIDOR (parity
    //    `ping_event`, desc.cpp:179-214): el cliente en reposo no manda nada;
    //    el canal envía GC_PING (44, 1 B) cada `ping_interval_ms` y el cliente
    //    responde CG_PONG (0xfe — ya en la tabla del framer), que resetea el
    //    timeout de inactividad. El ping es INDEPENDIENTE del tráfico entrante
    //    (tokio::select! — se envía incluso si llegan MOVE). Los paquetes de
    //    juego legítimos se ignoran con log (procesamiento F5); el cierre para
    //    headers desconocidos/variables lo hace el FRAMER (parity
    //    input.cpp:77-84) — este loop no lo relaja.
    let mut ping_timer = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_millis(config.ping_interval_ms),
        Duration::from_millis(config.ping_interval_ms),
    );
    // F5.3 (NPC AI): tick del mundo — mueve los mobs AGGRO hacia el jugador y
    // difunde su GC_MOVE. Intervalo fijo (500 ms — el paso usa el move_speed
    // del mob en UNITS/seg, realm::ai::step_toward).
    const AI_TICK_MS: u64 = 500;
    let mut ai_timer = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_millis(AI_TICK_MS),
        Duration::from_millis(AI_TICK_MS),
    );
    // Deadline de inactividad PERSISTENTE (basado en el último paquete
    // recibido): el select! cancela los brazos al ganar uno, así que el timer
    // del idle se recrea por iteración — pero con el MISMO deadline (derivado
    // de `last_packet`, que solo cambia al RECIBIR) → el ping del canal NO
    // resetea el idle; solo los paquetes del cliente lo hacen.
    let mut last_packet = tokio::time::Instant::now();
    loop {
        let idle_deadline = last_packet + config.timeout;
        let idle = tokio::time::sleep_until(idle_deadline);
        tokio::pin!(idle);
        tokio::select! {
            pkt = framer.next_packet(&mut conn) => {
                let pkt = pkt.map_err(|e| format!("framer (game): {e}"))?;
                last_packet = tokio::time::Instant::now();
                match pkt[0] {
                    header::CG_TIME_SYNC | header::CG_PONG | header::CG_MARK_LOGIN => continue,
                    header::CG_CLIENT_VERSION2 => {
                        // El cliente puede re-mandar la versión en la fase game
                        // (parity input.cpp:205-213 — sin validación, sin respuesta).
                        let name_end = pkt[1..34].iter().position(|&b| b == 0).unwrap_or(33);
                        eprintln!(
                            "server_realms: channel conn {conn_id}: VERSION {} (game) — ignorado",
                            String::from_utf8_lossy(&pkt[1..1 + name_end])
                        );
                        continue;
                    }
                    header::CG_MOVE => {
                        // F5.1: el movimiento del jugador. El cliente se mueve
                        // LOCALMENTE (sin ack — el server responde el
                        // GC_CHARACTER_MOVE solo a los observadores,
                        // input_main.cpp:1576-1588). La validación
                        // anti-speedhack: timer (input_main.cpp:1494-1516) +
                        // distancia (el umbral del TP_SPEED_CHECK).
                        match protocol::movement::TPacketCGMove::from_bytes(&pkt) {
                            Ok(mv) => {
                                match realm::movement::process_move(&mut motion, &mv, now32()) {
                                    Ok(r) => {
                                        eprintln!(
                                            "server_realms: channel conn {conn_id}: MOVE {} -> {},{} (func {})",
                                            row.name, r.x, r.y, mv.b_func
                                        );
                                    }
                                    Err(realm::movement::MoveError::NotMove) => {
                                        // ACCIÓN (ataque/skill/combo) — el
                                        // procesamiento es F5; se loguea.
                                        eprintln!(
                                            "server_realms: channel conn {conn_id}: MOVE func {} de {} — \
                                             acción pendiente de integración (F5)",
                                            mv.b_func, row.name
                                        );
                                    }
                                    Err(e @ (realm::movement::MoveError::SlowTimer
                                    | realm::movement::MoveError::FastTimer)) => {
                                        // Kick del C++ (DelayedDisconnect(3),
                                        // input_main.cpp:1505-1515) — el canal
                                        // cierra la conexión.
                                        eprintln!(
                                            "server_realms: channel conn {conn_id}: SPEEDHACK {} ({:?}) — \
                                             cierre (parity DelayedDisconnect)",
                                            row.name, e
                                        );
                                        return Err(format!("speedhack de {}", row.name));
                                    }
                                    Err(realm::movement::MoveError::TooFar) => {
                                        // Corrección del C++ (Show+Stop —
                                        // el define TP_SPEED_CHECK está
                                        // comentado, pero es el anti-teleport
                                        // estándar): se rechaza el MOVE, la
                                        // posición queda.
                                        eprintln!(
                                            "server_realms: channel conn {conn_id}: MOVE teleport de {} — \
                                             rechazado (posición {} ,{})",
                                            row.name, motion.x, motion.y
                                        );
                                    }
                                    Err(realm::movement::MoveError::InvalidFunc) => {
                                        eprintln!(
                                            "server_realms: channel conn {conn_id}: MOVE func inválido de {}",
                                            row.name
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "server_realms: channel conn {conn_id}: CG_MOVE malformado: {e}"
                                );
                            }
                        }
                        continue;
                    }
                    header::CG_ATTACK => {
                        // F5.2: el combate (realm::combat — el lane de
                        // combate). CG_ATTACK (8 B) -> bType>0 = skill
                        // (F5.2+: se ignora) -> si normal: cooldown/rango/
                        // daño contra el NPC objetivo -> paquetes del
                        // resultado (GcAttack + GcDamageInfo) + daño al
                        // HP del mob; hp <= 0 -> muerte (GC_DEAD +
                        // GC_CHARACTER_DEL — Packet.h:1349-1353/1296-1300)
                        // + se quita de la lista de NPCs vivos.
                        match protocol::combat::CgAttack::from_bytes(&pkt) {
                            Ok(attack) if attack.b_type != protocol::combat::CgAttack::TYPE_NORMAL => {
                                // Skills (bType > 0) — F5.2+ (el combat lane
                                // las rechaza con empty()).
                                eprintln!(
                                    "server_realms: channel conn {conn_id}: CG_ATTACK tipo {} de {} — \
                                     skills pendientes (F5.2+)",
                                    attack.b_type, row.name
                                );
                            }
                            Ok(attack) => {
                                let player = realm::combat::PlayerState::from_row(&row, &motion);
                                let target = live_npcs.get(&attack.victim_vid).map(|n| &n.state);
                                let result = realm::combat::handle_attack(
                                    &mut combat,
                                    &attack,
                                    &player,
                                    target,
                                    now_ms(),
                                    &mut |min, max| {
                                        // roll INCLUSIVE (parity number(min,max)).
                                        let span = max - min + 1;
                                        min + (rand32() % span as u32) as i32
                                    },
                                );
                                for pkt in &result.packets {
                                    conn.send(pkt)
                                        .await
                                        .map_err(|e| format!("enviando combate: {e}"))?;
                                }
                                if result.damage > 0 {
                                    if let Some(npc) = live_npcs.get_mut(&attack.victim_vid) {
                                        npc.hp -= result.damage;
                                        // F5.3 (AI): el mob se vuelve HOSTIL
                                        // al recibir daño — el tick de AI lo
                                        // perseguirá (parity: el C++ marca el
                                        // aggro en `OnDamage`/`Update`).
                                        npc.aggro = true;
                                        if npc.hp <= 0 {
                                            // Muerte del mob: GC_DEAD (14) +
                                            // GC_CHARACTER_DEL (2) — el
                                            // cliente reproduce la animación
                                            // y remueve.
                                            let dead = protocol::world::TPacketGCDead::new(attack.victim_vid);
                                            let del =
                                                protocol::world::TPacketGCCharacterDelete::new(attack.victim_vid);
                                            conn.send(&dead.to_bytes()).await
                                                .map_err(|e| format!("enviando GC_DEAD: {e}"))?;
                                            conn.send(&del.to_bytes()).await
                                                .map_err(|e| format!("enviando GC_CHARACTER_DEL: {e}"))?;
                                            // F5.3: recompensa del kill — exp
                                            // y gold del mob_proto con los
                                            // rates del config (realm::combat
                                            // — parity del C++, testeada).
                                            let reward = realm::combat::kill_reward(
                                                npc.exp,
                                                npc.gold_min,
                                                npc.gold_max,
                                                config.exp_rate,
                                                config.gold_rate,
                                                &mut |lo, hi| {
                                                    // roll INCLUSIVE (parity number(min,max)).
                                                    let span = (hi - lo + 1).max(1) as u32;
                                                    lo + (rand32() % span) as i32
                                                },
                                            );
                                            let (exp_gain, gold_gain) =
                                                (reward.exp_gain, reward.gold_gain);
                                            row.exp = row.exp.saturating_add(exp_gain.min(i32::MAX as i64) as i32);
                                            row.gold = row.gold.saturating_add(gold_gain.min(i32::MAX as i64) as i32);
                                            // Level-up (parity char.cpp
                                            // `GetNextExp` — exp_table por
                                            // nivel; el next_exp se recarga
                                            // de la DB al subir).
                                            let mut leveled = false;
                                            while next_exp > 0 && i64::from(row.exp) >= next_exp {
                                                row.exp = (i64::from(row.exp) - next_exp) as i32;
                                                row.level = row.level.saturating_add(1);
                                                leveled = true;
                                                next_exp = CommonRepo::new(&config.pg_conn)
                                                    .next_exp(row.level)
                                                    .await
                                                    .unwrap_or(0);
                                            }
                                            // GC_POINTS actualizado (el
                                            // cliente muestra exp/gold/nivel)
                                            // + persistencia durable.
                                            conn.send(&packets::points_packet(&row, next_exp).to_bytes())
                                                .await
                                                .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
                                            store.save_character(&row);
                                            // F5.3: DROP del mob — el drop
                                            // primario (`mob_proto.drop_item`),
                                            // con la probabilidad del
                                            // `drop_rate` del config. (El C++
                                            // además usa etc_drop_item.txt por
                                            // nombre — TRAP AGENTS.md §17 — el
                                            // subset base usa solo la columna.)
                                            if npc.drop_item > 0
                                                && (rand32() % 100) < u32::from(config.drop_rate)
                                            {
                                                let item_vid = next_item_vid();
                                                let gi = LiveGroundItem {
                                                    vnum: npc.drop_item as u32,
                                                    count: 1,
                                                    x: npc.state.x,
                                                    y: npc.state.y,
                                                    z: 0,
                                                };
                                                conn.send(
                                                    &TPacketGCItemGroundAdd::new(
                                                        item_vid, gi.vnum, gi.x, gi.y, gi.z, gi.count,
                                                    )
                                                    .to_bytes(),
                                                )
                                                .await
                                                .map_err(|e| format!("enviando GC_ITEM_GROUND_ADD: {e}"))?;
                                                // Ownership (parity
                                                // item.cpp:145-162 — el nombre
                                                // del dueño sobre el item).
                                                conn.send(
                                                    &TPacketGCItemOwnership::new(
                                                        item_vid,
                                                        row.name.as_bytes(),
                                                    )
                                                    .to_bytes(),
                                                )
                                                .await
                                                .map_err(|e| format!("enviando GC_ITEM_OWNERSHIP: {e}"))?;
                                                live_items.insert(item_vid, gi);
                                                eprintln!(
                                                    "server_realms: channel conn {conn_id}: {} — drop item \
                                                     vnum {} (vid {}) en el suelo",
                                                    row.name, npc.drop_item, item_vid
                                                );
                                            }
                                            eprintln!(
                                                "server_realms: channel conn {conn_id}: {} mató al mob vnum {} \
                                                 (vid {}): exp +{exp_gain}, gold +{gold_gain}{} (nivel {})",
                                                row.name, npc.vnum, attack.victim_vid,
                                                if leveled { ", LEVEL UP" } else { "" }, row.level
                                            );
                                            live_npcs.remove(&attack.victim_vid);
                                        } else {
                                            eprintln!(
                                                "server_realms: channel conn {conn_id}: {} golpeó mob vnum {} ({}/{})",
                                                row.name, npc.vnum, npc.hp, npc.max_hp
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "server_realms: channel conn {conn_id}: CG_ATTACK malformado: {e}"
                                );
                            }
                        }
                        continue;
                    }
                    // F5.3: chat — echo GC_CHAT (4) al jugador (parity
                    // `Chat()` input_main.cpp:641-685 → `ChatPacket` →
                    // char.cpp — sin interpret_command por ahora, YAGNI).
                    // CG_CHAT (3): header + length(WORD) + type + msg (el
                    // framer ya entrega `length` bytes totales — el formato
                    // de TPacketCGChat Packet.h:534-539).
                    header::CG_CHAT => {
                        if pkt.len() < 4 {
                            return Err(format!("CG_CHAT malformado ({})", pkt.len()));
                        }
                        let chat_type = pkt[3];
                        let msg = &pkt[4..];
                        // GC_CHAT: header(4) + size(WORD, incluye header 9 B)
                        // + type + dwVID + bEmpire + msg (Packet.h:1336-1343;
                        // el cliente hace size - sizeof(TPacketGCChat)).
                        let size = (9 + msg.len()) as u16;
                        let mut out = Vec::with_capacity(9 + msg.len());
                        out.push(header::GC_CHAT);
                        out.extend_from_slice(&size.to_le_bytes());
                        out.push(chat_type);
                        out.extend_from_slice(&(row.id as u32).to_le_bytes());
                        out.push(empire);
                        out.extend_from_slice(msg);
                        conn.send(&out)
                            .await
                            .map_err(|e| format!("enviando GC_CHAT: {e}"))?;
                        eprintln!(
                            "server_realms: channel conn {conn_id}: chat de {} (type {}): {}",
                            row.name,
                            chat_type,
                            String::from_utf8_lossy(msg)
                        );
                        continue;
                    }
                    // F5.3: pickup de un item del suelo (parity
                    // `ItemPickup` input_main.cpp:902-907 → `PickupItem`
                    // char_item.cpp:5888-5947): distancia ≤ 600
                    // (`CItem::DistanceValid`, item.cpp:461-472) → primer
                    // slot libre del inventario (INVENTORY_MAX_NUM = 90,
                    // length.h:29) → GC_ITEM_SET (el item entra al
                    // inventario) + GC_ITEM_GROUND_DEL (se quita del suelo)
                    // + persistencia (ItemRepo::upsert).
                    header::CG_ITEM_PICKUP => {
                        if pkt.len() < 5 {
                            return Err(format!("CG_ITEM_PICKUP malformado ({})", pkt.len()));
                        }
                        let vid = u32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]);
                        let Some(gi) = live_items.get(&vid).copied() else {
                            eprintln!(
                                "server_realms: channel conn {conn_id}: pickup de vid {vid} — \
                                 no hay item en el suelo"
                            );
                            continue;
                        };
                        let dist = realm::combat::distance_approx(motion.x - gi.x, motion.y - gi.y);
                        if dist > 600 {
                            eprintln!(
                                "server_realms: channel conn {conn_id}: pickup de vid {vid} — \
                                 fuera de rango ({dist} > 600)"
                            );
                            continue;
                        }
                        // Primer cell libre del inventario (parity
                        // `GetEmptyInventory`, char_item.cpp:709-711).
                        let occupied: std::collections::HashSet<u16> = inventory
                            .iter()
                            .filter(|i| i.window == "INVENTORY")
                            .map(|i| i.pos as u16)
                            .collect();
                        let Some(slot) = (0u16..90).find(|c| !occupied.contains(c)) else {
                            eprintln!(
                                "server_realms: channel conn {conn_id}: inventario lleno — \
                                 el item {vid} queda en el suelo"
                            );
                            continue;
                        };
                        // Item nuevo del pickup: id del rango ITEM_ID_RANGE
                        // (100M-200M — parity `ItemIDRangeManager.cpp:93,121`;
                        // el E2E Q8 sondea ese rango).
                        let id = ItemRepo::new(&config.pg_conn)
                            .max_id_in_range(100_000_000, 200_000_000)
                            .await?
                            .map(|m| m + 1)
                            .unwrap_or(100_000_000);
                        let new_item = database::item::ItemRow {
                            id,
                            window: "INVENTORY".to_string(),
                            pos: slot as i32,
                            count: gi.count as i64,
                            vnum: gi.vnum as i64,
                            sockets: [0; 3],
                            attrs: [(0, 0); 7],
                        };
                        // GC_ITEM_SET (51 B — el slot pintado del cliente).
                        let set = TPacketGCItemSet {
                            header: TPacketGCItemSet::HEADER,
                            cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: slot },
                            vnum: gi.vnum,
                            count: gi.count as u8,
                            flags: 0,
                            anti_flags: 0,
                            highlight: 0,
                            sockets: [0; 3],
                            attrs: [(0, 0); 7],
                        };
                        conn.send(&set.to_bytes())
                            .await
                            .map_err(|e| format!("enviando GC_ITEM_SET: {e}"))?;
                        conn.send(&TPacketGCItemGroundDel::new(vid).to_bytes())
                            .await
                            .map_err(|e| format!("enviando GC_ITEM_GROUND_DEL: {e}"))?;
                        // Persistencia durable + estado del mundo.
                        ItemRepo::new(&config.pg_conn).upsert(&new_item, row.id).await?;
                        live_items.remove(&vid);
                        inventory.push(new_item);
                        eprintln!(
                            "server_realms: channel conn {conn_id}: {} recogió item vnum {} (vid {}) \
                             → slot {slot} del inventario (id {id})",
                            row.name, gi.vnum, vid
                        );
                        continue;
                    }
                    // F5.3: REVIVE del jugador — CG_SCRIPT_ANSWER (29, 2 B:
                    // header + answer BYTE — Packet.h:679). El diálogo de
                    // muerte del cliente manda la respuesta; el C++ revive
                    // con `RestartAtSamePos` (cmd_general.cpp:534 — el mismo
                    // punto) o warpea a la ciudad (cmd_general.cpp:552-554 →
                    // WarpSet EMPIRE_START).
                    header::CG_SCRIPT_ANSWER => {
                        if row.hp <= 0 {
                            let answer = pkt.get(1).copied().unwrap_or(0);
                            // Restaurar hp/mp a los máximos del subset
                            // (parity PointChange(POINT_HP, GetMaxHP()) —
                            // el revive del C++ restaura antes de mostrar).
                            let max = packets::compute_max_points(&row).unwrap_or([100, 100, 0]);
                            row.hp = max[0];
                            row.mp = max[1];
                            store.save_character(&row);
                            if answer == 1 {
                                // Revive EN LA CIUDAD: GC_WARP — el cliente
                                // cierra la conexión y RECONECTA con el flujo
                                // DirectEnter completo (RecvWarpPacket →
                                // Connect(lAddr, wPort) — F4 ya lo sirve).
                                // Destino: el punto de salida del personaje
                                // (exit_x/y — el C++ usa EMPIRE_START; el
                                // runtime actual: village del mapa 41).
                                let (wx, wy) = if row.exit_x > 0 && row.exit_y > 0 {
                                    (row.exit_x, row.exit_y)
                                } else {
                                    (969_600, 278_400) // village c1 mapa 41
                                };
                                let (ip, port) = parse_listen(&config.listen)?;
                                let addr = packets::ip_to_inet_addr(&ip)?;
                                conn.send(&protocol::world::TPacketGCWarp::new(wx, wy, addr, port).to_bytes())
                                    .await
                                    .map_err(|e| format!("enviando GC_WARP: {e}"))?;
                                eprintln!(
                                    "server_realms: channel conn {conn_id}: {} revivió EN LA CIUDAD \
                                     (answer {answer}) — GC_WARP {wx},{wy} → {}:{port}, reconexión",
                                    row.name, ip
                                );
                            } else {
                                // RestartAtSamePos: remove + insert del
                                // personaje (el cliente reinicia la instancia
                                // en su sitio).
                                let vid = row.id as u32;
                                conn.send(&protocol::world::TPacketGCCharacterDelete::new(vid).to_bytes())
                                    .await
                                    .map_err(|e| format!("enviando GC_CHARACTER_DEL: {e}"))?;
                                conn.send(&packets::character_add(&row).to_bytes().to_vec())
                                    .await
                                    .map_err(|e| format!("enviando GC_CHARACTER_ADD: {e}"))?;
                                conn.send(&packets::character_additional_info(&row, empire).to_bytes().to_vec())
                                    .await
                                    .map_err(|e| format!("enviando GC_CHARACTER_ADDITIONAL_INFO: {e}"))?;
                                // GC_POINTS con hp/mp restaurados.
                                conn.send(&packets::points_packet(&row, next_exp).to_bytes())
                                    .await
                                    .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
                                eprintln!(
                                    "server_realms: channel conn {conn_id}: {} REVIVIÓ (answer {answer}, \
                                     hp {}/{}, mp {}/{})",
                                    row.name, row.hp, max[0], row.mp, max[1]
                                );
                            }
                        } else {
                            // Sin muerte: el script answer del diálogo de
                            // quests es F5.x — se ignora con log.
                            eprintln!(
                                "server_realms: channel conn {conn_id}: CG_SCRIPT_ANSWER sin muerte — \
                                 ignorado (quests F5.x)"
                            );
                        }
                        continue;
                    }
                    // TODO(F5 npcs): realm::npc::... para los NPCs/mobs
                    other => {
                        eprintln!(
                            "server_realms: channel conn {conn_id}: paquete de juego 0x{other:02x} ignorado \
                             (el procesamiento — movimiento/combate — es F5)"
                        );
                    }
                }
            }
            _ = &mut idle => {
                return Err(format!(
                    "timeout de inactividad de {} ms — sin paquetes del cliente, conexión cerrada",
                    config.timeout.as_millis()
                ));
            }
            _ = ping_timer.tick() => {
                // Heartbeat del server (parity desc.cpp:205-208): GC_PING cada
                // ping_interval_ms; el cliente responde CG_PONG (que resetea
                // `last_packet` al llegar por el brazo del recv).
                conn.send(&[header::GC_PING]).await
                    .map_err(|e| format!("enviando GC_PING: {e}"))?;
            }
            _ = ai_timer.tick() => {
                // F5.3 (NPC AI): los mobs AGGRO persiguen al jugador. Por
                // cada uno: si está FUERA de rango, paso hacia el jugador
                // (move_speed × tick, realm::ai::step_toward — pura) +
                // GC_MOVE (FUNC_MOVE, el cliente interpola); si está EN
                // RANGO (parity `melee_max_range` — 300 o el rango del mob),
                // ATACA: GC_MOVE(FUNC_ATTACK) + GC_DAMAGE_INFO + daño al
                // jugador (parity `SendMovePacket(FUNC_ATTACK, ...)`,
                // char_state.cpp:386 + `battle_hit`).
                if !live_npcs.is_empty() {
                    let px = motion.x;
                    let py = motion.y;
                    let vids: Vec<u32> = live_npcs
                        .iter()
                        .filter(|(_, n)| n.aggro)
                        .map(|(vid, _)| *vid)
                        .collect();
                    for vid in vids {
                        let Some(npc) = live_npcs.get_mut(&vid) else { continue };
                        let dist = realm::combat::distance_approx(
                            npc.state.x - px,
                            npc.state.y - py,
                        );
                        // F5.3 (de-aggro por distancia): si el jugador se
                        // aleja del mob hostil más allá del umbral, el mob
                        // pierde el aggro y deja de perseguir (parity del
                        // C++: el mob abandona la persecución fuera de su
                        // rango — el data-driven con `aggressive_sight` del
                        // mob_proto queda pendiente; umbral fijo 50 m).
                        const DE_AGGRO_DISTANCE: i32 = 5_000;
                        if dist > DE_AGGRO_DISTANCE {
                            npc.aggro = false;
                            eprintln!(
                                "server_realms: channel conn {conn_id}: mob vnum {} (vid {}) — \
                                 perdió el aggro (dist {dist} > {DE_AGGRO_DISTANCE})",
                                npc.vnum, vid
                            );
                            continue;
                        }
                        if dist <= realm::combat::melee_max_range(&npc.state) {
                            // EN RANGO: ataque del mob (FUNC_ATTACK + daño).
                            let mut roll = |lo: i32, hi: i32| {
                                let span = (hi - lo + 1).max(1) as u32;
                                lo + (rand32() % span) as i32
                            };
                            let dmg = realm::ai::attack_damage(
                                npc.damage_min,
                                npc.damage_max,
                                &mut roll,
                            );
                            // GC_MOVE(FUNC_ATTACK): x/y = posición actual del
                            // mob, dwDuration 0 (parity char_state.cpp:386).
                            let mv = protocol::movement::TPacketGCMove {
                                header: protocol::movement::TPacketGCMove::HEADER,
                                b_func: protocol::movement::TPacketGCMove::FUNC_ATTACK,
                                b_arg: 0,
                                b_rot: 0,
                                vid,
                                x: npc.state.x,
                                y: npc.state.y,
                                dw_time: now32(),
                                dw_duration: 0,
                            };
                            conn.send(&mv.to_bytes())
                                .await
                                .map_err(|e| format!("enviando GC_MOVE(FUNC_ATTACK): {e}"))?;
                            // GC_DAMAGE_INFO (135) al jugador — el número de
                            // daño (parity `SendDamagePacket`).
                            conn.send(
                                &protocol::combat::GcDamageInfo::new(
                                    row.id as u32,
                                    protocol::combat::damage_flag::NORMAL,
                                    dmg,
                                )
                                .to_bytes()
                                .to_vec(),
                            )
                            .await
                            .map_err(|e| format!("enviando GC_DAMAGE_INFO: {e}"))?;
                            // Daño al jugador + GC_POINTS (la barra) + save.
                            // La MUERTE del PC (hp <= 0): GC_DEAD + puntos a 0
                            // (el cliente muestra la pantalla de muerte); el
                            // revive lo dispara el CG_SCRIPT_ANSWER del
                            // cliente (handler propio — RestartAtSamePos).
                            row.hp = row.hp.saturating_sub(dmg);
                            if row.hp <= 0 {
                                row.hp = 0;
                                conn.send(&protocol::world::TPacketGCDead::new(row.id as u32).to_bytes())
                                    .await
                                    .map_err(|e| format!("enviando GC_DEAD: {e}"))?;
                                eprintln!(
                                    "server_realms: channel conn {conn_id}: {} MURIÓ (mob vnum {} vid {}) — \
                                     esperando revive (CG_SCRIPT_ANSWER)",
                                    row.name, npc.vnum, vid
                                );
                            } else {
                                eprintln!(
                                    "server_realms: channel conn {conn_id}: mob vnum {} (vid {}) \
                                     atacó a {} por {dmg} (hp {})",
                                    npc.vnum, vid, row.name, row.hp
                                );
                            }
                            conn.send(&packets::points_packet(&row, next_exp).to_bytes())
                                .await
                                .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
                            store.save_character(&row);
                            continue;
                        }
                        let (nx, ny) = realm::ai::step_toward(
                            npc.state.x,
                            npc.state.y,
                            px,
                            py,
                            npc.move_speed,
                            AI_TICK_MS,
                        );
                        if (nx, ny) == (npc.state.x, npc.state.y) {
                            continue; // ya en el jugador (o speed 0)
                        }
                        let rot = realm::ai::rotation_5deg(npc.state.x, npc.state.y, nx, ny);
                        npc.state.x = nx;
                        npc.state.y = ny;
                        let mv = protocol::movement::TPacketGCMove {
                            header: protocol::movement::TPacketGCMove::HEADER,
                            b_func: protocol::movement::TPacketGCMove::FUNC_MOVE,
                            b_arg: 0,
                            b_rot: rot,
                            vid,
                            x: nx,
                            y: ny,
                            dw_time: now32(),
                            dw_duration: AI_TICK_MS as u32,
                        };
                        conn.send(&mv.to_bytes())
                            .await
                            .map_err(|e| format!("enviando GC_MOVE: {e}"))?;
                    }
                    // F5.3 (PATRULLAJE): los mobs IDLE (no aggro, no NOMOVE)
                    // caminan cerca de su spawn — probabilidad 1/7 por tick y
                    // destino aleatorio 300-700 units dentro del radio del
                    // spawn (`realm::ai::patrol_step`, parity
                    // char_state.cpp:668-688). Solo los VISIBLES para el
                    // jugador (el C++ solo actualiza el sectree del PC) y con
                    // límite de paquetes por tick (no floodear).
                    const PATROL_RADIUS: i32 = 1_500; // units del spawn
                    const PATROL_VIEW: i32 = 2_500; // units del jugador
                    const PATROL_MAX_SENDS: usize = 20;
                    let mut patrol_sent = 0usize;
                    let patrol_vids: Vec<u32> = live_npcs
                        .iter()
                        .filter(|(_, n)| !n.aggro && !n.nomove)
                        .filter(|(_, n)| {
                            realm::combat::distance_approx(n.state.x - px, n.state.y - py)
                                <= PATROL_VIEW
                        })
                        .map(|(vid, _)| *vid)
                        .collect();
                    for vid in patrol_vids {
                        if patrol_sent >= PATROL_MAX_SENDS {
                            break;
                        }
                        let Some(npc) = live_npcs.get_mut(&vid) else { continue };
                        let mut roll = |lo: i32, hi: i32| {
                            let span = (hi - lo + 1).max(1) as u32;
                            lo + (rand32() % span) as i32
                        };
                        let Some((tx, ty)) = realm::ai::patrol_step(
                            npc.state.x,
                            npc.state.y,
                            npc.home_x,
                            npc.home_y,
                            PATROL_RADIUS,
                            &mut roll,
                        ) else {
                            continue;
                        };
                        let (nx, ny) = realm::ai::step_toward(
                            npc.state.x,
                            npc.state.y,
                            tx,
                            ty,
                            npc.move_speed,
                            AI_TICK_MS,
                        );
                        if (nx, ny) == (npc.state.x, npc.state.y) {
                            continue;
                        }
                        let rot = realm::ai::rotation_5deg(npc.state.x, npc.state.y, nx, ny);
                        npc.state.x = nx;
                        npc.state.y = ny;
                        let mv = protocol::movement::TPacketGCMove {
                            header: protocol::movement::TPacketGCMove::HEADER,
                            b_func: protocol::movement::TPacketGCMove::FUNC_MOVE,
                            b_arg: 0,
                            b_rot: rot,
                            vid,
                            x: nx,
                            y: ny,
                            dw_time: now32(),
                            dw_duration: AI_TICK_MS as u32,
                        };
                        conn.send(&mv.to_bytes())
                            .await
                            .map_err(|e| format!("enviando GC_MOVE (patrulla): {e}"))?;
                        patrol_sent += 1;
                    }
                }
            }
        }
    }
}

/// Paquetes del PLAYER LOAD (parity `input_db.cpp:428-459` + los DG_*
/// asíncronos del db — `ItemLoad`/`AffectLoad`): `GC_PHASE(LOADING)` ->
/// `TPacketGCMainCharacter` (113) -> 36×`TPacketGCQuickSlotAdd` (28) ->
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
fn enter_packets(
    row: &database::player::PlayerRow,
    empire: u8,
    lands: &[database::land::LandRow],
) -> Vec<Vec<u8>> {
    let mut out = vec![
        packets::character_add(row).to_bytes().to_vec(),
        packets::character_additional_info(row, empire).to_bytes().to_vec(),
        TPacketGCPhase::new(phase::GAME).to_bytes().to_vec(),
    ];
    if !lands.is_empty() {
        out.push(packets::land_list(lands));
    }
    out
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
    Ok(packets::login_success(&players, handle, rand32(), server_ip, port))
}

/// `"addr:port"` del config `listen` -> (addr, port). El puerto del canal es
/// el que el cliente usa en el DirectEnter (wPort del 449 B).
fn parse_listen(listen: &str) -> Result<(String, u16), String> {
    let Some(colon) = listen.rfind(':') else {
        return Err(format!("listen sin puerto: {listen}"));
    };
    let ip = listen[..colon].to_string();
    let port: u16 = listen[colon + 1..]
        .parse()
        .map_err(|_| format!("listen con puerto inválido: {listen}"))?;
    Ok((ip, port))
}

/// Empire del `GC_EMPIRE`: 1..3 de la cuenta -> su valor; `None`/0 -> random
/// 1..3 (parity `input_db.cpp:167-180`: `GetServerLocation(0)` falla y el C++
/// manda `number(1, 3)`).
fn empire_byte(empire: Option<i16>) -> u8 {
    match empire {
        Some(1..=3) => empire.unwrap() as u8,
        _ => (rand32() % 3) as u8 + 1,
    }
}

/// `GC_LOGIN_FAILURE` con log (patrón del auth).
async fn send_login_failure(conn: &mut Connection<TcpStream>, status: &str) -> Result<(), String> {
    eprintln!("server_realms: channel: GC_LOGIN_FAILURE {status}");
    conn.send(&TPacketGCLoginFailure::new(status).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_LOGIN_FAILURE: {e}"))
}

/// C-string → `&str` (hasta el primer NUL; bytes no-UTF8 → vacío defensivo).
fn cstr(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// NPC vivo del mundo del canal (F5.2): el estado del combate + el HP
/// runtime. `state` es la vista inmutable que `handle_attack` consume.
/// F5.3: `exp`/`gold_min`/`gold_max`/`drop_item` = recompensas del mob
/// (del `mob_proto`); `move_speed` + `aggro` = la AI mínima (F5.3): el mob
/// se vuelve hostil al recibir daño y persigue al jugador.
struct LiveNpc {
    state: realm::combat::NpcState,
    vnum: i64,
    max_hp: i32,
    hp: i32,
    exp: i64,
    gold_min: i32,
    gold_max: i32,
    drop_item: i64,
    /// `move_speed` del mob_proto (UNITS/seg) — el paso del AI por tick.
    move_speed: i32,
    /// Hostil: true tras recibir daño del jugador (el AI lo persigue).
    aggro: bool,
    /// `damage_min`/`damage_max` del mob_proto — el daño del ataque del mob.
    damage_min: i32,
    damage_max: i32,
    /// Posición del SPAWN (home) — el patrullaje (F5.3) clampa el destino
    /// al radio del spawn (parity del estado IDLE del C++).
    home_x: i32,
    home_y: i32,
    /// `ai_flag` del mob_proto: "NOMOVE" → el mob NO patrulla (parity
    /// `AIFLAG_NOMOVE` — char_state.cpp:668).
    nomove: bool,
}

/// Item EN EL SUELO del mundo del canal (F5.3): el estado que el pickup
/// consume (vnum/count/posición). El cliente lo pinta con el
/// `GC_ITEM_GROUND_ADD` y lo quita con el `GC_ITEM_GROUND_DEL`.
#[derive(Clone, Copy)]
struct LiveGroundItem {
    vnum: u32,
    count: u32,
    x: i32,
    y: i32,
    z: i32,
}

/// Contador global de VIDs de items del suelo del canal: arranca en
/// 50 000 (los PCs son ids bajos 1..5 y los NPCs 10 000+ — sin colisión).
fn next_item_vid() -> u32 {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(50_000);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Contador global de VIDs de NPCs del canal: arranca en 10 000 (los PCs son
/// ids bajos 1..5 — no colisionan; parity del AllocVID del C++ que separa
/// los rangos).
fn next_npc_vid() -> u32 {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(10_000);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// `now_ms` — reloj del servidor en ms desde boot (parity `get_dword_time`).
fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// `now_ms` como u32 (el wire del MOVE — parity `get_dword_time` con wrap).
fn now32() -> u32 {
    now_ms() as u32
}

/// Rand 32-bit determinista sin dependencias (nanos + contador — patrón del
/// nonce del handshake y de `unique_login_key` del auth). Uso: `random_key`
/// del 449 B (parity `DESC_MANAGER::MakeRandomKey`) y empire aleatorio.
fn rand32() -> u32 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    (nanos ^ c.wrapping_mul(0x9E37_79B9_7F4A_7C15)) as u32
}

/// Logins con sesión activa en el canal (parity `DESC_MANAGER::FindByLoginName`,
/// `db.cpp:354-359` — el C++ rechaza un segundo login del mismo nombre). El
/// guard libera al cerrar la conexión. Independiente del guard del auth (otro
/// proceso).
fn channel_logins() -> &'static Mutex<std::collections::HashSet<String>> {
    static M: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

struct ChannelLoginGuard {
    login: String,
}

impl ChannelLoginGuard {
    fn acquire(login: &str) -> Option<Self> {
        let mut set = channel_logins().lock().expect("channel_logins lock");
        if set.contains(login) {
            return None;
        }
        set.insert(login.to_string());
        Some(Self { login: login.to_string() })
    }
}

impl Drop for ChannelLoginGuard {
    fn drop(&mut self) {
        channel_logins()
            .lock()
            .expect("channel_logins lock")
            .remove(&self.login);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::player::PlayerRow;
    use std::time::Duration;
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
        // 2 (LOADING+113) + 36 quickslots + 2 (16+76) + 1 item + 1 affect.
        assert_eq!(pkts.len(), 42, "2 + 36 + 2 + 1 + 1");
        assert_eq!(pkts[0].len(), TPacketGCPhase::SIZE);
        assert_eq!(pkts[0][0], header::GC_PHASE);
        assert_eq!(pkts[0][1], phase::LOADING, "parity input_db.cpp:428");
        assert_eq!(pkts[1].len(), TPacketGCMainCharacter::SIZE, "113, 48 B");
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
        let pkts = enter_packets(&row, 3, &lands);
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
        assert_eq!(enter_packets(&row, 3, &[]).len(), 3);
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

    /// random_key nunca es 0 (parity MakeRandomKey: number(1, INT_MAX)).
    #[test]
    fn rand32_never_zero() {
        for _ in 0..100 {
            assert_ne!(rand32(), 0);
        }
    }

    /// parse_listen: el addr:port del config — la dirección del DirectEnter.
    #[test]
    fn parse_listen_addr_port() {
        assert_eq!(
            parse_listen("172.25.104.175:30003").unwrap(),
            ("172.25.104.175".to_string(), 30003)
        );
        assert_eq!(parse_listen("127.0.0.1:0").unwrap().1, 0, "puerto 0 (tests)");
        assert!(parse_listen("sinpuerto").is_err());
        assert!(parse_listen("a:b:c").is_err(), "puerto no numérico");
    }

    /// El timeout del canal es de INACTIVIDAD (no absoluto): cada paquete
    /// recibido resetea el timer; el silencio > timeout dispara el cierre.
    /// Con el reloj pausado: paquetes a t=0/150/300 (ventana 200 ms) siempre
    /// dentro de la ventana → la conexión sigue; silencio tras t=300 →
    /// el timer dispara.
    #[tokio::test(start_paused = true)]
    async fn idle_timeout_resets_on_traffic_and_fires_on_silence() {
        use tokio::io::AsyncWriteExt;
        let (server_side, mut client_side) = tokio::io::duplex(1024);
        let mut conn = Connection::new(server_side);
        let mut framer = Framer::new(ConnectionRole::Channel);
        let timeout = Duration::from_millis(200);

        // MOVE (16 B) como el paquete de juego del cliente vivo.
        let move_pkt = [7u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        // t=0: primer paquete → recv OK (el timer nace en la llamada).
        client_side.write_all(&move_pkt).await.unwrap();
        let pkt = recv_packet_idle(&mut conn, &mut framer, timeout).await.expect("t=0");
        assert_eq!(pkt[0], 7);

        // t=0..150: silencio; a t=150 llega otro paquete → recv OK
        // (150 < 200 — dentro de la ventana de la llamada).
        tokio::time::advance(Duration::from_millis(150)).await;
        client_side.write_all(&move_pkt).await.unwrap();
        let pkt = recv_packet_idle(&mut conn, &mut framer, timeout).await.expect("t=150");
        assert_eq!(pkt[0], 7);

        // t=150..300: silencio; a t=300 llega otro → recv OK (300-150=150 < 200
        // — el timer de ESTA llamada nace en t=150).
        tokio::time::advance(Duration::from_millis(150)).await;
        client_side.write_all(&move_pkt).await.unwrap();
        let pkt = recv_packet_idle(&mut conn, &mut framer, timeout).await.expect("t=300");
        assert_eq!(pkt[0], 7);

        // t=300..: silencio total → avanzar 250 > 200 → el timer dispara.
        tokio::time::advance(Duration::from_millis(250)).await;
        let err = recv_packet_idle(&mut conn, &mut framer, timeout).await;
        assert!(
            err.is_err() && err.unwrap_err().contains("inactividad"),
            "el silencio > timeout dispara el cierre"
        );
    }

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
