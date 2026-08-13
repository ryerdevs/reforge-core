//! Parser of the quest DSL: indentation-significant, typed catalog.
//!
//! Grammar (spec §2):
//! - `quest <name>` / `quest <name> family (<params>)` / `quest <name> = <base>(<args>)`
//! - `import <file>` (no extension), `block <name>(<params>)`, `use <name>(<args>)`
//! - `state <name>` → `on <trigger>[, <trigger>...] [with <expr>]` → `-> <action>(<args>) [as <x>]`
//! - `if <expr>` / `else` (1 level + else, spec §10)
//! - `#` comments, 2-space indentation.
//!
//! Every trigger, action and condition name is validated against the typed
//! catalog — an unknown name is a load error with line:column (spec §2 rule).

use crate::ast::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.msg)
    }
}

type R<T> = Result<T, ParseError>;

fn err<T>(line: usize, msg: impl Into<String>) -> R<T> {
    Err(ParseError { line, col: 1, msg: msg.into() })
}

/// Parse a quest file (imports + blocks + quests).
pub fn parse(text: &str) -> R<QuestFile> {
    let mut file = QuestFile { imports: Vec::new(), blocks: Vec::new(), quests: Vec::new() };
    let mut lines = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        lines.push((line_no, indent, trimmed.to_string()));
    }

    let mut idx = 0;
    // Imports must come first (spec §7).
    while idx < lines.len() {
        let (ln, ind, text) = &lines[idx];
        if *ind != 0 {
            return err(*ln, "import fuera de nivel raíz");
        }
        if let Some(rest) = text.strip_prefix("import ") {
            file.imports.push(rest.trim().to_string());
            idx += 1;
        } else {
            break;
        }
    }

    while idx < lines.len() {
        let (ln, ind, text) = &lines[idx];
        if *ind != 0 {
            return err(*ln, format!("esperaba quest/block en nivel raíz, encontré: {text}"));
        }
        if let Some(rest) = text.strip_prefix("block ") {
            let (block, next) = parse_block(&lines, idx, rest)?;
            file.blocks.push(block);
            idx = next;
        } else if text.starts_with("quest ") {
            let (quest, next) = parse_quest(&lines, idx)?;
            file.quests.push(quest);
            idx = next;
        } else {
            return err(*ln, format!("construcción desconocida: {text}"));
        }
    }
    Ok(file)
}

fn parse_block(lines: &[Line], idx: usize, rest: &str) -> R<(Block, usize)> {
    let (name, params) = parse_name_params(rest)?;
    let mut block = Block { name, params, body: Vec::new() };
    let mut i = idx + 1;
    while i < lines.len() && lines[i].1 > 0 {
        let (sln, _ind, stext) = &lines[i];
        block.body.push(parse_stmt(*sln, stext)?);
        i += 1;
    }
    Ok((block, i))
}

fn parse_quest(lines: &[Line], idx: usize) -> R<(Quest, usize)> {
    let (_, _, text) = &lines[idx];
    let rest = text.strip_prefix("quest ").unwrap();

    // Instance: `quest X = base(args)`
    if let Some(eq) = rest.find(" = ") {
        let name = rest[..eq].trim();
        let base_rest = rest[eq + 3..].trim();
        let (base, args) = parse_name_args(base_rest)?;
        return Ok((Quest::Instance(InstanceDef { name: name.to_string(), base, args }), idx + 1));
    }

    // Family: `quest X family (params)`
    if let Some(fpos) = rest.find(" family ") {
        let name = rest[..fpos].trim();
        let params_rest = rest[fpos + 8..].trim();
        let (_, params) = parse_name_params(params_rest)?;
        let (states, next) = parse_states(lines, idx + 1)?;
        return Ok((Quest::Family { name: name.to_string(), params, states }, next));
    }

    // Concrete: `quest X` + states.
    let name = rest.trim();
    let (states, next) = parse_states(lines, idx + 1)?;
    Ok((Quest::Concrete(QuestDef { name: name.to_string(), states }), next))
}

