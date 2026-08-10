# Metin2 Alpine Linux and Docker Compose Compatibility Baseline

> **Metadata**
> - Type: History
> - Status: Historical
> - Audience: Project agents and maintainers (historical context only)
> - Last verified: 2026-08-10
> - Original location: `docs/superpowers/plans/2026-08-06-metin2-alpine-docker-compose-baseline.md`
> - **Historical record.** Archived for context. This document is NOT current normative guidance: it was the planned Alpine/Docker compatibility baseline. The current runtime contract (MariaDB + legacy C++ processes via `scripts/` on WSL) and the Rust rewrite direction are described in `AGENTS.md` and the active `docs/plans/server-rewrite.md`. Paths and tasks in this file reflect the 2026-08-06 repo layout (e.g. `server/`), which has since been reorganized under `source/` — do not follow them literally.

> **For agentic workers:** execute this plan task-by-task. Use the project-local C++/CMake, Docker, debugging, documentation, and verification guidance before modifying implementation files.

**Goal:** Convert the already-buildable Linux C++ server into a reproducible Alpine Linux build and a one-command Docker Compose deployment without changing the legacy server's observable behavior.

**Architecture:** Keep the current `game` and `db` executables and their file-based `CONFIG`/`conf.txt` contracts. Build them in an Alpine/musl image, run each legacy process with the correct working directory, and connect the processes through Compose DNS. Use a MariaDB-compatible service for this compatibility horizon because the current code speaks the MySQL client protocol and the repository contains MariaDB/MySQL dumps. PostgreSQL remains the future Rust persistence target documented in ADR-0001.

**Tech Stack:** C++17, CMake 3.20+, Ninja, Alpine Linux 3.24.1, musl libc, OpenSSL, zlib, MariaDB Connector/C, Crypto++, Docker multi-stage builds, Docker Compose, MariaDB 10.11.18.

---

## Scope and non-goals

This plan covers the path from the current Linux CMake build to a reproducible Alpine image and a Compose stack. It does not migrate SQL schemas to PostgreSQL, rewrite code in Rust, merge `game` and `db`, or perform a broad C++ refactor. The existing `server/linux/build` output is treated as a local artifact; the container build must configure into a fresh directory so stale WSL paths cannot affect the result.

The initial runnable profile is a smoke profile containing MariaDB, schema loading, the legacy DB cache process, auth, and one game process. The full profile expands this to every existing `CONFIG` directory after the smoke profile proves the process and networking contract.

### Current evidence that constrains the design

- `server/linux/CMakeLists.txt` produces the `game` and `db` ELF executables through the `metin2_game_exe` and `metin2_db_exe` targets.
- `server/linux/src/libsql/CMakeLists.txt` links the legacy MySQL API against the MariaDB client library.
- The current generated cache resolves `MYSQL_LIBRARY` to `/usr/lib/libmariadb.so`; it must not be copied into the Alpine build.
- Game processes read `CONFIG` from their working directory, while the DB process reads `conf.txt` from its working directory.
- The checked-in runtime configurations use `DB_PORT=15000`, SQL databases named `account`, `common`, `hotbackup`, `log`, and `player`, and relative runtime assets under the corresponding process directories.
- `server/docker` is currently empty and the Windows host currently has `wsl.exe`, CMake, and Ninja but no `docker` command. Docker validation therefore runs from the Docker-enabled WSL/Alpine environment or Docker Desktop integration once available.

## Task 1: Freeze the legacy runtime contract before containerization

**Files:**

- Create: `docs/operations/metin2-alpine-runtime-baseline.md`
- Read: `AGENTS.md`
- Read: `docs/decisions/0001-postgresql-without-timescaledb-by-default.md`
- Read: `server/metin2_server+src/metin2/server/*/CONFIG`
- Read: `server/metin2_server+src/metin2/server/db/conf.txt`
- Read: `server/linux/CMakeLists.txt`
- Read: `server/linux/src/db/main/CMakeLists.txt`
- Read: `server/linux/src/game/main/CMakeLists.txt`

