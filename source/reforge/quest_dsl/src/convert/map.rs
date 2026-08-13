//! Equivalence tables legacy qc → DSL (spec §3-§5) + expression/statement
//! mappers. Verified against the deployed corpus (2026-08-13) and the
//! legacy bindings (`questlua_pc.cpp`, `questlua_global.cpp`,
//! `questlua_target.cpp` registrations).
//!
//! Contract: KNOWN names map to DSL; UNKNOWN names are `Err`/`None` and are
//! collected into the discrepancy report — the converter never fails on an
//! unmapped call (spec §9.2: equivalence tables + discrepancy report).

use crate::ast::*;
use crate::convert::qc::{split_args, LegacyStmt};
use crate::convert::Unmapped;

// ---------------------------------------------------------------------------
// Actions (spec §5)
// ---------------------------------------------------------------------------

/// Legacy script call name → DSL action. The script names are the ones the
/// corpus uses (`pc.setqf(...)`, `target.vid(...)`, `set_state(...)`).
pub const ACTION_TABLE: &[(&str, ActionName)] = &[
    ("say_title", ActionName::SayTitle),
    ("say", ActionName::Say),
    ("say_reward", ActionName::SayReward),
    ("say_item_vnum", ActionName::SayItemVnum),
    ("send_letter", ActionName::SendLetter),
    ("clear_letter", ActionName::ClearLetter),
    ("wait", ActionName::Wait),
    ("set_state", ActionName::SetState),
    ("setstate", ActionName::SetState), // alias legacy (questlua_quest.cpp:215)
    ("set_quest_state", ActionName::SetQuestState),
    ("pc.setqf", ActionName::SetQf),
    ("pc.give_item2", ActionName::GiveItem2),
    ("pc.remove_item", ActionName::RemoveItem),
    ("target.vid", ActionName::TargetVid),
    ("target.delete", ActionName::TargetDelete),
    ("pc.warp", ActionName::Warp),
    ("notice", ActionName::Notice),
    ("notice_multiline", ActionName::NoticeMultiline),
    ("affect.add_collect", ActionName::AffectAdd),
    ("affect.remove_all_collect", ActionName::AffectRemove),
    ("select", ActionName::Select),
    ("input_number", ActionName::InputNumber),
];

pub fn map_action_name(name: &str) -> Option<ActionName> {
    ACTION_TABLE.iter().find(|(n, _)| *n == name).map(|(_, a)| a.clone())
}

// ---------------------------------------------------------------------------
// Triggers (spec §3)
// ---------------------------------------------------------------------------

/// Maps a raw legacy trigger to a DSL trigger.
/// `20084.chat.gameforge.x._30_npcChat` → `20084.chat` (the npcChat locale
/// key is dropped — a UI string; recorded as a note by the converter).
/// Errors: bare `kill` (any mob — no DSL equivalent), unknown names.
pub fn map_trigger(text: &str) -> Result<Trigger, String> {
    let t = text.trim();
    let kind = match t {
        "login" => TriggerKind::Login,
        "levelup" => TriggerKind::LevelUp,
        "letter" => TriggerKind::Letter,
        "button" => TriggerKind::Button,
        "info" => TriggerKind::Info,
        "enter" => TriggerKind::Enter,
        "logout" => TriggerKind::Logout,
        "timer" => TriggerKind::Timer,
        "__TARGET__.target.click" => TriggerKind::TargetClick,
        "kill" => return Err("kill (cualquier mob) no existe en el DSL — requiere Rust hook".into()),
        _ => {
            let (vnum, rest) = t.split_once('.').ok_or_else(|| format!("trigger desconocido: {t}"))?;
            let v: u32 = vnum.parse().map_err(|_| format!("trigger sin vnum: {t}"))?;
            match rest.split('.').next().unwrap_or("") {
                "kill" => TriggerKind::Kill { target: TriggerTarget::Num(v) },
                "use" => TriggerKind::Use { target: TriggerTarget::Num(v) },
                "chat" => TriggerKind::Chat { target: TriggerTarget::Num(v) },
                other => return Err(format!("trigger desconocido: {t} ({other})")),
            }
        }
    };
    Ok(Trigger { kind })
}

