# Metin2 Project Skills Design Revision

> **Metadata**
> - Type: History
> - Status: Historical
> - Audience: Project agents and maintainers (historical context only)
> - Last verified: 2026-08-10
> - Original location: `docs/superpowers/specs/2026-08-06-metin2-project-skills-design-revision.md`
> - **Historical record.** Archived for context. This document is NOT current normative guidance: it revises the 2026-08-06 project-skills design (Alpine/Docker compatibility baseline, `server/` tree). The current project rules and skill set live in `AGENTS.md`; the repo layout has since been reorganized under `source/`.

This revision incorporates the project roadmap agreed after the initial design.

## Immediate baseline

The first engineering horizon is to make the existing C++ Metin2 server build and run reliably on Alpine Linux, then package it as a reproducible Docker Compose stack that can be started with one command. This phase is a compatibility baseline: preserve observable behavior, identify platform assumptions, and document runtime dependencies before changing the architecture.

Alpine is a first-class target in this phase. Skills must account for musl libc, `apk`, minimal images, compiler and linker differences, native dependencies, and the interaction between Windows, WSL, Docker Desktop, and Alpine containers.

BSD is a historical reference only and is excluded from the initial skills batch.

## Long-term modernization

The eventual Rust project is a structural rewrite, not a line-by-line translation of the early-2000s C++ server. It should address legacy coupling, unsafe practices, obsolete process boundaries, and unclear ownership of responsibilities.

One planned direction is to unify the current `game` and `db` responsibilities behind a coherent server architecture instead of preserving the existing split by default. The exact process model, data ownership, persistence boundary, protocols, and migration strategy must be decided through architecture documents and ADRs after the current behavior and dependencies are understood.

The Rust rewrite must not silently alter the C++ compatibility baseline while Alpine and Docker support are still being stabilized.

## `AGENTS.md`

Create a root `AGENTS.md` before implementation changes. It should be the durable instruction layer shared by agents and include:

- Current C++/Alpine/Docker build and run commands.
- Debugging and verification expectations.
- The compatibility-baseline rules.
- The future Rust rewrite direction.
- The intended investigation of `game`/`db` unification.
- Architecture and dependency-boundary rules for new work.
- Documentation and ADR requirements.
- Commands that are safe to automate and commands requiring explicit confirmation.

`AGENTS.md` is operational project guidance. It does not replace ADRs for decisions about data ownership, protocols, process consolidation, or Rust architecture.

## Skill-selection implications

The discovery and installation pass must look for skills covering C++, Linux, Alpine Linux, Docker, Docker Compose, Make, CMake, and build/runtime debugging. General debugging, architecture, documentation, planning, and verification skills already available to Codex should be reused unless a project-local equivalent is needed by another agent or is clearly better.

The skills will be installed at project scope for Codex and Command Code only after each candidate's scope, safety, Alpine relevance, and maintenance signals have been checked.
