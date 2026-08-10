---
Type: Reference
Status: Proposed
Audience: Contributors
Last verified: 2026-08-10
---

# Legacy MySQL Schema — Migration Inventory (G-PG)

Reproducible inventory of the legacy MariaDB schema that the Rust rewrite (`source/reforge`,
ADR-0003/0004, ADR-0005 PostgreSQL cutover) must migrate into the single canonical PostgreSQL
database. This document records **structure only** — no rows, no credentials, no raw dumps.

Companion docs: [PostgreSQL cutover ADR (proposed)](../../decisions/0005-postgresql-cutover-and-legacy-adapter.md), [legacy wire/pack
compatibility](../protocol/legacy-compatibility.md), [AGENTS.md](../../../AGENTS.md)
(§16–17: PROTO_FROM_DB, CP949 traps, multilanguage architecture).

---

## 1. Scope

- Databases inventoried: `account`, `common`, `player`, `log`, `hotbackup` (live) and the
  `srv1_*` clones (per-server templates, same DDL, template data preloaded).
- Critical tables detailed: `account`, `player`, `player_index`, `player_deleted`, `item`,
  `item_proto`, `mob_proto`, `quest`, `affect`, `safebox`, `guild*`, `shop*`, `land`, `object*`,
  `skill_proto`, `refine_proto`, `common.*`, `log.*`.
- Every column type/nullable/default, PK, unique key, secondary index, trigger, function and
  MySQL-specific construct was captured with **read-only** queries (Section 10).
- Out of scope: row data, `mysql`/`performance_schema`/`information_schema`/`sys` admin schemas.

## 2. Runtime context (evidence)

| Fact | Value | Source |
|---|---|---|
| Server | MariaDB `10.11.18-MariaDB-0+deb12u1` (Debian 12) | `SELECT @@version` |
| Server charset / collation | `utf8mb4` / `utf8mb4_general_ci` | `@@character_set_server`, `@@collation_server` |
| Default engine | `InnoDB` (all game tables) | `information_schema.TABLES.ENGINE` |
| sql_mode | `STRICT_TRANS_TABLES,ERROR_FOR_DIVISION_BY_ZERO,NO_AUTO_CREATE_USER,NO_ENGINE_SUBSTITUTION` | `@@sql_mode` |
| Identifier case | `lower_case_table_names=0` → mixed-case table names are significant (`GameTime`, `GameTimeIP`, `GameTimeLog`) | `@@lower_case_table_names` |
| Live DBs (from db binary config `db/conf.txt`) | `SQL_ACCOUNT=account`, `SQL_COMMON=common`, `SQL_PLAYER=player`, `SQL_HOTBACKUP=hotbackup`, `SQL_LOG=log`; `PROTO_FROM_DB=1`; `TABLE_POSTFIX=""` | `/home/m2/source/metin2_svfiles/main/srv1/db/conf.txt` |
| Host | WSL `Debian-M2`, `127.0.0.1:3306` (mariadb service; start with `service mariadb start`) | runtime check 2026-08-10 |

The db binary reads the **non-prefixed** databases (`account`, `common`, `player`, `log`).
The `srv1_*` databases are created by `sql/base/db_create.sql` as per-server templates (same DDL,
with base data preloaded — see the row counts in Section 3) and are **not** used by the deployed
configuration (`TABLE_POSTFIX=""`, `SQL_*` names unprefixed) — they can be dropped in the cutover
(or kept as templates). `srv1_player` carries the same `MakeCharacter` trigger as `player`
(Section 7.4); the proto imports (`item_proto`/`mob_proto`, PROTO_FROM_DB) went to the live
`player` schema only, so the clone proto tables are empty.

## 3. Database overview

| Database | Tables | Role | Population (InnoDB estimate, 2026-08-10) |
|---|---|---|---|
| `account` | 7 | Authentication + premium time | `account.account` 1 row |
| `common` | 6 | Global config, GM lists, locale keys, exp table | `exp_table` 120, `locale` 13 |
| `player` | 38 | World state: players, items, protos, guilds, quests, shops | `item_proto` 10918, `mob_proto` 2806, `refine_proto` 405, `skill_proto` 97, `shop_item` 336, `shop` 33, `land` 108, `object_proto` 41, `item` 21, `item_attr` 54, `item_attr_rare` 20, `player` 3, `banword` 115 |
| `log` | 26 | Audit/anti-cheat logs (append-heavy) | `bootlog` 375, `log` 150, `levellog` 11 |
| `hotbackup` | 0 | Reserved for `SQL_HOTBACKUP` — **empty** | — |
| `srv1_account` / `srv1_common` / `srv1_player` / `srv1_log` / `srv1_hotbackup` | 7/6/38/26/0 | Unused clones (identical DDL, template data) | **Not empty** — preloaded template data: `srv1_common.locale` 13, `exp_table` 120; `srv1_player.shop_item` 336, `refine_proto` 405, `skill_proto` 97, `land` 108, `item_attr` 54, `item_attr_rare` 20, `object_proto` 41 (residual leftovers: `srv1_log.bootlog` 3, `srv1_player.banword` 115, `shop` 33) |

`player` tables: `affect`, `banword`, `guild`, `guild_comment`, `guild_grade`, `guild_member`,
`guild_war`, `guild_war_bet`, `guild_war_reservation`, `horse_name`, `item`, `item_attr`,
`item_attr_rare`, `item_award`, `item_proto`, `land`, `lotto_list`, `marriage`, `messenger_list`,
`mob_proto`, `monarch`, `monarch_candidacy`, `monarch_election`, `myshop_pricelist`, `object`,
`object_proto`, `pcbang_ip`, `player`, `player_deleted`, `player_index`, `quest`, `refine_proto`,
`safebox`, `shop`, `shop_item`, `skill_proto`, `sms_pool`, `string`.

`log` tables: `acce`, `bootlog`, `change_empire`, `change_name`, `chat_log`, `command_log`,
`cube`, `dragon_slay_log`, `fish_log`, `GameTimeLog`, `goldlog`, `hackshield_log`, `hack_crc_log`,
`hack_log`, `invalid_server_log`, `levellog`, `log`, `loginlog`, `loginlog2`, `money_log`,
`pcbang_loginlog`, `quest_reward_log`, `refinelog`, `shout_log`, `speed_hack`, `vcard_log`.

