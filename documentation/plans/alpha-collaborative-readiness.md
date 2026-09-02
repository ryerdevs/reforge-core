---
Type: Plan
Status: Current
Audience: Contributors, maintainers
Last verified: 2026-09-02
---

# Collaborative alpha readiness

## Objective

Prepare `reforge-core` for a **collaborative server alpha**: a public
contributor preview that can be cloned, understood, verified, and improved
without claiming production readiness or complete compatibility parity.

The alpha is a path to attract maintainers for known gaps; it is not a promise
that every gameplay system, data set, or external-client flow is complete.

## Approved constraints

| Decision | Rule |
|---|---|
| License | The public repository uses **Apache-2.0**. All contradictory metadata and operational documentation must be aligned before the alpha tag. |
| Public boundary | The repository contains the authored Rust server, protocol, clean supporting tools, documentation, scripts, and lawful synthetic/versioned test data. Client source, packs, client binaries, decompiled material, proprietary inputs/outputs, and the frozen C++ oracle remain external. [ADR-0015](../adr/0015-rust-only-public-repository.md) remains authoritative. |
| Client-compatible checks | They remain operator-only checks using an independently obtained compatible client. They are evidence, not a clean-clone build dependency. |
| Alpha promise | Contributors can build and test the server from a clean clone and can see supported flows, known limitations, and the route for a first contribution. |
| Scope discipline | Do not delay alpha on total parity. A crash, data-loss, security, public-boundary, or reproducibility defect is a gate; a documented gameplay/content gap belongs in the registry. |

## Alpha definition of done

The first public alpha tag may be created only when all items below have
evidence in the repository and CI:

1. A clean Windows checkout can run the documented setup, create or load the
   lawful development database, start the Rust auth and channel roles, and run
   the standard verifier without an external client, local oracle, or
   machine-specific path.
2. `LICENSE`, Cargo metadata, deploy metadata, and public documentation all
   identify Apache-2.0 and the same public boundary.
3. The root README explains the project status, supported development flow,
   prerequisites, limitations, security reporting, and contribution path in
   under one screen before linking to details.
4. `CONTRIBUTING.md`, a code of conduct, security and support policies, issue
   and pull-request templates, and review ownership exist and agree with the
   public boundary.
5. The canonical documentation hub distinguishes current state, plans,
   reference, how-to material, explanations, decisions, guardrails, and
   immutable history. It does not duplicate the live registry or handoff.
6. Root `AGENTS.md` is a concise agent contract: purpose, public boundary,
   authoritative state, mandatory checks, local instruction precedence, and
   links to detailed guardrails. Rules that can be verified are enforced by
   scripts or CI rather than prose alone.
7. CI validates formatting, tests, clippy, documentation metadata and links,
   public-boundary/secret policy, and the repository's clean supported setup.
   Workflows from forks never receive write credentials or production secrets.
8. The release notes name supported flows, known P0/P1 limitations, local-only
   prerequisites, and the exact reporting path for regressions.

## Delivery phases

### A0 — Establish a truthful baseline

**Goal:** make current status auditable before changing release surfaces.

Detailed execution: [A0 truthful-baseline implementation plan](alpha-a0-truthful-baseline.md).

- Reconcile `progress.md`, the gap registry, roadmap summaries, and release
  metadata with the actual HEAD and verified evidence.
- Classify the dirty worktree into independently documented slices; commit,
  continue, or deliberately discard only with its owner’s approval. Generated
  agent artifacts must remain local and ignored.
- Inventory tracked developer/operator tools. Classify each as public clean
  tooling, external operator prerequisite, or disallowed material. Do not move
  client-related tools before the inventory records their inputs and outputs.
- Record existing reproducibility failures (database bootstrap, hard-coded
  paths, unavailable scripts) as owned registry items.

**Exit:** the live documents name the current commit, every pending alpha
blocker has an owner and exit criterion, and no generated/private material is
present in the intended release tree.

### A1 — License and public-boundary enforcement

