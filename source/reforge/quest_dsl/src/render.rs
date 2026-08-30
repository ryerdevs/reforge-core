//! Debug renderer: prints a `QuestFile` back in (approximately) the DSL
//! syntax. Used by tests for AST roundtrip verification and as the basis for
//! a future CLI validator output.

use crate::ast::*;
use std::fmt::Write as _;

/// Renders the file back to DSL text (best-effort, stable for tests).
pub fn render(file: &QuestFile) -> String {
    let mut out = String::new();
    for imp in &file.imports {
        let _ = writeln!(out, "import {imp}");
    }
    for b in &file.blocks {
        let _ = writeln!(out, "block {}({})", b.name, render_params(&b.params));
        render_stmts(&mut out, &b.body, "  ");
    }
    for q in &file.quests {
        match q {
            Quest::Concrete(d) => {
                let _ = writeln!(out, "quest {}", d.name);
                render_states(&mut out, &d.states);
            }
            Quest::Family {
                name,
                params,
                states,
            } => {
                let _ = writeln!(out, "quest {name} family ({})", render_params(params));
                render_states(&mut out, states);
            }
            Quest::Instance(i) => {
                let args: Vec<String> = i
                    .args
                    .iter()
                    .map(|(n, v)| format!("{n}: {}", render_value(v)))
                    .collect();
                let _ = writeln!(out, "quest {} = {}({})", i.name, i.base, args.join(", "));
            }
        }
    }
    out
}

fn render_states(out: &mut String, states: &[State]) {
    for s in states {
        let _ = writeln!(out, "  state {}", s.name);
        for e in &s.events {
            let trigs: Vec<String> = e.triggers.iter().map(render_trigger).collect();
            let cond = match &e.condition {
                Some(c) => format!(" with {}", render_expr(c)),
                None => String::new(),
            };
            let _ = writeln!(out, "    on {}{cond}", trigs.join(", "));
            render_stmts(out, &e.body, "      ");
        }
    }
}

/// Renders statements with a fixed indent; branch bodies get one extra
/// level (the parser expects `if`/`else` at statement indent and the body
/// DEEPER — 1 level + else, spec §10).
fn render_stmts(out: &mut String, stmts: &[Stmt], indent: &str) {
    for st in stmts {
        match st {
            Stmt::Branch(b) => {
                let head = match &b.condition {
                    Some(c) => format!("if {}", render_expr(c)),
                    None => "else".to_string(),
                };
                let _ = writeln!(out, "{indent}{head}");
                render_stmts(out, &b.body, &format!("{indent}  "));
            }
            other => {
                let _ = writeln!(out, "{indent}{}", render_stmt(other));
            }
        }
    }
}

fn render_stmt(st: &Stmt) -> String {
    match st {
        Stmt::Action { action, capture } => {
            let args: Vec<String> = action.args.iter().map(render_value).collect();
            let cap = capture
                .as_ref()
                .map(|c| format!(" as {c}"))
                .unwrap_or_default();
            format!(
                "-> {}({}){cap}",
                render_action(&action.name),
                args.join(", ")
            )
        }
        Stmt::Branch(b) => match &b.condition {
            Some(c) => format!("if {}", render_expr(c)),
            None => "else".to_string(),
        },
        Stmt::Use { name, args } => {
            let args: Vec<String> = args.iter().map(render_value).collect();
            format!("use {name}({})", args.join(", "))
        }
    }
}