## 4. Critical tables (DDL summary)

### 4.1 `account.account` (authentication)

| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| `id` | `int(11) unsigned` AI | NO | — | PK; AUTO_INCREMENT=2 |
| `login` | `varchar(16)` | NO | `''` | **UNIQUE** `login`; comment `LOGIN_MAX_LEN=30` |
| `password` | `varchar(42)` | NO | `''` | **MySQL `PASSWORD()` hash format: `*` + 40 uppercase hex SHA1(SHA1(pw))** — stored value must keep the leading `*` (see AGENTS.md §5) |
| `social_id` | `varchar(7)` | NO | `''` | **KEY** `social_id` — personal identifier, used in login query |
| `email` | `varchar(100)` | NO | `''` | |
| `securitycode` | `varchar(192)` | NO | `''` | |
| `status` | `varchar(8)` | NO | `'OK'` | account ban state |
| `lang` | `varchar(4)` | NO | `'es'` | Language System ALTER (not in static `account.sql`); client overwrites on login |
| `availDt`, `create_time`, `last_play`, `gold_expire`, `silver_expire`, `safebox_expire`, `autoloot_expire`, `fish_mind_expire`, `marriage_fast_expire`, `money_drop_rate_expire` | `datetime` | NO | `current_timestamp()` | premium/expiry timestamps |
| `real_name` | `varchar(16)` | YES | `''` | personal data |
| `question1/2`, `answer1/2` | `varchar(56)` | YES | NULL | personal data |
| `cash`, `mileage` | `int(11)` | YES | `0` | |

Indexes: PK `id`; UNIQUE `login`; KEY `social_id`. No FK.

### 4.2 `player.player` (character)

| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| `id` | `int(11) unsigned` AI | NO | — | PK; AUTO_INCREMENT=4 |
| `account_id` | `int(11) unsigned` | NO | `0` | **KEY** `account_id_idx` → `account.account.id` (app-level) |
| `name` | `varchar(24)` | NO | `'NONAME'` | **KEY** `name_idx`; validated by trigger `MakeCharacter` (Section 7.4) |
| `job` | `tinyint(2) unsigned` | NO | `0` | |
| `voice`, `dir`, `level_step` | `tinyint` | NO | `0` | |
| `x`, `y`, `z` | `int(11)` | NO | `0` | **UNITS** (village c1 = 969600/278400), not cells (AGENTS.md coordinate convention) |
| `map_index`, `exit_x/y`, `exit_map_index` | `int(11)` | NO | `0` | |
| `hp`, `mp` | `int(11)` | NO | `0` | |
| `stamina` | `smallint(6)` | NO | `0` | |
| `random_hp`, `random_sp` | `smallint(5)` | NO | `0` | comment: "if lvl 0, it will be negative" |
| `playtime` | `int(11)` | NO | `0` | seconds |
| `level` | `tinyint(2) unsigned` | NO | `1` | |
| `st`, `ht`, `dx`, `iq` | `smallint(3)` | NO | `0` | stats |
| `exp` | `int(11)` | NO | `0` | |
| `gold` | `int(11)` | NO | `0` | |
| `cheque` | `int(11)` | NO | `0` | |
| `stat_point`, `skill_point`, `sub_skill_point` | `smallint(3)` | NO | `0` | |
| `quickslot` | `tinyblob` | YES | NULL | binary layout, C++ struct |
| `ip` | `varchar(15)` | YES | `'0.0.0.0'` | last login IP |
| `part_main`, `part_hair`, `part_acce` | `int unsigned` | NO | `0` | appearance item vnums |
| `part_base` | `tinyint(3) unsigned` | NO | `0` | |
| `skill_group` | `tinyint(4)` | NO | `0` | |
| `skill_level` | `blob` | YES | NULL | binary skill map |
| `alignment` | `int(11)` | NO | `0` | |
| `last_play` | `datetime` | NO | `current_timestamp()` | |
| `change_name` | `tinyint(1)` | NO | `0` | |
| `mobile` | `varchar(24)` | YES | NULL | |
| `stat_reset_count` | `smallint(5)` | NO | `0` | |
| `horse_hp`, `horse_stamina` | `smallint(4)` | NO | `0` | |
| `horse_level` | `tinyint(2) unsigned` | NO | `0` | |
| `horse_hp_droptime` | `int(10) unsigned` | NO | `0` | |
| `horse_riding` | `tinyint(1)` | NO | `0` | |
| `horse_skill_point` | `smallint(3)` | NO | `0` | |

`player.player_deleted` mirrors `player.player` exactly minus auto_increment (used on character
deletion; PK `id` non-AI).

### 4.3 `player.player_index` (account → character slots)

| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| `id` | `int(11) unsigned` | NO | `0` | PK → `account.account.id` (1:1) |
| `pid1`…`pid5` | `int(11) unsigned` | NO | `0` | character slots; each has KEY `pidN_key` → `player.player.id` |
| `empire` | `tinyint(4) unsigned` | NO | `0` | |

### 4.4 `player.item` (instance)

| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| `id` | `int(11) unsigned` AI | NO | — | PK; **AUTO_INCREMENT=50000006** (live identity counter; **new** ids are handed out by the db binary from `ITEM_ID_RANGE = 100000000 200000000` in `db/conf.txt` — a separate allocator, independent of the identity counter, see 7.5) |
| `owner_id` | `int(11) unsigned` | NO | `0` | **KEY** `owner_id_idx` → `player.id` (owner 0 = ground/system) |
| `window` | `enum('INVENTORY','EQUIPMENT','SAFEBOX','MALL','DRAGON_SOUL_INVENTORY','BELT_INVENTORY','GROUND')` | NO | `'INVENTORY'` | container discriminator (see 8.2) |
| `pos` | `smallint(5) unsigned` | NO | `0` | slot inside window |
| `count` | `int(11) unsigned` | NO | `0` | |
| `vnum` | `int(11) unsigned` | NO | `0` | **KEY** `item_vnum_index` → `item_proto.vnum` |
| `socket0..2` | `int(10) unsigned` | NO | `0` | |
| `attrtype0..6` / `attrvalue0..6` | `tinyint(3) unsigned` / `smallint(6)` | NO | `0` | |