**Goal:** make the legal and repository contract consistent and testable.

- Align Cargo/deploy metadata and documentation with Apache-2.0; retain
  third-party notices where their licenses require them.
- Turn ADR-0015's exclusions into a deterministic boundary check covering
  forbidden paths, generated binaries, packs, client/decompiled artifacts, and
  accidental secrets. The check must allow the documented Rust server tooling.
- Document clean-tool requirements: versioned code may consume synthetic test
  fixtures; proprietary client material is supplied only by an operator outside
  the checkout and must never be committed as an input or output.
- Add security-oriented GitHub defaults: least-privilege workflow tokens,
  protected required checks, dependency review where available, and no secrets
  for untrusted fork code.

#### A1 implementation checklist

- [x] Cargo workspace metadata and deploy documentation identify Apache-2.0;
  historical records remain unchanged.
- [x] `scripts/check_boundary.ps1` rejects forbidden tracked/status paths,
  generated client proto outputs, extensionless binary content, and
  non-placeholder secret assignments in TOML, JSON, scripts, and legacy MySQL
  templates while allowing the documented protocol fixture.
- [x] The clean-tool contract states that lawful synthetic fixtures are allowed
  and proprietary client material stays outside the checkout as operator-only
  input/output.
- [x] CI has read-only permissions, Dependabot schedules, and pull-request
  dependency review; no write token or production secret is granted.
- [x] The active GitHub repository ruleset protects `main` and `beta` (including
  a future `beta` branch), requires pull requests with one fresh approval, and
  requires the `verify` and `dependency-review` checks.
- [x] Local evidence: the clean boundary check and its mutation suite pass,
  untracked forbidden, secret, generated-output, and extensionless-binary
  fixtures fail it, `check_docs.ps1` passes, and `git diff --check` is clean.

**Exit:** a deliberate forbidden-file fixture fails locally/CI, repository
metadata has one license, and the public checkout has an explicit supported
tooling inventory.

### A2 — Reproducible contributor environment

**Goal:** reduce first contribution to a documented clean-clone path.

- Replace repository-specific absolute paths with repository-relative defaults
  and explicit environment/config overrides. Preserve the operator-only client
  path as optional local configuration, never as a default build input.
- Version a lawful server development schema and minimal synthetic seed data
  sufficient for a supported login, character, and world test flow. Do not
  redistribute unverified production/client-derived data.
- Provide one documented setup command sequence and one diagnostic command for
  PostgreSQL, auth, channel, migrations/seed, logs, and teardown.
- Separate the portable standard verification gate from optional live-PG, WSL,
  oracle, and real-client checks; label each prerequisite and expected result.

**Exit:** a new contributor follows the tutorial on a clean Windows machine to
run the supported server flow and standard verifier, with a failure diagnosis
that does not depend on maintainer-specific paths.

### A3 — Documentation and agent governance

**Goal:** make the repository legible to humans and coding agents without
duplicating truth.

- Keep `documentation/` as the canonical documentation root. Its existing
  Diátaxis-aligned kinds remain: tutorial, how-to, reference, explanation,
  decision, guardrail, plan, history, snapshot, and hub.
- Rebuild the human entry path: concise root README → contributor tutorial →
  operational/reference details → live gap registry and handoff.
- Rebuild root `AGENTS.md` as a short, tool-neutral contract. Nested
  instructions are permitted only for local deltas. Vendor-specific files, if
  ever needed, must link back and cannot override the canonical contract.
- Add an **agent failure ledger** under `documentation/guardrails/`. Each entry
  records `failure → impact → violated invariant → evidence → permanent
  control → regression/policy check → owner → status`. Repeated lessons become
  concise guardrails; machine-detectable controls graduate to scripts/CI.
- Add the contribution, security, support, conduct, issue, PR, and ownership
  documents. Explain how to report a compatibility regression without sharing
  excluded client material.

**Exit:** a new person and a new coding agent each have one navigable entry
point, one authoritative source for live status, and one concrete first task.

### A4 — Enforce the contract

