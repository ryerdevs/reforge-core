//! qc → DSL converter (spec §9): parses legacy `.quest` files with the qc
//! parser, maps calls/triggers/conditions through the equivalence tables
//! (§3-§5) into the DSL AST and renders the DSL text.
//!
//! Anything unmappable goes into the discrepancy report — the converter
//! NEVER fails on unknown Lua; it produces `converted + unmapped` per file.
//! The parity harness (dual execution vs the legacy Lua) is a later slice
//! (spec §9.5); this slice delivers the converter + equivalence tables.

pub mod map;
pub mod qc;

use std::collections::BTreeMap;

use crate::ast::*;
use crate::family::{detect_similar_groups, expand_families, extract_family_params, quest_similarity};
use crate::parser::parse;
use crate::render::render;
use qc::parse_qc;

/// One unmapped construct: (file, what). `item` examples:
/// `call:pc.job`, `call:pc.setqf:expresión no representable...`,
/// `trigger:kill (...)`, `if:elseif ...`, `for:...`, `helper:function x`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unmapped {
    pub file: String,
    pub item: String,
}

impl Unmapped {
    fn new(file: &str, item: impl Into<String>) -> Self {
        Self { file: file.to_string(), item: item.into() }
    }
}

/// Family proposal from the quest-name heuristic (spec §6): quests whose
/// names share a prefix + numeric suffix (`collect_quest_lv30..lv96`,
/// `subquest_01..49`) are grouped. Human confirmation is required — the
/// similarity engine (spec §9.3) is a future slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyProposal {
    pub name: String,
    pub members: Vec<String>,
    pub param: String,
}

/// Per-file conversion result.
#[derive(Debug, Clone, PartialEq)]
pub struct FileResult {
    pub file: String,
    pub ok: bool,
    pub error: Option<String>,
    /// The rendered DSL text (when ok).
    pub dsl: Option<String>,
    pub unmapped: Vec<Unmapped>,
    /// Non-fatal notes (e.g. dropped npcChat locale keys).
    pub notes: Vec<String>,
}

/// Converts one legacy quest file to DSL text.
pub fn convert_file(file: &str, text: &str) -> FileResult {
    let legacy = match parse_qc(text) {
        Ok(q) => q,
        Err(e) => {
            return FileResult {
                file: file.to_string(),
                ok: false,
                error: Some(format!("qc parse: {e}")),
                dsl: None,
                unmapped: Vec::new(),
                notes: Vec::new(),
            }
        }
    };
    let mut unmapped = Vec::new();
    let mut notes = Vec::new();

    for u in &legacy.unparsed {
        unmapped.push(Unmapped::new(file, format!("raw(quest):{u}")));
    }

    let mut states = Vec::new();
    for st in &legacy.states {
        for u in &st.unparsed {
            unmapped.push(Unmapped::new(file, format!("raw(state):{u}")));
        }
        let mut events = Vec::new();
        for ev in &st.events {
            // Triggers: an unmappable trigger skips the whole event (the
            // DSL has no way to express it; the report records it).
            let mut triggers = Vec::new();
            let mut triggers_ok = true;
            for t in &ev.triggers {
                match map::map_trigger(t) {
                    Ok(tr) => triggers.push(tr),
                    Err(e) => {
                        unmapped.push(Unmapped::new(file, format!("trigger:{t} ({e})")));
                        triggers_ok = false;
                    }
                }
            }
            if !triggers_ok {
                continue;
            }
            // npcChat keys are UI strings dropped by the trigger mapping.
            for t in &ev.triggers {
                if let Some(k) = map::chat_key_suffix(t) {
                    notes.push(format!("chat key {k} descartada (UI string)"));
                }
            }
            // `with` condition: an unmappable condition skips the event.
            let condition = match &ev.condition {
                Some(c) => match map::map_expr(c, &map::Scope::default()) {
                    Ok(e) => Some(e),
                    Err(e) => {
                        unmapped.push(Unmapped::new(file, format!("with:{e}")));
                        None
                    }
                },
                None => None,
            };
            if ev.condition.is_some() && condition.is_none() {
                continue;
            }
            let body = map::map_stmts(&ev.body, file, &mut unmapped);
            events.push(Event { triggers, condition, body });
        }
        for h in &st.helpers {
            unmapped.push(Unmapped::new(file, format!("helper:function {} (Rust module)", h.name)));
        }
        states.push(State { name: st.name.clone(), events });
    }

    let file_ast = QuestFile {
        imports: Vec::new(),
        blocks: Vec::new(),
        quests: vec![Quest::Concrete(QuestDef { name: legacy.name.clone(), states })],
    };
    let dsl = render(&file_ast);
    // Safety: the generated DSL must re-parse (the renderer is the contract).
    match parse(&dsl) {
        Ok(_) => FileResult {
            file: file.to_string(),
            ok: true,
            error: None,
            dsl: Some(dsl),
            unmapped,
            notes,
        },
        Err(e) => FileResult {
            file: file.to_string(),
            ok: false,
            error: Some(format!("el DSL generado no reparsea: {e}")),
            dsl: None,
            unmapped,
            notes,
        },
    }
}

