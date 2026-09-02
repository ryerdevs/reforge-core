---
Type: Plan
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-09-02
---

# Alpha A0 truthful-baseline implementation plan

> **For agentic workers:** Execute one checked task at a time. Preserve the
> public boundary, record command output as evidence, and do not begin A1 until
> the A0 Oracle review accepts the reconciled state.

**Goal:** Establish one truthful, public-safe project baseline from which the
portable deploy, clean-clone setup, documentation reduction, and alpha release
work can proceed.

**Architecture:** A0 changes no gameplay behavior and ships no client material.
It separates live state from historical narrative, turns the known alpha
blockers into owned registry rows, classifies local artifacts/tools, and
commits or isolates the existing unrelated source edits. Later phases consume
these contracts rather than restating them.

**Tech stack:** Markdown, PowerShell, Git, existing documentation checks, and
the existing Rust workspace verification scripts.

---

## File map

| Path | Responsibility in A0 |
|---|---|
| `documentation/progress.md` | Current commit/deploy/gate snapshot and one concise handoff. |
| `documentation/plans/gap-registry.md` | Owned A0–A5 alpha blockers with evidence, dependencies, risk, and exit criteria. |
| `documentation/reference/document-authority.md` | The only precedence rule for current project state. |
| `documentation/DOCUMENTATION.md` | Documentation taxonomy and lifecycle policy aligned with that authority. |
| `documentation/README.md` | Navigation hub; links to the canonical live state and alpha plans only. |
| `documentation/history-index.md` | Read-only archive navigation and explicit non-authority notice. |
| `README.md`, `ROADMAP.md`, `CHANGELOG.md` | Lower-authority public summary/history surfaces; no live status claims. |
| `AGENTS.md`, `documentation/rules.md` | Concise agent/operational guardrails; no duplicated project history. |
| `.gitignore` | Local-agent/generated-artifact exclusions, with no deletion of user files. |
| `documentation/reference/public-tooling-boundary.md` (new) | Versioned inventory of public clean tools, external operator prerequisites, and disallowed material under ADR-0015. |

## Task 1: Capture reproducible baseline evidence

**Files:**
- Modify: `documentation/progress.md`
- Modify: `documentation/plans/gap-registry.md`
- Test: `scripts/status.ps1`, `scripts/verify.ps1`, `scripts/check_docs.ps1`

- [x] **Step 1: Capture repository and deploy facts without changing files.**

  Run:

  ```powershell
  git rev-parse HEAD
  git status --short
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/status.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check_docs.ps1
  ```

  Record the exact HEAD, tracked/untracked disposition, deploy executable hash
  (or its absence), listener state, and command outcomes. Do not infer a
  deployed hash from an old handoff entry.

- [x] **Step 2: Run the standard project verifier only after the snapshot is captured.**

  Run:

  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify.ps1
  ```

  Expected: `OK: verificacion completa`, or an exact failure with its owning
  blocker. The ignored PG/WSL leg is recorded as optional availability, never
  as a pass if its prerequisite is absent.

- [x] **Step 3: Replace stale live claims with evidence-linked current facts.**

  In `progress.md`, keep a short Current section that names the captured HEAD,
  last standard-gate result, deploy state, and one next action. Move no history
  into new prose; existing dated handoffs remain history. In the registry
  header, replace stale “tree state at verification” text with the same HEAD
  and a link to `progress.md` for volatile runtime facts.

- [x] **Step 4: Verify documentation and whitespace.**

  Run:

  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check_docs.ps1
  git diff --check
  ```

  Expected: `OK: check_docs (metadata + live state files)` and no whitespace
  errors.

- [x] **Step 5: Commit the truth snapshot.**

  ```powershell
  git add documentation/progress.md documentation/plans/gap-registry.md
  git commit -m "docs(status): reconcile alpha baseline evidence"
  ```

## Task 1R: Repair review-found truthfulness gaps

**Files:**
- Modify: `documentation/plans/alpha-a0-truthful-baseline.md`
- Modify: `documentation/progress.md`
- Modify: `documentation/plans/gap-registry.md`
- Modify: `README.md`
- Modify: `CHANGELOG.md`
- Test: `scripts/check_docs.ps1`, `git diff --check`