Write pattern: `INSERT INTO item … ON DUPLICATE KEY UPDATE` (`db/src/ClientManager.cpp:1451`,
`db/src/Cache.cpp:82`) — upsert by PK `id`.

### 4.5 `player.item_proto` (item definition; PROTO_FROM_DB=1)

| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| `vnum` | `int(11) unsigned` | NO | `0` | PK |
| `name` | `varbinary(24)` | NO | `'Noname'` | **raw CP949 bytes — do not transcode** (boot resolves drop tables by name, `GetVnumByOriginalName`; see 8.4) |
| `locale_name` | `varbinary(24)` | NO | `'Noname'` | raw bytes (Spanish names from client pack dump, 2026-08-08) |
| `type`, `subtype` | `tinyint(2)` | NO | `0` | |
| `weight` | `tinyint(3)` | YES | `0` | |
| `size` | `tinyint(3) unsigned` | YES | `0` | |
| `antiflag`, `flag`, `wearflag` | `int(11) unsigned` | YES | `0` | bitmasks |
| `immuneflag` | `set('PARA','CURSE','STUN','SLEEP','SLOW','POISON','TERROR')` | NO | `''` | |
| `gold` | `int(11)` | YES | `0` | |
| `shop_buy_price` | `int(10) unsigned` | NO | `0` | |
| `refined_vnum` | `int(10) unsigned` | NO | `0` | → `item_proto.vnum` |
| `refine_set` | `smallint(11) unsigned` | NO | `0` | |
| `refine_set2` | `smallint(5) unsigned` | NO | `0` | |
| `magic_pct` | `tinyint(4)` | NO | `0` | |
| `limittype0/1`, `limitvalue0/1` | `tinyint(4) unsigned` / `int(11)` | YES | `0` | |
| `applytype0..2`, `applyvalue0..2` | `tinyint(4) unsigned` / `int(11)` | YES | `0` | |
| `value0..5` | `int(11)` | YES | `0` | |
| `socket0..5` | `tinyint(4)` | YES | `-1` | negative default on tinyint |
| `specular`, `socket_pct` | `tinyint(4)` | NO | `0` | |
| `addon_type` | `smallint(6)` | NO | `0` | |

### 4.6 `player.mob_proto` (mob definition)

PK `vnum` (`int(11) unsigned`). Notable columns:

| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| `name` | `varchar(24)` utf8mb4 | NO | `'Noname'` | Spanish names (client pack dump, 2026-08-08); **was CP949 double-encoded** — now valid utf8mb4 |
| `locale_name` | `varbinary(24)` | NO | `'Noname'` | raw bytes, Spanish |
| `rank`, `type` | `tinyint(2) unsigned` | NO | `0` | |
| `battle_type` | `tinyint(1) unsigned` | NO | `0` | |
| `level` | `smallint(3) unsigned` | NO | `1` | |
| `size` | `enum('SMALL','MEDIUM','BIG')` | NO | `'SMALL'` | |
| `ai_flag` | `set('AGGR','NOMOVE','COWARD','NOATTSHINSU','NOATTCHUNJO','NOATTJINNO','ATTMOB','BERSERK','STONESKIN','GODSPEED','DEATHBLOW','REVIVE')` | YES | NULL | |
| `mount_capacity` | `tinyint(2) unsigned` | NO | `0` | |
| `setRaceFlag` | `set('ANIMAL','UNDEAD','DEVIL','HUMAN','ORC','MILGYO','INSECT','FIRE','ICE','DESERT','TREE','ATT_ELEC','ATT_FIRE','ATT_ICE','ATT_WIND','ATT_EARTH','ATT_DARK')` | NO | `''` | |
| `setImmuneFlag` | `set('STUN','SLOW','FALL','CURSE','POISON','TERROR','REFLECT')` | NO | `''` | |
| `empire` | `tinyint(4) unsigned` | NO | `0` | |
| `folder` | `varchar(100)` | NO | `''` | |
| `on_click` | `tinyint(4) unsigned` | NO | `0` | |
| `st`, `dx`, `ht`, `iq` | `smallint(5) unsigned` | NO | `0` | |
| `damage_min/max` | `smallint(5) unsigned` | NO | `0` | |
| `max_hp` | `int(10) unsigned` | NO | `0` | |
| `regen_cycle`, `regen_percent` | `tinyint(3) unsigned` | NO | `0` | |
| `gold_min`, `gold_max` | `int(11)` | NO | `0` | |
| `exp` | `int(10) unsigned` | NO | `0` | |
| `def` | `smallint(5) unsigned` | NO | `0` | |
| `attack_speed`, `move_speed` | `smallint(6) unsigned` | NO | `100` | |
| `aggressive_hp_pct`, `aggressive_sight`, `attack_range` | mixed | NO | `0` | |
| `drop_item` | `int(10) unsigned` | NO | `0` | → `item_proto.vnum` (primary drop) |
| `resurrection_vnum` | `int(10) unsigned` | NO | `0` | |
| `enchant_*` (curse/slow/poison/stun/critical/penetrate) | `tinyint` | NO | `0` | |
| `resist_*` (sword/twohand/dagger/bell/fan/bow/fire/elect/magic/wind/poison) | `tinyint(4)` | NO | `0` | |
| `dam_multiply` | `float` | YES | NULL | |
| `summon`, `drain_sp` | `int(11)` | YES | NULL | |
| `mob_color` | `int(10) unsigned` | YES | NULL | |
| `polymorph_item` | `int(10) unsigned` | NO | `0` | → `item_proto.vnum` |
| `skill_level0..4` / `skill_vnum0..4` | `tinyint(3) unsigned` / `int(10) unsigned` | YES | NULL | → `skill_proto.dwVnum` |
| `sp_berserk`, `sp_stoneskin`, `sp_godspeed`, `sp_deathblow`, `sp_revive` | `tinyint(4)` | NO | `0` | |

### 4.7 `player.quest` and `player.affect` (persistent state, REPLACE-written)

`quest`: `dwPID int(10) unsigned`, `szName varchar(32)`, `szState varchar(64)`, `lValue int(11)`.
PK `(dwPID, szName, szState)` + secondary keys on each column. Written with
`REPLACE INTO quest (dwPID, szName, szState, lValue)` (`db/src/ClientManager.cpp:584`,
`ClientManagerGuild.cpp:74,111`, `ClientManagerEventFlag.cpp:52`).