/// True when the legacy trigger carries an npcChat locale key suffix
/// (`20084.chat.gameforge.x._30_npcChat`) — the converter records it.
pub fn chat_key_suffix(text: &str) -> Option<&str> {
    let mut parts = text.split('.');
    let _vnum = parts.next()?;
    if parts.next()? != "chat" {
        return None;
    }
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() {
        None
    } else {
        Some(&text[text.find(rest[0])?..])
    }
}

// ---------------------------------------------------------------------------
// Conditions / expressions (spec §4)
// ---------------------------------------------------------------------------

/// Locals/assignments in scope of an event body. `local s = number(1, 100)`
/// binds `s` → the mapped expression; uses are substituted at map time.
/// `local sel = select(...)` binds a DSL capture (emitted bare).
#[derive(Debug, Clone, PartialEq)]
pub enum Binding {
    Expr(Expr),
    Capture,
    Unmapped,
}

#[derive(Debug, Clone, Default)]
pub struct Scope {
    map: std::collections::HashMap<String, Binding>,
}

impl Scope {
    pub fn insert(&mut self, name: &str, b: Binding) {
        self.map.insert(name.to_string(), b);
    }
    pub fn lookup(&self, name: &str) -> Option<&Binding> {
        self.map.get(name)
    }
}

/// Legacy condition function → DSL (spec §4; `pc.get_level()` → `pc.level`).
fn map_func_name(t: &str) -> Result<FuncName, String> {
    match t {
        "pc.get_level" | "pc.level" => Ok(FuncName::PcLevel),
        "pc.getqf" => Ok(FuncName::GetQf),
        "pc.count_item" => Ok(FuncName::CountItem),
        "number" => Ok(FuncName::Number),
        "get_time" => Ok(FuncName::GetTime),
        "pc.get_map_index" | "get_map_index" => Ok(FuncName::GetMapIndex),
        "pc.get_gm_level" | "get_gm_level" => Ok(FuncName::GetGmLevel),
        "pet.is_summon" => Ok(FuncName::PetIsSummon),
        "is_test_server" => Ok(FuncName::IsTestServer),
        other => Err(format!("función sin mapear: {other}")),
    }
}

/// Bare (paren-less) properties/functions the DSL knows: `pc.level`,
/// `get_time()`, ... (spec §4).
fn map_bare(t: &str) -> Option<FuncName> {
    match t {
        "pc.level" => Some(FuncName::PcLevel),
        "get_time" => Some(FuncName::GetTime),
        "get_map_index" => Some(FuncName::GetMapIndex),
        "get_gm_level" => Some(FuncName::GetGmLevel),
        "is_test_server" => Some(FuncName::IsTestServer),
        _ => None,
    }
}

/// Maps a legacy condition/expression to a DSL expression, substituting
/// in-scope locals. Unknown functions/properties/identifiers → `Err`
/// (reported, not fatal).
pub fn map_expr(text: &str, scope: &Scope) -> Result<Expr, String> {
    let toks = lex_expr(text)?;
    let mut p = ExprMapper { toks, pos: 0, scope };
    let e = p.parse_or()?;
    if p.pos != p.toks.len() {
        return Err(format!("expresión malformada cerca de {:?}", &p.toks[p.pos..]));
    }
    Ok(e)
}

struct ExprMapper<'a> {
    toks: Vec<String>,
    pos: usize,
    scope: &'a Scope,
}

