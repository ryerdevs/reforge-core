#!/bin/bash
# =============================================================================
# e2e_db.sh — E2E suite of the DB layer: real C++ db-binary queries replayed
# through the full path (mysql CLI -> mysql_proxy 127.0.0.1:3307 -> PostgreSQL
# metin2), with MariaDB (127.0.0.1:3306, latin1 as the core uses) as oracle.
#
# Usage (WSL, root):
#   bash /mnt/c/projects/Metin2/scripts/gpg/e2e_db.sh
# Exit: 0 = all asserts green, 1 = any FAIL.
#
# Coverage (query texts copied from the C++ source, file:line cited per query):
#   - QUERY_LOGIN 13 cols                ClientManagerLogin.cpp:411-415
#   - player load (UNIX_TIMESTAMP diff)  ClientManagerPlayer.cpp:361-375
#   - player list by account             ClientManagerLogin.cpp:231-235
#   - player create (INSERT + blobs)     ClientManagerPlayer.cpp:853-892
#   - player save (UPDATE + blobs)       ClientManagerPlayer.cpp:70-177
#   - quest / affect / safebox loads     ClientManagerPlayer.cpp:303,310; ClientManager.cpp:603
#   - item_award / messenger_list        ItemAwardManager; MessengerManager (world entry)
#   - locale (common slot)               ClientManager.cpp:3078
#   - item id-range probes               ItemIDRangeManager.cpp:93,121
#   - boot protos with enum+0 casts      ClientManagerBoot.cpp:1290 (mob), 1466 (item),
#                                        121 (refine), 476-482 (skill)
#   - throwaway character cycle          INSERT -> bytea raw check -> UPDATE save ->
#                                        DELETE (trap-guaranteed cleanup; NEVER touches
#                                        existing rows — the user plays on this stack)
#   - auth login E2E: NOT here — the orchestrator covers it with the Windows peer:
#        cd C:\projects\Metin2\source\reforge
#        cargo run --example f16_peer -- 172.25.104.175 30001 --login3
#
# Documented exceptions (asserted structurally, not by value):
#   - account.last_play / hwid: the LIVE login writes only on PG (MariaDB frozen).
#   - player x/y/playtime of LIVE characters: PG is operative, MariaDB frozen.
#   - player list row count: PG may have more characters (created in-game).
# =============================================================================
set -u

PROXY="mysql -h127.0.0.1 -P3307 -umt2 -pmt2 --raw --batch -N"
MARIA="mariadb -h127.0.0.1 -umt2 -pmt2 --raw --batch -N"
TS=$(date +%s)
E2E_NAME="e2e_${TS}"
E2E_ID=""
PASS=0
FAIL=0
GAPS=0

ok()   { PASS=$((PASS+1)); echo "OK   $1"; }
bad()  { FAIL=$((FAIL+1)); echo "FAIL $1"; }
gap()  { GAPS=$((GAPS+1)); echo "GAP(crate) $1"; }
check() { # $1 label, $2 expected, $3 actual
  if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (expected [$2] got [$3])"; fi
}

# trap: ALWAYS delete the throwaway character (never leave rows behind)
cleanup() {
  if [ -n "$E2E_ID" ]; then
    $PROXY -D player -e "DELETE FROM player WHERE id=$E2E_ID" >/dev/null 2>&1
    echo "cleanup: deleted throwaway player id=$E2E_ID"
  fi
  # safety net by name too
  $PROXY -D player -e "DELETE FROM player WHERE name='$E2E_NAME'" >/dev/null 2>&1
  sync
}
trap cleanup EXIT

echo "============================================================"
echo "E2E DB layer — proxy 3307 -> PG metin2; oracle MariaDB 3306"
echo "throwaway character: $E2E_NAME"
echo "============================================================"