fn render_params(params: &[Param]) -> String {
    params
        .iter()
        .map(|p| match &p.ty {
            Some(ty) => format!("{}: {}", p.name, render_ty(*ty)),
            None => p.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_ty(t: ParamType) -> &'static str {
    match t {
        ParamType::Vnum => "vnum",
        ParamType::Level => "level",
        ParamType::Key => "key",
        ParamType::Str => "str",
    }
}

fn render_trigger(t: &Trigger) -> String {
    match &t.kind {
        TriggerKind::Login => "login".into(),
        TriggerKind::LevelUp => "levelup".into(),
        TriggerKind::Letter => "letter".into(),
        TriggerKind::Button => "button".into(),
        TriggerKind::Info => "info".into(),
        TriggerKind::Enter => "enter".into(),
        TriggerKind::Logout => "logout".into(),
        TriggerKind::Timer => "timer".into(),
        TriggerKind::Chat { target } => format!("{}.chat", render_target(target)),
        TriggerKind::Kill { target } => format!("{}.kill", render_target(target)),
        TriggerKind::Use { target } => format!("{}.use", render_target(target)),
        TriggerKind::TargetClick => "__TARGET__.target.click".into(),
        TriggerKind::Rust(s) => s.clone(),
    }
}

fn render_target(t: &TriggerTarget) -> String {
    match t {
        TriggerTarget::Num(n) => n.to_string(),
        TriggerTarget::Param(p) => format!("({p})"),
    }
}

fn render_action(a: &ActionName) -> String {
    match a {
        ActionName::SayTitle => "say_title".into(),
        ActionName::Say => "say".into(),
        ActionName::SayReward => "say_reward".into(),
        ActionName::SayItemVnum => "say_item_vnum".into(),
        ActionName::SendLetter => "send_letter".into(),
        ActionName::ClearLetter => "clear_letter".into(),
        ActionName::Wait => "wait".into(),
        ActionName::SetState => "set_state".into(),
        ActionName::SetQuestState => "set_quest_state".into(),
        ActionName::SetQf => "set_qf".into(),
        ActionName::GiveItem2 => "give_item2".into(),
        ActionName::RemoveItem => "remove_item".into(),
        ActionName::TargetVid => "target_vid".into(),
        ActionName::TargetDelete => "target_delete".into(),
        ActionName::Warp => "warp".into(),
        ActionName::Notice => "notice".into(),
        ActionName::NoticeMultiline => "notice_multiline".into(),
        ActionName::AffectAdd => "affect_add".into(),
        ActionName::AffectRemove => "affect_remove".into(),
        ActionName::Select => "select".into(),
        ActionName::InputNumber => "input_number".into(),
        ActionName::Return => "return".into(),
    }
}

/// Renders a value back to DSL argument syntax (`5`, `"str"`, `@key`,
/// `(param)`, `get_time + 60 * 60 * 22`). Public for the CLI reports.
pub fn render_value(v: &Value) -> String {
    match v {
        Value::Num(n) => n.to_string(),
        Value::Str(s) => format!("\"{s}\""),
        Value::Key(k) => format!("@{k}"),
        Value::Param(p) => format!("({p})"),
        Value::Expr(e) => render_expr(e),
    }
}

fn render_expr(e: &Expr) -> String {
    match e {
        Expr::Between(a, b, c) => format!(
            "{} between {}, {}",
            render_expr(a),
            render_expr(b),
            render_expr(c)
        ),
        Expr::Compare(a, op, b) => {
            let op = match op {
                CmpOp::Eq => "==",
                CmpOp::Ne => "!=",
                CmpOp::Lt => "<",
                CmpOp::Gt => ">",
                CmpOp::Le => "<=",
                CmpOp::Ge => ">=",
            };
            format!("{} {op} {}", render_expr(a), render_expr(b))
        }
        Expr::Add(a, b) => format!("{} + {}", render_expr(a), render_expr(b)),
        Expr::Sub(a, b) => format!("{} - {}", render_expr(a), render_expr(b)),
        Expr::Mul(a, b) => format!("{} * {}", render_expr(a), render_expr(b)),
        Expr::Div(a, b) => format!("{} / {}", render_expr(a), render_expr(b)),
        Expr::And(a, b) => format!("{} and {}", render_expr(a), render_expr(b)),
        Expr::Or(a, b) => format!("{} or {}", render_expr(a), render_expr(b)),
        Expr::Not(a) => format!("not {}", render_expr(a)),
        Expr::Value(v) => render_value(v),
        Expr::Capture(c) => c.clone(),
        Expr::Func(f, args) => {
            let name = match f {
                FuncName::PcLevel => "pc.level",
                FuncName::CountItem => "count_item",
                FuncName::GetQf => "get_qf",
                FuncName::Number => "number",
                FuncName::GetTime => "get_time",
                FuncName::GetMapIndex => "get_map_index",
                FuncName::GetGmLevel => "get_gm_level",
                FuncName::PetIsSummon => "pet.is_summon",
                FuncName::IsTestServer => "is_test_server",
            };
            if args.is_empty() {
                // Sin paréntesis cuando no hay args (spec §4: `pc.level`).
                name.to_string()
            } else {
                let args: Vec<String> = args.iter().map(render_expr).collect();
                format!("{name}({})", args.join(", "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn roundtrip_concrete_quest() {
        let src = "quest q\n  state start\n    on login, levelup with pc.level >= 30\n      -> set_state(information)\n    on 601.kill\n      -> give_item2(30006, 1)\n";
        let file = parse(src).unwrap();
        let out = render(&file);
        // El render debe parsear de nuevo a un AST igual.
        let file2 = parse(&out).unwrap();
        assert_eq!(file, file2);
        assert!(out.contains("quest q"));
        assert!(out.contains("on login, levelup with pc.level >= 30"));
        assert!(out.contains("-> give_item2(30006, 1)"));
    }

    #[test]
    fn roundtrip_family() {
        let src = "quest fam family (level, mob)\n  state start\n    on login with pc.level >= (level)\n      -> wait()\nquest q = fam(level: 30, mob: 601)\n";
        let file = parse(src).unwrap();
        let out = render(&file);
        let file2 = parse(&out).unwrap();
        assert_eq!(file, file2);
    }

    #[test]
    fn roundtrip_branches() {
        let src = "quest q\n  state start\n    on 20011.chat\n      -> select(@_20_say, @_30_say) as choice\n      if choice == 1\n        -> warp(896500, 24600)\n      else\n        -> return\n";
        let file = parse(src).unwrap();
        let out = render(&file);
        let file2 = parse(&out).unwrap();
        assert_eq!(file, file2);
        assert!(out.contains("if choice == 1"), "{out}");
        assert!(out.contains("else"), "{out}");
    }

    #[test]
    fn roundtrip_expr_args() {
        // Full expressions as action args (set_qf values, affect durations)
        // must survive render → parse with the SAME AST.
        let src = "quest q\n  state start\n    on login\n      -> set_qf(duration, get_time() + 60*60*22)\n      -> affect_add(apply.MOV_SPEED, 10, 60*60*24*365*60)\n";
        let file = parse(src).unwrap();
        let out = render(&file);
        let file2 = parse(&out).unwrap();
        assert_eq!(file, file2);
        // Canonical render: zero-arg funcs stay bare, mul chains left-assoc;
        // `apply.MOV_SPEED` is a bare word → string (quoted).
        assert!(
            out.contains("set_qf(\"duration\", get_time + 60 * 60 * 22)"),
            "{out}"
        );
        assert!(
            out.contains("affect_add(\"apply.MOV_SPEED\", 10, 60 * 60 * 24 * 365 * 60)"),
            "{out}"
        );
    }
}
