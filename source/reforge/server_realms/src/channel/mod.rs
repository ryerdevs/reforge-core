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
//!    `GC_LOGIN_SUCCESS_NEWSLOT` (0x20, 449 B — `game_core::packets::login_success`
//!    con los 5 slots de `WorldStore::account_slots`; handle = conn_id,
//!    random_key = rand32 — parity `desc.cpp:955-988`).
//! 6. `CG_PLAYER_SELECT` (2 B) → `WorldStore::select_player` (índice → Q2 load)
//!    → **spawn best-effort**: `GC_PHASE(LOADING)` + `TPacketGCCharacterAdd`
//!    (37 B) + `TPacketGCCharacterAdditionalInfo` (70 B) — los GAPs del spawn
//!    completo están documentados en `game_core::packets` (mapa/sectree,
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
//!
//! Refactor R-s1 (oracle review 2026-08-13): el estado de la conexión vive
//! en `session::Session` (antes ~20 locales de `connection_inner`); `apply_kill`
//! pasó de 10 parámetros a método de Session. La estructura de archivos
//! (entry/game/movement/items/...) se divide en R-s2/R-s3.

mod chat;
mod combat;
mod entry;
mod events;
mod game;
mod items;
mod movement;
mod quest;
mod script;
mod session;
mod skills;
mod social;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use database::item::ItemRepo;
use game_core::ecs::{CombatEvent, Intent, NpcEvent, WorldSim};
use tokio::net::{TcpListener, TcpStream};

use crate::config::Config;
use session::Session;

/// Servidor channel: listener + tarea por conexión (patrón del auth) + el
/// MUNDO COMPARTIDO (ADR-0010 §1 — patrón Veloren): una instancia de
/// `WorldSim` por canal, los intents de las conexiones entran por el mpsc y
/// los eventos S→C salen por la cola de cada jugador (routing por vid).
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
    // F5.3 (ADR-0010): recurso `SpawnCache` del mundo compartido.
    let spawn_cache = std::sync::Arc::new(tokio::sync::Mutex::new(game_core::npc::MobCache::new()));
    // F5.4 (ADR-0011): caché de walkability COMPARTIDA entre conexiones —
    // get-or-load del mapa por id (index + Setting.txt + server_attr), los
    // fallos se cachean (un mapa roto no re-lee disco por MOVE).
    let map_store = std::sync::Arc::new(std::sync::Mutex::new(game_core::map::MapStore::new()));
    let mut world = WorldSim::new(spawn_cache);
    let (intent_tx, mut intent_rx) = tokio::sync::mpsc::unbounded_channel::<Intent>();
    // El canal conserva un emisor propio: el mpsc NO se cierra cuando la
    // última conexión termina (la tarea del mundo sigue viva para aceptar).
    let _keepalive = intent_tx.clone();
    // Routing de eventos por jugador: vid del player → su cola S→C.
    let mut routes: std::collections::HashMap<
        u32,
        tokio::sync::mpsc::UnboundedSender<NpcEvent>,
    > = std::collections::HashMap::new();
    // Tick de AI del mundo (500 ms — parity del tick previo del canal).
    const AI_TICK_MS: u64 = 500;
    let mut ai_timer = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_millis(AI_TICK_MS),
        Duration::from_millis(AI_TICK_MS),
    );
    let mut conn_id: u32 = 1;
    loop {
        tokio::select! {
            _ = ai_timer.tick() => {
                // Tick del mundo COMPARTIDO: los sistemas corren sobre TODAS
                // las entidades; cada evento va a la cola de su jugador.
                let events = world.update(AI_TICK_MS);
                route_events(events, &routes);
            }
            intent = intent_rx.recv() => {
                let Some(intent) = intent else { break };
                match intent {
                    Intent::Join { player, out } => {
                        // El routing del jugador se registra ANTES de enrutar
                        // sus eventos (los Spawned del join). `player` se
                        // mueve a join_player — los campos del log se copian.
                        let pvid = player.vid;
                        let pmap = player.map_index;
                        routes.insert(pvid, out);
                        let repo = database::npc::MobRepo::new(&config.pg_conn);
                        let events = match world.join_player(player, &repo, &config.map_path).await {
                            Ok(ev) => ev,
                            Err(e) => {
                                // Degradación del entry previo: mundo sin
                                // spawns (el log del mapa ya avisó).
                                eprintln!(
                                    "server_realms: channel: join del jugador {}: {e} — mundo sin spawns",
                                    pvid
                                );
                                Vec::new()
                            }
                        };
                        // La tabla de skills (UNA vez — estática en el
                        // runtime); si falla, las skills quedan desactivadas.
                        if let Err(e) = world
                            .load_skills(&game_core::skill::SkillRepo::new(&config.pg_conn))
                            .await
                        {
                            eprintln!("server_realms: channel: skill_proto: {e} — skills desactivadas");
                        }
                        let spawned = events
                            .iter()
                            .filter(|e| matches!(e, NpcEvent::Combat(CombatEvent::Spawned { .. })))
                            .count();
                        eprintln!(
                            "server_realms: channel: jugador {} en el mundo (mapa {}) — \
                             {spawned} entradas visibles materializadas",
                            pvid, pmap
                        );
                        route_events(events, &routes);
                    }
                    other => {
                        let events = world.process_intent(other, now_ms());
                        route_events(events, &routes);
                    }
                }
            }
            stream = listener.accept() => {
                let (stream, _peer) = match stream {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("server_realms: channel accept: {e}");
                        continue;
                    }
                };
                let cfg = config.clone();
                let id = conn_id;
                let tx = intent_tx.clone();
                let ms = map_store.clone();
                conn_id = conn_id.wrapping_add(1);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, cfg, id, tx, ms).await {
                        eprintln!("server_realms: channel conn {id}: {e}");
                    }
                });
            }
        }
    }
    Ok(())
}