- [x] **Step 1: Correct the closed-storage contradiction.**

  The progress snapshot must not say disk storage is open while the registry
  closes `G0.2`. Keep `G0.2` closed in the registry and replace the stale
  progress claim with a link to the registry row and its dated evidence.

- [x] **Step 2: Remove current-runtime claims from the static README.**

  Replace every claim that ports are listening, the deploy binary equals HEAD,
  or the verifier currently passes with a link to the two live state sources:

  ```markdown
  Current verified state and deployment availability:
  [`documentation/progress.md`](documentation/progress.md) and
  [`documentation/plans/gap-registry.md`](documentation/plans/gap-registry.md).
  ```

  Retain the README's project description and supported-flow links. This is a
  truthfulness repair, not the broader entry-point reduction owned by Task 4.

- [x] **Step 3: Record the dated baseline failure as evidence, not a release claim.**

  Under `CHANGELOG.md` → `[Unreleased]` → `Changed`, add one dated sentence
  that the 2026-09-02 standard verifier stopped at the WAL PostgreSQL tests
  because `127.0.0.1:5432` was unavailable, and link to `progress.md` for
  volatile state. Do not call it a code regression or a passing gate.

- [x] **Step 4: Re-run the review-found checks.**

  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check_docs.ps1
  git diff --check
  ```

  Expected: documentation metadata/live-state checks and whitespace checks pass;
  all three Important Task-1 review findings are addressed.

- [x] **Step 5: Commit the bounded repair.**

  ```powershell
  git add documentation/plans/alpha-a0-truthful-baseline.md documentation/progress.md documentation/plans/gap-registry.md README.md CHANGELOG.md
  git commit -m "docs(status): repair baseline truth gaps"
  ```

## Task 2: Classify the current worktree and local artifacts

**Files:**
- Modify: `.gitignore`
- Modify: `documentation/progress.md`
- Modify: `documentation/plans/gap-registry.md`
- Test: Git index/status commands

- [x] **Step 1: Build a disposition table for every dirty path.**

  Run:

  ```powershell
  git status --short
  git diff --name-status
  git ls-files --others --exclude-standard
  ```

  For each path, record exactly one disposition in the A0 registry row:
  `commit as named slice`, `continue in named slice`, `ignore as local tool
  state`, or `requires owner decision`. Never use `git clean`, `reset --hard`,
  or recursive deletion in this task.

- [x] **Step 2: Keep generated agent artifacts local without deleting them.**

  Add only the missing root ignore entries for local agent execution state:

  ```gitignore
  /.superpowers/
  /docs/superpowers/
  ```

  Do not ignore `documentation/`, and do not create a new public `docs/` tree.
  The project policy reserves `documentation/` for current documentation.

- [x] **Step 3: Split source edits before any alpha contract work depends on them.**

  The current pending paths are `spawn.rs`, `map.rs`, `channel/entry.rs`, and
  `channel/session.rs`. Inspect each diff and group it into independently
  verifiable behavior slices. The map/world-entry safety changes and the spawn
  view change must not share a commit unless their acceptance criteria and
  documentation are the same. Each retained behavior change needs a focused
  test, a mutation/negative test where applicable, current-doc evidence, and a
  conventional commit.

- [x] **Step 4: Prove the release index excludes local artifacts.**

  Run:

  ```powershell
  git check-ignore -v .superpowers/sdd/ignore-sentinel docs/superpowers/ignore-sentinel
  git ls-files .superpowers docs/superpowers source/client source/server source/deploy/win
  ```

  Expected: the first command names `.gitignore`; the second contains no local
  agent state, client/oracle material, deploy binaries, logs, dumps, or backups.

- [x] **Step 5: Commit local-artifact hygiene separately.**

  ```powershell
  git add .gitignore documentation/progress.md documentation/plans/gap-registry.md
  git commit -m "chore(repo): isolate local agent artifacts"
  ```

## Task 2R: Record artifact-hygiene closeout on every required summary

**Files:**
- Modify: `documentation/plans/alpha-a0-truthful-baseline.md`
- Modify: `CHANGELOG.md`
- Modify: `documentation/progress.md`
- Modify: `ROADMAP.md`
- Modify: `documentation/roadmap.md`
- Test: `scripts/check_docs.ps1`, `git diff --check`

- [x] **Step 1: Add dated historical evidence to the changelog.**

  Under `CHANGELOG.md` → `[Unreleased]` → `Changed`, record that the
  2026-09-02 A0 baseline classified the pending Rust slices and excluded local
  `.superpowers/` and `docs/superpowers/` artifacts without deleting them.
  Link to the registry for volatile disposition details.

- [x] **Step 2: Add one concise dated handoff entry.**

  In `documentation/progress.md`, record the committed artifact-isolation
  result, the two still-uncommitted named Rust slices, and the next A0 task.
  Do not restate the disposition table.

- [x] **Step 3: Keep both roadmap surfaces navigational, not live-state sources.**

  In root `ROADMAP.md` and `documentation/roadmap.md`, add a 2026-09-02
  navigation note that A0 classification is tracked by `ARQ-E` in the registry
  and that current status remains in the registry/progress pair. Update each
  document's metadata only after checking that this claim is true.

- [x] **Step 4: Validate the closeout.**

  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check_docs.ps1
  git diff --check
  ```

  Expected: no document becomes a third current-state source, and metadata/link
  validation passes.

