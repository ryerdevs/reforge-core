#!/bin/bash
# A/B test: switch the stack to MariaDB (no proxy, no PG) for one client entry test.
# Usage: bash swap_mariadb.sh ; then the user tries ONE world entry.
# Return to PG later: bash start_pg_stack.sh
export LANG=C
SV=/home/m2/source/metin2_svfiles/main/srv1
echo "=== stopping PG stack (db/auth/core) ==="
pkill -f 'srv1-db' ; pkill -f 'srv1-auth1' ; pkill -f 'srv1-ch1-core1'
sleep 3
echo "=== activating MariaDB confs ==="
cp "$SV/db/conf.txt_mariadb" "$SV/db/conf.txt"
cp "$SV/auth1/CONFIG_mariadb" "$SV/auth1/CONFIG"
cp "$SV/chan/ch1/core1/CONFIG_mariadb" "$SV/chan/ch1/core1/CONFIG"
grep -E 'SQL_ACCOUNT' "$SV/db/conf.txt" | head -1
grep PLAYER_SQL "$SV/auth1/CONFIG"
echo "=== starting stack on MariaDB ==="
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
echo "=== state ==="
ss -tln 2>/dev/null | grep -oE ':(3000[0-9])' | sort -u
pgrep -fl 'srv1-' | head -5
echo "=== STACK EN MARIADB (listo para 1 entrada de prueba) ==="
