//! Family expansion (spec §6): `quest X = base(param: value, ...)` → concrete
//! quests. `(name)` parameter references in conditions/actions and `@key`
//! locale keys (per-family key index) are substituted at load time.
//!
//! Rule (spec §6): a parameter is referenced as `(name)` WITHOUT spaces; the
//! converter generates the concrete text keys per instance (the family carries
//! its own key index — here the `@key` stays as-is when the instance does not
//! provide a `key_*` argument; the converter's key-table generation is a
//! future slice).

use crate::ast::*;
use std::collections::BTreeMap;

/// Expands all `Quest::Instance`s against their `Quest::Family` bases.
/// Returns the concrete quests in file order (families themselves are not
/// runtime quests; instances are).
pub fn expand_families(file: &QuestFile) -> Result<Vec<QuestDef>, String> {
    let families: std::collections::HashMap<&str, &Quest> = file
        .quests
        .iter()
        .filter_map(|q| match q {
            Quest::Family { name, .. } => Some((name.as_str(), q)),
            _ => None,
        })
        .collect();

    let mut out = Vec::new();
    for q in &file.quests {
        if let Quest::Instance(inst) = q {
            let family = families
                .get(inst.base.as_str())
                .ok_or_else(|| format!("familia no encontrada: {} (para {})", inst.base, inst.name))?;
            let Quest::Family { params, states, .. } = family else {
                unreachable!("families map solo contiene familias");
            };
            // Build the param → value map, validating types.
            let mut map = std::collections::HashMap::new();
            for p in params {
                let value = inst
                    .args
                    .iter()
                    .find(|(n, _)| n == &p.name)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| {
                        format!("instancia {} sin argumento `{}` (familia {})", inst.name, p.name, inst.base)
                    })?;
                if let Some(ty) = &p.ty {
                    validate_type(ty, &value, &inst.name, &p.name)?;
                }
                map.insert(p.name.clone(), value);
            }
            let states = states.iter().map(|s| substitute_state(s, &map)).collect();
            out.push(QuestDef { name: inst.name.clone(), states });
        }
    }
    Ok(out)
}

fn validate_type(ty: &ParamType, v: &Value, quest: &str, param: &str) -> Result<(), String> {
    let ok = match (ty, v) {
        (ParamType::Vnum, Value::Num(_)) => true,
        (ParamType::Level, Value::Num(_)) => true,
        (ParamType::Key, Value::Key(_)) | (ParamType::Key, Value::Str(_)) => true,
        (ParamType::Str, Value::Str(_)) => true,
        (ParamType::Str, Value::Key(_)) => true, // @key es un string de locale
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(format!("quest {quest}: el arg `{param}` ({v:?}) no coincide con el tipo {ty:?}"))
    }
}

fn substitute_state(s: &State, map: &std::collections::HashMap<String, Value>) -> State {
    State {
        name: s.name.clone(),
        events: s
            .events
            .iter()
            .map(|e| Event {
                triggers: e.triggers.iter().map(|t| subst_trigger(t, map)).collect(),
                condition: e.condition.as_ref().map(|c| subst_expr(c, map)),
                body: e.body.iter().map(|st| subst_stmt(st, map)).collect(),
            })
            .collect(),
    }
}

fn subst_trigger(t: &Trigger, map: &std::collections::HashMap<String, Value>) -> Trigger {
    let kind = match &t.kind {
        TriggerKind::Chat { target } => TriggerKind::Chat { target: subst_target(target, map) },
        TriggerKind::Kill { target } => TriggerKind::Kill { target: subst_target(target, map) },
        TriggerKind::Use { target } => TriggerKind::Use { target: subst_target(target, map) },
        other => other.clone(),
    };
    Trigger { kind }
}

fn subst_target(t: &TriggerTarget, map: &std::collections::HashMap<String, Value>) -> TriggerTarget {
    match t {
        TriggerTarget::Param(name) => match map.get(name) {
            Some(Value::Num(n)) if *n >= 0 && *n <= u32::MAX as i64 => {
                TriggerTarget::Num(*n as u32)
            }
            Some(other) => TriggerTarget::Param(format!("{other:?}")),
            None => TriggerTarget::Param(name.clone()),
        },
        other => other.clone(),
    }
}

fn subst_stmt(st: &Stmt, map: &std::collections::HashMap<String, Value>) -> Stmt {
    match st {
        Stmt::Action { action, capture } => Stmt::Action {
            action: Action {
                name: action.name.clone(),
                args: action.args.iter().map(|a| subst_value(a, map)).collect(),
            },
            capture: capture.clone(),
        },
        Stmt::Branch(b) => Stmt::Branch(Branch {
            condition: b.condition.as_ref().map(|c| subst_expr(c, map)),
            body: b.body.iter().map(|st| subst_stmt(st, map)).collect(),
        }),
        Stmt::Use { name, args } => Stmt::Use {
            name: name.clone(),
            args: args.iter().map(|a| subst_value(a, map)).collect(),
        },
    }
}

fn subst_expr(e: &Expr, map: &std::collections::HashMap<String, Value>) -> Expr {
    match e {
        Expr::Between(a, b, c) => {
            Expr::Between(Box::new(subst_expr(a, map)), Box::new(subst_expr(b, map)), Box::new(subst_expr(c, map)))
        }
        Expr::Compare(a, op, b) => Expr::Compare(Box::new(subst_expr(a, map)), *op, Box::new(subst_expr(b, map))),
        Expr::Add(a, b) => Expr::Add(Box::new(subst_expr(a, map)), Box::new(subst_expr(b, map))),
        Expr::Sub(a, b) => Expr::Sub(Box::new(subst_expr(a, map)), Box::new(subst_expr(b, map))),
        Expr::Mul(a, b) => Expr::Mul(Box::new(subst_expr(a, map)), Box::new(subst_expr(b, map))),
        Expr::Div(a, b) => Expr::Div(Box::new(subst_expr(a, map)), Box::new(subst_expr(b, map))),
        Expr::And(a, b) => Expr::And(Box::new(subst_expr(a, map)), Box::new(subst_expr(b, map))),
        Expr::Or(a, b) => Expr::Or(Box::new(subst_expr(a, map)), Box::new(subst_expr(b, map))),
        Expr::Not(a) => Expr::Not(Box::new(subst_expr(a, map))),
        Expr::Value(v) => Expr::Value(subst_value(v, map)),
        Expr::Capture(c) => Expr::Capture(c.clone()),
        Expr::Func(f, args) => Expr::Func(f.clone(), args.iter().map(|a| subst_expr(a, map)).collect()),
    }
}