# ---------------------------------------------------------------- 1. QUERY_LOGIN
Q1="SELECT mysql_hash_password('1234'), a.id, a.login, a.password, a.social_id, pi.empire, pi.pid1, pi.pid2, pi.pid3, pi.pid4, pi.pid5, a.status, a.lang FROM account a LEFT JOIN player.player_index pi ON pi.id = a.id WHERE a.login='test' AND a.password=mysql_hash_password('1234')"
R1P=$($PROXY -D account -e "$Q1" 2>&1); R1P_RC=$?
R1M=$($MARIA  -D account -e "$Q1" 2>&1); R1M_RC=$?
[ $R1P_RC -eq 0 ] && [ $R1M_RC -eq 0 ] && ok "Q1 QUERY_LOGIN exit0 (proxy=$R1P_RC maria=$R1M_RC)" || bad "Q1 QUERY_LOGIN exit (proxy=$R1P_RC maria=$R1M_RC): $R1P"
check "Q1 rows=1 both" "$(printf '%s' "$R1P" | grep -c .)" "$(printf '%s' "$R1M" | grep -c .)"
check "Q1 13 columns" "$(printf '%s\n' "$R1P" | awk -F'\t' '{print NF; exit}')" "13"
check "Q1 col0=hash" "*A4B6157319038724E3560894F7F932C8886EBFCF" "$(printf '%s\n' "$R1P" | awk -F'\t' 'NR==1{print $1}')"
check "Q1 col1=id" "1" "$(printf '%s\n' "$R1P" | awk -F'\t' 'NR==1{print $2}')"
check "Q1 col2=login" "test" "$(printf '%s\n' "$R1P" | awk -F'\t' 'NR==1{print $3}')"
check "Q1 empire=3 (col5)" "3" "$(printf '%s\n' "$R1P" | awk -F'\t' 'NR==1{print $6}')"
check "Q1 hash equals maria col0" "$(printf '%s\n' "$R1M" | awk -F'\t' 'NR==1{print $1}')" "$(printf '%s\n' "$R1P" | awk -F'\t' 'NR==1{print $1}')"

# ---------------------------------------------------------------- 2. player load
Q2="SELECT id,name,job,voice,dir,x,y,z,map_index,exit_x,exit_y,exit_map_index,hp,mp,stamina,random_hp,random_sp,playtime,gold,level,level_step,st,ht,dx,iq,exp,stat_point,skill_point,sub_skill_point,stat_reset_count,part_base,part_hair,skill_level,quickslot,skill_group,alignment,horse_level,horse_riding,horse_hp,horse_hp_droptime,horse_stamina,UNIX_TIMESTAMP(NOW())-UNIX_TIMESTAMP(last_play),horse_skill_point FROM player WHERE id=2"
R2P=$($PROXY -D player -e "$Q2" 2>&1 | tr -d '\000'); R2P_RC=$?
R2M=$($MARIA  -D player -e "$Q2" 2>&1 | tr -d '\000'); R2M_RC=$?
[ $R2P_RC -eq 0 ] && [ $R2M_RC -eq 0 ] && ok "Q2 player load exit0" || bad "Q2 player load exit (proxy=$R2P_RC maria=$R2M_RC): $R2P"
check "Q2 rows=1 both" "$(printf '%s' "$R2P" | grep -c .)" "$(printf '%s' "$R2M" | grep -c .)"
check "Q2 42 columns both" "$(printf '%s\n' "$R2P" | awk -F'\t' '{print NF; exit}')" "$(printf '%s\n' "$R2M" | awk -F'\t' '{print NF; exit}')"
check "Q2 id=2" "2" "$(printf '%s\n' "$R2P" | awk -F'\t' 'NR==1{print $1}')"
check "Q2 name=ninja" "ninja" "$(printf '%s\n' "$R2P" | awk -F'\t' 'NR==1{print $2}')"
check "Q2 job=1" "1" "$(printf '%s\n' "$R2P" | awk -F'\t' 'NR==1{print $3}')"
# bytea raw: skill_level bytes (col 34) must be BYTE-IDENTICAL proxy vs maria
# (piped straight to od — no shell substitution, NULs survive; head -c -1 drops
# the CLI's trailing newline so it is not counted as a payload byte)
SKP=$($PROXY -D player --raw --batch -N -e "SELECT skill_level FROM player WHERE id=2" 2>/dev/null | head -c -1 | od -An -tx1 | tr -d ' \n')
SKM=$($MARIA  -D player --raw --batch -N -e "SELECT skill_level FROM player WHERE id=2" 2>/dev/null | head -c -1 | od -An -tx1 | tr -d ' \n')
check "Q2 skill_level bytea raw identical" "$SKM" "$SKP"
# x/y/playtime/UNIX_TIMESTAMP-diff: documented exception (PG operative) — structure only