- [x] **Step 5: Commit the bounded closeout.**

  ```powershell
  git add documentation/plans/alpha-a0-truthful-baseline.md CHANGELOG.md documentation/progress.md ROADMAP.md documentation/roadmap.md
  git commit -m "docs(status): record artifact hygiene closeout"
  ```

## Task 3: Publish the tooling and deploy-boundary inventory

**Files:**
- Create: `documentation/reference/public-tooling-boundary.md`
- Modify: `documentation/README.md`
- Modify: `documentation/plans/gap-registry.md`
- Test: `scripts/check_docs.ps1`

- [ ] **Step 1: Write one reference inventory, not a second deployment guide.**

  Create a metadata-bearing reference with exactly three tables:

  ```markdown
  ## Public clean tooling
  | Path | Purpose | Allowed inputs/outputs | Verification |

  ## External operator prerequisites
  | Item | Why external | Configuration boundary | CI status |

  ## Prohibited public material
  | Material | Rule | Evidence | Handling on discovery |
  ```

  Populate it from the A0 audits. `source/reforge`, versioned scripts,
  configuration examples, and clean synthetic fixtures may be public. The
  external compatible client, client source/packs/binaries, frozen C++ oracle,
  legacy deploy trees, runtime logs/WAL, captures, database dumps, backups, and
  decompiled inputs/outputs may not be committed.

- [ ] **Step 2: Record deploy reproducibility as owned, sequenced work.**

  Add A2 child rows to the registry with these exact delivery boundaries:

  ```text
  A2.1 path contract and executable-relative deploy/TUI discovery
  A2.2 runtime configuration overrides and secret-free examples
  A2.3 native Windows PostgreSQL bootstrap plus lawful synthetic seed
  A2.4 whitelisted package assembly and manifest
  A2.5 TUI deploy/local-log/backup parity
  ```

  Each row must name its current evidence paths, owner, dependency, risk, and
  measurable exit condition from `alpha-collaborative-readiness.md`.

- [ ] **Step 3: Link the inventory from the documentation hub.**

  Add a single “Public tools and external prerequisites” row in
  `documentation/README.md`. Do not repeat the inventory in README, AGENTS, or
  the registry.

