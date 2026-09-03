---
Type: Reference
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-09-02
---

# reforge-core Agent Instructions

## Mission

**The compatible server is rewritten in Rust, module by module.** Less code, less complexity, fewer dependencies — more clarity and performance. Each Rust module preserves observable behavior and passes verification before the next one starts. Structural redesign, not line-by-line translation. Domain, protocol, concurrency and migration decisions are ADRs before code.

## Public repository boundary (ADR-0015)

- Public: authored Rust server in `source/reforge` + docs, scripts, metadata to build and verify it.
- Not public: client source, pack assets, generated client binaries, frozen C++ oracle in `source/server`. Never add them to the index or describe them as build inputs.
- Real-client checks use an external compatible client (operator prerequisite, `C:\projects\metin2-extra\client` locally). Keep it out of this repo.
- F7 standalone Rust client is deferred outside this repo. ADR-0015 supersedes ADR-0013.

## Live sources of truth

Read at session start, update at close. If they disagree, precedence is `documentation/README.md#document-authority-and-precedence`:

1. Fresh `scripts/verify.ps1` output (`OK: verificacion completa`)
2. `documentation/plans/gap-registry.md` — per-row owner/state/evidence/exit
3. `documentation/progress.md` — handoff (HEAD, deploy, gate, open rows)
4. ADRs → summaries → `documentation/history/` (read-only)

## Runtime

- **Primary (daily):** native Windows. PostgreSQL 18.4 `127.0.0.1:5432` (service `postgresql-metin2`, role `mt2`), Rust auth `127.0.0.1:30001`, Rust channel `127.0.0.1:30003`. `scripts/start_win.ps1` / `scripts/stop_win.ps1` / `scripts/status.ps1`. Logs `source/deploy/win/logs/`.
- **Parity (on-demand):** WSL `Debian-M2`, cap 2 GB, off when unused. Frozen C++ oracle + `mysql_proxy` only. Parity sessions via `scripts/start_m2_min.sh` and `scripts/gpg/parity_boot.sh`. `wsl --shutdown` after. Full delete at F6.
- **Coordinate convention:** DB `player.x/y` are **UNITS** (village c1 = `969600, 278400`). `AddGotoInfo` values are cells (÷100). See `documentation/rules.md` and `documentation/history/guardrails/world-entry-crash.md`.

## Frozen oracle

The C++ server in `source/server` is **NEVER rebuilt** (ADR-0012). Linux ELF `game_r41023`/`db_r41023` are parity oracles only. After any oracle touch in WSL, `sync` + `md5sum` verify before shutdown.

## Verification gate

```
scripts/verify.py   →  check_boundary.py
                    +  check_path_contract.py
                    +  check_docs.py
                    +  cargo fmt --check
                    +  cargo test --workspace
                    +  cargo clippy --workspace --all-targets -- -D warnings
                    +  git diff --check
                    =  OK: verificacion completa
```

GitHub `docs.yml` runs the same + `scripts/check_docs.py` metadata gate + handoff check (`source/reforge` touched ⇒ `progress.md` + `CHANGELOG.md` + `gap-registry.md` updated).

## Workflow rules

