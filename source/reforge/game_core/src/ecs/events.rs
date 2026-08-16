//! Intents (mpsc — patrón Veloren, ADR-0010 §1) y eventos S→C del mundo.
//!
//! # Wrappers por dominio (C3, R-s6, 2026-08-13)
//!
//! `Intent`/`NpcEvent` son WRAPPERS sobre sub-enums por dominio:
//! `Combat`/`Move`/`Skill`/`Item`/`Social`/`Quest`. Cada lane futuro escribe
//! SU sub-enum (aquí o en su propio archivo) y los brazos del wrapper NO
//! cambian — los `From<Sub> for Wrapper` hacen la construcción `.into()`.
//!
//! `player_vid()` (el routing del canal) se delega a cada sub-enum.
//!
//! NOTA: `Intent::Join`/`Intent::Leave` quedan como variantes PLANAS del
//! wrapper — son el ciclo de vida de la sesión, que maneja la propia tarea
//! del canal (Join es async y no pasa por `process_intent`); no pertenecen a
//! ningún dominio. El mpsc sobrevive (UNO solo — `UnboundedSender<NpcEvent>`).

use std::collections::HashMap;

use database::item::ProtoItem;

/// Datos del jugador al entrar al mundo (los manda la conexión tras el
/// ENTERGAME — el mundo crea la entidad y materializa su vista inicial).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerJoin {
    pub vid: u32,
    pub map_index: u32,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub mp: i32,
    pub max_mp: i32,
    /// El blob de niveles de skills (`player.skill_level` — 255 × 6 B).
    pub skill_level: Vec<u8>,
    pub level: i32,
    pub ht: i32,
    pub armor: i32,
    pub job: u8,
    /// `player.skill_group` — el skillgroup del personaje (parity
    /// `GetSkillGroup()`; el `k` de las skills usa la tabla real por
    /// job/skillgroup/nivel).
    pub skill_group: i16,
    pub st: i32,
    pub dx: i32,
    pub iq: i32,
}

/// Datos del objetivo de un ataque que la conexión necesita (log del golpe y
/// flujo de kill/recompensa/drop — todo el estado PG-bound queda en la
/// conexión).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KillInfo {
    pub vnum: i64,
    pub x: i32,
    pub y: i32,
    /// HP tras el golpe (el log "({}/{})").
    pub hp: i32,
    pub max_hp: i32,
    pub exp: i64,
    pub gold_min: i32,
    pub gold_max: i32,
    pub drop_item: i64,
    /// Nivel del mob (C33 — el factor de exp por level-delta).
    pub mob_level: i32,
}

/// Estado de un item del suelo que el pickup consume (mismo shape que el
/// `LiveGroundItem` del canal). Sockets/attrs: los que llevó el drop desde
/// su creación (lane attrs — el pickup los copia al ItemRow del inventario).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemView {
    pub vnum: u32,
    pub count: u32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub sockets: [i64; 3],
    pub attrs: [(i16, i16); 7],
}

// ---------------------------------------------------------------------------
// Intent — C→S por dominio
// ---------------------------------------------------------------------------

/// C→S de COMBATE: el CG_ATTACK + los syncs de stats del jugador que
/// alimentan las fórmulas (hp/sp/armor/level — pociones, equipar, level-up).
#[derive(Debug)]
pub enum CombatIntent {
    /// CG_ATTACK del jugador: el mundo resuelve cooldown/rango/daño
    /// (`handle_attack` puro) y emite `AttackResult`. `weapon` = el proto del
    /// arma equipada (la query PG la hizo la conexión).
    Attack {
        player_vid: u32,
        victim_vid: u32,
        b_type: u8,
        weapon: Option<ProtoItem>,
    },
    /// CG_TARGET del jugador (61, 5 B: header + vid — Packet.h:1369-1372):
    /// el cliente pide la barra de vida del objetivo (parity `SetTarget`,
    /// char.cpp:5048-5094 → GC_TARGET con el hp%). El mundo responde con
    /// `TargetResult` (o nada si el vid no es un mob materializado).
    Target { player_vid: u32, target_vid: u32 },
    /// HP del jugador cambiado por la sesión (pociones/revive).
    SetHp { player_vid: u32, hp: i32 },
    /// SP del jugador cambiado por la sesión (pociones/revive — el coste de
    /// las skills lo paga el mundo).
    SetMp { player_vid: u32, mp: i32 },
    /// iArmor del equipo recalculado (equipar/desequipar — queries PG del
    /// canal; el valor no cambia con el resto de operaciones).
    SetArmor { player_vid: u32, armor: i32 },
    /// Level-up del kill (la DEF del ataque del mob lo usa).
    SetLevel { player_vid: u32, level: i32 },
    /// PK mode del jugador (CG_PVP 41 — el handler del canal lo manda al
    /// setear el flag de sesión; el gate PvP `battle_is_attackable` del
    /// mundo lo consume — el mundo es donde están AMBOS jugadores).
    SetPvpMode { player_vid: u32, on: bool },
    /// Party del jugador (el canal lo sincroniza en Joined/LeftParty —
    /// "cannot attack same party", pvp.cpp:439-441).
    SetParty {
        player_vid: u32,
        party_id: Option<u32>,
    },
}