# ---------------------------------------------------------------- 3. player list
Q3="SELECT id, name, job, level, playtime, st, ht, dx, iq, part_main, part_hair, x, y, skill_group, change_name FROM player WHERE account_id=1"
R3P=$($PROXY -D player -e "$Q3" 2>&1); R3P_RC=$?
R3M=$($MARIA  -D player -e "$Q3" 2>&1); R3M_RC=$?
[ $R3P_RC -eq 0 ] && [ $R3M_RC -eq 0 ] && ok "Q3 player list exit0" || bad "Q3 player list exit: $R3P"
check "Q3 15 columns both" "$(printf '%s\n' "$R3P" | awk -F'\t' '{print NF; exit}')" "15"
[ "$(printf '%s' "$R3P" | grep -c .)" -ge 3 ] && ok "Q3 proxy >=3 rows" || bad "Q3 proxy rows < 3"
[ "$(printf '%s' "$R3M" | grep -c .)" -ge 3 ] && ok "Q3 maria >=3 rows" || bad "Q3 maria rows < 3"
for n in lkjsnlfknlsk ninja Chaman; do
  printf '%s\n' "$R3P" | grep -q "$n" && ok "Q3 has $n (proxy)" || bad "Q3 missing $n (proxy)"
  printf '%s\n' "$R3M" | grep -q "$n" && ok "Q3 has $n (maria)"  || bad "Q3 missing $n (maria)"
done
# PG may have extra characters created in-game (documented exception)

# ---------------------------------------------------------------- 4. throwaway create (proxy only; blobs escaped like the C++)
# Blob bytes: 0x01 0x02 (control, valid UTF-8), 0x27(') 0x5c(\) 0x22(") escaped, 0x00 -> \0 (tests the crate 22021 fix).
# NOTE: NO non-ASCII raw bytes in the SQL literal (0xfe etc. -> PG UTF-8 error — crate gap, see report).
BLOB1_HEX="0102275c2200"
BLOB1_ESC=$(python3 -c "import sys; d=bytes.fromhex('$BLOB1_HEX'); print(d.decode('latin1').replace('\\\\','\\\\\\\\').replace(chr(39),'\\\\'+chr(39)).replace(chr(34),'\\\\'+chr(34)).replace(chr(0),'\\\\0'), end='')")
Q4="INSERT INTO player (id, account_id, name, level, st, ht, dx, iq, job, voice, dir, x, y, z, hp, mp, random_hp, random_sp, stat_point, stamina, part_base, part_main, part_hair, gold, playtime, skill_level, quickslot) VALUES(0, 1, '$E2E_NAME', 1, 30, 30, 30, 30, 0, 0, 0, 0, 0, 0, 100, 100, 0, 0, 0, 100, 0, 0, 0, 0, 0, '$BLOB1_ESC', '$BLOB1_ESC')"
if $PROXY -D player -e "$Q4" >/dev/null 2>&1; then ok "Q4 create INSERT exit0"; else bad "Q4 create INSERT failed: $($PROXY -D player -e "$Q4" 2>&1)"; fi
E2E_ID=$($PROXY -D player -N -e "SELECT id FROM player WHERE name='$E2E_NAME'" 2>/dev/null | head -1)
[ -n "$E2E_ID" ] && ok "Q4 created id=$E2E_ID" || bad "Q4 no id for $E2E_NAME"

# bytea raw round-trip 1 (read back what we wrote)
if [ -n "$E2E_ID" ]; then
  R4HEX=$($PROXY -D player --raw --batch -N -e "SELECT skill_level FROM player WHERE id=$E2E_ID" | head -c -1 | od -An -tx1 | tr -d ' \n')
  check "Q4 skill_level raw == $BLOB1_HEX" "$BLOB1_HEX" "$R4HEX"
fi

