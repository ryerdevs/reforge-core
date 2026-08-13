//! Integration F4 slice 3 (HITO del slice): fake client legacy contra el
//! channel REAL con PostgreSQL de verdad — el flujo login→select→**entrada al
//! mundo** end-to-end (el cliente queda DENTRO del mapa, mundo vacío).
//! Gated con `#[ignore]` (requiere la PG de WSL).
//!
//! Ejecutar desde WSL:
//!
//! ```text
//! source ~/.cargo/env
//! cd /mnt/c/projects/Metin2/source/reforge
//! cargo test --package server_realms -- --ignored
//! ```
//!
//! Flujo verificado (parity input_login.cpp / input_db.cpp / building.cpp):
//! handshake → GC_PHASE(LOGIN) → LOGIN3 65 B (test/1234) → GC_EMPIRE(3) →
//! GC_PHASE(SELECT) → 449 B (slots [1,3,5,0,2]) → CG_PLAYER_SELECT(0) →
//! **PLAYER LOAD**: GC_PHASE(LOADING) + MAIN_CHARACTER(15) + POINTS(16) +
//! SKILLS(76) → [el cliente carga el mapa] → CG_ENTERGAME(10) → **ENTERGAME**:
//! ADD(1) + INFO(136) + GC_PHASE(GAME) + LAND_LIST(130, 18 lands del mapa 41).

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use network::{read_exact_size, Connection};
use protocol::world::TPacketCGMarkLogin;
use protocol::{
    phase, TPacketCGHandshake, TPacketCGLogin3, TPacketCGPlayerSelect, TPacketGCPhase,
    TPacketGCLoginSuccess,
};
use tokio::net::TcpStream;

const DEFAULT_PG: &str = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2";

fn pg_conn() -> String {
    std::env::var("DATABASE_TEST_PG").unwrap_or_else(|_| DEFAULT_PG.to_string())
}

fn write_temp_config(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("f4_channel_pg_{name}.toml"));
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

fn spawn_channel(config_path: &std::path::Path) -> (Child, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_server_realms"))
        .args(["--role", "channel", "--config"])
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
            if let Some(addr) = line.trim().strip_prefix("server_realms: channel escuchando en ") {
                let _ = tx.send(addr.to_string());
                break;
            }
        }
    });
    let addr = rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("el channel no anunció el listener"));
    (child, addr)
}

async fn client_handshake_channel(conn: &mut Connection<TcpStream>) -> u32 {
    let phase_pkt = read_exact_size(conn, TPacketGCPhase::SIZE).await.expect("GC_PHASE");
    assert_eq!(TPacketGCPhase::from_bytes(&phase_pkt).unwrap().phase, phase::HANDSHAKE);
    let hs_pkt = read_exact_size(conn, 13).await.expect("GC_HANDSHAKE");
    let nonce = u32::from_le_bytes([hs_pkt[1], hs_pkt[2], hs_pkt[3], hs_pkt[4]]);
    let dw_time = u32::from_le_bytes([hs_pkt[5], hs_pkt[6], hs_pkt[7], hs_pkt[8]]);
    conn.send(&TPacketCGHandshake::new(nonce, dw_time, 0).to_bytes())
        .await
        .expect("eco");
    let login_phase = read_exact_size(conn, TPacketGCPhase::SIZE)
        .await
        .expect("GC_PHASE(LOGIN)");
    assert_eq!(TPacketGCPhase::from_bytes(&login_phase).unwrap().phase, phase::LOGIN);
    dw_time
}

/// La conexión del GUILD MARK del cliente real: el cliente la abre en paralelo
/// al select (misma IP/puerto del canal) con el handle/random_key del 449 B
/// (`PythonNetworkStreamPhaseLogin.cpp:164-165` → `GuildMarkDownloader.cpp:219-229`):
/// recibe el handshake del server y responde `CG_MARK_LOGIN` (0x64, 9 B) EN
/// VEZ del eco. El canal normal (`guild_mark_server` OFF) la cierra sin
/// responder (`input.cpp:560-572`) — el cliente NO lo interpreta como fallo.
async fn mark_login_handshake(addr: &str, handle: u32, random_key: u32) {
    let stream = TcpStream::connect(addr).await.expect("connect mark");
    let mut conn = Connection::new(stream);

    // El server manda el handshake a TODA conexión nueva (GC_PHASE + GC_HANDSHAKE).
    let phase_pkt = read_exact_size(&mut conn, TPacketGCPhase::SIZE).await.expect("GC_PHASE (mark)");
    assert_eq!(TPacketGCPhase::from_bytes(&phase_pkt).unwrap().phase, phase::HANDSHAKE);
    let hs_pkt = read_exact_size(&mut conn, 13).await.expect("GC_HANDSHAKE (mark)");
    assert_eq!(hs_pkt[0], 0xff, "header GC_HANDSHAKE");

    // Respuesta mark: 0x64 + handle + random_key del 449 B (9 B totales).
    let mark = TPacketCGMarkLogin { header: 100, handle, random_key };
    conn.send(&mark.to_bytes()).await.expect("CG_MARK_LOGIN");

    // El canal cierra SIN responder (parity input.cpp:562-566) → EOF limpio.
    let mut b = [0u8; 1];
    let n = conn.recv(&mut b).await.expect("recv mark");
    assert_eq!(n, 0, "el canal cierra la conexión mark sin paquetes (EOF limpio)");
}