/// C→S de MOVIMIENTO: el CG_MOVE aceptado por la validación del canal
/// (posición NUEVA — el AI persigue la posición nueva y el spawn dinámico
/// se evalúa desde ella).
#[derive(Debug)]
pub enum MoveIntent {
    Move { player_vid: u32, x: i32, y: i32 },
}

/// C→S de SKILLS: el CG_USE_SKILL (el mundo valida nivel/cooldown/SP/rango,
/// evalúa el poly y emite `SkillResult` — daño y/o buff). `weapon` = el
/// proto del arma equipada (la query PG la hizo la conexión — la var `atk`
/// del poly la usa).
#[derive(Debug)]
pub enum SkillIntent {
    UseSkill {
        player_vid: u32,
        skill_id: u32,
        target_vid: u32,
        weapon: Option<ProtoItem>,
    },
}

/// C→S de ITEMS del suelo: el drop del kill (el mundo asigna el vid con
/// `VidAlloc` y crea la entidad — `DropResult`), el CG_ITEM_PICKUP (el mundo
/// reporta el item — `PickupResult`; la distancia y el inventario los decide
/// la conexión) y el commit del pickup (quita el item del suelo).
#[derive(Debug)]
pub enum ItemIntent {
    DropItem {
        player_vid: u32,
        vnum: u32,
        count: u32,
        x: i32,
        y: i32,
        z: i32,
        /// Sockets del item del suelo (el canal los calculó al crear el
        /// drop — lane attrs; el jugador soltando un item del inventario
        /// pasa los del row).
        sockets: [i64; 3],
        /// Attrs (tipo, valor) del item del suelo.
        attrs: [(i16, i16); 7],
    },
    PickupItem {
        player_vid: u32,
        item_vid: u32,
    },
    RemoveItem {
        item_vid: u32,
    },
}

/// C→S de TIENDAS NPC (F6 social — parity `CInputMain::Shop`,
/// input_main.cpp:1034-1096): el Open llega por el click en el NPC
/// (CG_ON_CLICK — el mundo resuelve el shop por npc_vnum); Buy/Sell/Sell2
/// por el CG_SHOP (50). El mundo valida el estado (shop abierto, pos, precio
/// del `item_proto`); el CANAL aplica oro/inventario/DB (tiene el
/// WorldStore/Batcher) y manda los paquetes GC.
#[derive(Debug)]
pub enum ShopIntent {
    /// Click en un NPC: el mundo resuelve el shop (`shop.npc_vnum`), valida
    /// la distancia (`SHOP_MAX_DISTANCE` = 1000, shop.h:6) y emite
    /// `Opened` con los items (o NADA — parity: el C++ falla en silencio).
    Open { player_vid: u32, npc_vid: u32 },
    /// El jugador cerró la ventana (CG_SHOP END) — `Closed`.
    Close { player_vid: u32 },
    /// CG_SHOP BUY (pos): compra el item COMPLETO (count del shop) —
    /// `BuyResult` con el precio, o `BuyRejected`.
    Buy { player_vid: u32, pos: u8 },
    /// CG_SHOP SELL (cell): vende todo el stack de la celda del inventario.
    Sell { player_vid: u32, cell: u16 },
    /// CG_SHOP SELL2 (cell, count): vende `count` unidades.
    Sell2 {
        player_vid: u32,
        cell: u16,
        count: u32,
    },
}

