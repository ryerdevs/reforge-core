#!/bin/bash
# DB-ONLY TEST: mariadb + srv1-db, NO auth, NO cores. Watch RSS for 60s.
# If srv1-db balloons alone -> db load loop (data/state issue).
# If srv1-db stays small -> the db<->auth interaction is the trigger.
SV=/home/m2/source/metin2_svfiles/main/srv1
service mariadb start
sleep 3
cd "$SV/db" && setsid nohup ./srv1-db > stdout 2>&1 &
for i in $(seq 1 12); do
  echo "--- t+$((i*5))s ---"
  free -m | awk 'NR<=2 {print}'
  ps aux | grep 'srv1-' | grep -v grep | awk '{printf "  %s %sMB rss %s%% cpu\n", $11, int($6/1024), $3}'
  sleep 5
done
echo "=== FINAL: db survived 60s alone ==="