/// Login del canal + el 449 B (handshake → LOGIN3 → GC_EMPIRE → SELECT →
/// 449 B con los asserts de slots). Reutilizado por el flujo completo y el
/// test del idle timeout.
async fn connect_login_449(addr: &str) -> Result<(Connection<TcpStream>, u32), String> {
    let stream = TcpStream::connect(addr).await.map_err(|e| format!("connect: {e}"))?;
    let mut conn = Connection::new(stream);
    let server_time = client_handshake_channel(&mut conn).await;
    // LOGIN3 del canal (65 B) — test/1234 (cuenta viva del E2E).
    conn.send(&TPacketCGLogin3::new_channel("test", "1234", [0; 4]).to_bytes_channel())
        .await
        .map_err(|e| format!("LOGIN3: {e}"))?;

    // GC_EMPIRE (2 B, 0x5a): empire de la cuenta test = 3 (E2E Q1).
    let empire_pkt = read_exact_size(&mut conn, 2).await.map_err(|e| format!("GC_EMPIRE: {e}"))?;
    assert_eq!(empire_pkt[0], 0x5a, "header GC_EMPIRE");
    assert_eq!(empire_pkt[1], 3, "empire de la cuenta test (pi.empire)");

    // GC_PHASE(SELECT) (2 B).
    let sel_phase = read_exact_size(&mut conn, 2).await.map_err(|e| format!("phase: {e}"))?;
    assert_eq!(sel_phase[0], 0xfd, "GC_PHASE");
    assert_eq!(sel_phase[1], phase::SELECT, "phase SELECT");

    // 449 B: slots del player_index [1, 3, 5, 0, 2] (E2E Q1).
    let mut pkt = read_exact_size(&mut conn, 1).await.map_err(|e| format!("hdr 449: {e}"))?;
    assert_eq!(pkt[0], TPacketGCLoginSuccess::HEADER, "header 0x20 (NEWSLOT)");
    let rest = read_exact_size(&mut conn, TPacketGCLoginSuccess::SIZE - 1)
        .await
        .map_err(|e| format!("449 B: {e}"))?;
    pkt.extend_from_slice(&rest);
    let success = TPacketGCLoginSuccess::from_bytes(&pkt).map_err(|e| e.to_string())?;
    assert_eq!(success.players[0].dw_id, 1, "slot 0 = pid1 (lkjsnlfknlsk)");
    assert_eq!(success.players[1].dw_id, 3, "slot 1 = pid2 (Chaman)");
    assert_eq!(success.players[2].dw_id, 5, "slot 2 = pid3");
    assert_eq!(success.players[3].dw_id, 0, "slot 3 vacio (pid4=0) -> zeroed");
    assert_eq!(success.players[4].dw_id, 2, "slot 4 = pid5 (ninja)");
    assert_eq!(success.players[0].name(), "lkjsnlfknlsk");
    assert_ne!(success.handle, 0, "handle = conn_id");
    assert_ne!(success.random_key, 0, "random_key (MakeRandomKey parity)");
    // lAddr/wPort del slot 0 = la dirección REAL del canal (el config listen
    // del subproceso usa 127.0.0.1:0 → el puerto del listener real): el
    // DirectEnter del cliente conecta ahí (PythonNetworkStream.cpp:458-469).
    assert_ne!(success.players[0].l_addr, 0, "lAddr = IP del server de juego (DirectEnter)");
    assert_ne!(success.players[0].w_port, 0, "wPort = puerto del canal (DirectEnter)");

    // El cliente REAL abre la conexión del guild mark en paralelo al select
    // (handle/random_key del 449 B) — el canal la cierra sin responder y el
    // select continúa en la conexión principal.
    mark_login_handshake(addr, success.handle, success.random_key).await;

    Ok((conn, server_time))
}

/// Select + PLAYER LOAD + ENTERGAME (tamaños fijos — el personaje tiene
/// 0 items/0 affects) + lee los NPCs del spawn del mapa (add 37 B [+
/// addInfo 70 B si NPC]) hasta que el header deje de ser 1/136. Devuelve
/// la lista (vid, x, y, wrace) de los adds — para que el fake elija el mob.
///
/// TIMING (F5 perf): la resolución de spawns usa la caché compartida + una
/// query batch (game_core::npc::MobCache) — el entry + los spawns fluyen en
/// SEGUNDOS. El assert de <15 s fija el contrato (la resolución previa,
/// 10k × load_by_vnum con conexión por llamada, stallaba ~3-4 min).
async fn enter_and_read_spawns(conn: &mut Connection<TcpStream>) -> Result<Vec<(u32, i32, i32, u32)>, String> {
    let t0 = std::time::Instant::now();
    // CG_PLAYER_SELECT slot 0.
    conn.send(&TPacketCGPlayerSelect::new(0).to_bytes())
        .await
        .map_err(|e| format!("select: {e}"))?;
    let _ = read_exact_size(conn, 2).await.map_err(|e| format!("loading: {e}"))?; // GC_PHASE(LOADING)
    let _ = read_exact_size(conn, 47).await.map_err(|e| format!("main_char: {e}"))?; // MAIN_CHARACTER (15, 47 B)
    for _ in 0..36 {
        let _ = read_exact_size(conn, 4).await.map_err(|e| format!("QS: {e}"))?;
    }
    let _ = read_exact_size(conn, 1021).await.map_err(|e| format!("16: {e}"))?; // POINTS
    let _ = read_exact_size(conn, 1531).await.map_err(|e| format!("76: {e}"))?; // SKILLS
    // VERSION (0xf1) + CG_ENTERGAME.
    let mut version = vec![0xf1u8];
    version.resize(67, 0);
    conn.send(&version).await.map_err(|e| format!("VERSION: {e}"))?;
    conn.send(&[10u8]).await.map_err(|e| format!("ENTERGAME: {e}"))?;
    // Cola del ENTERGAME (tamaños fijos).
    let _ = read_exact_size(conn, 37).await.map_err(|e| format!("ADD: {e}"))?;
    let _ = read_exact_size(conn, 70).await.map_err(|e| format!("INFO: {e}"))?;
    let _ = read_exact_size(conn, 2).await.map_err(|e| format!("GAME: {e}"))?;
    let land_hdr = read_exact_size(conn, 3).await.map_err(|e| format!("land: {e}"))?;
    let size = u16::from_le_bytes([land_hdr[1], land_hdr[2]]);
    let _ = read_exact_size(conn, (size - 3) as usize).await.map_err(|e| format!("lands: {e}"))?;
    let _ = read_exact_size(conn, 5).await.map_err(|e| format!("TIME: {e}"))?;
    let _ = read_exact_size(conn, 2).await.map_err(|e| format!("CHANNEL: {e}"))?;

    // Los NPCs del spawn del mapa (F5.2): add (1, 37 B) [+ addInfo (136,
    // 70 B) si type NPC] contiguos por mob. El add lleva vid@1, x@9, y@13,
    // wRaceNum (el vnum del mob)@22.
    let mut spawns: Vec<(u32, i32, i32, u32)> = Vec::new();
    loop {
        let hdr = read_exact_size(conn, 1).await.map_err(|e| format!("spawn hdr: {e}"))?;
        match hdr[0] {
            1 => {
                let body = read_exact_size(conn, 36).await.map_err(|e| format!("spawn add: {e}"))?;
                let mut pkt = hdr.clone();
                pkt.extend_from_slice(&body);
                let vid = u32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]);
                let x = i32::from_le_bytes([pkt[9], pkt[10], pkt[11], pkt[12]]);
                let y = i32::from_le_bytes([pkt[13], pkt[14], pkt[15], pkt[16]]);
                let wrace = u32::from_le_bytes([pkt[22], pkt[23], pkt[24], pkt[25]]);
                spawns.push((vid, x, y, wrace));
            }
            136 => {
                let _ = read_exact_size(conn, 69).await.map_err(|e| format!("spawn info: {e}"))?;
            }
            other => {
                // Fin de los spawns — el byte leído es el siguiente paquete
                // (ping/heartbeat del canal). No se devuelve al buffer; el
                // caller sigue con su fase.
                eprintln!("fin de spawns ({} adds); siguiente header 0x{other:02x}", spawns.len());
                break;
            }
        }
    }
    assert!(!spawns.is_empty(), "el mapa 41 tiene spawns (game_core::npc)");
    // F5 perf: la resolución con caché no debe stallar la entrada (el
    // contrato previo sin batch: ~3-4 min por entrada — regresión acá).
    let elapsed = t0.elapsed();
    assert!(
        elapsed < Duration::from_secs(15),
        "entry + spawns en {:.1} s — la resolución debe ser batch/caché",
        elapsed.as_secs_f64()
    );
    eprintln!(
        "entry + {} spawns leídos en {:.1} s (resolución batch/caché)",
        spawns.len(),
        elapsed.as_secs_f64()
    );
    Ok(spawns)
}

