//! `channel/session.rs` — el estado de UNA conexión del canal + los helpers
//! de IO del wire (R-s1 del refactor de channel.rs).
//!
//! `Session` agrupa TODO el estado que antes eran ~20 locales de
//! `connection_inner` (conn/framer/config/guards RAII/estado del player/
//! estado del game loop) — ADR-0010:57-59 ratifica el estado por conexión.
//! Los campos que el flujo de entrada llena PROGRESIVAMENTE (login → select
//! → entry) son `Option`/defaults hasta que su fase los setea; los accessors
//! `row()`/`store()`/`motion()` fallan con mensaje claro si se usan antes.
//!
//! El hook del harness F5 (bench_capture) sobrevive aquí intacto:
//! - inbound: `recv_packet_idle` (captura cada paquete recibido);
//! - outbound: `conn_send` (captura tras el Ok del socket);
//! - open/close: `CaptureGuard` (RAII — todos los retornos).

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use database::affect::AffectRow;
use database::common::CommonRepo;
use database::item::ItemRow;
use database::player::PlayerRow;
use network::framer::{ConnectionRole, Framer};
use network::Connection;
use game_core::ecs::{CombatIntent, Intent, ItemIntent, KillInfo, NpcEvent};
use game_core::world::WorldStore;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::channel::rand32;
use crate::config::Config;

/// Resultado de un handler de paquete del game loop (R-s3 — firma uniforme,
/// C6a): los handlers distinguen el cierre PROTOCOLARIO (speedhack — parity
/// `DelayedDisconnect`) del error FATAL (Err — socket/IO) y de los rechazos
/// normales (Continue — el paquete se procesó o se descartó con log).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// El paquete se procesó (o se rechazó con log); el loop sigue.
    Continue,
    /// Cierre protocolario con razón (p.ej. speedhack).
    Close(String),
}

impl Outcome {
    /// Convierte el Outcome al Result del game loop: Continue → Ok;
    /// Close(razón) → Err(razón) — el loop cierra la conexión con esa razón.
    pub fn into_result(self) -> Result<(), String> {
        match self {
            Outcome::Continue => Ok(()),
            Outcome::Close(reason) => Err(reason),
        }
    }
}

/// Hook del harness F5 (bench_capture): abre la captura al crear la conexión
/// y la cierra en TODOS los retornos (RAII — el handler tiene muchos
/// early-returns).
pub struct CaptureGuard(u32);

impl CaptureGuard {
    pub fn open(conn_id: u32) -> Self {
        crate::bench_capture::open_conn(conn_id);
        Self(conn_id)
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        crate::bench_capture::close_conn(self.0);
    }
}

/// Al terminar la conexión (RAII — TODOS los early-returns del handler) se
/// limpia la entidad del jugador del mundo compartido (`Intent::Leave`).
pub struct LeaveGuard {
    pub player_vid: u32,
    pub tx: UnboundedSender<Intent>,
}

impl Drop for LeaveGuard {
    fn drop(&mut self) {
        let _ = self.tx.send(Intent::Leave { player_vid: self.player_vid });
    }
}

/// Envío con captura golden (hook del harness F5 — bench_capture): no-op sin
/// `--bench-capture`. Los `.map_err` específicos de cada call site se
/// conservan (el wrapper solo añade la captura tras el Ok del socket).
pub(crate) async fn conn_send(
    conn: &mut Connection<TcpStream>,
    conn_id: u32,
    bytes: &[u8],
) -> Result<(), String> {
    conn.send(bytes).await.map_err(|e| format!("socket: {e}"))?;
    crate::bench_capture::capture_conn(conn_id, crate::bench_capture::Direction::Outbound, bytes);
    Ok(())
}

/// Lee el siguiente paquete con timeout de inactividad: si no llega NADA en
/// `timeout`, la conexión se cierra (el paquete que llega resetea el timer —
/// el timeout se crea por lectura). El handshake (antes de este helper) tiene
/// sus propios retries internos (F1.5 — una conexión muda muere en ellos).
/// Hook del harness F5: cada paquete recibido se captura (bench_capture —
/// no-op sin `--bench-capture`; los paquetes del handshake viven en
/// `network::handshake` y NO se capturan — documentado).
pub(crate) async fn recv_packet_idle<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    conn: &mut Connection<S>,
    framer: &mut Framer,
    timeout: Duration,
    conn_id: u32,
) -> Result<Vec<u8>, String> {
    let pkt = tokio::time::timeout(timeout, framer.next_packet(conn))
        .await
        .map_err(|_| {
            format!(
                "timeout de inactividad de {} ms — sin paquetes del cliente, conexión cerrada",
                timeout.as_millis()
            )
        })?
        .map_err(|e| format!("framer: {e}"))?;
    crate::bench_capture::capture_conn(conn_id, crate::bench_capture::Direction::Inbound, &pkt);
    Ok(pkt)
}