fn subst_value(v: &Value, map: &std::collections::HashMap<String, Value>) -> Value {
    match v {
        Value::Param(name) => map.get(name).cloned().unwrap_or_else(|| v.clone()),
        Value::Expr(e) => Value::Expr(Box::new(subst_expr(e, map))),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// Family extraction (spec §6 reverse: quests → family template + instances)
// ---------------------------------------------------------------------------

/// Result of `extract_family_params`: the family template + one instance per
/// usable member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyExtraction {
    /// The family quest template — states with `(param)` references.
    pub family: Quest,
    /// One instance per usable member: `quest X = base(param: value, ...)`.
    pub instances: Vec<InstanceDef>,
    /// Members excluded from the family (name, reason).
    pub excluded: Vec<(String, String)>,
}

/// Extracts a parameterized family from a group of structurally-equal quests
/// (spec §6): walks the members in lockstep, diffs the literal values slot by
/// slot; slots whose values vary across members become family parameters
/// (slots with the same value pattern share ONE parameter); the first usable
/// member becomes the template with `(param)` references; every usable member
/// becomes an instance `quest X = base(param: value, ...)`.
///
/// `suffix_param` is the name suggested by the name heuristic (`level` for
/// `collect_quest_lv30`, `index` for `subquest_01`): the first varying slot
/// whose values equal the numeric name suffixes takes that name (placed FIRST
/// in the parameter list); the rest are `p1`, `p2`, ... in slot order. The
/// names are placeholders — human confirmation / the similarity engine renames
/// them (spec §9.3). Members with a different structure (different slot path
/// set) are excluded, not failed.
pub fn extract_family_params(
    family_name: &str,
    suffix_param: &str,
    members: &[QuestDef],
) -> Result<FamilyExtraction, String> {
    if members.len() < 2 {
        return Err("se necesitan al menos 2 miembros para extraer una familia".into());
    }
    // Lockstep value collection: one (path → value) map per member.
    let slots: Vec<BTreeMap<String, Value>> = members.iter().map(collect_slots).collect();
    let base_paths: Vec<&String> = slots[0].keys().collect();

    // Structural equality = identical slot path sets.
    let mut usable: Vec<usize> = Vec::new();
    let mut excluded = Vec::new();
    for (i, m) in slots.iter().enumerate() {
        let paths: Vec<&String> = m.keys().collect();
        if paths == base_paths {
            usable.push(i);
        } else {
            excluded.push((members[i].name.clone(), "estructura distinta al resto del grupo".into()));
        }
    }
    if usable.len() < 2 {
        return Err("menos de 2 miembros con estructura idéntica — no hay familia".into());
    }

    // Numeric name suffixes (`collect_quest_lv30` → 30): the slot matching
    // them becomes `suffix_param`.
    let suffix_vals: Option<Vec<i64>> = usable
        .iter()
        .map(|&i| name_suffix(family_name, &members[i].name))
        .collect();

    // Varying slots → parameters (pattern = per-member values).
    let mut params: Vec<(String, Vec<Value>)> = Vec::new(); // (name, pattern)
    let mut slot_param: BTreeMap<String, String> = BTreeMap::new(); // path → param name
    for path in &base_paths {
        let vals: Vec<Value> = usable.iter().map(|&i| slots[i][*path].clone()).collect();
        if vals.windows(2).all(|w| w[0] == w[1]) {
            continue; // constant across the group — stays literal in the template
        }
        // Same value pattern → same parameter.
        let name = match params.iter().find(|(_, p)| p == &vals) {
            Some((n, _)) => n.clone(),
            None => {
                let is_suffix = suffix_vals.as_ref().is_some_and(|sv| {
                    vals.iter().enumerate().all(|(i, v)| matches!(v, Value::Num(n) if *n == sv[i]))
                });
                let n = if is_suffix { suffix_param.to_string() } else { format!("p{}", params.len() + 1) };
                if is_suffix {
                    params.insert(0, (n.clone(), vals));
                } else {
                    params.push((n.clone(), vals));
                }
                n
            }
        };
        slot_param.insert((*path).clone(), name);
    }

    // Template from the first usable member + instances for every usable one.
    let slot_repl: BTreeMap<String, Value> = slot_param
        .iter()
        .map(|(path, name)| (path.clone(), Value::Param(name.clone())))
        .collect();
    let template = rewrite_slots(&members[usable[0]], &slot_repl);
    let param_defs: Vec<Param> = params.iter().map(|(n, _)| Param { name: n.clone(), ty: None }).collect();
    let instances: Vec<InstanceDef> = usable
        .iter()
        .map(|&i| {
            let args = params
                .iter()
                .map(|(n, _)| {
                    let path = slot_param
                        .iter()
                        .find(|(_, pn)| *pn == n)
                        .map(|(p, _)| p.clone())
                        .expect("param name presente en slot_param");
                    (n.clone(), slots[i][&path].clone())
                })
                .collect();
            InstanceDef { name: members[i].name.clone(), base: family_name.to_string(), args }
        })
        .collect();

    Ok(FamilyExtraction {
        family: Quest::Family { name: family_name.to_string(), params: param_defs, states: template.states },
        instances,
        excluded,
    })
}

/// Numeric name suffix of a member: `collect_quest_lv30` → 30, `subquest_01` → 1.
fn name_suffix(family: &str, name: &str) -> Option<i64> {
    let rest = name.strip_prefix(family)?;
    rest.strip_prefix("_lv").or_else(|| rest.strip_prefix('_'))?.parse::<i64>().ok()
}

/// Slot paths of a quest: a deterministic path per literal VALUE position
/// (`state:<name>/event:<i>/cond`, `.../t:<i>/target`, `.../b:<i>/a:<j>`,
/// `.../b:<i>/cond`, `.../b:<i>/b:<j>`) PLUS structural markers
/// (`__st`/`__ev`/`__kind`/`__act`/...). Structural equality across members
/// = equal path sets — quests with different states/events/triggers/actions
/// but no literals (e.g. `wait()` bodies) must NOT compare equal.
fn collect_slots(q: &QuestDef) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for st in &q.states {
        let sp = format!("state:{}/", st.name);
        out.insert(format!("{sp}__st"), Value::Str("state".into()));
        for (ei, e) in st.events.iter().enumerate() {
            let ep = format!("{sp}event:{ei}/");
            out.insert(format!("{ep}__ev"), Value::Str("event".into()));
            match &e.condition {
                Some(c) => collect_expr(c, &format!("{ep}cond"), &mut out),
                None => {
                    out.insert(format!("{ep}__nocond"), Value::Str("nocond".into()));
                }
            }
            for (ti, t) in e.triggers.iter().enumerate() {
                let tp = format!("{ep}t:{ti}/");
                out.insert(format!("{tp}__kind"), Value::Str(trigger_kind_name(&t.kind).into()));
                match &t.kind {
                    TriggerKind::Chat { target }
                    | TriggerKind::Kill { target }
                    | TriggerKind::Use { target } => {
                        if let TriggerTarget::Num(n) = target {
                            out.insert(format!("{tp}target"), Value::Num(*n as i64));
                        }
                    }
                    _ => {}
                }
            }
            for (bi, s) in e.body.iter().enumerate() {
                collect_stmt(s, &format!("{ep}b:{bi}"), &mut out);
            }
        }
    }
    out
}

