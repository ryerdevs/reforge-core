#pragma once

//////////////////////////////////////////////////////////////////////////
// ### ServiceDefs Macros ###
//#define _IMPROVED_PACKET_ENCRYPTION_
#ifndef _IMPROVED_PACKET_ENCRYPTION_
#define USE_NO_PACKET_ENCRYPTION
#endif

//#define ENABLE_SEQUENCE_SYSTEM
//#define __PERFORMANCE_CHECK__
// ### ServiceDefs Macros ###
//////////////////////////////////////////////////////////////////////////

//////////////////////////////////////////////////////////////////////////
// ### Default Ymir Macros ###
#define LOCALE_SERVICE_EUROPE
#define ENABLE_COSTUME_SYSTEM
#define ENABLE_ENERGY_SYSTEM
#define ENABLE_DRAGON_SOUL_SYSTEM
#define ENABLE_NEW_EQUIPMENT_SYSTEM
// ### Default Ymir Macros ###
//////////////////////////////////////////////////////////////////////////

//////////////////////////////////////////////////////////////////////////
// ### New From LocaleInc ###
#define ENABLE_PACK_GET_CHECK
#define ENABLE_CANSEEHIDDENTHING_FOR_GM
#define ENABLE_PROTOSTRUCT_AUTODETECT
#define ENABLE_PLAYER_PER_ACCOUNT5
#define ENABLE_LEVEL_IN_TRADE
#define ENABLE_DICE_SYSTEM
#define ENABLE_EXTEND_INVEN_SYSTEM
#define ENABLE_LVL115_ARMOR_EFFECT
#define ENABLE_SLOT_WINDOW_EX
#define ENABLE_TEXT_LEVEL_REFRESH
#define ENABLE_USE_COSTUME_ATTR
// #define ENABLE_MAGIC_REDUCTION_SYSTEM
#define ENABLE_MOUNT_COSTUME_SYSTEM
#define ENABLE_WEAPON_COSTUME_SYSTEM
#define ENABLE_DISCORD_RPC
#define ENABLE_PET_SYSTEM_EX
#define ENABLE_LOCALE_COMMON
#define ENABLE_NO_DSS_QUALIFICATION
//#define ENABLE_NO_SELL_PRICE_DIVIDED_BY_5
#define ENABLE_PENDANT_SYSTEM
#define ENABLE_GLOVE_SYSTEM
#define ENABLE_MOVE_CHANNEL
#define ENABLE_QUIVER_SYSTEM
#define ENABLE_RACE_HEIGHT
#define ENABLE_ELEMENTAL_TARGET
#define ENABLE_INGAME_CONSOLE
#define ENABLE_4TH_AFF_SKILL_DESC
#define ENABLE_GUILD_TOKEN_AUTH
#define ENABLE_EMOTION_HIDE_WEAPON
#define ENABLE_MULTI_ITEM_PICK
#define ENABLE_CONQUEROR_UI
#define ENABLE_DS_GRADE_MYTH
#define ENABLE_MESSENGER_REMOVE_SYNC
// #define ENABLE_MODEL_LOD_LOAD
#define ENABLE_ITEM_GROUND_EX

#define ENABLE_NEW_EVENT_STRUCT
#ifdef ENABLE_NEW_EVENT_STRUCT
#define USE_NEW_EVENT_TEXT_AUTO_Y
#endif

#define WJ_SHOW_MOB_INFO
#ifdef WJ_SHOW_MOB_INFO
#define ENABLE_SHOW_MOBAIFLAG
#define ENABLE_SHOW_MOBLEVEL
#define WJ_SHOW_MOB_INFO_EX
#endif

#define ENABLE_WOLFMAN_CHARACTER
#ifdef ENABLE_WOLFMAN_CHARACTER
// #define DISABLE_WOLFMAN_ON_CREATE
#endif
// ### New From LocaleInc ###
//////////////////////////////////////////////////////////////////////////

//////////////////////////////////////////////////////////////////////////
// ### New System Defines - Extended Version ###
#define ENABLE_ACCE_COSTUME_SYSTEM
#ifdef ENABLE_ACCE_COSTUME_SYSTEM
// #define USE_ACCE_ABSORB_WITH_NO_NEGATIVE_BONUS
#endif

#define ENABLE_HIGHLIGHT_NEW_ITEM //if you want to see highlighted a new item when dropped or when exchanged
#ifdef ENABLE_HIGHLIGHT_NEW_ITEM
#define BL_ENABLE_PICKUP_ITEM_EFFECT // enable some extra highlight features from official
#define __BL_ENABLE_PICKUP_ITEM_EFFECT__ // alias
#endif

#define __BL_OFFICIAL_LOOT_FILTER__
#if defined(__BL_OFFICIAL_LOOT_FILTER__)
//#define ENABLE_PREMIUM_LOOT_FILTER // Enable Premium Usage of the Loot Filter System
#endif

#define ENABLE_MOUSEWHEEL_EVENT // if you want use SetMouseWheelScrollEvent or you want use mouse wheel to move the scrollbar
#define ENABLE_EMOJI_SYSTEM // it shows emojis in the textlines
#define __ENABLE_STEALTH_FIX__ // effects while hidden won't show up
#define ENABLE_MINIMAP_WHITEMARK_CIRCLE // circle dots in minimap instead of squares
// #define ENABLE_PRINT_RECV_PACKET_DEBUG // for debug: print received packets
#define ENABLE_MINIMAP_TELEPORT_CLICK // click on minimap as gm to warp directly
#define ENABLE_ATLAS_MARK_ON_WARP_SCROLLS // warp scrolls tooltips will show the mark on the atlas

// enable the won system as a currency
#define ENABLE_CHEQUE_SYSTEM
#ifdef ENABLE_CHEQUE_SYSTEM
#define DISABLE_CHEQUE_DROP
#define ENABLE_WON_EXCHANGE_WINDOW
#endif

// reversed official code
#define ENABLE_PLAYER_CHECKAFFECT
#define ENABLE_BL_APP_GET_TEXT
#define ENABLE_BL_TRACEBACK
#define __BL_MOUSE_WHEEL_TOP_WINDOW__
#define ENABLE_UI_CIRCLE
#define ENABLE_UI_MOVING
#define ENABLE_FONT_EX
#define __BL_CLIP_MASK__
#define ENABLE_AUTO_L2R
#define ENABLE_AREA_OPTIMIZATION
// ### New System Defines - Extended Version ###
//////////////////////////////////////////////////////////////////////////

