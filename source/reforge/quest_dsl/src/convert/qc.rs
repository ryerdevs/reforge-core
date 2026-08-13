//! Legacy qc parser — the Metin2 quest-compiler format (`.quest` files).
//!
//! ## The real grammar (verified against the deployed corpus, 2026-08-13)
//!
//! qc is a Lua 5.0 dialect (EUC-KR lexer patch, `!=` accepted, `--` comments)
//! with `begin`/`end` blocks instead of indentation:
//!
//! ```text
//! define <NAME> <value>                          -- preprocessor define (ignored)
//! quest <name> begin
//!     state <name> begin
//!         when <trigger>[ or <trigger>...][ with <expr>] begin
//!             <lua statements>
//!         end
//!     end
//! end
//! ```
//!
//! Triggers seen in the corpus (`biology/collect_quest_lv30.quest`,
//! `systems/check_collect_reward.quest`, `story/levelup.quest`,
//! `systems/give_basic_weapon.quest`):
//!
//! | Trigger | Example |
//! |---|---|
//! | Bare events | `login`, `levelup`, `letter`, `button`, `info`, `kill`, `enter`, `logout`, `timer` |
//! | Qualified | `601.kill`, `71035.use`, `631.kill or 632.kill or ...` |
//! | NPC chat | `20084.chat.gameforge.collect_quest_lv30._30_npcChat` (vnum + npcChat locale key) |
//! | Target | `__TARGET__.target.click` |
//!
//! Statements are Lua: `local x = expr`, `x = expr`, `if/elseif/else/end`
//! (also inline `if c then stmt end`), `for ... do ... end`, `function ... end`
//! (state-level helpers, called as `<quest>.<name>(...)`), `return [expr]`
//! and bare calls `name(args)`. Strings are double- OR single-quoted.
//! Indentation is NOT significant (the corpus mixes tabs and spaces).
//!
//! ## Pragmatics
//!
//! The parser is line-based and keyword-driven: `begin`/`end` nesting is
//! tracked structurally (quest → state → when), and if/for/function bodies
//! nest by their own `end`. Anything unrecognized is kept as
//! `LegacyStmt::Raw` / `unparsed` — the parser never drops data and never
//! fails on unknown Lua (the converter reports it).

