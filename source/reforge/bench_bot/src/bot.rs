//! El bot wire-level: una sesión completa auth → canal → select → mundo →
//! loop de juego (MOVE + PONG), hablando el protocolo legacy REAL con los
//! codecs del crate `protocol` (parity del cliente v40999 — flujo del spec
//! `docs/reference/protocol/login-flow.md` §4, verificado contra
//! `server_realms/src/{auth,channel}.rs` 2026-08-13).
//!
//! # Reloj del bot (anti-speedhack)
//!
//! El canal valida el `dw_time` de cada CG_MOVE contra SU reloj
//! (`game_core::movement::process_move` — `input_main.cpp:1494-1516`): `i_delta`
//! fuera de [−server_delta/50, 30000) → kick. El bot ancla su reloj al del
//! server con el `GC_HANDSHAKE` (el `dwTime` del server) y manda los MOVE con
//! `est_server − 500 ms` de margen: nunca adelantado (no FastTimer) y
//! +500 ms ≪ 30000 (no SlowTimer), aunque el offset derive ~unos ms.
//!
//! # Hitos medidos (ms desde el connect del auth)
//!
//! - `auth_ms`: connect → `GC_AUTH_SUCCESS` (bResult=1).
//! - `channel_login_ms`: connect canal → 449 B (`GC_LOGIN_SUCCESS_NEWSLOT`).
//! - `select_ms`: `CG_PLAYER_SELECT` enviado → `GC_PHASE(GAME)`.
//! - `world_ms` = la métrica "login→world" del F5 (total hasta `GC_PHASE(GAME)`).
//! - `spawns`: `GC_CHARACTER_ADD` en fase game (mobs visibles — dimensión de
//!   densidad del benchmark; el canal materializa los mobs del radio
//!   `SPAWN_VIEW` vía la cola del mundo compartido — la interpretación de la
//!   visibilidad es de la lane del canal).
//!
//! # Paseo dentro del envelope (F5.4, ADR-0011)
//!
//! El canal RECHAZA los MOVE que exceden el envelope por entidad
//! (`game_core/src/movement.rs:146-151`): `allowed = speed × (dt_ms + 100) / 1000
//! × 1.20` con speed 300 u/s. El cuadrado antiguo (paso 300 @ 1000 ms) tenía
//! patas diagonales/retorno de 424/600 u > 396 permitidos → el server
//! rechazaba TODO MOVE tras el primero (`chan7.err.log`: "fuera del envelope
//! (speed 300) — rechazado") y los bots nunca se movían físicamente. El paseo
//! actual es un ping-pong en el eje X: cada MOVE está a `step` units EXACTOS
//! del anterior (nunca diagonales), con `step = walk_speed × intervalo / 1000`
//! y `walk_speed` default 200 u/s → paso 200 u, margen 1.98× sobre el
//! envelope a dt=1000 ms (sigue dentro hasta dt ≈ 455 ms — aguanta el retardo
//! de procesado del server bajo carga). `--walk-speed > 300` ejercita el
//! rechazo deliberadamente (test negativo — el rechazo NO es kick, se ve en
//! el log del canal, no en el status del bot).

use std::io;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use protocol::header;
use protocol::movement::TPacketCGMove;
use protocol::phase;
use protocol::world::TPacketGCMainCharacter;
use protocol::{
    TPacketCGHandshake, TPacketCGLogin3, TPacketCGPlayerSelect, TPacketGCAuthSuccess,
    TPacketGCEmpire, TPacketGCHandshake, TPacketGCLoginFailure, TPacketGCLoginSuccess,
};
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;

use crate::report::{BotReport, Status};
use crate::splitter::{SplitError, Splitter, GC_CHANNEL_LIST};

/// Configuración de un bot (una sesión).
#[derive(Debug, Clone)]
pub struct BotConfig {
    pub auth_addr: String,
    pub channel_addr: String,
    pub login: String,
    pub password: String,
    /// Duración del loop de juego (después el bot se desconecta limpio).
    pub duration: Duration,
    /// Intervalo entre CG_MOVE (el "walk around").
    pub move_interval: Duration,
    /// Velocidad del paseo en units/s (default 200 — dentro del envelope del
    /// server; ver [`walk_step`]).
    pub walk_speed: u32,
    /// Timeout de silencio por fase (connect/auth/select/entry/game — el knob
    /// `--timeout-s`; evita que un bot cuelgue para siempre con PG caído).
    pub timeout: Duration,
}