/// Logins con sesión activa en el canal (parity `DESC_MANAGER::FindByLoginName`,
/// `db.cpp:354-359` — el C++ rechaza un segundo login del mismo nombre). El
/// guard libera al cerrar la conexión. Independiente del guard del auth (otro
/// proceso).
fn channel_logins() -> &'static Mutex<std::collections::HashSet<String>> {
    static M: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// Guard RAII del login activo (vive en `Session.login_guard` — R-s1: antes
/// era un local de `connection_inner`, lo que lo liberaba al terminar la fase
/// de login; ahora libera al cerrar la conexión, parity del C++).
pub struct ChannelLoginGuard {
    login: String,
}

impl ChannelLoginGuard {
    pub fn acquire(login: &str) -> Option<Self> {
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

/// El estado de UNA conexión del canal (R-s1 — ADR-0010:57-59).
///
/// Los handlers de paquetes (game loop) reciben `&mut Session`: el estado
/// del player (row/motion/inventory/next_exp), la infraestructura del wire
/// (conn/framer/config), los canales hacia el mundo COMPARTIDO
/// (intent_tx/event_rx) y el estado del propio loop (ping/idle/pickups).
pub struct Session {
    /// Socket + capa de paquetes del wire.
    pub conn: Connection<TcpStream>,
    /// Id de la conexión (logs + bench_capture).
    pub conn_id: u32,
    /// Framer del rol Channel (headera del cliente).
    pub framer: Framer,
    /// Config del canal (clonada por conexión — `run` la clona al aceptar).
    pub config: Config,
    /// Caché COMPARTIDA de walkability (F5.4 — `game_core::map::MapStore`).
    pub map_store: Arc<Mutex<game_core::map::MapStore>>,
    /// Emisor de intents hacia el MUNDO COMPARTIDO del canal.
    pub intent_tx: UnboundedSender<Intent>,
    /// Lado emisor del canal de eventos S→C (el Join manda un clone al mundo).
    pub event_tx: UnboundedSender<NpcEvent>,
    /// Cola de eventos S→C del mundo (la drena el game loop).
    pub event_rx: UnboundedReceiver<NpcEvent>,
    /// RAII de la captura golden del harness (open al crear, close al soltar).
    /// Nunca se LEE — vive solo por su Drop (dead_code intencional).
    #[allow(dead_code)]
    pub cap: CaptureGuard,
    /// RAII del `Intent::Leave` del mundo (None hasta el world join).
    pub leave: Option<LeaveGuard>,
    /// RAII del login activo (parity `DESC_MANAGER::FindByLoginName` —
    /// libera al CERRAR la conexión; el guard se adquiere en el login y
    /// vive aquí hasta el drop de la sesión).
    pub login_guard: Option<ChannelLoginGuard>,
    /// WorldStore (repos + Batcher) — None hasta el login OK.
    pub store: Option<WorldStore>,
    /// Empire del `GC_EMPIRE` (1..3) — set en el login.
    pub empire: u8,
    /// Fila del player (Q2) — None hasta el select.
    pub row: Option<PlayerRow>,
    /// Estado de movimiento anti-speedhack — None hasta el select.
    pub motion: Option<game_core::movement::PlayerMotion>,
    /// NEXT_EXP del nivel actual (mutable — el level-up del kill lo recarga).
    pub next_exp: i64,
    /// Inventario del player (mutable — pickup/uso/move).
    pub inventory: Vec<ItemRow>,
    /// Afectos del player (solo el entry los consume).
    pub affects: Vec<AffectRow>,
    /// Pickups en curso: el CG_ITEM_PICKUP manda el intent y el resultado
    /// vuelve por la cola — el set evita duplicar el mismo vid mientras el
    /// primer pickup se resuelve (parity del flujo síncrono previo).
    pub pending_pickups: std::collections::HashSet<u32>,
    /// Deadline de inactividad PERSISTENTE (último paquete del cliente: el
    /// select! cancela los brazos al ganar uno, así que el timer del idle se
    /// recrea por iteración — pero con el MISMO deadline, que solo cambia al
    /// RECIBIR → el ping del canal NO resetea el idle).
    pub last_packet: tokio::time::Instant,
    /// F5.4 (ADR-0011): walkability — si el mapa del jugador no cargó
    /// (map_path roto/fuera de rango), se loguea UNA vez y el chequeo se
    /// omite (fail-open: un mapa roto no congela a los jugadores; el envelope
    /// sigue activo).
    pub walkability_warned: bool,
    /// Heartbeat del server (GC_PING — parity `ping_event`, desc.cpp:179-214).
    pub ping_timer: tokio::time::Interval,
}

impl Session {
    /// Sesión nueva de una conexión aceptada: wire + guards + canales S→C.
    /// El resto de campos se llenan en las fases de `entry` (login/select/
    /// world join).
    pub fn new(
        stream: TcpStream,
        config: Config,
        conn_id: u32,
        intent_tx: UnboundedSender<Intent>,
        map_store: Arc<Mutex<game_core::map::MapStore>>,
    ) -> Self {
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let ping_timer = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_millis(config.ping_interval_ms),
            Duration::from_millis(config.ping_interval_ms),
        );
        Self {
            conn: Connection::new(stream),
            conn_id,
            framer: Framer::new(ConnectionRole::Channel),
            config,
            map_store,
            intent_tx,
            event_tx,
            event_rx,
            cap: CaptureGuard::open(conn_id),
            leave: None,
            login_guard: None,
            store: None,
            empire: 0,
            row: None,
            motion: None,
            next_exp: 0,
            inventory: Vec::new(),
            affects: Vec::new(),
            pending_pickups: std::collections::HashSet::new(),
            last_packet: tokio::time::Instant::now(),
            walkability_warned: false,
            ping_timer,
        }
    }

    /// La fila del player (invariante: seteada en la fase select — el game
    /// loop solo corre después del entry).
    pub fn row(&self) -> &PlayerRow {
        self.row.as_ref().expect("row: seteado en la fase select")
    }

    /// Acceso MUTABLE a la fila del player (mismo invariante que `row`).
    pub fn row_mut(&mut self) -> &mut PlayerRow {
        self.row.as_mut().expect("row: seteado en la fase select")
    }

    /// El WorldStore (invariante: seteado en el login OK).
    pub fn store(&self) -> &WorldStore {
        self.store.as_ref().expect("store: seteado en el login")
    }

    /// El estado de movimiento (invariante: seteado en la fase select).
    pub fn motion(&self) -> &game_core::movement::PlayerMotion {
        self.motion.as_ref().expect("motion: seteado en la fase select")
    }

    /// Acceso MUTABLE al estado de movimiento (mismo invariante que `motion`).
    pub fn motion_mut(&mut self) -> &mut game_core::movement::PlayerMotion {
        self.motion.as_mut().expect("motion: seteado en la fase select")
    }

    /// El vid del player en el wire (= su id en PG — parity del canal).
    pub fn player_vid(&self) -> u32 {
        self.row().id as u32
    }

    /// Envío con captura golden (wrap de `conn_send` — hook del harness F5).
    pub async fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        conn_send(&mut self.conn, self.conn_id, bytes).await
    }