/// El flujo completo del cliente REAL: handshake → LOGIN3 → GC_EMPIRE →
/// SELECT → 449 B → [conexión mark en paralelo] → CG_PLAYER_SELECT → PLAYER
/// LOAD (LOADING+15+16+76) → CG_ENTERGAME → ENTERGAME (ADD+INFO+GAME+
/// LAND_LIST). Reutilizado por el gated con subproceso y por el test contra
/// el canal DESPLEGADO (30003).
async fn full_login_select_entry_flow(addr: &str) -> Result<(), String> {
    let (mut conn, _server_time) = connect_login_449(addr).await?;

        // CG_PLAYER_SELECT slot 0 (2 B).
        conn.send(&TPacketCGPlayerSelect::new(0).to_bytes())
            .await
            .map_err(|e| format!("select: {e}"))?;

        // ---- PLAYER LOAD (parity input_db.cpp:428-459) ----
        // GC_PHASE(LOADING) (2 B).
        let loading = read_exact_size(&mut conn, 2).await.map_err(|e| format!("loading: {e}"))?;
        assert_eq!(loading[0], 0xfd, "GC_PHASE");
        assert_eq!(loading[1], phase::LOADING, "parity input_db.cpp:428");
        // MAIN_CHARACTER (15, 47 B — layout del CLIENTE sin BGM, sin empire):
        // vid = 1 (pid del slot 0), race = job.
        let main_pkt = read_exact_size(&mut conn, 47).await.map_err(|e| format!("main_char: {e}"))?;
        assert_eq!(main_pkt[0], 15, "header GC_MAIN_CHARACTER (Packet.h:160)");
        assert_eq!(u32::from_le_bytes([main_pkt[1], main_pkt[2], main_pkt[3], main_pkt[4]]), 1, "dwVID");
        let name_end = main_pkt[9..34].iter().position(|&b| b == 0).unwrap_or(25);
        assert_eq!(&main_pkt[9..9 + name_end], b"lkjsnlfknlsk", "szName del main character");
        let lx = i32::from_le_bytes([main_pkt[34], main_pkt[35], main_pkt[36], main_pkt[37]]);
        let ly = i32::from_le_bytes([main_pkt[38], main_pkt[39], main_pkt[40], main_pkt[41]]);
        assert!(lx > 0 && ly > 0, "lx/ly UNITS: {lx},{ly}");
        // 47 B exactos: el byte 46 es el skill_group (el layout del CLIENTE
        // no tiene empire — el del server 48 B desalinearía TODO el stream).
        // 36 × QUICKSLOT_ADD (28, 4 B) — el hotbar del entry (parity
        // input_db.cpp:455-456 SetQuickslot -> paquete, char_quickslot.cpp:96-103).
        for i in 0..36u8 {
            let q = read_exact_size(&mut conn, 4).await.map_err(|e| format!("QS {i}: {e}"))?;
            assert_eq!(q[0], 28, "header GC_QUICKSLOT_ADD (slot {i})");
            assert_eq!(q[1], i, "pos del slot {i}");
        }
        // POINTS (16, 1021 B): level@5 = level del personaje (>= 1),
        // MAX_HP@25 > 0 (subset ComputePoints — el HUD necesita los máximos),
        // MOV_SPEED@77 = 100 (parity char.cpp:2245).
        let points_pkt = read_exact_size(&mut conn, 1021).await.map_err(|e| format!("16: {e}"))?;
        assert_eq!(points_pkt[0], 16, "header GC_CHARACTER_POINTS");
        let level = i32::from_le_bytes([points_pkt[5], points_pkt[6], points_pkt[7], points_pkt[8]]);
        assert!(level >= 1, "POINT_LEVEL del personaje: {level}");
        let max_hp = i32::from_le_bytes([points_pkt[25], points_pkt[26], points_pkt[27], points_pkt[28]]);
        assert!(max_hp > 0, "POINT_MAX_HP > 0 (ComputePoints subset): {max_hp}");
        let mov = i32::from_le_bytes([points_pkt[77], points_pkt[78], points_pkt[79], points_pkt[80]]);
        assert_eq!(mov, 100, "POINT_MOV_SPEED = 100 (parity char.cpp:2245)");
        // SKILLS (76, 1531 B): 255 skills del bytea (1530 B).
        let skills_pkt = read_exact_size(&mut conn, 1531).await.map_err(|e| format!("76: {e}"))?;
        assert_eq!(skills_pkt[0], 76, "header GC_SKILL_LEVEL");

        // ---- el cliente carga el mapa; al terminar manda la VERSIÓN (0xf1,
        // 67 B — TPacketCGClientVersion2: header + filename[33] +
        // timestamp[33], Packet.h:974-979) y luego CG_ENTERGAME (10, 1 B).
        // El canal ignora el 0xf1 sin validar (parity input.cpp:205-213).
        let mut version = vec![0xf1u8];
        version.extend_from_slice(b"metin2client.exe\0");
        version.resize(1 + 33, 0);
        version.extend_from_slice(b"1215955205\0");
        version.resize(67, 0);
        assert_eq!(version.len(), 67, "TPacketCGClientVersion2");
        conn.send(&version).await.map_err(|e| format!("CG_CLIENT_VERSION2: {e}"))?;
        conn.send(&[10u8]).await.map_err(|e| format!("CG_ENTERGAME: {e}"))?;

        // ---- la cola restante del canal (los items/affects del entry que
        // quedan en el buffer + el ENTERGAME) — el orden es el del C++
        // (ItemLoad/AffectLoad asíncronos antes del Entergame, input_db.cpp
        // 1453/1563 + input_login.cpp:611-656). Loop por header: el número de
        // items/affects es vivo (datos reales de lkjsnlfknlsk).
        let mut items_seen = 0;
        let mut affects_seen = 0;
        let mut landed = false;
        loop {
            let hdr = read_exact_size(&mut conn, 1).await.map_err(|e| format!("cola: {e}"))?;
            match hdr[0] {
                21 => {
                    // GC_ITEM_SET (51 B packed): window/cell/vnum spot.
                    let body = read_exact_size(&mut conn, 50).await.map_err(|e| format!("item: {e}"))?;
                    let mut pkt = hdr.clone();
                    pkt.extend_from_slice(&body);
                    assert_eq!(pkt.len(), 51, "TPacketGCItemSet::SIZE");
                    let window = pkt[1];
                    assert!(
                        window == 1 || window == 2 || window == 5 || window == 6,
                        "window del item (INVENTORY/EQUIPMENT/DS/BELT): {window}"
                    );
                    assert!(u32::from_le_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]) > 0, "vnum del item");
                    items_seen += 1;
                }
                126 => {
                    // GC_AFFECT_ADD (22 B).
                    let body = read_exact_size(&mut conn, 21).await.map_err(|e| format!("affect: {e}"))?;
                    assert_eq!(body.len(), 21, "TPacketAffectElement");
                    affects_seen += 1;
                }
                1 => {
                    // ADD (37 B): vid = 1, type PC (6).
                    let add_pkt = read_exact_size(&mut conn, 36).await.map_err(|e| format!("ADD: {e}"))?;
                    let mut pkt = hdr.clone();
                    pkt.extend_from_slice(&add_pkt);
                    assert_eq!(pkt.len(), 37, "TPacketGCCharacterAdd::SIZE");
                    assert_eq!(u32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]), 1, "dwVID = pid");
                    assert_eq!(pkt[21], 6, "bType = CHAR_TYPE_PC (length.h:330)");
                    let x = i32::from_le_bytes([pkt[9], pkt[10], pkt[11], pkt[12]]);
                    let y = i32::from_le_bytes([pkt[13], pkt[14], pkt[15], pkt[16]]);
                    assert!(x > 0 && y > 0, "x/y UNITS: {x},{y}");
                }
                136 => {
                    // INFO (70 B): vid = 1, name = lkjsnlfknlsk.
                    let info_pkt = read_exact_size(&mut conn, 69).await.map_err(|e| format!("INFO: {e}"))?;
                    let mut pkt = hdr.clone();
                    pkt.extend_from_slice(&info_pkt);
                    assert_eq!(pkt.len(), 70, "TPacketGCCharacterAdditionalInfo::SIZE");
                    assert_eq!(u32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]), 1, "dwVID");
                    let name_end = pkt[5..30].iter().position(|&b| b == 0).unwrap_or(25);
                    assert_eq!(&pkt[5..5 + name_end], b"lkjsnlfknlsk", "name del personaje");
                    assert_eq!(pkt[50], 3, "bEmpire = empire de la cuenta");
                }
                0xfd => {
                    // GC_PHASE(GAME) (2 B).
                    let phase_byte = read_exact_size(&mut conn, 1).await.map_err(|e| format!("GAME: {e}"))?;
                    assert_eq!(phase_byte[0], phase::GAME, "parity input_login.cpp:616");
                }
                130 => {
                    // LAND_LIST (130, 3 + N×24 — el mapa 41 tiene 18 lands,
                    // parity log del core "map 41 count 18 elem_size: 432").
                    let size_bytes = read_exact_size(&mut conn, 2).await.map_err(|e| format!("land size: {e}"))?;
                    let size = u16::from_le_bytes([size_bytes[0], size_bytes[1]]);
                    assert_eq!(size, 3 + 18 * 24, "435 B (18 lands del mapa 41)");
                    let land_body = read_exact_size(&mut conn, (size - 3) as usize)
                        .await
                        .map_err(|e| format!("land body: {e}"))?;
                    assert_eq!(u32::from_le_bytes([land_body[0], land_body[1], land_body[2], land_body[3]]), 201u32, "primer land del mapa 41");
                    landed = true;
                }
                106 => {
                    // GC_TIME (106, 5 B): el reloj del server.
                    let time_body = read_exact_size(&mut conn, 4).await.map_err(|e| format!("TIME: {e}"))?;
                    let server_time = u32::from_le_bytes([time_body[0], time_body[1], time_body[2], time_body[3]]);
                    assert!(server_time > 1_700_000_000, "get_global_time (unix now): {server_time}");
                }
                121 => {
                    // GC_CHANNEL (121, 2 B): el canal 1.
                    let chan_byte = read_exact_size(&mut conn, 1).await.map_err(|e| format!("CHANNEL: {e}"))?;
                    assert_eq!(chan_byte[0], 1, "channel 1 (config del test)");
                    break; // fin de la cola del ENTERGAME
                }
                other => {
                    return Err(format!("cola inesperada: header 0x{other:02x}"));
                }
            }
        }
        assert!(landed, "el LAND_LIST llegó antes del cierre de la cola");
        eprintln!(
            "cola completa verificada: {items_seen} items, {affects_seen} affects, 18 lands, TIME, CHANNEL"
        );

        // ---- fase de juego: la secuencia de spawn del cliente real ----
        // CG_MOVE (7, 17 B — TPacketCGMove: header+func+arg+rot+x+y+time,
        // Packet.h:677-686) + TIME_SYNC (0xfc, 13 B) + PONG (0xfe, 1 B) —
        // los primeros paquetes que el cliente manda al spawn. Antes del fix
        // el MOVE (0x07) cerraba la conexión (framer sin la tabla de juego).
        let mut move_pkt = vec![0x07u8, 0, 0, 0]; // header + func + arg + rot
        move_pkt.extend_from_slice(&969600i32.to_le_bytes()); // lX
        move_pkt.extend_from_slice(&278400i32.to_le_bytes()); // lY
        move_pkt.extend_from_slice(&1234u32.to_le_bytes()); // dwTime
        assert_eq!(move_pkt.len(), 16, "TPacketCGMove (1+3+4+4+4)");
        conn.send(&move_pkt).await.map_err(|e| format!("CG_MOVE: {e}"))?;
        let mut timesync = [0u8; 13];
        timesync[0] = 0xfc;
        conn.send(&timesync).await.map_err(|e| format!("TIME_SYNC: {e}"))?;
        conn.send(&[0xfeu8]).await.map_err(|e| format!("PONG: {e}"))?;
        // CG_ATTACK (2, 8 B) + CG_ITEM_PICKUP (15, 5 B) — interacción básica.
        let attack = [0x02u8, 0, 0, 0, 0, 0, 0, 0];
        conn.send(&attack).await.map_err(|e| format!("CG_ATTACK: {e}"))?;
        let pickup = [0x0fu8, 0, 0, 0, 0];
        conn.send(&pickup).await.map_err(|e| format!("CG_ITEM_PICKUP: {e}"))?;

        // La conexión se MANTIENE: el canal no cierra por los paquetes de
        // juego (antes del fix: EOF inmediato tras el MOVE). Con el flujo
        // correcto, recv no devuelve EOF en 500 ms (el canal ignora y sigue).
        let closed = tokio::time::timeout(Duration::from_millis(500), async {
            let mut b = [0u8; 1];
            loop {
                match conn.recv(&mut b).await {
                    Ok(0) => return true, // EOF: el canal cerró
                    Ok(_) => continue,    // datos (p.ej. nada esperado) — seguir
                    Err(_) => return true,
                }
            }
        })
        .await;
        assert!(
            closed.unwrap_or(false) == false,
            "la conexión de juego se mantiene tras MOVE/ATTACK (el canal ignora — F5)"
        );
        Ok(())
}

