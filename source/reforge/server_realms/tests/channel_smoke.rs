//! Smoke F4 slice 2: fake client legacy contra el channel REAL
//! (`server_realms --role channel`) como subproceso — valida el wire del
//! flujo login SIN PostgreSQL (el config apunta a un puerto muerto → la
//! validación de credenciales devuelve GC_LOGIN_FAILURE "NOTAVAIL",
//! divergencia documentada del canal Rust: el C++ con el db caído no responde).
//!
//! Flujo verificado (2026-08-14 — SIN handshake del canal): GC_PHASE(LOGIN)
//! DIRECTO → CG_LOGIN3 (65 B, canal) → GC_LOGIN_FAILURE (10 B, "NOTAVAIL").
//! El guild mark (CG_MARK_LOGIN 0x64, 9 B — conexión paralela) → cierre
//! limpio sin responder (parity input.cpp:560-572).

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use network::{Connection, read_exact_size};
use protocol::{TPacketCGLogin3, TPacketGCPhase, phase};
use tokio::net::TcpStream;

/// Config temporal POR TEST (patrón auth_smoke — los tests del bin corren en
/// paralelo; un archivo común sería un race).
fn write_temp_config(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("f4_channel_smoke_{name}.toml"));
    // PG caído a propósito: 127.0.0.1:1 (nadie escucha) → NOTAVAIL.
    let toml = "listen = \"127.0.0.1:0\"\n\
                pg_conn = \"host=127.0.0.1 port=1 user=mt2 password=mt2 dbname=metin2\"\n\
                timeout_ms = 15000\n\
                no_more_clients = false\n";
    std::fs::write(&path, toml).expect("escribir config temporal");
    path
}

/// Arranca el channel y espera la línea `channel escuchando en addr` (stdout).
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
            if let Some(addr) = line
                .trim()
                .strip_prefix("server_realms: channel escuchando en ")
            {
                let _ = tx.send(addr.to_string());
                break;
            }
        }
    });
    let addr = rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("el channel no anunció el listener (stdout): ¿arrancó?"));
    (child, addr)
}

/// Nuevo contrato (2026-08-14 — sin handshake del canal): al conectar, el
/// canal manda GC_PHASE(LOGIN) DIRECTO (el paquete que dispara el LOGIN3 del
/// cliente — PythonNetworkStream.cpp:597-599). Antes había que responder el
/// handshake (GC_PHASE(HANDSHAKE) + eco CG_HANDSHAKE); ya no existe en el
/// canal (el AUTH lo mantiene).
async fn read_login_phase(conn: &mut Connection<TcpStream>) {
    let phase_pkt = read_exact_size(conn, TPacketGCPhase::SIZE)
        .await
        .expect("GC_PHASE");
    let gc_phase = TPacketGCPhase::from_bytes(&phase_pkt).expect("parse GC_PHASE");
    assert_eq!(gc_phase.phase, phase::LOGIN, "GC_PHASE(LOGIN) directo");
}

#[tokio::test]
async fn channel_handles_login3_with_db_down() {
    let config_path = write_temp_config("login3");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("connect {addr}: {e}"))?;
        let mut conn = Connection::new(stream);
        read_login_phase(&mut conn).await;

        // LOGIN3 al canal = 65 B (sin lang; framer rol Channel).
        let login3 = TPacketCGLogin3::new_channel("test", "1234", [0; 4]).to_bytes_channel();
        assert_eq!(login3.len(), 65, "LOGIN3 del canal");
        conn.send(&login3)
            .await
            .map_err(|e| format!("LOGIN3: {e}"))?;

        // Respuesta: GC_LOGIN_FAILURE (10 B) con la DB caída — NOTAVAIL
        // (divergencia documentada del canal Rust).
        let hdr = read_exact_size(&mut conn, 1)
            .await
            .map_err(|e| format!("header: {e}"))?;
        assert_eq!(hdr[0], 0x07, "GC_LOGIN_FAILURE (0x07)");
        let rest = read_exact_size(&mut conn, 9)
            .await
            .map_err(|e| format!("resto: {e}"))?;
        let mut pkt = hdr;
        pkt.extend_from_slice(&rest);
        let status = pkt[1..]
            .iter()
            .take_while(|&&b| b != 0)
            .copied()
            .collect::<Vec<u8>>();
        assert_eq!(status, b"NOTAVAIL", "DB caída -> NOTAVAIL (determinista)");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("flujo channel con DB caída");
}

