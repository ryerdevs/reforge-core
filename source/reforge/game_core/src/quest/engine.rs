//! `game_core::quest::engine` — el motor puro de las quests DSL.
//!
//! `QuestEngine` (parse + expansión de familias vía `quest_dsl`) evalúa los
//! eventos de UN jugador: matching de triggers, condiciones (el catálogo de
//! Expr del spec §4) y ejecución de acciones (el catálogo de ActionName del
//! spec §5) → `QuestOutcome` (diálogo GC_SCRIPT, efectos accionables, filas
//! sucias de `player.quest`, suspensión wait/select).
//!
//! Diseño: 100% puro y testeable (sin ECS, sin I/O) — el estado del jugador
//! vive en `QuestRuntime` (estados + flags + suspensión) y el contexto de
//! evaluación (`EvalCtx`) inyecta nivel/mapa/reloj/inventario/RNG/captures.
//! La integración ECS (world) y la conexión (paquetes/persistencia) viven en
//! `ecs/systems/quest.rs` y `server_realms/channel/quest.rs`.

use std::collections::HashMap;

use quest_dsl::ast::{ActionName, Expr, FuncName, Stmt, TriggerKind, TriggerTarget, Value};
use quest_dsl::QuestDef;

/// Trigger de evento del jugador (mapea al `TriggerKind` del DSL — spec §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestTrigger {
    Login,
    LevelUp,
    Letter,
    Button,
    Info,
    Enter,
    Logout,
    Timer,
    TargetClick,
    Kill(u32),
    Chat(u32),
    Use(u32),
}

impl QuestTrigger {
    /// ¿Coincide con un trigger del DSL? (Post-expansión de familias — los
    /// `TriggerTarget::Param` ya se resolvieron; si quedara uno, no matchea.)
    pub fn matches(self, kind: &TriggerKind) -> bool {
        let tnum = |t: &TriggerTarget| match t {
            TriggerTarget::Num(n) => Some(*n),
            TriggerTarget::Param(_) => None,
        };
        match (self, kind) {
            (QuestTrigger::Login, TriggerKind::Login)
            | (QuestTrigger::LevelUp, TriggerKind::LevelUp)
            | (QuestTrigger::Letter, TriggerKind::Letter)
            | (QuestTrigger::Button, TriggerKind::Button)
            | (QuestTrigger::Info, TriggerKind::Info)
            | (QuestTrigger::Enter, TriggerKind::Enter)
            | (QuestTrigger::Logout, TriggerKind::Logout)
            | (QuestTrigger::Timer, TriggerKind::Timer)
            | (QuestTrigger::TargetClick, TriggerKind::TargetClick) => true,
            (QuestTrigger::Kill(v), TriggerKind::Kill { target }) => tnum(target) == Some(v),
            (QuestTrigger::Chat(v), TriggerKind::Chat { target }) => tnum(target) == Some(v),
            (QuestTrigger::Use(v), TriggerKind::Use { target }) => tnum(target) == Some(v),
            _ => false,
        }
    }
}

/// Fila persistida de `player.quest` (carga del entry — la conexión las leyó
/// con `QuestRepo::load`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedFlag {
    pub quest: String,
    pub flag: String,
    pub value: i64,
}

/// Fila SUCIA de `player.quest` (set_qf/set_state — la conexión la persiste
/// con `QuestRepo::save`; `value == 0` = DELETE, parity QUERY_QUEST_SAVE).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyFlag {
    pub quest: String,
    pub flag: String,
    pub value: i64,
}

/// Efecto accionable del evento (los aplica la conexión — inventario, warp,
/// notice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestEffect {
    /// Recompensa: item al inventario (`give_item2`).
    GiveItem { vnum: u32, count: u32 },
    /// Quitar item del inventario (`remove_item`).
    RemoveItem { vnum: u32, count: u32 },
    /// Teleport (`warp` → GC_WARP 65).
    Warp { x: i64, y: i64 },
    /// Aviso (`notice` → GC_CHAT 4, CHAT_TYPE_NOTICE).
    Notice(String),
    /// Carta de quest (`send_letter` → GC_SCRIPT con [LETTER]).
    SendLetter(String),
    /// Flecha de quest (parity `target.vid`, questlua_target.cpp).
    TargetVid { name: String, vid: u32, title: String },
    /// Borra flecha (`target.delete`).
    TargetDelete { name: String },
    /// Buff de quest (`affect.add_collect`).
    AffectAdd { apply: String, value: i32, duration: i32 },
    /// Quita buff (`affect.remove_all_collect`).
    AffectRemove { apply: String },
}

/// Estado runtime de las quests de UN jugador.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestRuntime {
    /// Estado actual por quest: índice 1-based del state (parity
    /// `{quest}.__status`); 0 = no empezada.
    pub states: HashMap<String, usize>,
    /// Flags: (quest, flag) → valor (parity filas `player.quest`).
    pub flags: HashMap<(String, String), i64>,
    /// Evento suspendido (wait/select) esperando CG_SCRIPT_ANSWER.
    pub suspended: Option<Suspension>,
}

impl QuestRuntime {
    /// Carga las filas persistidas del entry: `{quest}.__status` = el estado
    /// actual; el resto = flags.
    pub fn load(rows: &[PersistedFlag]) -> QuestRuntime {
        let mut rt = QuestRuntime::default();
        for r in rows {
            let status = format!("{}.__status", r.quest);
            if r.flag == status {
                rt.states.insert(r.quest.clone(), r.value.max(0) as usize);
            } else {
                rt.flags.insert((r.quest.clone(), r.flag.clone()), r.value);
            }
        }
        rt
    }
}

/// Una quest suspendida (wait/select): la reanudación entra por el body del
/// evento en `stmt` (dentro del branch `branch` si aplica).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suspension {
    pub quest: String,
    /// Índice 1-based del state del evento suspendido.
    pub state: usize,
    /// Índice del evento dentro del state.
    pub event: usize,
    /// Índice de la PRÓXIMA sentencia (en el body del evento o del branch).
    pub stmt: usize,
    /// Índice del branch contenedor (None = nivel evento).
    pub branch: Option<usize>,
    /// Capture del select (`as name`) — None para wait.
    pub capture: Option<String>,
}