/// File stem of a quest path: `biology/collect_quest_lv30.quest` →
/// `collect_quest_lv30`.
fn quest_stem(name: &str) -> &str {
    let base = name.rsplit('/').next().unwrap_or(name);
    base.strip_suffix(".quest").unwrap_or(base)
}

/// Name heuristic (spec §6): `collect_quest_lv30` → family `collect_quest` +
/// param `level`; `subquest_01` → family `subquest` + param `index`.
/// Inputs may be file paths (`biology/collect_quest_lv30.quest`) — the stem
/// is used for matching, the full path is kept in the member list.
/// Groups with ≥2 members become proposals (human confirmation required —
/// the similarity engine is a future slice).
pub fn detect_families(converted: &[String]) -> Vec<FamilyProposal> {
    let mut groups: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for name in converted {
        let s = quest_stem(name);
        // `(.+)_lv(\d+)$` first: the underscore precedes the `lv` suffix.
        let matched = s
            .rsplit_once("_lv")
            .filter(|(_, suffix)| !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()))
            .map(|(prefix, _)| (prefix.to_string(), "level".to_string()))
            .or_else(|| {
                s.rsplit_once('_')
                    .filter(|(_, suffix)| !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()))
                    .map(|(prefix, _)| (prefix.to_string(), "index".to_string()))
            });
        if let Some((family, param)) = matched {
            groups.entry(family).or_insert_with(|| (param, Vec::new())).1.push(name.clone());
        }
    }
    groups
        .into_iter()
        .filter(|(_, (_, members))| members.len() >= 2)
        .map(|(name, (param, members))| FamilyProposal { name, members, param })
        .collect()
}

/// Whole-corpus conversion. `files` = (relative path, text) pairs.
/// Returns (converted DSL texts — member files + extracted family files,
/// aggregate report).
pub fn convert_corpus(files: &[(String, String)]) -> (Vec<(String, String)>, CorpusReport) {
    let mut outputs = Vec::new();
    let mut converted = Vec::new();
    let mut failed = Vec::new();
    let mut unmapped = Vec::new();
    let mut notes = Vec::new();
    for (file, text) in files {
        let r = convert_file(file, text);
        if r.ok {
            converted.push(file.clone());
            if let Some(dsl) = r.dsl {
                outputs.push((file.clone(), dsl));
            }
        } else {
            failed.push((file.clone(), r.error.unwrap_or_default()));
        }
        unmapped.extend(r.unmapped);
        notes.extend(r.notes);
    }
    let families = detect_families(&converted);
    let (family_outputs, family_failed, similarity) = build_family_outputs(&families, &outputs);
    outputs.extend(family_outputs.iter().map(|f| (f.file.clone(), f.text.clone())));
    (
        outputs,
        CorpusReport { converted, failed, unmapped, notes, families, family_outputs, family_failed, similarity },
    )
}

/// A generated family file: the family template + one instance per usable
/// member (spec §6), rendered to DSL and verified against the originals.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyOutput {
    pub name: String,
    /// Output relative path, next to the first member (`biology/collect_quest.family.quest`).
    pub file: String,
    /// The rendered family file text (family + instances).
    pub text: String,
    /// Members excluded from the family (name, reason).
    pub excluded: Vec<(String, String)>,
    /// Expansion parity: every expanded instance equals its original quest.
    pub parity_ok: bool,
}

/// Similarity-ranked family proposal (spec §9.3): the strict extraction
/// rejected the name group (structure differs); the similarity layer
/// measures how close the members are, what would become family params and
/// what differs structurally. Proposal only — the merge engine consumes it.
#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityProposal {
    pub name: String,
    pub suffix_param: String,
    /// Quest names of the cluster.
    pub members: Vec<String>,
    /// Mean pairwise similarity of the cluster.
    pub mean: f64,
    /// Lowest pairwise similarity within the cluster.
    pub min: f64,
    /// Common structural signature size (slot paths in every member).
    pub common: usize,
    /// Varying literal slots (family params): path → per-member values.
    pub params: Vec<(String, Vec<Value>)>,
    /// Structural deltas of the MOST similar pair (`<quest>:<slot path>` —
    /// the slot exists only in that member).
    pub deltas: Vec<String>,
}

/// Similarity threshold for clustering name groups (spec §9.3).
const SIM_THRESHOLD: f64 = 0.5;

