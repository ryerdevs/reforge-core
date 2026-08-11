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

## 6. WSL memory pressure kills stack processes silently

- **Rule:** WSL builds (cargo/gcc) with the game stack running can kill stack processes WITHOUT any log or OOM entry in dmesg — the process just disappears. Build with the game idle (or accept a possible core restart), and ALWAYS verify the stack processes after a WSL build (`pgrep` + `ss -ltn` on 30000/30001/30003/3307).
- **Why:** a killed core1 looks like a crash with no evidence trail; hours can go into "why did the core die" when the answer is memory pressure (4 GB host / 2 GB WSL cap — §1).
- **Evidence:** 2026-08-11: core1 died 2× during WSL builds, no SYSERR, no OOM in dmesg (session logs; stack re-verified with `pgrep`/`ss` after builds).
- **Consequence:** silent session drops (players disconnect), lost verification state, false "instability" hypotheses.
- **Status:** Active.

## 7. Never `cp` over a running binary

- **Rule:** `cp` over a binary that is executing fails with `Text file busy`. Deploy sequence: `pkill` the process → WAIT until it is gone (`pgrep` empty) → `cp` → `sync` → relaunch with the same command line → verify with `ss -ltnp`/`pgrep`.
- **Why:** 2026-08-11 the deploy pattern failed 2× with Text file busy when the target process was still running.
- **Evidence:** `scripts/gpg`-adjacent deploy scripts (pkill → cp → sync → relaunch, e.g. the mysql_proxy redeploys); `ss -ltnp` verification after relaunch.
- **Consequence:** failed deploy, stale binary running, or a half-copied file.
- **Status:** Active.

## 8. `wsl.exe` mangles quoted arguments from PowerShell

- **Rule:** PowerShell passes arguments to `wsl -d … -- bash -c "…"` after its own quote processing: inner double quotes, `$VAR`, parentheses and `%` get mangled/expanded. For WSL commands needing quoting, write a script file (`.sh`) in the temp dir and run `wsl -d Debian-M2 -- bash <file>` — no inline quoting.
- **Why:** 2026-08-11: 5+ attempts with inline quoting failed (mariadb `-e` with quotes, `python3 -c`, grep with parentheses); the script-file pattern worked first try every time.
- **Evidence:** session log of failed inline commands vs the script-file pattern (SQL/script files run via `wsl -- bash <file>`).
- **Consequence:** wasted round trips, corrupted commands that look right.
- **Status:** Active.

## 9. E2E tests must leave no residue

- **Rule:** E2E suites use UNIQUE names per run (`e2e_<ts>` / `m2e2_<ts>`) and a `trap cleanup EXIT` that deletes exactly those rows. Do a periodic sweep for residue (`WHERE name/login/account LIKE 'e2e_%'` / `m2e2_%`) — leftovers from old suites survive their cleanups.
- **Why:** 2026-08-11 a parity run found 2 `e2e_rust_*` players and 4 `m2e2_*` messenger rows from previous suites; swept manually (documented in the parity snapshot header).
- **Evidence:** `parity_check.py --snapshot` DIFF output (2026-08-11); the `e2e_db.sh` trap — verified 0 residue after the full 98-assert run.
- **Consequence:** parity false positives, DB pollution, throwaway rows mixed with the user's real data.
- **Status:** Active.

Related: [`rust-rewrite.md`](rust-rewrite.md) (two source copies), [`data-and-encoding.md`](data-and-encoding.md).