/// Enruta los eventos del mundo a la cola de cada jugador (el que no está
/// conectado — p.ej. tras su Leave — simplemente no recibe).
fn route_events(
    events: Vec<NpcEvent>,
    routes: &std::collections::HashMap<u32, tokio::sync::mpsc::UnboundedSender<NpcEvent>>,
) {
    for ev in events {
        if let Some(tx) = routes.get(&ev.player_vid()) {
            let _ = tx.send(ev);
        }
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
///
/// R-s1: la sesión (`session::Session`) se crea aquí con TODOS los campos
/// (wire, guards RAII, canales S→C); R-s2/R-s3: las fases viven en
/// `entry::run` y el loop de juego en `game::run`.
async fn handle_connection(
    stream: TcpStream,
    config: Config,
    conn_id: u32,
    intent_tx: tokio::sync::mpsc::UnboundedSender<Intent>,
    map_store: std::sync::Arc<std::sync::Mutex<game_core::map::MapStore>>,
) -> Result<(), String> {
    let mut session = Session::new(stream, config, conn_id, intent_tx, map_store);
    // Fases 1-7 (handshake → login → select → entry → world join): la sesión
    // queda LLENA (row/store/motion/leave) antes del loop de juego.
    entry::run(&mut session).await?;
    game::run(&mut session).await
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

/// DEF de items ARMOR equipados (parity char.cpp:2120-2126): la suma de
/// `value1 + 2×value5` de los items ARMOR (BODY/HEAD/SHIELD/FOOTS) del
/// EQUIPMENT — el `iArmor` del `player_def_grade` del ataque del mob.
/// Se recalcula SOLO en el entry y al equipar/desequipar (el valor no cambia
/// con el resto de operaciones del inventario) — el tick del AI lo lee del
/// mundo ECS (`WorldSim::set_player_armor`), eliminando las queries PG por
/// tick del código previo.
async fn equipped_armor(inventory: &[database::item::ItemRow], pg_conn: &str) -> Result<i32, String> {
    let mut armor = 0i32;
    for w in inventory.iter().filter(|i| i.window == "EQUIPMENT") {
        if let Some(p) = ItemRepo::new(pg_conn).load_proto_use_values(w.vnum).await? {
            const ITEM_TYPE_ARMOR: i16 = 2; // ItemData.h:73
            if p.b_type == ITEM_TYPE_ARMOR && matches!(p.b_sub_type, 0 | 1 | 2 | 4) {
                armor += p.values[1] + 2 * p.values[5];
            }
        }
    }
    Ok(armor)
}

/// Límite del stack de items (`g_bItemCountLimit` — config.cpp:39): usado
/// por el pickup (stacking), el move y el uso de items.
const ITEM_COUNT_LIMIT: i64 = 200;

/// `INVENTORY_MAX_NUM` del runtime (length.h:29 con `ENABLE_EXTEND_INVEN_SYSTEM`
/// activo — CommonDefines.h:32): 5×9×4 = 180 celdas. El wire del equip usa
/// `cell = INVENTORY_MAX_NUM + wear` (length.h:827 — `IsEquipPosition`).
const INVENTORY_MAX_NUM: u16 = 180;

/// `WEAR_MAX_NUM = 32` (length.h:77) — los slots de equipamiento.
const WEAR_MAX_NUM: u16 = 32;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
