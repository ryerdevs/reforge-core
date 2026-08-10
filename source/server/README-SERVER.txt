# README #

### What is this repository for? ###

* Quick summary
* Version
* [Learn Markdown](https://bitbucket.org/tutorials/markdowndemo)
####
# git config --global http.sslVerify false
# git config --global credential.helper "cache --timeout=3600"
###
# git add -A
# git commit -m "base added"
### or
# git commit -am "files updated"
###
# git push
###
# git pull
####

### How do I get set up? ###

* Summary of set up
// mysql dep
# cd /usr/ports/databases/mysql56-client
# make WITH_XCHARSET=all install clean
## OR
# pkg install mysql56-client

// extern extract
# cd ./Srcs
# tar -xzf Extern.tgz

// cryptopp dep
# cd ./Srcs/Extern/cryptopp
# gmake libcryptopp.a -j4

//how to compile all-the-server (db+game+dep)
# cd ./Srcs/Server
# gmake all -j4

//how to compile just game (not necessary)
# cd ./Srcs/Server/game/src
# gmake -j4

//how to compile just db (not necessary)
# cd ./Srcs/Server/db/src
# gmake -j4

* Configuration
@#GENERAL MACROS
#define __OBSOLETE__				// useless and pointless code removed
#define __UNIMPLEMENTED__			// drafts of new things to be implemented

@/Src/Server/common/CommonDefines.h
>>default
#define _IMPROVED_PACKET_ENCRYPTION_		// enable cryptopp mixed encryption for client<>server comunication
#define USE_IMPROVED_PACKET_DECRYPTED_BUFFER	// enable a buffer for decrypting incoming packets sent at different times (for IMPROVED_PACKET_ENCRYPTION only)
#define USE_NO_PACKET_ENCRYPTION		// disable the default tea encryption (only if _IMPROVED_PACKET_ENCRYPTION_ is also disabled)
#define ENABLE_SEQUENCE_SYSTEM			// enable sequence system
#define ENABLE_QUEST_CATEGORY			// enable quest category+new packet types (unimplemented)
>>systems
#define ENABLE_D_NJGUILD			// enable d.new_jump_all_guild+cpp relative functions (untested)
#define ENABLE_NEWSTUFF				// enable new stuff (new lua funcs, new CONFIG options, ecc)
#define ENABLE_FULL_NOTICE			// enable new big notice features
#define ENABLE_PORT_SECURITY			// block db_port, p2p_port, and remote adminpage exploits
#define ENABLE_BELT_INVENTORY_EX		// move the belt items into the BELT_INVENTORY window and prevents unknown belt positions if you de/increase wear/inventory slots
#define ENABLE_CMD_WARP_IN_DUNGEON		// /warp <player> will warp successfully even if the player is inside a dungeon (be sure a .quest login event won't still warp you out)
#define ENABLE_USE_COSTUME_ATTR			// enable the items reset costume and enchant costume
#define ENABLE_ITEM_ATTR_COSTUME		// enable costume_hair, costume_body, costume_weapon item_attr/_rare parts
#define MAP_ALLOW_LIMIT <int>			// define how many maps are allowed per game core (default 32)
#define ENABLE_PLAYER_PER_ACCOUNT5		// enable 5 characters (per account) in the select phase (instead of 4)
#define ENABLE_DICE_SYSTEM			// enable dice system: if the mob is king or boss and you're in party, the dropped item is randomly rolled
#define ENABLE_EXTEND_INVEN_SYSTEM		// enable 4 inventory pages
#define ENABLE_MOUNT_COSTUME_SYSTEM		// enable mount costume slot
#define ENABLE_WEAPON_COSTUME_SYSTEM		// enable weapon costume slot
#define ENABLE_PET_SYSTEM_EX			// enable item pet without quest	[@warme666]
#define ENABLE_SKILL_FLAG_PARTY			// enable skill flag party (separated from lycan define)
#define ENABLE_NO_DSS_QUALIFICATION		// disable dragon soul qualification [@warme666]
#define ENABLE_NO_SELL_PRICE_DIVIDED_BY_5	// disable dividing the sell price by 5 [@warme666]
#define ENABLE_CHECK_SELL_PRICE			// it checks if the sell price is higher than the buy price; if yes, sell price will be equal to the buy price
#define ENABLE_GOTO_LAG_FIX			// it force-clears the entities when switching sectree
#define ENABLE_MOUNT_COSTUME_EX_SYSTEM		// mount load via item_proto with APPLY_MOUNT (118) bonus like official
#define ENABLE_PENDANT_SYSTEM			// pendant equip implementation
#define ENABLE_GLOVE_SYSTEM			// glove equip implementation
#define ENABLE_MOVE_CHANNEL			// enable channel switch
#define ENABLE_REDUCED_ENTITY_VIEW		// it reduces the complexity of the entity view
#define ENABLE_ITEM_AUTOSTACK_EX		// autostacks purchased/crafted/etc items
#define ENABLE_ITEM_DS_INVENTORY_PROCESS	// process the ds inventory when counting/removing items
#define ENABLE_REGEN_RENEWAL			// regens events will be called only after the mobs' death
#define ENABLE_DS_GRADE_MYTH			// enable mythic ds grade
#define ENABLE_CHANNEL_STATUS_CACHE		// enable channel status cache (antiflood check)
#define ENABLE_MESSENGER_REMOVE_SYNC		// enable friend remove sync [@warme666]
#define ENABLE_DB_SQL_LOG			// enable SQL_LOG connection for db
#define ENABLE_ITEM_GROUND_EX			// extend item ground with item count socket attr [@warme666]
>>quests
#define ENABLE_QUEST_DIE_EVENT			// add quest event "die"
#define ENABLE_QUEST_BOOT_EVENT			// add quest event "boot"
#define ENABLE_QUEST_DND_EVENT			// add quest event "dnd"
>>lycan
#define ENABLE_WOLFMAN_CHARACTER		// enable wolfman character and the relative new features (claws, bleeding and so on) [@warme666]
#define DISABLE_WOLFMAN_ON_CREATE		// disable wolfman on create phase [@warme666]
#define USE_MOB_BLEEDING_AS_POISON		// if enabled, the mob_proto structure won't change and the bleeding % will be get from the poison field
#define USE_MOB_CLAW_AS_DAGGER			// if enabled, the mob_proto structure won't change and the claw % will be get from the dagger field
#define USE_ITEM_BLEEDING_AS_POISON		// if enabled, the poison reduce bonus can also reduce the bleeding damage as if it's bleeding reduce itself
#define USE_ITEM_CLAW_AS_DAGGER			// if enabled, the resist dagger bonus can also reduce the claw damage as if it's resist claw itself
#define USE_WOLFMAN_STONES			// if enabled, lycan stones can be dropped from the metin stones
#define USE_WOLFMAN_BOOKS			// if enabled, lycan skill books can be dropped
>>magic reduction stone
#define ENABLE_MAGIC_REDUCTION_SYSTEM		// enable resist magic reduction bonus
#define USE_MAGIC_REDUCTION_STONES		// enable resist magic reduction stone drops from metins
>>pet system
#define __PET_SYSTEM__				// enable traditional pet system
#define USE_ACTIVE_PET_SEAL_EFFECT		// it will activate the pet slot if it's active
#define USE_PET_SEAL_ON_LOGIN			// it will automatically spawn the pet after a rewarp
>>ex
#define DISABLE_STOP_RIDING_WHEN_DIE		// if DISABLE_TOP_RIDING_WHEN_DIE is defined, the player doesn't lose the horse after dying
#define ENABLE_ACCE_COSTUME_SYSTEM		// enable sash system
#define USE_ACCE_ABSORB_WITH_NO_NEGATIVE_BONUS	// enable only positive bonus in acce absorb
#define ENABLE_HIGHLIGHT_NEW_ITEM		// if you want to see highlighted a new item when dropped or when exchanged
#define ENABLE_KILL_EVENT_FIX			// if you want to fix the 0 exp problem about the when kill lua event (recommended)
#define ENABLE_SYSLOG_PACKET_SENT		// debug purposes
#define ENABLE_MOB_DROP_POLY			// enable drop type 'poly' for special_item_group.txt (idx drop mobvnum pct customitemvnum)
#define USE_ITEM_AWARD_CHECK_ATTRIBUTES		// it prevents bonuses higher than item_attr lvl1-lvl5 min-max range limit
#define __BL_OFFICIAL_LOOT_FILTER__		// enable Loot Filter System
#define __PREMIUM_LOOT_FILTER__			// enable Premium Usage of the Loot Filter System
#define ENABLE_CHEQUE_SYSTEM			// enable won
#define ENABLE_SHOP_USE_CHEQUE			// enable won usable in shops
#define DISABLE_CHEQUE_DROP			// disable won droppable into the ground by players
#define ENABLE_WON_EXCHANGE_WINDOW		// enable won conversion to yang and vice versa

@/Src/Server/db/src/ClientManager.h
#define ENABLE_PROTO_FROM_DB			// read protos from db if "PROTO_FROM_DB = 1" is specified inside conf.txt
,,						// mirror protos to db if "MIRROR2DB = 1" is specified inside conf.txt

@/Src/Server/db/src/ClientManager.cpp
#define ENABLE_DEFAULT_PRIV			// enable default priv loading from common.priv_settings
#define ENABLE_ITEMAWARD_REFRESH		// enable a select query every 5 seconds into player.item_award

@/Src/Server/db/src/ClientManagerBoot.cpp
#define ENABLE_AUTODETECT_VNUMRANGE		// if protos are loaded from db, it will automatically detect the vnum range for ds items

@/Src/Server/db/src/ProtoReader.cpp
#define ENABLE_NUMERIC_FIELD			// txt protos now can read numbers instead of tags as well

@/Srcs/Server/game/src/stdafx.h
>moved to CommonDefines.h

@/Srcs/Server/game/src/stdafx.h
#define ENABLE_LIMIT_TIME			// enable game timestamp expiration

@/Srcs/Server/game/src/BlueDragon.h
#define ENABLE_BLUEDRAGON_RENEWAL		// if enabled, the beran can't be damaged if all the stones are not destroyed (official 17.3)

@/Srcs/Server/game/src/guild.h
#define ENABLE_NEWGUILDMAKE			// enable pc.make_guild0 and disable CInputMain::AnswerMakeGuild

@/Srcs/Server/game/src/horse_rider.h
#define ENABLE_INFINITE_HORSE_HEALTH_STAMINA	// full horse health and stamina all the times

@/Srcs/Server/game/src/config.cpp
#define ENABLE_CMD_PLAYER			// enable PLAYER grade inside CMD
#define ENABLE_EXPTABLE_FROMDB			// read the exp table from the db
#define ENABLE_AUTODETECT_INTERNAL_IP		// autodetect internal ip if the public one is missing
#define ENABLE_GENERAL_CMD			// if enabled, it reads a general CMD from "locale/%s/conf/GENERAL_CMD", "locale/%s/conf/GENERAL_CMD_CHANNEL_%d", and/or "locale/%s/conf/GENERAL_CMD_CHANNEL_%d_HOSTNAME_%s"
#define ENABLE_GENERAL_CONFIG			// if enabled, it reads a general CONFIG from "locale/%s/conf/GENERAL_CONFIG", "locale/%s/conf/GENERAL_CONFIG_CHANNEL_%d", and/or "locale/%s/conf/GENERAL_CONFIG_CHANNEL_%d_HOSTNAME_%s"
,,						// in the GENERAL_CONFIG, all the options are valid except: HOSTNAME, CHANNEL, PLAYER_SQL, COMMON_SQL, LOG_SQL, PORT, P2P_PORT, MAP_ALLOW, AUTH_SERVER, TEEN_ADDR, TEEN_PORT

@/Srcs/Server/game/src/char_resist.cpp
#define ENABLE_IMMUNE_PERC			// enable 90% of success instead of 100% regarding immunes (antistun/slow/fall)

@/Srcs/Server/game/src/item.cpp
#define ENABLE_IMMUNE_FIX			// fix immune bug where you need to equip shield at last (or refresh compute e.g. un/riding horse)

@/Srcs/Server/game/src/item_manager.h
#define ENABLE_ITEM_PROTO_MAP			// improves GetTable load by O(1)

@/Srcs/Server/game/src/db.cpp
#define ENABLE_SPAMDB_REFRESH			// enable a select query every 10 minutes into common.spam_db

@/Srcs/Server/game/src/questlua.cpp
#define ENABLE_TRANSLATE_LUA			// enable translate.lua loading
#define ENABLE_QUESTLIB_EXTRA_LUA		// enable questlib_extra.lua loading

@/Srcs/Server/game/src/questlua_dungeon.cpp
#define D_JOIN_AS_JUMP_PARTY			// d.join will internally work as d.new_jump_party

@/Srcs/Server/game/src/questlua_pc.cpp
#define ENABLE_LOCALECHECK_CHANGENAME		// enable check that unable change name on Europe Locales
#define ENABLE_PC_OPENSHOP			// enable pc.open_shop0(idshop) but buy/sell not work yet

@/Srcs/Server/game/src/shop.cpp
#define ENABLE_SHOP_BLACKLIST			// enable ignore 70024 (Blessing Marble) and 70035 (Magic Copper Ore)

@/Srcs/Server/game/src/cmd.cpp
#define ENABLE_BLOCK_CMD_SHORTCUT		// if enabled, people won't be able to shorten commands

@/Srcs/Server/game/src/cmd_gm.cpp
#define ENABLE_STATPLUS_NOLIMIT			// disable only 90 points for con+/int+/str+/dex+ commands
#define ENABLE_SET_STATE_WITH_TARGET		// enable set_state target as 3rd arg
#define ENABLE_CMD_IPURGE_EX			// /ipurge 2nd arg can remove items from a specific window (inv/equip/ds/belt/all)

@/Srcs/Server/game/src/char_skill.cpp
#define ENABLE_FORCE2MASTERSKILL		// skill always pass to m1 when b17 instead of b(number(17-20))
#define ENABLE_NO_MOUNT_CHECK			// check whether horse mount vnum should be checked when skilling
#define ENABLE_NULLIFYAFFECT_LIMIT		// sura skill 66 won't nullify players with level < or > of yours by 9
#define ENABLE_MASTER_SKILLBOOK_NO_STEPS	// if enabled, you will only need a book to increase a master skill, and not as many as the level-20

@/Srcs/Server/game/src/char_item.cpp
#define ENABLE_FIREWORK_STUN			// enable stun affect when using firework items
#define ENABLE_ADDSTONE_FAILURE			// enable add stone failure
#define ENABLE_EFFECT_EXTRAPOT			// enable extrapot effects when using green/purple potions
#define ENABLE_ITEM_RARE_ATTR_LEVEL_PCT		// enable 1-5 level pct for item rare attr add/change
#define ENABLE_UNIQUE_ITEM_AUTOSPLIT		// enable autosplit when equipping stacked unique items

@/Srcs/Server/game/src/char_battle.cpp
#define ENABLE_NEWEXP_CALCULATION		// recalculate exp rate so you won't get random negative exp/marriage points
#define ENABLE_EFFECT_PENETRATE			// enable penetrate effect when performing a penetration
#define ENABLE_NO_DAMAGE_QUEST_RUNNING		// if enabled, no damage will be dealt to monsters while running quests (otherwise the quest kill event wouldn't be triggered)

@/Srcs/Server/game/src/char.h
#define NEW_ICEDAMAGE_SYSTEM			// add new system for nemere dungeon and so on
#define ENABLE_ANTI_CMD_FLOOD			// limit player's command execution to 10 commands per second, otherwise it'll be disconnected!
#define ENABLE_OPEN_SHOP_WITH_ARMOR		// if enabled, people can open a personal shop with the armor equipped
#define ENABLE_SKILL_COOLDOWN_CHECK		// if enabled, it will add a check on the skill usage to prevent some damage hacks

@/Srcs/Server/game/src/char.cpp
#define ENABLE_SHOWNPCLEVEL			// show Lv %d level even for NPCs (not applicable on mob/stone/warp)
#define ENABLE_GOHOME_IF_MAP_NOT_ALLOWED	// you'll go back to your village if you're not allowed to go in that map
#define ENABLE_GM_FLAG_IF_TEST_SERVER		// show the gm flag if it's on test server mode
#define ENABLE_GM_FLAG_FOR_LOW_WIZARD		// show the gm flag for low wizard too
#define ENABLE_MOUNT_ENTITY_REFRESH		// it will refresh entities when riding/unriding (it's also a quick work-around when the client desyncs)

@/Srcs/Server/game/src/input_db.cpp
#define ENABLE_GOHOME_IF_MAP_NOT_EXIST		// you'll go back to your village if the map doesn't exist

@/Srcs/Server/game/src/guild.cpp
#define ENABLE_GUILD_COMMENT_ANTIFLOOD		// prevent flood in guild comments

@/Srcs/Server/game/src/mining.cpp
#define ENABLE_PICKAXE_RENEWAL			// if the upgrading of the pickaxe will fail, it won't turn back of 1 grade, but just lose 10% mastering points.
#define ENABLE_AUTO_MINING			// auto-restart mining after it's done
#define ENABLE_AUTO_PICK_ORE			// give ore crystals directly in inventory if there's space

@/Srcs/Server/game/src/fishing.cpp
#define ENABLE_FISHINGROD_RENEWAL		// if the upgrading of the fishing rod will fail, it won't turn back of 1 grade, but just lose 10% mastering points.

@/Srcs/Server/game/src/questmanager.cpp
#define ENABLE_PARTYKILL			// re-enable PartyKill

@/Srcs/Server/game/src/input_auth.cpp
#define ENABLE_ACCOUNT_W_SPECIALCHARS		// enable special characters in account names (account.account.login)

@/Srcs/Server/game/src/input_main.cpp
#define ENABLE_CHAT_COLOR_SYSTEM		// enable chat colors based on IsGm or GetEmpire (+colored empire name)
#define ENABLE_CHAT_SPAMLIMIT			// limit chat spam to 4 messages for 5 seconds, if you spam it for 10 times, you'll be disconnected!
#define ENABLE_WHISPER_CHAT_SPAMLIMIT		// limit whisper chat to 10 messages per 5 seconds, otherwise you'll be disconnected!
#define ENABLE_CHAT_LOGGING			// enable chat logging (which saves all the gm chats)
#define ENABLE_CHECK_GHOSTMODE			// enable check that blocks the movements if the character is dead
#define ENABLE_TP_SPEED_CHECK			// enable speed check teleport back
#define ENABLE_SAFEBOX_MOVE_ANTIFLOOD		// enable antiflood check when moving items in and out the safebox

@/Srcs/Server/game/src/input_login.cpp
#define USE_LYCAN_CREATE_POSITION		// if enabled, the lycan will be warped to his own village at character creation

@/Src/Server/game/src/quest/qc.cc
#define ENABLE_WHEN_PARENTHESIS			// it enables () [] usage in the when cause for the quest file

* Dependencies
@/Srcs/Extern/include/boost
@/Srcs/Extern/include/cryptopp
@/Srcs/Extern/include/IL
@/Srcs/Extern/lib:
	libIL.a			//from DevIL
	libcryptopp.a	//from cryptopp
	libjasper.a
	libpng.a
	libtiff.a
	libjbig.a
	libmng.a
	liblcms.a
	libjpeg.a
@/usr/lib/liblzma.a	//or lib32

* Database configuration
* How to run tests
* Deployment instructions

### Contribution guidelines ###

* Writing tests
* Code review
#@general
@warme001: be aware about PLAYER_MAX_LEVEL_CONST (common/length.h) and gPlayerMaxLevel (game/config.h)
@warme002: be aware about ITEM_MAX_COUNT (common/item_length.h) and g_bItemCountLimit (game/config.h)
@warme003: do_click_safebox can be used by PLAYER in every map!
@warme004: `when vnum.kill begin` and `when kill begin` are both triggered
@warme005: different locale stuff
@warme006: not implemented stuff from another locale
@warme007: on db/src/ClientManager.cpp; commented locale set from common.locale due to its uselessness and bugginess (./close && ./start)
			it processes a NULL mysql connection (dat ymir threading) if there was a bit of overload before starting the process up again
@warme008: on char_item.cpp; now 27996 (poison bottle) can inflict poison
@warme009: on char_resist.cpp; if bleeding is used as poison, the bleeding enchantment is poison enchantment/50 (so mobs can bleed players)
@warme010: on char_state.cpp; test_server is used as "BOOL g_test_server"
@warme011: on dungeon.cpp; you should never use d.join instead of d.new_jump_party since it causes random crashes due to a wrong implementation of the party hash check
@warme012: trivial errors are now considered as simple logs
@warme013: unneccessary errors are now simply commented
@warme014: wrong lua return arguments number
@warme015: increased buffer size for prevention
@warme016: improved info in the relative print
@warme017: memory leaks or memory corruptions detected by asan
@warme019: wrong inventory slot type BYTE changed to WORD
@warme020: containers that edit themselves inside the loop may have dangling iterators

#@common
@fixme400: on tables.h; TPlayerTable hp/mp from short to int (hp/mp >32767 should be fixed)

#@db/src
@fixme201: on ProtoReader.cpp; changed 'SAMLL' into 'SMALL'
@fixme202: on ClientManagerGuild.cpp; fixed the guild remove member time issue if the player was offline
			(withdraw_time -> new_withdraw_time)
@fixme203: on ClientManagerPlayer.cpp; dandling pointer for "command"
@fixme204: on Cache.cpp; myshop_pricelist primary key duplication error if there are many items of the same vnum in the personal shop
@fixme205: on Cache.cpp, ClientManager.cpp; "replace into item" was deleting any foreign keys pointed to player.item

#@game/src
@fixme101: on log.cpp; fixed '%s' for invalid_server_log
@fixme102: on cmd_general.cpp; inside ACMD(do_war) fixed the unsigned bug
@fixme103: on config, input_login, input_main.cpp; fixed clientcheckversion (version > date) to (version != date) and delay from 10 to 0
@fixme104: on char.cpp, questlua_pc.cpp; fixed get status point after lv90 changing 90 with gPlayerMaxLevel
@fixme105: on cmd.cpp; disabled every korean command
@fixme106: on input_main.cpp; if a full-speeded player is on a mount (es. lion), he'll be brought back due to the distance range
@fixme107: on char_battle.cpp; if character (player|mob) has negative hp on dead, sura&co will absorb hp/mp losing 'em themselves
@fixme108: on char.cpp; if you change a mount but the previous is not 0, all the entities (npcs&co) in the player client
			(not others) are vanished until another refresh (not exists mounts still bug you after second mount call)
@fixme109: on questmanager.cpp; if you kill a player (war m), `when kill begin` will be triggered twice
@fixme110: on char_affect.cpp; if you attack when semi-transparent (revived or ninja skill or white flag) you'll still be transparent
@fixme111: on test.cpp; ConvertAttribute2 has x and y inverted (before y->x after x->y)
@fixme112: on char_item.cpp; you can change bonuses in equipped items too (until re-login)
			bonus values will not be refreshed by ChangePoint and unequipping it will remove back only the new bonuses set on
			e.g. you had a 2500hp bonus shoes, you changed it to 50mp when equipped and you'll unequipped
				what will it happen? instead of remove 2500hp, you won't receive 50mp and you also lose 50mp when unequipped
@fixme113: on char_item.cpp; same thing of #112
			you can remove stones from equipped items w/o losing bonuses
			e.g. have an item with antiwar+4 equipped:
					1) remove all the stones 2) unequip it 3) re-add stone 4) re-equip it 5) repeat it thrice
				result? an item with no stones but you'll have 75% of antiwar