fn trigger_kind_name(k: &TriggerKind) -> &'static str {
    match k {
        TriggerKind::Login => "login",
        TriggerKind::LevelUp => "levelup",
        TriggerKind::Letter => "letter",
        TriggerKind::Button => "button",
        TriggerKind::Info => "info",
        TriggerKind::Enter => "enter",
        TriggerKind::Logout => "logout",
        TriggerKind::Timer => "timer",
        TriggerKind::Chat { .. } => "chat",
        TriggerKind::Kill { .. } => "kill",
        TriggerKind::Use { .. } => "use",
        TriggerKind::TargetClick => "target.click",
        TriggerKind::Rust(_) => "rust",
    }
}

fn collect_expr(e: &Expr, path: &str, out: &mut BTreeMap<String, Value>) {
    match e {
        Expr::Value(v) => {
            out.insert(path.to_string(), v.clone());
        }
        Expr::Between(a, b, c) => {
            out.insert(format!("{path}/__between"), Value::Str("between".into()));
            collect_expr(a, &format!("{path}/v"), out);
            collect_expr(b, &format!("{path}/lo"), out);
            collect_expr(c, &format!("{path}/hi"), out);
        }
        Expr::Compare(a, op, b) => {
            out.insert(format!("{path}/__cmp"), Value::Str(cmp_op_name(*op).into()));
            collect_expr(a, &format!("{path}/l"), out);
            collect_expr(b, &format!("{path}/r"), out);
        }
        Expr::Add(a, b) => {
            out.insert(format!("{path}/__op"), Value::Str("add".into()));
            collect_expr(a, &format!("{path}/l"), out);
            collect_expr(b, &format!("{path}/r"), out);
        }
        Expr::Sub(a, b) => {
            out.insert(format!("{path}/__op"), Value::Str("sub".into()));
            collect_expr(a, &format!("{path}/l"), out);
            collect_expr(b, &format!("{path}/r"), out);
        }
        Expr::Mul(a, b) => {
            out.insert(format!("{path}/__op"), Value::Str("mul".into()));
            collect_expr(a, &format!("{path}/l"), out);
            collect_expr(b, &format!("{path}/r"), out);
        }
        Expr::Div(a, b) => {
            out.insert(format!("{path}/__op"), Value::Str("div".into()));
            collect_expr(a, &format!("{path}/l"), out);
            collect_expr(b, &format!("{path}/r"), out);
        }
        Expr::And(a, b) => {
            out.insert(format!("{path}/__op"), Value::Str("and".into()));
            collect_expr(a, &format!("{path}/l"), out);
            collect_expr(b, &format!("{path}/r"), out);
        }
        Expr::Or(a, b) => {
            out.insert(format!("{path}/__op"), Value::Str("or".into()));
            collect_expr(a, &format!("{path}/l"), out);
            collect_expr(b, &format!("{path}/r"), out);
        }
        Expr::Not(a) => {
            out.insert(format!("{path}/__not"), Value::Str("not".into()));
            collect_expr(a, &format!("{path}/x"), out);
        }
        Expr::Capture(c) => {
            out.insert(format!("{path}/__cap"), Value::Str(c.clone()));
        }
        Expr::Func(f, args) => {
            out.insert(format!("{path}/__fn"), Value::Str(func_name(f).into()));
            for (i, a) in args.iter().enumerate() {
                collect_expr(a, &format!("{path}/f:{i}"), out);
            }
        }
    }
}

fn cmp_op_name(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Eq => "==",
        CmpOp::Ne => "!=",
        CmpOp::Lt => "<",
        CmpOp::Gt => ">",
        CmpOp::Le => "<=",
        CmpOp::Ge => ">=",
    }
}

