#!/bin/bash
# Clean restart of the stack on PG: kill old (MariaDB) processes first, then start fresh.
export LANG=C
SV=/home/m2/source/metin2_svfiles/main/srv1
echo "=== killing old stack (any srv1-*) ==="
pkill -f 'srv1-db' ; pkill -f 'srv1-auth1' ; pkill -f 'srv1-ch1-core1'
sleep 4
echo "remaining: $(pgrep -fl 'srv1-' | wc -l)"
echo "=== confs activos (deben ser :3307) ==="
grep -E 'SQL_ACCOUNT' "$SV/db/conf.txt" | head -1
grep PLAYER_SQL "$SV/auth1/CONFIG"
echo "=== proxy vivo ==="
pgrep -fl mysql_proxy | head -1
echo "=== start fresh ==="
cd "$SV/db" && setsid nohup ./srv1-db > stdout 2>&1 &
for i in $(seq 1 18); do
  if grep -q 'BANWORD: total' "$SV/db/syslog" 2>/dev/null && ss -tln 2>/dev/null | grep -q ':30000'; then
    echo "db boot complete after ~$((i*5))s"; break
  fi
  sleep 5
done
cd "$SV/auth1" && setsid nohup ./srv1-auth1 > stdout 2>&1 &
sleep 6
cd "$SV/chan/ch1/core1" && setsid nohup ./srv1-ch1-core1 > stdout 2>&1 &
sleep 12
echo "=== procesos ==="
pgrep -fl 'srv1-' | head -6
echo "=== conexiones ESTABLECIDAS hacia 3307 (proxy) y 3306 (mariadb) ==="
ss -tnp 2>/dev/null | grep -E ':(3306|3307)' | head -12
echo "=== puertos ==="
ss -tln 2>/dev/null | grep -oE ':(3000[0-9])' | sort -u
echo "=== DONE ==="