impl ExprMapper<'_> {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.pos).map(String::as_str)
    }

    fn eat(&mut self, expected: &str) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_and()?;
        while self.eat("or") {
            let rhs = self.parse_and()?;
            lhs = Expr::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_cmp()?;
        while self.eat("and") {
            let rhs = self.parse_cmp()?;
            lhs = Expr::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_cmp(&mut self) -> Result<Expr, String> {
        let lhs = self.parse_add()?;
        let Some(op) = self.peek_cmp() else { return Ok(lhs) };
        self.pos += 1;
        let rhs = self.parse_add()?;
        // Corpus idiom `true == pet.is_summon(...)` → `pet.is_summon(...)`.
        if op == "==" && is_true(&lhs) {
            return Ok(rhs);
        }
        if op == "==" && is_true(&rhs) {
            return Ok(lhs);
        }
        let op = match op.as_str() {
            "==" => CmpOp::Eq,
            "!=" | "~=" => CmpOp::Ne,
            "<" => CmpOp::Lt,
            ">" => CmpOp::Gt,
            "<=" => CmpOp::Le,
            ">=" => CmpOp::Ge,
            _ => return Err(format!("operador desconocido: {op}")),
        };
        Ok(Expr::Compare(Box::new(lhs), op, Box::new(rhs)))
    }

    fn peek_cmp(&self) -> Option<String> {
        match self.peek() {
            Some("==") | Some("!=") | Some("~=") | Some("<") | Some(">") | Some("<=") | Some(">=") => {
                self.peek().map(str::to_string)
            }
            _ => None,
        }
    }

    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_mul()?;
        loop {
            if self.eat("+") {
                lhs = Expr::Add(Box::new(lhs), Box::new(self.parse_mul()?));
            } else if self.eat("-") {
                lhs = Expr::Sub(Box::new(lhs), Box::new(self.parse_mul()?));
            } else if self.eat("..") {
                return Err("concatenación `..` sin mapear (strings dinámicos)".into());
            } else {
                return Ok(lhs);
            }
        }
    }

    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_primary()?;
        loop {
            if self.eat("*") {
                lhs = Expr::Mul(Box::new(lhs), Box::new(self.parse_primary()?));
            } else if self.eat("/") {
                lhs = Expr::Div(Box::new(lhs), Box::new(self.parse_primary()?));
            } else {
                return Ok(lhs);
            }
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        if self.eat("not") {
            return Ok(Expr::Not(Box::new(self.parse_primary()?)));
        }
        let Some(tok) = self.peek() else {
            return Err("expresión vacía".into());
        };
        let t = tok.to_string();
        if let Ok(n) = t.parse::<i64>() {
            self.pos += 1;
            return Ok(Expr::Value(Value::Num(n)));
        }
        if is_quoted(&t) {
            self.pos += 1;
            return Ok(Expr::Value(Value::Str(t[1..t.len() - 1].to_string())));
        }
        // Function call: `name(args)`.
        if self.toks.get(self.pos + 1).map(|s| s == "(").unwrap_or(false) {
            self.pos += 2;
            let mut args = Vec::new();
            if !self.eat(")") {
                loop {
                    args.push(self.parse_or()?);
                    if self.eat(")") {
                        break;
                    }
                    if !self.eat(",") {
                        return Err("se esperaba `,` o `)` en la llamada".into());
                    }
                }
            }
            return Ok(Expr::Func(map_func_name(&t)?, args));
        }
        self.pos += 1;
        if t == "true" {
            return Ok(Expr::Capture("true".into()));
        }
        if let Some(f) = map_bare(&t) {
            return Ok(Expr::Func(f, Vec::new()));
        }
        match self.scope.lookup(&t) {
            Some(Binding::Expr(e)) => Ok(e.clone()),
            Some(Binding::Capture) => Ok(Expr::Capture(t)),
            Some(Binding::Unmapped) => Err(format!("local sin mapear: {t}")),
            None => Err(format!("función/propiedad sin mapear: {t}")),
        }
    }
}

fn is_true(e: &Expr) -> bool {
    matches!(e, Expr::Capture(c) if c == "true")
}

