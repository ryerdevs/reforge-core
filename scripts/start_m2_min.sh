#!/bin/bash
# Minimal Metin2 stack start (mariadb + db + auth1 + ch1-core1).
# Use for login testing: the full 9-core stack OOMs this 4GB RAM host.
# Usage: wsl -d Debian-M2 -- bash /mnt/c/projects/Metin2/scripts/start_m2_min.sh
SV=/home/m2/source/metin2_svfiles/main/srv1
echo "=== mariadb ==="
service mariadb start
sleep 4
mysqladmin ping 2>&1 | head -1
echo "=== srv1-db ==="
cd "$SV/db" && setsid nohup ./srv1-db > stdout 2>&1 &
sleep 5
echo "=== srv1-auth1 ==="
cd "$SV/auth1" && setsid nohup ./srv1-auth1 > stdout 2>&1 &
sleep 5
echo "=== srv1-ch1-core1 ==="
cd "$SV/chan/ch1/core1" && setsid nohup ./srv1-ch1-core1 > stdout 2>&1 &
sleep 15
echo "=== processes ==="
ps aux | grep 'srv1-' | grep -v grep | awk '{print $2, $11}'
echo "=== ports (expect 30000,30001,30002,30003,30004) ==="
ss -tln 2>/dev/null | grep -oE ':[0-9]{5}' | sort -u
echo "=== memory ==="
free -m | head -3
echo "=== auth1 syslog tail ==="
tail -5 "$SV/auth1/syslog"
echo "=== auth1 syserr tail ==="
tail -3 "$SV/auth1/syserr"
echo "=== DONE (test login NOW: client -> 172.25.104.175:30001) ==="