fn func_name(f: &FuncName) -> &'static str {
    match f {
        FuncName::PcLevel => "pc.level",
        FuncName::CountItem => "count_item",
        FuncName::GetQf => "get_qf",
        FuncName::Number => "number",
        FuncName::GetTime => "get_time",
        FuncName::GetMapIndex => "get_map_index",
        FuncName::GetGmLevel => "get_gm_level",
        FuncName::PetIsSummon => "pet.is_summon",
        FuncName::IsTestServer => "is_test_server",
    }
}

fn collect_stmt(s: &Stmt, path: &str, out: &mut BTreeMap<String, Value>) {
    match s {
        Stmt::Action { action, .. } => {
            out.insert(format!("{path}/__act"), Value::Str(action_name(&action.name).into()));
            for (i, a) in action.args.iter().enumerate() {
                out.insert(format!("{path}/a:{i}"), a.clone());
            }
        }
        Stmt::Branch(b) => {
            match &b.condition {
                Some(c) => collect_expr(c, &format!("{path}/cond"), out),
                None => {
                    out.insert(format!("{path}/__else"), Value::Str("else".into()));
                }
            }
            for (i, s2) in b.body.iter().enumerate() {
                collect_stmt(s2, &format!("{path}/b:{i}"), out);
            }
        }
        Stmt::Use { name, args } => {
            out.insert(format!("{path}/__use"), Value::Str(name.clone()));
            for (i, a) in args.iter().enumerate() {
                out.insert(format!("{path}/a:{i}"), a.clone());
            }
        }
    }
}

fn action_name(a: &ActionName) -> &'static str {
    match a {
        ActionName::SayTitle => "say_title",
        ActionName::Say => "say",
        ActionName::SayReward => "say_reward",
        ActionName::SayItemVnum => "say_item_vnum",
        ActionName::SendLetter => "send_letter",
        ActionName::ClearLetter => "clear_letter",
        ActionName::Wait => "wait",
        ActionName::SetState => "set_state",
        ActionName::SetQuestState => "set_quest_state",
        ActionName::SetQf => "set_qf",
        ActionName::GiveItem2 => "give_item2",
        ActionName::RemoveItem => "remove_item",
        ActionName::TargetVid => "target_vid",
        ActionName::TargetDelete => "target_delete",
        ActionName::Warp => "warp",
        ActionName::Notice => "notice",
        ActionName::NoticeMultiline => "notice_multiline",
        ActionName::AffectAdd => "affect_add",
        ActionName::AffectRemove => "affect_remove",
        ActionName::Select => "select",
        ActionName::InputNumber => "input_number",
        ActionName::Return => "return",
    }
}

/// Rebuilds a quest replacing the values at `repl`'s paths with the
/// replacement (constants — paths absent from `repl` — keep their value).
fn rewrite_slots(q: &QuestDef, repl: &BTreeMap<String, Value>) -> QuestDef {
    QuestDef {
        name: q.name.clone(),
        states: q
            .states
            .iter()
            .map(|st| {
                let sp = format!("state:{}/", st.name);
                State {
                    name: st.name.clone(),
                    events: st
                        .events
                        .iter()
                        .enumerate()
                        .map(|(ei, e)| {
                            let ep = format!("{sp}event:{ei}/");
                            Event {
                                triggers: e
                                    .triggers
                                    .iter()
                                    .enumerate()
                                    .map(|(ti, t)| rewrite_trigger(t, &format!("{ep}t:{ti}/target"), repl))
                                    .collect(),
                                condition: e
                                    .condition
                                    .as_ref()
                                    .map(|c| rewrite_expr(c, &format!("{ep}cond"), repl)),
                                body: e
                                    .body
                                    .iter()
                                    .enumerate()
                                    .map(|(bi, s)| rewrite_stmt(s, &format!("{ep}b:{bi}"), repl))
                                    .collect(),
                            }
                        })
                        .collect(),
                }
            })
            .collect(),
    }
}

fn rewrite_trigger(t: &Trigger, path: &str, repl: &BTreeMap<String, Value>) -> Trigger {
    let kind = match &t.kind {
        TriggerKind::Chat { target } => TriggerKind::Chat { target: rewrite_target(target, path, repl) },
        TriggerKind::Kill { target } => TriggerKind::Kill { target: rewrite_target(target, path, repl) },
        TriggerKind::Use { target } => TriggerKind::Use { target: rewrite_target(target, path, repl) },
        other => other.clone(),
    };
    Trigger { kind }
}

fn rewrite_target(t: &TriggerTarget, path: &str, repl: &BTreeMap<String, Value>) -> TriggerTarget {
    match t {
        TriggerTarget::Num(n) => match repl.get(path) {
            Some(Value::Param(p)) => TriggerTarget::Param(p.clone()),
            _ => TriggerTarget::Num(*n),
        },
        other => other.clone(),
    }
}