/// S→C de tiendas NPC — eventos VALIDADOS del mundo; el canal aplica la
/// parte PG (oro + items, unidad ACID del Batcher) y traduce al wire.
#[derive(Debug, Clone)]
pub enum ShopEvent {
    /// El shop abrió (GC_SHOP START — el canal manda el item list del wire).
    Opened {
        player_vid: u32,
        npc_vid: u32,
        items: Vec<crate::shop::ShopItem>,
    },
    /// El shop cerró (GC_SHOP END).
    Closed { player_vid: u32 },
    /// Compra validada: pos + precio del stack (el canal chequea oro/hueco y
    /// aplica la DB).
    BuyResult {
        player_vid: u32,
        pos: u8,
        vnum: i64,
        count: i64,
        price: i64,
    },
    /// Venta validada (shop abierto): el canal resuelve el item de la celda
    /// (count = el pedido — 0 = todo el stack, parity shop_manager.cpp:294).
    SellResult {
        player_vid: u32,
        cell: u16,
        count: i64,
    },
    /// Compra rechazada (pos inválido / shop no abierto / soldout).
    BuyRejected {
        player_vid: u32,
        pos: u8,
        error: crate::shop::ShopError,
    },
    /// Venta rechazada (shop no abierto).
    SellRejected {
        player_vid: u32,
        cell: u16,
        error: crate::shop::ShopError,
    },
}

/// C→S del INTERCAMBIO (parity `CInputMain::Exchange`, input_main.cpp:
/// 1111-1272): el canal valida oro/distancia/estado y manda el intent; el
/// mundo corre la máquina de estado (`game_core::trade::TradeSession`) y
/// emite los `TradeEvent` a AMBOS jugadores (routing por player_vid).
#[derive(Debug)]
pub enum TradeIntent {
    /// CG_EXCHANGE START (arg1 = vid del target).
    Start { player_vid: u32, target_vid: u32 },
    /// CG_EXCHANGE ITEM_ADD: la fila COMPLETA del item (el commit necesita
    /// id/count/vnum/sockets/attrs) + display_pos de la ventana.
    ItemAdd {
        player_vid: u32,
        row: database::item::ItemRow,
        display_pos: u8,
    },
    /// CG_EXCHANGE ITEM_DEL (arg1 = display_pos).
    ItemDel { player_vid: u32, display_pos: u8 },
    /// CG_EXCHANGE ELK_ADD (arg1 = oro — el canal validó `gold <= row.gold`).
    GoldAdd { player_vid: u32, gold: i64 },
    /// CG_EXCHANGE ACCEPT.
    Accept { player_vid: u32 },
    /// CG_EXCHANGE CANCEL — aborta para ambos.
    Cancel { player_vid: u32 },
    /// El ejecutor confirmó el commit ACID (DB ok) — el mundo emite `Done`
    /// a ambos y libera el par.
    CommitOk { player_vid: u32 },
    /// El ejecutor rechazó el commit (validación o DB falló) — el mundo
    /// cancela para ambos (parity `CExchange::Accept` → `goto EXCHANGE_END`).
    CommitFail { player_vid: u32 },
}

/// Un item recibido por el trade (el row NUEVO ya commiteado — el receptor
/// lo re-coloca en su inventario y re-upsertea, idempotente por id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeReceivedItem {
    pub row: database::item::ItemRow,
}

/// S→C del intercambio — el wire GC_EXCHANGE (47 B, `Packet.h:1828-1838`).
#[derive(Debug, Clone)]
pub enum TradeEvent {
    /// El par arrancó (GC_EXCHANGE START — arg1 = el vid del otro).
    Start { player_vid: u32, other_vid: u32 },
    /// El target ya está ocupado (parity `GC_ALREADY` — exchange.cpp:85).
    Already { player_vid: u32 },
    /// Item añadido a la ventana (GC_EXCHANGE ITEM_ADD — is_me distingue la
    /// ventana propia de la del target).
    ItemAdded {
        player_vid: u32,
        is_me: bool,
        display_pos: u8,
        vnum: i64,
        count: i64,
        sockets: [i64; 3],
        attrs: [(i16, i16); 7],
    },
    /// Item quitado de la ventana (GC_EXCHANGE ITEM_DEL).
    ItemRemoved {
        player_vid: u32,
        is_me: bool,
        display_pos: u8,
    },
    /// Oro añadido (GC_EXCHANGE GOLD_ADD).
    GoldAdded {
        player_vid: u32,
        is_me: bool,
        gold: i64,
    },
    /// Estado de aceptación (GC_EXCHANGE ACCEPT — solo cuando el par NO
    /// completó; al completar va directo al commit, parity exchange.cpp:
    /// 587-592).
    AcceptState {
        player_vid: u32,
        is_me: bool,
        accept: bool,
    },
    /// El par completó: el EJECUTOR corre el commit ACID
    /// (`game_core::trade::build_commit_units` + `WorldStore::exchange`) y
    /// responde `CommitOk`/`CommitFail`.
    Commit {
        player_vid: u32,
        plan: crate::trade::TradeCommitPlan,
    },
    /// Trade completado (a AMBOS, tras el CommitOk): oro recibido + items
    /// recibidos (rows nuevos) + items entregados (para el GC_ITEM_DEL).
    Done {
        player_vid: u32,
        gold_delta: i64,
        received: Vec<TradeReceivedItem>,
        delivered: Vec<database::item::ItemRow>,
    },
    /// Cancelado (GC_EXCHANGE END — sin cambios).
    Cancelled { player_vid: u32 },
}