fn parse_states(lines: &[Line], start: usize) -> R<(Vec<State>, usize)> {
    let mut states = Vec::new();
    let mut i = start;
    while i < lines.len() && lines[i].1 > 0 {
        let (ln, ind, text) = &lines[i];
        if *ind != 2 {
            return err(*ln, format!("se esperaba `state` con indentación 2, encontré: {text}"));
        }
        let Some(rest) = text.strip_prefix("state ") else {
            return err(*ln, format!("se esperaba `state`, encontré: {text}"));
        };
        let name = rest.trim();
        let mut state = State { name: name.to_string(), events: Vec::new() };
        i += 1;
        while i < lines.len() && lines[i].1 > 2 {
            let (eln, eind, etext) = &lines[i];
            if *eind != 4 {
                return err(*eln, format!("se esperaba `on` con indentación 4, encontré: {etext}"));
            }
            let Some(orest) = etext.strip_prefix("on ") else {
                return err(*eln, format!("se esperaba `on`, encontré: {etext}"));
            };
            let (triggers, cond, body, next) = parse_event(lines, i, orest)?;
            state.events.push(Event { triggers, condition: cond, body });
            i = next;
        }
        states.push(state);
    }
    Ok((states, i))
}

/// Línea del archivo: (número, indentación, texto sin comentarios).
type Line = (usize, usize, String);

/// Resultado de un evento parseado: (triggers, condición, cuerpo, próximo índice).
type ParsedEvent = (Vec<Trigger>, Option<Expr>, Vec<Stmt>, usize);

fn parse_event(
    lines: &[Line],
    idx: usize,
    rest: &str,
) -> R<ParsedEvent> {
    let (ln, _, _) = &lines[idx];
    // `on t1, t2 [with expr]`
    let mut triggers = Vec::new();
    let mut cond = None;
    let mut head = rest.trim().to_string();
    if let Some(w) = head.find(" with ") {
        let cexpr = head[w + 6..].trim().to_string();
        head = head[..w].trim().to_string();
        cond = Some(parse_expr(&cexpr).map_err(|m| ParseError { line: *ln, col: 1, msg: m })?);
    }
    for t in head.split(',') {
        triggers.push(parse_trigger(t.trim()).map_err(|m| ParseError { line: *ln, ..m })?);
    }
    let mut body = Vec::new();
    let mut i = idx + 1;
    while i < lines.len() && lines[i].1 > 4 {
        let (sln, sind, stext) = &lines[i];
        // Branches (spec §10): `if <expr>` / `else` at statement indent; the
        // branch BODY is the following lines with MORE indentation (1 level).
        if stext.starts_with("if ") || stext == "else" {
            let condition = match stext.strip_prefix("if ") {
                Some(c) => {
                    Some(parse_expr(c.trim()).map_err(|m| ParseError { line: *sln, col: 1, msg: m })?)
                }
                None => None,
            };
            let mut bbody = Vec::new();
            i += 1;
            while i < lines.len() && lines[i].1 > *sind {
                let (bsln, _bind, bstext) = &lines[i];
                bbody.push(parse_stmt(*bsln, bstext)?);
                i += 1;
            }
            body.push(Stmt::Branch(Branch { condition, body: bbody }));
        } else {
            body.push(parse_stmt(*sln, stext)?);
            i += 1;
        }
    }
    Ok((triggers, cond, body, i))
}

fn parse_stmt(line: usize, text: &str) -> R<Stmt> {
    if let Some(rest) = text.strip_prefix("-> ") {
        let (action, capture) = parse_action(rest)?;
        Ok(Stmt::Action { action, capture })
    } else if let Some(rest) = text.strip_prefix("use ") {
        let (name, args) = parse_use_args(rest)?;
        Ok(Stmt::Use { name, args })
    } else if let Some(rest) = text.strip_prefix("if ") {
        let cond = parse_expr(rest).map_err(|m| ParseError { line, col: 1, msg: m })?;
        Ok(Stmt::Branch(Branch { condition: Some(cond), body: Vec::new() }))
    } else if text == "else" {
        Ok(Stmt::Branch(Branch { condition: None, body: Vec::new() }))
    } else {
        err(line, format!("sentencia desconocida: {text}"))
    }
}