/// For each name-heuristic proposal: diff the members' converted quests and
/// extract the family (template + instances). Members whose DSL is missing or
/// does not parse are reported as failures; structurally different members are
/// excluded from the extraction (not a failure). When the STRICT extraction
/// rejects the whole group, the similarity layer (spec §9.3) ranks the
/// members instead — the report gets proposals, not just failures.
fn build_family_outputs(
    families: &[FamilyProposal],
    outputs: &[(String, String)],
) -> (Vec<FamilyOutput>, Vec<(String, String)>, Vec<SimilarityProposal>) {
    let mut outs = Vec::new();
    let mut failed = Vec::new();
    let mut similarity = Vec::new();
    for prop in families {
        let mut members: Vec<QuestDef> = Vec::new();
        for m in &prop.members {
            match outputs.iter().find(|(f, _)| f == m) {
                Some((_, dsl)) => match parse(dsl).ok().and_then(|f| f.quests.into_iter().next()) {
                    Some(Quest::Concrete(qd)) => members.push(qd),
                    _ => failed.push((prop.name.clone(), format!("miembro {m}: DSL no parseable"))),
                },
                None => failed.push((prop.name.clone(), format!("miembro {m}: sin salida convertida"))),
            }
        }
        match extract_family_params(&prop.name, &prop.param, &members) {
            Ok(ext) => {
                let excluded = ext.excluded.clone();
                let mut quests = vec![ext.family.clone()];
                quests.extend(ext.instances.clone().into_iter().map(Quest::Instance));
                let text = render(&QuestFile { imports: Vec::new(), blocks: Vec::new(), quests });
                // Parity harness (spec §9.5): the family file must reparse and
                // expand to the exact original member quests.
                let parity_ok = parse(&text).is_ok_and(|f| {
                    expand_families(&f).is_ok_and(|expanded| {
                        let usable: Vec<QuestDef> = members
                            .iter()
                            .filter(|m| !excluded.iter().any(|(n, _)| n == &m.name))
                            .cloned()
                            .collect();
                        expanded.len() == usable.len() && expanded.iter().zip(&usable).all(|(a, b)| a == b)
                    })
                });
                let dir = prop.members[0].rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
                outs.push(FamilyOutput {
                    name: prop.name.clone(),
                    file: format!("{dir}{}.family.quest", prop.name),
                    text,
                    excluded,
                    parity_ok,
                });
            }
            Err(e) => {
                if members.len() < 2 {
                    failed.push((prop.name.clone(), e));
                    continue;
                }
                // Similarity layer (spec §9.3): the strict gate rejected the
                // group (structure differs) — measure and propose instead of
                // just failing. Clusters above the threshold get one proposal
                // each; a heterogeneous group still gets a whole-set proposal
                // (threshold 0 unions everything) so nothing is hidden.
                let mut groups = detect_similar_groups(&members, SIM_THRESHOLD);
                if groups.is_empty() {
                    groups = detect_similar_groups(&members, 0.0);
                }
                let by_name: std::collections::HashMap<&str, &QuestDef> =
                    members.iter().map(|m| (m.name.as_str(), m)).collect();
                for g in groups {
                    // Structural deltas of the most similar pair.
                    let mut deltas = Vec::new();
                    let mut best: Option<(f64, &QuestDef, &QuestDef)> = None;
                    for (i, a) in g.members.iter().enumerate() {
                        for b in &g.members[i + 1..] {
                            let s = quest_similarity(by_name[a.as_str()], by_name[b.as_str()]);
                            if best.is_none_or(|(bs, _, _)| s.score > bs) {
                                best = Some((s.score, by_name[a.as_str()], by_name[b.as_str()]));
                            }
                        }
                    }
                    if let Some((_, pa, pb)) = best {
                        let s = quest_similarity(pa, pb);
                        for p in &s.a_only {
                            deltas.push(format!("{}:{}", pa.name, p));
                        }
                        for p in &s.b_only {
                            deltas.push(format!("{}:{}", pb.name, p));
                        }
                    }
                    similarity.push(SimilarityProposal {
                        name: prop.name.clone(),
                        suffix_param: prop.param.clone(),
                        members: g.members,
                        mean: g.mean,
                        min: g.min,
                        common: g.common_paths.len(),
                        params: g.params,
                        deltas,
                    });
                }
            }
        }
    }
    (outs, failed, similarity)
}

/// Aggregate discrepancy report for a corpus run.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CorpusReport {
    pub converted: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub unmapped: Vec<Unmapped>,
    /// Non-fatal notes across the corpus.
    pub notes: Vec<String>,
    pub families: Vec<FamilyProposal>,
    /// Family files generated (family template + instances), with parity.
    pub family_outputs: Vec<FamilyOutput>,
    /// Family proposals that could not be extracted (name, reason).
    pub family_failed: Vec<(String, String)>,
    /// Similarity-ranked proposals for the REJECTED name groups (spec §9.3).
    pub similarity: Vec<SimilarityProposal>,
}

