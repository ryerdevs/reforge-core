#!/bin/bash
# G-PG part 2/3: capture live SHOW CREATE TABLE for the 17 login/boot tables (read-only)
M="mariadb -h127.0.0.1 -umt2 -pmt2"
OUT=/tmp/gpg/show_create_all.txt
: > "$OUT"
while IFS= read -r t; do
  db="${t%%.*}"
  tbl="${t##*.}"
  echo "===== $t =====" >> "$OUT"
  $M -e "SHOW CREATE TABLE \`$db\`.\`$tbl\`\G" >> "$OUT" 2>&1
done <<'EOF'
account.account
player.player
player.player_index
player.mob_proto
player.item_proto
player.shop
player.shop_item
player.skill_proto
player.refine_proto
player.item_attr
player.item_attr_rare
player.banword
player.land
player.object_proto
player.object
player.monarch
common.locale
common.priv_settings
EOF
wc -l "$OUT"
echo "DONE"