/// El LOGIN3 puede llegar SIN esperar el GC_PHASE(LOGIN) del server (el
/// cliente manda su login al procesar la fase — SetLoginPhase →
/// SendLoginPacket inmediato, PythonNetworkStreamPhaseLogin.cpp:85-138).
/// El canal (sin handshake — 2026-08-14) lo procesa igual: la respuesta es
/// GC_LOGIN_FAILURE (0x07), NO un reenvío de GC_PHASE(LOGIN) que re-dispararía
/// el LOGIN3 del cliente.
#[tokio::test]
async fn channel_login3_immediate_is_processed_not_dropped() {
    let config_path = write_temp_config("login3immediate");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("connect {addr}: {e}"))?;
        let mut conn = Connection::new(stream);

        // LOGIN3 del canal (65 B) INMEDIATO — sin leer el GC_PHASE(LOGIN)
        // (el server lo envía al aceptar; el cliente no tiene por qué leerlo
        // antes de mandar su login — caso del cliente real intermitente).
        let login3 = TPacketCGLogin3::new_channel("test", "1234", [0; 4]).to_bytes_channel();
        assert_eq!(login3.len(), 65, "LOGIN3 del canal");
        conn.send(&login3)
            .await
            .map_err(|e| format!("LOGIN3: {e}"))?;

        // El canal procesa el login directo → PG caída → NOTAVAIL. El server
        // manda GC_PHASE(LOGIN) AL ACEPTAR (antes de leer el LOGIN3): el
        // cliente lo consume y la respuesta real es GC_LOGIN_FAILURE (0x07).
        let phase_pkt = read_exact_size(&mut conn, TPacketGCPhase::SIZE)
            .await
            .map_err(|e| format!("GC_PHASE: {e}"))?;
        let gc_phase = TPacketGCPhase::from_bytes(&phase_pkt).map_err(|e| format!("parse: {e}"))?;
        assert_eq!(gc_phase.phase, phase::LOGIN, "GC_PHASE(LOGIN) del accept");
        let hdr = read_exact_size(&mut conn, 1)
            .await
            .map_err(|e| format!("header: {e}"))?;
        assert_eq!(hdr[0], 0x07, "GC_LOGIN_FAILURE (0x07)");
        let rest = read_exact_size(&mut conn, 9)
            .await
            .map_err(|e| format!("resto: {e}"))?;
        let mut pkt = hdr;
        pkt.extend_from_slice(&rest);
        let status = pkt[1..]
            .iter()
            .take_while(|&&b| b != 0)
            .copied()
            .collect::<Vec<u8>>();
        assert_eq!(
            status, b"NOTAVAIL",
            "DB caída -> NOTAVAIL (login inmediato)"
        );
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("flujo channel con LOGIN3 inmediato");
}

/// El cliente LENTO (2026-08-14): sin handshake ya no hay carrera de 32
/// intentos — el canal manda GC_PHASE(LOGIN) al aceptar y espera el LOGIN3
/// con el timeout del config. Un LOGIN3 TARDÍO (segundos después de conectar)
/// se procesa igual: NOTAVAIL con la PG caída.
#[tokio::test]
async fn channel_slow_client_late_login3_is_processed() {
    let config_path = write_temp_config("slowclient");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("connect {addr}: {e}"))?;
        let mut conn = Connection::new(stream);

        // GC_PHASE(LOGIN) directo (sin handshake previo).
        let phase_pkt = read_exact_size(&mut conn, TPacketGCPhase::SIZE)
            .await
            .map_err(|e| format!("GC_PHASE: {e}"))?;
        let gc_phase = TPacketGCPhase::from_bytes(&phase_pkt).map_err(|e| format!("parse: {e}"))?;
        assert_eq!(gc_phase.phase, phase::LOGIN, "phase LOGIN directo");

        // Espera deliberada (cliente lento) antes del LOGIN3.
        tokio::time::sleep(Duration::from_secs(2)).await;

        let login3 = TPacketCGLogin3::new_channel("test", "1234", [0; 4]).to_bytes_channel();
        conn.send(&login3)
            .await
            .map_err(|e| format!("LOGIN3: {e}"))?;

        // El flujo de login normal -> PG caída -> NOTAVAIL.
        let hdr = read_exact_size(&mut conn, 1)
            .await
            .map_err(|e| format!("header: {e}"))?;
        assert_eq!(hdr[0], 0x07, "GC_LOGIN_FAILURE (0x07)");
        let rest = read_exact_size(&mut conn, 9)
            .await
            .map_err(|e| format!("resto: {e}"))?;
        let mut pkt = hdr;
        pkt.extend_from_slice(&rest);
        let status = pkt[1..]
            .iter()
            .take_while(|&&b| b != 0)
            .copied()
            .collect::<Vec<u8>>();
        assert_eq!(status, b"NOTAVAIL", "DB caída -> NOTAVAIL (cliente lento)");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("flujo channel con cliente lento");
}