1. **Read before write.** `AGENTS.md` + nearest `AGENTS.md` + `progress.md`/`gap-registry.md`.
2. **Inspect before edit.** Source, build, runtime state first. Declare scope; preserve unrelated changes.
3. **Minimal + justified.** One line before fifty, stdlib before dep, YAGNI. Never hide warnings undocumented.
4. **Verify proportionally.** Inspection → focused check → build/run. Report real command output; no success claims without evidence.
5. **Never rebuild frozen C++.** Two-copy sync rule is dead.
6. **Confirm destructive ops.** Deleting volumes/DBs/caches needs explicit user OK. Use `python scripts/clean.py --what-if` first.
7. **Docs after every change.** Update canonical docs + `Last verified`; list exact required doc updates if outside lane scope. ADRs before architecture.
8. **Log the change.** `CHANGELOG.md` (Keep a Changelog), `documentation/roadmap.md`, ADRs. Never end with unlogged changes.
9. **Parallelize.** Independent lanes via `@explorer` / `@librarian` / `@oracle` / `@fixer` in background; `source/reforge` graph first (`graphify query`); reconcile before next step.
10. **Plan before code.** Architecture/rewrite → discuss alternatives/risks + ADR, then implement. Devil's advocate on every user plan.
11. **Ponytail always.** Do more with less; small is consequence of necessary, not of trimming. Never cut validation/security/accessibility.
12. **Never block chat.** Commands >15s → `Start-Process -RedirectStandardOutput` or background task, then end turn. Verify next turn with <10s checks. Never chain stop/copy/start/verify in one `;` call.
13. **Commits.** Conventional Commits in English, atomic, imperative, no `Co-authored-by:` trailer. Single author `ryer <82473243+ryerdevs@users.noreply.github.com>`. No history rewrites after push.
14. **Automate in scripts.** Repeatable cross-platform tooling → `scripts/*.py` (`manage.py`, `bootstrap_db.py`, `package.py`). Don't improvise.
15. **Single handoff.** `documentation/progress.md` is the session handoff. Read at start, update at close.
16. **Tests that catch bugs.** Every fix needs a mutation test (fails if reverted) + `proptest` where invariants exist.

## OpenCode 2 subagent lifecycle

- Start an independent lane only through the OpenCode subagent mechanism, with
  one bounded objective, explicit file ownership, and a validation owner.
- Treat a child as live until the scheduler reports a terminal result. Do not
  resume, re-prompt, or duplicate a child merely to check its status; reconcile
  its result first. If task tracking is `unconfirmed`, stop scheduling that
  lane and diagnose the service/session state instead of guessing.
- To stop a background child, run `opencode2 api post
  /api/session/<session-id>/interrupt`, then verify both `/api/session/active`
  and `/api/session/<session-id>`. Never use a second child as a workaround for
  an uncertain first child.
- The OpenCode service owns child-session lifecycle. Project instructions guide
  agents; scripts and CI enforce repository policy. See the V2
  [agents](https://opencode.ai/v2/docs/agents/) and
  [API](https://opencode.ai/v2/docs/api/) references.

> `OPENCODE_EXPERIMENTAL_BACKGROUND_SUBAGENTS=true` in `~/.config/opencode/opencode.jsonc` keeps lane tasks backgrounded.

## Documentation

Hub & Authority: `documentation/README.md`. Phase map: `documentation/roadmap.md`. DB: `documentation/schema.md`. Guardrails: `documentation/rules.md`. Wire: `documentation/reference/login-flow.md`. ADRs: `documentation/adr/` (17 ADRs, ADR-0013 superseded by ADR-0015). History: `documentation/history/` (indexed in `documentation/progress.md#historical-archive-index`).

Workspace: `source/reforge` (flat layout, `unsafe_code = "forbid"`, `server_realms` single binary roles `auth|channel` by config — ADR-0003/0004). Members: `protocol`, `network`, `database`, `game_core`, `server_realms`, `mysql_proxy`, `locale_import`, `bench_bot`, `quest_dsl`, `admin_tui`.

Historical narratives (login fix chain, locale CP949, World-entry crash `0xc0000374`) are preserved in `documentation/history/` and `CHANGELOG.md` — not repeated here.

## Guardrails for the Rust rewrite

- Don't mix modernization into oracle work.
- `game`+`db` unified per ADR-0002 (one process per region, `database` crate; legacy shim thin).
- Keep oracle stable while porting; verify parity per module.
- Explicit adapters only: peer protocol shim (ADR-0002), PG cutover `mysql_proxy` (ADR-0005), `protocol::legacy` PanamaPack/hybrid-crypt (ADR-0006, deleted at new client), no partial Rust in legacy client F0-F6 (ADR-0007).
- Current ADRs: 0008 data layer (tokio-postgres 0.7 + WAL durable), 0009 server-side locale, 0010 domain boundaries (bevy_ecs World), 0011 anti-hack, 0012 Windows-native + WSL on-demand, 0014 five stat points/level, 0015 Rust-only public boundary, 0016 quest DSL, 0017 regional channels deferred.
