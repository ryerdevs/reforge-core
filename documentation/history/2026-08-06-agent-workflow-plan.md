# Metin2 Agent Skills and AGENTS.md Implementation Plan

> **Metadata**
> - Type: History
> - Status: Historical
> - Audience: Project agents and maintainers (historical context only)
> - Last verified: 2026-08-10
> - Original location: `docs/superpowers/plans/2026-08-06-metin2-agent-skills-and-agents-md.md`
> - **Historical record.** Archived for context. This document is NOT current normative guidance: it describes the 2026-08-06 plan to set up project skills and a root `AGENTS.md` (the workspace had no Git repository at the time). The current project rules live in `AGENTS.md`; the skills installed at the time are superseded by the current project skill set.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a project-local skill set and durable agent instructions for the Alpine Linux compatibility baseline and the future Rust server rewrite.

**Architecture:** Use the `skills.sh` CLI from the workspace root with project scope, targeting Codex and Command Code. Keep discovery, C++/Linux/Alpine/Docker/build/debugging skills separate from durable project policy in a root `AGENTS.md`; reuse existing Codex skills for architecture, documentation, planning, and verification when they already cover the need.

**Tech Stack:** `npx skills`, Codex project skills, Command Code project skills, C++, Make, CMake, Alpine Linux, musl libc, Docker Compose, MySQL, future Rust.

---

### Task 1: Establish the durable project guidance

**Files:**
- Create: `AGENTS.md`
- Reference: `server/`, `server/linux/`, `server/metin2_server+src/`, `.commandcode/`

- [ ] **Step 1: Record the current baseline**

Write `AGENTS.md` with the following verified project facts: the legacy server is C++, the workspace contains a Linux/CMake area and older Makefiles, Alpine Linux is the immediate target, BSD is a compatibility reference being phased out, and the root currently has no Git repository.

- [ ] **Step 2: Record the two-horizon roadmap**

Add explicit rules that the current phase preserves and documents the existing C++ behavior while making it buildable and runnable in Alpine/Docker, and that the later phase is a structural Rust rewrite rather than a line-by-line translation.

- [ ] **Step 3: Record architectural guardrails**

Document that the future design may unify the current `game` and `db` responsibilities, but that process boundaries, data ownership, protocols, and migration checkpoints must be decided through architecture documents and ADRs before implementation.

- [ ] **Step 4: Record agent operating rules**

Document the required workflow: inspect before modifying, prefer reproducible commands, separate compatibility fixes from modernization, verify build/runtime changes, document architectural decisions, and request confirmation before destructive Docker or data operations.

- [ ] **Step 5: Review the file**

Run:

```powershell
Get-Content -Raw .\AGENTS.md
rg -n "Alpine|musl|Docker|C\\+\\+|Rust|game|db|ADR|destructive" .\AGENTS.md
```

Expected: the file contains the current baseline, the roadmap, the architecture guardrails, and the operating rules without references to BSD as a target or Rust as an immediate implementation requirement.

### Task 2: Install the discovery skill at project scope

**Files:**
- Create: `.agents/skills/find-skills/`
- Create: `.commandcode/skills/find-skills/`

- [ ] **Step 1: Install Vercel Labs `find-skills`**

Run from `C:\projects\Metin2`:

```powershell
npx skills add https://github.com/vercel-labs/skills --skill find-skills -a codex -a command-code -y
```

Expected: the CLI installs `find-skills` into the project skill directories for Codex and Command Code and exits successfully.

- [ ] **Step 2: Verify the discovery skill**

Run:

```powershell
npx skills list
Get-ChildItem -Recurse .\.agents\skills\find-skills, .\.commandcode\skills\find-skills
```

Expected: both agent targets expose a `SKILL.md` for `find-skills` and the listing does not report a global-only installation.

### Task 3: Discover and vet the domain skills

**Files:**
- Read: `.agents/skills/find-skills/SKILL.md`
- Read: `.commandcode/skills/find-skills/SKILL.md`
- Create: project skill directories only for accepted results from the discovery output

- [ ] **Step 1: Search the required domains**

