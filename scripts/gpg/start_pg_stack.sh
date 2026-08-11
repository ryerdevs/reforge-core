#!/bin/bash
# Start the C++ stack on PostgreSQL (via mysql_proxy) and leave it running.
# Activates the *_pg conf variants; requires mariadb, PG 18 and mysql_proxy already up.
export LANG=C
SV=/home/m2/source/metin2_svfiles/main/srv1
echo "=== activating PG confs ==="
cp "$SV/db/conf.txt_pg" "$SV/db/conf.txt"
cp "$SV/auth1/CONFIG_pg" "$SV/auth1/CONFIG"
cp "$SV/chan/ch1/core1/CONFIG_pg" "$SV/chan/ch1/core1/CONFIG"
grep -E 'SQL_ACCOUNT|SQL_PLAYER' "$SV/db/conf.txt" | head -2
grep PLAYER_SQL "$SV/auth1/CONFIG"
echo "=== preconditions ==="
pgrep -fl mysql_proxy | head -1
ss -tln 2>/dev/null | grep -E ':(3306|5432)' | head -2
echo "=== starting srv1-db ==="
cd "$SV/db" && setsid nohup ./srv1-db > stdout 2>&1 &
for i in $(seq 1 18); do
  if grep -q 'BANWORD: total' "$SV/db/syslog" 2>/dev/null && ss -tln 2>/dev/null | grep -q ':30000'; then
    echo "db boot complete (BANWORD + port 30000) after ~$((i*5))s"; break
  fi
  sleep 5
done
tail -3 "$SV/db/stdout"
echo "=== starting srv1-auth1 ==="
cd "$SV/auth1" && setsid nohup ./srv1-auth1 > stdout 2>&1 &
sleep 6
echo "=== starting srv1-ch1-core1 ==="
cd "$SV/chan/ch1/core1" && setsid nohup ./srv1-ch1-core1 > stdout 2>&1 &
sleep 12
echo "=== final state ==="
ss -tln 2>/dev/null | grep -oE ':(3000[0-9])' | sort -u
pgrep -fl 'srv1-' | head -8
echo "=== auth1 syslog tail ==="
tail -3 "$SV/auth1/syslog"
echo "=== core1 syslog tail ==="
tail -3 "$SV/chan/ch1/core1/syslog"
echo "=== eth0 ==="
ip addr show eth0 2>/dev/null | grep 'inet '
echo "=== STACK PG UP (leave running) ==="
