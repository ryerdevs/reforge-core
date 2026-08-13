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
}

/// Estado de un item del suelo que el pickup consume (mismo shape que el
/// `LiveGroundItem` del canal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemView {
    pub vnum: u32,
    pub count: u32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
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
    Attack { player_vid: u32, victim_vid: u32, b_type: u8, weapon: Option<ProtoItem> },
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
    UseSkill { player_vid: u32, skill_id: u32, target_vid: u32, weapon: Option<ProtoItem> },
}

/// C→S de ITEMS del suelo: el drop del kill (el mundo asigna el vid con
/// `VidAlloc` y crea la entidad — `DropResult`), el CG_ITEM_PICKUP (el mundo
/// reporta el item — `PickupResult`; la distancia y el inventario los decide
/// la conexión) y el commit del pickup (quita el item del suelo).
#[derive(Debug)]
pub enum ItemIntent {
    DropItem { player_vid: u32, vnum: u32, count: u32, x: i32, y: i32, z: i32 },
    PickupItem { player_vid: u32, item_vid: u32 },
    RemoveItem { item_vid: u32 },
}

/// Lane futuro SOCIAL (guild/party/emotes) — vacío hoy; el wrapper no cambia
/// al crecer (C3: cada lane escribe su sub-enum).
#[derive(Debug)]
pub enum SocialIntent {}

/// Lane futuro QUEST — vacío hoy; el wrapper no cambia al crecer.
#[derive(Debug)]
pub enum QuestIntent {}

/// Intents de la conexión hacia el mundo (mpsc unbounded — la tarea del
/// canal los procesa y enruta los eventos de vuelta). `Join` lo maneja la
/// tarea del canal (async — carga la tabla de spawns); el resto los procesa
/// `WorldSim::process_intent` (los dominios van en los sub-enums — C3).
#[derive(Debug)]
pub enum Intent {
    /// El jugador entra al mundo (tras el ENTERGAME). `out` = su cola de
    /// eventos (el routing del canal la registra con su vid). Lo maneja la
    /// tarea del canal (async — fuera de los dominios).
    Join { player: PlayerJoin, out: tokio::sync::mpsc::UnboundedSender<NpcEvent> },
    /// El jugador salió (disconnect/error — lo manda el RAII de la conexión).
    Leave { player_vid: u32 },
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
    MobAttack { player_vid: u32, vid: u32, vnum: i64, x: i32, y: i32, damage: i32 },
    /// El mob empezó a perseguir al jugador (aggro proactivo).
    AggroOn { player_vid: u32, vid: u32, vnum: i64 },
    /// El mob perdió el aggro por distancia.
    AggroOff { player_vid: u32, vid: u32, vnum: i64 },
    /// El mob se MATERIALIZÓ al acercarse el jugador — paquetes
    /// ADD(+INFO) ya construidos por `game_core::npc::entry_spawns` (parity
    /// byte-exacta del entry).
    Spawned { player_vid: u32, packets: Vec<Vec<u8>> },
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
}

/// S→C del dominio MOVIMIENTO: el mob se MOVIÓ (GC_MOVE FUNC_MOVE —
/// persecución o patrulla).
#[derive(Debug, Clone)]
pub enum MoveEvent {
    Moved { player_vid: u32, vid: u32, x: i32, y: i32, rot: u8, duration_ms: u32 },
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
    AffectRemoved { player_vid: u32, skill_id: u32, point: u8 },
}

/// S→C del dominio ITEMS: el drop se creó en el mundo (el vid lo asignó el
/// mundo — el canal manda GC_ITEM_GROUND_ADD + GC_ITEM_OWNERSHIP con él) y
/// la respuesta al pickup (el item, si sigue en el suelo).
#[derive(Debug, Clone)]
pub enum ItemEvent {
    /// El drop se creó en el mundo — el vid lo asignó el mundo (el canal
    /// manda GC_ITEM_GROUND_ADD + GC_ITEM_OWNERSHIP con él).
    DropResult { player_vid: u32, item_vid: u32, vnum: u32, count: u32, x: i32, y: i32, z: i32 },
    /// Respuesta al pickup: el item (si sigue en el suelo).
    PickupResult { player_vid: u32, item_vid: u32, item: Option<ItemView> },
}

/// Lane futuro SOCIAL (guild/party) — vacío hoy; el wrapper no cambia.
#[derive(Debug, Clone)]
pub enum SocialEvent {}

/// Lane futuro QUEST — vacío hoy; el wrapper no cambia.
#[derive(Debug, Clone)]
pub enum QuestEvent {}

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
            | CombatEvent::AttackResult { player_vid, .. } => *player_vid,
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
            | SkillEvent::AffectRemoved { player_vid, .. } => *player_vid,
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
    /// El jugador destino del evento (routing del canal) — sin variantes
    /// hoy (lane futuro).
    pub fn player_vid(&self) -> u32 {
        match *self {}
    }
}

impl QuestEvent {
    /// El jugador destino del evento (routing del canal) — sin variantes
    /// hoy (lane futuro).
    pub fn player_vid(&self) -> u32 {
        match *self {}
    }
}
