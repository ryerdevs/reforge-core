#!/bin/bash
# CONTROL TEST: boot WSL, start ONLY mariadb, watch memory for 120s.
# If the VM survives this, the trigger is the Metin2 processes.
# Usage: wsl -d Debian-M2 -- bash /mnt/c/projects/Metin2/scripts/watch_control.sh
echo "=== control: mariadb only, no srv1 processes ==="
service mariadb start
sleep 3
mysqladmin ping 2>&1 | head -1
for i in $(seq 1 12); do
  echo "--- t+$((i*10))s ---"
  free -m | awk 'NR<=2'
  sleep 10
done
echo "=== FINAL (survived 120s with mariadb only) ==="
ps aux | grep -cE 'maria|srv1' | head -1