/// El HITO del slice: el cliente real entra al mundo a través del canal Rust
/// (subproceso con PG real — el flujo incluye la conexión mark en paralelo).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package server_realms -- --ignored"]
async fn channel_full_login_select_spawn_flow() {
    let config_path = write_temp_config("full");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = full_login_select_entry_flow(&addr).await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("flujo login→select→mundo del canal contra PG real");
}

/// Verificación del canal DESPLEGADO (30003 — el binario release que sirve al
/// usuario): el mismo flujo completo del cliente real, sin subproceso.
/// `CHANNEL_TEST_ADDR` sobreescribe la dirección (default 172.25.104.175:30003).
#[tokio::test]
#[ignore = "requiere el canal desplegado en 30003 (WSL): cargo test --package server_realms -- --ignored"]
async fn channel_deployed_30003_full_flow() {
    let addr = std::env::var("CHANNEL_TEST_ADDR")
        .unwrap_or_else(|_| "172.25.104.175:30003".to_string());
    full_login_select_entry_flow(&addr)
        .await
        .expect("flujo completo contra el canal desplegado en 30003");
}

/// LOGIN3 con password mala → NOID (parity db.cpp:244-249).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package server_realms -- --ignored"]
async fn channel_wrong_password_noid() {
    let config_path = write_temp_config("noid");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr).await.map_err(|e| format!("connect: {e}"))?;
        let mut conn = Connection::new(stream);
        client_handshake_channel(&mut conn).await;
        conn.send(&TPacketCGLogin3::new_channel("test", "mala", [0; 4]).to_bytes_channel())
            .await
            .map_err(|e| format!("LOGIN3: {e}"))?;
        let hdr = read_exact_size(&mut conn, 1).await.map_err(|e| format!("header: {e}"))?;
        assert_eq!(hdr[0], 0x07, "GC_LOGIN_FAILURE");
        let rest = read_exact_size(&mut conn, 9).await.map_err(|e| format!("resto: {e}"))?;
        let status = rest.iter().take_while(|&&b| b != 0).copied().collect::<Vec<u8>>();
        assert_eq!(status, b"NOID", "password mala -> NOID");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("password mala -> NOID");
}