/// `action(args) [as capture]`.
fn parse_action(text: &str) -> R<(Action, Option<String>)> {
    let (name, args, capture) = parse_call(text)?;
    let aname = match name.as_str() {
        "say_title" => ActionName::SayTitle,
        "say" => ActionName::Say,
        "say_reward" => ActionName::SayReward,
        "say_item_vnum" => ActionName::SayItemVnum,
        "send_letter" => ActionName::SendLetter,
        "clear_letter" => ActionName::ClearLetter,
        "wait" => ActionName::Wait,
        "set_state" => ActionName::SetState,
        "set_quest_state" => ActionName::SetQuestState,
        "set_qf" => ActionName::SetQf,
        "give_item2" => ActionName::GiveItem2,
        "remove_item" => ActionName::RemoveItem,
        "target_vid" => ActionName::TargetVid,
        "target_delete" => ActionName::TargetDelete,
        "warp" => ActionName::Warp,
        "notice" => ActionName::Notice,
        "notice_multiline" => ActionName::NoticeMultiline,
        "affect_add" => ActionName::AffectAdd,
        "affect_remove" => ActionName::AffectRemove,
        "select" => ActionName::Select,
        "input_number" => ActionName::InputNumber,
        "return" => ActionName::Return,
        other => return err(1, format!("acción desconocida: {other}")),
    };
    Ok((Action { name: aname, args }, capture))
}

/// `name(args) [as capture]` — splits and parses the arg list.
fn parse_call(text: &str) -> R<(String, Vec<Value>, Option<String>)> {
    let text = text.trim();
    let (call, capture) = match text.find(" as ") {
        Some(pos) => (text[..pos].trim(), Some(text[pos + 4..].trim().to_string())),
        None => (text, None),
    };
    let Some(open) = call.find('(') else {
        // Action without parens: `-> wait` → `wait()`.
        if call.ends_with(')') {
            return err(1, "paréntesis sin abrir");
        }
        return Ok((call.to_string(), Vec::new(), capture));
    };
    if !call.ends_with(')') {
        return err(1, format!("acción sin `)` de cierre: {call}"));
    }
    let name = call[..open].trim().to_string();
    let args_text = call[open + 1..call.len() - 1].trim();
    let args = if args_text.is_empty() {
        Vec::new()
    } else {
        split_args(args_text)?
            .into_iter()
            .map(|a| parse_value(a.trim()))
            .collect::<R<Vec<_>>>()?
    };
    Ok((name, args, capture))
}

/// Split a comma-separated arg list respecting parentheses and quotes.
fn split_args(text: &str) -> R<Vec<String>> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut in_str = false;
    for c in text.chars() {
        match c {
            '"' => {
                in_str = !in_str;
                cur.push(c);
            }
            '(' if !in_str => {
                depth += 1;
                cur.push(c);
            }
            ')' if !in_str => {
                depth -= 1;
                if depth < 0 {
                    return err(1, "paréntesis sin abrir en args");
                }
                cur.push(c);
            }
            ',' if !in_str && depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if in_str || depth != 0 {
        return err(1, "args sin cerrar (comillas o paréntesis)");
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

/// Value: number, `@key`, `(param)`, quoted string, bare word (state name)
/// or a full expression — `set_qf(duration, get_time() + 60 * 60 * 22)` and
/// `affect_add(apply.MOV_SPEED, 10, 60 * 60 * 24 * 365 * 60)` are evaluated
/// by the runtime (Value::Expr). The typed catalog validates actions, not
/// literal values.
fn parse_value(text: &str) -> R<Value> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('@') {
        return Ok(Value::Key(rest.to_string()));
    }
    if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
        return Ok(Value::Str(text[1..text.len() - 1].to_string()));
    }
    if let Ok(n) = text.parse::<i64>() {
        return Ok(Value::Num(n));
    }
    // `(name)` — family parameter reference (spec §6). Only a plain word
    // qualifies; `(get_time() + 5)` is a parenthesized expression.
    if text.starts_with('(') && text.ends_with(')') && text.len() >= 3 {
        let inner = &text[1..text.len() - 1];
        if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Ok(Value::Param(inner.to_string()));
        }
    }
    // Full expression args: literals come back as literals (`5` → Num),
    // anything else becomes Value::Expr.
    if text.contains(|c: char| "()+-*/<>=~' ".contains(c)) {
        let e = parse_expr(text).map_err(|m| ParseError { line: 1, col: 1, msg: m })?;
        return Ok(match e {
            Expr::Value(v) => v,
            other => Value::Expr(Box::new(other)),
        });
    }
    // Bare word: state names (`set_state(information)`, spec §2) and block
    // args (`use reward_sequence(..., go)`). Zero-arg condition functions
    // (`get_time`, `pc.level`) stay expressions (they roundtrip via render).
    if let Some(f) = bare_func(text) {
        return Ok(Value::Expr(Box::new(Expr::Func(f, Vec::new()))));
    }
    if !text.is_empty() {
        return Ok(Value::Str(text.to_string()));
    }
    err(1, "valor vacío")
}