/// El GUILD MARK (2026-08-14, sin handshake): el cliente abre una conexión
/// SEPARADA en paralelo al select y manda CG_MARK_LOGIN (0x64, 9 B) como
/// primer paquete (GuildMarkDownloader.cpp:213-229). El canal normal
/// (`guild_mark_server` OFF) la cierra SIN responder (parity
/// input.cpp:560-572) — el cliente no lo interpreta como fallo (el mark es
/// opcional). El GC_PHASE(LOGIN) enviado al aceptar se ignora.
#[tokio::test]
async fn channel_guild_mark_connection_closed_clean() {
    let config_path = write_temp_config("marklogin");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("connect {addr}: {e}"))?;
        let mut conn = Connection::new(stream);

        // El GC_PHASE(LOGIN) del accept (2 B) llega SIEMPRE primero — se
        // consume antes de mandar el mark (el C++ también lo manda al aceptar;
        // el downloader lo ignora — su __LoginState_RecvPhase solo responde
        // si manda el índice después).
        read_login_phase(&mut conn).await;

        // CG_MARK_LOGIN (9 B, header 0x64) como PRIMER paquete — sin handshake.
        let mark = protocol::world::TPacketCGMarkLogin {
            header: 100,
            handle: 0xDEAD_BEEF,
            random_key: 0xCAFE_BABE,
        };
        conn.send(&mark.to_bytes())
            .await
            .map_err(|e| format!("CG_MARK_LOGIN: {e}"))?;

        // El canal cierra SIN responder: el read devuelve EOF (0 bytes) o
        // error de conexión cerrada — NUNCA un paquete de respuesta.
        let mut buf = [0u8; 2];
        let n = conn
            .recv(&mut buf)
            .await
            .map_err(|e| format!("recv: {e}"))?;
        assert_eq!(
            n, 0,
            "cierre limpio sin respuesta (parity input.cpp:560-572)"
        );
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("flujo channel con guild mark");
}

/// El chequeo de estado del canal (2026-08-14 — ServerStateChecker.cpp:43-69):
/// al seleccionar el servidor, el cliente abre UNA conexión PARALELA y manda
/// CG_STATE_CHECKER (0xce, 1 B) como PRIMER paquete. El canal RESPONDE
/// GC_RESPOND_CHANNELSTATUS (0xd2 — parity input.cpp:573-589 +
/// input_db.cpp:2433-2461): [0xd2][nSize=1 i32][port u16][status u8][0x01].
/// El cliente hace Initialize()/Disconnect al recibirla.
#[tokio::test]
async fn channel_state_checker_responds() {
    let config_path = write_temp_config("statechecker");
    let (mut child, addr) = spawn_channel(&config_path);

    let result = async {
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("connect {addr}: {e}"))?;
        let mut conn = Connection::new(stream);
        read_login_phase(&mut conn).await;

        // CG_STATE_CHECKER (0xce, 1 B — solo header).
        conn.send(&[0xce])
            .await
            .map_err(|e| format!("CG_STATE_CHECKER: {e}"))?;

        // Respuesta de 9 B: header + nSize + port + status + bSuccess.
        let resp = read_exact_size(&mut conn, 9)
            .await
            .map_err(|e| format!("respuesta: {e}"))?;
        assert_eq!(resp[0], 0xd2, "GC_RESPOND_CHANNELSTATUS");
        assert_eq!(
            i32::from_le_bytes([resp[1], resp[2], resp[3], resp[4]]),
            1,
            "nSize 1"
        );
        assert_eq!(resp[7], 1, "status recomendado (STATE_DICT[1])");
        assert_eq!(resp[8], 1, "bSuccess");

        // El puerto del estado = el puerto real del channel (listen efímero).
        let port = addr
            .rsplit(':')
            .next()
            .unwrap()
            .parse::<u16>()
            .expect("puerto del addr");
        assert_eq!(
            u16::from_le_bytes([resp[5], resp[6]]),
            port,
            "puerto del canal"
        );

        // El canal cierra tras responder (el cliente desconecta al recibirla).
        let mut buf = [0u8; 1];
        let n = conn
            .recv(&mut buf)
            .await
            .map_err(|e| format!("recv: {e}"))?;
        assert_eq!(n, 0, "cierre tras la respuesta");
        Ok::<(), String>(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&config_path);
    result.expect("flujo channel con state checker");
}