/// Select de un slot VACÍO (pid4=0) → cierre limpio (parity input_login.cpp:266-271).
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package server_realms -- --ignored"]
async fn channel_select_empty_slot_closes() {
    let config_path = write_temp_config("empty");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr).await.map_err(|e| format!("connect: {e}"))?;
        let mut conn = Connection::new(stream);
        client_handshake_channel(&mut conn).await;
        conn.send(&TPacketCGLogin3::new_channel("test", "1234", [0; 4]).to_bytes_channel())
            .await
            .map_err(|e| format!("LOGIN3: {e}"))?;
        // Consume GC_EMPIRE + GC_PHASE(SELECT) + 449 B (flujo del login OK).
        let _ = read_exact_size(&mut conn, 2).await.map_err(|e| format!("empire: {e}"))?;
        let _ = read_exact_size(&mut conn, 2).await.map_err(|e| format!("phase: {e}"))?;
        let _ = read_exact_size(&mut conn, TPacketGCLoginSuccess::SIZE)
            .await
            .map_err(|e| format!("449: {e}"))?;

        // Select del slot 3 (pid4 = 0 — vacío).
        conn.send(&TPacketCGPlayerSelect::new(3).to_bytes())
            .await
            .map_err(|e| format!("select: {e}"))?;
        // El canal cierra sin paquetes de spawn (parity "player index not found").
        let mut b = [0u8; 1];
        let n = conn.recv(&mut b).await.map_err(|e| format!("recv: {e}"))?;
        assert_eq!(n, 0, "EOF limpio — slot vacío cierra la conexión");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("select de slot vacío cierra");
}