/// Lane SOCIAL (C3 + N1): TIENDAS NPC (compra/venta) + INTERCAMBIO
/// jugador↔jugador (F6). Guild/party crecen aquí después.
#[derive(Debug)]
pub enum SocialIntent {
    Shop(ShopIntent),
    Trade(TradeIntent),
}

impl From<ShopIntent> for SocialIntent {
    fn from(i: ShopIntent) -> Self {
        SocialIntent::Shop(i)
    }
}
impl From<TradeIntent> for SocialIntent {
    fn from(i: TradeIntent) -> Self {
        SocialIntent::Trade(i)
    }
}
/// Conveniencia de los call sites (canal/tests): `ShopIntent.into()` →
/// `Intent::Social(Shop)`.
impl From<ShopIntent> for Intent {
    fn from(i: ShopIntent) -> Self {
        Intent::Social(SocialIntent::Shop(i))
    }
}
impl From<TradeIntent> for Intent {
    fn from(i: TradeIntent) -> Self {
        Intent::Social(SocialIntent::Trade(i))
    }
}

/// C→S del dominio QUEST: los triggers de evento del jugador + la
/// reanudación del diálogo (CG_SCRIPT_ANSWER). El engine vive en
/// `game_core::quest` (N1: la primera variante era un error de compilación
/// en `systems/quest.rs` hasta implementarla).
#[derive(Debug)]
pub enum QuestIntent {
    /// Carga las quests DSL (texto renderizado por `quest_dsl`) en el mundo —
    /// la envía el canal al arrancar (directorio de quests del runtime).
    /// `texts` = diccionario clave->texto del quest_text del runtime
    /// (ADR-0009 — el server resuelve los textos de diálogo; vacío = las
    /// claves se envían tal cual).
    Load {
        text: String,
        texts: HashMap<String, String>,
    },
    /// Carga las filas persistidas del jugador (`player.quest` — la conexión
    /// las leyó con `QuestRepo::load` en el entry).
    Init {
        player_vid: u32,
        rows: Vec<crate::quest::PersistedFlag>,
    },
    /// Un trigger de evento. `items` = snapshot de counts del inventario
    /// (las condiciones `count_item` — la conexión lo calcula al enviar).
    Event {
        player_vid: u32,
        trigger: crate::quest::QuestTrigger,
        items: HashMap<u32, i64>,
    },
    /// CLICK a un NPC (CG_ON_CLICK — wiring 2026-08-13): el mundo resuelve
    /// el VNUM del vid (NpcIndex → Mob) y dispara el trigger `Chat(vnum)`
    /// — las quests con `when <vnum>.chat` ofrecen su diálogo. `items` =
    /// counts del inventario (igual que Event). Sin quests para el vnum →
    /// sin evento (silencio, parity StartShopping).
    NpcClick {
        player_vid: u32,
        npc_vid: u32,
        items: HashMap<u32, i64>,
    },
    /// La respuesta del diálogo suspendido (CG_SCRIPT_ANSWER: 1..n del
    /// select, 0 del [NEXT]).
    Answer { player_vid: u32, answer: u8 },
    /// CG_QUEST_INPUT_STRING (30): el texto del diálogo de input del quest.
    /// El engine aún no tiene la acción `input` del DSL (mapeada-pendiente) —
    /// el mundo lo loguea y no-op (GAP documentado, mod.rs §Cobertura).
    Input { player_vid: u32, text: String },
    /// CG_QUEST_CONFIRM (31): la respuesta del diálogo de confirmación
    /// (answer + el requestPID del jugador que espera). El engine no tiene
    /// confirmación cross-player — log + no-op (GAP documentado).
    Confirm {
        player_vid: u32,
        answer: u8,
        request_pid: u32,
    },
    /// CG_SCRIPT_BUTTON (66): el índice del botón del diálogo/ventana de
    /// quest (parity `ScriptButton` input_main.cpp:1850-1868 — Confirm
    /// timeout / QuestInfo si idx & 0x80000000 / QuestButton). El engine no
    /// tiene la API de botones — log + no-op (GAP documentado).
    Button { player_vid: u32, idx: u32 },
}

