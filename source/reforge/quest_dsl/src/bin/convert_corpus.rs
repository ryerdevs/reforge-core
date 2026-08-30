//! Corpus converter CLI: converts every legacy `.quest` under a directory to
//! the DSL and writes the discrepancy report.
//!
//! Usage: `convert_corpus <quest-dir> <out-dir>`
//!
//! Output: the DSL files under `<out-dir>` (same relative paths, `.quest`
//! extension) + `<out-dir>/conversion_report.txt` with the summary,
//! unmapped items and family proposals. Exit code 0 unless no files were
//! read.

use std::path::{Path, PathBuf};

use quest_dsl::convert;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("uso: convert_corpus <quest-dir> <out-dir>");
        std::process::exit(2);
    }
    let quest_dir = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[2]);

    let mut paths = Vec::new();
    collect_quests(&quest_dir, &mut paths);
    if paths.is_empty() {
        eprintln!("sin archivos .quest en {}", quest_dir.display());
        std::process::exit(1);
    }

    let files: Vec<(String, String)> = paths
        .iter()
        .map(|p| {
            let rel = p
                .strip_prefix(&quest_dir)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/");
            let text = match std::fs::read(p) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(e) => {
                    eprintln!("no se pudo leer {}: {e}", p.display());
                    String::new()
                }
            };
            (rel, text)
        })
        .collect();

    let (outputs, report) = convert::convert_corpus(&files);

    let _ = std::fs::create_dir_all(&out_dir);
    for (rel, dsl) in &outputs {
        let out_path = out_dir.join(rel);
        if let Some(parent) = out_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&out_path, dsl) {
            eprintln!("no se pudo escribir {}: {e}", out_path.display());
        }
    }

    let mut rep = String::new();
    rep.push_str(&format!(
        "convertidos: {} / {}\nfallidos: {}\nsin mapear: {} items\n",
        report.converted.len(),
        files.len(),
        report.failed.len(),
        report.unmapped.len()
    ));
    if !report.failed.is_empty() {
        rep.push_str("\n== FALLIDOS ==\n");
        for (f, e) in &report.failed {
            rep.push_str(&format!("{f}: {e}\n"));
        }
    }
    let top = report.top_unmapped_calls(10);
    if !top.is_empty() {
        rep.push_str("\n== TOP-10 LLAMADAS SIN MAPEAR ==\n");
        for (name, n) in top {
            rep.push_str(&format!("{name}: {n}\n"));
        }
    }
    if !report.families.is_empty() {
        rep.push_str("\n== FAMILIAS PROPUESTAS (requieren confirmación humana) ==\n");
        for f in &report.families {
            rep.push_str(&format!(
                "{} (param {}) — {} miembros\n",
                f.name,
                f.param,
                f.members.join(", ")
            ));
        }
    }
    if !report.family_outputs.is_empty() {
        rep.push_str("\n== FAMILIAS EXTRAÍDAS (archivos escritos) ==\n");
        for f in &report.family_outputs {
            rep.push_str(&format!(
                "{} -> {} (parity: {})\n",
                f.name,
                f.file,
                if f.parity_ok { "OK" } else { "FALLO" }
            ));
            for (m, why) in &f.excluded {
                rep.push_str(&format!("  excluido: {m} ({why})\n"));
            }
        }
    }
    if !report.family_failed.is_empty() {
        rep.push_str("\n== FAMILIAS NO EXTRAÍDAS ==\n");
        for (n, why) in &report.family_failed {
            rep.push_str(&format!("{n}: {why}\n"));
        }
    }
    if !report.similarity.is_empty() {
        rep.push_str("\n== FAMILIAS SIMILARES (spec §9.3 — propuestas, requieren merge) ==\n");
        for p in &report.similarity {
            rep.push_str(&format!(
                "{} — {} miembros, score medio {:.3}, mínimo {:.3}, {} slots comunes, {} params\n",
                p.name,
                p.members.len(),
                p.mean,
                p.min,
                p.common,
                p.params.len()
            ));
        }
    }
    if !report.unmapped.is_empty() {
        rep.push_str("\n== ITEMS SIN MAPEAR ==\n");
        for u in &report.unmapped {
            rep.push_str(&format!("{}: {}\n", u.file, u.item));
        }
    }
    let _ = std::fs::write(out_dir.join("conversion_report.txt"), &rep);
    write_family_proposals(&out_dir, &report);
    print!("{rep}");
}

/// `family_proposals.txt`: the full similarity analysis of the rejected
/// name groups — scores, the varying slots (params) and the structural
/// deltas of the most similar pair (spec §9.3 groundwork for the merge
/// engine).
fn write_family_proposals(out_dir: &std::path::Path, report: &convert::CorpusReport) {
    if report.similarity.is_empty() {
        return;
    }
    let mut txt = String::from("== FAMILY PROPOSALS (spec §9.3) ==\n");
    for p in &report.similarity {
        txt.push_str(&format!(
            "\n## {} (suffix param: {})\n",
            p.name, p.suffix_param
        ));
        txt.push_str(&format!(
            "members ({}): {}\n",
            p.members.len(),
            p.members.join(", ")
        ));
        txt.push_str(&format!(
            "score: mean {:.3}, min {:.3} — common slots: {}\n",
            p.mean, p.min, p.common
        ));
        if !p.params.is_empty() {
            txt.push_str("params (slots that vary across members):\n");
            for (path, vals) in &p.params {
                let vs: Vec<String> = vals.iter().map(quest_dsl::render::render_value).collect();
                txt.push_str(&format!("  {path}: {}\n", vs.join(" | ")));
            }
        }
        if !p.deltas.is_empty() {
            txt.push_str("structural deltas (most similar pair):\n");
            for d in &p.deltas {
                txt.push_str(&format!("  {d}\n"));
            }
        }
    }
    let _ = std::fs::write(out_dir.join("family_proposals.txt"), &txt);
}

fn collect_quests(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_quests(&p, out);
        } else if p.extension().is_some_and(|e| e == "quest") {
            out.push(p);
        }
    }
}