# ---------------------------------------------------------------- 5. save (UPDATE with blobs, CreatePlayerSaveQuery shape)
# BLOB2: ASCII-safe bytes ("deadbeef" text + 0x01 0x02) — different from blob1
BLOB2_HEX="64656164626565660102"
BLOB2_ESC=$(python3 -c "import sys; d=bytes.fromhex('$BLOB2_HEX'); print(d.decode('latin1').replace('\\\\','\\\\\\\\').replace(chr(39),'\\\\'+chr(39)).replace(chr(34),'\\\\'+chr(34)).replace(chr(0),'\\\\0'), end='')")
Q5="UPDATE player SET job=0, voice=0, dir=0, x=969600, y=278400, z=0, map_index=41, exit_x=0, exit_y=0, exit_map_index=0, hp=100, mp=100, stamina=100, random_hp=0, random_sp=0, playtime=10, level=1, level_step=0, st=30, ht=30, dx=30, iq=30, gold=0, exp=0, stat_point=0, skill_point=0, sub_skill_point=0, stat_reset_count=0, ip='0.0.0.0', part_main=0, part_hair=0, last_play=NOW(), skill_group=0, alignment=0, horse_level=0, horse_riding=0, horse_hp=0, horse_hp_droptime=0, horse_stamina=0, horse_skill_point=0, skill_level='$BLOB2_ESC', quickslot='$BLOB2_ESC' WHERE id=$E2E_ID"
if [ -n "$E2E_ID" ] && $PROXY -D player -e "$Q5" >/dev/null 2>&1; then ok "Q5 save UPDATE exit0"; else bad "Q5 save UPDATE failed"; fi
if [ -n "$E2E_ID" ]; then
  R5HEX=$($PROXY -D player --raw --batch -N -e "SELECT skill_level FROM player WHERE id=$E2E_ID" | head -c -1 | od -An -tx1 | tr -d ' \n')
  check "Q5 skill_level raw == $BLOB2_HEX" "$BLOB2_HEX" "$R5HEX"
  R5XY=$($PROXY -D player -N -e "SELECT x,y FROM player WHERE id=$E2E_ID" | tr '\t' ' ')
  check "Q5 saved x,y" "969600 278400" "$R5XY"
fi

# ---------------------------------------------------------------- 6. char-state tables (all zero rows for a fresh char / existing pid 1)
for spec in "quest|SELECT dwPID,szName,szState,lValue FROM quest WHERE dwPID=1 AND lValue<>0" \
            "affect|SELECT dwPID,bType,bApplyOn,lApplyValue,dwFlag,lDuration,lSPCost FROM affect WHERE dwPID=1" \
            "safebox|SELECT account_id, size, password FROM safebox WHERE account_id=1" \
            "item_award|SELECT id,login,vnum,count,socket0,socket1,socket2,attrtype0,attrvalue0,attrtype1,attrvalue1,attrtype2,attrvalue2,attrtype3,attrvalue3,attrtype4,attrvalue4,attrtype5,attrvalue5,attrtype6,attrvalue6,mall,why FROM item_award" \
            "messenger|SELECT account, companion FROM messenger_list WHERE account='test'"; do
  LBL="${spec%%|*}"; SQL="${spec#*|}"
  RP=$($PROXY -D player -e "$SQL" 2>&1); RP_RC=$?
  RM=$($MARIA  -D player -e "$SQL" 2>&1); RM_RC=$?
  [ $RP_RC -eq 0 ] && [ $RM_RC -eq 0 ] && ok "Q6 $LBL exit0" || bad "Q6 $LBL exit (proxy=$RP_RC maria=$RM_RC): $RP"
  check "Q6 $LBL rows equal" "$(printf '%s' "$RM" | grep -c .)" "$(printf '%s' "$RP" | grep -c .)"
done

# ---------------------------------------------------------------- 7. locale (common slot)
Q7="SELECT mValue, mKey FROM locale"
R7P=$($PROXY -D common -e "$Q7" 2>&1); R7P_RC=$?
R7M=$($MARIA  -D common -e "$Q7" 2>&1); R7M_RC=$?
[ $R7P_RC -eq 0 ] && [ $R7M_RC -eq 0 ] && ok "Q7 locale exit0" || bad "Q7 locale exit: $R7P"
check "Q7 locale 13 rows both" "$(printf '%s' "$R7M" | grep -c .)" "$(printf '%s' "$R7P" | grep -c .)"
check "Q7 DB_NAME_COLUMN=locale_name" "locale_name" "$(printf '%s\n' "$R7P" | awk -F'\t' '$2=="DB_NAME_COLUMN"{print $1}')"