/// Intents de la conexión hacia el mundo (mpsc unbounded — la tarea del
/// canal los procesa y enruta los eventos de vuelta). `Join` lo maneja la
/// tarea del canal (async — carga la tabla de spawns); el resto los procesa
/// `WorldSim::process_intent` (los dominios van en los sub-enums — C3).
#[derive(Debug)]
pub enum Intent {
    /// El jugador entra al mundo (tras el ENTERGAME). `out` = su cola de
    /// eventos (el routing del canal la registra con su vid). Lo maneja la
    /// tarea del canal (async — fuera de los dominios).
    Join {
        player: PlayerJoin,
        out: tokio::sync::mpsc::UnboundedSender<NpcEvent>,
    },
    /// El jugador salió (disconnect/error — lo manda el RAII de la conexión).
    Leave {
        player_vid: u32,
    },
    Combat(CombatIntent),
    Move(MoveIntent),
    Skill(SkillIntent),
    Item(ItemIntent),
    Social(SocialIntent),
    Quest(QuestIntent),
}

impl From<CombatIntent> for Intent {
    fn from(i: CombatIntent) -> Self {
        Intent::Combat(i)
    }
}
impl From<MoveIntent> for Intent {
    fn from(i: MoveIntent) -> Self {
        Intent::Move(i)
    }
}
impl From<SkillIntent> for Intent {
    fn from(i: SkillIntent) -> Self {
        Intent::Skill(i)
    }
}
impl From<ItemIntent> for Intent {
    fn from(i: ItemIntent) -> Self {
        Intent::Item(i)
    }
}
impl From<SocialIntent> for Intent {
    fn from(i: SocialIntent) -> Self {
        Intent::Social(i)
    }
}
impl From<QuestIntent> for Intent {
    fn from(i: QuestIntent) -> Self {
        Intent::Quest(i)
    }
}

// ---------------------------------------------------------------------------
// NpcEvent — S→C por dominio (el canal enruta por `player_vid`)
// ---------------------------------------------------------------------------

