#!/bin/bash
# STACK TEST: boot mariadb + db + auth1 + ch1-core1, watch memory every 5s.
# If memory climbs monotonically -> leak in the game binary (reverted code).
# Usage: wsl -d Debian-M2 -- bash /mnt/c/projects/Metin2/scripts/watch_stack.sh
SV=/home/m2/source/metin2_svfiles/main/srv1
service mariadb start
sleep 3
cd "$SV/db" && setsid nohup ./srv1-db > stdout 2>&1 &
sleep 3
cd "$SV/auth1" && setsid nohup ./srv1-auth1 > stdout 2>&1 &
sleep 3
cd "$SV/chan/ch1/core1" && setsid nohup ./srv1-ch1-core1 > stdout 2>&1 &
for i in $(seq 1 20); do
  echo "--- t+$((i*5))s ---"
  free -m | awk 'NR<=2 {print}'
  ps aux | grep 'srv1-' | grep -v grep | awk '{printf "  %s %sMB rss %s%% cpu\n", $11, int($6/1024), $3}'
  sleep 5
done
echo "=== FINAL: survived 100s with stack ==="
ss -tln 2>/dev/null | grep -oE ':[0-9]{5}' | sort -u
