# README #

This README would normally document whatever steps are necessary to get your application up and running.

### What is this repository for? ###

* Quick summary
* Version
* [Learn Markdown](https://bitbucket.org/tutorials/markdowndemo)

### How do I get set up? ###

* Summary of set up
-Extract Extern.rar in ".\Srcs\"
-Open ".\Srcs\Extern\cryptopp\cryptlib.sln" and compile it
-Open ".\Srcs\Client\Metin2Client.sln" and compile it

@\Srcs\Extern\cryptopp is compiler-version based and should recompile with MT target option
>maybe without /GL and /LTCG should be fine too

* Configuration
@#PYTHON2 2.7
>in ".\Srcs\Extern\include\Python27"

@#GRANNY2 (granny2.11 by default)
>easy switch in ".\Srcs\Extern\include\granny.h" amongst granny2.4, 2.7, 2.8, 2.9, 2.11

@#GENERAL MACROS
#define __OBSOLETE__					//useless and pointless code removed
#define __UNIMPLEMENTED__				//drafts of new things to be implemented

@\Srcs\Client\UserInterface\Locale_inc.h for ymir system implementations
>>from servicedefs
#define _IMPROVED_PACKET_ENCRYPTION_		// enable cryptopp mixed encryption for client<>server comunication	[@warme666]
#define USE_NO_PACKET_ENCRYPTION		// disable the default tea encryption (only if _IMPROVED_PACKET_ENCRYPTION_ is also disabled)
#define ENABLE_SEQUENCE_SYSTEM			// enable sequence system	[@warme666]
>>default
#define LOCALE_SERVICE_EUROPE			//locale you want for locale.cpp
#define ENABLE_COSTUME_SYSTEM			//costume system	[@warme666]
#define ENABLE_ENERGY_SYSTEM			//energy system	[@warme666]
#define ENABLE_DRAGON_SOUL_SYSTEM		//dss system	[@warme666]
#define ENABLE_NEW_EQUIPMENT_SYSTEM		//belt system	[@warme666]
>>new macros
#define ENABLE_PACK_GET_CHECK			//enable py|pyc|txt check on pack.Get
#define ENABLE_CANSEEHIDDENTHING_FOR_GM	//enable gm to see invisible characters (both normal semi-transparent and gm invisibility)
#define ENABLE_PROTOSTRUCT_AUTODETECT	// if enabled, all the item_proto/mob_proto official structures (2007~2016) are automatically detected and supported without recompiling
#define ENABLE_PLAYER_PER_ACCOUNT5		//enable 5 characters per account	[@warme666]
#define ENABLE_LEVEL_IN_TRADE			//for localeInfo.EXCHANGE_TITLE with level
#define ENABLE_TEXT_LEVEL_REFRESH		//enable text tail level refresh when levelup
#define ENABLE_DICE_SYSTEM				//enable dice system: if the mob is king or boss and you're in party, the dropped item is randomly rolled	[@warme666]
#define ENABLE_EXTEND_INVEN_SYSTEM		//enable 4 inventory pages	[@warme666]
#define ENABLE_LVL115_ARMOR_EFFECT		//enable sparkle effect for 115 armors
#define ENABLE_USE_COSTUME_ATTR			//enable the items reset costume and enchant costume
#define ENABLE_SLOT_WINDOW_EX			//it will fix the refresh of the activated and cooltimed slots in the skill page (V) (except when switching to the equitation tab and vice versa)
#define WJ_SHOW_MOB_INFO				//enable mob info update for showing the mob lvl and the asterisk in name if aggro (ENABLE_SHOW_MOBAIFLAG and ENABLE_SHOW_MOBLEVEL)
#define ENABLE_WOLFMAN_CHARACTER		//enable wolfman features	[@warme666]
#define DISABLE_WOLFMAN_ON_CREATE		//disable wolfman on create phase [@warme666]
#define ENABLE_MAGIC_REDUCTION_SYSTEM	// enable resist magic reduction bonus	[@warme666]
#define ENABLE_MOUNT_COSTUME_SYSTEM		// enable mount costume slot	[@warme666]
#define ENABLE_WEAPON_COSTUME_SYSTEM	// enable weapon costume system	[@warme666]
#define ENABLE_PET_SYSTEM_EX			// enable item pet without quest	[@warme666]
#define ENABLE_LOCALE_COMMON		// it loads shared locale data from locale/common/
#define ENABLE_NO_DSS_QUALIFICATION	//disable dragon soul qualification [@warme666]
#define ENABLE_NO_SELL_PRICE_DIVIDED_BY_5	// disable dividing the sell price by 5 [@warme666]
#define ENABLE_PENDANT_SYSTEM	// pendant equip implementation [@warme666]
#define ENABLE_GLOVE_SYSTEM	// glove equip implementation [@warme666]
#define ENABLE_MOVE_CHANNEL	// enable channel switch [@warme666]
#define ENABLE_QUIVER_SYSTEM	// enable quiver system [@warme666]
#define ENABLE_RACE_HEIGHT	// enable race_height.txt load
#define ENABLE_ELEMENTAL_TARGET	// enable elemental target
#define ENABLE_INGAME_CONSOLE	// enable the debug console back again by pressing "," if gm
#define ENABLE_4TH_AFF_SKILL_DESC	// enable the 4th affect skill desc
#define ENABLE_EMOTION_HIDE_WEAPON	// if enabled, the weapon will be hidden when playing emotions
#define ENABLE_MULTI_ITEM_PICK		// pick all the items on the ground instead of one
#define ENABLE_CONQUEROR_UI		// add working draft of new character status ui
#define ENABLE_DS_GRADE_MYTH		// enable mythic ds grade
#define ENABLE_MESSENGER_REMOVE_SYNC	// enable friend remove sync [@warme666]
#define ENABLE_MODEL_LOD_LOAD		// if disabled, no _lod_%d.gr2 model gets loaded
#define ENABLE_ITEM_GROUND_EX		// extend item ground with item count socket attr [@warme666]
#define ENABLE_MESSENGER_REMOVE_SYNC	// enable friend remove sync [@warme666]
>>ex
#define ENABLE_ACCE_COSTUME_SYSTEM	// enable acce n scale
#define ENABLE_MOUSEWHEEL_EVENT		// enable OnMouseWheel python event
#define ENABLE_HIGHLIGHT_NEW_ITEM	// enable new item with highlight effect
#define ENABLE_EMOJI_SYSTEM		// enable images in textlines
#define __ENABLE_STEALTH_FIX__		// enable stealth non-invisible effect fix
#define ENABLE_MINIMAP_WHITEMARK_CIRCLE	// enable whitemark circle flags instead of square ones
#define ENABLE_BL_APP_GET_TEXT		// enable app.GetTextLength app.GetTextWidth functions
#define ENABLE_BL_TRACEBACK		// enable official traceback extra info
#define ENABLE_PLAYER_CHECKAFFECT	// enable player.CheckAffect(affectID)
#define __BL_MOUSE_WHEEL_TOP_WINDOW__	// enable OnMouseWheelButtonUp OnMouseWheelButtonDown python event
#define ENABLE_UI_CIRCLE		// enable ui Circle component
#define ENABLE_UI_MOVING		// enable ui Move components
#define ENABLE_FONT_EX			// enable font italic,bold,strikeout,underline and can be mixed
#define __BL_CLIP_MASK__		// enable official clip masking
#define ENABLE_MINIMAP_TELEPORT_CLICK	// click on minimap and get warped as gm
#define ENABLE_ATLAS_MARK_ON_WARP_SCROLLS // warp scrolls tooltips will show the mark on the atlas

@\Srcs\Client\GameLib\ActorInstanceAttach.cpp
#define AUTODETECT_LYCAN_RODNPICK_BONE	// adjust fishrod/pickaxe attached bone for lycan to equip_right instead of equip_right_weapon

@\Srcs\Client\GameLib\ActorInstanceCollisionDetection.cpp
#define ENABLE_NPC_WITHOUT_COLLISIONS		// disable collisions for all npc (except doors)
#define ENABLE_PETS_WITHOUT_COLLISIONS		// disable collisions for pets
#define ENABLE_SHOPS_WITHOUT_COLLISIONS		// disable collisions for shops
#define ENABLE_MOUNTS_WITHOUT_COLLISIONS	// disable collisions for mounts

@\Srcs\Client\GameLib\MapOutdoor.cpp
#define ENABLE_FOG_LOAD				// enable fog.tga load

@\Srcs\Client\EterLib\error.cpp
#define ENABLE_CRASH_MINIDUMP			// it will generate a debuggable client\logs\metin2client_{version}_{date}.dmp file instead of "??????"

@\Srcs\Client\EterPack\EterPack.cpp
#define ENABLE_CRC32_CHECK				//mine: enable/disable crc32 check for type2

@\Srcs\Client\UserInterface\PythonApplication.cpp
#define ENABLE_LOAD_ITEM_LIST_FROM_ROOT		//load item_list.txt from root
#define ENABLE_LOAD_ITEM_SCALE_FROM_ROOT	//load item_scale.txt from root
#define ENABLE_LOAD_SKILL_TABLE_FROM_ROOT	//load SkillTable.txt from root

@\Srcs\Client\UserInterface\PythonMiniMap.cpp
#define ENABLE_NEW_ATLAS_MARK_INFO		//read the new locale/<lan>/map/<mapname>_point.txt structure (files used for offline minimap npc rendering)

@\Srcs\Client\UserInterface\PythonPlayer.cpp
#define ENABLE_NO_PICKUP_LIMIT		//if enabled, there will be no 0.5s of delay when picking up items with keyboard (\\z)

@\Srcs\Client\UserInterface\UserInterface.cpp
#define ENABLE_PYLIB_CHECK				//check python lib files to prevent exploit before load them
#define ENABLE_MILES_CHECK				//check miles files to prevent mss32.dll exploit before load them

@\Srcs\Client\UserInterface\InstanceBase.cpp
#define ENABLE_SIMPLE_REFINED_EFFECT_CHECK		// enable simple refine effect check (+7 blue, +8 green, +9 red) for any weapon/armor
#define USE_WEAPON_COSTUME_WITH_EFFECT			// enable refine effect for weapon costume
#define USE_BODY_COSTUME_WITH_EFFECT			// enable refine effect for body costume

@\Srcs\Client\UserInterface\PythonBackground.cpp
#define ENABLE_ATLASINFO_FROM_ROOT		//read atlasinfo.txt from root instead of locale

@\Srcs\Client\GameLib\GameLibDefines.h
>moved all to Locale_inc.h

@\Srcs\Client\GameLib\RaceDataFile.cpp
#define ENABLE_SKIN_EXTENDED			//extended source/targetskin[2-9] inside .msm

@\Srcs\Client\GameLib\ItemData.cpp
#define ENABLE_LOAD_ALTER_ITEMICON		//load a default item icon if the one inside the item_list.txt is missing

@\Srcs\Client\GameLib\ActorInstanceMotion.cpp
#define ENABLE_WALK_RUN_ANI_ALTERNATIVE		//load run animation instead of walk if missing

@\Srcs\Client\UserInterface\InstanceBase.cpp
#define ENABLE_NO_MOUNT_CHECK			//enable attack and skill from all horses/mounts

@\Srcs\Tools\DumpProto\dump_proto\dump_proto.cpp
#define ENABLE_ADDONTYPE_AUTODETECT				//it autodetects the addontype field from already known vnums (compatibility 100%)

@\Srcs\Tools\DumpProto\dump_proto\ItemCSVReader.cpp
#define ENABLE_NUMERIC_FIELD				//txt protos now can read numbers instead of tags as well

* Dependencies
@\Srcs\Extern.rar
>it contains all is needed

* Database configuration
* How to run tests
* Deployment instructions
-Debug and Release are for UserInterface.sln
-MfcDebug and MfcRelease are for WorldEditor.sln

### Contribution guidelines ###

* Writing tests
* Code review
#@@Globally
@warme601: use release as advanced distribute with syserr.txt and so on
@warme666: those features requires the same feature server-sidely otherwise you'll get random issues. (packets not correctly handled)
@warme667: on ScriptLib/StdAfx.h; AT has been unset before loading python include
@warme668: trivial errors will be treated as mere warnings (sys_err -> sys_log) (TraceError -> Tracenf)
@warme669: on UserInterface/PythonNetworkStreamPhaseGame.cpp; the packet receive will be increased from 4 to 8 (totally removing the limit will cause packet instability)
@warme670: on PythonApplication.cpp; the text in the error message was wrong
@warme671: on GameLib/ActorInstanceMotion.cpp, EterPack/EterPackManager.cpp; commented out the useless debug code that occludes the view
@warme672: on EterGrnLib/ThingInstance.cpp; prints not checking whether the variables were null or not


#@@Client
@warme001: AIL_startup responsible to load *.asi *.flt *.m3d *.mix
@warme002: comments to be cleaned if necessary
@warme003: messages now contain more useful information


@fixme001: on UserInterface/Packet.h; for do_view_equip (WEAR_MAX_NUM: server 32, client 11) now equal (32 both sides)
@fixme002: on EterLib/GrpImageTexture.cpp, GrpImage.cpp; to show the name of the failed mapped .dds load
@fixme003: on PRTerrainLib/TextureSet.cpp; a new texture was added where the last was put
@fixme004: on PRTerrainLib/TextureSet.cpp; a new textureset index was -1 instead of 0
@fixme005: on EterLib/SkyBox.cpp; the bottom pic was not shown
@fixme006: on UserInterface/PythonNetworkStreamModule.cpp, PythonNetworkStreamPhaseGame.cpp; "SEQUENCE mismatch 0xaf != 0x64 header 254" fix
			This happens due to a bug on the TODO_RECV_SYMBOL phase when calling the __SendSymbolCRCList.
			That function will connect via MarkServer_Login and iterate m_kVec_dwGuildID to send sub-"HEADER_CG_SYMBOL_CRC" packets.
			If m_kVec_dwGuildID is 0, the server will never receive packets after logged in the MarkServer, and the connection won't be closed.
			When a connection is established, a ping_event will be triggered every 60 seconds.
			When the time will come, a ping packet will be send to the client, and the client will reply back with a pong one.
			In this case, the secondary marklogin connection would be asynchronous, and the packet sequence for the pong mismatched too.
			After the sequence error occurs, the marklogin connection will be finally closed.

			In few words, everytime someone logs in the server (after character selection),
			the syserr will get once the mismatch error after 60 seconds.
			The fix is to not establish a marklogin connection for TODO_RECV_SYMBOL if the m_kVec_dwGuildID is 0.
@fixme008: on EterLib/IME.cpp; Ctrl+V crash when pasting images&co (no checks whether the handle was NULL or not)
@fixme009: on UserInterface/PythonPlayerModule.cpp; player.GetItemLink wasn't considering 6-7 bonuses enough
			they could have been seen as 1-5 bonuses if the item didn't have 1-5 bonuses
@fixme010: on UserInterface/PythonCharacterManager.cpp; ymir forgot .m_dwVID in the format argument (c_rkCreateData -> c_rkCreateData.m_dwVID)
@fixme011: on EterLib/IME.cpp; non-printing/control characters were printed in the (chat) input (the ones you get when you press Ctrl+<key> in game)
@fixme012: on EterLib/TextTag.cpp; on arabic locales, the [HyperText code] (alias Prism code) could be edited pressing <Backspace>
@fixme013: on UserInterface/PythonPlayerModule.cpp; player.IsValuableItem was selecting a wrong item.cell
@fixme014: on UserInterface/PythonPlayerInput.cpp; if you (mouse) click a monster without having arrows, the automatic attack will go in loop (clicking on ground again will fix, but moving with WASD will be bad)
@fixme015: on GameLib/MapOutdoorLoad.cpp; regen.txt was loaded from launcher even though it's used only by the WorldEditor
@fixme016: on UserInterface/PythonShop.cpp; ShopEx data weren't cleared if normal shops were open
@fixme018: on EterLib/GrpDevice.cpp; on computers where 800x600 resolution has been removed, it would trigger "CREATE_NO_APPROPRIATE_DEVICE"
@fixme020: on ScriptLib/Resource.cpp; .png textures weren't listed
@fixme021: on GameLib/RaceManager.cpp; new npclist.txt autodetects season shape.msm automatically
@fixme022: on UserInterface/PythonItem.cpp; GetCloseItem distance calculation failed during float conversion (faraway items got distance 2 making them unpickable)
@fixme024: on UserInterface/Packet.h; TChannelStatus will always return the status offline any channel with ports higher than 32767.
@fixme025: on UserInterface/InstanceBase.h, InstanceBaseEffect.cpp; if you were to receive many damage packets, the damage queue will keep showing damage texts even after a while
@fixme026: on GameLib/RaceMotionDataEvent.h; backslashes were giving a wrong crc32 for the registered effects
@fixme027: on EffectLib/EffectMesh.cpp, EffectMeshInstance.cpp; custom mde files with no index vertex would trigger a crash
@fixme028: on EterPythonLib/PythonWindowManagerModule.cpp; wnd.TextSetFontColor had a signed/unsigned limit issue
@fixme029: on UserInterface/PythonApplication.cpp, EterGrnLib/ModelInstanceUpdate.cpp; black screen fix - effectmanager wasn't delecting expired effects, and granny wasn't updating the models
@fixme030: on EffectLib/EffectManager.cpp, GameLib/ActorInstance.cpp, GameLib/ActorInstanceAttach.cpp, GameLib/Area.cpp; the calculated crc32 will mismatch in case of uppercase letters
@fixme031: on UserInterface/PythonApplication.cpp; the dropshadow style in win10 and the bigger taskbar were causing boring issues
@fixme032: on UserInterface/PythonApplicationProcedure.cpp; ALT+key was triggering a windows system sound
@fixme033: on GameLib/ActorInstanceSync.cpp; there was a desync when a mental strong body was being attacked without moving (server-side respawn still glitched)
@fixme034: on UserInterface/NetworkActorManager.cpp; stones weren't properly cleared as actors
@fixme035: on EterPack.cpp; memory leak in eterpack::get for panama and hybrid type
@fixme036: on PythonTextTail.cpp; level position wrongly positioned
@fixme037: on PythonPlayerInputMouse.cpp; little glitch when picking items with the horse (infinite rotation)
@fixme038: on mileslib/SoundManager.cpp; stopped sounds were later played back unsuccessfully
@fixme039: on EterLib/StateManager.cpp; the anisotropic texture filtering was applied only once at startup, but not after lostdevice/recoverydevice
@fixme040: on UserInterface/PythonNetworkStreamPhaseGameItem.cpp; green/purple potions played the potion sound twice
@fixme041: on UserInterface/PythonSkill.cpp; skill "wep" tag miscalculated
@fixme042: on EterImageLib/DXTCImage.cpp; the load of some texture headers were causing a crash
@fixme043: on PRTerrainLib/TextureSet.cpp; textureset missing texture would cause a crash
@fixme044: on EterBase/lzo.cpp; missing null checks that could prevent crashes
@fixme045: on UserInterface/InstanceBaseEffect.cpp; bitwise operator OR used instead of the logical one
@fixme046: on UserInterface/PythonNetworkStream.cpp; the wrong packet was used to display the items, so some were missing sometimes
@fixme047: on UserInterface/PythonSystem.cpp; the window resolution on fullscreen mode was crashing the launcher if too high
@fixme048: on EterLib/IME.cpp; the selected editline was placing the cursor in the wrong position
@fixme049: on EterPythonLib/SlotWindow.cpp; the disabled slot were highlighting only the first slot if the item size was 2-3
@fixme050: on EterLib/GrpFontTexture.cpp; fix font dirty dots when rendering
@fixme052: on UserInterface/PythonPlayer.cpp; RefreshInventory was called several more times than needed
@fixme053: on GameLib/ActorInstanceBattle.cpp; shamans on horse don't properly reg the hits every time
@fixme054: on UserInterface/PythonPlayerInput.cpp, PythonPlayerSkill.cpp; the horse riding skill couldn't be found if moved inside playersettingsmodule.py
@fixme055: on EterPack/EterPack.h; the packs were using the old files instead of the new ones from the new patches if duplicated
@fixme056; on UserInterface/InstanceBase.cpp; the horse sometimes would start rotating around the target indefinitely
@fixme057; on EterGrnLib/ModelInstance.cpp, ModelInstanceRender.cpp; if two players are close and they have the same weapon/armor (e.g.) Sword+6 and Sword+9, you'll see both of them with the same specular %
@fixme058; on ScriptLib/PythonLauncher.cpp; loss of performance and easily pattern findable python .pyc load code
@fixme059; on MilesLib/SoundData.cpp; missing mpeg audio types not handled
@fixme060: on SpeedTreeLib/SpeedTreeWrapper.cpp; fixed speedtree memory corruption if texture is missing


#@@Tools
#@/DumpProto
@fixme201: on ItemCSVReader.cpp; race splitted with | instead of ,


#@/Metin2PackMaker
@fixme301: on Main.cpp; The path ignored is always "d:/ymir work/sound/" instead of the chosen one
@fixme302: on Main.cpp; The directory ignored is always "CVS" instead of the chosen one


#@/Metin2MsaMaker
@fixme401: on Metin2MSAMaker.cpp; the accumulation wasn't calculated at all on model versions above 2.4


#@/General
@fixme501: on UserInterface/Packet.h; mob race word to dword
@fixme502: on UserInterface/Packet.h; character part word to dword


* Other guidelines
@\Srcs\Client\EterBase\Filename.h pseudo-fix to "successfully" compile
>needs rewriting (or simply fixing)

@\Srcs\Client\ScriptLib\PythonUtils.cpp with pseudo-long fix
#define PyLong_AsLong PyLong_AsLongLong
#define PyLong_AsUnsignedLong PyLong_AsUnsignedLongLong
>just this so it'll be 8 bytes instead of 4 for tuples
>an another way is just to change every color function with Py_xUnsigned instead of its Signed


### Who do I talk to? ###

* Repo owner or admin
martysama0134