/// S→C del dominio COMBATE: el ataque/aggro de los mobs, el resultado del
/// CG_ATTACK del jugador y el CICLO DE VIDA del mob (Spawned/Despawned —
/// pertenecen al dominio del mob; el lane de spawn no emite eventos propios).
#[derive(Debug, Clone)]
pub enum CombatEvent {
    /// El mob ATACÓ al jugador (GC_MOVE FUNC_ATTACK + GC_DAMAGE_INFO + daño
    /// ya aplicado al Hp del jugador en el mundo).
    MobAttack {
        player_vid: u32,
        vid: u32,
        vnum: i64,
        x: i32,
        y: i32,
        damage: i32,
    },
    /// El mob empezó a perseguir al jugador (aggro proactivo).
    AggroOn {
        player_vid: u32,
        vid: u32,
        vnum: i64,
    },
    /// El mob perdió el aggro por distancia.
    AggroOff {
        player_vid: u32,
        vid: u32,
        vnum: i64,
    },
    /// El mob se MATERIALIZÓ al acercarse el jugador — paquetes
    /// ADD(+INFO) ya construidos por `game_core::npc::entry_spawns` (parity
    /// byte-exacta del entry).
    Spawned {
        player_vid: u32,
        packets: Vec<Vec<u8>>,
    },
    /// El mob se DESMATERIALIZÓ (lejos de todos los jugadores) —
    /// GC_CHARACTER_DEL.
    Despawned { player_vid: u32, vid: u32 },
    /// Resultado del CG_ATTACK del jugador: los paquetes del golpe
    /// (GcAttack/GcDamageInfo — `handle_attack`) + el daño + el estado del
    /// objetivo. `victim = None` solo si el golpe no hizo daño (bloqueado).
    AttackResult {
        player_vid: u32,
        victim_vid: u32,
        packets: Vec<Vec<u8>>,
        damage: i32,
        dead: bool,
        victim: Option<KillInfo>,
    },
    /// Respuesta al CG_TARGET: el vid del objetivo + su HP% (parity
    /// `SetTarget`/`BroadcastTargetPacket` — GC_TARGET 63, char.cpp:
    /// 5048-5143; bHPPercent 0 para PCs — el subset solo apunta mobs).
    TargetResult {
        player_vid: u32,
        vid: u32,
        hp: i32,
        max_hp: i32,
    },
    /// El ataque del jugador contra OTRO JUGADOR (PvP — parity
    /// `battle_melee_attack` + `SendDamagePacket`): el daño va al Hp del PC
    /// víctima (mundo) y el `packets` (GC_DAMAGE_INFO) se manda al atacante
    /// Y a la víctima (char_battle.cpp:1508-1527 — ambos descs).
    /// `victim_hp` = HP del objetivo tras el golpe (el log del atacante).
    PvPAttackResult {
        player_vid: u32,
        victim_vid: u32,
        packets: Vec<Vec<u8>>,
        damage: i32,
        dead: bool,
        victim_hp: i32,
    },
    /// El GOLPE recibido de otro jugador (routing a la VÍCTIMA — la pareja
    /// del `PvPAttackResult` del mismo ataque): el canal aplica el daño al
    /// row, manda GC_DAMAGE_INFO + GC_POINTS (la barra) y GC_DEAD si murió
    /// (el flujo de muerte/revive compartido con el MobAttack).
    PvPVictimHit {
        player_vid: u32,
        attacker_vid: u32,
        packets: Vec<Vec<u8>>,
        damage: i32,
        dead: bool,
    },
}

/// S→C del dominio MOVIMIENTO: el mob se MOVIÓ (GC_MOVE FUNC_MOVE —
/// persecución o patrulla).
#[derive(Debug, Clone)]
pub enum MoveEvent {
    Moved {
        player_vid: u32,
        vid: u32,
        x: i32,
        y: i32,
        rot: u8,
        duration_ms: u32,
    },
}

/// S→C del dominio SKILLS: el resultado del CG_USE_SKILL (los paquetes del
/// daño, el flujo de kill si aplica, el coste SP/HP pagado y el buff
/// aplicado — GC_AFFECT_ADD) + la expiración de los buffs server-timed
/// (GC_AFFECT_REMOVE).
#[derive(Debug, Clone)]
pub enum SkillEvent {
    /// Resultado del CG_USE_SKILL: los paquetes del daño (GcDamageInfo —
    /// `game_core::skill::damage_flag_for_attr`), el flujo de kill si aplica, el
    /// coste SP/HP pagado (GC_POINTS en el canal) y el buff aplicado
    /// (GC_AFFECT_ADD). Vacío para los rechazos (cooldown/nivel/SP/rango —
    /// parity: el legacy rechaza en silencio).
    SkillResult {
        player_vid: u32,
        skill_id: u32,
        /// El vid del objetivo (el mob del daño — para el flujo de kill).
        victim_vid: u32,
        packets: Vec<Vec<u8>>,
        damage: i32,
        dead: bool,
        victim: Option<KillInfo>,
        sp_cost: i32,
        hp_cost: i32,
        /// El buff aplicado (elemento del wire — el canal lo envuelve en el
        /// TPacketGCAffectAdd; el mundo ya lo tiene en `Affects`).
        buff: Option<protocol::world::TPacketAffectElement>,
    },
    /// Un buff expiró (el mundo lo revirtió — GC_AFFECT_REMOVE en el canal).
    AffectRemoved {
        player_vid: u32,
        skill_id: u32,
        point: u8,
    },
    /// Modo SPLASH (área — flag SKILL_FLAG_SPLASH): el resultado COMPLETO
    /// del uso, UNA entrada por víctima (mobs Y PCs atacables dentro del
    /// radio, `lMaxHit` aplicado — parity `FuncSplashDamage`). El coste
    /// SP/cooldown se paga UNA vez por uso (no por víctima — el mundo ya lo
    /// aplicó; el canal descuenta el row con este evento). Los PCs golpeados
    /// reciben ADEMÁS su propio `SplashVictimHit` (routing a la víctima —
    /// parity `SendDamagePacket`: el mismo GC_DAMAGE_INFO a ambos descs).
    SplashResult {
        player_vid: u32,
        skill_id: u32,
        victims: Vec<SplashVictimInfo>,
        sp_cost: i32,
        hp_cost: i32,
    },
    /// El GOLPE recibido por un PC víctima del splash (routing a la VÍCTIMA
    /// — la pareja del `SplashResult` del caster): el canal manda el mismo
    /// GC_DAMAGE_INFO que ve el caster + el daño al row + GC_POINTS (la
    /// barra) + GC_DEAD si murió (flujo compartido con el PvP).
    SplashVictimHit {
        player_vid: u32,
        attacker_vid: u32,
        packets: Vec<Vec<u8>>,
        damage: i32,
        dead: bool,
    },
}

