---
Type: Decision
Status: Accepted
Audience: Contributors, maintainers
Date: 2026-08-30
Last verified: 2026-08-30
Supersedes: ADR-0013
Superseded by: —
---

# ADR-0015: Rust-only public repository boundary

## Status

Accepted on 2026-08-30. This decision supersedes [ADR-0013](0013-client-rewrite.md):
the proposed Rust client workspace is not part of this public repository, and
F7 is deferred outside it.

## Context

- The public repository is intended to distribute the authored Rust server, its
  compatibility protocol, supporting server tooling, documentation, and
  verification scripts.
- Client source, pack source, generated client binaries, and game assets are not
  authored server implementation and must not become part of the public server
  distribution.
- Real-client verification remains valuable, but it can use an operator-provided
  compatible client kept outside this repository. Server tests and CI must not
  require that client or its assets to be present in the checkout.
- ADR-0013 accepted a Rust client workspace at `source/client_rust` in the same
  repository. That boundary is no longer compatible with the public-repository
  scope.

## Decision

1. `source/reforge` is the canonical implementation of the authored Rust
   server. Repository documentation, scripts, protocol metadata, and other
   supporting files may remain when they serve that server.
2. Client source and pack source are excluded from the public repository. In
   particular, `source/client` and `source/tools/pack` are not tracked, and the
   ignore rules prevent them from being added accidentally. Client binaries,
   packs, extracted assets, and other third-party game content are also kept
   outside the repository.
3. End-to-end compatibility checks use an external, operator-provided client.
   The client location and its assets are local test prerequisites, not a
   repository API, build input, or distributable project artifact.
4. F7—the standalone Rust client—is deferred outside this repository. No
   `source/client_rust` workspace, client build pipeline, or pack-conversion
   pipeline is planned in this server repository. A future client project must
   make its own repository and licensing decisions.
5. The server keeps its documented compatibility boundary while F6 parity work
   continues. Removing or changing that boundary remains a separate server
   decision; this ADR does not remove `protocol::legacy`.
6. Historical documents remain historical records. They may describe the former
   client plan, but active documentation must describe the client as external
   and must not present excluded client or pack paths as repository contents.

## Alternatives considered

### Keep the client workspace in this repository

Rejected: it conflicts with the server-only public boundary and couples the
server distribution to client source, assets, and their licensing obligations.

### Add the client as a submodule or downloadable repository dependency

Rejected for this repository: it still makes the public server checkout appear
to distribute or require client material and would make ordinary server
verification depend on an external source layout. External client verification
is sufficient without a repository dependency.

### Stop real-client verification

Rejected: packet compatibility and world-entry behavior are observable server
contracts. They should continue to be checked with a client that operators
obtain and license independently.

### Start F7 in a second repository now

Rejected as premature: F7 has no server-side acceptance requirement today. The
future client can be started as a separate project when the server boundary is
ready and its requirements are known.

## Consequences

- The public repository has a smaller, clearer scope: authored Rust server code
  rather than a server-plus-client distribution.
- Server builds, unit tests, and CI remain runnable without client source,
  proprietary packs, or generated client artifacts.
- Real-client smoke tests require a separately obtained compatible client and
  cannot be reproduced from this repository alone; reports must identify that
  external prerequisite.
- The former client plan remains available as superseded decision context, but
  its implementation is not an active server-repository task.
- The Apache License in the repository applies to the authored repository work;
  external client software and assets retain their own terms.

## Not decided in this ADR

- The future client's repository, engine, UI, asset formats, protocol changes,
  encryption, and distribution license.
- The date and acceptance criteria for beginning F7.
