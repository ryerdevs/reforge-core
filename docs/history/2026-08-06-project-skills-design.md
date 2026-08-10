# Metin2 Project Skills Design

> **Metadata**
> - Type: History
> - Status: Historical
> - Audience: Project agents and maintainers (historical context only)
> - Last verified: 2026-08-10
> - Original location: `docs/superpowers/specs/2026-08-06-metin2-project-skills-design.md`
> - **Historical record.** Archived for context. This document is NOT current normative guidance: it was the initial 2026-08-06 design for the project-local skill set, based on the repo layout and goals of that date (Alpine/Docker compatibility baseline, `server/` tree). The current project skill set and rules are described in `AGENTS.md`; the repo layout has since been reorganized under `source/`.

## Context

The workspace contains a legacy Metin2 server written in C++, with BSD-oriented history, an existing Linux port/build area, Makefiles, CMake files, Docker-related files, and MySQL assets. The immediate goal is to make the server build and run reliably on Linux, specifically Alpine Linux in the current Windows/WSL workflow. A future phase will refactor the server into Rust; Rust skills are intentionally outside this first installation batch.

The workspace currently has a `.commandcode` directory but is not yet a Git repository. The server sources are under `server/`, including `server/linux` and `server/metin2_server+src`.

## Goals

- Improve agent support for maintaining and porting the legacy C++ server.
- Make Alpine Linux, musl, package management, linking, and container behavior first-class concerns.
- Establish a practical Docker and Docker Compose workflow for starting the server with one command.
- Improve diagnosis of compiler, linker, runtime, and container failures.
- Support architecture and documentation work without installing redundant skills.
- Keep the skills usable by both Codex and Command Code at project scope.

## Scope of the first installation batch

The first batch will include only vetted skills for:

- Skill discovery and installation.
- C++ maintenance, modernization, and legacy-code analysis.
- Linux development and portability.
- Alpine Linux specifically, including musl libc, `apk`, compiler/linker behavior, and minimal images.
- Docker and Docker Compose.
- Make and CMake build workflows.
- Debugging and build-failure diagnosis.

BSD-specific skills are excluded because BSD is a compatibility reference being phased out. Rust-specific skills are deferred until the Linux C++ baseline is stable.

## Reuse policy

Codex already has general skills for debugging, clean code, architecture improvement, documentation, ADRs, planning, and verification. We will reuse those where possible instead of installing overlapping third-party versions. A project-local equivalent will only be added when it is needed by another agent or provides clearly better Metin2-specific guidance.

## Installation approach

Use the `skills.sh` CLI and the Vercel Labs `find-skills` workflow to discover candidates. Install selected skills at project scope for the Codex and Command Code agents, keeping the installation visible and reproducible in the project. Do not install every result automatically.

Each candidate must be checked for:

1. Clear scope and useful activation criteria.
2. Compatibility with Codex and Command Code project skill directories.
3. No destructive or unrelated automation.
4. Appropriate Alpine/Linux assumptions.
5. Reasonable maintenance or adoption signals where available.

## Verification

After installation:

- Confirm the expected project skill directories and installed names.
- Read each installed `SKILL.md` and check that it matches the intended domain.
- Check that no duplicate or unrelated skills were installed.
- Report any candidate that was skipped and why.
- Keep actual Docker, build, and source changes for a later implementation task.

## Exclusions

This design does not change C++ source code, Dockerfiles, Compose files, Alpine packages, build flags, database schemas, or authentication. It only establishes the agent skill set needed for those later tasks.