@fixme114: on char_item.cpp; gathering of #111, #112 and few others.
@fixme115: on char_item.cpp; you can retrieve all the item on the ground if you're in a party and the owner is not in yours.
@fixme116: on char_skill.cpp; normal horse mount skills cannot inflict damage
@fixme117: on char_item.cpp; you can't swap equipment from inventory if full, and also prevent unmotivated belt swap if its inventory is not empty
@fixme118: on char.cpp; when ComputePoints is called:
			you'll gain as many hp/mp as many you have in your equipment bonuses
			affect hp/mp will be lost when login or updating
@fixme119: on input_main.cpp; you can put items from safebox/mall to belt inventory w/o checking the type (items with size>1 are not placeable anyway)
@fixme120: on input_login.cpp; few packet IDs not checked
@fixme121: on char_item.cpp; the refine scroll item value 1 from the magic stone was generating useless syserrs
@fixme122: on arena.cpp; few other potions were not checked on arena map
@fixme123: on char_item.cpp; USE_CHANGE_ATTRIBUTE2 (24) sub type check bug (the condition could never be true)
@fixme124: on char_item.cpp; no check on 6-7 add/change items about costume stuff
@fixme125: on char.cpp; dungeon regen pointing to a dangling pointer (not required -> removed)
@fixme126: on marriage.cpp; fix lovepoints overflow
@fixme127: on cube.cpp; /cube r_info exploit fix; it can cause a crash due to an unchecked cube npc masters vnums
			e.g. 1) you open the Baek-Go cube's console 2) click on an npc/kill a mob without close the cube console
				3) digit /cube r_info 4) crash core
