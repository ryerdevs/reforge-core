//! `channel/script.rs` — el handler del CG_SCRIPT_ANSWER (R-s3): el REVIVE
//! del jugador (parity `cmd_general.cpp:534-554` — RestartAtSamePos o el
//! warp a la ciudad) y — desde el lane quest — la RESPUESTA del diálogo de
//! quest (el [NEXT]/[QUESTION] del GC_SCRIPT 45 → reanuda la quest
//! suspendida en el mundo).
//!
//! CG_SCRIPT_ANSWER (29, 2 B: header + answer BYTE — Packet.h:679). El
//! diálogo de muerte del cliente manda la respuesta; el C++ revive con
//! `RestartAtSamePos` (el mismo punto) o warpea a la ciudad
//! (`WarpSet EMPIRE_START`). El diálogo de quest (mismo paquete) solo puede
//! estar abierto VIVO — la distinción es el hp (parity del C++: el quest
//! manager reanuda la quest antes que el flujo de muerte).
//!
//! C6a (firma uniforme): sin muerte / answer no-muerto → log + Continue.

use game_core::ecs::{CombatIntent, Intent, QuestIntent};
use game_core::packets;

use crate::channel::parse_listen;
use crate::channel::session::{Outcome, Session};

/// DEATH PENALTY: `aiExpLossPercents` (constants.cpp:768-789) — el % del
/// next_exp que se pierde al morir, por nivel (índice 0 = lvl 0 → 0).
/// 5% lvl 1-10, 4% 11-30, 3% 31-40, 2% 41-60, 1% 61+ (el resto se repite).
const EXP_LOSS_PERCENT: [i32; 121] = [
    0, // 0
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, // 1-10
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, // 11-20
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, // 21-30
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, // 31-40
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, // 41-50
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, // 51-60
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 61-70
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 71-80
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 81-90
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 91-100
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 101-110
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 111-120
];

/// CG_SCRIPT_ANSWER (29): revive con la respuesta del diálogo de muerte —
/// answer 1 → GC_WARP a la ciudad (el cliente RECONECTA con el flujo
/// DirectEnter completo); si no → RestartAtSamePos (remove + insert del
/// personaje en su sitio).
pub async fn handle(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if session.row().hp <= 0 {
        let answer = pkt.get(1).copied().unwrap_or(0);
        revive(session, answer).await?;
    } else {
        // Diálogo de quest suspendido (CG_SCRIPT_ANSWER del GC_SCRIPT 45 —
        // el [NEXT]/[QUESTION] del quest dialog): la reanudación la resuelve
        // el mundo (`QuestIntent::Answer` — no-op si no hay quest suspendida;
        // el answer del select (1..n) se ata al capture `as name`).
        let answer = pkt.get(1).copied().unwrap_or(0);
        session.intent(Intent::Quest(QuestIntent::Answer {
            player_vid: session.player_vid(),
            answer,
        }))?;
        eprintln!(
            "server_realms: channel conn {}: respuesta de quest {answer} → mundo",
            session.conn_id
        );
    }
    Ok(Outcome::Continue)
}