/// Una víctima del modo SPLASH (`SkillEvent::SplashResult`): su paquete
/// GC_DAMAGE_INFO (el caster ve TODOS), el daño, si murió y — mobs — los
/// datos del kill (el canal aplica la recompensa UNA vez por víctima).
#[derive(Debug, Clone)]
pub struct SplashVictimInfo {
    pub victim_vid: u32,
    pub packets: Vec<Vec<u8>>,
    pub damage: i32,
    pub dead: bool,
    pub victim: Option<KillInfo>,
}

/// S→C del dominio ITEMS: el drop se creó en el mundo (el vid lo asignó el
/// mundo — el canal manda GC_ITEM_GROUND_ADD + GC_ITEM_OWNERSHIP con él) y
/// la respuesta al pickup (el item, si sigue en el suelo). Los sockets/attrs
/// viajan con el drop (los pobló el canal al crearlo — parity CreateItem).
#[derive(Debug, Clone)]
pub enum ItemEvent {
    /// El drop se creó en el mundo — el vid lo asignó el mundo (el canal
    /// manda GC_ITEM_GROUND_ADD + GC_ITEM_OWNERSHIP con él).
    DropResult {
        player_vid: u32,
        item_vid: u32,
        vnum: u32,
        count: u32,
        x: i32,
        y: i32,
        z: i32,
        sockets: [i64; 3],
        attrs: [(i16, i16); 7],
    },
    /// Respuesta al pickup: el item (si sigue en el suelo).
    PickupResult {
        player_vid: u32,
        item_vid: u32,
        item: Option<ItemView>,
    },
}

/// S→C del lane SOCIAL (F6): tiendas NPC + intercambio — el canal enruta
/// por `player_vid` y `social::emit` traduce al wire (GC_SHOP/GC_EXCHANGE).
#[derive(Debug, Clone)]
pub enum SocialEvent {
    Shop(ShopEvent),
    Trade(TradeEvent),
}

/// S→C del dominio QUEST: el resultado de procesar un intent — el diálogo
/// (GC_SCRIPT 45), los efectos accionables y las filas sucias a persistir.
#[derive(Debug, Clone)]
pub enum QuestEvent {
    /// El resultado del evento/reanudación: `script` = markup del GC_SCRIPT
    /// (None sin diálogo); `effects` = items/warp/notice (los aplica la
    /// conexión); `dirty` = filas `player.quest` (value 0 = delete);
    /// `suspended` = la quest espera CG_SCRIPT_ANSWER.
    Run {
        player_vid: u32,
        script: Option<String>,
        effects: Vec<crate::quest::QuestEffect>,
        dirty: Vec<crate::quest::DirtyFlag>,
        suspended: bool,
    },
}

/// Eventos S→C que el canal enruta por `player_vid` (la cola de cada
/// conexión solo recibe los suyos). Wrapper por dominio (C3): los brazos
/// son estables — cada lane futuro crece en SU sub-enum.
#[derive(Debug, Clone)]
pub enum NpcEvent {
    Combat(CombatEvent),
    Move(MoveEvent),
    Skill(SkillEvent),
    Item(ItemEvent),
    Social(SocialEvent),
    Quest(QuestEvent),
}