/// Timeout de fase por defecto (20 s — el knob `--timeout-s` lo reemplaza;
/// el default vive en `main.rs` como `DEFAULT_TIMEOUT_S`).
/// Margen del dw_time de los MOVE (ver doc del módulo).
const CLOCK_MARGIN_MS: i64 = 500;

/// Fase del fallo (para clasificar el status del reporte).
#[derive(Debug, Clone, Copy)]
enum Stage {
    Auth,
    ChannelLogin,
    Game,
}

#[derive(Debug)]
enum BotError {
    Connect(String, io::Error),
    Timeout(&'static str),
    /// `GC_LOGIN_FAILURE` del server (status legacy: NOID/ALREADY/...).
    LoginFailure(String),
    /// `GC_AUTH_SUCCESS` con bResult=0.
    AuthFailed,
    /// La cuenta no tiene personaje en ningún slot del 449 B.
    NoCharacter,
    /// Header fuera de la tabla S→C (drift de protocolo).
    Desync(u8),
    /// EOF del server.
    Eof,
    /// Error de parseo de un paquete conocido.
    Protocol(&'static str, String),
    /// Paquete inesperado en la fase.
    Unexpected(Stage, String),
}

impl From<SplitError> for BotError {
    fn from(e: SplitError) -> Self {
        match e {
            SplitError::UnknownHeader { header } => BotError::Desync(header),
            SplitError::Eof | SplitError::UnexpectedEof { .. } => BotError::Eof,
            SplitError::Io(e) => BotError::Connect("socket".into(), e),
            SplitError::BadEmbeddedLength { header, .. } => BotError::Desync(header),
        }
    }
}

/// Reloj del bot anclado al del server (ms desde el boot del server).
#[derive(Default)]
struct ServerClock {
    offset_ms: i64,
    synced: bool,
}

impl ServerClock {
    /// Ancla el reloj con el `dwTime` del `GC_HANDSHAKE` recibido.
    fn observe(&mut self, server_time: u32, local_ms: u64) {
        self.offset_ms = i64::from(server_time) - local_ms as i64;
        self.synced = true;
    }

    /// Estimación del reloj del server (ms).
    fn server_ms(&self, local_ms: u64) -> u64 {
        if self.synced {
            (local_ms as i64 + self.offset_ms) as u64
        } else {
            local_ms
        }
    }

    /// `dw_time` para los MOVE: est_server − margen (ver doc del módulo).
    fn move_time_ms(&self, local_ms: u64) -> u32 {
        self.server_ms(local_ms).saturating_sub(CLOCK_MARGIN_MS as u64) as u32
    }
}

/// Reloj local monotónico (ms desde el arranque del bot — misma forma que el
/// `now_ms()` del server, channel.rs:1922).
fn local_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Contadores de bytes/paquetes de la sesión.
#[derive(Default)]
struct Counts {
    rx_packets: u64,
    rx_bytes: u64,
    tx_packets: u64,
    tx_bytes: u64,
}

impl Counts {
    fn rx(&mut self, p: &[u8]) {
        self.rx_packets += 1;
        self.rx_bytes += p.len() as u64;
    }
    fn tx(&mut self, p: &[u8]) {
        self.tx_packets += 1;
        self.tx_bytes += p.len() as u64;
    }
}

/// Clave de cliente del LOGIN3: determinista por login (el server la guarda
/// para LOGIN_BY_KEY; con legacy vacío no se usa en el wire).
fn client_key(login: &str) -> [u32; 4] {
    let mut h = 0x811c_9dc5u32;
    for b in login.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    [h, h.wrapping_add(1), h.wrapping_add(2), h.wrapping_add(3)]
}

/// Lee el siguiente paquete S→C con timeout de fase. Los bytes ya leídos
/// quedan en el buffer del splitter si el timeout dispara (sin pérdida).
async fn recv<R: AsyncRead + Unpin>(
    rd: &mut R,
    sp: &mut Splitter,
    tout: Duration,
    label: &'static str,
) -> Result<Vec<u8>, BotError> {
    tokio::time::timeout(tout, sp.next(rd))
        .await
        .map_err(|_| BotError::Timeout(label))?
        .map_err(BotError::from)
}

async fn send(wr: &mut OwnedWriteHalf, data: &[u8], counts: &mut Counts) -> Result<(), BotError> {
    wr.write_all(data)
        .await
        .map_err(|e| BotError::Unexpected(Stage::Game, format!("send: {e}")))?;
    counts.tx(data);
    Ok(())
}

async fn connect(addr: &str, timeout: Duration) -> Result<TcpStream, BotError> {
    tokio::time::timeout(timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| BotError::Timeout("connect"))?
        .map_err(|e| BotError::Connect(addr.to_string(), e))
}

/// Handshake C→S (parity del cliente): eco del nonce con el reloj alineado
/// (`dw_time` = reloj del server, `l_delta` = 0 → bias ≈ RTT ≤ 80 ms).
async fn client_handshake(
    rd: &mut OwnedReadHalf,
    wr: &mut OwnedWriteHalf,
    sp: &mut Splitter,
    clock: &mut ServerClock,
    timeout: Duration,
) -> Result<(), BotError> {
    let p = recv(rd, sp, timeout, "handshake_phase").await?;
    if p[0] != header::GC_PHASE || p[1] != phase::HANDSHAKE {
        return Err(BotError::Unexpected(Stage::Auth, format!("esperando GC_PHASE(HANDSHAKE), got 0x{:02x}", p[0])));
    }
    let p = recv(rd, sp, timeout, "handshake").await?;
    if p[0] != header::GC_HANDSHAKE {
        return Err(BotError::Unexpected(Stage::Auth, format!("esperando GC_HANDSHAKE, got 0x{:02x}", p[0])));
    }
    let hs = TPacketGCHandshake::from_bytes(&p)
        .map_err(|e| BotError::Protocol("GC_HANDSHAKE", e.to_string()))?;
    clock.observe(hs.dw_time, local_ms());
    send(wr, &TPacketCGHandshake::new(hs.dw_handshake, hs.dw_time, 0).to_bytes(), &mut Counts::default()).await?;
    Ok(())
}

/// Fase auth (:30001): handshake → `GC_PHASE(AUTH)` → LOGIN3 (68 B, lang
/// "es") → `GC_AUTH_SUCCESS` (bResult=1).
async fn auth_phase(cfg: &BotConfig, clock: &mut ServerClock, counts: &mut Counts) -> Result<(), BotError> {
    let stream = connect(&cfg.auth_addr, cfg.timeout).await?;
    let (mut rd, mut wr) = stream.into_split();
    let mut sp = Splitter::new();
    client_handshake(&mut rd, &mut wr, &mut sp, clock, cfg.timeout).await?;

    // `GC_PHASE(AUTH)` → el cliente manda el LOGIN3 (parity
    // AccountConnector.cpp `__AuthState_RecvPhase` — sin él el server espera).
    loop {
        let p = recv(&mut rd, &mut sp, cfg.timeout, "auth_phase").await?;
        counts.rx(&p);
        match p[0] {
            header::GC_PHASE if p[1] == phase::AUTH => break,
            // Reenvíos del handshake (retry del server) — tolerar.
            header::GC_PHASE => continue,
            header::CG_TIME_SYNC | header::CG_PONG => continue,
            other => {
                return Err(BotError::Unexpected(Stage::Auth, format!("header 0x{other:02x} esperando PHASE_AUTH")))
            }
        }
    }
    send(
        &mut wr,
        &TPacketCGLogin3::new_auth(&cfg.login, &cfg.password, client_key(&cfg.login), "es").to_bytes_auth(),
        counts,
    )
    .await?;

    // Respuesta: [151-153 legacy] → [164 channel list] → 150 (o 7).
    loop {
        let p = recv(&mut rd, &mut sp, cfg.timeout, "auth_result").await?;
        counts.rx(&p);
        match p[0] {
            header::GC_AUTH_SUCCESS => {
                let a = TPacketGCAuthSuccess::from_bytes(&p)
                    .map_err(|e| BotError::Protocol("GC_AUTH_SUCCESS", e.to_string()))?;
                if a.b_result == 1 {
                    return Ok(());
                }
                return Err(BotError::AuthFailed);
            }
            header::GC_LOGIN_FAILURE => {
                let f = TPacketGCLoginFailure::from_bytes(&p)
                    .map_err(|e| BotError::Protocol("GC_LOGIN_FAILURE", e.to_string()))?;
                return Err(BotError::LoginFailure(f.status().into_owned()));
            }
            // Aditivos en login exitoso (auth.rs:366-378): ignorar.
            151 | 152 | 153 | GC_CHANNEL_LIST => continue,
            other => return Err(BotError::Desync(other)),
        }
    }
}

/// Fase canal (:30003): GC_PHASE(LOGIN) DIRECTO (sin handshake del canal -
/// 2026-08-14: el canal ya no handshakea; el reloj queda anclado por el auth) -
/// GC_EMPIRE + GC_PHASE(SELECT) + 449 B -> CG_PLAYER_SELECT.
/// mundo continúa en la MISMA conexión) y `(slot, x, y)` del personaje
/// elegido (el primero no vacío).
#[allow(clippy::type_complexity)]
async fn channel_phase(
    cfg: &BotConfig,
    counts: &mut Counts,
) -> Result<(u8, i32, i32, OwnedReadHalf, OwnedWriteHalf, Splitter), BotError> {
    let stream = connect(&cfg.channel_addr, cfg.timeout).await?;
    let (mut rd, mut wr) = stream.into_split();
    let mut sp = Splitter::new();
    // SIN handshake del canal (SOLUCIÓN DEFINITIVA 2026-08-14 — el cliente
    // real conecta con Connect() crudo y manda su LOGIN3 al procesar la fase
    // Login; el canal manda GC_PHASE(LOGIN) directo; el reloj del bot queda
    // anclado por el handshake del AUTH).

    loop {
        let p = recv(&mut rd, &mut sp, cfg.timeout, "channel_phase").await?;
        counts.rx(&p);
        match p[0] {
            header::GC_PHASE if p[1] == phase::LOGIN => break,
            header::GC_PHASE | header::CG_TIME_SYNC | header::CG_PONG => continue,
            other => {
                return Err(BotError::Unexpected(
                    Stage::ChannelLogin,
                    format!("header 0x{other:02x} esperando PHASE_LOGIN"),
                ))
            }
        }
    }
    send(
        &mut wr,
        &TPacketCGLogin3::new_channel(&cfg.login, &cfg.password, client_key(&cfg.login)).to_bytes_channel(),
        counts,
    )
    .await?;

    loop {
        let p = recv(&mut rd, &mut sp, cfg.timeout, "channel_login").await?;
        counts.rx(&p);
        match p[0] {
            header::GC_EMPIRE => {
                let e = TPacketGCEmpire::from_bytes(&p)
                    .map_err(|err| BotError::Protocol("GC_EMPIRE", err.to_string()))?;
                let _ = e.b_empire; // solo información (el 449 B basta)
            }
            header::GC_PHASE => continue,
            header::GC_LOGIN_SUCCESS_NEWSLOT => {
                let s = TPacketGCLoginSuccess::from_bytes(&p)
                    .map_err(|e| BotError::Protocol("449B", e.to_string()))?;
                let slot = s
                    .players
                    .iter()
                    .position(|pl| pl.dw_id != 0)
                    .ok_or(BotError::NoCharacter)? as u8;
                let (x, y) = (s.players[slot as usize].x, s.players[slot as usize].y);
                send(&mut wr, &TPacketCGPlayerSelect::new(slot).to_bytes(), counts).await?;
                return Ok((slot, x, y, rd, wr, sp));
            }
            header::GC_LOGIN_FAILURE => {
                let f = TPacketGCLoginFailure::from_bytes(&p)
                    .map_err(|e| BotError::Protocol("GC_LOGIN_FAILURE", e.to_string()))?;
                return Err(BotError::LoginFailure(f.status().into_owned()));
            }
            header::CG_TIME_SYNC | header::CG_PONG => continue,
            other => return Err(BotError::Desync(other)),
        }
    }
}

/// Entry al mundo: paquetes del load (LOADING, MAIN_CHARACTER, quickslots,
/// POINTS, SKILLS, items, affects) → `CG_ENTERGAME` → ADD + INFO +
/// `GC_PHASE(GAME)` + lands + TIME + CHANNEL + spawns. Devuelve los ms desde
/// la entrada (el select ya se envió en `channel_phase`) hasta
/// `GC_PHASE(GAME)`.
async fn world_phase(
    rd: &mut OwnedReadHalf,
    wr: &mut OwnedWriteHalf,
    sp: &mut Splitter,
    counts: &mut Counts,
    timeout: Duration,
) -> Result<u64, BotError> {
    let t0 = Instant::now();
    let mut sent_entergame = false;
    loop {
        let p = recv(rd, sp, timeout, "world_entry").await?;
        counts.rx(&p);
        match p[0] {
            TPacketGCMainCharacter::HEADER => {
                // El cliente manda CG_ENTERGAME al terminar de cargar el mapa
                // (game.py:206); el server procesa la cola tras sus entry
                // sends — mandarlo al ver el MAIN_CHARACTER es equivalente.
                if !sent_entergame {
                    send(wr, &[header::CG_ENTERGAME], counts).await?;
                    sent_entergame = true;
                }
            }
            header::GC_PHASE if p[1] == phase::GAME => return Ok(elapsed_ms(t0)),
            // LOADING (4) y el resto de la cola del entry: se cuentan y se
            // siguen consumiendo.
            header::GC_PHASE => continue,
            header::CG_TIME_SYNC | header::CG_PONG => continue,
            _ => continue,
        }
    }
}

/// Paso del paseo: `walk_speed` units/s × intervalo → units por MOVE
/// (mínimo 1). El envelope del server permite `speed × (dt+100ms)/1000 ×
/// 1.20` (movement.rs:148, speed 300 u/s) → con el default (200 u/s,
/// 1000 ms) el paso de 200 u tiene margen 1.98× a dt=1000 ms y sigue dentro
/// hasta dt ≈ 455 ms (aguanta el retardo de procesado del server bajo carga).
pub fn walk_step(walk_speed: u32, interval: Duration) -> i32 {
    let ms = u64::try_from(interval.as_millis()).unwrap_or(0).max(1);
    ((u64::from(walk_speed) * ms / 1000).max(1)) as i32
}

/// Paseo del bot: ping-pong en el eje X alternando salida y retorno
/// (`origen → +step → origen → −step → origen → ...`) — cada MOVE está a
/// `step` units EXACTOS del anterior (nunca patas diagonales/retornos de
/// cuadrado). El cuadrado anterior (paso 300, intervalo 1000 ms) tenía patas
/// de 424/600 u vs 396 permitidos → el envelope rechazaba todo MOVE tras el
/// primero (evidencia `chan7.err.log`: "fuera del envelope (speed 300) —
/// rechazado") y los bots nunca se movían físicamente. El ping-pong mantiene
/// TODAS las patas dentro de `allowed` y el bot se mueve de verdad (mobs al
/// acercarse — el escenario que el benchmark quería ejercitar).
struct Walk {
    origin_x: i32,
    origin_y: i32,
    step: i32,
    dir: i32, // +1 este (rot 18), −1 oeste (rot 54)
    out: bool, // true → salir a origen ± step; false → volver al origen
}

impl Walk {
    fn new(x: i32, y: i32, step: i32) -> Self {
        Self { origin_x: x, origin_y: y, step, dir: 1, out: true }
    }

    fn next(&mut self) -> (i32, i32, u8) {
        let rot = if self.dir > 0 { 18 } else { 54 };
        let x = if self.out {
            self.origin_x + self.dir * self.step
        } else {
            self.origin_x
        };
        if self.out {
            self.dir = -self.dir;
        }
        self.out = !self.out;
        (x, self.origin_y, rot)
    }
}

/// Sesión completa de un bot → reporte. Toma el config por valor (se
/// construye uno por tarea — `tokio::spawn` exige owned).
pub async fn run_bot(cfg: BotConfig, index: usize) -> BotReport {
    let start = Instant::now();
    let mut clock = ServerClock::default();
    let mut counts = Counts::default();
    let mut auth_ms = None;
    let mut channel_login_ms = None;
    let mut select_ms = None;
    let mut world_ms = None;
    let mut alive_ms = 0u64;
    let mut moves = 0u64;
    let mut pings = 0u64;
    let mut spawns = 0u64;
    let mut status = Status::Ok;
    let mut note = String::new();

    let result: Result<(), BotError> = async {
        auth_phase(&cfg, &mut clock, &mut counts).await?;
        auth_ms = Some(elapsed_ms(start));

        let (slot, x, y, mut rd, mut wr, mut sp) =
            channel_phase(&cfg, &mut counts).await?;
        channel_login_ms = Some(elapsed_ms(start));
        let _ = slot; // el slot elegido (información)

        select_ms = Some(world_phase(&mut rd, &mut wr, &mut sp, &mut counts, cfg.timeout).await?);
        world_ms = Some(elapsed_ms(start));
        let world_t0 = Instant::now();

        // ---- Loop de juego: MOVE periódico + PONG + conteo, hasta duration.
        let mut walk = Walk::new(x, y, walk_step(cfg.walk_speed, cfg.move_interval));
        let mut move_tick = tokio::time::interval_at(
            tokio::time::Instant::now() + cfg.move_interval,
            cfg.move_interval,
        );
        let deadline = tokio::time::Instant::now() + cfg.duration;
        loop {
            tokio::select! {
                p = recv(&mut rd, &mut sp, cfg.timeout, "game") => {
                    match p {
                        Ok(pkt) => {
                            counts.rx(&pkt);
                            match pkt[0] {
                                header::GC_PING => {
                                    send(&mut wr, &[header::CG_PONG], &mut counts).await?;
                                    pings += 1;
                                }
                                header::GC_CHARACTER_ADD => spawns += 1,
                                header::CG_TIME_SYNC | header::CG_PONG => {}
                                _ => {}
                            }
                        }
                        Err(BotError::Eof) => {
                            note = "server closed the connection".into();
                            break;
                        }
                        Err(e) => return Err(e),
                    }
                }
                _ = move_tick.tick() => {
                    let (mx, my, rot) = walk.next();
                    let mv = TPacketCGMove {
                        header: TPacketCGMove::HEADER,
                        b_func: TPacketCGMove::FUNC_MOVE,
                        b_arg: 0,
                        b_rot: rot,
                        x: mx,
                        y: my,
                        dw_time: clock.move_time_ms(local_ms()),
                    };
                    send(&mut wr, &mv.to_bytes(), &mut counts).await?;
                    moves += 1;
                }
                _ = tokio::time::sleep_until(deadline) => break,
            }
        }
        alive_ms = elapsed_ms(world_t0);
        Ok(())
    }
    .await;

    if let Err(e) = result {
        let (st, n) = classify(&e);
        status = st;
        if note.is_empty() {
            note = n;
        }
    }
    BotReport {
        index,
        login: cfg.login.clone(),
        status,
        note,
        auth_ms,
        channel_login_ms,
        select_ms,
        world_ms,
        alive_ms,
        rx_packets: counts.rx_packets,
        rx_bytes: counts.rx_bytes,
        tx_packets: counts.tx_packets,
        tx_bytes: counts.tx_bytes,
        moves,
        pings,
        spawns,
    }
}

fn classify(e: &BotError) -> (Status, String) {
    match e {
        BotError::Connect(addr, e) => (Status::Timeout, format!("connect {addr}: {e}")),        BotError::Timeout(label) => (Status::Timeout, format!("timeout en {label}")),
        BotError::LoginFailure(s) => (Status::LoginFailed, s.clone()),
        BotError::AuthFailed => (Status::AuthFailed, "bResult=0".into()),
        BotError::NoCharacter => (Status::NoCharacter, "cuenta sin personaje".into()),
        BotError::Desync(h) => (Status::Desync, format!("header 0x{h:02x} fuera de la tabla S→C")),
        BotError::Eof => (Status::Disconnected, "server closed".into()),
        BotError::Protocol(ctx, e) => (Status::WorldFailed, format!("{ctx}: {e}")),
        BotError::Unexpected(Stage::Auth, m) => (Status::AuthFailed, m.clone()),
        BotError::Unexpected(Stage::ChannelLogin, m) => (Status::LoginFailed, m.clone()),
        BotError::Unexpected(Stage::Game, m) => (Status::WorldFailed, m.clone()),
    }
}

fn elapsed_ms(t0: Instant) -> u64 {
    t0.elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_anchor_and_margin() {
        let mut c = ServerClock::default();
        assert_eq!(c.move_time_ms(1000), 500, "sin anclar: local − margen");
        c.observe(50_000, 10_000); // server 40s adelantado
        assert_eq!(c.server_ms(10_000), 50_000);
        assert_eq!(c.move_time_ms(10_000), 49_500, "est_server − 500");
        assert_eq!(c.move_time_ms(10_500), 50_000, "avanza con el reloj local");
        // El margen nunca deja el dw_time adelantado respecto al server.
        assert!(c.move_time_ms(10_000) <= c.server_ms(10_000) as u32);
    }

    #[test]
    fn client_key_is_deterministic_and_distinct() {
        assert_eq!(client_key("bench_0"), client_key("bench_0"));
        assert_ne!(client_key("bench_0"), client_key("bench_1"));
        assert_eq!(client_key("test").len(), 4);
    }

    #[test]
    fn walk_step_default_is_within_envelope() {
        // Default del harness: 200 u/s @ 1000 ms → paso 200 u.
        let step = walk_step(200, Duration::from_millis(1000));
        assert_eq!(step, 200);
        // El envelope del server: allowed = speed × (dt+100)/1000 × 1.20
        // (movement.rs:148, speed 300). A dt=1000 ms → 396; el paso queda
        // con margen 1.98×; incluso con retardo de procesado (dt=500 ms →
        // 216 u) el paso sigue dentro.
        let allowed = |dt_ms: u64| (300.0 * (dt_ms as f64 + 100.0) / 1000.0 * 1.20) as i64;
        assert!(i64::from(step) <= allowed(1000), "margen a dt=1000");
        assert!(i64::from(step) <= allowed(500), "margen bajo carga");
    }

    #[test]
    fn walk_step_scales_with_speed_and_interval() {
        assert_eq!(walk_step(200, Duration::from_millis(250)), 50);
        assert_eq!(walk_step(300, Duration::from_millis(1000)), 300);
        assert_eq!(walk_step(200, Duration::from_millis(100)), 20);
        assert_eq!(walk_step(1, Duration::from_millis(1000)), 1, "mínimo 1 u");
        assert_eq!(walk_step(1000, Duration::from_millis(1)), 1, "1 ms → 1 u");
        assert_eq!(walk_step(250, Duration::from_millis(1000)), 250, "knob --walk-speed");
    }

    #[test]
    fn walk_pingpong_legs_are_exactly_step() {
        // Cada MOVE está a `step` EXACTOS del anterior (sin diagonales ni
        // retornos de cuadrado — la causa del rechazo del envelope en el
        // cuadrado antiguo: patas 424/600 u > 396 permitidos).
        let mut w = Walk::new(969_600, 278_400, 200);
        let mut prev = (969_600i32, 278_400i32);
        for _ in 0..64 {
            let (x, y, rot) = w.next();
            assert_eq!(y, 278_400, "ping-pong en el eje X");
            let dx = i64::from(x - prev.0);
            let dy = i64::from(y - prev.1);
            assert_eq!(dx * dx + dy * dy, 200i64 * 200, "pata SIEMPRE = step");
            assert!(rot == 18 || rot == 54, "este/oeste (rot {rot})");
            prev = (x, y);
        }
        assert_eq!(prev.0, 969_600, "número par de pasos → vuelve al origen");
    }

    #[test]
    fn walk_stays_within_teleport_limit() {
        // La desviación máxima del origen es `step` ≤ 2500 (anti-teleport) —
        // el paseo nunca puede ser confundido con un salto.
        let mut w = Walk::new(969_600, 278_400, 200);
        for _ in 0..128 {
            let (x, _, _) = w.next();
            assert!((x - 969_600).abs() <= 200, "dentro del paseo: {x}");
        }
    }

    #[test]
    fn classify_maps_errors_to_statuses() {
        assert_eq!(classify(&BotError::AuthFailed).0, Status::AuthFailed);
        assert_eq!(classify(&BotError::LoginFailure("ALREADY".into())).0, Status::LoginFailed);
        assert_eq!(classify(&BotError::NoCharacter).0, Status::NoCharacter);
        assert_eq!(classify(&BotError::Desync(0x99)).0, Status::Desync);
        assert_eq!(classify(&BotError::Eof).0, Status::Disconnected);
        assert_eq!(classify(&BotError::Timeout("auth_result")).0, Status::Timeout);
        assert_eq!(
            classify(&BotError::Unexpected(Stage::Auth, "x".into())).0,
            Status::AuthFailed
        );
        assert_eq!(
            classify(&BotError::Unexpected(Stage::ChannelLogin, "x".into())).0,
            Status::LoginFailed
        );
        assert_eq!(
            classify(&BotError::Unexpected(Stage::Game, "x".into())).0,
            Status::WorldFailed
        );
        assert_eq!(
            classify(&BotError::Protocol("449B", "bad length".into())).0,
            Status::WorldFailed
        );
    }
}