- [ ] **Step 1: Inventory all process directories and configuration files**

Run from `C:\projects\Metin2`:

```powershell
Get-ChildItem -Force -Recurse .\server\metin2_server+src\metin2\server -Filter CONFIG | Select-Object FullName,Length
Get-ChildItem -Force .\server\metin2_server+src\metin2\server\db -Filter conf.txt | Select-Object FullName,Length
Get-ChildItem -Force -Recurse .\server\linux\build -File | Where-Object { $_.Name -in @('game','db') } | Select-Object FullName,Length
```

Expected: the output identifies `auth`, `db`, `game99`, and the `channel1` through `channel4` process directories, plus the two current Linux build outputs. The command must not modify or delete the existing generated build.

- [ ] **Step 2: Build a process/port/database matrix**

Extract these keys from every `CONFIG` and from `db/conf.txt`:

```powershell
rg -n "^(CHANNEL|HOSTNAME|PORT|P2P_PORT|DB_ADDR|DB_PORT|PLAYER_SQL|COMMON_SQL|LOG_SQL|BIND_PORT|SQL_ACCOUNT|SQL_COMMON|SQL_HOTBACKUP|SQL_PLAYER)" .\server\metin2_server+src\metin2\server
```

Record the result in `docs/operations/metin2-alpine-runtime-baseline.md` as a table with columns `service`, `working directory`, `executable`, `configuration`, `client port`, `P2P port`, `legacy DB-cache endpoint`, and `SQL databases`. Derive port mappings from the checked-in files instead of inventing them in Compose.

- [ ] **Step 3: Identify runtime-relative assets and write requirements**

Run:

```powershell
rg -n "fopen\(|ifstream|ofstream|LoadFile\(|CONFIG|conf\.txt|CMD|locale|package|data|log|cores" .\server\metin2_server+src\metin2\src\server --glob '*.c' --glob '*.cc' --glob '*.cpp' --glob '*.h' --glob '*.hpp'
```

Document which process directories need `data`, `locale`, `package`, `mark`, `CMD`, writable `log`, `cores`, and `pid` paths. The container design must preserve the current working-directory assumptions rather than changing source code to use absolute paths.

- [ ] **Step 4: Define smoke and full acceptance boundaries**

Document the smoke profile as `db`, `auth`, and `channel1/first`; document the full profile as `db`, `auth`, `game99`, and all `channel1` through `channel4` `first`, `game1`, and `game2` directories that actually exist. State that a process is ready only after its configured TCP port accepts connections and its logs show successful startup; container creation alone is not readiness.

## Task 2: Make the CMake build reproducible on Alpine

**Files:**

- Modify: `server/linux/CMakeLists.txt`
- Create: `server/linux/CMakePresets.json`
- Modify: `server/linux/src/db/main/CMakeLists.txt` only if an install rule needs target-local placement
- Modify: `server/linux/src/game/main/CMakeLists.txt` only if an install rule needs target-local placement

- [ ] **Step 1: Make dependency discovery portable between Debian-like Linux and Alpine**

Update the existing MariaDB/MySQL discovery so it works with Alpine's `mariadb-connector-c-dev` package and does not report only Debian package names. Prefer a target-based `pkg-config` path when available, retain a clear explicit include/library fallback, and keep the existing `MYSQL_*` compatibility variables consumed by `metin2_sql` and the game/DB targets.

Keep OpenSSL, zlib, Threads, and bundled Crypto++ as explicit dependencies. Do not replace the legacy SQL wrapper or introduce a PostgreSQL client library in this compatibility task.

- [ ] **Step 2: Add clean Alpine configure/build presets**

Create `server/linux/CMakePresets.json` with a schema version supported by the declared CMake minimum and these presets:

```text
alpine-release -> Ninja, out/alpine-release, Release, compile_commands enabled
alpine-debug   -> Ninja, out/alpine-debug, Debug, compile_commands enabled
```

The presets must not reuse `server/linux/build`, which currently contains WSL-generated paths. Keep `METIN2_32BIT` explicitly disabled for the first Alpine x86_64 baseline unless the current runtime assets and deployment target require a separate 32-bit investigation.