impl CorpusReport {
    /// Top-N unmapped CALL names by frequency (the parity groundwork
    /// evidence: which legacy functions the DSL catalog still lacks).
    pub fn top_unmapped_calls(&self, n: usize) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for u in &self.unmapped {
            if let Some(rest) = u.item.strip_prefix("call:") {
                let name = rest.split(':').next().unwrap_or(rest).to_string();
                *counts.entry(name).or_default() += 1;
            }
        }
        let mut v: Vec<(String, usize)> = counts.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real corpus-shaped quest (from collect_quest_lv30.quest, condensed):
    /// family-ish quest with dialog, the find_npc/target idiom, kill with
    /// probability and a state transition.
    const COLLECT: &str = "\
quest collect_quest_lv30 begin
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
\t\twhen 601.kill begin
\t\t\tlocal s = number(1, 100)
\t\t\tif s <= 5 then
\t\t\t\tpc.give_item2(30006, 1)
\t\t\tend
\t\tend
\t\twhen 20084.chat.gameforge.collect_quest_lv30._140_npcChat with pc.count_item(30006) >0 begin
\t\t\tpc.remove_item(30006, 1)
\t\t\tpc.setqf(\"duration\", 0)
\t\t\tset_state(go_to_disciple)
\t\tend
\tend
\tstate __complete begin
\tend
end
";

    #[test]
    fn converts_real_corpus_quest() {
        let r = convert_file("biology/collect_quest_lv30.quest", COLLECT);
        assert!(r.ok, "{:?}", r.error);
        let dsl = r.dsl.clone().unwrap();
        // The output must re-parse as DSL.
        let file = parse(&dsl).unwrap_or_else(|e| panic!("{e}\n{dsl}"));
        let Quest::Concrete(q) = &file.quests[0] else { panic!() };
        assert_eq!(q.name, "collect_quest_lv30");
        assert_eq!(q.states.len(), 3);
        // start: on login, levelup with pc.level >= 30
        let ev = &q.states[0].events[0];
        assert_eq!(ev.triggers.len(), 2);
        assert!(ev.condition.is_some());
        // information: target_vid idiom converted + send_letter
        let ev = &q.states[1].events[0];
        let Stmt::Action { action, .. } = &ev.body[0] else { panic!() };
        assert_eq!(action.name, ActionName::TargetVid);
        assert_eq!(action.args, vec![
            Value::Str("__TARGET__".into()),
            Value::Num(20084),
            Value::Str("gameforge.collect_herb_lv10._150_sayTitle".into()),
        ]);
        let Stmt::Action { action, .. } = &ev.body[1] else { panic!() };
        assert_eq!(action.name, ActionName::SendLetter);
        // 601.kill: if number(1, 100) <= 5 → give_item2(30006, 1)
        let ev = &q.states[1].events[1];
        let Stmt::Branch(b) = &ev.body[0] else { panic!("if") };
        assert!(matches!(b.condition, Some(Expr::Compare(_, CmpOp::Le, _))));
        // chat with key + condition: key dropped (note), condition mapped
        let ev = &q.states[1].events[2];
        assert_eq!(ev.triggers.len(), 1);
        assert!(matches!(ev.triggers[0].kind, TriggerKind::Chat { target: TriggerTarget::Num(20084) }));
        assert!(ev.condition.is_some());
        assert_eq!(ev.body.len(), 3); // remove_item, set_qf, set_state
        // Notes record the dropped npcChat key.
        assert!(r.notes.iter().any(|n| n.contains("_140_npcChat")), "{:?}", r.notes);
        // Nothing unmapped in this quest.
        assert!(r.unmapped.is_empty(), "{:?}", r.unmapped);
    }

    #[test]
    fn converts_select_and_if_else() {
        let src = "\
quest branch_quest begin
\tstate start begin
\t\twhen 20011.chat begin
\t\t\tlocal choice = select(gameforge.q._20_say, gameforge.q._30_say)
\t\t\tif choice == 1 then
\t\t\t\tpc.warp(896500, 24600)
\t\t\telse
\t\t\t\treturn
\t\t\tend
\t\tend
\tend
end
";
        let r = convert_file("branch.quest", src);
        assert!(r.ok, "{:?}", r.error);
        assert!(r.unmapped.is_empty(), "{:?}", r.unmapped);
        let dsl = r.dsl.unwrap();
        let file = parse(&dsl).unwrap();
        let Quest::Concrete(q) = &file.quests[0] else { panic!() };
        let body = &q.states[0].events[0].body;
        let Stmt::Action { action, capture } = &body[0] else { panic!() };
        assert_eq!(action.name, ActionName::Select);
        assert_eq!(capture.as_deref(), Some("choice"));
        let Stmt::Branch(b) = &body[1] else { panic!() };
        let Stmt::Action { action, .. } = &b.body[0] else { panic!() };
        assert_eq!(action.name, ActionName::Warp);
        let Stmt::Branch(el) = &body[2] else { panic!() };
        assert!(el.condition.is_none());
        let Stmt::Action { action, .. } = &el.body[0] else { panic!() };
        assert_eq!(action.name, ActionName::Return);
    }

    #[test]
    fn unmapped_constructs_are_reported_not_fatal() {
        let src = "\
define ENABLE_SAY false
quest give_basic_weapon begin
\tstate start begin
\t\twhen login begin
\t\t\tif pc.getqf(\"basic_weapon\") != 0 then
\t\t\t\treturn
\t\t\tend
\t\t\tlocal itemJob = { {19, 1}, {3009, 1} }
\t\t\tfor _, elem in ipairs(itemJob[pc.job]) do
\t\t\t\tpc.give_item2(elem[1], elem[2])
\t\t\tend
\t\t\tpc.setqf(\"basic_weapon\", 1)
\t\t\tset_state(__COMPLETE__)
\t\tend
\tend
end
";
        let r = convert_file("systems/give_basic_weapon.quest", src);
        assert!(r.ok, "{:?}", r.error);
        let items: Vec<&str> = r.unmapped.iter().map(|u| u.item.as_str()).collect();
        assert!(items.iter().any(|i| i.starts_with("for:")), "{items:?}");
        assert!(items.iter().any(|i| i.starts_with("local:itemJob:")), "{items:?}");
        // The mapped statements survive.
        let dsl = r.dsl.unwrap();
        assert!(dsl.contains("get_qf(\"basic_weapon\") != 0"), "{dsl}");
        assert!(dsl.contains("-> return"), "{dsl}");
        assert!(dsl.contains("set_qf(\"basic_weapon\", 1)"), "{dsl}");
    }

    #[test]
    fn skips_events_with_unmapped_triggers() {
        let src = "\
quest levelup begin
\tstate start begin
\t\twhen kill begin
\t\t\tpc.setqf(\"select\", 1)
\t\tend
\t\twhen button begin
\t\t\tsay(gameforge.levelup._140_sayTitle)
\t\tend
\tend
end
";
        let r = convert_file("story/levelup.quest", src);
        assert!(r.ok, "{:?}", r.error);
        assert!(r.unmapped.iter().any(|u| u.item.starts_with("trigger:kill")), "{:?}", r.unmapped);
        let file = parse(&r.dsl.clone().unwrap()).unwrap();
        let Quest::Concrete(q) = &file.quests[0] else { panic!() };
        // The kill event was skipped; the button event survived.
        assert_eq!(q.states[0].events.len(), 1);
    }

    #[test]
    fn detects_family_groups() {
        // Corpus form: file paths with .quest extension and subdirectories.
        let names = vec![
            "biology/collect_quest_lv30.quest".to_string(),
            "biology/collect_quest_lv40.quest".to_string(),
            "biology/collect_quest_lv96.quest".to_string(),
            "systems/check_collect_reward.quest".to_string(),
            "story/subquest_01.quest".to_string(),
            "story/subquest_02.quest".to_string(),
            "story/levelup.quest".to_string(),
        ];
        let props = detect_families(&names);
        assert_eq!(props.len(), 2);
        let cq = props.iter().find(|p| p.name == "collect_quest").unwrap();
        assert_eq!(cq.param, "level");
        assert_eq!(cq.members.len(), 3);
        let sq = props.iter().find(|p| p.name == "subquest").unwrap();
        assert_eq!(sq.param, "index");
        assert_eq!(sq.members.len(), 2);
    }

    #[test]
    fn corpus_report_top_unmapped() {
        let rep = CorpusReport {
            unmapped: vec![
                Unmapped::new("a", "call:pc.job"),
                Unmapped::new("b", "call:pc.job"),
                Unmapped::new("c", "call:syschat"),
                Unmapped::new("d", "trigger:kill (...)"),
            ],
            ..CorpusReport::default()
        };
        let top = rep.top_unmapped_calls(10);
        assert_eq!(top[0], ("pc.job".to_string(), 2));
        assert_eq!(top[1], ("syschat".to_string(), 1));
    }

    // -- family extraction through the converter (parity harness, spec §9.5) --

    /// Two legacy quests in the corpus style: `collect_quest_lv30/40` —
    /// identical structure, literals that vary (level, mob, herb, drug) and a
    /// name-embedded locale key (`pc.setqf` with an expression value too).
    fn collect_quest(n: i64, mob: i64, herb: i64, drug: i64) -> String {
        format!(
            "\
quest collect_quest_lv{n} begin
\tstate start begin
\t\twhen login or levelup with pc.level >= {n} begin
\t\t\tset_state(information)
\t\tend
\tend
\tstate information begin
\t\twhen letter begin
\t\t\tsend_letter(gameforge.collect_quest_lv{n}._10_sendLetter)
\t\tend
\t\twhen {mob}.kill begin
\t\t\tlocal s = number(1, 100)
\t\t\tif s <= 5 then
\t\t\t\tpc.give_item2({herb}, 1)
\t\t\tend
\t\tend
\t\twhen 20084.chat begin
\t\t\tpc.remove_item({drug}, 1)
\t\t\tpc.setqf(\"duration\", get_time()+60*60*22)
\t\t\tset_state(go_to_disciple)
\t\tend
\tend
end
"
        )
    }

    #[test]
    fn converts_set_qf_and_affect_with_expr_args() {
        let r = convert_file("systems/timers.quest", &collect_quest(30, 601, 30006, 71035));
        assert!(r.ok, "{:?}", r.error);
        assert!(r.unmapped.is_empty(), "{:?}", r.unmapped);
        let dsl = r.dsl.unwrap();
        let file = parse(&dsl).unwrap();
        let Quest::Concrete(q) = &file.quests[0] else { panic!() };
        // The chat event: remove_item, set_qf(expr), set_state.
        let body = &q.states[1].events[2].body;
        let Stmt::Action { action, .. } = &body[1] else { panic!() };
        assert_eq!(action.name, ActionName::SetQf);
        assert!(matches!(&action.args[1], Value::Expr(e) if matches!(e.as_ref(), Expr::Add(_, _))), "{:?}", action.args);
        assert!(dsl.contains("set_qf(\"duration\", get_time + 60 * 60 * 22)"), "{dsl}");
    }

    #[test]
    fn family_extraction_parity_through_converter() {
        let r30 = convert_file("biology/collect_quest_lv30.quest", &collect_quest(30, 601, 30006, 71035));
        let r40 = convert_file("biology/collect_quest_lv40.quest", &collect_quest(40, 602, 30007, 71036));
        assert!(r30.ok, "{:?}", r30.error);
        assert!(r40.ok, "{:?}", r40.error);
        assert!(r30.unmapped.is_empty() && r40.unmapped.is_empty());
        let file30 = parse(&r30.dsl.unwrap()).unwrap();
        let file40 = parse(&r40.dsl.unwrap()).unwrap();
        let Quest::Concrete(d30) = &file30.quests[0] else { panic!() };
        let Quest::Concrete(d40) = &file40.quests[0] else { panic!() };

        let ext = extract_family_params("collect_quest", "level", &[d30.clone(), d40.clone()]).unwrap();
        assert!(ext.excluded.is_empty());
        let Quest::Family { name, params, .. } = &ext.family else { panic!("family") };
        assert_eq!(name, "collect_quest");
        let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["level", "p1", "p2", "p3", "p4"], "{names:?}");

        // Family file: template + instances — must render, reparse and
        // expand to EXACTLY the converted originals (parity harness).
        let mut quests = vec![ext.family.clone()];
        quests.extend(ext.instances.clone().into_iter().map(Quest::Instance));
        let text = render(&QuestFile { imports: vec![], blocks: vec![], quests });
        let file = parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        let expanded = expand_families(&file).unwrap();
        assert_eq!(expanded.len(), 2);
        assert_eq!(&expanded[0], d30, "lv30 parity");
        assert_eq!(&expanded[1], d40, "lv40 parity");
        // The rendered family file mentions the (param) refs.
        assert!(text.contains("on (p3).kill"), "{text}");
        assert!(text.contains("pc.level >= (level)"), "{text}");
    }

    #[test]
    fn convert_corpus_emits_family_files() {
        let files = vec![
            ("biology/collect_quest_lv30.quest".to_string(), collect_quest(30, 601, 30006, 71035)),
            ("biology/collect_quest_lv40.quest".to_string(), collect_quest(40, 602, 30007, 71036)),
        ];
        let (outputs, report) = convert_corpus(&files);
        assert_eq!(report.failed, vec![]);
        assert_eq!(report.families.len(), 1);
        assert_eq!(report.family_failed, vec![]);
        assert_eq!(report.family_outputs.len(), 1);
        let f = &report.family_outputs[0];
        assert_eq!(f.name, "collect_quest");
        assert_eq!(f.file, "biology/collect_quest.family.quest");
        assert!(f.parity_ok, "expansión != originales");
        // The family file is part of the outputs (written by the CLI).
        assert!(outputs.iter().any(|(path, _)| path == &f.file));
        assert!(outputs.iter().any(|(path, _)| path == "biology/collect_quest_lv30.quest"));
        // The family text contains the template + both instances.
        assert!(f.text.contains("quest collect_quest family (level, p1, p2, p3, p4)"), "{}", f.text);
        assert!(f.text.contains("quest collect_quest_lv30 = collect_quest(level: 30"), "{}", f.text);
        assert!(f.text.contains("quest collect_quest_lv40 = collect_quest(level: 40"), "{}", f.text);
    }

    /// `collect_quest(..)` + an extra `state __giveup__` — the REAL-corpus
    /// shape (lv30 has states/events the other levels lack).
    fn collect_quest_with_giveup(n: i64, mob: i64, herb: i64, drug: i64) -> String {
        let base = collect_quest(n, mob, herb, drug);
        let giveup = "\n\tstate __giveup__ begin\n\t\twhen letter begin\n\t\t\tclear_letter()\n\t\tend\n\tend\n";
        format!("{}{}end\n", base.strip_suffix("end\n").unwrap(), giveup)
    }

    /// When the strict extraction rejects a name group (structure differs),
    /// the similarity layer (spec §9.3) proposes instead: score + params +
    /// structural deltas — never a silent failure.
    #[test]
    fn similarity_proposals_when_extraction_rejects() {
        let files = vec![
            ("biology/collect_quest_lv30.quest".to_string(), collect_quest(30, 601, 30006, 71035)),
            ("biology/collect_quest_lv40.quest".to_string(), collect_quest_with_giveup(40, 602, 30007, 71036)),
        ];
        let (_, report) = convert_corpus(&files);
        assert_eq!(report.failed, vec![]);
        // Strict gate: no family file (structure differs), no silent failure.
        assert!(report.family_outputs.is_empty());
        assert_eq!(report.family_failed, vec![]);
        // Similarity layer: one proposal for the rejected group.
        assert_eq!(report.similarity.len(), 1);
        let p = &report.similarity[0];
        assert_eq!(p.name, "collect_quest");
        assert_eq!(p.members.len(), 2);
        assert!(p.mean > 0.5, "score medio demasiado bajo: {p:?}");
        assert!(p.min <= p.mean && p.min > 0.0, "{p:?}");
        assert!(p.common > 0);
        // The varying literal slots are the family params (level included).
        assert!(
            p.params.iter().any(|(path, _)| path.ends_with("cond/r")),
            "el slot del nivel debería variar: {:?}",
            p.params
        );
        // The extra `__giveup__` state shows up as a structural delta.
        assert!(
            p.deltas.iter().any(|d| d.contains("__giveup__")),
            "el delta estructural del __giveup__ debería aparecer: {:?}",
            p.deltas
        );
    }

    /// Real-corpus similarity evidence (spec §9.3): the 6 name groups are
    /// structurally diverse (the strict gate rejects them) — the similarity
    /// layer must produce a ranked proposal for EACH one, with real scores.
    #[test]
    fn real_corpus_similarity_scores() {
        let Some(dir) = find_corpus_dir() else {
            eprintln!("corpus de quests no accesible — test condicional omitido");
            return;
        };
        let mut files: Vec<(String, String)> = Vec::new();
        collect_quest_files(&dir, &dir, &mut files);
        if files.len() < 10 {
            eprintln!("corpus incompleto en {} — omitido", dir.display());
            return;
        }
        let (_, report) = convert_corpus(&files);
        assert!(report.failed.is_empty(), "conversión fallida: {:?}", report.failed);
        // Every rejected name group got a similarity proposal.
        let rejected: Vec<&str> = report.families.iter().map(|f| f.name.as_str()).collect();
        assert!(!rejected.is_empty());
        for name in &rejected {
            assert!(
                report.similarity.iter().any(|p| p.name == *name),
                "sin propuesta de similitud para el grupo {name}"
            );
        }
        // The collect_quest proposal is the ground truth for the report.
        let cq = report.similarity.iter().find(|p| p.name == "collect_quest").expect("propuesta collect_quest");
        assert!(cq.members.len() >= 2);
        assert!((0.0..=1.0).contains(&cq.mean));
        eprintln!(
            "collect_quest: {} miembros, mean {:.3}, min {:.3}, {} slots comunes, {} params",
            cq.members.len(),
            cq.mean,
            cq.min,
            cq.common,
            cq.params.len()
        );
        for g in &report.similarity {
            eprintln!("grupo {}: {} miembros, mean {:.3}, min {:.3}, {} params", g.name, g.members.len(), g.mean, g.min, g.params.len());
        }
    }

    /// Real-corpus evidence (spec §9.5): runs the full conversion +
    /// extraction + expansion on the deployed corpus when reachable.
    ///
    /// The REAL collect_quest_lv30..96 files are NOT structurally identical
    /// (they evolved per level — different states/events; verified 2026-08-13:
    /// all converted quests have unique slot-path sets). Strict family
    /// extraction therefore correctly reports "no family" for them — the
    /// template-building similarity engine is spec §9.3 (future slice).
    /// The test asserts the honest contract: either a family IS extracted
    /// with full parity, or the extraction fails with the structural-diversity
    /// reason (surfaced as `family_failed` in the report).
    #[test]
    fn real_corpus_collect_quest_family_parity() {
        let Some(dir) = find_corpus_dir() else {
            eprintln!("corpus de quests no accesible — test condicional omitido");
            return;
        };
        let mut files: Vec<(String, String)> = Vec::new();
        collect_quest_files(&dir, &dir, &mut files);
        files.retain(|(f, _)| {
            let stem = f.rsplit('/').next().unwrap_or(f);
            stem.starts_with("collect_quest_lv") && stem.ends_with(".quest")
        });
        files.sort_by(|a, b| a.0.cmp(&b.0));
        if files.len() < 2 {
            eprintln!("grupo collect_quest_lv* no encontrado en {} — omitido", dir.display());
            return;
        }
        let (outputs, report) = convert_corpus(&files);
        assert_eq!(report.failed, vec![], "conversión del grupo real falló: {:?}", report.failed);
        let mut members: Vec<QuestDef> = Vec::new();
        for (f, dsl) in &outputs {
            if f.ends_with(".family.quest") {
                continue;
            }
            if let Some(Quest::Concrete(qd)) = parse(dsl).ok().and_then(|f| f.quests.into_iter().next()) {
                members.push(qd);
            }
        }
        assert!(members.len() >= 2, "grupo real sin miembros convertidos");
        match extract_family_params("collect_quest", "level", &members) {
            Ok(ext) => {
                // A structurally-uniform subset EXISTS: verify full parity.
                eprintln!("miembros usables: {} — excluidos: {:?}", ext.instances.len(), ext.excluded);
                assert!(ext.instances.len() >= 2, "grupo real demasiado pequeño: {:?}", ext.excluded);
                let mut quests = vec![ext.family.clone()];
                quests.extend(ext.instances.clone().into_iter().map(Quest::Instance));
                let text = render(&QuestFile { imports: vec![], blocks: vec![], quests });
                let file = parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
                let expanded = expand_families(&file).expect("expansión de la familia real");
                let usable: Vec<&QuestDef> = members
                    .iter()
                    .filter(|m| !ext.excluded.iter().any(|(n, _)| n == &m.name))
                    .collect();
                assert_eq!(expanded.len(), usable.len());
                for (e, u) in expanded.iter().zip(&usable) {
                    assert_eq!(e, *u, "parity del miembro {}", u.name);
                }
            }
            Err(e) => {
                // Honest reality: the real family files are NOT template-identical.
                assert!(e.contains("estructura"), "error de extracción inesperado: {e}");
                let unique: usize = {
                    let mut seen = std::collections::BTreeSet::new();
                    for m in &members {
                        seen.insert(crate::family::collect_slots_public(m).into_keys().collect::<Vec<_>>());
                    }
                    seen.len()
                };
                eprintln!("grupo real estructuralmente diverso — extracción correctamente rechazada: {e}");
                eprintln!(
                    "miembros convertidos: {} — fingerprints de estructura únicos: {} (similitud = spec §9.3)",
                    members.len(),
                    unique
                );
            }
        }
    }

    fn find_corpus_dir() -> Option<std::path::PathBuf> {
        let candidates = [
            std::env::var_os("QUEST_CORPUS").map(std::path::PathBuf::from),
            Some(std::path::PathBuf::from(
                "C:\\projects\\Metin2\\source\\deploy\\main\\srv1\\share\\locale\\germany\\quest",
            )),
            Some(std::path::PathBuf::from(
                "C:\\projects\\Metin2\\source\\deploy\\main\\srv1\\share\\locale\\spain\\quest",
            )),
        ];
        candidates.into_iter().flatten().find(|p| p.is_dir())
    }

    fn collect_quest_files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                collect_quest_files(root, &p, out);
            } else if p.extension().is_some_and(|e| e == "quest") {
                let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                // Lossy: legacy files mix CP949 bytes (comments) — the CLI
                // does the same (read + from_utf8_lossy).
                if let Ok(bytes) = std::fs::read(&p) {
                    out.push((rel, String::from_utf8_lossy(&bytes).into_owned()));
                }
            }
        }
    }
}