/// Contexto de evaluación de condiciones (lo que el mundo/la conexión
/// conocen). Los flags NO viven aquí (el engine los pasa por separado — los
/// set_qf del evento deben verse en las condiciones posteriores).
pub struct EvalCtx<'a> {
    pub level: i32,
    pub map_index: u32,
    /// `get_time()` = segundos del reloj del server (now_ms / 1000).
    pub now_s: i64,
    /// Snapshot de counts del inventario (`count_item(vnum)` — lo calcula la
    /// conexión al enviar el intent).
    pub items: &'a HashMap<u32, i64>,
    /// Captures activos (la respuesta del select — `as name`).
    pub captures: &'a HashMap<String, i64>,
    /// `number(min, max)` — el RNG del mundo (roll INCLUSIVE).
    pub rng: &'a mut dyn FnMut(i64, i64) -> i64,
}

/// Resultado de procesar un evento/reanudación.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuestOutcome {
    /// Diálogo GC_SCRIPT (markup legacy) — None sin texto.
    pub script: Option<String>,
    pub effects: Vec<QuestEffect>,
    pub dirty: Vec<DirtyFlag>,
    /// La quest quedó suspendida esperando CG_SCRIPT_ANSWER.
    pub suspended: bool,
}

/// El motor de quests: las quests DSL cargadas (parse + familias expandidas) y
/// el diccionario de textos de diálogo (ADR-0009 — el server es dueño del
/// texto; resolución de claves `gameforge.*` con fallback a la clave).
pub struct QuestEngine {
    quests: Vec<QuestDef>,
    /// clave -> texto (cargado del quest_text del runtime; vacío = las
    /// claves se envían tal cual — el cliente las resuelve de su pack).
    texts: std::collections::HashMap<String, String>,
}

impl QuestEngine {
    /// Carga un archivo DSL (texto renderizado por `quest_dsl`) — parse +
    /// expansión de familias. NOTA: `expand_families` devuelve SOLO las
    /// instancias — las quests concretas del archivo se recogen aparte.
    pub fn load(text: &str) -> Result<QuestEngine, String> {
        let file = quest_dsl::parse(text).map_err(|e| e.to_string())?;
        let mut quests = Vec::new();
        for q in &file.quests {
            if let quest_dsl::ast::Quest::Concrete(d) = q {
                quests.push(d.clone());
            }
        }
        quests.extend(quest_dsl::expand_families(&file)?);
        Ok(QuestEngine { quests, texts: std::collections::HashMap::new() })
    }

    /// Adjunta el diccionario de textos (clave -> texto) — la resolución
    /// server-side de las claves de diálogo (ADR-0009). Sin diccionario, las
    /// claves se envían tal cual (el cliente las resuelve de su pack).
    pub fn with_texts(mut self, texts: std::collections::HashMap<String, String>) -> Self {
        self.texts = texts;
        self
    }

    /// Las quests concretas (familias ya expandidas) — en orden del archivo.
    pub fn quests(&self) -> &[QuestDef] {
        &self.quests
    }

    /// Una quest por nombre.
    pub fn quest(&self, name: &str) -> Option<&QuestDef> {
        self.quests.iter().find(|q| q.name == name)
    }

    /// Índice 1-based de un state por nombre (parity `GetQuestStateIndex`).
    fn state_index(q: &QuestDef, name: &str) -> Option<usize> {
        q.states.iter().position(|s| s.name == name).map(|i| i + 1)
    }