/// `name` or `name(params)` where params are `name: type`.
fn split_head(text: &str) -> R<(String, String)> {
    match text.find('(') {
        Some(open) => {
            let name = text[..open].trim();
            let rest = text[open..].trim();
            if !rest.ends_with(')') {
                return err(1, format!("params sin `)`: {text}"));
            }
            Ok((name.to_string(), rest[1..rest.len() - 1].to_string()))
        }
        None => Ok((text.trim().to_string(), String::new())),
    }
}

/// `use name(args)` — args are plain values (spec §7).
fn parse_use_args(text: &str) -> R<(String, Vec<Value>)> {
    let (name, args_text) = split_head(text)?;
    let mut args = Vec::new();
    for a in split_args(&args_text)? {
        args.push(parse_value(a.trim())?);
    }
    Ok((name, args))
}

fn parse_name_params(text: &str) -> R<(String, Vec<Param>)> {
    let (name, args_text) = split_head(text)?;
    let mut params = Vec::new();
    for p in split_args(&args_text)? {
        let p = p.trim();
        match p.find(':') {
            Some(colon) => {
                let pname = p[..colon].trim();
                let ty = match p[colon + 1..].trim() {
                    "vnum" => ParamType::Vnum,
                    "level" => ParamType::Level,
                    "key" => ParamType::Key,
                    "str" => ParamType::Str,
                    other => return err(1, format!("tipo de param desconocido: {other}")),
                };
                params.push(Param { name: pname.to_string(), ty: Some(ty) });
            }
            None => params.push(Param { name: p.to_string(), ty: None }),
        }
    }
    Ok((name, params))
}

/// `name(args)` — instance args `(param: value, ...)`.
fn parse_name_args(text: &str) -> R<(String, Vec<(String, Value)>)> {
    let (name, args_text) = split_head(text)?;
    let mut args = Vec::new();
    for p in split_args(&args_text)? {
        let p = p.trim();
        let Some(colon) = p.find(':') else {
            return err(1, format!("arg de instancia sin `: valor`: {p}"));
        };
        let aname = p[..colon].trim().to_string();
        let value = parse_value(p[colon + 1..].trim())?;
        args.push((aname, value));
    }
    Ok((name, args))
}

/// Trigger catalog (spec §3).
fn parse_trigger(text: &str) -> R<Trigger> {
    let text = text.trim();
    let kind = match text {
        "login" => TriggerKind::Login,
        "levelup" => TriggerKind::LevelUp,
        "letter" => TriggerKind::Letter,
        "button" => TriggerKind::Button,
        "info" => TriggerKind::Info,
        "enter" => TriggerKind::Enter,
        "logout" => TriggerKind::Logout,
        "timer" => TriggerKind::Timer, // alias (decision §11.6)
        "__TARGET__.target.click" => TriggerKind::TargetClick,
        _ => {
            if let Some(vnum) = text.strip_suffix(".chat") {
                return Ok(Trigger { kind: TriggerKind::Chat { target: parse_target(vnum)? } });
            }
            if let Some(vnum) = text.strip_suffix(".kill") {
                return Ok(Trigger { kind: TriggerKind::Kill { target: parse_target(vnum)? } });
            }
            if let Some(vnum) = text.strip_suffix(".use") {
                return Ok(Trigger { kind: TriggerKind::Use { target: parse_target(vnum)? } });
            }
            // `arena.*`, `oxevent.*`, `d.*`, `wedding.*` → Rust modules.
            if text.contains('.') && text.ends_with(".*") {
                return Ok(Trigger { kind: TriggerKind::Rust(text.to_string()) });
            }
            return err(1, format!("trigger desconocido: {text}"));
        }
    };
    Ok(Trigger { kind })
}

/// `<vnum>` fijo o `(param)` de familia (spec §6).
fn parse_target(text: &str) -> R<TriggerTarget> {
    let text = text.trim();
    if let Ok(v) = text.parse::<u32>() {
        return Ok(TriggerTarget::Num(v));
    }
    if text.starts_with('(') && text.ends_with(')') && text.len() >= 3 {
        return Ok(TriggerTarget::Param(text[1..text.len() - 1].to_string()));
    }
    err(1, format!("target de trigger inválido: {text}"))
}