fn rewrite_expr(e: &Expr, path: &str, repl: &BTreeMap<String, Value>) -> Expr {
    match e {
        Expr::Value(v) => Expr::Value(repl.get(path).cloned().unwrap_or_else(|| v.clone())),
        Expr::Between(a, b, c) => Expr::Between(
            Box::new(rewrite_expr(a, &format!("{path}/v"), repl)),
            Box::new(rewrite_expr(b, &format!("{path}/lo"), repl)),
            Box::new(rewrite_expr(c, &format!("{path}/hi"), repl)),
        ),
        Expr::Compare(a, op, b) => {
            Expr::Compare(Box::new(rewrite_expr(a, &format!("{path}/l"), repl)), *op, Box::new(rewrite_expr(b, &format!("{path}/r"), repl)))
        }
        Expr::Add(a, b) => Expr::Add(Box::new(rewrite_expr(a, &format!("{path}/l"), repl)), Box::new(rewrite_expr(b, &format!("{path}/r"), repl))),
        Expr::Sub(a, b) => Expr::Sub(Box::new(rewrite_expr(a, &format!("{path}/l"), repl)), Box::new(rewrite_expr(b, &format!("{path}/r"), repl))),
        Expr::Mul(a, b) => Expr::Mul(Box::new(rewrite_expr(a, &format!("{path}/l"), repl)), Box::new(rewrite_expr(b, &format!("{path}/r"), repl))),
        Expr::Div(a, b) => Expr::Div(Box::new(rewrite_expr(a, &format!("{path}/l"), repl)), Box::new(rewrite_expr(b, &format!("{path}/r"), repl))),
        Expr::And(a, b) => Expr::And(Box::new(rewrite_expr(a, &format!("{path}/l"), repl)), Box::new(rewrite_expr(b, &format!("{path}/r"), repl))),
        Expr::Or(a, b) => Expr::Or(Box::new(rewrite_expr(a, &format!("{path}/l"), repl)), Box::new(rewrite_expr(b, &format!("{path}/r"), repl))),
        Expr::Not(a) => Expr::Not(Box::new(rewrite_expr(a, &format!("{path}/x"), repl))),
        Expr::Capture(c) => Expr::Capture(c.clone()),
        Expr::Func(f, args) => Expr::Func(
            f.clone(),
            args.iter().enumerate().map(|(i, a)| rewrite_expr(a, &format!("{path}/f:{i}"), repl)).collect(),
        ),
    }
}

fn rewrite_stmt(s: &Stmt, path: &str, repl: &BTreeMap<String, Value>) -> Stmt {
    match s {
        Stmt::Action { action, capture } => Stmt::Action {
            action: Action {
                name: action.name.clone(),
                args: action
                    .args
                    .iter()
                    .enumerate()
                    .map(|(i, a)| repl.get(&format!("{path}/a:{i}")).cloned().unwrap_or_else(|| a.clone()))
                    .collect(),
            },
            capture: capture.clone(),
        },
        Stmt::Branch(b) => Stmt::Branch(Branch {
            condition: b.condition.as_ref().map(|c| rewrite_expr(c, &format!("{path}/cond"), repl)),
            body: b
                .body
                .iter()
                .enumerate()
                .map(|(i, s2)| rewrite_stmt(s2, &format!("{path}/b:{i}"), repl))
                .collect(),
        }),
        Stmt::Use { name, args } => Stmt::Use {
            name: name.clone(),
            args: args
                .iter()
                .enumerate()
                .map(|(i, a)| repl.get(&format!("{path}/a:{i}")).cloned().unwrap_or_else(|| a.clone()))
                .collect(),
        },
    }
}

/// Test-only access to the slot map (diagnostics).
#[cfg(test)]
pub(crate) fn collect_slots_public(q: &QuestDef) -> BTreeMap<String, Value> {
    collect_slots(q)
}

// ---------------------------------------------------------------------------
// Similarity engine (spec §9.3): measure + rank near-identical quests
// ---------------------------------------------------------------------------

/// Pairwise similarity breakdown between two converted quests (spec §9.3).
#[derive(Debug, Clone, PartialEq)]
pub struct QuestSimilarity {
    /// Normalized score in [0, 1] — 1.0 = identical structure and literals.
    /// Structural differences dominate; differing literal VALUES (potential
    /// family params) barely lower the score (params are similarity, not
    /// difference).
    pub score: f64,
    /// Slot-path overlap: shared positions / all positions (the shape).
    pub structural: f64,
    /// Same-kind markers at shared positions (actions, triggers, operators).
    pub marker_ok: f64,
    /// Shared literal slots with equal values (1.0 when none shared).
    pub literal_ok: f64,
    /// Literal slots shared with DIFFERENT values — the family params.
    pub params: Vec<String>,
    /// Slots present only in `a` (its extra structure).
    pub a_only: Vec<String>,
    /// Slots present only in `b`.
    pub b_only: Vec<String>,
}

/// Similarity metric: Jaccard over the slot-path signatures + value
/// agreement. Paths come from `collect_slots` — `__*` markers encode
/// structure (states, events, triggers, action kinds, operators), the rest
/// are literal positions. A path present in both quests with a different
/// literal value counts as a family PARAMETER (similar, not different); a
/// path present in only one quest is a structural delta.
pub fn quest_similarity(a: &QuestDef, b: &QuestDef) -> QuestSimilarity {
    let sa = collect_slots(a);
    let sb = collect_slots(b);
    let mut inter = 0usize;
    let mut marker_shared = 0usize;
    let mut marker_equal = 0usize;
    let mut literal_shared = 0usize;
    let mut literal_equal = 0usize;
    let mut params = Vec::new();
    let mut a_only = Vec::new();
    for (p, va) in &sa {
        match sb.get(p) {
            Some(vb) => {
                inter += 1;
                if is_literal_slot(p) {
                    literal_shared += 1;
                    if va == vb {
                        literal_equal += 1;
                    } else {
                        params.push(p.clone());
                    }
                } else {
                    marker_shared += 1;
                    if va == vb {
                        marker_equal += 1;
                    }
                }
            }
            None => a_only.push(p.clone()),
        }
    }
    let b_only: Vec<String> = sb.keys().filter(|p| !sa.contains_key(*p)).cloned().collect();
    let union = sa.len() + sb.len() - inter;
    let structural = if union == 0 { 1.0 } else { inter as f64 / union as f64 };
    let marker_ok = if marker_shared == 0 { 1.0 } else { marker_equal as f64 / marker_shared as f64 };
    let literal_ok = if literal_shared == 0 { 1.0 } else { literal_equal as f64 / literal_shared as f64 };
    let score = structural * (0.9 * marker_ok + 0.1 * literal_ok);
    QuestSimilarity { score, structural, marker_ok, literal_ok, params, a_only, b_only }
}

/// A literal slot is a path whose LAST segment is not a `__` marker
/// (`state:__giveup__/b:0/a:0` is a literal slot even inside a `__giveup__`).
fn is_literal_slot(p: &str) -> bool {
    !p.rsplit('/').next().unwrap_or(p).starts_with("__")
}