/// A parsed legacy quest file.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyQuest {
    pub name: String,
    pub states: Vec<LegacyState>,
    /// Unrecognized lines at quest level (reported, not fatal).
    pub unparsed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegacyState {
    pub name: String,
    pub events: Vec<LegacyEvent>,
    /// State-level helper functions (`function name(...) ... end`) — called
    /// from events as `<quest>.<name>(...)`; no DSL equivalent (Rust module).
    pub helpers: Vec<LegacyFunc>,
    /// Unrecognized lines at state level.
    pub unparsed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegacyEvent {
    /// Raw trigger strings, e.g. `20084.chat.gameforge.x._30_npcChat`.
    pub triggers: Vec<String>,
    /// Raw condition text after `with` (if any).
    pub condition: Option<String>,
    pub body: Vec<LegacyStmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegacyFunc {
    pub name: String,
    pub body: Vec<LegacyStmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LegacyStmt {
    /// `name(args)` — args as raw text (commas split at depth 0).
    Call { name: String, args: Vec<String> },
    /// `local x = expr` or `local x`.
    Local { name: String, value: Option<String> },
    /// `x = expr`.
    Assign { name: String, value: String },
    /// `return [expr]`.
    Return { value: Option<String> },
    /// `if <cond> then ... [elseif ...] [else ...] end` — elseif as
    /// `(cond, body)` pairs.
    If {
        cond: String,
        then: Vec<LegacyStmt>,
        elseif_: Vec<(String, Vec<LegacyStmt>)>,
        els: Vec<LegacyStmt>,
    },
    /// `for <head> do ... end`.
    For { head: String, body: Vec<LegacyStmt> },
    /// `while <head> do ... end` — tracked for correct `end` nesting
    /// (reported as unmapped by the converter).
    While { head: String, body: Vec<LegacyStmt> },
    /// `repeat ... until <cond>` — closed by `until`, no `end`.
    Repeat { body: Vec<LegacyStmt>, until: String },
    /// Any unrecognized line (kept for the discrepancy report).
    Raw(String),
}

/// Parses a legacy `.quest` file. Errors only on structural problems
/// (missing `quest`/`begin`/`end`) — unknown Lua is kept in the AST.
/// Errors carry the source line: `línea N: ...`.
pub fn parse_qc(text: &str) -> Result<LegacyQuest, String> {
    let text = text.trim_start_matches('\u{feff}');
    let lines = strip_comments(text);
    let mut i = 0;
    let mut pre = Vec::new(); // leading non-quest lines (prose samples, notes)

    // Top level: `define` lines are skipped; `quest <name> begin` opens.
    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() {
            i += 1;
            continue;
        }
        if t.starts_with("define ") {
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("quest ") {
            let name = rest
                .trim_end()
                .strip_suffix("begin")
                .ok_or_else(|| format!("línea {}: quest malformado (falta `begin`): {t}", i + 1))?
                .trim();
            if name.is_empty() {
                return Err(format!("línea {}: quest sin nombre: {t}", i + 1));
            }
            i += 1;
            let mut quest = LegacyQuest { name: name.to_string(), states: Vec::new(), unparsed: pre };
            while i < lines.len() {
                let t = lines[i].trim();
                if t.is_empty() {
                    i += 1;
                    continue;
                }
                if t == "end" {
                    return Ok(quest);
                }
                if let Some(rest) = t.strip_prefix("state ") {
                    let stname = rest
                        .trim_end()
                        .strip_suffix("begin")
                        .ok_or_else(|| format!("línea {}: state sin `begin`: {t}", i + 1))?
                        .trim();
                    i += 1;
                    let (state, next) = parse_state_body(&lines, i, stname)?;
                    quest.states.push(state);
                    i = next;
                } else {
                    quest.unparsed.push(t.to_string());
                    i += 1;
                }
            }
            return Err(format!("línea {}: quest sin `end` de cierre", i + 1));
        }
        // Prose/notes before the quest header (samples/*.quest) — kept and
        // reported, not fatal.
        pre.push(t.to_string());
        i += 1;
    }
    Err("archivo sin quest".into())
}

fn parse_state_body(lines: &[String], mut i: usize, name: &str) -> Result<(LegacyState, usize), String> {
    let mut state = LegacyState { name: name.to_string(), events: Vec::new(), helpers: Vec::new(), unparsed: Vec::new() };
    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() {
            i += 1;
            continue;
        }
        if t == "end" {
            return Ok((state, i + 1));
        }
        if let Some(rest) = t.strip_prefix("when ") {
            let (event, next) = parse_when(lines, i, rest)?;
            state.events.push(event);
            i = next;
        } else if let Some(rest) = t.strip_prefix("function ") {
            let (fname, body, next) = parse_func(lines, i, rest)?;
            state.helpers.push(LegacyFunc { name: fname, body });
            i = next;
        } else {
            state.unparsed.push(t.to_string());
            i += 1;
        }
    }
    Err(format!("state {name} sin `end` de cierre"))
}

/// Position of the `begin` keyword as a standalone word (space/tab/start
/// before, space/tab/end after).
fn find_begin(text: &str) -> Option<usize> {
    let b = text.as_bytes();
    let mut search = 0;
    while let Some(rel) = text[search..].find("begin") {
        let p = search + rel;
        let before_ok = p == 0 || b[p - 1].is_ascii_whitespace();
        let after_ok = p + 5 >= b.len() || b[p + 5].is_ascii_whitespace();
        if before_ok && after_ok {
            return Some(p);
        }
        search = p + 5;
    }
    None
}

fn parse_when(lines: &[String], mut i: usize, first_rest: &str) -> Result<(LegacyEvent, usize), String> {
    // The when head may span lines (`... or` / `... with` / `begin` on the
    // next line) and may carry an INLINE body: `when 20119.click begin
    // horse_menu.horse_menu() end` (horse_menu.quest).
    let mut rest = first_rest.trim().to_string();
    while find_begin(&rest).is_none() {
        i += 1;
        if i >= lines.len() {
            return Err(format!("línea {}: when sin `begin`: {rest}", i + 1));
        }
        rest.push(' ');
        rest.push_str(lines[i].trim());
    }
    let p = find_begin(&rest).expect("find_begin confirmó");
    let head = rest[..p].trim();
    let inline = rest[p + 5..].trim();

    // `when <triggers>[ with <cond>]` — `with` is whitespace-delimited so the
    // condition can contain `or`/`and` without confusing the trigger split.
    let (trig_head, condition) = match head.find(" with ") {
        Some(w) => (head[..w].trim(), Some(head[w + 6..].trim().to_string())),
        None => (head, None),
    };
    let triggers: Vec<String> = trig_head.split_whitespace().filter(|w| *w != "or").map(str::to_string).collect();
    if triggers.is_empty() {
        return Err(format!("línea {}: when sin triggers: {head}", i + 1));
    }
    let mut body = Vec::new();
    if !inline.is_empty() {
        // Inline body on the when line: `when X begin stmt() end`.
        let inl = inline
            .strip_suffix(" end")
            .ok_or_else(|| format!("línea {}: when inline sin `end`: {inline}", i + 1))?;
        parse_inline(inl, &mut body)?;
        return Ok((LegacyEvent { triggers, condition, body }, i + 1));
    }
    i += 1;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() {
            i += 1;
            continue;
        }
        if t == "end" {
            return Ok((LegacyEvent { triggers, condition, body }, i + 1));
        }
        let (stmt, next) = parse_stmt_line(lines, i, t)?;
        body.push(stmt);
        i = next;
    }
    Err(format!("línea {}: when sin `end` de cierre", i + 1))
}

/// Dispatches one statement line; multi-line constructs consume their
/// `end` and return the next index.
fn parse_stmt_line(lines: &[String], i: usize, t: &str) -> Result<(LegacyStmt, usize), String> {
    if let Some(rest) = t.strip_prefix("if ") {
        return parse_if(lines, i, rest);
    }
    // `if(...)` without space (corpus variant).
    if t.starts_with("if(") {
        return parse_if(lines, i, &t[2..]);
    }
    if let Some(rest) = t.strip_prefix("for ") {
        return parse_for(lines, i, rest);
    }
    if let Some(rest) = t.strip_prefix("while ") {
        return parse_while(lines, i, rest);
    }
    if t == "repeat" {
        return parse_repeat(lines, i);
    }
    if let Some(rest) = t.strip_prefix("function ") {
        let (name, _body, next) = parse_func(lines, i, rest)?;
        // Helpers are state-level; an anonymous function (`function (a, b)`)
        // inside a call is a Raw with its body consumed for correct nesting.
        return Ok((LegacyStmt::Raw(format!("function {name}")), next));
    }
    if t == "end" {
        return Err(format!("línea {}: end inesperado", i + 1));
    }
    if t == "else" || t.starts_with("elseif ") || t.starts_with("else if ") {
        return Err(format!("línea {}: {t} sin if", i + 1));
    }
    if let Some(rest) = t.strip_prefix("local ") {
        return Ok((parse_local(rest)?, i + 1));
    }
    if let Some(rest) = t.strip_prefix("return") {
        let v = rest.trim();
        let value = if v.is_empty() { None } else { Some(v.to_string()) };
        return Ok((LegacyStmt::Return { value }, i + 1));
    }
    if let Some(pos) = top_level_eq(t)
        && !t[pos..].starts_with("==")
    {
        let name = t[..pos].trim();
        let value = t[pos + 1..].trim();
        return Ok((LegacyStmt::Assign { name: name.to_string(), value: value.to_string() }, i + 1));
    }
    if let Some(open) = t.find('(')
        && t.ends_with(')')
        && !t[..open].trim().is_empty()
    {
        let name = t[..open].trim().to_string();
        let args = split_args(&t[open + 1..t.len() - 1]);
        return Ok((LegacyStmt::Call { name, args }, i + 1));
    }
    Ok((LegacyStmt::Raw(t.to_string()), i + 1))
}

fn parse_local(rest: &str) -> Result<LegacyStmt, String> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err("local sin nombre".into());
    }
    match top_level_eq(rest) {
        Some(pos) if !rest[pos..].starts_with("==") => Ok(LegacyStmt::Local {
            name: rest[..pos].trim().to_string(),
            value: Some(rest[pos + 1..].trim().to_string()),
        }),
        _ => Ok(LegacyStmt::Local { name: rest.to_string(), value: None }),
    }
}