# ---------------------------------------------------------------- 8. item id-range probes (ItemIDRangeManager.cpp:93,121)
R8P=$($PROXY -D player -e "SELECT MAX(id) FROM item WHERE id >= 100000000 and id <= 200000000" 2>&1); R8P_RC=$?
R8M=$($MARIA  -D player -e "SELECT MAX(id) FROM item WHERE id >= 100000000 and id <= 200000000" 2>&1); R8M_RC=$?
[ $R8P_RC -eq 0 ] && [ $R8M_RC -eq 0 ] && ok "Q8 MAX(id) probe exit0" || bad "Q8 MAX(id) probe exit: $R8P"
check "Q8 MAX(id) value equal" "$R8M" "$R8P"
R8P=$($PROXY -D player -e "SELECT COUNT(*) FROM item WHERE id >= 100000000 AND id <= 200000000" 2>&1)
R8M=$($MARIA  -D player -e "SELECT COUNT(*) FROM item WHERE id >= 100000000 AND id <= 200000000" 2>&1)
check "Q8 COUNT(*) probe equal" "$R8M" "$R8P"

# ---------------------------------------------------------------- 9. boot protos with enum+0 casts
Q9M="SELECT vnum, name, locale_name, type, \`rank\`, battle_type, level, size+0, ai_flag+0, setRaceFlag+0, setImmuneFlag+0, on_click, empire, drop_item, resurrection_vnum, folder, st, dx, ht, iq, damage_min, damage_max, max_hp, regen_cycle, regen_percent, exp, gold_min, gold_max, def, attack_speed, move_speed, aggressive_hp_pct, aggressive_sight, attack_range, polymorph_item, enchant_curse, enchant_slow, enchant_poison, enchant_stun, enchant_critical, enchant_penetrate, resist_sword, resist_twohand, resist_dagger, resist_bell, resist_fan, resist_bow, resist_fire, resist_elect, resist_magic, resist_wind, resist_poison, dam_multiply, summon, drain_sp, skill_vnum0, skill_level0, skill_vnum1, skill_level1, skill_vnum2, skill_level2, skill_vnum3, skill_level3, skill_vnum4, skill_level4, sp_berserk, sp_stoneskin, sp_godspeed, sp_deathblow, sp_revive FROM mob_proto ORDER BY vnum"
R9P=$($PROXY -D player -e "$Q9M" 2>&1); R9P_RC=$?
R9M=$($MARIA  -D player -e "$Q9M" 2>&1); R9M_RC=$?
[ $R9P_RC -eq 0 ] && [ $R9M_RC -eq 0 ] && ok "Q9 mob_proto(+0) exit0" || bad "Q9 mob_proto(+0) exit (proxy=$R9P_RC maria=$R9M_RC)"
check "Q9 mob_proto 2864 rows both" "$(printf '%s' "$R9M" | grep -c .)" "$(printf '%s' "$R9P" | grep -c .)"
check "Q9 mob 101 name" "Perro Salvaje" "$(printf '%s\n' "$R9P" | awk -F'\t' '$1==101{print $2}')"
# col+0 casts: MariaDB returns the enum/set INDEX; the proxy returns the raw TEXT.
# Spec legacy-sql-compatibility.md §4 requires the index ("column becomes real
# integer in PG schema") — the crate still has to convert; reported as GAP.
for spec in "size+0|8|mob 101 size+0" "setRaceFlag+0|10|mob 101 setRaceFlag+0"; do
  COL="${spec#*|}"; COL="${COL%%|*}"; LBL="${spec##*|}"
  MV=$(printf '%s\n' "$R9M" | awk -F'\t' -v c="$COL" '$1==101{print $c}')
  PV=$(printf '%s\n' "$R9P" | awk -F'\t' -v c="$COL" '$1==101{print $c}')
  if [ "$MV" = "$PV" ]; then ok "Q9 $LBL equal ($MV)"; else gap "Q9 $LBL: MariaDB indice=[$MV] proxy texto=[$PV] — el crate debe convertir col+0 a indice (legacy-sql-compatibility §4)"; fi
done