- [ ] **Step 3: Add installable executable targets**

Add install rules for the `game` and `db` runtime executables under a predictable prefix such as `/opt/metin2/bin`. Do not install static libraries, source code, or the stale generated build directory into the runtime image.

- [ ] **Step 4: Verify a clean native Linux configure and build**

Run inside the Alpine WSL/Docker build environment from `server/linux`:

```sh
cmake --preset alpine-release
cmake --build --preset alpine-release --target metin2-all --parallel
cmake --install out/alpine-release --prefix /opt/metin2
file out/alpine-release/src/game/main/game out/alpine-release/src/db/main/db
```

Expected: CMake reports the Alpine MariaDB client, the `metin2-all` target completes, the installed files are `/opt/metin2/bin/game` and `/opt/metin2/bin/db`, and `file` identifies Linux ELF x86-64 executables. Verify that the fresh cache contains no `/mnt/c/` or `C:/projects/` path:

```sh
! grep -R -n -E '/mnt/c/|C:/projects|C:\\\\projects' out/alpine-release/CMakeCache.txt
```

## Task 3: Create a minimal, reproducible Alpine multi-stage image

**Files:**

- Create: `server/docker/Dockerfile`
- Create: `server/docker/.dockerignore`
- Create: `server/docker/entrypoint.sh`
- Create: `server/docker/healthcheck.sh`

- [ ] **Step 1: Define the build context and exclusions**

Use `server` as the Docker build context. The `.dockerignore` must exclude the generated `linux/build` and `linux/out` directories, Windows/IDE metadata, `.agents`, `.commandcode`, SQL development leftovers not needed by the image, and any `.env` files. It must retain `linux/extern/libcryptopp.a`, the Crypto++ headers, the Linux CMake sources, the legacy server source tree, and the runtime assets required by the selected process directories.

- [ ] **Step 2: Implement the Alpine builder stage**

Use `alpine:3.24.1` for both build and runtime stages; this is the current stable Alpine release recorded by the Alpine project on the plan date. Install only the builder packages required by the verified CMake configure: `build-base`, `cmake`, `ninja`, `pkgconf`, `linux-headers`, `openssl-dev`, `zlib-dev`, and `mariadb-connector-c-dev`. Use the repository's bundled Crypto++ artifact instead of downloading a second copy.

Configure and build with the presets from Task 2, then install only the two executables into `/opt/metin2/bin`.

- [ ] **Step 3: Implement the minimal runtime stage**

Use `alpine:3.24.1` for runtime. Install only runtime libraries required by `ldd` and the executable startup test: `libgcc`, `libstdc++`, `openssl`, `zlib`, and `mariadb-connector-c`, adjusting names only when `apk` identifies the exact package mapping in the same Alpine release.

Copy the installed `game` and `db` binaries and the selected legacy runtime assets into the final image. Create a non-root `metin2` user and group, give it ownership of runtime log/core/pid locations, and keep the root filesystem otherwise immutable where the legacy process permits it. Do not expose database credentials through Dockerfile `ARG` or `ENV` defaults.

- [ ] **Step 4: Preserve per-process working directories**

Make the entrypoint accept an explicit service directory and executable name, copy or materialize only the selected process configuration into a writable runtime location, and `exec` the process so signals and exit codes reach Compose. The entrypoint must:

1. validate that the requested process directory and executable exist;
2. render `DB_ADDR`, `DB_PORT`, `PLAYER_SQL`, `COMMON_SQL`, and `LOG_SQL` against Compose service names without changing client/P2P ports;
3. render the DB process's `SQL_ACCOUNT`, `SQL_COMMON`, `SQL_HOTBACKUP`, and `SQL_PLAYER` values against the MariaDB service;
4. avoid printing passwords in logs;
5. run from the rendered directory so `CONFIG`, `conf.txt`, `CMD`, locale, data, and package lookups retain their legacy behavior.

