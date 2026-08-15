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
use database::item::{ItemRepo, ItemRow};
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
    /// Pool COMPARTIDO de conexiones PG del canal (los repos lo usan — NINGÚN
    /// camino abre conexión propia por llamada, fix 2026-08-13).
    pub pool: database::pool::PgPool,
    /// Batcher ÚNICO del canal (WAL durable + audit — el WorldStore lo usa
    /// para los saves/exchange; un solo loop de flush por canal).
    pub batcher: std::sync::Arc<database::wal::Batcher>,
    /// Caché COMPARTIDA de walkability (F5.4 — `game_core::map::MapStore`).
    pub map_store: Arc<Mutex<game_core::map::MapStore>>,
    /// Emisor de intents hacia el MUNDO COMPARTIDO del canal.
    pub intent_tx: UnboundedSender<Intent>,
    /// Lado emisor del canal de eventos S→C (el Join manda un clone al mundo).
    pub event_tx: UnboundedSender<NpcEvent>,
    /// Cola de eventos S→C del mundo (la drena el game loop).
    pub event_rx: UnboundedReceiver<NpcEvent>,
    /// Lado emisor del canal de CHAT S→C (el broadcast/whisper de OTRAS
    /// sesiones entrega aquí los bytes — gap-lane-C; el registro de peers
    /// guarda un clone por sesión activa).
    pub chat_tx: UnboundedSender<Vec<u8>>,
    /// Cola de bytes de chat S→C (la drena el game loop — ver game.rs).
    pub chat_rx: UnboundedReceiver<Vec<u8>>,
    /// RAII del peer de chat (registrado en el world join — chat.rs;
    /// desregistra al soltar la sesión).
    pub chat_guard: Option<crate::channel::chat::ChatPeerGuard>,
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
    /// Modo de movimiento del personaje (parity m_bIsWalking/m_bNowWalking
    /// del CHARACTER C++ — do_set_walk_mode/do_set_run_mode
    /// cmd_general.cpp:927-937). El C++ NO lo persiste en DB (la row de 42
    /// columnas no tiene columna walking — parity): vive en el CHARACTER;
    /// aquí en la sesión (por conexión). Lo consume `channel/gm.rs`
    /// (set_walk_mode → GC_WALK_MODE).
    pub walking: bool,
    /// NEXT_EXP del nivel actual (mutable — el level-up del kill lo recarga).
    pub next_exp: i64,
    /// Inventario del player (mutable — pickup/uso/move).
    pub inventory: Vec<ItemRow>,
    /// Battle points CACHEADOS (ComputeBattlePoints — el entry los computa
    /// con los protos y el equip/unequip los re-computa; `points_packet` los
    /// lee en TODOS los caminos — la ventana del cliente muestra ataque
    /// (daño del arma) y defensa (level+HT+armor) desde aquí).
    pub battle: game_core::packets::BattlePoints,
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
    /// Login de la CUENTA (normalizado, minúsculas) — el gmlist de GM exige
    /// la pareja (mName del personaje, mAccount de la cuenta): set en el
    /// login del entry (parity `gm_get_level` gm.cpp:50-105).
    pub account_login: String,
    /// Id de la CUENTA (PG) — set en el login del entry; lo usan los
    /// handlers de la fase select (create/delete/empire/change-name) para
    /// las queries del `player_index`.
    pub account_id: i64,
    /// `social_id` de la cuenta — confirmación del borrado de personaje
    /// (parity `ClientManagerPlayer.cpp:972-977`: los ÚLTIMOS 7 chars vs los
    /// primeros 7 del `private_code` del CG_CHARACTER_DELETE).
    pub social_id: String,
    /// Empire de la cuenta en el LOGIN (`player_index.empire` — `None` = sin
    /// fila de índice). Distinto de `empire` (el byte del GC_EMPIRE, que es
    /// RANDOM cuando la cuenta no tiene imperio — `empire_byte`): el handler
    /// de CG_EMPIRE necesita saber si la cuenta YA tenía imperio (parity
    /// `input_login.cpp:814-823` — con imperio + personajes → cierre).
    pub account_empire: Option<i16>,
    /// dw_arrow (F4 slice): el último CG_USE_SKILL disparado fue un skill de
    /// ARCO (flag USE_ARROW_DAMAGE) — la flecha se consume cuando llega el
    /// `SkillResult` (el mundo pudo RECHAZAR el skill: cooldown/SP/rango →
    /// sin evento → sin consumo). Se resetea en CADA dispatch (el gate de
    /// skills.rs lo pone a true solo para skills de arco). Race residual
    /// documentada: dos skills de arco DISTINTOS resueltos antes de drenar
    /// la cola consumen 1 flecha en vez de 2 (el mismo skill lo rechaza el
    /// cooldown del mundo).
    pub pending_arrow_shot: bool,
    /// Flag PvP del jugador (CG_PVP 41 — lane D). Solo en MEMORIA: la tabla
    /// `player.player` de esta variante NO tiene columna de pvp (parity: el
    /// TPlayerTable del C++ tampoco — el modo PvP del Metin2 completo es
    /// efímero de sesión). El cliente de esta variante nunca envía CG_PVP
    /// (header definido, sin sender — Packet.h:53); el handler es defensivo.
    pub pvp_mode: bool,
    /// Postura sentado del jugador (CG_CHARACTER_POSITION 28 — lane D):
    /// Sitdown/Standup. En memoria (parity: `m_pointsInstant.position` del
    /// C++ — no se persiste).
    pub sitting: bool,
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
        pool: database::pool::PgPool,
        batcher: std::sync::Arc<database::wal::Batcher>,
    ) -> Self {
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (chat_tx, chat_rx) = tokio::sync::mpsc::unbounded_channel();
        let ping_timer = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_millis(config.ping_interval_ms),
            Duration::from_millis(config.ping_interval_ms),
        );
        Self {
            conn: Connection::new(stream),
            conn_id,
            framer: Framer::new(ConnectionRole::Channel),
            config,
            pool,
            batcher,
            map_store,
            intent_tx,
            event_tx,
            event_rx,
            chat_tx,
            chat_rx,
            chat_guard: None,
            cap: CaptureGuard::open(conn_id),
            leave: None,
            login_guard: None,
            store: None,
            empire: 0,
            row: None,
            motion: None,
            walking: false,
            next_exp: 0,
            inventory: Vec::new(),
            battle: game_core::packets::BattlePoints::default(),
            affects: Vec::new(),
            pending_pickups: std::collections::HashSet::new(),
            last_packet: tokio::time::Instant::now(),
            walkability_warned: false,
            ping_timer,
            account_login: String::new(),
            account_id: 0,
            social_id: String::new(),
            account_empire: None,
            pending_arrow_shot: false,
            pvp_mode: false,
            sitting: false,
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
                CommonRepo::new(self.pool.clone()).next_exp(level).await.unwrap_or(0);
        }
        if leveled {
            // El nivel del mundo COMPARTIDO (la DEF del ataque del mob lo usa).
            self.intent(Intent::Combat(CombatIntent::SetLevel {
                player_vid: self.player_vid(),
                level: i32::from(self.row().level),
            }))?;
        }
        // GC_POINTS actualizado (el cliente muestra exp/gold/nivel) + persistencia.
        self.send(&game_core::packets::points_packet(self.row(), self.next_exp, &self.battle).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
        self.save();
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

    /// Snapshot de counts del inventario (las condiciones `count_item` del
    /// engine de quests): vnum -> count total (suma de stacks del window
    /// INVENTORY).
    pub fn inventory_counts(&self) -> std::collections::HashMap<u32, i64> {
        let mut counts = std::collections::HashMap::new();
        for i in &self.inventory {
            if i.window == "INVENTORY" {
                *counts.entry(i.vnum as u32).or_insert(0) += i.count;
            }
        }
        counts
    }

    /// PESO básico (lane D — parity de la fórmula clásica del Metin2,
    /// `char.cpp GetMaxWeight`: `(30 + level + ST×2) × 10`; el C++ de esta
    /// variante no tiene peso — el gate es solo server-side, el cliente no
    /// muestra la barra). Divergencia documentada: sin bonus de montura ni
    /// `POINT_HT` (el subset base).
    pub fn max_weight(&self) -> i64 {
        let row = self.row();
        (30 + i64::from(row.level) + 2 * i64::from(row.st)) * 10
    }

    /// Peso ACTUAL del inventario (lane D): `Σ count × item_proto.weight`
    /// para los stacks del window INVENTORY, dividido por 10 (unidades del
    /// item_proto crudas → la escala del `GetWeight` del C++ clásico). Sin
    /// fila de proto → el vnum pesa 0 (fail-open: un proto roto no congela
    /// el inventario; se loguea).
    pub async fn inventory_weight(&self) -> Result<i64, String> {
        let mut total = 0i64;
        let mut seen = std::collections::HashSet::new();
        for i in &self.inventory {
            if i.window != "INVENTORY" || !seen.insert(i.vnum) {
                continue;
            }
            let weight = match ItemRepo::new(self.pool.clone())
                .load_proto_use_values(i.vnum)
                .await?
            {
                Some(p) => p.weight,
                None => {
                    eprintln!(
                        "server_realms: channel conn {}: item vnum {} sin \
                         item_proto — pesa 0 (fail-open)",
                        self.conn_id, i.vnum
                    );
                    0
                }
            };
            total += weight
                * self
                    .inventory
                    .iter()
                    .filter(|x| x.window == "INVENTORY" && x.vnum == i.vnum)
                    .map(|x| x.count)
                    .sum::<i64>();
        }
        Ok(total / 10)
    }

    /// Save durable de la sesión: sincroniza la posición del MOVIMIENTO (la
    /// fuente de verdad del x/y — el cliente la actualiza con cada MOVE) en
    /// la fila del player y la persiste vía el Batcher del canal (100 ms,
    /// WAL + audit). Fix 2026-08-13: los saves anteriores persistían la fila
    /// CARGADA AL ENTRAR (x/y ANTIGUOS — el movimiento vive en `motion` y
    /// nunca se sincronizaba) → la posición se perdía al reconectar. El save
    /// es fire-and-forget e idempotente (el WAL cubre el replay).
    /// Fix 2026-08-14 (panic del cierre): el motion SOLO está seteado tras el
    /// ENTERGAME — una conexión que se corta durante la carga (antes del
    /// ENTERGAME) NO tiene motion → el sync se omite (la fila conserva su
    /// posición cargada) y el save sigue persistiendo el resto de campos.
    /// Save durable de la sesión — DEFENSIVO (fix 2026-08-14: panic en el
    /// cierre por RST — `store()` con expect; el save al cierre corre en
    /// TODOS los caminos, incluidos los de sesión SIN store/row: login
    /// fallido (NOID/NOTAVAIL retorna Ok del entry -> game::run -> cierre por
    /// EOF), guild mark, slot vacío). Sin store/row/motion -> save omitido
    /// (no hay estado que persistir).
    pub fn save(&mut self) {
        let Some(store) = self.store.as_ref() else { return };
        if let Some(m) = self.motion.as_ref()
            && let Some(row) = self.row.as_mut()
        {
            // P0-B (2026-08-14): NO persistir posiciones fuera del mapa (un
            // row con coords inválidas crashea el CLIENTE con 0xC0000374 en
            // el próximo load — probado 2×). El movimiento ya valida
            // walkability por MOVE (el x/y del motion es válido); el check
            // aquí es el seguro del borde de ESCRITURA (warps/GM). Leniente:
            // solo out-of-bounds; mapa no cargable → fail-open (se persiste).
            let in_bounds = {
                let mut mstore = self.map_store.lock().unwrap();
                match mstore.load(&self.config.map_path, row.map_index) {
                    Ok(()) => mstore
                        .get(row.map_index)
                        .map(|map| map.attr(m.x, m.y).is_some())
                        .unwrap_or(true),
                    Err(_) => true, // fail-open (parity del movimiento)
                }
            };
            if in_bounds {
                row.x = m.x;
                row.y = m.y;
            }
        }
        let Some(row) = self.row.as_ref() else { return };
        store.save_character(row);
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