@fixme128: on char.cpp; mining hack fix; you can mine a vein anywhere in the map because there's no check on the character
			which means, you can stay at 0x0y and mining a vein in 666x999y and get the stuff beside him or in the pc's inventory
@fixme129: on PetSystem.cpp; the azrael pets (53005->34004 normal/53006->34009 gold) don't give the buff if not in dungeon at summon up and remove them anyway when unsummoned
@fixme130: on messenger_manager.cpp; and cmd_general.cpp if you do /messenger_auth n XXX, the player with the name XXX will receive a "refused friend invite" print from you
			which means, if you flood this packet, the "victim" will be disconnected or at maximum could get lag
@fixme131: on char.cpp; fix annoying sync packets sendable even on unfightable pc/npc entities
			e.g. wallhack against players' shops inside the village's squares (where the NOPK attr is set) to move them out and kill them
@fixme132: on shop.cpp; if two people buy the same item at the same time from a pc's shop, the slower one will receive a wrong return packet (crash client)
@fixme133: on input_main.cpp; banword check and hyper text feature were processing the final chat string instead of the raw one
@fixme134: on questlua_pc.cpp; the pc.mount_bonus was addable even if the mount wasn't spawn (only /unmount pc.unmount can remove it)
@fixme135: on char.cpp; if the Sync is made before a move packet and the sectree differs of few x/y coordinates, the sectree will be changed without update (crash character) (troublesome -> removed)
@fixme136: on char.cpp; there are no checks about the zero division exception: e.g. if you set a mob's max hp to 0 in the mob proto, you'll get random crashes.
@fixme137: on char_battle.cpp; when a player dies, the HP could have a negative value. Now it's 0 like the official.
@fixme138: on db.cpp, input_auth.cpp; the account's password was shown in the mysql history queries as clear text at every login attempt (mysql full granted user required -> now hashed)
@fixme139: on shop.h; CShop class destructor wasn't virtual. If a derived class like CShopEx was deleted, a memory leak would have been generated.
@fixme140: on input_main.cpp; the belt could be put into the safebox even though the belt inventory isn't empty.
@fixme141: on char_item.cpp; the items in the belt inventory could be used even if their slot were not available
@fixme142: on messenger_manager.cpp; sql injection fix about net.SendMessengerRemovePacket
@fixme143: on guild_manager.cpp; sql injection fix about net.SendAnswerMakeGuildPacket
@fixme144: on sectree_manager.cpp; if map/index doesn't end with a newline, the game will crash (before refactory)
@fixme145: on input_main.cpp; guild_add_member can add any vid as guild's member even if it's a mob or an npc
@fixme147: on char_item.cpp; ramadan candy item can be used even if the relative affect is still up
@fixme148: on item_manager_read_tables.cpp; type quest, special, attr not handled in ConvSpecialDropItemFile
@fixme149: on char.cpp; refine material skip exploit if items are swapped
@fixme150: on exchange.cpp; char_item.cpp; prevent item module swapping if the quest is suspended
@fixme152: on questlua_pc.cpp; pc.get_special_ride_vnum was checking socket2 instead of socket0
@fixme153: on threeway_war.cpp; kills made of people over lvl99 weren't counted
@fixme154: on cmd_gm.cpp; the /all_skill_master command will now set the right amount of points to the sub skills
@fixme156: on char_affect.cpp; prevent doubling the affect values before they are loaded (such as pc.mount_bonus at login; because the quest is loaded before the quests)
@fixme157: on OxEvent.cpp; the attender list wasn't cleared after stopping the ox event
@fixme158: on input_main.cpp; the deviltower refiner won't set the flag to 0 anymore if you have no money, and it will decrease it by 1 for allowing multiple refine attempts
@fixme159: on exchange.cpp; when exchanging, a wrong check in the ds items was not allowing the exchange due to "not enough space in ds inventory" if the first sub ds inventory slot was not empty
@fixme160: on DragonSoul.cpp; when removing a ds stone, if the destination slot wasn't empty, the ds item in there would have been replaced and lost
@fixme168: on questevent.cpp; if the quest info name is already null, the std::string constructor will throw an exception (refactored for increasing timer name limit)
@fixme169: on char_item.cpp; missing checks for mythical peach like a second vnum and nullptr
@fixme170: on item.cpp; missing condition on special mineral slots for just 2 bonuses
@fixme171: on dungeon.cpp; FCountMonster was also counting horses and pets
@fixme172: on dungeon.cpp; FNotice was sending ChatPackets to even non-player characters
@fixme173: on item.cpp; by pressing \\z to pick up items, sometimes, it would fail due to far distance
@fixme174: on questmanager.cpp; in case of quest error in server_timer, it would try to print the error to the current player causing a core crash
@fixme177: on cmd_gm.cpp; /priv_guild snprintf had an empty format
@fixme179: on ClientManagerBoot.cpp, ProtoReader.cpp; the item size with 0 length will cause game cores
@fixme180: on cmd_general.cpp; /costume will cause game core crashes if the relative costume bonus ids aren't present inside fn_string or have no %d
@fixme182: on input_login.cpp; you'd get no horse level if you relogged in with no job set
@fixme183: on input_main.cpp, messenger_manager.cpp; player rename wasn't clearing the previous nickname from the friendlist
@fixme184: on length.h; some slots of the dragon soul inventory weren't working properly
@fixme185: on ClientManagerBoot.cpp; the material count var isn't initialized and cause crashes on certain circumstances
@fixme186: on item.cpp; the ClearMountAttributeAndAffect without owner would trigger a core crash
@fixme187: on input.cpp; the input buffer will keep increasing until it saturates the ram if people keep sending broken packets
@fixme188: on char.h, questlua_npc|dungeon|global.cpp, dungeon.cpp; the scripted npc->Dead() was triggering player rewards if the mob was previously damaged
@fixme189: on item_manager.cpp; the Stones array had an out of range error
@fixme190: on input_login.cpp; exploit by accessing non-existing players
@fixme191: on safebox.cpp; switching safebox size level was causing a memory leak
@fixme192: on char_battle.cpp; the deathblow wasn't affecting warrior and wolfman races
@fixme193: on guild_manager.cpp; observer players can be killed to increment the guild war kills
@fixme194: on char_battle.cpp; mobs killed in non attackable zones weren't dropping yangs/items
@fixme195: on regen.cpp; missing 'ma' 'ra' 'sa' handling regen type
@fixme196: on char_item.cpp; possible item duplication bug while moving splitted items onto the same slot
@fixme197: on cmd_general.cpp; stucked connections while fully quitting from game would leave the character online
@fixme198: on char_battle.cpp; karma drop duplication bug: the dropped item gets duplicated after warping to another game core
@fixme199: on char.h, char.cpp, char_item.cpp, char_quickslot.cpp; the mobs were allocating unused memory
@fixme300: on cmd_gm.cpp; /full and /ipurge were causing a glitch in the status page by having negative status
@fixme301: on input_main.cpp, char.cpp; leaders and party members can leave the party while being on other channels
@fixme302: on questpc.cpp; HEADER_GC_QUEST_INFO had a wrong calculated size
@fixme303: on item_manager_read_tables.cpp; the load of items had O(n) complexity for each vnum of any Group
@fixme304: on char.h, char.cpp; yielded quest states were pointing to dangling item pointers
@fixme305: on char_skill.cpp; skill affects were not cleared after a skill reset
@fixme306: on char.cpp; real time items were not expired if inside the safebox
@fixme307: on cmd_general.cpp; crash core if the item for feeding the horse didn't exist in the item proto
@fixme308: on sectree_manager.cpp; when loading map/index, if the map is already loaded, the regen will duplicate
@fixme309: on questlua_pc.cpp; flushing the cache after changing the player name was causing item duplication
@fixme310: on ClientManager.cpp; only the first 3 players of the account had a full empire select check
@fixme311: on questmanager.cpp; CancelServerTimers not properly erasing the element from the container
@fixme312: on char_skill.cpp; mobs could ran away on death instead of playing the death animation
@fixme313: on char.cpp; very fast mounts would reach coordinates like -2147483648 and being unable to sync
@fixme314: on char_item.cpp; unique items can now be be stackable without issues
@fixme315: on building.cpp; skip guild land load from other maps
@fixme316: on char_item.cpp; items with sockets set at runtime weren't automatically stacked properly
@fixme317: on questnpc.cpp; quest chat scripts "with" conditions weren't selecting any quest
@fixme318: on char_change_empire.cpp; get account id for empire could get abused
@fixme319: on cmd_gm.cpp, desc.cpp, input.cpp; login key wasn't cleared properly and banned people could relogin until db restart
@fixme320: on battle.cpp; CalcAttackRating was miscalculating the victim level
@fixme321: on cmd_gm.cpp; the /poly command wasn't properly removing the polymorph from the marble item before transforming
@fixme322: on dragon_soul_table.cpp, DragonSoul.cpp; dragon_soul_table.txt steps size calculated as if they were grades, and vice versa
@fixme323: on item.cpp; workaround that deletes dangling item pointers from being updated
@fixme324: on length.h; the IsBeltInventoryPosition function was getting in conflict with dss inventory
@fixme325: on desc.cpp; only when _IMPROVED_PACKET_ENCRYPTION_ is disabled, the buffer wasn't properly getting increased above DEFAULT_PACKET_BUFFER_SIZE
@fixme326: on shop_manager.cpp; dead players could open shops and not being able to respawn until they relogin
@fixme327: on battle.cpp; spectactors could be killed by special skills and being counted in guild wars
@fixme328: on ClientManagerBoot.cpp, ProtoReader.cpp; MirrorItemTableIntoDB without specular, and sql_mode fix
@fixme329: on AsyncSQL.cpp; reconnect when query fails for connection lost
@fixme330: on ClientManager.cpp; safebox passwords don't get updated if they are not previously registrated
@fixme333: on ClientManager.h, ClientManager.cpp; m_pkAuthPeer was storing only the last booted auth and not all, failing all the checks
@fixme334: on event.cpp; very rare edge case where the deadEvent causes a crash during dungeon destroy
@fixme336: on questnpc.cpp; quest triggers that don't require player input should skip quest yield otherwise they get permafrozen until server restart
@fixme337: on char_battle.cpp; the mobs when searching for the nearest victim would pick dead players too
@fixme338: on char.cpp; memory corruption when copying the pet's spawn effect filename
@fixme339: on cmd_gm.cpp; the /m functions were causing infinite loops if negative values are used
@fixme340: on item_manager_read_tables.cpp; pkGroup wasn't deleted properly sometimes
@fixme341: on ClientManager.cpp; the gmhost packet was copying a non-16-length string causing a buffer overflow
@fixme342: on char.cpp; getting quest flags during login or logout may trigger a core crash, and improved to not break the current PC ptr
@fixme344: on char_item.cpp; ENABLE_BOOKS_STACKFIX is now a fixme: remove only one item unit instead of the whole stack
@fixme345: on DragonSoul.cpp; ch->RemoveFromCharacter before item->SetCount(0) was causing a potential dupe bug
@fixme346: on char_item.cpp; you could potentially cause a crash by using scrolls, or weapons as materials if they have the same vnum of the used ones
@fixme347: on ClientManagerLogin.cpp; you could disconnect a specific <login_name> account by spoofing the login2 packet when simulating an already logged in account recycling the loginkey
@fixme348: on constants.cpp; the polymorph power table was missing the first element, causing the skill at P to do little to no damage
@fixme349: on item.cpp; equipped items with expiration were trying to read the timestamp as stones
@fixme350: on ClientManager.cpp; wrong locale_name fields picked up for specific locales
@fixme351: on PrivManager.cpp; very rare memory corruption with the expired empire priv rates
@fixme352: on char.h, char.cpp; fSyncTime was glitchy and caused a floating point overflow after 21 days of uptime
@fixme353: on ClientManagerLogin.cpp, desc.cpp; channels can be switched client-side with no server-side checks bypassing many things

