#!/bin/bash
# G-PG part 2/3: apply DDL, dump 17 tables from MariaDB (read-only), translate, import to PG
set -e
export PGPASSWORD=mt2
PG="psql -h 127.0.0.1 -U mt2 -d metin2 -v ON_ERROR_STOP=1 -q"
DUMP="mysqldump -h127.0.0.1 -umt2 -pmt2 --no-create-info --skip-extended-insert --hex-blob --complete-insert --skip-add-locks --skip-lock-tables --single-transaction --skip-triggers --skip-comments --no-tablespaces --default-character-set=utf8mb4"
TR="/mnt/c/Users/RICARD~1/AppData/Local/Temp/opencode/gpg/06-translate.py"

echo "== apply DDL =="
$PG -f /tmp/gpg/05-ddl.sql
echo "DDL applied."

echo "== dump/translate/import =="
declare -A TABLES=(
  [account.account]=account
  [player.player]=player
  [player.player_index]=player
  [player.mob_proto]=player
  [player.item_proto]=player
  [player.shop]=player
  [player.shop_item]=player
  [player.skill_proto]=player
  [player.refine_proto]=player
  [player.item_attr]=player
  [player.item_attr_rare]=player
  [player.banword]=player
  [player.land]=player
  [player.object_proto]=player
  [player.object]=player
  [player.monarch]=player
  [common.locale]=common
  [common.priv_settings]=common
)
for t in "${!TABLES[@]}"; do
  db="${t%%.*}"
  tbl="${t##*.}"
  echo "--- $t ---"
  $DUMP "$db" "$tbl" | python3 "$TR" > /tmp/gpg/data_${db}_${tbl}.sql
  rows_my=$(grep -c "^INSERT" /tmp/gpg/data_${db}_${tbl}.sql || true)
  echo "dumped $rows_my INSERT statements"
  $PG -f /tmp/gpg/data_${db}_${tbl}.sql
  cnt=$($PG -tAc "SELECT count(*) FROM ${db}.${tbl}")
  echo "imported into ${db}.${tbl}: $cnt rows"
done
sync
echo "== DONE =="