fn parse_if(lines: &[String], mut i: usize, rest: &str) -> Result<(LegacyStmt, usize), String> {
    // The condition may span lines (levelup.quest: `if current == 0 and` +
    // continuations until the line carrying `then`).
    let mut text = rest.to_string();
    while find_then(&text).is_none() {
        i += 1;
        if i >= lines.len() {
            return Err(format!("línea {}: if sin `then`: {text}", i + 1));
        }
        text.push(' ');
        text.push_str(lines[i].trim());
    }
    let (cond, inline) = split_then(&text)?;
    let mut then = Vec::new();
    if let Some(inl) = inline {
        // One-line form `if c then stmt [end]`. Without the inline `end`
        // (horse_menu.quest: `if s == 1 then s = 0`), the terminator is on
        // the NEXT line — fall through to the elseif/else/end loop.
        let (inl, inline_end) = match inl.strip_suffix(" end") {
            Some(s) => (s, true),
            None => (inl.as_str(), false),
        };
        parse_inline(inl, &mut then)?;
        i += 1;
        if inline_end {
            return Ok((LegacyStmt::If { cond, then, elseif_: Vec::new(), els: Vec::new() }, i));
        }
    } else {
        i += 1;
        then = collect_body(lines, &mut i)?;
    }
    let mut elseif_ = Vec::new();
    let mut els = Vec::new();
    loop {
        if i >= lines.len() {
            return Err(format!("línea {}: if sin `end` de cierre", i + 1));
        }
        let t = lines[i].trim();
        if t == "end" {
            return Ok((LegacyStmt::If { cond, then, elseif_, els }, i + 1));
        }
        let elseif_rest = t.strip_prefix("elseif ").or_else(|| t.strip_prefix("else if "));
        if let Some(r) = elseif_rest {
            let mut etext = r.to_string();
            while find_then(&etext).is_none() {
                i += 1;
                if i >= lines.len() {
                    return Err(format!("línea {}: elseif sin `then`: {etext}", i + 1));
                }
                etext.push(' ');
                etext.push_str(lines[i].trim());
            }
            let (c2, inline) = split_then(&etext)?;
            let mut body = Vec::new();
            match inline {
                Some(inl) => {
                    let (inl, inline_end) = match inl.strip_suffix(" end") {
                        Some(s) => (s, true),
                        None => (inl.as_str(), false),
                    };
                    parse_inline(inl, &mut body)?;
                    i += 1;
                    if !inline_end {
                        body.extend(collect_body(lines, &mut i)?);
                    }
                }
                None => {
                    i += 1;
                    body = collect_body(lines, &mut i)?;
                }
            }
            elseif_.push((c2, body));
            continue;
        }
        if t == "else" {
            i += 1;
            els = collect_body(lines, &mut i)?;
            continue;
        }
        return Err(format!("línea {}: if malformado cerca de: {t}", i + 1));
    }
}