/// A cluster of quests pairwise similar above a threshold (spec §9.3).
#[derive(Debug, Clone, PartialEq)]
pub struct SimilarGroup {
    /// Quest names, in input order.
    pub members: Vec<String>,
    /// Mean pairwise similarity within the group.
    pub mean: f64,
    /// Lowest pairwise similarity within the group.
    pub min: f64,
    /// The common structural signature: slot paths present in EVERY member.
    pub common_paths: Vec<String>,
    /// Literal slots varying across the group (family params): path → value
    /// per member (member order = `members` order).
    pub params: Vec<(String, Vec<Value>)>,
}

/// Clusters quests by pairwise similarity (O(n²), threshold-based union-find).
/// Groups with ≥2 members are returned, ranked by mean score descending.
/// The strict parity gate (`extract_family_params`) is NOT applied here —
/// this layer only measures and proposes (spec §9.3); the merge engine
/// consumes these groups next.
pub fn detect_similar_groups(quests: &[QuestDef], threshold: f64) -> Vec<SimilarGroup> {
    let n = quests.len();
    let mut sim = vec![vec![0.0f64; n]; n];
    for i in 0..n {
        for j in (i + 1)..n {
            let s = quest_similarity(&quests[i], &quests[j]).score;
            sim[i][j] = s;
            sim[j][i] = s;
        }
    }
    // Union-find over pairs above the threshold.
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r {
            r = parent[r];
        }
        parent[x] = r;
        r
    }
    for (i, row) in sim.iter().enumerate() {
        for (j, &v) in row.iter().enumerate().skip(i + 1) {
            if v >= threshold {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        groups.entry(find(&mut parent, i)).or_default().push(i);
    }
    let mut out = Vec::new();
    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        let names: Vec<String> = members.iter().map(|&i| quests[i].name.clone()).collect();
        let mut scores = Vec::new();
        for (k, &i) in members.iter().enumerate() {
            for &j in members.iter().skip(k + 1) {
                scores.push(sim[i][j]);
            }
        }
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        let min = scores.iter().copied().fold(f64::MAX, f64::min);
        // Common signature: slot paths present in EVERY member.
        let slot_maps: Vec<BTreeMap<String, Value>> =
            members.iter().map(|&i| collect_slots(&quests[i])).collect();
        let common_paths: Vec<String> = slot_maps[0]
            .keys()
            .filter(|p| slot_maps.iter().all(|m| m.contains_key(*p)))
            .cloned()
            .collect();
        // Literal slots with ≥2 distinct values across the group → params.
        let mut params = Vec::new();
        for p in &common_paths {
            if !is_literal_slot(p) {
                continue;
            }
            let vals: Vec<Value> = slot_maps.iter().map(|m| m[p].clone()).collect();
            if vals.iter().skip(1).any(|v| v != &vals[0]) {
                params.push((p.clone(), vals));
            }
        }
        out.push(SimilarGroup { members: names, mean, min, common_paths, params });
    }
    out.sort_by(|a, b| b.mean.partial_cmp(&a.mean).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn expands_family_instance() {
        let file = parse(
            "quest collect_quest family (level, mob, herb, drug)\n  state start\n    on login, levelup with pc.level >= (level)\n      -> set_state(information)\n  state information\n    on (mob).kill with number(1, 100) <= 5\n      -> give_item2((herb), 1)\nquest collect_quest_lv30 = collect_quest(level: 30, mob: 601, herb: 30006, drug: 71035)\nquest collect_quest_lv40 = collect_quest(level: 40, mob: 602, herb: 30007, drug: 71036)\n",
        )
        .unwrap();
        let out = expand_families(&file).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "collect_quest_lv30");
        assert_eq!(out[1].name, "collect_quest_lv40");
        // El trigger (mob).kill se sustituyó por 601.kill en la lv30.
        let e = &out[0].states[1].events[0];
        assert!(matches!(
            e.triggers[0].kind,
            crate::ast::TriggerKind::Kill { target: crate::ast::TriggerTarget::Num(601) }
        ));
        // La condición pc.level >= (level) → >= 30 vive en el evento login
        // del state start (states[0]); el evento kill lleva
        // `number(1, 100) <= 5` (Le).
        let login_e = &out[0].states[0].events[0];
        assert!(matches!(
            login_e.condition,
            Some(Expr::Compare(_, CmpOp::Ge, _))
        ));
        assert!(matches!(e.condition, Some(Expr::Compare(_, CmpOp::Le, _))));
        // El arg (herb) → 30006.
        let Stmt::Action { action, .. } = &e.body[0] else { panic!("action") };
        assert_eq!(action.args[0], Value::Num(30006));
    }

    #[test]
    fn substitutes_params_inside_expr_args() {
        // Value::Expr args: a family param inside an arithmetic expression
        // (`set_qf(duration, get_time() + (hours) * 3600)`) is substituted
        // recursively at expansion.
        let file = parse(
            "quest timers family (hours)\n  state start\n    on login\n      -> set_qf(duration, get_time() + (hours) * 3600)\nquest timer_1h = timers(hours: 1)\nquest timer_3h = timers(hours: 3)\n",
        )
        .unwrap();
        let out = expand_families(&file).unwrap();
        assert_eq!(out.len(), 2);
        let Stmt::Action { action, .. } = &out[0].states[0].events[0].body[0] else { panic!("action") };
        let Value::Expr(e) = &action.args[1] else { panic!("expr arg: {:?}", action.args) };
        assert!(matches!(
            e.as_ref(),
            Expr::Add(_, rhs) if matches!(rhs.as_ref(), Expr::Mul(l, r) if matches!(l.as_ref(), Expr::Value(Value::Num(1))) && matches!(r.as_ref(), Expr::Value(Value::Num(3600))))
        ), "{e:?}");
        let Stmt::Action { action, .. } = &out[1].states[0].events[0].body[0] else { panic!() };
        assert!(matches!(&action.args[1], Value::Expr(e) if format!("{e:?}").contains("3")), "{:?}", action.args);
    }

    #[test]
    fn missing_arg_is_error() {
        let file = parse(
            "quest fam family (level)\n  state start\n    on login\n      -> wait()\n\
             quest q = fam()\n",
        )
        .unwrap();
        let e = expand_families(&file).unwrap_err();
        assert!(e.contains("sin argumento `level`"), "{e}");
    }

    #[test]
    fn unknown_base_is_error() {
        let file = parse("quest q = nonexistent(level: 30)\n").unwrap();
        assert!(expand_families(&file).is_err());
    }

    // -- extraction (spec §6 reverse) --

    /// Two corpus-shaped quests that differ ONLY in literals: level (cond),
    /// mob (kill target), herb (give_item2), drug (remove_item) and a
    /// name-embedded locale key.
    fn group() -> (QuestDef, QuestDef) {
        let file = parse(
            "quest collect_quest_lv30\n  state start\n    on login with pc.level >= 30\n      -> set_state(information)\n  state information\n    on letter\n      -> send_letter(gameforge.collect_quest_lv30._10_sendLetter)\n    on 601.kill\n      -> give_item2(30006, 1)\n    on 20084.chat\n      -> remove_item(71035, 1)\n      -> set_qf(duration, 0)\nquest collect_quest_lv40\n  state start\n    on login with pc.level >= 40\n      -> set_state(information)\n  state information\n    on letter\n      -> send_letter(gameforge.collect_quest_lv40._10_sendLetter)\n    on 602.kill\n      -> give_item2(30007, 1)\n    on 20084.chat\n      -> remove_item(71036, 1)\n      -> set_qf(duration, 0)\n",
        )
        .unwrap();
        let defs: Vec<QuestDef> = file
            .quests
            .iter()
            .map(|q| match q {
                Quest::Concrete(d) => d.clone(),
                _ => panic!("esperaba concrete"),
            })
            .collect();
        (defs[0].clone(), defs[1].clone())
    }

    #[test]
    fn extracts_family_params_and_expands_back() {
        let (a, b) = group();
        let ext = extract_family_params("collect_quest", "level", &[a.clone(), b.clone()]).unwrap();
        assert!(ext.excluded.is_empty());
        let Quest::Family { name, params, states } = &ext.family else { panic!("family") };
        assert_eq!(name, "collect_quest");
        // level first (suffix match), then p1..p4 in slot order.
        let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["level", "p1", "p2", "p3", "p4"], "{names:?}");
        assert_eq!(states.len(), 2);
        assert_eq!(ext.instances.len(), 2);
        assert_eq!(ext.instances[0].name, "collect_quest_lv30");
        assert_eq!(ext.instances[0].base, "collect_quest");
        assert_eq!(ext.instances[1].name, "collect_quest_lv40");
        // The template must contain the (param) references.
        let text = crate::render::render(&QuestFile {
            imports: vec![],
            blocks: vec![],
            quests: vec![ext.family.clone(), Quest::Instance(ext.instances[0].clone())],
        });
        assert!(text.contains("pc.level >= (level)"), "{text}");
        assert!(text.contains("(p3).kill"), "{text}");
        assert!(text.contains("give_item2((p2), 1)"), "{text}");
        assert!(text.contains("remove_item((p4), 1)"), "{text}");
        assert!(text.contains(
            "quest collect_quest_lv30 = collect_quest(level: 30, p1: \"gameforge.collect_quest_lv30._10_sendLetter\", p2: 30006, p3: 601, p4: 71035)"
        ), "{text}");
        // Roundtrip: the family file re-parses.
        let file2 = parse(&text).unwrap();
        assert_eq!(
            file2,
            crate::ast::QuestFile {
                imports: vec![],
                blocks: vec![],
                quests: vec![ext.family.clone(), Quest::Instance(ext.instances[0].clone())]
            }
        );
        // Parity: expansion of the instances reproduces the original quests.
        let expanded = expand_families(&file2).unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0], a);
        // Both instances, hand-written family from the extraction output.
        let file3 = parse(
            "quest collect_quest family (level, p1, p2, p3, p4)\n  state start\n    on login with pc.level >= (level)\n      -> set_state(information)\n  state information\n    on letter\n      -> send_letter((p1))\n    on (p3).kill\n      -> give_item2((p2), 1)\n    on 20084.chat\n      -> remove_item((p4), 1)\n      -> set_qf(duration, 0)\nquest collect_quest_lv30 = collect_quest(level: 30, p1: gameforge.collect_quest_lv30._10_sendLetter, p2: 30006, p3: 601, p4: 71035)\nquest collect_quest_lv40 = collect_quest(level: 40, p1: gameforge.collect_quest_lv40._10_sendLetter, p2: 30007, p3: 602, p4: 71036)\n",
        )
        .unwrap();
        let expanded_all = expand_families(&file3).unwrap();
        assert_eq!(expanded_all.len(), 2);
        assert_eq!(expanded_all[0], a);
        assert_eq!(expanded_all[1], b);
    }

    #[test]
    fn extraction_shared_pattern_is_one_param() {
        // Two give_item2 slots with the SAME varying value share one param.
        let file = parse(
            "quest q1\n  state start\n    on login\n      -> give_item2(100, 1)\n    on logout\n      -> give_item2(100, 1)\nquest q2\n  state start\n    on login\n      -> give_item2(200, 1)\n    on logout\n      -> give_item2(200, 1)\n",
        )
        .unwrap();
        let defs: Vec<QuestDef> = file
            .quests
            .iter()
            .map(|q| match q {
                Quest::Concrete(d) => d.clone(),
                _ => panic!(),
            })
            .collect();
        let ext = extract_family_params("q", "level", &defs).unwrap();
        let Quest::Family { params, .. } = &ext.family else { panic!() };
        assert_eq!(params.len(), 1, "{:?}", params);
        assert_eq!(params[0].name, "p1");
    }

    #[test]
    fn extraction_excludes_structural_differences() {
        let file = parse(
            "quest a\n  state start\n    on login\n      -> wait()\nquest b\n  state start\n    on login\n      -> wait()\nquest c\n  state start\n    on login\n      -> wait()\n  state extra\n    on letter\n      -> wait()\n",
        )
        .unwrap();
        let defs: Vec<QuestDef> = file
            .quests
            .iter()
            .map(|q| match q {
                Quest::Concrete(d) => d.clone(),
                _ => panic!(),
            })
            .collect();
        let ext = extract_family_params("x", "index", &defs).unwrap();
        assert_eq!(ext.excluded.len(), 1);
        assert_eq!(ext.excluded[0].0, "c");
        assert_eq!(ext.instances.len(), 2);
        assert!(ext.instances[0].args.is_empty());
        let Quest::Family { states, .. } = &ext.family else { panic!("family") };
        assert_eq!(states.len(), 1);
    }

    #[test]
    fn extraction_single_member_is_error() {
        let file = parse("quest a\n  state start\n    on login\n      -> wait()\n").unwrap();
        let Quest::Concrete(d) = &file.quests[0] else { panic!() };
        assert!(extract_family_params("a", "level", std::slice::from_ref(d)).is_err());
    }

    // -- similarity engine (spec §9.3) --

    fn defs(text: &str) -> Vec<QuestDef> {
        parse(text)
            .unwrap()
            .quests
            .iter()
            .map(|q| match q {
                Quest::Concrete(d) => d.clone(),
                _ => panic!("esperaba concrete"),
            })
            .collect()
    }

    #[test]
    fn similarity_identical_quests_is_one() {
        let (a, _) = group();
        let s = quest_similarity(&a, &a);
        assert_eq!(s.score, 1.0);
        assert_eq!(s.structural, 1.0);
        assert_eq!(s.marker_ok, 1.0);
        assert_eq!(s.literal_ok, 1.0);
        assert!(s.params.is_empty() && s.a_only.is_empty() && s.b_only.is_empty());
    }

    #[test]
    fn similarity_params_only_is_high() {
        // The lv30/lv40 pair differs ONLY in literal values (family params).
        let (a, b) = group();
        let s = quest_similarity(&a, &b);
        assert_eq!(s.structural, 1.0, "{s:?}");
        assert_eq!(s.marker_ok, 1.0, "{s:?}");
        assert!(s.score > 0.85, "{s:?}");
        // level cond, key, herb, mob, drug — the extraction's 5 params.
        assert_eq!(s.params.len(), 5, "{:?}", s.params);
        assert!(s.a_only.is_empty() && s.b_only.is_empty(), "{s:?}");
    }

    #[test]
    fn similarity_structural_delta_lowers_score() {
        let v = defs(
            "quest a\n  state start\n    on login\n      -> wait()\nquest b\n  state start\n    on login\n      -> wait()\n    on letter\n      -> wait()\n",
        );
        // a is a SUBSET of b: the extra event lives in b.
        let s = quest_similarity(&v[0], &v[1]);
        assert!(s.structural < 1.0, "{s:?}");
        assert!(s.a_only.is_empty());
        assert!(!s.b_only.is_empty());
        assert!(s.score < 1.0 && s.score > 0.3, "{s:?}");
        // The reverse direction reports the deltas on a.
        let s2 = quest_similarity(&v[1], &v[0]);
        assert!(!s2.a_only.is_empty());
        assert_eq!(s.score, s2.score);
    }

    #[test]
    fn similarity_different_action_kind_at_same_slot() {
        let v = defs(
            "quest a\n  state start\n    on login\n      -> say(@x)\nquest b\n  state start\n    on login\n      -> say_title(@x)\n",
        );
        let s = quest_similarity(&v[0], &v[1]);
        assert!(s.marker_ok < 1.0, "{s:?}");
        assert!(s.score < 1.0, "{s:?}");
        // Same literal value — not a param; the difference is the ACTION.
        assert!(s.params.is_empty(), "{:?}", s.params);
    }

    #[test]
    fn detects_similar_groups() {
        // a/b: params-only pair (0.95); c: a different quest.
        let v = defs(
            "quest a\n  state start\n    on login\n      -> give_item2(100, 1)\nquest b\n  state start\n    on login\n      -> give_item2(200, 1)\nquest c\n  state start\n    on letter\n      -> say_title(@k)\n      -> warp(1, 2)\n",
        );
        let groups = detect_similar_groups(&v, 0.5);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members, vec!["a", "b"]);
        assert!(groups[0].mean > 0.85, "{:?}", groups[0]);
        assert_eq!(groups[0].params.len(), 1);
        assert_eq!(groups[0].params[0].1, vec![Value::Num(100), Value::Num(200)]);
        // The common signature contains the shared structural markers.
        assert!(groups[0].common_paths.iter().any(|p| p.ends_with("__act")), "{:?}", groups[0].common_paths);
        // Threshold above the pair score → no groups.
        assert!(detect_similar_groups(&v, 0.99).is_empty());
    }

    #[test]
    fn similarity_groups_ranked_by_mean() {
        // ab pair (0.95) + cd pair (lower): two groups, ab first.
        let v = defs(
            "quest a\n  state start\n    on login\n      -> give_item2(100, 1)\nquest b\n  state start\n    on login\n      -> give_item2(200, 1)\nquest c\n  state start\n    on letter\n      -> say_title(@k)\nquest d\n  state start\n    on letter\n      -> say_title(@k2)\n      -> warp(1, 2)\n",
        );
        let groups = detect_similar_groups(&v, 0.5);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].members, vec!["a", "b"]);
        assert_eq!(groups[1].members, vec!["c", "d"]);
        assert!(groups[0].mean > groups[1].mean, "{:?}", groups);
    }
}