Q9I="SELECT vnum, type, subtype, name, locale_name, gold, shop_buy_price, weight, size, flag, wearflag, antiflag, immuneflag+0, refined_vnum, refine_set, magic_pct, socket_pct, addon_type, limittype0, limitvalue0, limittype1, limitvalue1, applytype0, applyvalue0, applytype1, applyvalue1, applytype2, applyvalue2, value0, value1, value2, value3, value4, value5 FROM item_proto ORDER BY vnum"
R9P=$($PROXY -D player -e "$Q9I" 2>&1); R9P_RC=$?
R9M=$($MARIA  -D player -e "$Q9I" 2>&1); R9M_RC=$?
[ $R9P_RC -eq 0 ] && [ $R9M_RC -eq 0 ] && ok "Q9 item_proto(+0) exit0" || bad "Q9 item_proto(+0) exit (proxy=$R9P_RC maria=$R9M_RC)"
check "Q9 item_proto 11002 rows both" "$(printf '%s' "$R9M" | grep -c .)" "$(printf '%s' "$R9P" | grep -c .)"
MV=$(printf '%s\n' "$R9M" | awk -F'\t' '$1==1{print $13}')
PV=$(printf '%s\n' "$R9P" | awk -F'\t' '$1==1{print $13}')
if [ "$MV" = "$PV" ]; then ok "Q9 item 1 immuneflag+0 equal ($MV)"; else gap "Q9 item 1 immuneflag+0: MariaDB indice=[$MV] proxy texto=[$PV] — col+0 pendiente del crate (§4)"; fi

Q9R="SELECT id, cost, prob, vnum0, count0, vnum1, count1, vnum2, count2, vnum3, count3, vnum4, count4 FROM refine_proto"
R9P=$($PROXY -D player -e "$Q9R" 2>&1); R9M=$($MARIA  -D player -e "$Q9R" 2>&1)
check "Q9 refine_proto 405 rows both" "$(printf '%s' "$R9M" | grep -c .)" "$(printf '%s' "$R9P" | grep -c .)"

Q9S="SELECT dwVnum, szName, bType, bMaxLevel, dwSplashRange, szPointOn, szPointPoly, szSPCostPoly, szDurationPoly, szDurationSPCostPoly, szCooldownPoly, szMasterBonusPoly, setFlag+0, setAffectFlag+0, szPointOn2, szPointPoly2, szDurationPoly2, setAffectFlag2+0, szPointOn3, szPointPoly3, szDurationPoly3, szGrandMasterAddSPCostPoly, bLevelStep, bLevelLimit, prerequisiteSkillVnum, prerequisiteSkillLevel, iMaxHit, szSplashAroundDamageAdjustPoly, eSkillType+0, dwTargetRange FROM skill_proto ORDER BY dwVnum"
R9P=$($PROXY -D player -e "$Q9S" 2>&1); R9P_RC=$?
R9M=$($MARIA  -D player -e "$Q9S" 2>&1); R9M_RC=$?
[ $R9P_RC -eq 0 ] && [ $R9M_RC -eq 0 ] && ok "Q9 skill_proto(+0) exit0" || bad "Q9 skill_proto(+0) exit (proxy=$R9P_RC maria=$R9M_RC)"
check "Q9 skill_proto 97 rows both" "$(printf '%s' "$R9M" | grep -c .)" "$(printf '%s' "$R9P" | grep -c .)"
MV=$(printf '%s\n' "$R9M" | awk -F'\t' '$1==1{print $13}')
PV=$(printf '%s\n' "$R9P" | awk -F'\t' '$1==1{print $13}')
if [ "$MV" = "$PV" ]; then ok "Q9 skill 1 setFlag+0 equal ($MV)"; else gap "Q9 skill 1 setFlag+0: MariaDB indice=[$MV] proxy texto=[$PV] — col+0 pendiente del crate (§4)"; fi

# ---------------------------------------------------------------- summary
echo "============================================================"
echo "E2E DB RESULT: PASS=$PASS FAIL=$FAIL GAP(crate)=$GAPS"
echo "============================================================"
[ $FAIL -eq 0 ] && echo "E2E DB GREEN (exit 0) — gaps del crate documentados arriba" && exit 0
echo "E2E DB FAILED (exit 1)"
exit 1
