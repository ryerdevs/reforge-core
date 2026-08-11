#!/bin/bash
# =============================================================================
# parity_boot.sh — G-PG boot parity harness
# Spec: docs/plans/server-rewrite.md §8.2.1d  (ADR-0005 Accepted, G-PG)
#
# Compares the C++ baseline boot+login behavior on MariaDB vs on PostgreSQL
# through the mysql_proxy adapter (127.0.0.1:3307, NOT built yet — the proxy
# lane owns it; the orchestrator runs the gate when it exists).
#
# Usage (run inside WSL Debian-M2, as root):
#   bash parity_boot.sh                 # baseline run + PG run + compare
#   bash parity_boot.sh --only-baseline # capture MariaDB baseline snapshot only
#   bash parity_boot.sh --only-pg       # PG run + compare vs existing baseline
#
# What each run does:
#   1. activates the conf variant (MariaDB: *_mariadb / PG: *_pg over the active
#      conf.txt / CONFIG files — see B6);
#   2. starts the minimal stack (mariadb + srv1-db + srv1-auth1 + srv1-ch1-core1,
#      same order as scripts/start_m2_min.sh);
#   3. waits for the db boot to finish (last "Start of pid" block in db/syslog
#      shows "BANWORD: total" + port 30000 listening — the fprintf "Complete!"
#      lines stay in the libc stdout buffer and never reach db/stdout);
#   4. snapshots db/auth/core syslogs+stdout+syserr into $WORK/<label>/.
#
# Comparison (PG run vs baseline snapshot):
#   A. NO NEW SYSERR lines (set difference of normalized SYSERR lines; known
#      baseline errors are allowed, new ones are not);
#   B. boot table lines equal (MOB #/SKILL: #/SHOP: #/REFINE: id/ITEM_ATTR:/
#      BANWORD:/OBJ_PROTO:/OBJ:/Complete!/Loading ... from MySQL/GM: lines,
#      timestamps stripped, compared as sorted sets);
#   C. LoginSuccess of account `test` found in core1 syslog of the PG run
#      (requires a REAL client login test/1234 during the PG run — AGENTS.md
#      runbook; without a client this check fails with a clear message).
#
# Always restores the MariaDB conf variants and leaves the stack STOPPED on
# exit (initial state). Exit: 0 = all checks pass, 1 = parity failure,
# 2 = usage/environment error.
# =============================================================================
set -u

SV=/home/m2/source/metin2_svfiles/main/srv1
WORK=/tmp/gpg/parity_boot
LABEL_BASE=baseline_mariadb
LABEL_PG=pg_proxy
BOOT_WAIT=90          # seconds to wait for db "Complete!" line
MODE=both

usage() { grep '^#' "$0" | sed 's/^# \{0,1\}//' | head -30; }

for arg in "$@"; do
  case "$arg" in
    --only-baseline) MODE=baseline ;;
    --only-pg) MODE=pg ;;
    -h|--help) usage; exit 0 ;;
    *) echo "ERROR: unknown argument: $arg"; usage; exit 2 ;;
  esac
done

mkdir -p "$WORK"

# ---------------------------------------------------------------- helpers
stop_stack() {
  for p in srv1-db srv1-auth1 srv1-ch1-core1; do
    pkill -f "\./$p" 2>/dev/null
  done
  for i in $(seq 1 20); do
    if ! pgrep -f 'srv1-(db|auth1|ch1-core1)' >/dev/null 2>&1; then break; fi
    sleep 1
  done
  if pgrep -f 'srv1-(db|auth1|ch1-core1)' >/dev/null 2>&1; then
    echo "ERROR: stack processes still running, cannot continue"; exit 2
  fi
}

activate_confs() {
  local variant="$1"   # mariadb | pg
  cp -p "$SV/db/conf.txt_$variant" "$SV/db/conf.txt"
  cp -p "$SV/auth1/CONFIG_$variant" "$SV/auth1/CONFIG"
  cp -p "$SV/chan/ch1/core1/CONFIG_$variant" "$SV/chan/ch1/core1/CONFIG"
  echo "confs activated: $variant"
}