/// Mini expression parser (spec §4): `pc.level >= 30`, `count_item(30006) > 0`,
/// `number(1, 100) <= 5`, `pc.level between 15, 39` (decision §11.1).
///
/// Grammar (precedence): or → and → comparison → between → add/sub → mul/div
/// → primary (number, string, `@key`, `(param)`, func call, `not expr`).
pub fn parse_expr(text: &str) -> Result<Expr, String> {
    let mut p = ExprParser { toks: tokenize(text)?, pos: 0 };
    let e = p.parse_or()?;
    if p.pos != p.toks.len() {
        return Err(format!("expresión malformada cerca de: {:?}", &p.toks[p.pos..]));
    }
    Ok(e)
}

fn tokenize(text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                // String literal: consume hasta la comilla de cierre.
                cur.push(c);
                for c2 in chars.by_ref() {
                    cur.push(c2);
                    if c2 == '"' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            '(' | ')' | ',' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push(c.to_string());
            }
            c if c.is_ascii_digit() || c.is_ascii_alphabetic() || c == '_' || c == '@' || c == '.' => {
                cur.push(c);
            }
            _ => {
                // Operadores: == != <= >= < > + - * /
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                let mut op = c.to_string();
                if matches!(c, '=' | '!' | '<' | '>') && chars.peek() == Some(&'=') {
                    op.push('=');
                    chars.next();
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

struct ExprParser {
    toks: Vec<String>,
    pos: usize,
}

impl ExprParser {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.pos).map(|s| s.as_str())
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
        let lhs = self.parse_between()?;
        match self.peek() {
            Some("==") => {
                self.pos += 1;
                Ok(Expr::Compare(Box::new(lhs), CmpOp::Eq, Box::new(self.parse_between()?)))
            }
            Some("!=") => {
                self.pos += 1;
                Ok(Expr::Compare(Box::new(lhs), CmpOp::Ne, Box::new(self.parse_between()?)))
            }
            Some("<") => {
                self.pos += 1;
                Ok(Expr::Compare(Box::new(lhs), CmpOp::Lt, Box::new(self.parse_between()?)))
            }
            Some(">") => {
                self.pos += 1;
                Ok(Expr::Compare(Box::new(lhs), CmpOp::Gt, Box::new(self.parse_between()?)))
            }
            Some("<=") => {
                self.pos += 1;
                Ok(Expr::Compare(Box::new(lhs), CmpOp::Le, Box::new(self.parse_between()?)))
            }
            Some(">=") => {
                self.pos += 1;
                Ok(Expr::Compare(Box::new(lhs), CmpOp::Ge, Box::new(self.parse_between()?)))
            }
            _ => Ok(lhs),
        }
    }

    fn parse_between(&mut self) -> Result<Expr, String> {
        let lhs = self.parse_add()?;
        if self.eat("between") {
            let lo = self.parse_add()?;
            if !self.eat(",") {
                return Err("between requiere `a between b, c`".into());
            }
            let hi = self.parse_add()?;
            return Ok(Expr::Between(Box::new(lhs), Box::new(lo), Box::new(hi)));
        }
        Ok(lhs)
    }

    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_mul()?;
        loop {
            if self.eat("+") {
                lhs = Expr::Add(Box::new(lhs), Box::new(self.parse_mul()?));
            } else if self.eat("-") {
                lhs = Expr::Sub(Box::new(lhs), Box::new(self.parse_mul()?));
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
        // Function call: `name(...)`.
        if self.toks.get(self.pos + 1).map(|s| s == "(").unwrap_or(false) {
            self.pos += 2; // name + (
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
            let fname = match t.as_str() {
                "pc.level" => FuncName::PcLevel,
                "count_item" => FuncName::CountItem,
                "get_qf" => FuncName::GetQf,
                "number" => FuncName::Number,
                "get_time" => FuncName::GetTime,
                "get_map_index" => FuncName::GetMapIndex,
                "get_gm_level" => FuncName::GetGmLevel,
                "pet.is_summon" => FuncName::PetIsSummon,
                "is_test_server" => FuncName::IsTestServer,
                other => return Err(format!("función desconocida: {other}")),
            };
            return Ok(Expr::Func(fname, args));
        }
        self.pos += 1;
        if let Ok(n) = t.parse::<i64>() {
            Ok(Expr::Value(Value::Num(n)))
        } else if let Some(key) = t.strip_prefix('@') {
            Ok(Expr::Value(Value::Key(key.to_string())))
        } else if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
            Ok(Expr::Value(Value::Str(t[1..t.len() - 1].to_string())))
        } else if let Some(f) = bare_func(&t) {
            Ok(Expr::Func(f, Vec::new()))
        } else if t == "(" {
            // `(param)` — reference a family parameter (spec §6): the lexer
            // splits `(` and `)` as separate tokens, so the pattern is
            // `(`, name, `)`.
            let name = self.toks.get(self.pos).cloned();
            let close = self.toks.get(self.pos + 1).map(|s| s == ")").unwrap_or(false);
            if let (Some(name), true) = (name, close) {
                self.pos += 2;
                Ok(Expr::Value(Value::Param(name)))
            } else {
                Err("`(` sin cierre de param".into())
            }
        } else {
            // Bare identifier: a capture from `as <name>` (spec §10).
            Ok(Expr::Capture(t))
        }
    }
}

/// Funciones invocables SIN paréntesis (`pc.level`, `get_time()` con 0 args
/// se puede escribir desnuda — spec §4).
fn bare_func(t: &str) -> Option<FuncName> {
    match t {
        "pc.level" => Some(FuncName::PcLevel),
        "get_time" => Some(FuncName::GetTime),
        "get_map_index" => Some(FuncName::GetMapIndex),
        "get_gm_level" => Some(FuncName::GetGmLevel),
        "is_test_server" => Some(FuncName::IsTestServer),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(text: &str) -> QuestFile {
        parse(text).unwrap_or_else(|e| panic!("parse error: {e}\n{text}"))
    }

    #[test]
    fn parses_concrete_quest() {
        let f = parse_ok(
            "# quests/biology/collect_quest_lv30.quest\nquest collect_quest_lv30\n  state start\n    on login, levelup with pc.level >= 30\n      -> set_state(information)\n  state information\n    on 601.kill with number(1, 100) <= 5\n      -> give_item2(30006, 1)\n",
        );
        assert_eq!(f.quests.len(), 1);
        let Quest::Concrete(q) = &f.quests[0] else { panic!("esperaba concrete") };
        assert_eq!(q.name, "collect_quest_lv30");
        assert_eq!(q.states.len(), 2);
        let e = &q.states[0].events[0];
        assert_eq!(e.triggers.len(), 2);
        assert!(matches!(e.triggers[0].kind, TriggerKind::Login));
        assert!(matches!(e.triggers[1].kind, TriggerKind::LevelUp));
        let Stmt::Action { action, .. } = &e.body[0] else { panic!("esperaba action") };
        assert!(matches!(action.name, ActionName::SetState));
        assert_eq!(action.args, vec![Value::Str("information".into())]);
    }

    #[test]
    fn parses_kill_trigger_with_vnum() {
        let f = parse_ok(
            "quest q\n  state start\n    on 601.kill\n      -> wait()\n",
        );
        let Quest::Concrete(q) = &f.quests[0] else { panic!() };
        let e = &q.states[0].events[0];
        assert!(matches!(e.triggers[0].kind, TriggerKind::Kill { target: TriggerTarget::Num(601) }));
    }

    #[test]
    fn rejects_unknown_action() {
        let e = parse("quest q\n  state start\n    on login\n      -> fly_to_the_moon(1)\n");
        assert!(e.is_err());
        assert!(e.unwrap_err().msg.contains("acción desconocida"));
    }

    #[test]
    fn rejects_bad_indentation() {
        let e = parse("quest q\n  state start\n    on login\n  -> wait()\n");
        assert!(e.is_err());
    }

    #[test]
    fn parses_import_and_blocks() {
        let f = parse_ok(
            "import helpers\nblock reward_sequence(title, text, next_state)\n  -> say_title((title))\n  -> set_state((next_state))\nquest q\n  state start\n    on letter\n      use reward_sequence(@a, @b, go)\n",
        );
        assert_eq!(f.imports, vec!["helpers"]);
        assert_eq!(f.blocks.len(), 1);
        assert_eq!(f.blocks[0].name, "reward_sequence");
        assert_eq!(f.blocks[0].params.len(), 3);
        let Quest::Concrete(q) = &f.quests[0] else { panic!() };
        let Stmt::Use { name, args } = &q.states[0].events[0].body[0] else { panic!() };
        assert_eq!(name, "reward_sequence");
        assert_eq!(
            args.as_slice(),
            &[Value::Key("a".into()), Value::Key("b".into()), Value::Str("go".into())]
        );
    }

    #[test]
    fn parses_select_capture_and_if_else() {
        let f = parse_ok(
            "quest q\n  state start\n    on 20011.chat\n      -> select(@_20_say, @_30_say) as choice\n      if choice == 1\n        -> warp(896500, 24600)\n      else\n        -> return\n",
        );
        let Quest::Concrete(q) = &f.quests[0] else { panic!() };
        let body = &q.states[0].events[0].body;
        assert_eq!(body.len(), 3);
        let Stmt::Action { capture, .. } = &body[0] else { panic!() };
        assert_eq!(capture.as_deref(), Some("choice"));
        let Stmt::Branch(b) = &body[1] else { panic!() };
        assert!(b.condition.is_some());
        assert_eq!(b.body.len(), 1);
        let Stmt::Branch(els) = &body[2] else { panic!() };
        assert!(els.condition.is_none());
        assert_eq!(els.body.len(), 1);
    }

    #[test]
    fn parses_between_native() {
        let e = parse_expr("pc.level between 15, 39").expect("between");
        assert!(matches!(e, Expr::Between(_, _, _)));
        let e = parse_expr("get_qf(duration) != 0 and pc.level >= 30").expect("expr");
        assert!(matches!(e, Expr::And(_, _)));
        let e = parse_expr("number(1, 100) <= 5").expect("number");
        assert!(matches!(e, Expr::Compare(_, CmpOp::Le, _)));
        let e = parse_expr("get_time() + 60 * 60 * 22").expect("arith");
        assert!(matches!(e, Expr::Add(_, _)));
        let e = parse_expr("not is_test_server()").expect("not");
        assert!(matches!(e, Expr::Not(_)));
    }

    #[test]
    fn comments_and_empty_lines_ignored() {
        let f = parse_ok("# solo comentario\n\nquest q\n  state start\n    on login\n      -> wait()\n");
        assert_eq!(f.quests.len(), 1);
    }

    #[test]
    fn parses_expr_args() {
        let f = parse_ok(
            "quest q\n  state start\n    on login\n      -> set_qf(duration, get_time() + 60 * 60 * 22)\n      -> affect_add(apply.MOV_SPEED, 10, 60*60*24*365*60)\n",
        );
        let Quest::Concrete(q) = &f.quests[0] else { panic!() };
        let body = &q.states[0].events[0].body;
        let Stmt::Action { action, .. } = &body[0] else { panic!() };
        assert_eq!(action.name, ActionName::SetQf);
        assert_eq!(action.args[0], Value::Str("duration".into()));
        assert!(matches!(&action.args[1], Value::Expr(e) if matches!(e.as_ref(), Expr::Add(_, _))), "{:?}", action.args);
        let Stmt::Action { action, .. } = &body[1] else { panic!() };
        assert_eq!(action.name, ActionName::AffectAdd);
        assert_eq!(action.args[0], Value::Str("apply.MOV_SPEED".into()));
        assert_eq!(action.args[1], Value::Num(10));
        assert!(matches!(&action.args[2], Value::Expr(_)), "{:?}", action.args);
    }

    #[test]
    fn parses_bare_func_arg() {
        // `get_time` bare (the render form) still parses as an expression.
        let f = parse_ok("quest q\n  state start\n    on login\n      -> set_qf(x, get_time)\n");
        let Quest::Concrete(q) = &f.quests[0] else { panic!() };
        let Stmt::Action { action, .. } = &q.states[0].events[0].body[0] else { panic!() };
        assert!(
            matches!(&action.args[1], Value::Expr(e) if matches!(e.as_ref(), Expr::Func(FuncName::GetTime, a) if a.is_empty())),
            "{:?}",
            action.args
        );
    }

    #[test]
    fn rejects_garbage_expr_arg() {
        // Operator-containing text must be a VALID expression, not a Str.
        let err = parse("quest q\n  state start\n    on login\n      -> set_qf(x, 60 * * 22)\n").unwrap_err();
        assert!(err.msg.contains("expresión"), "{err}");
    }
}