**Goal:** make common regressions hard to merge.

- Extend the existing verification scripts rather than creating parallel
  commands. Add focused checks only when they reliably enforce an approved
  rule.
- Require all behavior fixes to include a regression/negative test; use
  property tests where an invariant merits them.
- Validate document metadata, links, canonical status references, required
  release files, license metadata, public boundary, and no-secret policy.
- Configure branch protection and required review/status checks in the hosting
  platform; document any settings that cannot live in Git.

**Exit:** a pull request that violates a documented machine-checkable rule is
rejected before merge, and an intentionally invalid fixture proves each new
check is meaningful.

### A5 — Alpha candidate and release

**Goal:** make an honest, reproducible contributor preview.

- Execute the clean-clone tutorial independently from the author environment.
- Run the standard verifier and record optional-live check availability
  separately; never label an unavailable optional leg as passed.
- Triage all remaining P0/P1 items into fixed, accepted limitation, or release
  blocker. Publish known issues with labels and reproducible reports.
- Tag only after the alpha definition of done is satisfied. The release notes
  must say “contributor preview”, list supported flows and exclusions, and link
  to contribution/security/support routes.

**Exit:** an external contributor can reproduce the documented result, file a
safe report, and submit a small verified pull request without privileged local
assets.

## Change map and dependencies

| Phase | Depends on | Likely paths | Validation owner |
|---|---|---|---|
| A0 | none | `documentation/{progress.md,plans/gap-registry.md,roadmap.md}`, `ROADMAP.md`, `.gitignore`, tooling inventory | orchestrator |
| A1 | A0 | `LICENSE`, `source/reforge/Cargo.toml`, deploy metadata, `.github/`, boundary check script, ADR/reference links | orchestrator + security reviewer |
| A2 | A1 | `scripts/`, versioned server-development schema/seed, config examples, tutorials/how-to | implementer + clean-machine reviewer |
| A3 | A0, A1 | `README.md`, `AGENTS.md`, `documentation/`, GitHub community files | librarian + orchestrator |
| A4 | A1, A3 | `scripts/verify.ps1`, `scripts/check_docs.ps1`, `.github/workflows/` | fixer + orchestrator |
| A5 | A2, A3, A4 | release notes, issue tracker, tag metadata | maintainer |

The A2 and A3 lanes may run in parallel after A1. A4 follows their accepted
contracts; it must not invent new policy. Gameplay/content work in the gap
registry proceeds independently unless it changes an alpha gate.

## Risk policy

| Risk | Handling |
|---|---|
| Proprietary material enters the public repository | Treat as a release blocker; reject with automated boundary checks and document the external-prerequisite route. |
| Synthetic data accidentally claims parity | Label it development-only; use it only for documented supported flows. Full parity remains a separate registry objective. |
| Documentation becomes another stale status source | `document-authority.md` remains the precedence rule; summaries link rather than restate live status. |
| Agents repeat a previous mistake | Record the invariant and evidence in the failure ledger; promote a stable rule to a guardrail and automate it where possible. |
| CI blocks legitimate external contributors | Test fork-safe workflows and keep optional oracle/client checks explicitly local. |

## Evidence and references

- [ADR-0015](../adr/0015-rust-only-public-repository.md) — public boundary.
- [Document authority](../reference/document-authority.md) — live-state
  precedence.
- [Documentation policy](../DOCUMENTATION.md) — kinds and metadata.
- [Diátaxis](https://diataxis.fr/start-here/) — documentation needs.
- [AGENTS.md format](https://agents.md/) and [Codex instruction
  layering](https://learn.chatgpt.com/docs/agent-configuration/agents-md) —
  concise instruction-file guidance.
- [OpenSSF GitHub practices](https://best.openssf.org/SCM-BestPractices/github/)
  and [GitHub Actions hardening](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions)
  — enforceable repository controls.
- [Google SRE postmortem culture](https://sre.google/workbook/postmortem-culture/)
  — owned, tracked corrective actions.