start_stack() {
  service mariadb start >/dev/null 2>&1
  sleep 4
  cd "$SV/db" && setsid nohup ./srv1-db > stdout 2>&1 &
  sleep 5
  cd "$SV/auth1" && setsid nohup ./srv1-auth1 > stdout 2>&1 &
  sleep 5
  cd "$SV/chan/ch1/core1" && setsid nohup ./srv1-ch1-core1 > stdout 2>&1 &
}

wait_boot() {
  # The db boot logs (sys_log(0)) go to db/syslog; the "Complete!" fprintf lines
  # stay in the libc stdout buffer (redirected to file) and never reach it.
  # Signal of a finished boot: the LAST "Start of pid" block contains
  # "BANWORD: total" (late InitializeTables step) and port 30000 is listening.
  local t=0
  while [ $t -lt $BOOT_WAIT ]; do
    if awk '/Start of pid/{buf=""} {buf=buf $0 "\n"} END{exit !(buf ~ /BANWORD: total/)}' \
         "$SV/db/syslog" 2>/dev/null && \
       ss -tln 2>/dev/null | grep -q ':30000'; then
      sleep 5   # let auth/core settle after db boot
      return 0
    fi
    sleep 2
    t=$((t + 2))
  done
  return 1
}

snapshot() {
  local label="$1"
  rm -rf "$WORK/$label"
  mkdir -p "$WORK/$label"
  # syslogs accumulate across boots ("Start of pid: N" blocks) — keep ONLY the
  # last boot block so the compare does not see historical runs.
  local lastblk='awk '\''/Start of pid/{buf=""} {buf=buf $0 "\n"} END{printf "%s", buf}'\'''
  eval "$lastblk" < "$SV/db/syslog"          > "$WORK/$label/db_syslog"    2>/dev/null || true
  cp -p "$SV/db/syserr"        "$WORK/$label/db_syserr"        2>/dev/null
  cp -p "$SV/db/stdout"        "$WORK/$label/db_stdout"        2>/dev/null
  eval "$lastblk" < "$SV/auth1/syslog"       > "$WORK/$label/auth1_syslog" 2>/dev/null || true
  cp -p "$SV/auth1/syserr"     "$WORK/$label/auth1_syserr"     2>/dev/null
  eval "$lastblk" < "$SV/chan/ch1/core1/syslog" > "$WORK/$label/core1_syslog" 2>/dev/null || true
  cp -p "$SV/chan/ch1/core1/syserr" "$WORK/$label/core1_syserr" 2>/dev/null
  cp -p "$SV/chan/ch1/core1/stdout" "$WORK/$label/core1_stdout" 2>/dev/null
  echo "snapshot: $WORK/$label"
}

# extract SYSERR lines from a snapshot dir, normalized:
#   strip the syslog line prefix AND the inner SYSERR timestamp
#   ("SYSERR: Aug 10 21:24:58 :: msg" -> "SYSERR: msg")
extract_syserr() {
  local label="$1"
  cat "$WORK/$label"/* 2>/dev/null \
    | grep -aho 'SYSERR.*' \
    | sed -E 's/^SYSERR: [A-Z][a-z]{2} [ 0-9]{1,2} [0-9]{2}:[0-9]{2}:[0-9]{2} :: /SYSERR: /' \
    | sed -E 's/^[[:space:]]*//' \
    | sort -u
}

# extract boot table lines from a snapshot dir, normalized:
#   strip the syslog line prefix ("Aug 10 21:24:57 :: msg" -> "msg")
extract_boot() {
  local label="$1"
  cat "$WORK/$label"/db_syslog "$WORK/$label"/db_stdout \
      "$WORK/$label"/core1_syslog "$WORK/$label"/core1_stdout 2>/dev/null \
    | grep -ahE 'MOB #|SKILL: #|SHOP: #|REFINE: id|ITEM_ATTR:|BANWORD:|OBJ_PROTO:|OBJ:|Success (PLAYER|ACCOUNT|COMMON|LOG)' \
    | sed -E 's/^[[:space:]]*\[[^]]*\]//' \
    | sed -E 's/^[[:space:]]*[A-Z][a-z]{2} [ 0-9]{1,2} [0-9]{2}:[0-9]{2}:[0-9]{2} :: //' \
    | sed -E 's/^[[:space:]]*//' \
    | sort -u
}