`affect`: `dwPID int(10) unsigned`, `bType smallint(5) unsigned`, `bApplyOn tinyint(4) unsigned`,
`lApplyValue int(11)`, `dwFlag int(10) unsigned`, `lDuration int(11)`, `lSPCost int(11)`.
PK `(dwPID, bType, bApplyOn, lApplyValue)`. Written with `REPLACE INTO affect …`
(`db/src/ClientManagerPlayer.cpp:1151`).

### 4.8 Other `player` tables (condensed)

| Table | PK / unique | Columns (key) | Notes |
|---|---|---|---|
| `safebox` | PK `account_id` | `size tinyint(3) unsigned`, `password varchar(6)`, `gold int(11)` | password = 6 digits |
| `guild` | PK `id` AI | `name varchar(12)`, `sp smallint(6)` (default 1000), `master int unsigned` (→ player.id), `level`, `exp`, `skill_point`, `skill tinyblob`, `win/draw/loss`, `ladder_point`, `gold int(11)`, `token bigint(20) unsigned` | |
| `guild_member` | PK `(guild_id, pid)`, **UNIQUE `pid`** | `grade tinyint(2)`, `is_general tinyint(1)`, `offer int unsigned` | |
| `guild_grade` | PK `(guild_id, grade)` | `name varchar(12)`, `auth set('ADD_MEMBER','REMOVE_MEMEBER','NOTICE','USE_SKILL')` | note typo `REMOVE_MEMEBER` in the set value |
| `guild_war` | PK `(id_from, id_to)` | — | |
| `guild_war_bet` | PK `(war_id, login)` | `gold int(10) unsigned`, `guild int unsigned` | |
| `guild_war_reservation` | PK `id` AI | `guild1, guild2`, `time datetime`, `type`, `warprice`, `initscore`, `started`, `bet_from/to`, `winner` (default −1), `power1/2`, `handicap`, `result1/2` | |
| `guild_comment` | PK `id` AI | KEY `aaa(notice,id,guild_id)`, KEY `guild_id` | |
| `shop` | PK `vnum` | `name varchar(32)`, `npc_vnum smallint(6)` (→ mob_proto.vnum) | |
| `shop_item` | **UNIQUE `(shop_vnum, item_vnum, count)`** | `count int unsigned` | |
| `land` | PK `id` AI (293) | `map_index`, `x,y,width,height` (int unsigned), `guild_id` (→ guild.id), `guild_level_limit`, `price`, `enable enum('YES','NO')` | guild land |
| `object` | PK `id` AI | `land_id` (→ land.id), `vnum` (→ object_proto.vnum), `map_index`, `x,y`, `x_rot/y_rot/z_rot float`, `life` | |
| `object_proto` | PK `vnum` | `name`, `price`, `materials`, `upgrade_vnum`, `upgrade_limit_time`, `life`, `reg_1..4`, `npc`, `group_vnum`, `dependent_group` | |
| `refine_proto` | PK `id` AI (760) | `vnum0..4`/`count0..4`, `cost`, `src_vnum`, `result_vnum`, `prob smallint(6)` (default 100) | → item_proto.vnum |
| `skill_proto` | PK `dwVnum` | see 4.9 | |
| `horse_name` | PK `id` | `name varchar(16)` | |
| `marriage` | PK `(pid1, pid2)` | `is_married`, `love_point`, `time int unsigned` | |
| `messenger_list` | PK `(account, companion)` | — | → account.login |
| `monarch` | PK `empire` | `pid` (→ player.id), `windate`, `money bigint(20) unsigned` | REPLACE-written |
| `monarch_candidacy` | PK `pid` | `date`, `name`, `windate` | |
| `monarch_election` | PK `pid` | `selectedpid`, `electiondata` | |
| `myshop_pricelist` | **UNIQUE `(owner_id, item_vnum)`** | `price int(10) unsigned` | REPLACE-written |
| `pcbang_ip` | PK `id` AI, **UNIQUE `ip`** | `pcbang_id`, KEY `pcbang_id` | |
| `item_award` | PK `id` AI | `login varchar(16)` (→ account.login), `vnum int(6) unsigned`, `count`, `given_time`, `taken_time`, `item_id`, `why`, `socket0..2`, `attrtype0..6/attrvalue0..6`, `mall tinyint(1)` | KEYs on `given_time`, `taken_time` |
| `item_attr` / `item_attr_rare` | no PK | `apply enum(<118 values>)`, `prob`, `lv1..5`, `weapon/body/wrist/foots/neck/head/shield/ear/costume_body/costume_hair/costume_weapon/pendant/glove` (int unsigned) | attribute tables (not keyed!) |
| `lotto_list` | PK `id` AI | `server`, `pid`, `time` | |
| `sms_pool` | PK `id` AI, KEY `sent_idx(sent)` | `server`, `sender`, `receiver`, `mobile`, `sent enum('N','Y')`, `msg varchar(80)` | external SMS gateway |
| `banword` | PK `word` | — | |
| `string` | PK `name` | `text text` | locale string table (per-db) |

### 4.9 `player.skill_proto`

PK `dwVnum` (`int(11)`). Key columns: `szName varbinary(32)` (raw bytes), `bType`, `bLevelStep`,
`bMaxLevel`, `bLevelLimit`, `szPointOn`/`szPointOn2` enum (32 values), polynomial string columns
`szPointPoly`, `szSPCostPoly`, `szDurationPoly`, `szDurationSPCostPoly`, `szCooldownPoly`,
`szMasterBonusPoly`, `szAttackGradePoly`, `szPointPoly2`, `szDurationPoly2`, `szPointOn3`,
`szPointPoly3`, `szDurationPoly3`, `szGrandMasterAddSPCostPoly`, `szSplashAroundDamageAdjustPoly`
(all `varchar(100)` — parsed as `number/atk/mwep`-style formulas by the client/server; see the
`string_replace_word` over-read postmortem in AGENTS.md), `setFlag` set (29 values), `setAffectFlag`
enum (45 values) + `setAffectFlag2`, `prerequisiteSkillVnum`, `prerequisiteSkillLevel`,
`eSkillType enum('NORMAL','MELEE','RANGE','MAGIC')`, `iMaxHit`, `dwTargetRange`, `dwSplashRange`.