/// REVIVE del jugador (compartido por el CG_SCRIPT_ANSWER del diálogo de
/// muerte y los comandos `/restart_here`/`/restart_town` — el C++ trata
/// ambos con el mismo flujo de do_restart, cmd_general.cpp:402-570).
///
/// SIEMPRE manda ANTES el GC_CHAT tipo CHAT_TYPE_COMMAND
/// "CloseRestartWindow" (parity cmd_general.cpp:460 — el cliente cierra la
/// ventana de muerte; sin él los botones se quedan).
///
/// `answer == 1` → revive EN LA CIUDAD (GC_WARP — el cliente reconecta con
/// DirectEnter; parity `WarpSet` de SCMD_RESTART_TOWN) y PERSISTE la
/// posición del village (el DirectEnter recarga la posición guardada).
/// Cualquier otro →
/// RestartAtSamePos (remove + insert en el mismo punto; parity
/// `ch->RestartAtSamePos()` + `PointChange(HP, 50-hp)` — el subset restaura
/// a los máximos, divergencia documentada). Restaura hp/mp a los máximos,
/// sincroniza el mundo COMPARTIDO, reenvía ADDITIONAL_INFO con los parts y
/// persiste.
pub async fn revive(session: &mut Session, answer: u8) -> Result<(), String> {
    // Cierra la ventana de muerte ANTES del revive (parity
    // `ch->ChatPacket(CHAT_TYPE_COMMAND, "CloseRestartWindow")` —
    // cmd_general.cpp:460, previo al WarpSet/RestartAtSamePos; el cliente la
    // despacha a `__RestartDialog_Close`, game.py:1874).
    session
        .send(&close_restart_window_packet(session.empire))
        .await
        .map_err(|e| format!("enviando GC_CHAT CloseRestartWindow: {e}"))?;
    // DEATH PENALTY (parity `INSTANT_FLAG_DEATH_PENALTY`,
    // char_battle.cpp:310-337): al morir se pierde
    // `MIN(800000, (GetNextExp() * __GetExpLossPerc(level)) / 100)` exp —
    // la tabla `aiExpLossPercents` (constants.cpp:768: 5% lvl 1-10, 4%
    // 11-30, 3% 31-40, 2% 41-60, 1% 61+). El revive EN LA CIUDAD (answer
    // 1 → `bTown`) NO pierde exp; RestartAtSamePos sí.
    if answer != 1 {
        let level = i32::from(session.row().level);
        let pct = EXP_LOSS_PERCENT[level.clamp(0, EXP_LOSS_PERCENT.len() as i32 - 1) as usize];
        let loss = ((session.next_exp.max(0) * i64::from(pct)) / 100).min(800_000);
        let conn_id = session.conn_id;
        let name = session.row().name.clone();
        let row = session.row_mut();
        row.exp = row.exp.saturating_sub(loss as i32);
        eprintln!(
            "server_realms: channel conn {conn_id}: {name} perdió {loss} exp al morir ({}% del next_exp)",
            pct
        );
    }
    // Restaurar hp/mp a los máximos del subset (parity
    // PointChange(POINT_HP, GetMaxHP()) — el revive del C++ restaura
    // antes de mostrar).
    let max = packets::compute_max_points(session.row()).unwrap_or([100, 100, 0]);
    {
        let row = session.row_mut();
        row.hp = max[0];
        row.mp = max[1];
    }
    session.save();
    // El mundo COMPARTIDO refleja el HP/SP restaurados (el daño del AI y
    // el coste de las skills los gastan de ahí).
    session.intent(Intent::Combat(CombatIntent::SetHp {
        player_vid: session.player_vid(),
        hp: session.row().hp,
    }))?;
    session.intent(Intent::Combat(CombatIntent::SetMp {
        player_vid: session.player_vid(),
        mp: session.row().mp,
    }))?;
    if answer == 1 {
        // Revive EN LA CIUDAD: GC_WARP — el cliente cierra la conexión y
        // RECONECTA con el flujo DirectEnter completo (RecvWarpPacket →
        // Connect(lAddr, wPort) — F4 ya lo sirve). Destino: el village fijo
        // del mapa 41 (969600, 278400 en UNITS) — parity del C++
        // `WarpSet(EMPIRE_START_X(empire), EMPIRE_START_Y(empire))` o
        // GetRecallPositionByEmpire (cmd_general.cpp:475/489/554). NO se usa
        // exit_x/exit_y (valen 960640,263099 — el punto de entrada, no el
        // village; el runtime actual tiene un solo mapa con village fijo).
        let (wx, wy) = (969_600, 278_400); // village c1 del mapa 41
        // BUG C26: el revive en la ciudad NO persistía el destino — el
        // `save()` copia el x/y desde el motion (session.rs:592-608), así
        // que el row guardaba la posición de la MUERTE y el DirectEnter del
        // GC_WARP recargaba donde murió. Parity C++: `WarpSet(x, y)` mueve
        // al personaje ANTES de persistir (char.cpp:5236-5238) — aquí se
        // actualiza row + motion y se guarda ANTES del GC_WARP.
        {
            let row = session.row_mut();
            row.x = wx;
            row.y = wy;
        }
        session.motion = Some(game_core::movement::initial(wx, wy));
        session.save();
        let (ip, port) = parse_listen(&session.config.listen)?;
        let addr = packets::ip_to_inet_addr(&ip)?;
        session
            .send(&protocol::world::TPacketGCWarp::new(wx, wy, addr, port).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_WARP: {e}"))?;
        eprintln!(
            "server_realms: channel conn {}: {} revivió EN LA CIUDAD \
             (answer {answer}) — GC_WARP {wx},{wy} (village mapa 41) → \
             {}:{port}, reconexión",
            session.conn_id,
            session.row().name,
            ip
        );
    } else {
        // RestartAtSamePos: remove + insert del personaje (el cliente
        // reinicia la instancia en su sitio).
        let vid = session.player_vid();
        session
            .send(&protocol::world::TPacketGCCharacterDelete::new(vid).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_CHARACTER_DEL: {e}"))?;
        session
            .send(&packets::character_add(session.row(), session.mov_speed).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_CHARACTER_ADD: {e}"))?;
        // ADDITIONAL_INFO con los parts computados del equipo (el revive
        // reinserta la instancia).
        let parts = packets::equipped_parts(session.row(), &session.inventory);
        let arrows = super::equipped_arrow_index(&session.inventory)
            .map(|i| session.inventory[i].count as u32)
            .unwrap_or(0);
        session
            .send(
                &packets::character_additional_info_with_parts(
                    session.row(),
                    session.empire,
                    &parts,
                    arrows,
                )
                .to_bytes(),
            )
            .await
            .map_err(|e| format!("enviando GC_CHARACTER_ADDITIONAL_INFO: {e}"))?;
        // GC_POINTS con hp/mp restaurados.
        session
            .send(
                &packets::points_packet(session.row(), session.next_exp, &session.battle)
                    .to_bytes(),
            )
            .await
            .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
        eprintln!(
            "server_realms: channel conn {}: {} REVIVIÓ (answer {answer}, \
             hp {}/{}, mp {}/{})",
            session.conn_id,
            session.row().name,
            session.row().hp,
            max[0],
            session.row().mp,
            max[1]
        );
    }
    Ok(())
}

/// `EChatType` (length.h:260-276): COMMAND — el cliente lo despacha a
/// `ServerCommand` → `__ServerCommand_Build` (PythonNetworkStreamPhaseGame.
/// cpp:1313; game.py:1874). OJO: NO es 9 — 9 es MONARCH_NOTICE (el enum
/// arranca en TALKING=0; el cliente solo rutea el tipo 5 al ServerCommand).
const CHAT_TYPE_COMMAND: u8 = 5;

/// GC_CHAT tipo CHAT_TYPE_COMMAND "CloseRestartWindow" (parity
/// `ChatPacket(CHAT_TYPE_COMMAND, "CloseRestartWindow")` — char.cpp:3928-3958).
/// Layout del GC_CHAT (Packet.h:1336-1343): header(4) + size(WORD, incluye
/// los 9 B fijos) + type + dwVID + bEmpire + payload SIN NUL. El C++ manda
/// `id = 0` (NO el vid — char.cpp:3947) y el payload es el texto CRUDO, sin
/// el prefijo "Name : " del chat (buf.write(chatbuf, len) —
/// char.cpp:3950-3952); el cliente parsea el payload como comando, así que
/// el prefijo lo rompería.
fn close_restart_window_packet(empire: u8) -> Vec<u8> {
    const TEXT: &[u8] = b"CloseRestartWindow";
    let mut out = Vec::with_capacity(9 + TEXT.len());
    out.push(protocol::header::GC_CHAT);
    out.extend_from_slice(&((9 + TEXT.len()) as u16).to_le_bytes());
    out.push(CHAT_TYPE_COMMAND);
    out.extend_from_slice(&0u32.to_le_bytes()); // id=0 — parity ChatPacket
    out.push(empire);
    out.extend_from_slice(TEXT);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use database::player::PlayerRow;
    use tokio::io::AsyncReadExt;

    /// Fila mínima del player: MUERTO (hp 0), con exit_x/exit_y = los valores
    /// reales del row (960640, 263099 — el punto de entrada, NO el village)
    /// para probar que el warp del revive ya NO los usa.
    fn dead_row() -> PlayerRow {
        PlayerRow {
            id: 1,
            name: "Revivor".into(),
            job: 0,
            voice: 0,
            dir: 0,
            x: 960_640,
            y: 263_099,
            z: 0,
            map_index: 41,
            exit_x: 960_640,
            exit_y: 263_099,
            exit_map_index: 0,
            hp: 0,
            mp: 0,
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

    /// Sesión de test (mismo patrón que channel/chat.rs): par de sockets
    /// localhost (el lado `client_side` lee lo que la sesión envía), pool sin
    /// conectar (revive no toca PG — `save()` con store None retorna temprano)
    /// y `listen` por defecto ("127.0.0.1:30001" — lo que parsea el GC_WARP).
    /// Devuelve el RECEIVER de intents: revive manda SetHp/SetMp al mundo y
    /// sin él (caído) el envío falla — el test lo mantiene vivo con `_rx`.
    async fn test_session(
        vid: u32,
    ) -> (
        Session,
        tokio::net::TcpStream,
        tokio::sync::mpsc::UnboundedReceiver<game_core::ecs::Intent>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind localhost");
        let addr = listener.local_addr().expect("addr");
        let client_side = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (server_side, _peer) = listener.accept().await.expect("accept");
        let pool = database::pool::new_pool("host=localhost dbname=metin2", 2)
            .expect("pool sin conectar (lazy)");
        let wal_dir = std::env::temp_dir()
            .join(format!("script_test_wal_{}", std::process::id()))
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
        let (intent_tx, intent_rx) = tokio::sync::mpsc::unbounded_channel();
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
        s.row = Some(dead_row());
        s.motion = Some(game_core::movement::initial(960_640, 263_099));
        (s, client_side, intent_rx)
    }

    /// Lee UN paquete S→C size-prefixed (GC_CHAT: header + size WORD + resto).
    async fn read_packet(sock: &mut tokio::net::TcpStream) -> Vec<u8> {
        let mut hdr = [0u8; 3];
        sock.read_exact(&mut hdr).await.expect("paquete del server");
        let size = u16::from_le_bytes([hdr[1], hdr[2]]) as usize;
        let mut body = vec![0u8; size - 3];
        sock.read_exact(&mut body)
            .await
            .expect("cuerpo del paquete");
        let mut pkt = hdr.to_vec();
        pkt.extend_from_slice(&body);
        pkt
    }

    /// BUG 1 + BUG 2 (answer 1 — revive en la ciudad): el PRIMER paquete es
    /// el GC_CHAT CHAT_TYPE_COMMAND "CloseRestartWindow" (cierra la ventana de
    /// muerte — parity cmd_general.cpp:460) y el GC_WARP va al village FIJO
    /// del mapa 41 (969600, 278400), NO a exit_x/exit_y (960640, 263099).
    #[tokio::test]
    async fn revive_town_sends_close_restart_then_warp_village() {
        let (mut s, mut sock, _rx) = test_session(301).await;
        revive(&mut s, 1).await.expect("revive OK");
        // GC_CHAT "CloseRestartWindow" (byte-exacto — char.cpp:3928-3958).
        let cmd = read_packet(&mut sock).await;
        assert_eq!(cmd[0], protocol::header::GC_CHAT, "GC_CHAT (header 4)");
        assert_eq!(
            u16::from_le_bytes([cmd[1], cmd[2]]) as usize,
            cmd.len(),
            "size = 9 + payload"
        );
        assert_eq!(cmd[3], CHAT_TYPE_COMMAND, "type = CHAT_TYPE_COMMAND (5)");
        assert_eq!(
            u32::from_le_bytes(cmd[4..8].try_into().unwrap()),
            0,
            "id = 0 — parity ChatPacket (char.cpp:3947), no el vid"
        );
        assert_eq!(cmd[8], 1, "bEmpire");
        assert_eq!(&cmd[9..], b"CloseRestartWindow", "payload crudo sin NUL");
        // GC_WARP al village del mapa 41 (15 B fijos, header 65).
        let mut warp = [0u8; protocol::world::TPacketGCWarp::SIZE];
        sock.read_exact(&mut warp).await.expect("GC_WARP");
        assert_eq!(warp[0], protocol::world::TPacketGCWarp::HEADER, "GC_WARP");
        assert_eq!(
            i32::from_le_bytes(warp[1..5].try_into().unwrap()),
            969_600,
            "x = village c1 mapa 41 (no exit_x 960640)"
        );
        assert_eq!(
            i32::from_le_bytes(warp[5..9].try_into().unwrap()),
            278_400,
            "y = village c1 mapa 41 (no exit_y 263099)"
        );
        // BUG C26: la posición del row quedó en el village (el DirectEnter
        // del GC_WARP recarga la posición GUARDADA — el save del revive
        // debe persistir el destino, no la muerte).
        assert_eq!(s.row().x, 969_600, "row.x persistido = village c1");
        assert_eq!(s.row().y, 278_400, "row.y persistido = village c1");
        assert_eq!(
            s.motion().x,
            969_600,
            "motion.x = village (el save copia del motion)"
        );
        assert_eq!(
            s.motion().y,
            278_400,
            "motion.y = village (el save copia del motion)"
        );
    }

    /// BUG 1 (answer 0 — RestartAtSamePos): el GC_CHAT "CloseRestartWindow"
    /// también viaja ANTES del remove/insert (parity cmd_general.cpp:460 —
    /// el comando se manda en AMBOS paths de do_restart).
    #[tokio::test]
    async fn revive_here_sends_close_restart_before_restart_at_same_pos() {
        let (mut s, mut sock, _rx) = test_session(302).await;
        revive(&mut s, 0).await.expect("revive OK");
        // 1º: GC_CHAT "CloseRestartWindow".
        let cmd = read_packet(&mut sock).await;
        assert_eq!(cmd[0], protocol::header::GC_CHAT, "GC_CHAT primero");
        assert_eq!(cmd[3], CHAT_TYPE_COMMAND);
        assert_eq!(&cmd[9..], b"CloseRestartWindow");
        // 2º: GC_CHARACTER_DEL (5 B, header 2) + GC_CHARACTER_ADD (37 B,
        // header 1) + ADDITIONAL_INFO (70 B, header 136) + GC_POINTS
        // (1021 B, header 16).
        let mut del = [0u8; protocol::world::TPacketGCCharacterDelete::SIZE];
        sock.read_exact(&mut del).await.expect("GC_CHARACTER_DEL");
        assert_eq!(del[0], protocol::world::TPacketGCCharacterDelete::HEADER);
        assert_eq!(
            u32::from_le_bytes(del[1..5].try_into().unwrap()),
            1,
            "vid = row.id"
        );
        let mut add = [0u8; 37];
        sock.read_exact(&mut add).await.expect("GC_CHARACTER_ADD");
        assert_eq!(add[0], protocol::TPacketGCCharacterAdd::HEADER);
        let mut info = [0u8; 70];
        sock.read_exact(&mut info).await.expect("ADDITIONAL_INFO");
        assert_eq!(info[0], protocol::TPacketGCCharacterAdditionalInfo::HEADER);
        let mut points = [0u8; protocol::world::TPacketGCPoints::SIZE];
        sock.read_exact(&mut points).await.expect("GC_POINTS");
        assert_eq!(points[0], protocol::world::TPacketGCPoints::HEADER);
    }

    /// BUG 1 (wiring del CG_SCRIPT_ANSWER 29): el handler del diálogo de
    /// muerte (muerto) reviva — primer paquete CloseRestartWindow.
    #[tokio::test]
    async fn handle_dead_answer_revives_with_close_restart_first() {
        let (mut s, mut sock, _rx) = test_session(303).await;
        let pkt = vec![protocol::header::CG_SCRIPT_ANSWER, 1];
        let outcome = handle(&mut s, &pkt).await.expect("handle OK");
        assert_eq!(outcome, Outcome::Continue);
        let cmd = read_packet(&mut sock).await;
        assert_eq!(cmd[0], protocol::header::GC_CHAT);
        assert_eq!(cmd[3], CHAT_TYPE_COMMAND);
        assert_eq!(&cmd[9..], b"CloseRestartWindow");
    }
}