# ---------------------------------------------------------------- runs
run_baseline() {
  echo "===== BASELINE RUN (MariaDB confs) ====="
  stop_stack
  activate_confs mariadb
  start_stack
  if ! wait_boot; then
    echo "FAIL: db boot did not complete (no 'BANWORD: total' in db/syslog within ${BOOT_WAIT}s)"
    snapshot "$LABEL_BASE"
    return 1
  fi
  snapshot "$LABEL_BASE"
  echo "baseline boot OK; SYSERR count: $(extract_syserr "$LABEL_BASE" | wc -l)"
  echo "baseline boot lines: $(extract_boot "$LABEL_BASE" | wc -l)"
  return 0
}

run_pg() {
  echo "===== PG RUN (proxy confs, 127.0.0.1:3307) ====="
  if [ ! -d "$WORK/$LABEL_BASE" ]; then
    echo "ERROR: no baseline snapshot at $WORK/$LABEL_BASE — run --only-baseline first"
    return 2
  fi
  if ! ss -tln 2>/dev/null | grep -q ':3307'; then
    echo "WARN: nothing listening on 127.0.0.1:3307 (mysql_proxy not deployed yet?)"
    echo "      boot will fail — this is expected until the proxy lane delivers it."
  fi
  stop_stack
  activate_confs pg
  start_stack
  if ! wait_boot; then
    echo "FAIL: db boot did not complete on PG confs (no 'BANWORD: total' in db/syslog within ${BOOT_WAIT}s)"
    snapshot "$LABEL_PG"
    return 1
  fi
  snapshot "$LABEL_PG"
  return 0
}

# ---------------------------------------------------------------- compare
compare() {
  echo "===== COMPARE (PG run vs baseline) ====="
  local rc=0

  local base_err pg_err new_err
  base_err=$(extract_syserr "$LABEL_BASE")
  pg_err=$(extract_syserr "$LABEL_PG")
  new_err=$(comm -13 <(printf '%s\n' "$base_err") <(printf '%s\n' "$pg_err"))
  if [ -n "$new_err" ]; then
    echo "FAIL (A): NEW SYSERR lines in PG run:"
    printf '%s\n' "$new_err" | sed 's/^/    /'
    rc=1
  else
    echo "PASS (A): no NEW SYSERR lines in PG run (baseline $(printf '%s\n' "$base_err" | wc -l) vs PG $(printf '%s\n' "$pg_err" | wc -l))"
  fi

  local boot_diff
  boot_diff=$(diff <(extract_boot "$LABEL_BASE") <(extract_boot "$LABEL_PG") || true)
  if [ -n "$boot_diff" ]; then
    echo "FAIL (B): boot table lines differ:"
    printf '%s\n' "$boot_diff" | sed 's/^/    /'
    rc=1
  else
    echo "PASS (B): boot table lines equal ($(extract_boot "$LABEL_BASE" | wc -l) lines)"
  fi

  local nlogin
  nlogin=$(grep -ac 'LoginSuccess' "$WORK/$LABEL_PG/core1_syslog" 2>/dev/null || true)
  nlogin=${nlogin:-0}
  if [ "$nlogin" -ge 1 ]; then
    echo "PASS (C): LoginSuccess found in core1 syslog of PG run ($nlogin)"
  else
    echo "FAIL (C): no LoginSuccess in PG run core1 syslog — requires a REAL client login (test/1234) during the PG run (AGENTS.md runbook)"
    rc=1
  fi
  return $rc
}

# ---------------------------------------------------------------- main
rc=0
case "$MODE" in
  baseline)
    run_baseline; rc=$?
    ;;
  pg)
    run_pg; rc=$?
    if [ $rc -eq 0 ]; then compare; rc=$?; fi
    ;;
  both)
    run_baseline || rc=1
    run_pg || rc=1
    compare; rc=$?
    ;;
esac

# always restore MariaDB confs and leave the stack stopped (initial state)
stop_stack
activate_confs mariadb
sync
if [ $rc -eq 0 ]; then
  echo "PARITY GREEN (exit 0)"
else
  echo "PARITY FAILED (exit $rc)"
fi
exit $rc