### 4.10 `common.*`

| Table | PK / unique | Columns | Notes |
|---|---|---|---|
| `exp_table` | PK `level` AI (121) | `exp int(10) unsigned` | level → cumulative exp |
| `gmhost` | PK `mIP` | — | trusted GM IPs |
| `gmlist` | PK `mID` AI | `mAccount varchar(16)`, `mName`, `mContactIP`, `mServerIP` (default `'ALL'`), `mAuthority enum('IMPLEMENTOR','HIGH_WIZARD','GOD','LOW_WIZARD','PLAYER')` | |
| `locale` | PK `mKey` | `mValue varchar(255)` | 13 rows: `DB_NAME_COLUMN=locale_name`, `LOCALE=spain`, `SKILL_POWER_BY_LEVEL*`, `SKILL_DAMAGE_BY_LEVEL*` |
| `priv_settings` | PK `(priv_type, id, type)` | `priv_type enum('PLAYER','GUILD','EMPIRE')`, `type int unsigned` (1=item_drop … 4=exp), `value int` (0–1000 %), `duration datetime` | REPLACE-written |
| `spam_db` | PK `word` | `score int(3)`, `type enum('SPAM','MAPS')` | |

### 4.11 `account.*` and `log.*` (condensed)

`account`: `account` (4.1), `block_exception` (`login varchar(16)`, no key),
`GameTime` (PK `UserID`, `paymenttype tinyint(2)`, `LimitTime int unsigned`, `LimitDt`, `Scores`),
`GameTimeIP` (PK `ipid` AI, **UNIQUE `(ip,startIP,endIP)`**, KEY `ip(ip)`), `GameTimeLog`
(KEY `login_key(login)`; `type enum('IP_FREE','FREE','IP_TIME','IP_DAY','TIME','DAY')`),
`iptocountry` (no keys: `IP_FROM/IP_TO/COUNTRY_NAME varchar`), `string`.

`log`: 26 append-only audit tables. Common shape: `pid/int` identifiers + `datetime` + **varbinary
text columns** (see §5). Notable: `loginlog2` (`type`/`is_gm`/`client_version` varbinary, `ip
int unsigned` = `inet_aton(...)`, `playtime datetime` — misuse of datetime for a duration),
`levellog` (comment on `pid`: "contains REPLACE query!"), `chat_log` (`where int unsigned`, `when
datetime` — **PG reserved words**, backtick-quoted in `game/src/log.cpp:327`; `type
enum('NORMAL','WHISPER','PARTY','GUILD')`, `msg varbinary(512)`), `shout_log` and
`quest_reward_log` (also carry `when`/`where`), `log` (`what bigint(11) unsigned`),
`hackshield_log.ip int unsigned` (`inet_aton`), `goldlog` (`date`/`time` split). Most log tables
have **no PK at all**.

## 5. Charset / collation matrix

- **All game databases and tables:** `utf8mb4` / `utf8mb4_general_ci` (InnoDB). Only the admin
  `sys` schema uses `utf8mb3`.
- **Column level:** every `varchar/char/text` column checked via `information_schema.COLUMNS` —
  all `utf8mb4_general_ci` in the game schemas (Section 10, query 5). No per-column exceptions.
- **Binary columns (no charset — migration hazards):**
  - `player.item_proto.name`, `player.item_proto.locale_name` → `varbinary(24)`
  - `player.mob_proto.locale_name` → `varbinary(24)` (mob `name` is varchar utf8mb4)
  - `player.skill_proto.szName` → `varbinary(32)`
  - `log.*` text-ish columns → `varbinary` (3/11/15/16/20/33/50/56/80/300/512) incl.
    `acce.success` (3), `loginlog2.client_version` (11), `ip`/name columns (15/16),
    `loginlog2.type/is_gm` (20), `goldlog.how`/`hack_log.why` (33), `log.how` (50),
    `hostname`/`quest_name`/`item_name` (56), `goldlog.hint`/`log.hint` (80),
    `command_log.command` (300), `chat_log.msg`/`shout_log.message` (512)
  - Blobs: `player.player.quickslot` (tinyblob), `player.player.skill_level` (blob),
    `player.player_deleted.*`, `player.guild.skill` (tinyblob)

### CP949 / bytea hazards (rules from AGENTS.md §15–17)

1. **`item_proto.name` must stay raw bytes.** The C++ boot resolves drop tables
   (`etc_drop_item.txt` etc.) **by name** (`GetVnumByOriginalName`, `item_manager_read_tables.cpp`)
   against `item_proto.name` — which holds **original CP949** item names as raw varbinary bytes.
   Any transcode to UTF-8 breaks server boot (`No such an item (name: …)` → boot abort). In
   PostgreSQL this column must be `bytea` and the lookups must compare raw bytes.
2. **The client translates names from its pack, the server does not** (AGENTS.md §17): visible
   names come from client `locale/es/item_proto|mob_proto`. The server columns are lookup keys,
   not display strings. Do not "fix" them for display.
3. **CP949 double-encoding incident (2026-08-08):** `mob_proto.name` was CP949→latin1→utf8mb4
   double-encoded and was rewritten with Spanish names from the client pack dump; `locale_name`
   holds those Spanish names as raw bytes (varbinary). Migration must preserve the current bytes
   of `locale_name` (varbinary → bytea) or convert with an explicit, verified mapping — no
   heuristic re-encoding.
4. **Server lua lexer requires CP949/EUC-KR (2 bytes/char) for Korean** — data-file rule, but it
   constrains any future quest/localization content migration (not the schema itself).
5. `common.locale` values are plain utf8mb4 text (skill power tables) — safe.

## 6. Foreign keys and relations

**There are ZERO declared foreign keys in the game schemas**
(`information_schema.REFERENTIAL_CONSTRAINTS` returns no rows for `account/common/player/log`;
all `SHOW CREATE TABLE` output shows bare `KEY`/`UNIQUE KEY`, no `CONSTRAINT … FOREIGN KEY`).

Relations are application-level and must be re-declared as real FKs (or documented join indexes)
in the PostgreSQL schema:

| Relation | From | To | Cardinality |
|---|---|---|---|
| account → characters | `player.player.account_id` | `account.account.id` | 1:N (KEY `account_id_idx`) |
| account → character slots | `player.player_index.id` | `account.account.id` | 1:1 (login query joins `player_index` by account id) |
| slots → characters | `player.player_index.pid1..pid5` | `player.player.id` | 5 × 1:1 (KEY `pidN_key`) |
| owner → character | `player.item.owner_id` | `player.player.id` | 1:N (KEY `owner_id_idx`) |
| item def | `player.item.vnum` | `player.item_proto.vnum` | N:1 (KEY `item_vnum_index`) |
| quest / affect | `player.quest.dwPID`, `player.affect.dwPID` | `player.player.id` | 1:N |
| safebox | `player.safebox.account_id` | `account.account.id` | 1:1 |
| guild | `player.guild.master` | `player.player.id` | 1:1 |
| guild membership | `player.guild_member.guild_id` / `pid` | `player.guild.id` / `player.player.id` | N:1 (UNIQUE `pid`) |
| guild war | `player.guild_war.id_from/id_to` | `player.guild.id` | N:1 |
| shop | `player.shop.npc_vnum` | `player.mob_proto.vnum` | N:1 |
| shop items | `player.shop_item.shop_vnum` / `item_vnum` | `player.shop.vnum` / `player.item_proto.vnum` | N:1 (UNIQUE triple) |
| myshop prices | `player.myshop_pricelist.owner_id` / `item_vnum` | `player.player.id` / `player.item_proto.vnum` | N:1 |
| land/object | `player.land.guild_id` → `guild.id`; `player.object.land_id` → `land.id`; `player.object.vnum` → `object_proto.vnum` | | N:1 |
| refine | `player.refine_proto.src_vnum/result_vnum/vnum0..4` | `player.item_proto.vnum` | N:1 |
| mob skills | `player.mob_proto.skill_vnum0..4` | `player.skill_proto.dwVnum` | N:1 |
| mob drop / polymorph | `player.mob_proto.drop_item`, `player.mob_proto.polymorph_item` | `player.item_proto.vnum` | N:1 |
| mob revive | `player.mob_proto.resurrection_vnum` | `player.mob_proto.vnum` | N:1 (self) |
| guild comments | `player.guild_comment.guild_id` | `player.guild.id` | N:1 (KEY `guild_id`) |
| guild war bets | `player.guild_war_bet.guild` | `player.guild.id` | N:1 |
| guild war reservations | `player.guild_war_reservation.guild1`/`guild2` | `player.guild.id` | N:1 |
| horse name | `player.horse_name.id` | `player.player.id` | 1:1 (PK = player id) |
| item awards (def) | `player.item_award.vnum` | `player.item_proto.vnum` | N:1 |
| item awards (instance) | `player.item_award.item_id` | `player.item.id` | N:1 (NULL until granted) |
| object upgrade | `player.object_proto.upgrade_vnum` | `player.object_proto.vnum` | N:1 (self) |
| object npc | `player.object_proto.npc` | `player.mob_proto.vnum` | N:1 |
| item awards | `player.item_award.login` | `account.account.login` | N:1 (by login, not id) |
| monarch/marriage/lotto/horse | `pid`-style columns | `player.player.id` | N:1 |
| messenger | `player.messenger_list.account/companion` | `account.account.login` | N:1 (by login) |

## 7. MySQL-specific constructs and PG implications

### 7.1 `unsigned`

Widespread (all PKs and most counters). Display widths (`int(11)`, `tinyint(2)`, …) are
MySQL-only decoration — drop them. Range mapping for PostgreSQL:

| MySQL | Range | PostgreSQL |
|---|---|---|
| `tinyint unsigned` | 0–255 | `smallint` (or `int` + CHECK) |
| `smallint unsigned` | 0–65535 | `integer` (or `int` + CHECK) |
| `int unsigned` | 0–4 294 967 295 | `bigint` — **`int` overflow risk** (`item.id` AI at 50 000 006, `exp`, `gold` paths) |
| `bigint unsigned` | 0–18 446 744 073 709 551 615 | `numeric(20,0)` (`guild.token`, `monarch.money`) |
| `float` | 4-byte | `real` (`mob_proto.dam_multiply`, `object.*_rot`) |

### 7.2 `enum` and `set`

`enum` columns (12 tables, 15 distinct enum definitions — `item_attr.apply` and
`item_attr_rare.apply` share one identical 118-value definition): `account.GameTimeLog.type`,
`common.gmlist.mAuthority`, `common.priv_settings.priv_type`, `common.spam_db.type`,
`log.chat_log.type`, `player.item.window`, `player.item_attr.apply` + `item_attr_rare.apply`
(118 values each), `player.land.enable`, `player.mob_proto.size`, `player.skill_proto.szPointOn/
szPointOn2/setAffectFlag/setAffectFlag2/eSkillType`, `player.sms_pool.sent`.

`set` columns (6 column instances): `player.guild_grade.auth`, `player.item_proto.immuneflag`,
`player.mob_proto.ai_flag/setRaceFlag/setImmuneFlag`, `player.skill_proto.setFlag` (29 values).

PG implications:
- `enum` → `text` + CHECK constraint (preferred; keeps the 118-value `item_attr.apply` manageable),
  or native PG `enum` type. The C++/protocol layer compares string values — `text` preserves
  wire-compatible literals.
- `set` → `text[]` array, or `text` + CHECK on comma-joined values (the server writes and reads
  sets as comma-separated strings; array round-trip needs an adapter in `protocol::legacy`).
- The `guild_grade.auth` value `REMOVE_MEMEBER` is a **typo baked into the data** — keep the
  literal or migrate values.

### 7.3 Zero dates

No `'0000-00-00 00:00:00'` defaults or data patterns in the game schemas (checked all
`date/datetime/timestamp/year` columns; only the admin `sys` views carry them). MariaDB
`datetime` → PG `timestamp without time zone` is a direct mapping; `log.acce.time timestamp` →
`timestamp`. `log.goldlog` splits `date` + `time` — keep or merge. `log.loginlog2.playtime
datetime` is a duration stored in a datetime — migrate to `interval`/`bigint` seconds with an
explicit decision.

