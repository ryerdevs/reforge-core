#!/bin/bash
# G-PG part 2/3: verification — counts MariaDB vs PG, login SQL on PG, account/characters, versions
export PGPASSWORD=mt2
PG="psql -h 127.0.0.1 -U mt2 -d metin2 -tA"
M="mariadb -h127.0.0.1 -umt2 -pmt2 -N"

echo "== versions =="
psql --version
mariadb --version
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SHOW server_version"

echo
echo "== row counts: MariaDB vs PG (per migrated table) =="
printf "%-28s %10s %10s %s\n" TABLE MARIADB PG DIFF
for t in account.account player.player player.player_index player.mob_proto player.item_proto \
         player.shop player.shop_item player.skill_proto player.refine_proto player.item_attr \
         player.item_attr_rare player.banword player.land player.object_proto player.object \
         player.monarch common.locale common.priv_settings; do
  db="${t%%.*}"; tbl="${t##*.}"
  my=$($M -e "SELECT COUNT(*) FROM \`$db\`.\`$tbl\`")
  pg=$($PG -c "SELECT count(*) FROM ${db}.${tbl}")
  if [ "$my" = "$pg" ]; then d=OK; else d="MISMATCH"; fi
  printf "%-28s %10s %10s %s\n" "$t" "$my" "$pg" "$d"
done

echo
echo "== login SQL reference test on PG =="
echo "--- plain: SELECT * FROM account.account WHERE login='test' AND password=hash ---"
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -x -c "SELECT id, login, password, social_id, status, lang FROM account.account WHERE login='test' AND password='*A4B6157319038724E3560894F7F932C8886EBFCF'"
echo "--- QUERY_LOGIN shape (13-col join, per ClientManagerLogin.cpp:411-415) ---"
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -x -c "SELECT a.id, a.login, a.password, a.social_id, pi.empire, pi.pid1, pi.pid2, pi.pid3, pi.pid4, pi.pid5, a.status, a.lang FROM account.account a LEFT JOIN player.player_index pi ON pi.id = a.id WHERE a.login='test' AND a.password='*A4B6157319038724E3560894F7F932C8886EBFCF'"

echo
echo "== characters in PG (coordinates in UNITS) =="
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -x -c "SELECT id, account_id, name, job, level, x, y, map_index FROM player.player ORDER BY id"
echo "--- player_index (slots) ---"
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -x -c "SELECT * FROM player.player_index"

echo
echo "== bytea spot checks: item_proto name/locale_name preserved as raw bytes =="
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SELECT encode(name,'hex') FROM player.item_proto WHERE vnum=1"
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SELECT encode(name,'hex') FROM player.item_proto WHERE vnum=2"
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SELECT name FROM player.mob_proto WHERE vnum=101"
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SELECT encode(locale_name,'hex') FROM player.mob_proto WHERE vnum=101"

echo
echo "== identities seeded =="
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SELECT 'account.id next', last_value+1 FROM account.account_id_seq"
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SELECT 'player.id next', last_value+1 FROM player.player_id_seq"
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SELECT 'refine_proto.id next', last_value+1 FROM player.refine_proto_id_seq"
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SELECT 'land.id next', last_value+1 FROM player.land_id_seq"