    /// Lectura con timeout de inactividad + captura (wrap de `recv_packet_idle`).
    pub async fn recv_idle(&mut self) -> Result<Vec<u8>, String> {
        recv_packet_idle(&mut self.conn, &mut self.framer, self.config.timeout, self.conn_id).await
    }

    /// Envía un intent hacia el mundo COMPARTIDO (el mpsc vive con la tarea
    /// del canal — el error solo ocurre si esa tarea murió).
    pub fn intent(&self, intent: Intent) -> Result<(), String> {
        self.intent_tx
            .send(intent)
            .map_err(|_| "canal de intents del mundo cerrado".to_string())
    }

    /// Flujo de kill compartido (ataque normal y skills — el cliente
    /// reproduce la muerte): GC_DEAD (14) + GC_CHARACTER_DEL (2) +
    /// recompensa (kill_reward — exp/gold con rates) + level-up (next_exp
    /// recargado de la DB) + GC_POINTS + save durable + roll del drop
    /// (DropItem → el mundo asigna el vid).
    ///
    /// R-s1: era una función libre con 10 parámetros (el canary del oracle) —
    /// ahora método de Session.
    pub async fn apply_kill(&mut self, victim_vid: u32, v: KillInfo) -> Result<(), String> {
        self.send(&protocol::world::TPacketGCDead::new(victim_vid).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_DEAD: {e}"))?;
        self.send(&protocol::world::TPacketGCCharacterDelete::new(victim_vid).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_CHARACTER_DEL: {e}"))?;
        // F5.3: recompensa del kill — exp y gold del mob_proto con los rates
        // del config (game_core::combat — parity del C++, testeada).
        let reward = game_core::combat::kill_reward(
            v.exp,
            v.gold_min,
            v.gold_max,
            self.config.exp_rate,
            self.config.gold_rate,
            &mut |lo, hi| {
                // roll INCLUSIVE (parity number(min,max)).
                let span = (hi - lo + 1).max(1) as u32;
                lo + (rand32() % span) as i32
            },
        );
        let (exp_gain, gold_gain) = (reward.exp_gain, reward.gold_gain);
        {
            let row = self.row_mut();
            row.exp = row.exp.saturating_add(exp_gain.min(i32::MAX as i64) as i32);
            row.gold = row.gold.saturating_add(gold_gain.min(i32::MAX as i64) as i32);
        }
        // Level-up (parity char.cpp `GetNextExp` — exp_table por nivel; el
        // next_exp se recarga de la DB al subir).
        let mut leveled = false;
        while self.next_exp > 0 && i64::from(self.row().exp) >= self.next_exp {
            let next = self.next_exp;
            let exp = (i64::from(self.row().exp) - next) as i32;
            let level = self.row().level.saturating_add(1);
            let row = self.row_mut();
            row.exp = exp;
            row.level = level;
            leveled = true;
            self.next_exp =
                CommonRepo::new(&self.config.pg_conn).next_exp(level).await.unwrap_or(0);
        }
        if leveled {
            // El nivel del mundo COMPARTIDO (la DEF del ataque del mob lo usa).
            self.intent(Intent::Combat(CombatIntent::SetLevel {
                player_vid: self.player_vid(),
                level: i32::from(self.row().level),
            }))?;
        }
        // GC_POINTS actualizado (el cliente muestra exp/gold/nivel) + persistencia.
        self.send(&game_core::packets::points_packet(self.row(), self.next_exp).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
        self.store().save_character(self.row());
        // F5.3: DROP del mob — el drop primario (`mob_proto.drop_item`), con
        // la probabilidad del `drop_rate` del config. (El C++ además usa
        // etc_drop_item.txt por nombre — TRAP AGENTS.md §17 — el subset base
        // usa solo la columna.) El vid lo asigna el MUNDO (`DropResult` — el
        // GC_ITEM_GROUND_ADD sale cuando llega).
        if v.drop_item > 0 && (rand32() % 100) < u32::from(self.config.drop_rate) {
            self.intent(Intent::Item(ItemIntent::DropItem {
                player_vid: self.player_vid(),
                vnum: v.drop_item as u32,
                count: 1,
                x: v.x,
                y: v.y,
                z: 0,
            }))?;
        }
        eprintln!(
            "server_realms: channel conn {}: {} mató al mob vnum {} \
             (vid {}): exp +{exp_gain}, gold +{gold_gain}{} (nivel {})",
            self.conn_id,
            self.row().name,
            v.vnum,
            victim_vid,
            if leveled { ", LEVEL UP" } else { "" },
            self.row().level
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    /// El timeout del canal es de INACTIVIDAD (no absoluto): cada paquete
    /// recibido resetea el timer; el silencio > timeout dispara el cierre.
    /// Con el reloj pausado: paquetes a t=0/150/300 (ventana 200 ms) siempre
    /// dentro de la ventana → la conexión sigue; silencio tras t=300 →
    /// el timer dispara.
    #[tokio::test(start_paused = true)]
    async fn idle_timeout_resets_on_traffic_and_fires_on_silence() {
        let (server_side, mut client_side) = tokio::io::duplex(1024);
        let mut conn = Connection::new(server_side);
        let mut framer = Framer::new(ConnectionRole::Channel);
        let timeout = Duration::from_millis(200);

        // MOVE (16 B) como el paquete de juego del cliente vivo.
        let move_pkt = [7u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        // t=0: primer paquete → recv OK (el timer nace en la llamada).
        client_side.write_all(&move_pkt).await.unwrap();
        let pkt = recv_packet_idle(&mut conn, &mut framer, timeout, 1).await.expect("t=0");
        assert_eq!(pkt[0], 7);

        // t=0..150: silencio; a t=150 llega otro paquete → recv OK
        // (150 < 200 — dentro de la ventana de la llamada).
        tokio::time::advance(Duration::from_millis(150)).await;
        client_side.write_all(&move_pkt).await.unwrap();
        let pkt = recv_packet_idle(&mut conn, &mut framer, timeout, 1).await.expect("t=150");
        assert_eq!(pkt[0], 7);

        // t=150..300: silencio; a t=300 llega otro → recv OK (300-150=150 < 200
        // — el timer de ESTA llamada nace en t=150).
        tokio::time::advance(Duration::from_millis(150)).await;
        client_side.write_all(&move_pkt).await.unwrap();
        let pkt = recv_packet_idle(&mut conn, &mut framer, timeout, 1).await.expect("t=300");
        assert_eq!(pkt[0], 7);

        // t=300..: silencio total → avanzar 250 > 200 → el timer dispara.
        tokio::time::advance(Duration::from_millis(250)).await;
        let err = recv_packet_idle(&mut conn, &mut framer, timeout, 1).await;
        assert!(
            err.is_err() && err.unwrap_err().contains("inactividad"),
            "el silencio > timeout dispara el cierre"
        );
    }
}
