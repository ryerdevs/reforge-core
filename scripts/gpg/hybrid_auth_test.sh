#!/bin/bash
# =============================================================================
# hybrid_auth_test.sh — F2a hybrid test harness: swap the C++ auth for the Rust
# auth (server_realms --role auth) on the SAME IP:port (172.25.104.175:30001).
#
# The db (srv1-db, 30000) and the core (srv1-ch1-core1, 30003) stay RUNNING —
# only the auth process is replaced, so the client can be pointed at the Rust
# auth without a full stack restart.
#
# Usage (run inside WSL Debian-M2, as root; the orchestrator invokes it):
#   bash hybrid_auth_test.sh <rust_auth_bin> <config_toml>   # swap auth -> Rust
#   bash hybrid_auth_test.sh --restore                        # C++ auth back
#
# Steps of the swap:
#   1. pkill -f srv1-auth1                                    # C++ auth only
#   2. wait until 127.0.0.1:30001 is free; assert core1 30003 still alive
#   3. start "$1" --config "$2" in background (log /tmp/gpg/hybrid_auth.log,
#      pid recorded in /tmp/gpg/hybrid_auth.pid)
#   4. wait until 172.25.104.175:30001 is LISTENING (or timeout -> FAIL)
#   5. print the mechanical peer check to run FROM WINDOWS:
#        cd C:\projects\Metin2\source\reforge
#        cargo run --example f16_peer -- 172.25.104.175 30001 --login3
#      Expected: GC_AUTH_SUCCESS (auth Rust answered LOGIN3 for test/1234).
#      NOTE: the real end-to-end test is the client login test/1234
#      (AGENTS.md runbook); the f16_peer example is the mechanical check.
#
# Restore (--restore):
#   kill the recorded Rust auth pid, then start the C++ auth again:
#     cd $SV/auth1 && setsid nohup ./srv1-auth1 > stdout 2>&1 &
#
# Exit: 0 = swap OK (auth listening), 1 = precondition/startup failure,
#       2 = usage error.
# =============================================================================
set -u

SV=/home/m2/source/metin2_svfiles/main/srv1
AUTH_IP=172.25.104.175
AUTH_PORT=30001
LOG=/tmp/gpg/hybrid_auth.log
PIDFILE=/tmp/gpg/hybrid_auth.pid
WAIT=30   # seconds to wait for the auth port

usage() { sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; }

if [ "${1:-}" = "--restore" ]; then
  echo "== restore C++ auth =="
  if [ -f "$PIDFILE" ]; then
    kill "$(cat "$PIDFILE")" 2>/dev/null && echo "Rust auth pid $(cat "$PIDFILE") killed"
    rm -f "$PIDFILE"
  else
    pkill -f 'server_realms' 2>/dev/null && echo "server_realms killed (no pidfile)"
  fi
  for i in $(seq 1 10); do
    if ! ss -tln 2>/dev/null | grep -q ":${AUTH_PORT}"; then break; fi
    sleep 1
  done
  cd "$SV/auth1" && setsid nohup ./srv1-auth1 > stdout 2>&1 &
  for i in $(seq 1 15); do
    if ss -tln 2>/dev/null | grep -q ":${AUTH_PORT}"; then break; fi
    sleep 1
  done
  if ss -tln 2>/dev/null | grep -q ":${AUTH_PORT}"; then
    echo "OK: C++ auth listening on ${AUTH_IP}:${AUTH_PORT}"
    sync; exit 0
  fi
  echo "FAIL: C++ auth did not bind ${AUTH_PORT}"; exit 1
fi

if [ $# -lt 2 ]; then
  echo "ERROR: usage: hybrid_auth_test.sh <rust_auth_bin> <config_toml> | --restore"
  usage; exit 2
fi
RUST_BIN="$1"
RUST_CFG="$2"

echo "== preconditions =="
pgrep -af 'srv1-auth1' || { echo "FAIL: srv1-auth1 not running (expected before swap)"; exit 1; }
pgrep -af 'srv1-ch1-core1' || { echo "FAIL: core1 not running"; exit 1; }
ss -tln 2>/dev/null | grep -q ':30003' || { echo "FAIL: core1 port 30003 not listening"; exit 1; }
[ -x "$RUST_BIN" ] || { echo "FAIL: rust auth binary not executable: $RUST_BIN"; exit 1; }
[ -f "$RUST_CFG" ] || { echo "FAIL: config not found: $RUST_CFG"; exit 1; }

echo "== (a) stop C++ auth only =="
pkill -f 'srv1-auth1'
for i in $(seq 1 10); do
  if ! ss -tln 2>/dev/null | grep -q ":${AUTH_PORT}"; then break; fi
  sleep 1
done
if ss -tln 2>/dev/null | grep -q ":${AUTH_PORT}"; then
  echo "FAIL: ${AUTH_PORT} still in use after killing srv1-auth1"; exit 1
fi
echo "OK: ${AUTH_PORT} free"

echo "== (b) core1 still alive =="
ss -tln 2>/dev/null | grep -q ':30003' && echo "OK: core1 30003 still listening"
pgrep -af 'srv1-ch1-core1' >/dev/null && echo "OK: core1 process alive"

echo "== (c) start Rust auth: $RUST_BIN --config $RUST_CFG =="
cd "$SV" && setsid nohup "$RUST_BIN" --config "$RUST_CFG" > "$LOG" 2>&1 &
echo $! > "$PIDFILE"
echo "pid $(cat "$PIDFILE") -> log $LOG"

echo "== (d) wait for ${AUTH_IP}:${AUTH_PORT} =="
ok=0
for i in $(seq 1 "$WAIT"); do
  if ss -tln 2>/dev/null | grep -q ":${AUTH_PORT}"; then ok=1; break; fi
  if ! kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
    echo "FAIL: rust auth process exited early"; tail -20 "$LOG"; exit 1
  fi
  sleep 1
done
if [ "$ok" != 1 ]; then
  echo "FAIL: ${AUTH_PORT} not listening within ${WAIT}s"; tail -20 "$LOG"; exit 1
fi
echo "OK: ${AUTH_IP}:${AUTH_PORT} listening (rust auth)"
ss -tln | grep ":${AUTH_PORT}"
echo "log tail:"; tail -5 "$LOG"

echo "== (e) mechanical peer check (run FROM WINDOWS, source/reforge) =="
cat <<'EOF'
    cd C:\projects\Metin2\source\reforge
    cargo run --example f16_peer -- 172.25.104.175 30001 --login3
    # Expected: GC_AUTH_SUCCESS (login test/1234 against the Rust auth).
    # Then the real end-to-end: client login test/1234 (AGENTS.md runbook).
EOF

echo "== restore when done: bash hybrid_auth_test.sh --restore =="
sync
echo "== HYBRID SWAP OK (exit 0) =="
exit 0