The healthcheck helper must test the configured local TCP port with an Alpine-available tool and return non-zero until the process is listening. It must not treat a PID file alone as readiness.

- [ ] **Step 5: Verify image contents and dynamic dependencies**

Run:

```sh
docker build --file server/docker/Dockerfile --tag metin2-server:alpine-dev server
docker run --rm --entrypoint /bin/sh metin2-server:alpine-dev -c 'file /opt/metin2/bin/game /opt/metin2/bin/db && ldd /opt/metin2/bin/game && ldd /opt/metin2/bin/db'
```

Expected: both files are Linux ELF binaries, `ldd` reports no `not found` entries, and the image contains no compiler, CMake, Ninja, source tree, or static build libraries in the final runtime stage.

## Task 4: Orchestrate MariaDB and the legacy processes with Compose

**Files:**

- Create: `server/docker/compose.yaml`
- Create: `server/docker/.env.example`
- Create: `server/docker/.gitignore`
- Create: `server/docker/schema-init.sh` or an equivalent one-shot initialization mechanism

- [ ] **Step 1: Define the MariaDB compatibility service**

Use `mariadb:10.11.18` for the current MySQL/MariaDB schema. Configure a named volume, a healthcheck that executes a local SQL ping, and no host port publication by default. The service must use the credentials supplied through the local `.env` file and never hard-code production secrets in Compose.

The schema initialization service must wait for MariaDB health, create `account`, `common`, `hotbackup`, `log`, and `player`, and import the corresponding files from `server/metin2_mysql_dump`. It must be idempotent for a fresh named volume and must fail visibly if any dump import fails. Do not add `down -v` or automatic volume deletion to any script.

- [ ] **Step 2: Define explicit Compose networks and volumes**

Create an internal backend network for MariaDB, the legacy DB cache process, auth, and game services. Add a public-facing network only to services that need client access. Use named volumes for database data and per-process writable logs/cores where persistence is needed; mount SQL dumps read-only into the schema initializer.

- [ ] **Step 3: Define the default smoke profile**

The default `docker compose up -d` path must start:

```text
mariadb -> schema-init -> metin2-db -> metin2-auth -> metin2-channel1-first
```

Use `depends_on` health/completion conditions. Configure `metin2-db` with working directory `server/db`, executable `db`, and internal port `15000`; configure auth and the first game process with the exact ports from their checked-in `CONFIG` files. Publish only the client-facing auth/game ports needed for local testing.

- [ ] **Step 4: Add the full server profile without changing the smoke contract**

Add the remaining existing `game99` and channel process directories under a Compose profile named `full`. Each service must have a unique service name, explicit working directory, executable, port mapping from the runtime matrix, healthcheck, restart policy appropriate for development, and the same database endpoint variables. Do not collapse multiple independent legacy processes into a shell background process; Compose must supervise them separately.

- [ ] **Step 5: Document safe local environment setup**

Provide `.env.example` with non-secret development values for MariaDB root credentials, the `metin2` application credentials, and the Compose project name. Add `.env` to `server/docker/.gitignore`. Document the exact invocation from the workspace root:

```powershell
Copy-Item .\server\docker\.env.example .\server\docker\.env
docker compose --env-file .\server\docker\.env -f .\server\docker\compose.yaml up -d
```

The documentation must state that `.env.example` values are for local development only and must be replaced before any shared deployment.

## Task 5: Validate the one-command deployment end-to-end

**Files:**

- Modify: `docs/operations/metin2-alpine-runtime-baseline.md`
- Create or modify: `docs/operations/metin2-docker-compose.md`
- Modify: `AGENTS.md` with the verified commands and known limitations

- [ ] **Step 1: Validate Compose syntax and service graph**

Run from the workspace root in the Docker-enabled environment:

```sh
docker compose --env-file server/docker/.env.example -f server/docker/compose.yaml config --quiet
docker compose --env-file server/docker/.env.example -f server/docker/compose.yaml config --services
```

Expected: Compose accepts the file, lists MariaDB, schema initialization, the legacy DB process, auth, and the smoke game process, and reports no unresolved environment variables or duplicate host ports.

