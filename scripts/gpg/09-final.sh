#!/bin/bash
# G-PG part 2/3: final checks — MariaDB intact, PG listening, cluster restart cycle, locale content
export PGPASSWORD=mt2
echo "== MariaDB intact: databases =="
mariadb -h127.0.0.1 -umt2 -pmt2 -N -e "SHOW DATABASES;"
echo "== MariaDB key counts (unchanged) =="
mariadb -h127.0.0.1 -umt2 -pmt2 -N -e "SELECT COUNT(*) FROM account.account; SELECT COUNT(*) FROM player.player; SELECT COUNT(*) FROM player.mob_proto; SELECT COUNT(*) FROM player.item_proto;"
echo "== PG listening =="
ss -tlnp | grep 5432
echo "== PG processes =="
pgrep -af postgres | head -8
echo "== common.locale content =="
psql -h 127.0.0.1 -U mt2 -d metin2 -c "SELECT * FROM common.locale ORDER BY \"mKey\";"
echo "== cluster restart cycle (stop/start) =="
pg_ctlcluster 18 main stop
pg_lsclusters
pg_ctlcluster 18 main start
pg_lsclusters
psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SELECT 'reconnect OK: ' || current_database()"
sync
echo "== ALL DONE =="