/// El timeout del canal es de INACTIVIDAD (no absoluto — slice 3.8): con
/// `timeout_ms = 200`, el cliente mandando MOVE cada 100 ms durante >400 ms
/// mantiene la conexión VIVA (cada paquete resetea el timer); el silencio
/// >200 ms la cierra. Antes del fix, un timeout absoluto de 200 ms habría
/// matado la conexión a los 200 ms aunque el cliente estuviera enviando.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package server_realms -- --ignored"]
async fn channel_idle_timeout_reset_by_traffic() {
    let config_path = std::env::temp_dir().join("f4_idle_pg.toml");
    let toml = format!(
        "listen = \"127.0.0.1:0\"\npg_conn = \"{}\"\ntimeout_ms = 200\nno_more_clients = false\nping_interval_ms = 100\n",
        pg_conn()
    );
    std::fs::write(&config_path, &toml).expect("config idle");
    // Spawn propio con stderr PIPED (spawn_channel lo tira a null — para el
    // diagnóstico del cierre del canal en este test).
    let mut child = Command::new(env!("CARGO_BIN_EXE_server_realms"))
        .args(["--role", "channel", "--config"])
        .arg(&config_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("ejecutar server_realms");
    let log_path = std::env::temp_dir().join("f4_idle_channel.log");
    let stderr = std::fs::File::create(&log_path).expect("log idle");
    let stderr_pipe = child.stderr.take().expect("stderr piped");
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        let mut file = stderr;
        let mut reader = BufReader::new(stderr_pipe);
        let mut line = String::new();
        while let Ok(n) = reader.read_line(&mut line) {
            if n == 0 {
                break;
            }
            let _ = file.write_all(line.as_bytes());
            line.clear();
        }
    });
    // Espera el anuncio del listener (como spawn_channel).
    let mut stdout_reader = BufReader::new(child.stdout.take().expect("stdout piped"));
    let mut line = String::new();
    let addr = loop {
        line.clear();
        if stdout_reader.read_line(&mut line).unwrap_or(0) == 0 {
            panic!("el canal no anunció el listener");
        }
        if let Some(a) = line.trim().strip_prefix("server_realms: channel escuchando en ") {
            break a.to_string();
        }
    };

    let result = async {
        let (mut conn, _server_time) = connect_login_449(&addr).await?;
        // Select + entry + los NPCs del spawn (F5.2 — el helper consume los
        // adds del mapa antes de la fase de juego).
        let _spawns = enter_and_read_spawns(&mut conn).await?;
        // CG_ENTERGAME (10, 1 B) — el canal entra en la fase de juego (pings).
        conn.send(&[10u8]).await.map_err(|e| format!("CG_ENTERGAME: {e}"))?;
        // Consume el ENTERGAME del canal (ADD + INFO + GAME + lands) hasta la
        // fase de pings (el helper del combat hace lo mismo — el primer ping).
        tokio::time::timeout(Duration::from_millis(2000), async {
            loop {
                let hdr = read_exact_size(&mut conn, 1).await.map_err(|e| format!("entergame: {e}"))?;
                if hdr[0] == 44 {
                    conn.send(&[0xfeu8]).await.map_err(|e| format!("CG_PONG: {e}"))?;
                    break;
                }
                // ADD (1): 26 B + INFO (136): 134 B — salta el tamaño del header.
                let size = match hdr[0] {
                    1 => 26,
                    136 => 134,
                    _ => return Err(format!("header inesperado en el ENTERGAME: 0x{:02x}", hdr[0])),
                };
                let _ = read_exact_size(&mut conn, size).await.map_err(|e| format!("entergame body: {e}"))?;
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|_| "el canal no entró en la fase de juego (pings) en 2 s")??;

        // Fase de juego EN REPOSO (el escenario real del cliente): el cliente
        // no manda nada — el canal envía GC_PING (44, 1 B) cada 100 ms
        // (heartbeat del server, desc.cpp:179-214) y el fake responde
        // CG_PONG (0xfe, 1 B) — cada pong resetea el idle de 200 ms.
        // 500 ms de pings/pongs → 2.5× el timeout → la sesión SIGUE VIVA.
        let t0 = std::time::Instant::now();
        let mut pings = 0;
        while t0.elapsed() < Duration::from_millis(500) {
            let hdr = read_exact_size(&mut conn, 1).await.map_err(|e| format!("ping: {e}"))?;
            assert_eq!(hdr[0], 44, "GC_PING del heartbeat (desc.cpp:205-208)");
            conn.send(&[0xfeu8]).await.map_err(|e| format!("CG_PONG: {e}"))?;
            pings += 1;
        }
        assert!(pings >= 3, "pings recibidos (intervalo 100 ms): {pings}");
        // La conexión sigue viva tras 500 ms (2.5× el timeout) — el pong
        // resetea el idle. El recv no debe ver EOF en 150 ms (dentro de la
        // ventana del idle desde el último pong).
        let closed = tokio::time::timeout(Duration::from_millis(150), async {
            let mut b = [0u8; 1];
            loop {
                match conn.recv(&mut b).await {
                    Ok(0) => return true,
                    Ok(_) => continue,
                    Err(_) => return true,
                }
            }
        })
        .await;
        assert!(
            closed.unwrap_or(false) == false,
            "la sesión vive > 2×timeout con el heartbeat (pings/pongs)"
        );

        // Silencio total (el fake deja de responder) > timeout → el idle
        // dispara → EOF. Los pings que el canal mandó antes de cerrar quedan
        // en el buffer — se leen hasta el EOF.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let mut b = [0u8; 1];
        let mut eof = false;
        for _ in 0..8 {
            let n = conn.recv(&mut b).await.map_err(|e| format!("recv: {e}"))?;
            if n == 0 {
                eof = true;
                break;
            }
            assert_eq!(b[0], 44, "ping residual del canal antes del cierre");
        }
        assert!(eof, "el silencio > timeout cierra la conexión (idle)");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    let log = std::fs::read_to_string(&log_path).unwrap_or_default();
    let _ = std::fs::remove_file(&log_path);
    if let Err(e) = result {
        eprintln!("--- log del canal idle ---\n{log}\n--- fin ---");
        panic!("idle timeout reseteado por tráfico contra PG real: {e}");
    }
}

/// F5.2 — el combate end-to-end contra un NPC del spawn del mapa 41: el fake
/// ataca al primer mob (cooldown 1250 ms entre golpes al mismo objetivo —
/// battle.cpp:808-838) hasta el GC_DEAD + GC_CHARACTER_DEL (la muerte), y
/// verifica que el NPC removido ya no recibe daño.
#[tokio::test]
#[ignore = "requiere PG real (WSL): cargo test --package server_realms -- --ignored"]
async fn channel_combat_kills_npc() {
    let config_path = write_temp_config("combat");
    // Spawn propio con stderr PIPED (spawn_channel lo tira a null — para el
    // diagnóstico del canal en este test).
    let mut child = Command::new(env!("CARGO_BIN_EXE_server_realms"))
        .args(["--role", "channel", "--config"])
        .arg(&config_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("ejecutar server_realms");
    let log_path = std::env::temp_dir().join("f4_combat_channel.log");
    let stderr = std::fs::File::create(&log_path).expect("log combat");
    let stderr_pipe = child.stderr.take().expect("stderr piped");
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        let mut file = stderr;
        let mut reader = BufReader::new(stderr_pipe);
        let mut line = String::new();
        while let Ok(n) = reader.read_line(&mut line) {
            if n == 0 {
                break;
            }
            let _ = file.write_all(line.as_bytes());
            line.clear();
        }
    });
    // Espera el anuncio del listener.
    let mut stdout_reader = BufReader::new(child.stdout.take().expect("stdout piped"));
    let mut line = String::new();
    let addr = loop {
        line.clear();
        if stdout_reader.read_line(&mut line).unwrap_or(0) == 0 {
            panic!("el canal no anunció el listener");
        }
        if let Some(a) = line.trim().strip_prefix("server_realms: channel escuchando en ") {
            break a.to_string();
        }
    };

    let result = async {
        let (mut conn, server_time) = connect_login_449(&addr).await?;
        // Entry + spawns: el fake elige un mob de BAJO HP atacable — el
        // 20001 (NPC lvl 1, 120 HP — spawnea del npc.txt) o el 101 (126 HP —
        // del regen del runtime, que el lane npc NO emite: kind `r` — GAP
        // reportado al orquestador); si ninguno, el más cercano a la aldea.
        let spawns = enter_and_read_spawns(&mut conn).await?;
        let target = spawns
            .iter()
            .find(|(_, _, _, wrace)| *wrace == 20001)
            .or_else(|| spawns.iter().find(|(_, _, _, wrace)| *wrace == 101))
            .or_else(|| {
                spawns.iter().min_by_key(|(_, x, y, _)| {
                    let dx = i64::from(*x - 969600);
                    let dy = i64::from(*y - 278400);
                    dx * dx + dy * dy
                })
            })
            .copied()
            .expect("al menos un spawn");
        let (first_vid, npc_x, npc_y, _wrace) = target;
        eprintln!("objetivo: mob {_wrace} vid {first_vid} en ({npc_x},{npc_y})");

        // El fake se MUEVE cerca del NPC (rango melee 300 units — battle.cpp:144-167):
        // MOVE en pasos <= 2000 units (envelope F5.1), dwTime anclado al reloj
        // del server (el handshake lo dio). Loop hasta el rango (tope 200
        // pasos — el mapa 41 es grande; si no llega, fail — no colgar).
        let t0 = std::time::Instant::now();
        let mut px = 969600i32;
        let mut py = 278400i32;
        let mut reached = false;
        for _ in 0..200 {
            let dx = npc_x - px;
            let dy = npc_y - py;
            let dist_sq = i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy);
            if dist_sq <= (300i64 * 300) {
                reached = true;
                break;
            }
            let step = (2000f64 / (dist_sq as f64).sqrt()).min(1.0);
            let nx = px + (f64::from(dx) * step) as i32;
            let ny = py + (f64::from(dy) * step) as i32;
            let mut mv = vec![0x07u8, 1, 0, 0]; // header + FUNC_MOVE
            mv.extend_from_slice(&nx.to_le_bytes());
            mv.extend_from_slice(&ny.to_le_bytes());
            mv.extend_from_slice(&(server_time + t0.elapsed().as_millis() as u32).to_le_bytes());
            conn.send(&mv).await.map_err(|e| format!("MOVE: {e}"))?;
            px = nx;
            py = ny;
        }
        assert!(reached, "el fake llegó al rango del NPC ({npc_x},{npc_y}) desde ({px},{py})");
        eprintln!("fake en ({px},{py}) — NPC en ({npc_x},{npc_y})");

        // CG_ATTACK (8 B): header 2 + bType 0 (normal) + victim_vid + 2 CRC.
        let attack = |vid: u32| -> Vec<u8> {
            let mut pkt = vec![0x02u8, 0];
            pkt.extend_from_slice(&vid.to_le_bytes());
            pkt.extend_from_slice(&[0u8, 0]);
            pkt
        };

        // Golpes con el cooldown del combate (1250 ms — battle.cpp:808-838):
        // el fake ataca hasta ver el GC_DEAD. El tope se dimensiona al HP del
        // objetivo conocido (20001/101: 120-126 HP, daño lvl-1 ~3-9 → 60
        // golpes = 78 s); el fallback (mob desconocido, p.ej. el 5001 de
        // 30000 HP) solo verifica el FLUJO del daño (≥ 5 golpes con daño).
        let hits_until_dead = match _wrace {
            20001 | 101 => 60,
            _ => 25,
        };
        let mut hits = 0;
        let mut died = false;
        while hits < hits_until_dead && !died {
            conn.send(&attack(first_vid)).await.map_err(|e| format!("CG_ATTACK: {e}"))?;
            // Lee la respuesta del golpe: GC_ATTACK (0x0C, 10 B) + [GC_DAMAGE_INFO (0x87, 10 B)] + [muerte].
            // En el golpe FINAL el canal manda GC_DEAD (0x0e) directo (sin GC_ATTACK).
            let hdr = read_exact_size(&mut conn, 1).await.map_err(|e| format!("rsp hdr: {e}"))?;
            match hdr[0] {
                0x0e => {
                    let _ = read_exact_size(&mut conn, 4).await.map_err(|e| format!("dead: {e}"))?;
                    let del = read_exact_size(&mut conn, 1).await.map_err(|e| format!("del hdr: {e}"))?;
                    assert_eq!(del[0], 2, "GC_CHARACTER_DEL tras GC_DEAD");
                    let _ = read_exact_size(&mut conn, 4).await.map_err(|e| format!("del: {e}"))?;
                    died = true;
                }
                0x0c => {
                    let _ = read_exact_size(&mut conn, 9).await.map_err(|e| format!("GcAttack: {e}"))?;
                    // El siguiente paquete: damage info o muerte.
                    let hdr2 = read_exact_size(&mut conn, 1).await.map_err(|e| format!("rsp2 hdr: {e}"))?;
                    match hdr2[0] {
                        0x87 => {
                            let _ = read_exact_size(&mut conn, 9).await.map_err(|e| format!("dmg: {e}"))?;
                            hits += 1;
                        }
                        14 => {
                            let _ = read_exact_size(&mut conn, 4).await.map_err(|e| format!("dead: {e}"))?;
                            // GC_CHARACTER_DEL (2, 5 B) tras el GC_DEAD.
                            let del = read_exact_size(&mut conn, 1).await.map_err(|e| format!("del hdr: {e}"))?;
                            assert_eq!(del[0], 2, "GC_CHARACTER_DEL tras GC_DEAD");
                            let _ = read_exact_size(&mut conn, 4).await.map_err(|e| format!("del: {e}"))?;
                            died = true;
                        }
                        other => return Err(format!("respuesta inesperada tras GcAttack: 0x{other:02x}")),
                    }
                }
                other => {
                    // Paquete del canal intercalado (ping 44/heartbeat) — leer y reintentar.
                    if other == 44 {
                        conn.send(&[0xfeu8]).await.map_err(|e| format!("pong: {e}"))?;
                        continue;
                    }
                    return Err(format!("header inesperado en el combate: 0x{other:02x}"));
                }
            }
            // Cooldown entre golpes (1250 ms del combate — battle.cpp:808).
            if !died {
                tokio::time::sleep(Duration::from_millis(1300)).await;
            }
        }
        if hits_until_dead > 25 {
            assert!(died, "el mob {_wrace} (vid {first_vid}) murió tras {hits} golpes (tope {hits_until_dead})");
        } else {
            assert!(hits >= 5, "el flujo del daño funciona (≥5 golpes con GC_DAMAGE_INFO, hubo {hits})");
            eprintln!("flujo de daño OK: {hits} golpes sin muerte (mob {_wrace} — HP desconocido/alto)");
        }
        if died {
            eprintln!("combate OK: mob {_wrace} vid {first_vid} muerto tras {hits} golpes");
        }

        // El NPC removido: atacar al mismo vid ya no produce daño (no existe
        // en la lista de NPCs vivos del canal) — solo si el mob murió.
        // El canal sigue vivo (pings 44/heartbeat): el assert es "no llega
        // daño" (0x87/0x0c) en 2 s, no el EOF (la conexión del jugador vive).
        if died {
            conn.send(&attack(first_vid)).await.map_err(|e| format!("CG_ATTACK 2: {e}"))?;
            let mut b = [0u8; 1];
            let n = tokio::time::timeout(
                Duration::from_millis(2000),
                conn.recv(&mut b),
            )
            .await;
            match n {
                // El comportamiento correcto: el vid muerto ya no está en la
                // lista de NPCs vivos del canal → NO llega daño en 2 s (la
                // conexión sigue viva, solo no daña). El timeout es el ÉXITO.
                Err(_) => {
                    eprintln!("vid muerto sin respuesta en 2 s: OK (no daña)");
                }
                Ok(n) => {
                    let n = n.map_err(|e| format!("recv tras ataque a vid muerto: {e}"))?;
                    assert_ne!(b[0], 0x87, "el canal NO daña al vid muerto");
                    assert_ne!(b[0], 0x0c, "el canal NO daña al vid muerto (GC_ATTACK)");
                    assert!(n > 0, "EOF: el canal cerró la conexión (vivo, no daña)");
                }
            }
        }
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    let log = std::fs::read_to_string(&log_path).unwrap_or_default();
    let _ = std::fs::remove_file(&log_path);
    if let Err(e) = result {
        eprintln!("--- log del canal combate ---\n{log}\n--- fin ---");
        panic!("combate contra un NPC del spawn: {e}");
    }
}