- [ ] **Step 2: Build and start the smoke profile**

Run:

```sh
docker compose --env-file server/docker/.env -f server/docker/compose.yaml build
docker compose --env-file server/docker/.env -f server/docker/compose.yaml up -d
docker compose --env-file server/docker/.env -f server/docker/compose.yaml ps
docker compose --env-file server/docker/.env -f server/docker/compose.yaml logs --no-color --tail=200
```

Expected: MariaDB and schema initialization complete, the legacy DB process listens on `15000`, auth listens on its configured port, the smoke game listens on its configured port, and logs do not contain a failed SQL connection or missing relative asset error.

- [ ] **Step 3: Verify database contents and inter-service connectivity**

Run a read-only SQL check through the MariaDB container and port checks from the relevant application containers. Verify that all five databases exist, that the `metin2` user can authenticate, and that the legacy DB process resolves the Compose MariaDB hostname rather than `127.0.0.1`.

- [ ] **Step 4: Exercise restart behavior without deleting data**

Run:

```sh
docker compose --env-file server/docker/.env -f server/docker/compose.yaml restart metin2-db metin2-auth metin2-channel1-first
docker compose --env-file server/docker/.env -f server/docker/compose.yaml ps
docker compose --env-file server/docker/.env -f server/docker/compose.yaml down
```

Expected: services restart and become healthy again, the named MariaDB volume remains present, and `down` stops/removes containers and networks without deleting database data. A volume-destructive command such as `down -v` requires explicit confirmation outside this plan.

- [ ] **Step 5: Validate the full profile separately**

After the smoke profile passes, run the full profile and verify every service from the runtime matrix. Record any process that cannot start because of a legacy platform assumption as a separate compatibility issue; do not hide it by weakening healthchecks or running the process in the background.

## Task 6: Document the operational boundary and handoff to PostgreSQL/Rust

**Files:**

- Modify: `AGENTS.md`
- Modify: `docs/operations/metin2-alpine-runtime-baseline.md`
- Create: `docs/operations/metin2-docker-compose.md`
- Reference: `docs/decisions/0001-postgresql-without-timescaledb-by-default.md`

- [ ] **Step 1: Publish the supported commands**

Document the verified Alpine build commands, the smoke Compose command, the full-profile command, log inspection, health inspection, and the safe shutdown command. Separate commands that run in Windows PowerShell, Alpine WSL, and Docker-enabled environments so a failure is attributable to the correct layer.

- [ ] **Step 2: Record known compatibility constraints**

Document that the current C++ process layout, MySQL API, MariaDB schemas, relative working directories, and legacy `CONFIG` files remain in force for this baseline. Record the current Docker prerequisite gap on the Windows host if Docker is still unavailable there, and point to the Docker-enabled WSL/Alpine path used for verification.

- [ ] **Step 3: Define the migration handoff**

State that PostgreSQL migration begins only after this baseline has reproducible build/run evidence and the current SQL usage has been catalogued. The Rust rewrite may unify `game` and `db`, but that process/data ownership change requires a new architecture document and ADR; it must not be smuggled into the compatibility image.

## Verification checklist

Before declaring this plan complete, run the project verification skill and confirm all of the following with fresh output:

- `cmake --preset alpine-release` succeeds in a clean output directory.
- `cmake --build --preset alpine-release --target metin2-all` succeeds.
- Installed `game` and `db` binaries are Alpine-compatible ELF files with no missing shared libraries.
- `docker compose ... config --quiet` succeeds.
- MariaDB health and all five schema imports succeed.
- The legacy DB cache process, auth, and smoke game process become healthy through Compose.
- No tracked or generated file contains local WSL paths in the Alpine build cache.
- No credentials are committed, printed by the entrypoint, or baked into image layers.
- `docker compose down` does not remove the named database volume.
- The docs identify PostgreSQL as the future target and MariaDB/MySQL as the current compatibility dependency.

If Docker is unavailable in the active environment, report the exact missing prerequisite and stop at static validation; do not claim the image or Compose stack passed.