- [ ] **Step 4: Validate all reference links and metadata.**

  Run:

  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check_docs.ps1
  git diff --check
  ```

- [ ] **Step 5: Commit the inventory and owned blockers.**

  ```powershell
  git add documentation/reference/public-tooling-boundary.md documentation/README.md documentation/plans/gap-registry.md
  git commit -m "docs(alpha): classify public tooling and deploy blockers"
  ```

## Task 4: Make documentation authority unambiguous without rewriting history

**Files:**
- Modify: `documentation/DOCUMENTATION.md`
- Modify: `documentation/reference/document-authority.md`
- Modify: `documentation/README.md`
- Modify: `documentation/roadmap.md`
- Modify: `documentation/schema.md`
- Modify: `documentation/reference/login-flow.md`
- Modify: `README.md`
- Modify: `ROADMAP.md`
- Modify: `CHANGELOG.md`
- Modify: `documentation/history-index.md`
- Test: `scripts/check_docs.ps1`

- [x] **Step 1: Establish the exact current-state wording.**

  The only live status sources are:

  ```markdown
  1. `documentation/plans/gap-registry.md` — owned work, state, evidence, dependency, risk, exit.
  2. `documentation/progress.md` — current verified snapshot and session handoff.
  ```

  `README.md`, `ROADMAP.md`, `CHANGELOG.md`, ADRs, and history must link to
  those files and must not present themselves as a competing live status source.
  Preserve the verification precedence already defined in
  `document-authority.md`.

- [x] **Step 2: Reduce public entry documents to their unique job.**

  - Root README: purpose, alpha limitations, supported first path, and links.
  - `documentation/README.md`: human navigation, no model/preset/team internals.
  - `documentation/roadmap.md`: sole phase map; set `Type: Plan` if it remains
    the active phase map.
  - Root ROADMAP: change its metadata and introduction to an explicitly
    historical compatibility narrative. Remove or relabel every present-tense
    `live`/`current` status claim in it, including the stale G0.2 and backup
    cadence claims; preserve the underlying historical body with a clear dated
    archive boundary and links to the phase map and live state.
  - CHANGELOG: chronological evidence only.

  Remove the stale detailed status matrix from README only after its supported
  claims are represented by a stable reference or linked registry row.

- [x] **Step 3: Preserve history while making its non-authority visible.**

  Do not edit `documentation/history/**`. In `history-index.md`, add a clear
  note that archived metadata and paths are historical and may be stale; expand
  the index to include the omitted dated records found in the A0 audit. Link to
  their canonical successors where a live reader needs current information.

- [x] **Step 4: Correct identified current-document defects.**

  Correct the empty `## Fuente` heading in `schema.md`, replace the stale
  current-plan claim in `reference/login-flow.md` with a link to the historical
  plan, and reconcile the README claims about gold, quest actions, and locale
  with their current registry/ADR evidence. Correct the A0.2 changelog link to
  the exact `#a0-current-worktree-disposition` fragment. Do not change
  historical files.

- [x] **Step 5: Run current-document checks.**

  Run:

  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check_docs.ps1
  git diff --check
  ```

  Expected: metadata and live-state checks pass, all new relative links resolve,
  and no current document claims authority outside the two live files.

- [x] **Step 6: Commit the authority reduction.**

  ```powershell
  git add README.md ROADMAP.md documentation CHANGELOG.md AGENTS.md
  git commit -m "docs: simplify current-state authority"
  ```

## Task 5: Gate A0 and hand off A1/A2/A3

**Files:**
- Modify: `documentation/progress.md`
- Modify: `documentation/plans/gap-registry.md`
- Modify: `CHANGELOG.md`
- Test: `scripts/verify.ps1`, `scripts/check_docs.ps1`, Git status

- [ ] **Step 1: Run the final A0 evidence set.**

  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check_docs.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify.ps1
  git diff --check
  git status --short
  ```

  Expected: documented result for each command; no unintended tracked material;
  optional external-client/oracle legs reported only as available or unavailable.

- [ ] **Step 2: Record only verified A0 completion.**

  Mark `ARQ-E`/A0 complete only if the Task 1–4 exits are evidenced. Otherwise
  leave the exact failing child row `OPEN` or `BLOCKED`; never convert an audit
  finding into a completion claim.

- [ ] **Step 3: Request the planned architecture gate.**

  Give the reviewer the changed-path list, verifier output, public-tooling
  inventory, retained dirty-slice disposition, and the specific question:
  “Does the A0 contract make A1 license/boundary, A2 reproducibility, and A3
  documentation governance independently executable without creating another
  source of truth?”

- [ ] **Step 4: Commit the A0 handoff after the gate is reconciled.**

  ```powershell
  git add documentation/progress.md documentation/plans/gap-registry.md CHANGELOG.md
  git commit -m "docs(alpha): close truthful baseline"
  ```

## Coverage review

| Alpha plan requirement | A0 task |
|---|---|
| Truthful live state and owned blockers | 1, 4, 5 |
| Dirty/public artifact disposition | 2 |
| Clean-tool/public-boundary inventory | 3 |
| Portable deploy sequencing | 3 (A2.1–A2.5 registry rows) |
| Documentation simplification contract | 4 |
| Safe phase handoff and architecture gate | 5 |

No A0 task modifies the external client, client-derived data, frozen C++ oracle,
or gameplay behavior. Those constraints are enforced by ADR-0015 and the alpha
readiness plan.
