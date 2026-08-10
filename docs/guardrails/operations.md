---
Type: Guardrail
Status: Current
Audience: Operators, contributors, agents
Last verified: 2026-08-10
---

# Guardrail: operations (WSL runtime)

Rules for running the legacy test stack on the WSL Debian-M2 environment. Source of truth: `../../AGENTS.md` runbook; `../../CHANGELOG.md` (session evidence).

## 1. WSL memory: minimal start only

- **Rule:** the full stack (9 cores) blows the machine's memory (4 GB host, WSL cap 2 GB). Use `scripts/start_m2_min.sh` (db + auth + ch1-core1 — enough for login) unless there is more RAM.
- **Why:** OOM kills WSL sessions mid-verification; the environment is the project's bottleneck.
- **Evidence:** AGENTS.md runbook notes; CHANGELOG 2026-08-08/09 session logs (stack re-armed at 617 MB/2 GB).
- **Consequence:** WSL crash `Wsl/Service/E_UNEXPECTED`; lost verification evidence.
- **Status:** Active.

## 2. Mandatory boot order

- **Rule:** `mariadb → srv1-db → srv1-auth1 → cores`. The db peer will not start cleanly otherwise.
- **Why:** the legacy topology has a hard startup dependency chain (verified empirically in every session).
- **Evidence:** AGENTS.md runbook ("Mandatory boot order"); CHANGELOG session logs.
- **Consequence:** auth/cores spin without the db; login fails with timeouts.
- **Status:** Active.

## 3. Always `sync` after deploy in WSL

- **Rule:** after deploying binaries in WSL, run `sync` and verify with md5sum. WSL crashes can LOSE writes without flushing (ext4).
- **Why:** the previous model deployed binaries that were never flushed → the running stack used stale binaries while source said otherwise.
- **Evidence:** AGENTS.md "CAUTION: WSL crashes can LOSE writes"; CHANGELOG 2026-08-08 (md5-synced both source copies before deploy).
- **Consequence:** undetected stale binaries; "works in source, fails in runtime" mysteries.
- **Status:** Active.

## 4. Check the WSL IP after every restart

- **Rule:** `serverinfo.py` bakes host `172.25.104.175` (WSL eth0 IP) — **check after every WSL restart**; the IP can change.
- **Why:** the client connects by IP from `root/serverinfo.py` in the pack; a changed IP silently breaks login.
- **Evidence:** AGENTS.md protocol facts + runbook; CHANGELOG 2026-08-08/09 sessions.
- **Consequence:** client cannot reach auth/channel; "login broken" with a network cause.
- **Status:** Active.

## 5. No runtime/build artifacts in git

- **Rule:** the runtime (`source/deploy/`), installed clients, packs, build artifacts (`**/target/`, obj/bin/Debug/Release), `Extern/`, `graphify-out/`, `.opencode/` are NOT in the repo. Sources only (~150–200 MB) when the GitHub repo is created.
- **Why:** the SSD was FULL (5 GB free) after accumulated artifacts; binaries/packs go to Releases or external storage.
- **Evidence:** `.gitignore` (root); `../../ROADMAP.md` "GitHub repository (preparation)"; AGENTS.md layout table (`source\deploy\` gitignored).
- **Consequence:** repo bloat, secret/asset leaks, CI breakage.
- **Status:** Active.

Related: [`rust-rewrite.md`](rust-rewrite.md) (two source copies), [`data-and-encoding.md`](data-and-encoding.md).