impl NpcEvent {
    /// El jugador destino del evento (el routing del canal por conexión) —
    /// delegado a cada sub-enum (C3).
    pub fn player_vid(&self) -> u32 {
        match self {
            NpcEvent::Combat(e) => e.player_vid(),
            NpcEvent::Move(e) => e.player_vid(),
            NpcEvent::Skill(e) => e.player_vid(),
            NpcEvent::Item(e) => e.player_vid(),
            NpcEvent::Social(e) => e.player_vid(),
            NpcEvent::Quest(e) => e.player_vid(),
        }
    }
}

impl From<CombatEvent> for NpcEvent {
    fn from(e: CombatEvent) -> Self {
        NpcEvent::Combat(e)
    }
}
impl From<MoveEvent> for NpcEvent {
    fn from(e: MoveEvent) -> Self {
        NpcEvent::Move(e)
    }
}
impl From<SkillEvent> for NpcEvent {
    fn from(e: SkillEvent) -> Self {
        NpcEvent::Skill(e)
    }
}
impl From<ItemEvent> for NpcEvent {
    fn from(e: ItemEvent) -> Self {
        NpcEvent::Item(e)
    }
}
impl From<SocialEvent> for NpcEvent {
    fn from(e: SocialEvent) -> Self {
        NpcEvent::Social(e)
    }
}
impl From<QuestEvent> for NpcEvent {
    fn from(e: QuestEvent) -> Self {
        NpcEvent::Quest(e)
    }
}

impl CombatEvent {
    /// El jugador destino del evento (routing del canal).
    pub fn player_vid(&self) -> u32 {
        match self {
            CombatEvent::MobAttack { player_vid, .. }
            | CombatEvent::AggroOn { player_vid, .. }
            | CombatEvent::AggroOff { player_vid, .. }
            | CombatEvent::Spawned { player_vid, .. }
            | CombatEvent::Despawned { player_vid, .. }
            | CombatEvent::AttackResult { player_vid, .. }
            | CombatEvent::TargetResult { player_vid, .. }
            | CombatEvent::PvPAttackResult { player_vid, .. }
            | CombatEvent::PvPVictimHit { player_vid, .. } => *player_vid,
        }
    }
}

impl MoveEvent {
    /// El jugador destino del evento (routing del canal).
    pub fn player_vid(&self) -> u32 {
        match self {
            MoveEvent::Moved { player_vid, .. } => *player_vid,
        }
    }
}

impl SkillEvent {
    /// El jugador destino del evento (routing del canal).
    pub fn player_vid(&self) -> u32 {
        match self {
            SkillEvent::SkillResult { player_vid, .. }
            | SkillEvent::AffectRemoved { player_vid, .. }
            | SkillEvent::SplashResult { player_vid, .. }
            | SkillEvent::SplashVictimHit { player_vid, .. } => *player_vid,
        }
    }
}

impl ItemEvent {
    /// El jugador destino del evento (routing del canal).
    pub fn player_vid(&self) -> u32 {
        match self {
            ItemEvent::DropResult { player_vid, .. }
            | ItemEvent::PickupResult { player_vid, .. } => *player_vid,
        }
    }
}

impl SocialEvent {
    /// El jugador destino del evento (routing del canal) — delegado a los
    /// sub-eventos del lane (shop/trade).
    pub fn player_vid(&self) -> u32 {
        match self {
            SocialEvent::Shop(e) => match e {
                ShopEvent::Opened { player_vid, .. }
                | ShopEvent::Closed { player_vid }
                | ShopEvent::BuyResult { player_vid, .. }
                | ShopEvent::SellResult { player_vid, .. }
                | ShopEvent::BuyRejected { player_vid, .. }
                | ShopEvent::SellRejected { player_vid, .. } => *player_vid,
            },
            SocialEvent::Trade(e) => match e {
                TradeEvent::Start { player_vid, .. }
                | TradeEvent::Already { player_vid }
                | TradeEvent::ItemAdded { player_vid, .. }
                | TradeEvent::ItemRemoved { player_vid, .. }
                | TradeEvent::GoldAdded { player_vid, .. }
                | TradeEvent::AcceptState { player_vid, .. }
                | TradeEvent::Commit { player_vid, .. }
                | TradeEvent::Done { player_vid, .. }
                | TradeEvent::Cancelled { player_vid } => *player_vid,
            },
        }
    }
}

impl QuestEvent {
    /// El jugador destino del evento (routing del canal).
    pub fn player_vid(&self) -> u32 {
        match self {
            QuestEvent::Run { player_vid, .. } => *player_vid,
        }
    }
}
