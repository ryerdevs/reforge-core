---
Type: Reference
Status: Current
Audience: All
Last verified: 2026-09-02
---

# Public tooling and deploy boundary

This inventory classifies versioned clean tooling, operator-supplied
prerequisites, and material that must not enter the public checkout. The
repository boundary in [ADR-0015](../adr/0015-rust-only-public-repository.md)
is authoritative. This page is an inventory, not a deployment procedure; the
owned reproducibility work remains in the [gap registry](../plans/gap-registry.md).

Versioned code may consume lawful synthetic test fixtures. Proprietary client
material is supplied only by an operator outside the checkout and must never be
committed as an input or output; this follows [ADR-0015](../adr/0015-rust-only-public-repository.md)
and this boundary inventory. The boundary check permits the documented
`source/reforge/protocol/tests/golden/auth_login3_40999.bin` protocol fixture;
build binaries and other binary-looking outputs remain unversioned. The authored
public repository uses Apache-2.0; external client software and assets retain
their own terms. The check's narrow `mt2` allowance covers only the documented
local PostgreSQL default; A2.2 still owns the move to secret-free, supplied
configuration.

## Public clean tooling

| Path | Purpose | Allowed inputs/outputs | Verification |
|---|---|---|---|
| [`source/reforge/` workspace](../../source/reforge/README.md) | Authored Rust server, protocol, database, realm, and verification workspace. | Versioned Rust and Cargo inputs plus lawful synthetic test data; local build and test outputs only. It must not consume client, pack, decompiled, or frozen-oracle material. | [`source/reforge/README.md`](../../source/reforge/README.md):10-14,33-47; [ADR-0015](../adr/0015-rust-only-public-repository.md):21-29,34-50 |
| Versioned verification and maintenance commands: [`scripts/check_boundary.ps1`](../../scripts/check_boundary.ps1), [`scripts/check_docs.ps1`](../../scripts/check_docs.ps1), [`scripts/verify.ps1`](../../scripts/verify.ps1), [`scripts/status.ps1`](../../scripts/status.ps1), and [`scripts/clean.ps1`](../../scripts/clean.ps1) | Check the public boundary and documentation, run the standard Rust gate, capture status, and preview cleanup of regenerable local artifacts. | A checkout and explicitly supplied operator state where a command requires it; diagnostics and regenerable outputs stay local or ignored. No client or oracle material is a standard clean-check input. | [`AGENTS.md`](../../AGENTS.md):40-51; [`scripts/check_boundary.ps1`](../../scripts/check_boundary.ps1):1-8; [`scripts/check_docs.ps1`](../../scripts/check_docs.ps1):1-7 |
| Versioned operator controls: [`scripts/start_win.ps1`](../../scripts/start_win.ps1), [`scripts/stop_win.ps1`](../../scripts/stop_win.ps1), [`scripts/deploy_win.ps1`](../../scripts/deploy_win.ps1), [`scripts/build_admin_tui.ps1`](../../scripts/build_admin_tui.ps1), [`scripts/backup_win.ps1`](../../scripts/backup_win.ps1), and [`scripts/restore_drill.ps1`](../../scripts/restore_drill.ps1) | Versioned controls for the native Windows server runtime and its local operator checks. | Scripts may read explicitly supplied local PostgreSQL and deploy state and may write local logs, backups, or build artifacts. They must not add client, oracle, legacy-tree, or proprietary data to the checkout. Current fixed-path behavior is an A2 blocker. | [`scripts/start_win.ps1`](../../scripts/start_win.ps1):12-30; [`scripts/deploy_win.ps1`](../../scripts/deploy_win.ps1):24-28,61-80; [alpha readiness plan](../plans/alpha-collaborative-readiness.md):102-119 |
| Sanitized configuration examples: [`auth.example.toml`](../../source/deploy/win/examples/auth.example.toml) and [`channel.example.toml`](../../source/deploy/win/examples/channel.example.toml) | Public examples of the auth and channel configuration shape. | Example values only; no operator secrets, client paths, oracle paths, or runtime outputs. Secret-free and path-neutral examples are an A2.2 exit condition. | [`auth.example.toml`](../../source/deploy/win/examples/auth.example.toml):1-10; [`channel.example.toml`](../../source/deploy/win/examples/channel.example.toml):1-12; [alpha readiness plan](../plans/alpha-collaborative-readiness.md):86-100,102-119 |
| Versioned protocol golden fixture: [`auth_login3_40999.bin`](../../source/reforge/protocol/tests/golden/auth_login3_40999.bin) | Byte-exact regression input for the Rust protocol tests. | One reviewed protocol fixture only; it is not a client executable, pack, or build output. New captures remain subject to the boundary review. | [`golden_auth.rs`](../../source/reforge/protocol/tests/golden_auth.rs):1-24; `scripts/check_boundary.ps1` |
| Clean synthetic verifier: [`source/reforge/game_core/tests/synthetic_verifier.rs`](../../source/reforge/game_core/tests/synthetic_verifier.rs) | Deterministic server-side negative and mutation verifiers. | Synthetic rows and in-memory values only; test output stays local. No production or client-derived data. | [`synthetic_verifier.rs`](../../source/reforge/game_core/tests/synthetic_verifier.rs):1-9,27-35,95-104 |
| Versioned documentation and policy: [`documentation/DOCUMENTATION.md`](../DOCUMENTATION.md) and [`document-authority.md`](document-authority.md) | Canonical documentation rules, authority, and boundary references. | Versioned Markdown, relative links, and concise evidence citations only. | [`DOCUMENTATION.md`](../DOCUMENTATION.md):43-58,71-85,116-129; [`document-authority.md`](document-authority.md):14-30 |
| Future versioned lawful synthetic fixtures (A2.3) | Development-only schema, seed, or fixture material for the supported contributor flow. This path does not exist at this baseline. | New material may contain authored or synthetic data only; production, client-derived, decompiled, and oracle-derived data remain excluded. | [alpha readiness plan](../plans/alpha-collaborative-readiness.md):102-119; A2.3 in the [gap registry](../plans/gap-registry.md) |

