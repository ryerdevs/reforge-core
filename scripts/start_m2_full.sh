#!/bin/bash
# Full Metin2 stack start (db + auth + 9 channel cores).
# WARNING: on this 4GB RAM host this overcommits WSL memory (crashes).
# Prefer start_m2_min.sh for login testing.
# Usage: wsl -d Debian-M2 -- bash /mnt/c/projects/Metin2/scripts/start_m2_full.sh
SV=/home/m2/source/metin2_svfiles/main/srv1
service mariadb start
sleep 4
cd "$SV/db" && setsid nohup ./srv1-db > stdout 2>&1 &
sleep 5
cd "$SV/auth1" && setsid nohup ./srv1-auth1 > stdout 2>&1 &
sleep 5
for d in "$SV"/chan/ch*/core*; do
  [ -d "$d" ] || continue
  ch=$(basename "$(dirname "$d")")
  co=$(basename "$d")
  name="srv1-$ch-$co"
  (cd "$d" && setsid nohup "./$name" > stdout 2>&1 &)
  echo "launched $name"
done
sleep 15
ps aux | grep 'srv1-' | grep -v grep | awk '{print $2, $11}'
ss -tln 2>/dev/null | grep -oE ':[0-9]{5}' | sort -u
echo "=== DONE ==="