fn is_quoted(t: &str) -> bool {
    t.len() >= 2 && ((t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')))
}

/// Legacy expression lexer: dotted names are single tokens (`pc.getqf`,
/// `gameforge.x._10_sendLetter`), strings single- or double-quoted,
/// `!=`/`~=` both accepted, `..` is an operator token.
fn lex_expr(text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut cs = text.chars().peekable();
    while let Some(c) = cs.next() {
        match c {
            '"' | '\'' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                let mut s = String::new();
                s.push(c);
                for c2 in cs.by_ref() {
                    s.push(c2);
                    if c2 == c {
                        break;
                    }
                }
                if !s.ends_with(c) {
                    return Err("string sin cerrar".into());
                }
                out.push(s);
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            '(' | ')' | ',' | '[' | ']' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push(c.to_string());
            }
            c if c.is_ascii_alphanumeric()
                || c == '_'
                || (c == '.' && !cur.is_empty() && matches!(cs.peek(), Some(p) if p.is_ascii_alphanumeric() || *p == '_')) =>
            {
                cur.push(c);
            }
            _ => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                let mut op = String::new();
                op.push(c);
                if matches!(c, '=' | '!' | '~' | '<' | '>') && cs.peek() == Some(&'=') {
                    op.push('=');
                    cs.next();
                }
                if c == '.' && cs.peek() == Some(&'.') {
                    op.push('.');
                    cs.next();
                }
                out.push(op);
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Values (action args)
// ---------------------------------------------------------------------------

/// Maps a legacy action argument to a DSL `Value`. Bare identifiers default
/// to strings (state names, full locale keys `gameforge.q._10_sendLetter`,
/// `apply.MOV_SPEED` — the DSL stores them verbatim for exact parity).
/// Locals are substituted; full expressions (arithmetic, calls) map to
/// `Value::Expr` — the runtime evaluates them
/// (`pc.setqf("duration", get_time()+60*60*22)`,
/// `affect.add_collect(apply.MOV_SPEED, 10, 60*60*24*365*60)`).
pub fn map_value(text: &str, scope: &Scope) -> Result<Value, String> {
    let t = text.trim();
    if let Ok(n) = t.parse::<i64>() {
        return Ok(Value::Num(n));
    }
    if is_quoted(t) {
        return Ok(Value::Str(t[1..t.len() - 1].to_string()));
    }
    if let Some(k) = t.strip_prefix('@') {
        return Ok(Value::Key(k.to_string()));
    }
    if t.chars().any(|c| "()+*/<>= ".contains(c)) {
        let e = map_expr(t, scope)?;
        return match e {
            Expr::Value(v) => Ok(v),
            // Full expression args (set_qf values, affect durations).
            other => Ok(Value::Expr(Box::new(other))),
        };
    }
    match scope.lookup(t) {
        Some(Binding::Expr(e)) => match e {
            Expr::Value(v) => Ok(v.clone()),
            // A local bound to a full expression (`local d = get_time() + ...`).
            other => Ok(Value::Expr(Box::new(other.clone()))),
        },
        Some(Binding::Capture) => Err(format!("captura `{t}` no permitida como argumento")),
        Some(Binding::Unmapped) => Err(format!("local sin mapear: {t}")),
        None => Ok(Value::Str(t.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Statements (event bodies)
// ---------------------------------------------------------------------------

/// Maps legacy event-body statements to DSL statements. Unmappable
/// constructs (loops, elseif chains, unmapped calls/locals) are pushed to
/// `report` and SKIPPED — siblings keep converting (the discrepancy report
/// tells the human exactly what needs review).
pub fn map_stmts(stmts: &[LegacyStmt], file: &str, report: &mut Vec<Unmapped>) -> Vec<Stmt> {
    let mut scope = Scope::default();
    map_stmts_inner(stmts, &mut scope, file, report)
}

fn map_stmts_inner(stmts: &[LegacyStmt], scope: &mut Scope, file: &str, report: &mut Vec<Unmapped>) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < stmts.len() {
        // Corpus idiom: `local v = find_npc_by_vnum(N)` + `if v != 0 then
        // target.vid("__TARGET__", v, key) end` → `-> target_vid(__TARGET__, N, key)`
        // (the DSL action does the lookup at runtime — spec §7 example).
        if let Some(st) = try_map_target_vid(stmts, i, scope, file) {
            out.push(st);
            i += 2;
            continue;
        }
        match &stmts[i] {
            LegacyStmt::Call { name, args } => {
                if name == "return" {
                    out.push(action_stmt(ActionName::Return, Vec::new()));
                } else if let Some(aname) = map_action_name(name) {
                    let mut margs = Vec::new();
                    let mut ok = true;
                    for a in args {
                        match map_value(a, scope) {
                            Ok(v) => margs.push(v),
                            Err(e) => {
                                report.push(Unmapped::new(file, format!("call:{name}:{e}")));
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        out.push(action_stmt(aname, margs));
                    }
                } else {
                    report.push(Unmapped::new(file, format!("call:{name}")));
                }
            }
            LegacyStmt::Local { name, value: Some(v) } => bind_local(name, v, scope, &mut out, file, report),
            LegacyStmt::Assign { name, value } => bind_local(name, value, scope, &mut out, file, report),
            LegacyStmt::Local { name, value: None } => scope.insert(name, Binding::Unmapped),
            LegacyStmt::Return { value: _ } => out.push(action_stmt(ActionName::Return, Vec::new())),
            LegacyStmt::If { cond, then, elseif_, els } => {
                if !elseif_.is_empty() {
                    report.push(Unmapped::new(file, "if:elseif (anidado no permitido en el DSL)"));
                } else {
                    match map_expr(cond, scope) {
                        Ok(c) => {
                            let tbody = map_stmts_inner(then, scope, file, report);
                            let mut branches = vec![Stmt::Branch(Branch { condition: Some(c), body: tbody })];
                            if !els.is_empty() {
                                let ebody = map_stmts_inner(els, scope, file, report);
                                branches.push(Stmt::Branch(Branch { condition: None, body: ebody }));
                            }
                            out.extend(branches);
                        }
                        Err(e) => report.push(Unmapped::new(file, format!("if:{e}"))),
                    }
                }
            }
            LegacyStmt::For { head, .. } => {
                report.push(Unmapped::new(file, format!("for:{head} (no hay loops en el DSL)")));
            }
            LegacyStmt::While { head, .. } => {
                report.push(Unmapped::new(file, format!("while:{head} (no hay loops en el DSL)")));
            }
            LegacyStmt::Repeat { until, .. } => {
                report.push(Unmapped::new(file, format!("repeat:until {until} (no hay loops en el DSL)")));
            }
            LegacyStmt::Raw(r) => report.push(Unmapped::new(file, format!("raw:{r}"))),
        }
        i += 1;
    }
    out
}

/// `local x = value` / `x = value` — binds the mapped expression (for
/// substitution at uses) or emits `-> select(...) as x` / `-> input_number(...) as x`
/// when the value is one of the capturing calls (spec §10).
fn bind_local(
    name: &str,
    value: &str,
    scope: &mut Scope,
    out: &mut Vec<Stmt>,
    file: &str,
    report: &mut Vec<Unmapped>,
) {
    let v = value.trim();
    for prefix in ["select(", "input_number("] {
        if let Some(args_text) = v.strip_prefix(prefix).and_then(|r| r.strip_suffix(')')) {
            let mut args = Vec::new();
            let mut ok = true;
            for a in split_args(args_text) {
                match map_value(&a, scope) {
                    Ok(x) => args.push(x),
                    Err(e) => {
                        report.push(Unmapped::new(file, format!("{prefix}:{e}")));
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                let aname = if prefix.starts_with("select") { ActionName::Select } else { ActionName::InputNumber };
                out.push(Stmt::Action { action: Action { name: aname, args }, capture: Some(name.to_string()) });
                scope.insert(name, Binding::Capture);
                return;
            }
        }
    }
    match map_expr(v, scope) {
        Ok(e) => scope.insert(name, Binding::Expr(e)),
        // Unmapped bindings are reported at bind time (a table literal, a
        // special.* lookup...) so the discrepancy report is complete even if
        // the local is never used by a mapped statement.
        Err(e) => {
            report.push(Unmapped::new(file, format!("local:{name}:{e}")));
            scope.insert(name, Binding::Unmapped);
        }
    }
}

/// The `find_npc_by_vnum` → `target_vid` corpus idiom (see above).
fn try_map_target_vid(
    stmts: &[LegacyStmt],
    i: usize,
    scope: &Scope,
    file: &str,
) -> Option<Stmt> {
    let LegacyStmt::Local { name, value: Some(v) } = &stmts[i] else { return None };
    let num = v
        .trim()
        .strip_prefix("find_npc_by_vnum(")?
        .strip_suffix(')')?
        .trim()
        .parse::<i64>()
        .ok()?;
    let LegacyStmt::If { cond, then, elseif_, els } = stmts.get(i + 1)? else { return None };
    if !elseif_.is_empty() || !els.is_empty() || then.len() != 1 {
        return None;
    }
    let guard = cond.trim();
    let guard_ok = guard == format!("{name} != 0") || guard == format!("{name} ~= 0") || guard == format!("0 != {name}");
    if !guard_ok {
        return None;
    }
    let LegacyStmt::Call { name: cname, args } = &then[0] else { return None };
    if cname != "target.vid" || args.len() != 3 || args[1].trim() != name {
        return None;
    }
    let tname = map_value(args[0].trim(), scope).ok()?;
    let key = map_value(args[2].trim(), scope).ok()?;
    let _ = file;
    Some(Stmt::Action {
        action: Action { name: ActionName::TargetVid, args: vec![tname, Value::Num(num), key] },
        capture: None,
    })
}

fn action_stmt(name: ActionName, args: Vec<Value>) -> Stmt {
    Stmt::Action { action: Action { name, args }, capture: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope_with(pairs: &[(&str, &str)]) -> Scope {
        let mut s = Scope::default();
        for (k, v) in pairs {
            match map_expr(v, &s) {
                Ok(e) => s.insert(k, Binding::Expr(e)),
                Err(_) => s.insert(k, Binding::Unmapped),
            }
        }
        s
    }

    #[test]
    fn maps_triggers() {
        assert!(matches!(map_trigger("login"), Ok(Trigger { kind: TriggerKind::Login })));
        assert!(matches!(
            map_trigger("601.kill"),
            Ok(Trigger { kind: TriggerKind::Kill { target: TriggerTarget::Num(601) } })
        ));
        assert!(matches!(
            map_trigger("71035.use"),
            Ok(Trigger { kind: TriggerKind::Use { target: TriggerTarget::Num(71035) } })
        ));
        assert!(matches!(
            map_trigger("20084.chat.gameforge.collect_quest_lv30._30_npcChat"),
            Ok(Trigger { kind: TriggerKind::Chat { target: TriggerTarget::Num(20084) } })
        ));
        assert!(matches!(map_trigger("__TARGET__.target.click"), Ok(Trigger { kind: TriggerKind::TargetClick })));
        assert!(map_trigger("kill").is_err());
        assert!(map_trigger("dungeon.foo").is_err());
    }

    #[test]
    fn chat_key_suffix_detection() {
        assert_eq!(
            chat_key_suffix("20084.chat.gameforge.collect_quest_lv30._30_npcChat"),
            Some("gameforge.collect_quest_lv30._30_npcChat")
        );
        assert_eq!(chat_key_suffix("20084.chat"), None);
        assert_eq!(chat_key_suffix("login"), None);
    }

    #[test]
    fn maps_actions() {
        assert_eq!(map_action_name("pc.setqf"), Some(ActionName::SetQf));
        assert_eq!(map_action_name("target.vid"), Some(ActionName::TargetVid));
        assert_eq!(map_action_name("say_title"), Some(ActionName::SayTitle));
        assert_eq!(map_action_name("wait"), Some(ActionName::Wait));
        assert_eq!(map_action_name("pc.job"), None);
        assert_eq!(map_action_name("pc.give_item"), None);
        assert_eq!(map_action_name("syschat"), None);
    }

    #[test]
    fn maps_conditions() {
        let s = Scope::default();
        // pc.get_level() → pc.level
        let e = map_expr("pc.get_level() >= 30", &s).unwrap();
        assert!(matches!(e, Expr::Compare(_, CmpOp::Ge, _)));
        // number(1, 100) <= 5
        assert!(matches!(map_expr("number(1, 100) <= 5", &s).unwrap(), Expr::Compare(_, CmpOp::Le, _)));
        // pc.count_item(30006) == 0
        let e = map_expr("pc.count_item(30006) == 0", &s).unwrap();
        assert!(matches!(e, Expr::Compare(_, CmpOp::Eq, _)));
        // true == pet.is_summon(34003) or true == pet.is_summon(34001)
        let e = map_expr("true == pet.is_summon(34003) or true == pet.is_summon(34001)", &s).unwrap();
        let Expr::Or(a, b) = e else { panic!("or") };
        assert!(matches!(*a, Expr::Func(FuncName::PetIsSummon, _)));
        assert!(matches!(*b, Expr::Func(FuncName::PetIsSummon, _)));
        // get_time() > pc.getqf("duration")
        let e = map_expr("get_time() > pc.getqf(\"duration\")", &s).unwrap();
        assert!(matches!(e, Expr::Compare(_, CmpOp::Gt, _)));
        // bare pc.level (property, levelup.quest: `pc.level >= lev`)
        let s = scope_with(&[("lev", "pc.getqf(\"current\")")]);
        let e = map_expr("pc.level >= lev", &s).unwrap();
        assert!(matches!(e, Expr::Compare(_, CmpOp::Ge, _)));
    }

    #[test]
    fn substitutes_locals_in_conditions() {
        // local s = number(1, 100); if s <= 5 → number(1, 100) <= 5
        let s = scope_with(&[("s", "number(1, 100)"), ("pass_percent", "60")]);
        let e = map_expr("s <= pass_percent", &s).unwrap();
        assert!(matches!(e, Expr::Compare(_, CmpOp::Le, _)));
        // local lev = pc.getqf("current"); if lev == 0 → get_qf("current") == 0
        let s = scope_with(&[("lev", "pc.getqf(\"current\")")]);
        let e = map_expr("lev == 0", &s).unwrap();
        assert!(matches!(e, Expr::Compare(_, CmpOp::Eq, _)));
    }

    #[test]
    fn unmapped_expressions_error() {
        let s = Scope::default();
        assert!(map_expr("string.format(gameforge.x._10_say, 1)", &s).is_err());
        assert!(map_expr("table.getn(reward_gold)", &s).is_err());
        assert!(map_expr("pc.job == 1", &s).is_err());
        assert!(map_expr("special.levelup_quest[lev][2]", &s).is_err());
        assert!(map_expr("item_name(30006)", &s).is_err());
        // concat
        assert!(map_expr("a .. b", &s).is_err());
        // local sin mapear
        let s = scope_with(&[("v", "find_npc_by_vnum(20084)")]);
        assert!(map_expr("v != 0", &s).is_err());
    }

    #[test]
    fn maps_values() {
        let s = scope_with(&[("pass_percent", "60")]);
        assert_eq!(map_value("30006", &s).unwrap(), Value::Num(30006));
        assert_eq!(map_value("1", &s).unwrap(), Value::Num(1));
        assert_eq!(map_value("\"duration\"", &s).unwrap(), Value::Str("duration".into()));
        assert_eq!(map_value("'x'", &s).unwrap(), Value::Str("x".into()));
        // full locale key → verbatim string (exact parity)
        assert_eq!(
            map_value("gameforge.collect_quest_lv30._10_sendLetter", &s).unwrap(),
            Value::Str("gameforge.collect_quest_lv30._10_sendLetter".into())
        );
        // apply.MOV_SPEED stays a string
        assert_eq!(map_value("apply.MOV_SPEED", &s).unwrap(), Value::Str("apply.MOV_SPEED".into()));
        // local substitution
        assert_eq!(map_value("pass_percent", &s).unwrap(), Value::Num(60));
        // full expressions map to Value::Expr (runtime-evaluated)
        let e = map_value("60*60*24*365*60", &s).unwrap();
        assert!(matches!(e, Value::Expr(_)), "{e:?}");
        let e = map_value("pc.getqf(\"x\")", &s).unwrap();
        assert!(matches!(e, Value::Expr(_)), "{e:?}");
    }

    #[test]
    fn maps_expr_args_in_set_qf_and_affect() {
        let mut rep = Vec::new();
        let stmts = vec![
            LegacyStmt::Call { name: "pc.setqf".into(), args: vec!["\"duration\"".into(), "get_time()+60*60*22".into()] },
            LegacyStmt::Call {
                name: "affect.add_collect".into(),
                args: vec!["apply.MOV_SPEED".into(), "10".into(), "60*60*24*365*60".into()],
            },
        ];
        let out = map_stmts(&stmts, "t.quest", &mut rep);
        assert!(rep.is_empty(), "{rep:?}");
        assert_eq!(out.len(), 2);
        let Stmt::Action { action, .. } = &out[0] else { panic!() };
        assert_eq!(action.name, ActionName::SetQf);
        assert_eq!(action.args[0], Value::Str("duration".into()));
        assert!(matches!(&action.args[1], Value::Expr(e) if matches!(e.as_ref(), Expr::Add(_, _))), "{:?}", action.args[1]);
        let Stmt::Action { action, .. } = &out[1] else { panic!() };
        assert_eq!(action.name, ActionName::AffectAdd);
        assert_eq!(action.args[0], Value::Str("apply.MOV_SPEED".into()));
        assert_eq!(action.args[1], Value::Num(10));
        assert!(matches!(&action.args[2], Value::Expr(_)), "{:?}", action.args[2]);
    }

    #[test]
    fn maps_statements_with_locals_and_if() {
        let mut rep = Vec::new();
        let stmts = vec![
            LegacyStmt::Local { name: "s".into(), value: Some("number(1, 100)".into()) },
            LegacyStmt::If {
                cond: "s <= 5".into(),
                then: vec![LegacyStmt::Call { name: "pc.give_item2".into(), args: vec!["30006".into(), "1".into()] }],
                elseif_: vec![],
                els: vec![],
            },
        ];
        let out = map_stmts(&stmts, "t.quest", &mut rep);
        assert_eq!(out.len(), 1);
        let Stmt::Branch(b) = &out[0] else { panic!("if") };
        assert!(matches!(b.condition, Some(Expr::Compare(_, CmpOp::Le, _))));
        let Stmt::Action { action, .. } = &b.body[0] else { panic!() };
        assert_eq!(action.name, ActionName::GiveItem2);
        assert_eq!(action.args, vec![Value::Num(30006), Value::Num(1)]);
        assert!(rep.is_empty());
    }

    #[test]
    fn reports_unmapped_and_skips() {
        let mut rep = Vec::new();
        let stmts = vec![
            LegacyStmt::Call { name: "pc.job".into(), args: vec![] },
            LegacyStmt::Call { name: "pc.setqf".into(), args: vec!["\"a\"".into(), "\"x\" .. y".into()] },
            LegacyStmt::For { head: "i = 1, 10".into(), body: vec![] },
            LegacyStmt::Raw("mystery()".into()),
        ];
        let out = map_stmts(&stmts, "t.quest", &mut rep);
        assert!(out.is_empty());
        assert_eq!(rep.len(), 4);
        assert!(rep[0].item.starts_with("call:pc.job"));
        assert!(rep[1].item.starts_with("call:pc.setqf:"));
        assert!(rep[2].item.starts_with("for:"));
        assert!(rep[3].item.starts_with("raw:"));
    }

    #[test]
    fn maps_find_npc_target_vid_idiom() {
        let mut rep = Vec::new();
        let stmts = vec![
            LegacyStmt::Local { name: "v".into(), value: Some("find_npc_by_vnum(20084)".into()) },
            LegacyStmt::If {
                cond: "v != 0".into(),
                then: vec![LegacyStmt::Call {
                    name: "target.vid".into(),
                    args: vec!["\"__TARGET__\"".into(), "v".into(), "gameforge.collect_herb_lv10._150_sayTitle".into()],
                }],
                elseif_: vec![],
                els: vec![],
            },
            LegacyStmt::Call { name: "send_letter".into(), args: vec!["gameforge.collect_quest_lv30._10_sendLetter".into()] },
        ];
        let out = map_stmts(&stmts, "t.quest", &mut rep);
        assert!(rep.is_empty());
        assert_eq!(out.len(), 2);
        let Stmt::Action { action, .. } = &out[0] else { panic!() };
        assert_eq!(action.name, ActionName::TargetVid);
        assert_eq!(
            action.args,
            vec![
                Value::Str("__TARGET__".into()),
                Value::Num(20084),
                Value::Str("gameforge.collect_herb_lv10._150_sayTitle".into())
            ]
        );
    }

    #[test]
    fn maps_select_capture() {
        let mut rep = Vec::new();
        let stmts = vec![
            LegacyStmt::Local { name: "sel".into(), value: Some("select(gameforge.locale.confirm)".into()) },
            LegacyStmt::If {
                cond: "sel == 1".into(),
                then: vec![LegacyStmt::Call { name: "set_state".into(), args: vec!["go".into()] }],
                elseif_: vec![],
                els: vec![],
            },
        ];
        let out = map_stmts(&stmts, "t.quest", &mut rep);
        assert!(rep.is_empty());
        assert_eq!(out.len(), 2);
        let Stmt::Action { action, capture } = &out[0] else { panic!() };
        assert_eq!(action.name, ActionName::Select);
        assert_eq!(capture.as_deref(), Some("sel"));
        let Stmt::Branch(b) = &out[1] else { panic!() };
        assert!(matches!(b.condition, Some(Expr::Compare(_, CmpOp::Eq, _))));
    }
}