## External operator prerequisites

| Item | Why external | Configuration boundary | CI status |
|---|---|---|---|
| Separately obtained compatible client | Real-client compatibility checks use client software and assets that are outside the authored Rust server. | Keep the client and its packs/assets outside this checkout. Supply its path only to an operator-only check; never use it as a build input or commit its inputs or outputs. | Operator-only optional check; not a clean-clone or CI input. |
| Native Windows PostgreSQL installation and service | The native runtime currently expects a local PostgreSQL service and the A2 plan must provide the lawful development bootstrap. | Keep the installation, service state, database, credentials, WAL, logs, dumps, and backups outside the clean source tree; pass connection settings through the eventual explicit configuration contract. | Live-PG leg is optional and environment-gated; the clean standard path must not depend on this maintainer machine. |
| On-demand WSL frozen-oracle parity environment | The frozen C++ implementation is retained only for local parity evidence and is not authored public Rust tooling. | Keep `source/server/**` and the WSL environment outside the public checkout boundary; parity may inform evidence but must never become a build or CI input. | Operator-only parity check; never a GitHub CI input. |

## Prohibited public material

| Material | Rule | Evidence | Handling on discovery |
|---|---|---|---|
| Client source, pack source, extracted assets, and generated client binaries | Do not commit or distribute client implementation, packs, assets, or generated client artifacts from this server repository. | [ADR-0015](../adr/0015-rust-only-public-repository.md):21-29,39-50; [`.gitignore`](../../.gitignore):70-79,115-125 | Stop the release or change, quarantine the material outside the checkout, remove it from the public index after owner review, and report the boundary violation without attaching the material. |
| Frozen C++ oracle `source/server/**` and decompiled inputs or outputs | Keep the oracle and any decompiled or proprietary evidence local-only; do not copy it into the public server tree or make it a build input. | [ADR-0015](../adr/0015-rust-only-public-repository.md):24-29,54-56; [`.gitignore`](../../.gitignore):61-75; [alpha readiness plan](../plans/alpha-collaborative-readiness.md):82-100 | Reject the release, remove the material from the public index without deleting the operator's local evidence, and record only a path-level boundary finding. |
| Legacy deploy trees: `source/deploy/main/**`, `source/deploy/baks/**`, and `source/deploy/sql/**` | These legacy runtime, backup, and SQL trees are not clean Rust-server inputs and must not be committed as public deploy content. | [`.gitignore`](../../.gitignore):61-68; [alpha readiness plan](../plans/alpha-collaborative-readiness.md):19-27 | Keep them outside the public index and package allowlist; quarantine any discovered copy and use only future lawful synthetic development data for contributor setup. |
| Deploy executables and generated build binaries, including `source/deploy/**/*.exe` and `**/target/**` | Generated executables and build output are local or release outputs, not versioned public source inputs. | [`.gitignore`](../../.gitignore):41-59,121-133; [`scripts/build_admin_tui.ps1`](../../scripts/build_admin_tui.ps1):45-68 | Do not stage them; remove them from a package manifest or index, regenerate from versioned Rust sources, and keep any local copy outside the versioned tree. |
| Production configurations, credentials, and machine-local overrides | Only sanitized examples may be versioned. Operator secrets and machine-specific paths must not become public inputs or defaults. | [`.gitignore`](../../.gitignore):15-20; [`auth.toml`](../../source/deploy/win/auth.toml):1-7; [`channel.toml`](../../source/deploy/win/channel.toml):1-12; [alpha readiness plan](../plans/alpha-collaborative-readiness.md):86-100,102-119 | Quarantine the file, rotate or revoke any real credential, remove it from the public index after owner review, and replace it with a secret-free example and explicit override. |
| Runtime logs, WAL, and operator packet captures, including `source/deploy/win/logs/**` and `**/wal/**` | Operational diagnostics and captured traffic are local evidence, not public source or reproducible clean-clone inputs. | [`.gitignore`](../../.gitignore):22-27,48-57,134-135; [`document-authority.md`](document-authority.md):28-30 | Keep them local and ignored; scrub them from reports, do not attach them to public changes, and record only a concise path-level finding. The separately documented protocol fixture above is the narrow exception. |
| Database dumps and backups, including `**/*.dump` and deploy backup directories | Dumps and backups can contain runtime or proprietary data and cannot substitute for a lawful versioned development seed. | [`.gitignore`](../../.gitignore):48-60; [`backup-restore.md`](backup-restore.md):12-19,62-67; [alpha readiness plan](../plans/alpha-collaborative-readiness.md):102-119 | Never stage or package them; keep operational copies outside the checkout and use the A2.3 synthetic schema/seed path for public setup. |

## GitHub enforcement

The workflow uses read-only repository permissions and runs the boundary and
dependency-review checks without repository or production secrets. GitHub
repository settings must enable branch protection and the required checks for
`main` and `beta`; those hosting controls cannot be represented in this checkout.