fn parse_for(lines: &[String], mut i: usize, rest: &str) -> Result<(LegacyStmt, usize), String> {
    // `for <head> do [<inline> end]`
    let (head, inline) = match rest.find(" do") {
        Some(p) => (rest[..p].trim().to_string(), rest[p + 3..].trim().to_string()),
        None => return Err(format!("línea {}: for sin `do`: {rest}", i + 1)),
    };
    let mut body = Vec::new();
    if inline.is_empty() {
        i += 1;
        body = collect_body(lines, &mut i)?;
        if i < lines.len() && lines[i].trim() == "end" {
            i += 1;
        } else {
            return Err(format!("línea {}: for sin `end`: {rest}", i + 1));
        }
    } else if let Some(inl) = inline.strip_suffix(" end") {
        parse_inline(inl, &mut body)?;
        i += 1;
    } else {
        // Inline without `end`: the terminator is on a following line.
        parse_inline(&inline, &mut body)?;
        i += 1;
        body.extend(collect_body(lines, &mut i)?);
        if i < lines.len() && lines[i].trim() == "end" {
            i += 1;
        } else {
            return Err(format!("línea {}: for sin `end`: {rest}", i + 1));
        }
    }
    Ok((LegacyStmt::For { head, body }, i))
}

fn parse_func(lines: &[String], i: usize, rest: &str) -> Result<(String, Vec<LegacyStmt>, usize), String> {
    // `function name(args)` or anonymous `function (args)` (a call argument:
    // `table.foreachi(list, function (n, v) ... end)`).
    let name = rest.split('(').next().unwrap_or("").trim().to_string();
    let label = if name.is_empty() { "(anonymous)".to_string() } else { name.clone() };
    let mut i = i + 1;
    let body = collect_body(lines, &mut i)?;
    if i < lines.len() && lines[i].trim() == "end" {
        i += 1;
    } else {
        return Err(format!("línea {}: function {label} sin `end`", i + 1));
    }
    Ok((label, body, i))
}

/// `while <head> do ... end` — same shape as `for` (the corpus nests while
/// loops inside ifs; the `end` must be tracked for correct nesting).
fn parse_while(lines: &[String], mut i: usize, rest: &str) -> Result<(LegacyStmt, usize), String> {
    let (head, inline) = match rest.find(" do") {
        Some(p) => (rest[..p].trim().to_string(), rest[p + 3..].trim().to_string()),
        None => return Err(format!("línea {}: while sin `do`: {rest}", i + 1)),
    };
    let mut body = Vec::new();
    if inline.is_empty() {
        i += 1;
        body = collect_body(lines, &mut i)?;
        if i < lines.len() && lines[i].trim() == "end" {
            i += 1;
        } else {
            return Err(format!("línea {}: while sin `end`: {rest}", i + 1));
        }
    } else if let Some(inl) = inline.strip_suffix(" end") {
        parse_inline(inl, &mut body)?;
        i += 1;
    } else {
        parse_inline(&inline, &mut body)?;
        i += 1;
        body.extend(collect_body(lines, &mut i)?);
        if i < lines.len() && lines[i].trim() == "end" {
            i += 1;
        } else {
            return Err(format!("línea {}: while sin `end`: {rest}", i + 1));
        }
    }
    Ok((LegacyStmt::While { head, body }, i))
}

/// `repeat ... until <cond>` — no `end`; the `until` line closes it.
fn parse_repeat(lines: &[String], mut i: usize) -> Result<(LegacyStmt, usize), String> {
    i += 1;
    let mut body = Vec::new();
    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() {
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("until") {
            return Ok((LegacyStmt::Repeat { body, until: rest.trim().to_string() }, i + 1));
        }
        let (s, next) = parse_stmt_line(lines, i, t)?;
        body.push(s);
        i = next;
    }
    Err(format!("línea {}: repeat sin `until`", i + 1))
}

/// Collects statement lines until a line that terminates a block
/// (`end`/`else`/`elseif` at any indentation — the corpus is not
/// indentation-significant).
fn collect_body(lines: &[String], i: &mut usize) -> Result<Vec<LegacyStmt>, String> {
    let mut body = Vec::new();
    while *i < lines.len() {
        let t = lines[*i].trim();
        if t.is_empty() {
            *i += 1;
            continue;
        }
        if t == "end" || t == "else" || t.starts_with("elseif ") || t.starts_with("else if ") {
            break;
        }
        let (s, next) = parse_stmt_line(lines, *i, t)?;
        body.push(s);
        *i = next;
    }
    Ok(body)
}

