---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Date: 2026-08-10
Last verified: 2026-08-10
Supersedes: ADR-0003 (partial — workspace layout and crate names)
Superseded by: —
---

# ADR-0004: `reforge` workspace structure and names

## Context

ADR-0003 defined the Rust workspace in `source/reforge` with a flat layout and generic layer names (`protocol`, `net`, `db`, `game`, `auth`). The user asked for a more professional structure and names with identity, without a brand prefix (rejected `m2-*`). After evaluating the `crates/` subdirectory proposal (tokio/serde/bevy convention), the user **rejected it**: he prefers the flat layout at the workspace root.

He also decided **ONE single binary** (not several): the `game`+`db` unification (ADR-0002) removes the db broker as a process, and the channel isolation required by ADR-0002 is achieved with **N processes of the SAME binary** with different config (role `auth` | `channel`), not with N binaries. This resolves the inconsistency between ADR-0002 ("auth as its own process") and the unified plan ("auth as a mode of the same binary"): auth is a role of the same binary, and the process running that role is the auth server.

## Decision

1. **Flat layout** in `source/reforge` (no subdirectories): `protocol/`, `network/`, `database/`, `realm/`, `server_realms/`.
2. **Crate names** (renames over ADR-0003):

| Before (ADR-0003) | Now | Justification |
|---|---|---|
| `protocol` | `protocol` | Unchanged — not ambiguous in the workspace context |
| `net` | `network` | Communicates the full layer; "transport" rejected by the user |
| `db` | `database` | Unambiguous; "db" was ambiguous |
| `game` | `realm` | Names the domain (the world simulation by regions); "server" stays reserved for the binary |
| `auth` (crate) | module `network::auth` | Auth is pure network layer (handshake, LOGIN3, keys, PanamaPack) — lives inside network (F2) |

3. **One single binary `server_realms`** (provisional user name) with roles by config: `--role auth` (port 30001) | `--role channel` (region, port 30003). One artifact, N isolated processes. Scales by config: cross-server regions = new role, no new binary.
4. **Workspace conventions**: centralized `[workspace.dependencies]` (tokio 1.49: rt-multi-thread/net/io-util/time/sync/macros), `[workspace.lints.rust] unsafe_code = "forbid"` (there is no unsafe; the lint guarantees it), `rust-toolchain.toml` (1.97.0), workspace `README.md` with architecture + names glossary.
5. **Runtime**: the legacy runtime keeps `source/deploy` (Windows copy of the instances tree, gitignored; the WSL tree `metin2_svfiles` is NOT touched — the startup scripts depend on that path). 2026-08-10: the intermediate rename to `source/realms` was **reverted by user correction**. The name **`server_realms`** is the rewrite's binary crate in `source/reforge/server_realms`, which will host the compiled binary + configs from F2 (provisional user name).

## Alternatives considered

- **`crates/` + `servers/` subdirectories**: proposed (industry convention), rejected by the user — he prefers the flat layout. Revisited only if the workspace grows beyond 8–10 crates (YAGNI).
- **`m2-`/`metin2-` prefix**: rejected by the user — names without prefix.
- **`transport` for net**: rejected by the user — `network`.
- **Several binaries (`auth-server`, `channel-server`)**: rejected — they duplicate main()/config/deps and can diverge; the single binary scales by config.
- **Runtime names**: `deploy` (stays — the rename to `realms` was reverted by user correction), `runtime`/`production` (do not evoke the case), `release` (collides with `cargo build --release` and GitHub Releases) → `realms` went to the rewrite binary's name (`server_realms`, provisional).

## Consequences

### Positive

- Project identity (domain names, not implementation names).
- Clean, legible workspace root; a single build artifact.
- Scaling by config (roles), not by binaries; new regions = new config.
- Clear boundaries: `server_realms` (thin executable) vs crates (libraries).

### Negative

- The renames touch references in docs/specs/ADRs (updated in the same session).
- The split of `protocol/src/lib.rs` (~1700 lines) into modules is **deferred to F2** (when PanamaPack and the world packets arrive) — YAGNI today.
- If the workspace grows a lot, the flat layout fills up — `crates/` is revisited then.

## Not decided in this ADR

- Internal module structure of `protocol` (F2).
- **Binary config: TOML — DECIDED (2026-08-10).** The `server_realms` configs (role, region, ports, rates...) are written in **TOML** (comments, nested tables, Rust-native; via config-rs in F2; clap for the `--role` args). `server_realms/` will host the compiled binary + configs from F2.
- Instances scheme of the runtime (`source/deploy`) and of the `server_realms` folder (binary + configs) — F2/F5.