### 7.4 Triggers and functions (must be re-implemented)

- **Trigger `player.MakeCharacter`** (BEFORE INSERT ON `player`, definer `mt2@localhost`,
  created 2026-08-07): validates `name` with `REGEXP '[^A-Za-z0-9]'`; on match sets
  `new.name = NULL`, which fails the NOT NULL constraint in strict mode → insert rejected.
  PG: replicate as a `BEFORE INSERT` trigger/function or a CHECK constraint (`name ~ '^[A-Za-z0-9]+$'`).
- **Function `account.mysql_hash_password(pw VARCHAR(255)) RETURNS VARCHAR(255)`** (definer
  `root@localhost`, DETERMINISTIC): `CONCAT('*', UPPER(SHA1(UNHEX(SHA1(pw)))))` — the MySQL
  `PASSWORD()` format. Used by `QUERY_LOGIN` (auth) and the login query join. PG: implement as a
  SQL function (`md5` is not SHA1 — use `digest()` from `pgcrypto`: `'*' || upper(encode(digest(
  decode(digest(pw,'sha1'),'hex'),'sha1'),'hex'))`) or verify hashes in the Rust auth crate and
  drop the SQL function. **Hashes in `account.password` must not be recomputed/rehashed on cutover.**

### 7.5 `AUTO_INCREMENT` / identity

| Table | Column | Next value (2026-08-10) | Note |
|---|---|---|---|
| `player.item` | `id` | **50000006** | live identity counter; the db binary allocates **new** ids from `ITEM_ID_RANGE = 100000000 200000000` (`db/conf.txt`) — independent of the identity counter; PG `bigserial`/`identity` must be seeded at 50 000 006 (no collision: the allocator window starts above it) |
| `player.player` | `id` | 4 | |
| `player.land` | `id` | 293 | |
| `player.refine_proto` | `id` | 760 | |
| `common.exp_table` | `level` | 121 | |
| `account.account` | `id` | 2 | |
| `GameTimeIP.ipid`, `guild.id`, `guild_war_reservation.id`, `guild_comment.id`, `object.id`, `pcbang_ip.id`, `lotto_list.id`, `sms_pool.id`, `item_award.id`, `gmlist.mID` | — | default | |

PG: `GENERATED ALWAYS AS IDENTITY` with matching `setval()`, or `bigserial`. The db binary also
uses `INSERT … ON DUPLICATE KEY UPDATE` (item) → PG `INSERT … ON CONFLICT (id) DO UPDATE`.

### 7.6 `REPLACE INTO` / upsert semantics

The C++ server writes with MySQL `REPLACE` (DELETE+INSERT, resets untouched columns) on:
`monarch`, `quest` (multiple call sites), `affect`, `horse_name`, `myshop_pricelist`, `levellog`,
`priv_settings`, `guild_invite_limit` (see 7.7). PG equivalent is `INSERT … ON CONFLICT DO
UPDATE` — **semantics differ**: REPLACE deletes the old row (so columns not present in the
statement revert to defaults). The Rust migration must decide per-table whether the upsert must
carry the full column set (match REPLACE) or only the changed columns (match ON DUPLICATE).

### 7.7 Ghost table: `guild_invite_limit`

`game/src/guild.cpp:245` and `game/src/input_main.cpp:2541` execute
`REPLACE INTO guild_invite_limit VALUES(%d, %d)` but **no such table exists** in any schema
(verified `SHOW TABLES LIKE '%guild%'` / `'%invite%'`). The writes fail silently (query error
logged). Migration decision: either create the table in PG (2 columns: guild id, unix time) or
remove the dead writes from the ported code.

### 7.8 No table comments/other

Comments exist on a few columns (e.g. `log.levellog.pid` "contains REPLACE query!",
`hackshield_log.ip` `inet_aton('%s')`, `goldlog.how` values list) — useful documentation, carried
into PG column comments.

## 8. Static sources (reproducibility)

- **Canonical DDL scripts (WSL svfiles, NOT in the Windows repo):**
  `/home/m2/source/metin2_svfiles/sql/account.sql`, `common.sql`, `player.sql`, `log.sql`,
  `base/db_create.sql` (creates the unused `srv1_*` databases), `protos/*.sql` and
  `updates/*.sql` (mob_proto/item_proto/skill_proto ALTERs, gold 64-bit, acce system).
  Cross-verified 2026-08-10: the static `account.sql` matches the live `SHOW CREATE TABLE` for
  `GameTime`, `GameTimeIP`, `GameTimeLog`, `account`.
- **Deltas applied to the live DB (verified 2026-08-10; not in the original static DDL):**
  - `account.account.lang varchar(4) DEFAULT 'es'` (Language System 1.2.6 ALTER — confirmed
    absent from the static `account.sql`, which only carries a comment about it);
  - function `account.mysql_hash_password` (created 2026-08-07/08, fix item 11);
  - trigger `player.MakeCharacter` (created 2026-08-07 18:22; the static `player.sql` also
    carries the same `CREATE TRIGGER` since that session — kept here because it postdates the
    base DDL);
  - `mob_proto`/`item_proto` name data rewritten 2026-08-08 (Spanish pack dump) — data, not DDL;
  - `player.player`/`player_index` data fixes (coordinates, slot 4) — data, not DDL.
- `account.string` / `player.string` are **not** deltas: both exist in the static
  `account.sql` / `player.sql` (locale string key/value tables, one per db).
- The Windows copy `source/server` contains **no `.sql` files** — the repo cannot rebuild the
  schema from source alone. G-PG should vendor the verified DDL (as the `schema_legacy` migration
  baseline in the Rust workspace) once approved.
- The Rust workspace already pins `protocol::legacy` (ADR-0006, proposed) for wire/pack
  compatibility; this schema inventory feeds ADR-0005 (PostgreSQL cutover adapter, proposed).

## 9. Migration hazards checklist (summary)

1. `item_proto.name`/`locale_name`, `mob_proto.locale_name`, `skill_proto.szName` → `bytea`,
   preserve bytes exactly; never transcode (boot lookup by name).
2. `int unsigned` → `bigint` (item ids at 50 000 006, gold/exp paths); `bigint unsigned` →
   `numeric(20,0)`.