/// Inline single statement after `then`/`do` (`if c then stmt end`).
/// Supports the common corpus forms: return, local, assignment, call.
fn parse_inline(text: &str, out: &mut Vec<LegacyStmt>) -> Result<(), String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(());
    }
    if let Some(rest) = t.strip_prefix("return") {
        let v = rest.trim();
        out.push(LegacyStmt::Return { value: if v.is_empty() { None } else { Some(v.to_string()) } });
        return Ok(());
    }
    if let Some(rest) = t.strip_prefix("local ") {
        out.push(parse_local(rest)?);
        return Ok(());
    }
    if let Some(pos) = top_level_eq(t)
        && !t[pos..].starts_with("==")
    {
        out.push(LegacyStmt::Assign {
            name: t[..pos].trim().to_string(),
            value: t[pos + 1..].trim().to_string(),
        });
        return Ok(());
    }
    if let Some(open) = t.find('(')
        && t.ends_with(')')
    {
        out.push(LegacyStmt::Call {
            name: t[..open].trim().to_string(),
            args: split_args(&t[open + 1..t.len() - 1]),
        });
        return Ok(());
    }
    out.push(LegacyStmt::Raw(t.to_string()));
    Ok(())
}

/// Position of the `then` keyword: the LAST occurrence with non-identifier
/// characters around it. The corpus glues it (`pc.count_item(50703)<5-
/// pc.getqf("collect_count")then`) and spans it across lines.
fn find_then(text: &str) -> Option<usize> {
    let b = text.as_bytes();
    let mut last = None;
    let mut search = 0;
    while let Some(rel) = text[search..].find("then") {
        let p = search + rel;
        let before_ok = p == 0 || !(b[p - 1].is_ascii_alphanumeric() || b[p - 1] == b'_');
        let after_ok = p + 4 >= b.len() || !(b[p + 4].is_ascii_alphanumeric() || b[p + 4] == b'_');
        if before_ok && after_ok {
            last = Some(p);
        }
        search = p + 4;
    }
    last
}

/// `"<cond> then [<inline>]"` → (cond, inline). The inline is present only
/// for one-line forms; whether it carries its own `end` is decided by the
/// caller (the corpus writes `if s == 1 then s = 0` WITHOUT the inline end).
fn split_then(rest: &str) -> Result<(String, Option<String>), String> {
    let rest = rest.trim();
    let p = find_then(rest).ok_or_else(|| format!("if sin `then`: {rest}"))?;
    let cond = rest[..p].trim().to_string();
    if cond.is_empty() {
        return Err("if sin condición".into());
    }
    let after = rest[p + 4..].trim();
    if after.is_empty() {
        Ok((cond, None))
    } else {
        Ok((cond, Some(after.to_string())))
    }
}

/// Position of the first `=` outside quotes/parens that is not part of `==`.
fn top_level_eq(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut prev = '\0';
    for (idx, c) in text.char_indices() {
        match c {
            '"' | '\'' if quote.is_none() => quote = Some(c),
            '"' | '\'' if quote == Some(c) => quote = None,
            '(' if quote.is_none() => depth += 1,
            ')' if quote.is_none() => depth -= 1,
            '=' if quote.is_none() && depth == 0 && prev != '=' && !text[idx + 1..].starts_with('=') => {
                return Some(idx);
            }
            _ => {}
        }
        prev = c;
    }
    None
}

/// Splits a comma-separated argument list at depth 0 (parens/quotes safe).
pub(crate) fn split_args(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match c {
            '"' | '\'' if quote.is_none() => {
                quote = Some(c);
                cur.push(c);
            }
            '"' | '\'' if quote == Some(c) => {
                quote = None;
                cur.push(c);
            }
            '(' if quote.is_none() => {
                depth += 1;
                cur.push(c);
            }
            ')' if quote.is_none() => {
                depth -= 1;
                cur.push(c);
            }
            ',' if quote.is_none() && depth == 0 => {
                let arg = std::mem::take(&mut cur);
                if !arg.trim().is_empty() {
                    out.push(arg.trim().to_string());
                }
            }
            _ => cur.push(c),
        }
    }
    let last = cur.trim();
    if !last.is_empty() {
        out.push(last.to_string());
    }
    out
}