Run these searches from the workspace root:

```powershell
npx skills find "c++"
npx skills find "linux"
npx skills find "alpine linux"
npx skills find "docker compose"
npx skills find "cmake make"
npx skills find "debugging build errors"
```

Expected: each command returns named skills or reports that no exact result exists; no installation happens during this search step.

- [ ] **Step 2: Prefer Alpine-specific guidance**

From the search results, prefer a skill whose instructions explicitly cover Alpine, musl, `apk`, minimal images, native compilation, or WSL/container interactions. If no Alpine-specific skill exists, select a Linux/container skill that documents Debian-versus-Alpine and glibc-versus-musl differences rather than assuming a generic Linux host.

- [ ] **Step 3: Reject unsafe or redundant candidates**

Reject candidates that install unrelated tooling, run destructive cleanup automatically, modify source code without an explicit request, or duplicate a stronger Codex skill already available for debugging, architecture, documentation, planning, or verification.

- [ ] **Step 4: Install only accepted candidates**

For each accepted result, run the exact `npx skills add` command shown by the discovery result, adding `-a codex -a command-code -y` and keeping project scope. Do not use the wildcard skill selector.

Expected: only accepted C++, Linux/Alpine, Docker/Compose, Make/CMake, and debugging skills are installed in the project.

### Task 4: Verify the project skill set

**Files:**
- Read: all project-local `SKILL.md` files under `.agents/skills/` and `.commandcode/skills/`

- [ ] **Step 1: List installed skills by agent**

Run:

```powershell
npx skills list
Get-ChildItem -Directory .\.agents\skills, .\.commandcode\skills | Select-Object FullName
```

Expected: the output shows the discovery skill and the accepted domain skills for both agents.

- [ ] **Step 2: Inspect every installed instruction file**

Run:

```powershell
Get-ChildItem -Recurse -Filter SKILL.md .\.agents\skills, .\.commandcode\skills | ForEach-Object { Write-Output "--- $($_.FullName)"; Get-Content -Raw $_.FullName }
```

Expected: each file has clear activation criteria, no unrelated project scope, and no instruction that conflicts with the root `AGENTS.md`.

- [ ] **Step 3: Check for duplicate installations**

Compare the skill names under both agent directories. Keep the same accepted set for Codex and Command Code, and report any agent-specific limitation rather than silently installing a different set.

- [ ] **Step 4: Confirm no runtime implementation occurred**

Run:

```powershell
rg --files -g 'Dockerfile*' -g 'docker-compose*' -g 'Cargo.toml' -g '*.cpp' -g '*.cc' -g '*.h' | Select-Object -First 200
```

Expected: this setup phase has not modified C++ sources, Dockerfiles, Compose files, Alpine packages, database schemas, or Rust code.

### Task 5: Hand off to the next engineering phase

**Files:**
- Read: `AGENTS.md`
- Read: `docs/history/2026-08-06-project-skills-revision.md`

- [ ] **Step 1: Report the installed set**

Summarize each installed skill, its agent targets, and the reason it was accepted. List rejected or unavailable Alpine-specific candidates separately.

- [ ] **Step 2: Define the next implementation boundary**

Use the installed guidance to plan a separate Alpine build/container task. That task must first inventory compiler, linker, runtime-library, package, port, and database dependencies before creating the Docker Compose stack.

- [ ] **Step 3: Preserve the Rust rewrite boundary**

Do not begin the Rust rewrite in the same task as the Alpine compatibility baseline. The Rust phase starts only after the C++ server has a reproducible build/run path and the current `game`/`db` behavior and data ownership are documented.

## Self-review

- The plan covers the approved Alpine/Docker compatibility baseline, project-local skills, root `AGENTS.md`, and the future Rust/game-db architecture direction.
- It does not include BSD installation, Rust implementation, source refactoring, database changes, or Docker runtime changes.
- All commands are read-only except the explicit `npx skills add` installation commands and creation of `AGENTS.md`.
- The workspace has no Git repository, so no commit step is included; changes must be reported for later version-control setup.