3. `enum`/`set` → `text` + CHECK / `text[]`; keep literal values byte-identical (incl. typo
   `REMOVE_MEMEBER`).
4. `REPLACE INTO` semantics ≠ `ON CONFLICT` — decide per table (7.6).
5. Identity seeds: `setval` for `item` (50 000 006), `player` (4), `land` (293), `refine_proto`
   (760), `exp_table` (121), `account` (2). `item` new ids come from `ITEM_ID_RANGE`
   (100000000–200000000, `db/conf.txt`), independent of the identity counter.
6. Re-implement trigger `MakeCharacter` and function `mysql_hash_password` (or verify hashes in
   Rust auth); do not rehash `account.password` values.
7. No real FKs today — declare them in PG (Section 6) or document their absence.
8. `guild_invite_limit` ghost table — create or drop the writes.
9. `loginlog2.ip`, `hackshield_log.ip` are `inet_aton()` integers — map to `inet`/`bigint`
   deliberately.
10. Mixed-case table names (`GameTime*`) and reserved-word columns (`player.item.window`,
    `player.mob_proto.rank`, `log.chat_log.where/when`, `log.shout_log.where/when`,
    `log.quest_reward_log.when`) — quoted identifiers in PG (the backtick translation in the
    adapter must quote them too).
11. Zero dates: none in game schemas (no cleanup needed); `loginlog2.playtime` datetime-as-duration
    needs a type decision.
12. `common.locale`, `player.string`, `account.string` key/value tables — plain text, safe.

## 10. Read-only commands used (evidence)

All queries executed against WSL `Debian-M2` MariaDB (`127.0.0.1:3306`, local dev instance;
credentials not recorded here) on **2026-08-10** (UTC+2 session). No writes, no dumps, no row
export. Raw outputs kept in session temp files (not committed):
`gpg_inventory1.txt`, `gpg_ddl.txt`, `gpg_ddl2.txt`, `gpg_ddl3.txt`, `gpg_ddl4.txt`,
`gpg_ddl5.txt`.

```bash
# 1. Schemas + default charsets/collations
SELECT SCHEMA_NAME, DEFAULT_CHARACTER_SET_NAME, DEFAULT_COLLATION_NAME
FROM information_schema.SCHEMATA
WHERE SCHEMA_NAME NOT IN ('mysql','performance_schema','information_schema');

# 2. Tables: engine/collation/row estimate/create options
SELECT TABLE_SCHEMA, TABLE_NAME, ENGINE, TABLE_COLLATION, TABLE_ROWS, CREATE_OPTIONS
FROM information_schema.TABLES
WHERE TABLE_SCHEMA NOT IN ('mysql','performance_schema','information_schema')
ORDER BY TABLE_SCHEMA, TABLE_NAME;

# 3. Triggers
SELECT TRIGGER_SCHEMA, TRIGGER_NAME, EVENT_MANIPULATION, EVENT_OBJECT_TABLE, ACTION_TIMING
FROM information_schema.TRIGGERS
WHERE TRIGGER_SCHEMA NOT IN ('mysql','performance_schema','information_schema');

# 4. Routines
SELECT ROUTINE_SCHEMA, ROUTINE_NAME, ROUTINE_TYPE, DATA_TYPE
FROM information_schema.ROUTINES
WHERE ROUTINE_SCHEMA NOT IN ('mysql','performance_schema','information_schema');

# 5. Text columns with per-column charset/collation
SELECT TABLE_SCHEMA, TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT,
       CHARACTER_SET_NAME, COLLATION_NAME, COLUMN_KEY, EXTRA
FROM information_schema.COLUMNS
WHERE TABLE_SCHEMA NOT IN ('mysql','performance_schema','information_schema')
  AND DATA_TYPE IN ('varchar','char','text','tinytext','mediumtext','longtext')
  AND CHARACTER_SET_NAME IS NOT NULL;

# 6. Hazard columns: binary/enum/set/timestamp/datetime/year
SELECT TABLE_SCHEMA, TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, EXTRA
FROM information_schema.COLUMNS
WHERE TABLE_SCHEMA NOT IN ('mysql','performance_schema','information_schema')
  AND DATA_TYPE IN ('binary','varbinary','tinyblob','blob','mediumblob','longblob',
                    'enum','set','timestamp','datetime','year');

# 7. Full DDL of every game table (structure only)
SHOW CREATE TABLE <db>.<table>;   -- 77 tables: account(7) common(6) player(38) log(26)

# 8. Foreign keys — expected empty for game schemas
SELECT CONSTRAINT_SCHEMA, TABLE_NAME, CONSTRAINT_NAME
FROM information_schema.REFERENTIAL_CONSTRAINTS
WHERE CONSTRAINT_SCHEMA IN ('account','common','player','log');

# 9. Non-PK indexes
SELECT TABLE_SCHEMA, TABLE_NAME, INDEX_NAME,
       GROUP_CONCAT(COLUMN_NAME ORDER BY SEQ_IN_INDEX) AS cols, MAX(NON_UNIQUE) AS non_uniq
FROM information_schema.STATISTICS
WHERE TABLE_SCHEMA IN ('account','common','player','log') AND INDEX_NAME <> 'PRIMARY'
GROUP BY TABLE_SCHEMA, TABLE_NAME, INDEX_NAME;

# 10. Auto-increment counters
SELECT TABLE_SCHEMA, TABLE_NAME, AUTO_INCREMENT
FROM information_schema.TABLES
WHERE TABLE_SCHEMA IN ('account','common','player','log')
  AND AUTO_INCREMENT IS NOT NULL AND AUTO_INCREMENT > 1;

# 11. Server identity
SELECT @@version, @@version_comment, @@sql_mode, @@default_storage_engine,
       @@character_set_server, @@collation_server, @@lower_case_table_names;
```

Static inventory cross-checks: `SHOW CREATE TABLE` output diffed against
`/home/m2/source/metin2_svfiles/sql/*.sql`; write patterns grepped from
`source/server/{game,db}/src/*.cpp` (`REPLACE INTO`, `ON DUPLICATE KEY UPDATE`); live DB choice
verified from `main/srv1/db/conf.txt` (`SQL_*` names, `PROTO_FROM_DB=1`, `TABLE_POSTFIX=""`).
