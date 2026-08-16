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
//! - El canal usa un `PgPool` compartido (`database::pool`) y UN `Batcher`
//!   por canal (Arc, flush 100 ms — cláusula del pool de ADR-0008, ejecutada
//!   2026-08-13); `Session` lo recibe por referencia (pool + batcher).
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
mod gm;
mod items;
mod movement;
mod party;
mod pvp;
mod quest;
mod quickslot;
mod safebox;
mod script;
mod session;
mod shop;
mod skills;
mod social;
mod trade;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use database::item::ItemRepo;
use game_core::ecs::{CombatEvent, Intent, NpcEvent, QuestIntent, WorldSim};
use protocol::world::TItemPos;
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
    // Pool COMPARTIDO de conexiones PG del canal (fix del cuello del entry
    // 2026-08-13): los repos del crate database YA NO abren conexión por
    // llamada — el pool se crea aquí con `pool_max_size` del toml (default
    // 10) y las queries reutilizan sus conexiones. LAZY a propósito: sin
    // sanity en el arranque — con PG caída el canal sigue escuchando y cada
    // login degrada a NOTAVAIL (semántica previa de `WorldStore::new`, que el
    // smoke `channel_handles_login3_with_db_down` fija como contrato).
    let pool = match database::pool::new_pool(&config.pg_conn, config.pool_max_size) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("server_realms: channel: {e} — PG no disponible, abortando");
            return Err(std::io::Error::other(e));
        }
    };
    // Replay del WAL local UNA vez por proceso (idempotente — `replay_once`).
    // Antes vivía en WorldStore::new (por login, bloqueando con NOTAVAIL).
    // Fail-open: si falla (PG caída / archivo corrupto) el canal SIGUE — el
    // archivo WAL queda en disco (no se borra) y el próximo arranque lo
    // reintenta; los logins degradan a NOTAVAIL mientras tanto.
    let wal_dir = game_core::world::wal_dir();
    if let Err(e) = game_core::world::replay_once(&pool, &wal_dir).await {
        eprintln!("server_realms: channel: replay del WAL: {e} — sigue (fail-open); el WAL queda en disco");
    }
    // Batcher ÚNICO del canal (WAL local durable-first + audit en la misma
    // tx, 100 ms / 64 mutations — semántica intacta de `database::wal`): los
    // jugadores COMPARTEN el worker de flush (antes había un Batcher por
    // WorldStore = por jugador).
    let sink = database::wal::WalSink::new(database::wal::PgMutationSink::new(pool.clone()), wal_dir);
    let batcher = std::sync::Arc::new(database::wal::Batcher::spawn(
        Duration::from_millis(100),
        64,
        sink,
    ));
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
    // F5 quests (wiring 2026-08-13, fix A/B 2026-08-14): la conversión del
    // corpus (194 archivos, ~2 s) NO puede bloquear el arranque — el cliente
    // conecta al backlog del listener y cierra si el handshake llega tarde
    // ("32 attempts" con quests cargadas; A/B: quest_path vacío conecta). La
    // carga va en SEGUNDO PLANO: el accept loop arranca inmediatamente; el
    // canal arranca SIN quests y las recibe cuando la conversión acaba
    // (QuestIntent::Load por el mpsc normal — fail-open documentado).
    let quest_dir = if config.quest_path.is_empty() {
        default_quest_dir(&config.map_path)
    } else {
        config.quest_path.clone()
    };
    if !quest_dir.is_empty() {
        let tx = intent_tx.clone();
        tokio::spawn(async move {
            if let Some((text, texts)) = load_quest_corpus(&quest_dir) {
                let _ = tx.send(Intent::Quest(QuestIntent::Load { text, texts }));
            }
        });
    }
    // El canal conserva un emisor propio: el mpsc NO se cierra cuando la
    // última conexión termina (la tarea del mundo sigue viva para aceptar).
    let _keepalive = intent_tx.clone();
    // Routing de eventos por jugador: vid del player → su cola S→C.
    let mut routes: std::collections::HashMap<
        u32,
        tokio::sync::mpsc::UnboundedSender<NpcEvent>,
    > = std::collections::HashMap::new();
    // El CHAT (gap-lane-C) usa un registro EQUIVALENTE por su cuenta: el
    // broadcast GC_CHAT y el whisper entregan bytes a otras sesiones por el
    // outbox `chat_rx` de cada una (vid → peer en `chat.rs::peers()` — con
    // nombre/posición/empire; `routes` solo sabe de eventos del mundo).
    // Tick de AI del mundo. 2026-08-15: 500 → 250 ms — los pasos de
    // speed×0.5s cada medio segundo se veían "a saltos rápidos" (el C++
    // mueve los mobs cada frame ~100ms con pasos continuos — con 500ms el
    // cliente interpola cada paso pero con pausas entre ellos). Con 250ms
    // los pasos son speed×0.25s — 2× más suave, misma velocidad real
    // (step_toward usa tick.dt_ms). El dt del mundo se pasa por update().
    const AI_TICK_MS: u64 = 250;
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
                // Harness F5: tick_ms por tick (timing de sistemas) + los
                // contadores del mundo -> `--bench-capture` (no-op sin flag).
                crate::bench_capture::record_metrics(world.metrics());
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
                        let repo = database::npc::MobRepo::new(pool.clone());
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
                            .load_skills(&game_core::skill::SkillRepo::new(pool.clone()))
                            .await
                        {
                            eprintln!("server_realms: channel: skill_proto: {e} — skills desactivadas");
                        }
                        // La tabla de tiendas (UNA vez — estática en el
                        // runtime). FIX 2026-08-14: load_shops NUNCA se
                        // llamaba — la ShopTable estaba vacía y el click a
                        // un vendedor moría en silencio (parity
                        // StartShopping — shop_manager.cpp:102-152). Si
                        // falla, las tiendas quedan desactivadas.
                        if let Err(e) = world
                            .load_shops(&game_core::shop::ShopRepo::new(pool.clone()))
                            .await
                        {
                            eprintln!("server_realms: channel: player.shop: {e} — tiendas desactivadas");
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
                let pool = pool.clone();
                let batcher = batcher.clone();
                conn_id = conn_id.wrapping_add(1);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, cfg, id, tx, ms, pool, batcher).await {
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
    pool: database::pool::PgPool,
    batcher: std::sync::Arc<database::wal::Batcher>,
) -> Result<(), String> {
    let mut session = Session::new(stream, config, conn_id, intent_tx, map_store, pool, batcher);
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
async fn equipped_armor(
    inventory: &[database::item::ItemRow],
    pool: &database::pool::PgPool,
) -> Result<i32, String> {
    let mut armor = 0i32;
    for w in inventory.iter().filter(|i| i.window == "EQUIPMENT") {
        if let Some(p) = ItemRepo::new(pool.clone()).load_proto_use_values(w.vnum).await? {
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

/// `WEAR_ARROW = 9` (length.h:110) — el slot del arco/flechas (el cell del
/// wire = INVENTORY_MAX_NUM + wear, length.h:827).
const WEAR_ARROW: u16 = 9;
/// `WEAR_FOOTS = 2` (length.h:102 — las BOTAS; el nombre WEAR_SHOES es del
/// cliente, el server usa FOOTS; el cell del wire = INVENTORY_MAX_NUM + 2).
const WEAR_FOOTS: u16 = 2;
/// `WEAR_WEAPON = 4` (length.h:104) — el slot del arma (el cell del wire =
/// INVENTORY_MAX_NUM + wear). El daño del arma (value3/value4 —
/// `GetValue(3)/(4)`, battle.cpp:460-461) alimenta los `POINT_WEAPON_MIN/MAX`
/// del GC_POINTS (BattlePoints).
const WEAR_WEAPON: u16 = 4;

/// El proto de la BOTA equipada (WEAR_FOOTS → cell INVENTORY_MAX_NUM + 2):
/// el C27 lee sus applies `APPLY_MOV_SPEED` (`ModifyPoints` item.cpp:718-735
/// — el equip los aplica a POINT_MOV_SPEED sobre la base 100 del
/// ComputePoints PC, char.cpp:2245). `None` = sin botas equipadas.
async fn equipped_boots_proto(
    pool: &database::pool::PgPool,
    inventory: &[database::item::ItemRow],
) -> Result<Option<database::item::ProtoItem>, String> {
    for w in inventory.iter().filter(|i| i.window == "EQUIPMENT") {
        if w.pos as u16 == INVENTORY_MAX_NUM + WEAR_FOOTS {
            return ItemRepo::new(pool.clone()).load_proto_use_values(w.vnum).await;
        }
    }
    Ok(None)
}

/// El proto del ARMA equipada (WEAR_WEAPON — cell INVENTORY_MAX_NUM + 4):
/// `value3/value4` = el daño min/max del arma (parity `GetValue(3)/(4)` —
/// battle.cpp:460-461; el cliente lee los mismos values en `__SetWeaponPower`
/// para su ATT_MIN/ATT_MAX local). `None` = sin arma (manos vacías).
async fn equipped_weapon_proto(
    pool: &database::pool::PgPool,
    inventory: &[database::item::ItemRow],
) -> Result<Option<database::item::ProtoItem>, String> {
    for w in inventory.iter().filter(|i| i.window == "EQUIPMENT") {
        if w.pos as u16 == INVENTORY_MAX_NUM + WEAR_WEAPON {
            return ItemRepo::new(pool.clone()).load_proto_use_values(w.vnum).await;
        }
    }
    Ok(None)
}

/// El item de flechas EQUIPADO (WEAR_ARROW → cell 180+9=189): el slot que el
/// legacy lee con `GetWear(WEAR_ARROW)` (char_battle.cpp:2747 —
/// GetArrowAndBow; el count mínimo para disparar). `None` = sin flechas
/// equipadas.
pub(crate) fn equipped_arrow_index(inventory: &[database::item::ItemRow]) -> Option<usize> {
    inventory
        .iter()
        .position(|i| i.window == "EQUIPMENT" && i.pos as u16 == INVENTORY_MAX_NUM + WEAR_ARROW)
}

/// Consume UNA flecha del slot equipado (dw_arrow — parity `UseArrow`,
/// char_battle.cpp:2770-2789): el count baja y el item SE QUEDA con count 0
/// (el gate del próximo disparo lo rechaza — GetArrowAndBow = 0); el cliente
/// ve el count nuevo por GC_ITEM_UPDATE (38 B). Sin flecha equipada → no-op
/// (el disparo ya se resolvió; defensivo).
pub(crate) async fn consume_arrow(session: &mut Session) -> Result<(), String> {
    let Some(idx) = equipped_arrow_index(&session.inventory) else {
        return Ok(());
    };
    session.inventory[idx].count -= 1;
    let up = protocol::world::TPacketGCItemUpdate {
        header: protocol::world::TPacketGCItemUpdate::HEADER,
        cell: TItemPos {
            window: TItemPos::WINDOW_EQUIPMENT,
            cell: INVENTORY_MAX_NUM + WEAR_ARROW,
        },
        count: session.inventory[idx].count as u8,
        sockets: session.inventory[idx].sockets,
        attrs: session.inventory[idx].attrs,
    };
    session
        .send(&up.to_bytes())
        .await
        .map_err(|e| format!("enviando GC_ITEM_UPDATE (flecha): {e}"))?;
    ItemRepo::new(session.pool.clone())
        .upsert(&session.inventory[idx], session.row().id)
        .await?;
    eprintln!(
        "server_realms: channel conn {}: {} gastó 1 flecha (quedan {})",
        session.conn_id, session.row().name, session.inventory[idx].count
    );
    Ok(())
}

/// ¿El item del suelo es ORO? (vnum 1 — parity `GetType() == ITEM_ELK`,
/// char_item.cpp:5919-5926; el kill-drop y el drop manual usan vnum 1 —
/// parity DropGold). Helper del pickup (C22) — la usa events.rs.
pub(crate) fn is_gold_item(vnum: i64) -> bool {
    vnum == 1
}

/// El dir de quests por defecto (derivado del `map_path`): el `quest`
/// hermano del dir del mapa; si está vacío (el runtime desplegado tiene
/// `spain/quest` vacío y el corpus legacy en `germany/quest`), el `quest`
/// del locale `germany` hermano.
fn default_quest_dir(map_path: &str) -> String {
    let locale = std::path::Path::new(map_path)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    for cand in [locale.join("quest"), locale.join("germany").join("quest")] {
        if cand.is_dir()
            && std::fs::read_dir(&cand).map(|mut e| e.next().is_some()).unwrap_or(false)
        {
            return cand.display().to_string();
        }
    }
    locale.join("quest").display().to_string()
}

/// Carga el corpus de quests del runtime (`quest_path`): convierte cada
/// `.quest` legacy a DSL (`quest_dsl::convert::convert_corpus`) y concatena
/// los que convierten — el SUBSET usable con el estado actual del conversor
/// (el corpus 194/194 convierte; los constructos no mapeados se reportan y
/// los eventos con triggers no mapeados se omiten — quest_dsl). También
/// parsea `quest_text.txt` del mismo dir (clave<TAB>texto por línea — el
/// diccionario ADR-0009; ausente = claves sin resolver). `None` si el dir no
/// existe o nada convirtió (fail-open: el canal sigue sin quests).
fn load_quest_corpus(dir: &str) -> Option<(String, std::collections::HashMap<String, String>)> {
    let mut files: Vec<(String, String)> = Vec::new();
    collect_quest_files(std::path::Path::new(dir), &mut files);
    if files.is_empty() {
        eprintln!("server_realms: channel: quests: {dir} vacío o inexistente — sin quests");
        return None;
    }
    let (outputs, report) = quest_dsl::convert::convert_corpus(&files);
    for (f, e) in &report.failed {
        eprintln!("server_realms: channel: quest {f}: {e}");
    }
    if outputs.is_empty() {
        return None;
    }
    let mut text = String::new();
    for (_, dsl) in &outputs {
        text.push_str(dsl);
        text.push('\n');
    }
    let texts = load_quest_texts(dir);
    eprintln!(
        "server_realms: channel: quests convertidas: {} archivos ({} unmapped, {} fallidas; {} textos de quest)",
        outputs.len(),
        report.unmapped.len(),
        report.failed.len(),
        texts.len()
    );
    Some((text, texts))
}

/// `quest_text.txt` del dir de quests: una `clave<TAB>texto` por línea (el
/// formato legacy del quest_text de metin2). Ausente/vacío -> diccionario
/// vacío (fallback: las claves se envían tal cual).
fn load_quest_texts(dir: &str) -> std::collections::HashMap<String, String> {
    let path = std::path::Path::new(dir).join("quest_text.txt");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return std::collections::HashMap::new();
    };
    let mut texts = std::collections::HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('\t') {
            texts.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    texts
}

/// Recoge los `.quest` del dir (recursivo) — lossy UTF-8 (los archivos
/// legacy mezclan bytes CP949 en comentarios; el CLI del conversor hace lo
/// mismo).
fn collect_quest_files(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_quest_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "quest") {
            let rel = p.to_string_lossy().replace('\\', "/");
            if let Ok(bytes) = std::fs::read(&p) {
                out.push((rel, String::from_utf8_lossy(&bytes).into_owned()));
            }
        }
    }
}

/// `now_ms` — reloj del servidor en ms desde boot (parity `get_dword_time`).
/// Reloj del servidor en ms — BASE COMPARTIDA unix-ms (FIX P0-A 2026-08-14):
/// el AUTH usa la misma base y el cliente ancla su reloj al dwTime del
/// handshake del auth → el desfase de arranque auth/canal desaparece (el
/// kick del speedhack por skew era el síntoma — FastTimer/SlowTimer al
/// moverte tras un restart independiente). El wrap u32 de `now32` (49,7 días)
/// es parity del `get_dword_time` del C++.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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

    /// El wiring de quests (2026-08-13): un corpus fixture (2 archivos qc)
    /// se convierte a DSL (`load_quest_corpus`) y el texto carga en el
    /// QuestEngine — la quest del NPC con `when <vnum>.chat` queda disponible
    /// para el click.
    #[test]
    fn quest_corpus_loads_into_engine() {
        let dir = std::env::temp_dir().join(format!("quest_wire_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(
            dir.join("hello.quest"),
            "quest hello begin\n\tstate start begin\n\t\twhen login begin\n\t\t\tsay(gameforge.x._t)\n\t\tend\n\tend\nend\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("sub/npc1.quest"),
            "quest npc1 begin\n\tstate start begin\n\t\twhen 20084.chat begin\n\t\t\tsay(gameforge.npc1._s)\n\t\tend\n\tend\nend\n",
        )
        .unwrap();
        let (text, texts) = load_quest_corpus(dir.to_str().unwrap()).expect("corpus convierte");
        assert!(texts.is_empty(), "sin quest_text.txt: diccionario vacío");
        let engine = game_core::quest::QuestEngine::load(&text).expect("DSL parsea");
        let names: Vec<&str> = engine.quests().iter().map(|q| q.name.as_str()).collect();
        assert!(names.contains(&"hello") && names.contains(&"npc1"), "{names:?}");
        // La quest del NPC tiene el trigger chat 20084 (la asociación del click).
        let npc1 = engine.quest("npc1").expect("npc1");
        let has_chat = npc1.states[0].events.iter().any(|e| {
            e.triggers.iter().any(|t| {
                t.kind
                    == quest_dsl::ast::TriggerKind::Chat {
                        target: quest_dsl::ast::TriggerTarget::Num(20084),
                    }
            })
        });
        assert!(has_chat, "el trigger chat 20084 sobrevive la conversión");
        std::fs::remove_dir_all(&dir).unwrap();
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