#@/Server (general)
@fixme401: fixed the guild disband time issue
		on db/src/ClientManagerGuild.cpp
			(withdraw_time -> new_disband_time)
		on game/src/guild.cpp
			(new_withdraw_time -> new_disband_time)
@fixme402: fixed the usage of the affect system before its loading
		on game/src/char_item.cpp
			added an IsAffectLoaded check when using an item
@fixme403: fixed player.myshop_pricelist corrupted data
		on db/src/ClientManager.cpp; db/src/ClientManager.h; game/src/char.cpp
			TPacketMyshopPricelistHeader to TItemPriceListTable

#@/General
@fixme501: on game/src/char.h, game/src/char.cpp, game/src/packet.h; mob race word to dword
@fixme502: on common/tables.h, game/src/packet.h; character part word to dword

* Other guidelines

## Trick to replace ALUA macro (notepad++) for all questlua*.cpp files (CTRL+F)
# Find (Search Type -> Regular Expression)
int[ \t]*([A-Za-z0-9\_]+)[ \t]*\([ \t]*lua_State[ \t]*\*[ \t]*[\/\*]*L[\*\/]*[ \t]*\)
# Replace with (File -> Save all) [26 files, 621 replaces]
ALUA\($1\)

### Who do I talk to? ###

* Repo owner or admin
martysama0134