/// Strips `--` comments (also inline) outside quotes; keeps line structure.
fn strip_comments(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            let mut out = String::new();
            let mut quote: Option<char> = None;
            let mut cs = l.chars().peekable();
            while let Some(c) = cs.next() {
                match c {
                    '"' | '\'' if quote.is_none() => {
                        quote = Some(c);
                        out.push(c);
                    }
                    '"' | '\'' if quote == Some(c) => {
                        quote = None;
                        out.push(c);
                    }
                    '-' if quote.is_none() && cs.peek() == Some(&'-') => break,
                    _ => out.push(c),
                }
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLLECT_START: &str = "\
quest collect_quest_lv30  begin
\tstate start begin
\t\twhen login or levelup with pc.level >= 30 begin
\t\t\tset_state(information)
\t\tend
\tend

\tstate information begin
\t\twhen letter begin
\t\t\tlocal v = find_npc_by_vnum(20084)
\t\t\tif v != 0 then
\t\t\t\ttarget.vid(\"__TARGET__\", v, gameforge.collect_herb_lv10._150_sayTitle)
\t\t\tend
\t\t\tsend_letter(gameforge.collect_quest_lv30._10_sendLetter)
\t\tend

\t\twhen button or info begin
\t\t\tsay_title(gameforge.collect_quest_lv30._10_sendLetter)
\t\t\tsay(gameforge.collect_quest_lv30._20_say)
\t\tend
\tend
end
";

    #[test]
    fn parses_real_corpus_shape() {
        let q = parse_qc(COLLECT_START).expect("parse");
        assert_eq!(q.name, "collect_quest_lv30");
        assert_eq!(q.states.len(), 2);
        assert_eq!(q.states[0].name, "start");
        // when login or levelup with pc.level >= 30
        let ev = &q.states[0].events[0];
        assert_eq!(ev.triggers, vec!["login".to_string(), "levelup".to_string()]);
        assert_eq!(ev.condition.as_deref(), Some("pc.level >= 30"));
        assert_eq!(ev.body.len(), 1);
        let LegacyStmt::Call { name, .. } = &ev.body[0] else { panic!("set_state") };
        assert_eq!(name, "set_state");
        // information: find_npc_by_vnum idiom + send_letter
        let ev2 = &q.states[1].events[0];
        assert_eq!(ev2.triggers, vec!["letter".to_string()]);
        assert!(matches!(&ev2.body[0], LegacyStmt::Local { name, .. } if name == "v"));
        assert!(matches!(&ev2.body[1], LegacyStmt::If { .. }));
        let LegacyStmt::Call { name, args } = &ev2.body[2] else { panic!() };
        assert_eq!(name, "send_letter");
        assert_eq!(args, &vec!["gameforge.collect_quest_lv30._10_sendLetter".to_string()]);
        // button or info
        let ev3 = &q.states[1].events[1];
        assert_eq!(ev3.triggers, vec!["button".to_string(), "info".to_string()]);
    }

    #[test]
    fn parses_chat_trigger_with_locale_key() {
        let q = parse_qc("quest q begin\n\tstate start begin\n\t\twhen 20018.chat.gameforge.check_collect_reward._010_npcChat   begin\n\t\t\tsay(gameforge.check_collect_reward._030_say)\n\t\tend\n\tend\nend\n")
            .expect("parse");
        let ev = &q.states[0].events[0];
        assert_eq!(ev.triggers, vec!["20018.chat.gameforge.check_collect_reward._010_npcChat".to_string()]);
        let LegacyStmt::Call { name, .. } = &ev.body[0] else { panic!() };
        assert_eq!(name, "say");
    }

    #[test]
    fn parses_target_click_or_chat_with_tab() {
        let q = parse_qc("quest q begin\n\tstate start begin\n\t\twhen __TARGET__.target.click or\t20084.chat.gameforge.collect_quest_lv30._30_npcChat begin\n\t\t\ttarget.delete(\"__TARGET__\")\n\t\tend\n\tend\nend\n")
            .expect("parse");
        assert_eq!(
            q.states[0].events[0].triggers,
            vec![
                "__TARGET__.target.click".to_string(),
                "20084.chat.gameforge.collect_quest_lv30._30_npcChat".to_string()
            ]
        );
    }

    #[test]
    fn parses_if_elseif_else_and_inline_if() {
        let q = parse_qc(
            "quest q begin\n\tstate start begin\n\t\twhen login begin\n\t\t\tif pc.getqf(\"basic_weapon\") != 0 then\n\t\t\t\treturn\n\t\t\tend\n\t\t\tif lev == 0 then return end\n\t\t\tif reward == 1 then\n\t\t\t\taffect.add_collect(1, 1000, 60*60*24*365*60)\n\t\t\telseif reward == 2 then\n\t\t\t\taffect.add_collect(apply.DEF_GRADE_BONUS, 120, 60*60*24*365*60)\n\t\t\telse\n\t\t\t\taffect.add_collect(apply.ATT_GRADE_BONUS, 51, 60*60*24*365*60)\n\t\t\tend\n\t\tend\n\tend\nend\n",
        )
        .expect("parse");
        let body = &q.states[0].events[0].body;
        assert_eq!(body.len(), 3);
        let LegacyStmt::If { cond, then, elseif_, els } = &body[0] else { panic!() };
        assert_eq!(cond, "pc.getqf(\"basic_weapon\") != 0");
        assert!(matches!(&then[0], LegacyStmt::Return { value: None }));
        assert!(elseif_.is_empty() && els.is_empty());
        let LegacyStmt::If { cond, then, .. } = &body[1] else { panic!() };
        assert_eq!(cond, "lev == 0");
        assert!(matches!(&then[0], LegacyStmt::Return { .. }));
        let LegacyStmt::If { cond, elseif_, els, .. } = &body[2] else { panic!() };
        assert_eq!(cond, "reward == 1");
        assert_eq!(elseif_.len(), 1);
        assert_eq!(elseif_[0].0, "reward == 2");
        assert_eq!(els.len(), 1);
    }

    #[test]
    fn parses_for_and_helper_function() {
        let q = parse_qc(
            "quest give_basic_weapon begin\n\tstate start begin\n\t\twhen login begin\n\t\t\tfor _, elem in ipairs(itemJob[pc.job]) do\n\t\t\t\tpc.give_item2(elem[1], elem[2])\n\t\t\tend\n\t\t\tpc.setqf(\"basic_weapon\", 1)\n\t\t\tset_state(__COMPLETE__)\n\t\tend\n\tend\n\tstate run begin\n\t\twhen letter begin\n\t\t\tsay(\"\")\n\t\tend\n\t\tfunction show_mob_pos(lev)\n\t\t\tmap_index = pc.get_map_index()\n\t\tend\n\tend\nend\n",
        )
        .expect("parse");
        let st = &q.states[0];
        let body = &st.events[0].body;
        let LegacyStmt::For { head, body: fbody } = &body[0] else { panic!("for") };
        assert_eq!(head, "_, elem in ipairs(itemJob[pc.job])");
        assert!(matches!(&fbody[0], LegacyStmt::Call { name, .. } if name == "pc.give_item2"));
        let LegacyStmt::Call { name, .. } = &body[1] else { panic!() };
        assert_eq!(name, "pc.setqf");
        // state-level helper function
        let run = &q.states[1];
        assert_eq!(run.helpers.len(), 1);
        assert_eq!(run.helpers[0].name, "show_mob_pos");
        assert!(matches!(&run.helpers[0].body[0], LegacyStmt::Assign { name, .. } if name == "map_index"));
    }

    #[test]
    fn strips_inline_comments_but_not_in_strings() {
        let q = parse_qc(
            "quest q begin\n\tstate start begin\n\t\twhen login begin -- comment\n\t\t\tsay(\"a--b\") -- tail\n\t\tend\n\tend\nend\n",
        )
        .expect("parse");
        let body = &q.states[0].events[0].body;
        let LegacyStmt::Call { name, args } = &body[0] else { panic!() };
        assert_eq!(name, "say");
        assert_eq!(args, &vec!["\"a--b\"".to_string()]);
    }

    #[test]
    fn skips_define_lines_and_bom() {
        let q = parse_qc("\u{feff}define ENABLE_SAY false\nquest q begin\n\tstate start begin\n\t\twhen login begin\n\t\t\treturn\n\t\tend\n\tend\nend\n")
            .expect("parse");
        assert_eq!(q.name, "q");
        assert!(matches!(&q.states[0].events[0].body[0], LegacyStmt::Return { value: None }));
    }

    #[test]
    fn errors_without_quest_header() {
        assert!(parse_qc("state start begin\nend\n").is_err());
        assert!(parse_qc("quest q\n").is_err()); // falta begin
    }

    #[test]
    fn parses_multiline_when_head() {
        // collect_quest_lv50.quest: the trigger list continues on the next
        // line; main_quest_lv32.quest: tab before `begin`.
        let q = parse_qc(
            "quest q begin\n\tstate start begin\n\t\twhen __TARGET__.target.click or\n\t\t\t20084.chat.gameforge.q._30_npcChat begin\n\t\t\ttarget.delete(\"__TARGET__\")\n\t\tend\n\t\twhen 71178.use\n\t\t\tbegin\n\t\t\twait()\n\t\tend\n\t\twhen 9011.chat.gameforge.marriage_manage._540_npcChat with\n\t\t\tpc.count_item(30006) > 0 begin\n\t\t\tsay(\"x\")\n\t\tend\n\t\twhen 20047.click or\t20048.click\tbegin\n\t\t\tsay(\"y\")\n\t\tend\n\tend\nend\n",
        )
        .expect("parse");
        let evs = &q.states[0].events;
        assert_eq!(evs[0].triggers, vec!["__TARGET__.target.click".to_string(), "20084.chat.gameforge.q._30_npcChat".to_string()]);
        assert_eq!(evs[1].triggers, vec!["71178.use".to_string()]);
        assert_eq!(evs[2].triggers, vec!["9011.chat.gameforge.marriage_manage._540_npcChat".to_string()]);
        assert_eq!(evs[2].condition.as_deref(), Some("pc.count_item(30006) > 0"));
        assert_eq!(evs[3].triggers, vec!["20047.click".to_string(), "20048.click".to_string()]);
    }

    #[test]
    fn parses_multiline_if_condition_and_glued_then() {
        // levelup.quest: condition spans lines; collect_herb_lv10: `then`
        // glued to the expression; horse_menu: inline if without `end`.
        let q = parse_qc(
            "quest q begin\n\tstate start begin\n\t\twhen letter begin\n\t\t\tif current == 0 and\n\t\t\t\tpc.get_level() > completed_level and\n\t\t\t\tpc.get_level() < 90 then\n\t\t\t\tpc.setqf(\"current\", lev)\n\t\t\tend\n\t\t\tif pc.count_item(50703)<5- pc.getqf(\"collect_count\")then\n\t\t\t\tsay(\"x\")\n\t\t\tend\n\t\t\tif s == 1 then s = 0\n\t\t\telse\n\t\t\t\treturn\n\t\t\tend\n\t\t\tif foundskel == 4 then\n\t\t\t\tsay(\"a\")\n\t\t\telse if foundskel < 4 then\n\t\t\t\tsay(\"b\")\n\t\t\telse\n\t\t\t\tsay(\"c\")\n\t\t\tend\n\t\tend\n\tend\nend\n",
        )
        .expect("parse");
        let body = &q.states[0].events[0].body;
        assert_eq!(body.len(), 4);
        let LegacyStmt::If { cond, .. } = &body[0] else { panic!() };
        assert!(cond.starts_with("current == 0 and"));
        assert!(cond.contains("pc.get_level() < 90"));
        let LegacyStmt::If { cond, .. } = &body[1] else { panic!() };
        assert!(cond.contains("collect_count"));
        // inline without end + else on the next line
        let LegacyStmt::If { then, els, .. } = &body[2] else { panic!() };
        assert!(matches!(&then[0], LegacyStmt::Assign { name, .. } if name == "s"));
        assert!(matches!(&els[0], LegacyStmt::Return { .. }));
        // `else if` variant
        let LegacyStmt::If { elseif_, els, .. } = &body[3] else { panic!() };
        assert_eq!(elseif_.len(), 1);
        assert!(!els.is_empty());
    }

    #[test]
    fn tolerates_prose_before_quest() {
        let q = parse_qc("NOTES:\nThis is a sample quest file.\nquest q begin\n\tstate start begin\n\t\twhen login begin\n\t\t\treturn\n\t\tend\n\tend\nend\n")
            .expect("parse");
        assert_eq!(q.name, "q");
        assert_eq!(q.unparsed.len(), 2);
    }

    #[test]
    fn parses_inline_when_body_and_while_repeat() {
        // horse_menu.quest: inline when body; new_quest_lv75.quest: while +
        // repeat inside ifs (end-nesting must stay correct).
        let q = parse_qc(
            "quest q begin\n\tstate start begin\n\t\twhen 20119.click begin horse_menu.horse_menu() end\n\t\twhen 1137.kill begin\n\t\t\tif pc.countitem(diarypage) < 1 then\n\t\t\t\tlocal counter2 = 0\n\t\t\t\twhile counter2 < 5 do\n\t\t\t\t\tif pc.getqf(\"x\") == 0 then\n\t\t\t\t\t\tpc.setqf(\"x\", 1)\n\t\t\t\t\tend\n\t\t\t\t\tcounter2 = counter2 + 1\n\t\t\t\tend\n\t\t\t\trepeat\n\t\t\t\t\tcounter2 = counter2 - 1\n\t\t\t\tuntil counter2 <= 0\n\t\t\t\tpc.setqf(\"done\", 1)\n\t\t\tend\n\t\tend\n\tend\nend\n",
        )
        .expect("parse");
        let evs = &q.states[0].events;
        // inline when body
        let LegacyStmt::Call { name, .. } = &evs[0].body[0] else { panic!("inline call") };
        assert_eq!(name, "horse_menu.horse_menu");
        // while + repeat nested inside if
        let body = &evs[1].body;
        let LegacyStmt::If { then, .. } = &body[0] else { panic!("if") };
        assert!(matches!(&then[0], LegacyStmt::Local { .. }));
        let LegacyStmt::While { head, body: wb } = &then[1] else { panic!("while") };
        assert_eq!(head, "counter2 < 5");
        assert_eq!(wb.len(), 2); // if + assign
        let LegacyStmt::Repeat { body: rb, until } = &then[2] else { panic!("repeat") };
        assert_eq!(rb.len(), 1);
        assert_eq!(until, "counter2 <= 0");
        assert!(matches!(&then[3], LegacyStmt::Call { name, .. } if name == "pc.setqf"));
    }

    #[test]
    fn splits_args_with_quotes_and_nesting() {
        assert_eq!(
            split_args("pc.getqf(\"collect_count\")+1, 60*60*22, \"a,b(c)\", 'x'"),
            vec![
                "pc.getqf(\"collect_count\")+1".to_string(),
                "60*60*22".to_string(),
                "\"a,b(c)\"".to_string(),
                "'x'".to_string()
            ]
        );
    }
}