    /// Un trigger de evento del jugador (login/kill/chat/...). Vacío si la
    /// quest está suspendida (parity `pc.IsRunning()` — sin otros eventos
    /// mientras hay un diálogo arriba).
    // Los 8 params son las entradas del motor puro (estado + contexto) — el
    // bundling en un struct es YAGNI para este slice (ver la nota del módulo).
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        rt: &mut QuestRuntime,
        trigger: QuestTrigger,
        level: i32,
        map_index: u32,
        now_s: i64,
        items: &HashMap<u32, i64>,
        rng: &mut dyn FnMut(i64, i64) -> i64,
    ) -> QuestOutcome {
        let empty = HashMap::new();
        let mut ctx = EvalCtx { level, map_index, now_s, items, captures: &empty, rng };
        let mut out = QuestOutcome::default();
        if rt.suspended.is_some() {
            return out;
        }
        // Snapshot de flags para la evaluación (los set_qf del evento
        // sincronizan ambos — las condiciones posteriores ven el valor nuevo).
        let mut flags = rt.flags.clone();
        for q in &self.quests {
            let st = rt.states.get(&q.name).copied().unwrap_or(0);
            let target = if st == 0 { 1 } else { st };
            let Some(sd) = q.states.get(target - 1) else { continue };
            for (ei, ev) in sd.events.iter().enumerate() {
                if !ev.triggers.iter().any(|t| trigger.matches(&t.kind)) {
                    continue;
                }
                let cond_ok = match &ev.condition {
                    None => true,
                    Some(c) => match self.eval_expr(&flags, &q.name, c, &mut ctx) {
                        Ok(v) => v != 0,
                        Err(e) => {
                            eprintln!("game_core: quest {}: condición del evento {ei}: {e}", q.name);
                            false
                        }
                    },
                };
                if !cond_ok {
                    continue;
                }
                // Dispara: la quest ARRANCA si no estaba corriendo (parity
                // FuncMissHandleEvent — los eventos del state start de las
                // quests no iniciadas).
                if st == 0 {
                    rt.states.insert(q.name.clone(), 1);
                    out.dirty.push(DirtyFlag {
                        quest: q.name.clone(),
                        flag: format!("{}.__status", q.name),
                        value: 1,
                    });
                }
                let mut script = String::new();
                let res = self.exec_body(
                    rt, &mut flags, q, target, ei, &ev.body, 0, None, &mut ctx, &mut script, &mut out,
                );
                match res {
                    Ok(Some(susp)) => {
                        out.script = Some(script);
                        rt.suspended = Some(susp);
                        out.suspended = true;
                        return out;
                    }
                    Ok(None) => {
                        if !script.is_empty() {
                            out.script = Some(script);
                        }
                    }
                    Err(e) => {
                        eprintln!("game_core: quest {}: evento {ei}: {e}", q.name);
                    }
                }
            }
        }
        out
    }

    /// Reanuda la quest suspendida con la respuesta del cliente
    /// (CG_SCRIPT_ANSWER): el valor del select (1..n) se ata al capture
    /// (`as name`); el wait reanuda sin capture. No-op sin suspensión.
    #[allow(clippy::too_many_arguments)]
    pub fn answer(
        &self,
        rt: &mut QuestRuntime,
        answer: i64,
        level: i32,
        map_index: u32,
        now_s: i64,
        items: &HashMap<u32, i64>,
        rng: &mut dyn FnMut(i64, i64) -> i64,
    ) -> QuestOutcome {
        let mut out = QuestOutcome::default();
        let Some(susp) = rt.suspended.take() else { return out };
        let Some(q) = self.quest(&susp.quest) else { return out };
        let Some(sd) = q.states.get(susp.state - 1) else { return out };
        let Some(ev) = sd.events.get(susp.event) else { return out };
        let body = match susp.branch {
            Some(b) => match ev.body.get(b) {
                Some(Stmt::Branch(br)) => &br.body,
                _ => return out,
            },
            None => &ev.body,
        };
        let mut captures = HashMap::new();
        if let Some(c) = &susp.capture {
            captures.insert(c.clone(), answer);
        }
        let mut ctx = EvalCtx { level, map_index, now_s, items, captures: &captures, rng };
        let mut flags = rt.flags.clone();
        let mut script = String::new();
        let res = self.exec_body(
            rt, &mut flags, q, susp.state, susp.event, body, susp.stmt, susp.branch, &mut ctx, &mut script,
            &mut out,
        );
        match res {
            Ok(Some(s2)) => {
                out.script = Some(script);
                rt.suspended = Some(s2);
                out.suspended = true;
            }
            Ok(None) => {
                if !script.is_empty() {
                    out.script = Some(script);
                }
            }
            Err(e) => {
                eprintln!("game_core: quest {}: reanudación: {e}", q.name);
            }
        }
        out
    }

    /// Ejecuta `body[from..]` (del evento o del branch) acumulando el
    /// diálogo, los efectos y las filas sucias. Devuelve la suspensión si una
    /// wait/select cortó la ejecución.
    #[allow(clippy::too_many_arguments)]
    fn exec_body(
        &self,
        rt: &mut QuestRuntime,
        flags: &mut HashMap<(String, String), i64>,
        q: &QuestDef,
        state: usize,
        event: usize,
        body: &[Stmt],
        from: usize,
        branch: Option<usize>,
        ctx: &mut EvalCtx,
        script: &mut String,
        out: &mut QuestOutcome,
    ) -> Result<Option<Suspension>, String> {
        let mut i = from;
        while i < body.len() {
            match &body[i] {
                Stmt::Action { action, capture } => {
                    let cap = capture.clone();
                    let args = action.args.as_slice();
                    match &action.name {
                        ActionName::SayTitle | ActionName::Say => {
                            // Parity `_say` (questlua_global.cpp:62-68): texto +
                            // [ENTER]. say_title usa el mismo markup (el título
                            // del diálogo es un skin del cliente — la clave se
                            // envía sin resolver: la resolución de locale es un
                            // slice futuro, ADR-0009).
                            let k = args.first().ok_or_else(|| "say sin clave".to_string())?;
                            script.push_str(&self.key_text(k)?);
                            script.push_str("[ENTER]");
                        }
                        ActionName::Wait => {
                            // Parity `member_wait`: [NEXT] + suspensión — la
                            // reanudación llega por CG_SCRIPT_ANSWER (answer 0).
                            script.push_str("[NEXT]");
                            return Ok(Some(Suspension {
                                quest: q.name.clone(),
                                state,
                                event,
                                stmt: i + 1,
                                branch,
                                capture: None,
                            }));
                        }
                        ActionName::Select => {
                            // Parity `GotoSelectState` (questlua.cpp:901-937):
                            // `[QUESTION 1;key|2;key|...]` + suspensión; el
                            // answer (1..n) es el resultado del select.
                            script.push_str("[QUESTION ");
                            for (n, k) in args.iter().enumerate() {
                                if n > 0 {
                                    script.push('|');
                                }
                                script.push_str(&format!("{};{}", n + 1, self.key_text(k)?));
                            }
                            script.push(']');
                            return Ok(Some(Suspension {
                                quest: q.name.clone(),
                                state,
                                event,
                                stmt: i + 1,
                                branch,
                                capture: cap,
                            }));
                        }
                        ActionName::SetState => {
                            // Parity `set_state` → flag `{quest}.__status`
                            // (questpc.cpp:115-118) — índice 1-based.
                            let name = args.first().ok_or_else(|| "set_state sin nombre".to_string())?;
                            let target = Self::name_text(name)?;
                            let Some(idx) = Self::state_index(q, &target) else {
                                return Err(format!("set_state a estado desconocido `{target}`"));
                            };
                            rt.states.insert(q.name.clone(), idx);
                            out.dirty.push(DirtyFlag {
                                quest: q.name.clone(),
                                flag: format!("{}.__status", q.name),
                                value: idx as i64,
                            });
                        }
                        ActionName::SetQf => {
                            let flag = Self::name_text(args.first().ok_or_else(|| "set_qf sin nombre".to_string())?)?;
                            let value = args.get(1).ok_or_else(|| "set_qf sin valor".to_string())?;
                            let v = self.eval_arg(flags, &q.name, value, ctx)?;
                            rt.flags.insert((q.name.clone(), flag.clone()), v);
                            flags.insert((q.name.clone(), flag.clone()), v);
                            out.dirty.push(DirtyFlag { quest: q.name.clone(), flag, value: v });
                        }
                        ActionName::GiveItem2 => {
                            let vnum = args.first().ok_or_else(|| "give_item2 sin vnum".to_string())?;
                            let v = u32::try_from(self.eval_arg(flags, &q.name, vnum, ctx)?)
                                .map_err(|_| format!("give_item2 vnum fuera de rango: {vnum:?}"))?;
                            let c = match args.get(1) {
                                Some(c) => u32::try_from(self.eval_arg(flags, &q.name, c, ctx)?)
                                    .map_err(|_| "give_item2 count fuera de rango".to_string())?,
                                None => 1,
                            };
                            out.effects.push(QuestEffect::GiveItem { vnum: v, count: c.max(1) });
                        }
                        ActionName::RemoveItem => {
                            let vnum = args.first().ok_or_else(|| "remove_item sin vnum".to_string())?;
                            let v = u32::try_from(self.eval_arg(flags, &q.name, vnum, ctx)?)
                                .map_err(|_| format!("remove_item vnum fuera de rango: {vnum:?}"))?;
                            let c = u32::try_from(self.eval_arg(flags, &q.name, args.get(1).ok_or_else(|| "remove_item sin count".to_string())?, ctx)?)
                                .map_err(|_| "remove_item count fuera de rango".to_string())?;
                            out.effects.push(QuestEffect::RemoveItem { vnum: v, count: c.max(1) });
                        }
                        ActionName::Warp => {
                            let x = self.eval_arg(flags, &q.name, args.first().ok_or_else(|| "warp sin x".to_string())?, ctx)?;
                            let y = self.eval_arg(flags, &q.name, args.get(1).ok_or_else(|| "warp sin y".to_string())?, ctx)?;
                            out.effects.push(QuestEffect::Warp { x, y });
                        }
                        ActionName::Notice => {
                            let k = args.first().ok_or_else(|| "notice sin texto".to_string())?;
                            out.effects.push(QuestEffect::Notice(self.key_text(k)?));
                        }
                        ActionName::SayReward => {
                            let k = args.first().ok_or_else(|| "say_reward sin clave".to_string())?;
                            script.push_str(&self.key_text(k)?);
                            script.push_str("[ENTER]");
                        }
                        ActionName::SendLetter => {
                            let k = args.first().ok_or_else(|| "send_letter sin clave".to_string())?;
                            out.effects.push(QuestEffect::SendLetter(self.key_text(k)?));
                        }
                        ActionName::SetQuestState => {
                            // Parity `_set_quest_state` (questlua_global.cpp:872-906 +
                            // questpc.cpp:120-131): cruza a otra quest; si
                            // quest==actual, actualiza `pqs->st` además del flag.
                            let qn = Self::name_text(args.first().ok_or_else(|| "set_quest_state sin quest".to_string())?)?;
                            let sn = Self::name_text(args.get(1).ok_or_else(|| "set_quest_state sin state".to_string())?)?;
                            let Some(tq) = self.quests.iter().find(|qq| qq.name == qn) else {
                                return Err(format!("set_quest_state quest desconocida `{qn}`"));
                            };
                            let Some(idx) = Self::state_index(tq, &sn) else {
                                return Err(format!("set_quest_state state desconocido `{qn}.{sn}`"));
                            };
                            rt.states.insert(qn.clone(), idx);
                            out.dirty.push(DirtyFlag { quest: qn.clone(), flag: format!("{qn}.__status"), value: idx as i64 });
                        }
                        ActionName::TargetVid => {
                            let name = Self::name_text(args.first().ok_or("target_vid sin name")?)?;
                            let vid = u32::try_from(self.eval_arg(flags, &q.name, args.get(1).ok_or("target_vid sin vid")?, ctx)?).map_err(|_| "target_vid vid fuera de rango")?;
                            let title = self.key_text(args.get(2).ok_or("target_vid sin key")?)?;
                            out.effects.push(QuestEffect::TargetVid { name, vid, title });
                        }
                        ActionName::TargetDelete => {
                            let name = Self::name_text(args.first().ok_or("target_delete sin name")?)?;
                            out.effects.push(QuestEffect::TargetDelete { name });
                        }
                        ActionName::AffectAdd => {
                            let apply = Self::name_text(args.first().ok_or("affect_add sin apply")?)?;
                            let value = self.eval_arg(flags, &q.name, args.get(1).ok_or("affect_add sin value")?, ctx)? as i32;
                            let duration = self.eval_arg(flags, &q.name, args.get(2).ok_or("affect_add sin duration")?, ctx)? as i32;
                            out.effects.push(QuestEffect::AffectAdd { apply, value, duration });
                        }
                        ActionName::AffectRemove => {
                            let apply = args.first().map(|v| Self::name_text(v).unwrap_or_default()).unwrap_or_default();
                            out.effects.push(QuestEffect::AffectRemove { apply });
                        }
                        ActionName::Return => return Ok(None),
                        other => {
                            eprintln!("game_core: quest {}: acción `{other:?}` mapeada-pero-pendiente (ignorada)", q.name);
                        }
                    }
                }
                Stmt::Branch(b) => {
                    let cond_ok = match &b.condition {
                        None => true,
                        Some(c) => match self.eval_expr(flags, &q.name, c, ctx) {
                            Ok(v) => v != 0,
                            Err(e) => return Err(format!("condición de branch: {e}")),
                        },
                    };
                    if cond_ok
                        && let Some(susp) = self.exec_body(
                            rt, flags, q, state, event, &b.body, 0, Some(i), ctx, script, out,
                        )?
                    {
                        return Ok(Some(susp));
                    }
                }
                Stmt::Use { name, .. } => {
                    // Los bloques sin resolver son del loader (futuro) — no
                    // llegan al runtime (los .family.quest los expande quest_dsl).
                    eprintln!("game_core: quest {}: `use {name}` sin resolver (loader pendiente)", q.name);
                }
            }
            i += 1;
        }
        Ok(None)
    }

    /// Evalúa una expresión del catálogo (spec §4) → i64. Los flags se pasan
    /// aparte para que los set_qf del evento se vean en las condiciones
    /// posteriores.
    fn eval_expr(
        &self,
        flags: &HashMap<(String, String), i64>,
        quest: &str,
        e: &Expr,
        ctx: &mut EvalCtx,
    ) -> Result<i64, String> {
        match e {
            Expr::Value(v) => match v {
                Value::Num(n) => Ok(*n),
                Value::Expr(inner) => self.eval_expr(flags, quest, inner, ctx),
                other => Err(format!("valor sin numérico en condición: {other:?}")),
            },
            Expr::Capture(c) => ctx
                .captures
                .get(c)
                .copied()
                .ok_or_else(|| format!("captura `{c}` sin valor (¿select sin respuesta?)")),
            Expr::Between(a, b, c) => {
                let (v, lo, hi) = (
                    self.eval_expr(flags, quest, a, ctx)?,
                    self.eval_expr(flags, quest, b, ctx)?,
                    self.eval_expr(flags, quest, c, ctx)?,
                );
                Ok(i64::from(lo <= v && v <= hi))
            }
            Expr::Compare(a, op, b) => {
                let (x, y) = (self.eval_expr(flags, quest, a, ctx)?, self.eval_expr(flags, quest, b, ctx)?);
                let r = match op {
                    quest_dsl::ast::CmpOp::Eq => x == y,
                    quest_dsl::ast::CmpOp::Ne => x != y,
                    quest_dsl::ast::CmpOp::Lt => x < y,
                    quest_dsl::ast::CmpOp::Gt => x > y,
                    quest_dsl::ast::CmpOp::Le => x <= y,
                    quest_dsl::ast::CmpOp::Ge => x >= y,
                };
                Ok(i64::from(r))
            }
            Expr::Add(a, b) => Ok(self.eval_expr(flags, quest, a, ctx)? + self.eval_expr(flags, quest, b, ctx)?),
            Expr::Sub(a, b) => Ok(self.eval_expr(flags, quest, a, ctx)? - self.eval_expr(flags, quest, b, ctx)?),
            Expr::Mul(a, b) => Ok(self.eval_expr(flags, quest, a, ctx)? * self.eval_expr(flags, quest, b, ctx)?),
            Expr::Div(a, b) => {
                let (x, y) = (self.eval_expr(flags, quest, a, ctx)?, self.eval_expr(flags, quest, b, ctx)?);
                if y == 0 {
                    Err("división por cero".into())
                } else {
                    Ok(x / y)
                }
            }
            Expr::And(a, b) => Ok(i64::from(
                self.eval_expr(flags, quest, a, ctx)? != 0 && self.eval_expr(flags, quest, b, ctx)? != 0,
            )),
            Expr::Or(a, b) => Ok(i64::from(
                self.eval_expr(flags, quest, a, ctx)? != 0 || self.eval_expr(flags, quest, b, ctx)? != 0,
            )),
            Expr::Not(a) => Ok(i64::from(self.eval_expr(flags, quest, a, ctx)? == 0)),
            Expr::Func(f, args) => self.eval_func(flags, quest, f, args, ctx),
        }
    }

    fn eval_func(
        &self,
        flags: &HashMap<(String, String), i64>,
        quest: &str,
        f: &FuncName,
        args: &[Expr],
        ctx: &mut EvalCtx,
    ) -> Result<i64, String> {
        let arg = |i: usize| -> Result<&Expr, String> {
            args.get(i).ok_or_else(|| format!("función sin argumento {i}"))
        };
        match f {
            FuncName::PcLevel => Ok(i64::from(ctx.level)),
            FuncName::CountItem => {
                let v = self.eval_expr(flags, quest, arg(0)?, ctx)?;
                let vnum = u32::try_from(v).map_err(|_| format!("count_item vnum inválido: {v}"))?;
                Ok(*ctx.items.get(&vnum).unwrap_or(&0))
            }
            FuncName::GetQf => {
                let name = Self::arg_name(arg(0)?)?;
                Ok(*flags.get(&(quest.to_string(), name)).unwrap_or(&0))
            }
            FuncName::Number => {
                let (lo, hi) = (self.eval_expr(flags, quest, arg(0)?, ctx)?, self.eval_expr(flags, quest, arg(1)?, ctx)?);
                Ok((ctx.rng)(lo, hi))
            }
            FuncName::GetTime => Ok(ctx.now_s),
            FuncName::GetMapIndex => Ok(i64::from(ctx.map_index)),
            FuncName::GetGmLevel => Ok(0),  // subset sin GM (documentado)
            FuncName::PetIsSummon => Ok(0), // subset sin mascotas (documentado)
            FuncName::IsTestServer => Ok(0),
        }
    }

    /// Evalúa un argumento numérico (Num / Param-expandido / Expr).
    fn eval_arg(
        &self,
        flags: &HashMap<(String, String), i64>,
        quest: &str,
        v: &Value,
        ctx: &mut EvalCtx,
    ) -> Result<i64, String> {
        match v {
            Value::Num(n) => Ok(*n),
            Value::Expr(e) => self.eval_expr(flags, quest, e, ctx),
            other => Err(format!("argumento numérico inválido: {other:?}")),
        }
    }

    /// Clave de diálogo/locale (Str del corpus o `@key` del DSL) — RESUELTA
    /// contra el diccionario de textos (ADR-0009): si la clave está en
    /// `texts`, se envía el TEXTO; si no, la clave tal cual (fallback — el
    /// cliente la resolvería de su pack).
    fn key_text(&self, v: &Value) -> Result<String, String> {
        let key = match v {
            Value::Str(s) | Value::Key(s) => s.clone(),
            Value::Num(n) => n.to_string(),
            other => return Err(format!("clave de diálogo inválida: {other:?}")),
        };
        Ok(self.texts.get(&key).cloned().unwrap_or(key))
    }

    /// Nombre (state, flag) — Str/Key del corpus.
    fn name_text(v: &Value) -> Result<String, String> {
        match v {
            Value::Str(s) | Value::Key(s) => Ok(s.clone()),
            other => Err(format!("nombre inválido: {other:?}")),
        }
    }

    /// El nombre del flag en `get_qf(...)` — Str del convertidor o Capture
    /// del DSL a mano (`get_qf(duration)` — spec §4).
    fn arg_name(e: &Expr) -> Result<String, String> {
        match e {
            Expr::Value(Value::Str(s)) | Expr::Value(Value::Key(s)) => Ok(s.clone()),
            Expr::Capture(c) => Ok(c.clone()),
            other => Err(format!("nombre de flag inválido: {other:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng_fixed(v: i64) -> Box<dyn FnMut(i64, i64) -> i64> {
        Box::new(move |_, _| v)
    }

    fn ctx_bits(level: i32, now_s: i64, items: &HashMap<u32, i64>) -> (i32, u32, i64, &HashMap<u32, i64>) {
        (level, 41, now_s, items)
    }

    /// El DSL corpus-style (convertido) — la quest de recolección con su
    /// ciclo completo: login → information → kill → chat → go_to_disciple.
    const COLLECT: &str = "\
quest collect_quest_lv30
  state start
    on login, levelup with pc.level >= 30
      -> set_state(information)
  state information
    on letter
      -> send_letter(gameforge.collect_quest_lv30._10_sendLetter)
    on 601.kill with number(1, 100) <= 5
      -> give_item2(30006, 1)
    on 20084.chat with count_item(30006) > 0
      -> remove_item(30006, 1)
      -> set_qf(duration, get_time() + 60 * 60 * 22)
      -> set_state(go_to_disciple)
  state go_to_disciple
    on letter with get_qf(done) == 0
      -> say_title(gameforge.collect_quest_lv30._50_sayTitle)
      -> say(gameforge.collect_quest_lv30._40_say)
      -> wait()
      -> set_qf(done, 1)
";

    fn load_collect() -> QuestEngine {
        QuestEngine::load(COLLECT).expect("parse collect")
    }

    #[test]
    fn loads_dsl_and_expands_families() {
        let text = "\
quest fam family (level, mob, herb)
  state start
    on login with pc.level >= (level)
      -> set_state(information)
  state information
    on (mob).kill
      -> give_item2((herb), 1)
quest collect_lv30 = fam(level: 30, mob: 601, herb: 30006)
quest collect_lv40 = fam(level: 40, mob: 602, herb: 30007)
";
        let e = QuestEngine::load(text).expect("parse familia");
        assert_eq!(e.quests().len(), 2);
        assert_eq!(e.quests()[0].name, "collect_lv30");
        assert_eq!(e.quests()[1].name, "collect_lv40");
        // El trigger (mob).kill se expandió a 601.kill.
        let ev = &e.quests()[0].states[1].events[0];
        assert!(QuestTrigger::Kill(601).matches(&ev.triggers[0].kind));
        assert!(!QuestTrigger::Kill(602).matches(&ev.triggers[0].kind));
    }

    #[test]
    fn trigger_matching_catalog() {
        let t = TriggerKind::Chat { target: TriggerTarget::Num(20084) };
        assert!(QuestTrigger::Chat(20084).matches(&t));
        assert!(!QuestTrigger::Chat(20085).matches(&t));
        assert!(!QuestTrigger::Kill(20084).matches(&t));
        assert!(QuestTrigger::Login.matches(&TriggerKind::Login));
        assert!(QuestTrigger::LevelUp.matches(&TriggerKind::LevelUp));
        assert!(QuestTrigger::Letter.matches(&TriggerKind::Letter));
        assert!(QuestTrigger::Timer.matches(&TriggerKind::Timer));
        // Un Param sin expandir nunca matchea (invariante del runtime).
        let p = TriggerKind::Kill { target: TriggerTarget::Param("mob".into()) };
        assert!(!QuestTrigger::Kill(601).matches(&p));
    }

    #[test]
    fn login_starts_quest_and_transitions() {
        let e = load_collect();
        let mut rt = QuestRuntime::default();
        let items = HashMap::new();
        let (level, map, now, items_ref) = ctx_bits(30, 1_000, &items);
        let out = e.run(&mut rt, QuestTrigger::Login, level, map, now, items_ref, &mut *rng_fixed(5));
        // Arranque + set_state(information): dos filas sucias del estado.
        assert_eq!(out.effects, vec![]);
        assert_eq!(out.script, None);
        assert_eq!(rt.states.get("collect_quest_lv30"), Some(&2), "{:?}", rt.states);
        let status: Vec<&DirtyFlag> = out.dirty.iter().filter(|d| d.flag.ends_with(".__status")).collect();
        assert_eq!(status.len(), 2, "{:?}", out.dirty);
        assert_eq!(status[0].value, 1);
        assert_eq!(status[1].value, 2);
        // Nivel bajo: la condición falla y la quest no arranca.
        let mut rt2 = QuestRuntime::default();
        let out2 = e.run(&mut rt2, QuestTrigger::Login, 29, map, now, items_ref, &mut *rng_fixed(5));
        assert!(out2.dirty.is_empty() && out2.effects.is_empty(), "{out2:?}");
        assert!(rt2.states.is_empty());
    }

    #[test]
    fn kill_with_condition_rewards_item() {
        let e = load_collect();
        let mut rt = QuestRuntime::default();
        let items = HashMap::new();
        let (_, map, now, items_ref) = ctx_bits(30, 1_000, &items);
        // La quest arranca con el login (nivel 30).
        e.run(&mut rt, QuestTrigger::Login, 30, map, now, items_ref, &mut *rng_fixed(5));
        // number(1,100) <= 5 con rng 5 → recompensa.
        let out = e.run(&mut rt, QuestTrigger::Kill(601), 30, map, now, items_ref, &mut *rng_fixed(5));
        assert_eq!(out.effects, vec![QuestEffect::GiveItem { vnum: 30006, count: 1 }], "{out:?}");
        // rng 6 → la condición falla → sin recompensa.
        let out = e.run(&mut rt, QuestTrigger::Kill(601), 30, map, now, items_ref, &mut *rng_fixed(6));
        assert!(out.effects.is_empty(), "{out:?}");
        // El kill de otro mob no matchea.
        let out = e.run(&mut rt, QuestTrigger::Kill(602), 30, map, now, items_ref, &mut *rng_fixed(5));
        assert!(out.effects.is_empty(), "{out:?}");
    }

    #[test]
    fn chat_uses_count_item_and_flags() {
        let e = load_collect();
        let mut rt = QuestRuntime::default();
        let items = HashMap::from([(30006u32, 1i64)]);
        let (_, map, now, items_ref) = ctx_bits(30, 1_000, &items);
        e.run(&mut rt, QuestTrigger::Login, 30, map, now, items_ref, &mut *rng_fixed(5));
        // Sin el item en el inventario: la condición count_item falla.
        let empty = HashMap::new();
        let out = e.run(&mut rt, QuestTrigger::Chat(20084), 30, map, now, &empty, &mut *rng_fixed(5));
        assert!(out.effects.is_empty(), "{out:?}");
        // Con el item: remove + set_qf(get_time()+...)+ set_state.
        let out = e.run(&mut rt, QuestTrigger::Chat(20084), 30, map, now, items_ref, &mut *rng_fixed(5));
        assert_eq!(
            out.effects,
            vec![QuestEffect::RemoveItem { vnum: 30006, count: 1 }],
            "{out:?}"
        );
        assert_eq!(rt.states.get("collect_quest_lv30"), Some(&3));
        let dur = rt.flags.get(&("collect_quest_lv30".into(), "duration".into())).copied().unwrap_or(0);
        assert_eq!(dur, now + 60 * 60 * 22, "get_time() + 60*60*22");
        assert!(out.dirty.iter().any(|d| d.flag == "duration" && d.value == dur), "{:?}", out.dirty);
    }

    #[test]
    fn wait_suspends_and_answer_resumes() {
        let e = load_collect();
        let mut rt = QuestRuntime::default();
        // El evento de chat exige count_item(30006) > 0 — el jugador tiene el item.
        let items = HashMap::from([(30006u32, 1i64)]);
        let (_, map, now, items_ref) = ctx_bits(30, 1_000, &items);
        e.run(&mut rt, QuestTrigger::Login, 30, map, now, items_ref, &mut *rng_fixed(5));
        e.run(&mut rt, QuestTrigger::Chat(20084), 30, map, now, items_ref, &mut *rng_fixed(5));
        // go_to_disciple: letter → say_title + say + wait → suspensión.
        let out = e.run(&mut rt, QuestTrigger::Letter, 30, map, now, items_ref, &mut *rng_fixed(5));
        let script = out.script.expect("diálogo");
        assert!(script.contains("gameforge.collect_quest_lv30._50_sayTitle[ENTER]"), "{script}");
        assert!(script.contains("gameforge.collect_quest_lv30._40_say[ENTER]"), "{script}");
        assert!(script.ends_with("[NEXT]"), "{script}");
        assert!(out.suspended);
        assert!(rt.suspended.is_some());
        // Mientras está suspendida, ningún otro evento corre (parity IsRunning).
        let out = e.run(&mut rt, QuestTrigger::Kill(601), 30, map, now, items_ref, &mut *rng_fixed(5));
        assert!(out.effects.is_empty() && !out.suspended, "{out:?}");
        // Reanudación (answer 0 del [NEXT]): set_qf(done, 1).
        let out = e.answer(&mut rt, 0, 30, map, now, items_ref, &mut *rng_fixed(5));
        assert!(!out.suspended);
        assert!(rt.suspended.is_none());
        assert_eq!(rt.flags.get(&("collect_quest_lv30".into(), "done".into())), Some(&1));
        // La quest terminó: el evento letter ya no vuelve a disparar.
        let out = e.run(&mut rt, QuestTrigger::Letter, 30, map, now, items_ref, &mut *rng_fixed(5));
        assert!(out.script.is_none(), "{out:?}");
    }

    #[test]
    fn select_binds_capture_and_branches() {
        let e = QuestEngine::load(
            "quest branch\n  state start\n    on 20011.chat\n      -> select(@_20_say, @_30_say) as choice\n      if choice == 1\n        -> warp(896500, 24600)\n      else\n        -> return\n",
        )
        .expect("parse");
        let mut rt = QuestRuntime::default();
        let items = HashMap::new();
        let (_, map, now, items_ref) = ctx_bits(5, 0, &items);
        let out = e.run(&mut rt, QuestTrigger::Chat(20011), 5, map, now, items_ref, &mut *rng_fixed(0));
        let script = out.script.expect("diálogo");
        assert_eq!(script, "[QUESTION 1;_20_say|2;_30_say]", "{script}");
        assert!(out.suspended);
        // answer 1 → la rama warp.
        let out = e.answer(&mut rt, 1, 5, map, now, items_ref, &mut *rng_fixed(0));
        assert_eq!(out.effects, vec![QuestEffect::Warp { x: 896500, y: 24600 }], "{out:?}");
        // answer 2 → la rama return (sin efectos).
        let out = e.run(&mut rt, QuestTrigger::Chat(20011), 5, map, now, items_ref, &mut *rng_fixed(0));
        assert!(out.suspended, "{out:?}");
        let out = e.answer(&mut rt, 2, 5, map, now, items_ref, &mut *rng_fixed(0));
        assert!(out.effects.is_empty(), "{out:?}");
    }

    #[test]
    fn set_qf_cooldown_gates_use_event() {
        // El patrón del corpus: `with get_qf(duration) == 0` + set_qf del
        // deadline; el reset (999.chat) vuelve a permitir el uso.
        let e = QuestEngine::load(
            "quest drug\n  state start\n    on 71035.use with get_qf(duration) == 0\n      -> remove_item(71035, 1)\n      -> set_qf(duration, get_time() + 3600)\n    on 999.chat\n      -> set_qf(duration, 0)\n",
        )
        .expect("parse");
        let mut rt = QuestRuntime::default();
        let items = HashMap::new();
        let (_, map, now, items_ref) = ctx_bits(5, 100, &items);
        // Primer uso: remove + cooldown.
        let out = e.run(&mut rt, QuestTrigger::Use(71035), 5, map, now, items_ref, &mut *rng_fixed(0));
        assert_eq!(out.effects, vec![QuestEffect::RemoveItem { vnum: 71035, count: 1 }]);
        assert_eq!(rt.flags.get(&("drug".into(), "duration".into())), Some(&(100 + 3600)));
        // Segundo uso (mismo tiempo): el cooldown bloquea.
        let out = e.run(&mut rt, QuestTrigger::Use(71035), 5, map, now, items_ref, &mut *rng_fixed(0));
        assert!(out.effects.is_empty(), "{out:?}");
        // El reset del evento 999.chat vuelve a permitir el uso.
        e.run(&mut rt, QuestTrigger::Chat(999), 5, map, now, items_ref, &mut *rng_fixed(0));
        let out = e.run(&mut rt, QuestTrigger::Use(71035), 5, map, now, items_ref, &mut *rng_fixed(0));
        assert_eq!(out.effects, vec![QuestEffect::RemoveItem { vnum: 71035, count: 1 }]);
    }

    #[test]
    fn state_machine_gates_events_by_current_state() {
        let e = QuestEngine::load(
            "quest two_states\n  state start\n    on 601.kill\n      -> set_state(fighting)\n  state fighting\n    on 601.kill\n      -> give_item2(30006, 1)\n",
        )
        .expect("parse");
        let mut rt = QuestRuntime::default();
        let items = HashMap::new();
        let (_, map, now, items_ref) = ctx_bits(5, 0, &items);
        // Primer kill: la quest arranca (state start) y transiciona — SIN
        // recompensa (el evento del state fighting no corre en este evento).
        let out = e.run(&mut rt, QuestTrigger::Kill(601), 5, map, now, items_ref, &mut *rng_fixed(0));
        assert!(out.effects.is_empty(), "{out:?}");
        assert_eq!(rt.states.get("two_states"), Some(&2));
        // Segundo kill: ya en fighting → recompensa.
        let out = e.run(&mut rt, QuestTrigger::Kill(601), 5, map, now, items_ref, &mut *rng_fixed(0));
        assert_eq!(out.effects, vec![QuestEffect::GiveItem { vnum: 30006, count: 1 }]);
    }

    #[test]
    fn runtime_loads_persisted_flags() {
        let rows = vec![
            PersistedFlag { quest: "q".into(), flag: "q.__status".into(), value: 2 },
            PersistedFlag { quest: "q".into(), flag: "duration".into(), value: 42 },
            PersistedFlag { quest: "q".into(), flag: "q.__status".into(), value: 3 }, // overwrite
        ];
        let rt = QuestRuntime::load(&rows);
        assert_eq!(rt.states.get("q"), Some(&3));
        assert_eq!(rt.flags.get(&("q".into(), "duration".into())), Some(&42));
    }

    #[test]
    fn answer_without_suspension_is_noop() {
        let e = load_collect();
        let mut rt = QuestRuntime::default();
        let items = HashMap::new();
        let (_, map, now, items_ref) = ctx_bits(30, 1_000, &items);
        let out = e.answer(&mut rt, 1, 30, map, now, items_ref, &mut *rng_fixed(5));
        assert!(out.script.is_none() && out.effects.is_empty(), "{out:?}");
    }

    #[test]
    fn say_reward_appends_dialog_like_say() {
        let e = QuestEngine::load(
            "quest r\n  state start\n    on letter\n      -> say_reward(@reward_key)\n",
        )
        .expect("parse");
        let mut rt = QuestRuntime::default();
        let items = HashMap::new();
        let (_, map, now, items_ref) = ctx_bits(5, 0, &items);
        let out = e.run(&mut rt, QuestTrigger::Letter, 5, map, now, items_ref, &mut *rng_fixed(0));
        assert_eq!(out.script.as_deref(), Some("reward_key[ENTER]"), "{out:?}");
    }

    #[test]
    fn send_letter_emits_effect_with_resolved_key() {
        let mut texts = HashMap::new();
        texts.insert("my_letter".into(), "Carta!".into());
        let e = QuestEngine::load(
            "quest r\n  state start\n    on letter\n      -> send_letter(@my_letter)\n",
        )
        .expect("parse")
        .with_texts(texts);
        let mut rt = QuestRuntime::default();
        let items = HashMap::new();
        let (_, map, now, items_ref) = ctx_bits(5, 0, &items);
        let out = e.run(&mut rt, QuestTrigger::Letter, 5, map, now, items_ref, &mut *rng_fixed(0));
        assert_eq!(out.effects, vec![QuestEffect::SendLetter("Carta!".into())]);
    }

    #[test]
    fn set_quest_state_cross_quest() {
        // Dos quests: `other` en start, `main` la cruza a `other.information`.
        let e = QuestEngine::load(
            "quest other\n  state start\n    on letter\n      -> say(@a)\n  state information\n    on letter\n      -> say(@b)\nquest main\n  state start\n    on letter\n      -> set_quest_state(other, information)\n",
        )
        .expect("parse");
        let mut rt = QuestRuntime::default();
        let items = HashMap::new();
        let (_, map, now, items_ref) = ctx_bits(5, 0, &items);
        let out = e.run(&mut rt, QuestTrigger::Letter, 5, map, now, items_ref, &mut *rng_fixed(0));
        assert_eq!(rt.states.get("other"), Some(&2), "{:?}", rt.states);
        assert!(out.dirty.iter().any(|d| d.quest == "other" && d.flag == "other.__status" && d.value == 2), "{:?}", out.dirty);
        // La quest cruzada ya puede disparar su evento del nuevo state.
        let out2 = e.run(&mut rt, QuestTrigger::Letter, 5, map, now, items_ref, &mut *rng_fixed(0));
        assert!(out2.script.as_deref().unwrap_or("").contains("b[ENTER]"), "{out2:?}");
    }
    #[test]
    fn target_vid_and_affect_verifier() {
        let mut texts = HashMap::new();
        texts.insert("title".into(), "Objetivo!".into());
        let e = QuestEngine::load("quest q\n  state start\n    on letter\n      -> target_vid(__TARGET__, 20084, @title)\n      -> affect_add(apply.MOV_SPEED, 10, 60*60)\n      -> affect_remove(apply.MOV_SPEED)\n      -> target_delete(__TARGET__)\n").expect("parse").with_texts(texts);
        let mut rt = QuestRuntime::default();
        let items = HashMap::new();
        let (_, map, now, items_ref) = ctx_bits(5, 0, &items);
        let out = e.run(&mut rt, QuestTrigger::Letter, 5, map, now, items_ref, &mut *rng_fixed(0));
        assert_eq!(out.effects[0], QuestEffect::TargetVid { name: "__TARGET__".into(), vid: 20084, title: "Objetivo!".into() });
        assert_eq!(out.effects[1], QuestEffect::AffectAdd { apply: "apply.MOV_SPEED".into(), value: 10, duration: 3600 });
        assert_eq!(out.effects[2], QuestEffect::AffectRemove { apply: "apply.MOV_SPEED".into() });
        assert_eq!(out.effects[3], QuestEffect::TargetDelete { name: "__TARGET__".into() });
        assert_eq!(out.script, None);
        // mutation: without handler effects would be empty -> fails
        assert!(out.effects.len() == 4, "verifier: 4 effects");
    }
}
