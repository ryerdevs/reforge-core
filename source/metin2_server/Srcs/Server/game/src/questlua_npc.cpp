#include "stdafx.h"
#include "utils.h"
#include "config.h"
#include "questmanager.h"
#include "char.h"
#include "party.h"
#include "xmas_event.h"
#include "char_manager.h"
#include "shop_manager.h"
#include "guild.h"
#include "packet.h"
#include "buffer_manager.h"

namespace quest
{
	// "npc" lua functions

	ALUA(npc_open_shop)
	{
		int iShopVnum = 0;

		if (lua_gettop(L) == 1)
		{
			if (lua_isnumber(L, 1))
				iShopVnum = (int) lua_tonumber(L, 1);
		}

		if (CQuestManager::instance().GetCurrentNPCCharacterPtr())
			CShopManager::instance().StartShopping(CQuestManager::instance().GetCurrentCharacterPtr(), CQuestManager::instance().GetCurrentNPCCharacterPtr(), iShopVnum);
		return 0;
	}

	ALUA(npc_is_pc)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();
		if (npc && npc->IsPC())
			lua_pushboolean(L, 1);
		else
			lua_pushboolean(L, 0);
		return 1;
	}

	ALUA(npc_get_empire)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();
		if (npc)
			lua_pushnumber(L, npc->GetEmpire());
		else
			lua_pushnumber(L, 0);
		return 1;
	}

	ALUA(npc_get_race)
	{
		lua_pushnumber(L, CQuestManager::instance().GetCurrentNPCRace());
		return 1;
	}

	ALUA(npc_get_guild)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();
		const CGuild* pGuild = nullptr;
		if (npc)
			pGuild = npc->GetGuild();

		lua_pushnumber(L, pGuild ? pGuild->GetID() : 0);
		return 1;
	}

	ALUA(npc_get_remain_skill_book_count)
	{
		const LPCHARACTER npc = CQuestManager::instance().GetCurrentNPCCharacterPtr();
		if (!npc || npc->IsPC() || npc->GetRaceNum() != xmas::MOB_SANTA_VNUM)
		{
			lua_pushnumber(L, 0);
			return 1;
		}
		lua_pushnumber(L, MAX(0, npc->GetPoint(POINT_ATT_GRADE_BONUS)));
		return 1;
	}

	ALUA(npc_dec_remain_skill_book_count)
	{
		const LPCHARACTER npc = CQuestManager::instance().GetCurrentNPCCharacterPtr();
		if (!npc || npc->IsPC() || npc->GetRaceNum() != xmas::MOB_SANTA_VNUM)
		{
			return 0;
		}
		npc->SetPoint(POINT_ATT_GRADE_BONUS, MAX(0, npc->GetPoint(POINT_ATT_GRADE_BONUS)-1));
		return 0;
	}

	ALUA(npc_get_remain_hairdye_count)
	{
		const LPCHARACTER npc = CQuestManager::instance().GetCurrentNPCCharacterPtr();
		if (!npc || npc->IsPC() || npc->GetRaceNum() != xmas::MOB_SANTA_VNUM)
		{
			lua_pushnumber(L, 0);
			return 1;
		}
		lua_pushnumber(L, MAX(0, npc->GetPoint(POINT_DEF_GRADE_BONUS)));
		return 1;
	}

	ALUA(npc_dec_remain_hairdye_count)
	{
		const LPCHARACTER npc = CQuestManager::instance().GetCurrentNPCCharacterPtr();
		if (!npc || npc->IsPC() || npc->GetRaceNum() != xmas::MOB_SANTA_VNUM)
		{
			return 0;
		}
		npc->SetPoint(POINT_DEF_GRADE_BONUS, MAX(0, npc->GetPoint(POINT_DEF_GRADE_BONUS)-1));
		return 0;
	}

	ALUA(npc_is_quest)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		if (npc)
		{
			const std::string & r_st = q.GetCurrentQuestName();

			if (q.GetQuestIndexByName(r_st) == npc->GetQuestBy())
			{
				lua_pushboolean(L, 1);
				return 1;
			}
		}

		lua_pushboolean(L, 0);
		return 1;
	}

	ALUA(npc_kill)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER ch = q.GetCurrentCharacterPtr();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		ch->SetQuestNPCID(0);
		if (npc)
			npc->DeadNoReward(); // @fixme188 from Dead()
		return 0;
	}

	ALUA(npc_purge)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER ch = q.GetCurrentCharacterPtr();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		ch->SetQuestNPCID(0);
		if (npc)
		{
			M2_DESTROY_CHARACTER(npc);
		}
		return 0;
	}

	ALUA(npc_is_near)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER ch = q.GetCurrentCharacterPtr();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		lua_Number dist = 10;

		if (lua_isnumber(L, 1))
			dist = lua_tonumber(L, 1);

		if (ch == nullptr || npc == nullptr)
		{
			lua_pushboolean(L, false);
		}
		else
		{
			lua_pushboolean(L, DISTANCE_APPROX(ch->GetX() - npc->GetX(), ch->GetY() - npc->GetY()) < dist*100);
		}

		return 1;
	}

	ALUA(npc_is_near_vid)
	{
		if (!lua_isnumber(L, 1))
		{
			sys_err("invalid vid");
			lua_pushboolean(L, 0);
			return 1;
		}

		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER ch = CHARACTER_MANAGER::instance().Find((DWORD)lua_tonumber(L, 1));
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		lua_Number dist = 10;

		if (lua_isnumber(L, 2))
			dist = lua_tonumber(L, 2);

		if (ch == nullptr || npc == nullptr)
		{
			lua_pushboolean(L, false);
		}
		else
		{
			lua_pushboolean(L, DISTANCE_APPROX(ch->GetX() - npc->GetX(), ch->GetY() - npc->GetY()) < dist*100);
		}

		return 1;
	}

	ALUA(npc_unlock)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER ch = q.GetCurrentCharacterPtr();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		if ( npc != nullptr)
		{
			if (npc->IsPC())
				return 0;

			if (npc->GetQuestNPCID() == ch->GetPlayerID())
			{
				npc->SetQuestNPCID(0);
			}
		}
		return 0;
	}

	ALUA(npc_lock)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER ch = q.GetCurrentCharacterPtr();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		if (!npc || npc->IsPC())
		{
			lua_pushboolean(L, TRUE);
			return 1;
		}

		if (npc->GetQuestNPCID() == 0 || npc->GetQuestNPCID() == ch->GetPlayerID())
		{
			npc->SetQuestNPCID(ch->GetPlayerID());
			lua_pushboolean(L, TRUE);
		}
		else
		{
			lua_pushboolean(L, FALSE);
		}

		return 1;
	}

	ALUA(npc_get_leader_vid)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		const LPPARTY party = npc->GetParty();
		const LPCHARACTER leader = party->GetLeader();

		if (leader)
			lua_pushnumber(L, leader->GetVID());
		else
			lua_pushnumber(L, 0);

		return 1;
	}

	ALUA(npc_get_vid)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		lua_pushnumber(L, npc->GetVID());

		return 1;
	}

	ALUA(npc_get_vid_attack_mul)
	{
		//CQuestManager& q = CQuestManager::instance();

		const lua_Number vid = lua_tonumber(L, 1);
		const LPCHARACTER targetChar = CHARACTER_MANAGER::instance().Find(vid);

		if (targetChar)
			lua_pushnumber(L, targetChar->GetAttMul());
		else
			lua_pushnumber(L, 0);

		return 1;
	}

	ALUA(npc_set_vid_attack_mul)
	{
		//CQuestManager& q = CQuestManager::instance();

		const lua_Number vid = lua_tonumber(L, 1);
		const lua_Number attack_mul = lua_tonumber(L, 2);

		const LPCHARACTER targetChar = CHARACTER_MANAGER::instance().Find(vid);

		if (targetChar)
			targetChar->SetAttMul(attack_mul);

		return 0;
	}

	ALUA(npc_get_vid_damage_mul)
	{
		//CQuestManager& q = CQuestManager::instance();

		const lua_Number vid = lua_tonumber(L, 1);
		const LPCHARACTER targetChar = CHARACTER_MANAGER::instance().Find(vid);

		if (targetChar)
			lua_pushnumber(L, targetChar->GetDamMul());
		else
			lua_pushnumber(L, 0);

		return 1;
	}

	ALUA(npc_set_vid_damage_mul)
	{
		//CQuestManager& q = CQuestManager::instance();

		const lua_Number vid = lua_tonumber(L, 1);
		const lua_Number damage_mul = lua_tonumber(L, 2);

		const LPCHARACTER targetChar = CHARACTER_MANAGER::instance().Find(vid);

		if (targetChar)
			targetChar->SetDamMul(damage_mul);

		return 0;
	}
#ifdef ENABLE_NEWSTUFF
	ALUA(npc_get_level0)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		lua_pushnumber(L, npc->GetLevel());
		return 1;
	}

	ALUA(npc_get_name0)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		lua_pushstring(L, npc->GetName());
		return 1;
	}

	ALUA(npc_get_pid0)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		lua_pushnumber(L, npc->GetPlayerID());
		return 1;
	}

	ALUA(npc_get_vnum0)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		lua_pushnumber(L, npc->GetRaceNum());
		return 1;
	}

	ALUA(npc_get_type0)
	{
		lua_pushnumber(L, CQuestManager::instance().GetCurrentNPCCharacterPtr()->GetMobTable().bType);
		return 1;
	}

	ALUA(npc_is_available0)
	{
		CQuestManager& q = CQuestManager::instance();
		const LPCHARACTER npc = q.GetCurrentNPCCharacterPtr();

		lua_pushboolean(L, npc!= nullptr);
		return 1;
	}

	ALUA(npc_select_vid0)
	{
		const LPCHARACTER npc_old = CQuestManager::instance().GetCurrentNPCCharacterPtr();
		const LPCHARACTER npc_new = CHARACTER_MANAGER::instance().Find(lua_tonumber(L, 1));
		const LPCHARACTER ch = CQuestManager::instance().GetCurrentCharacterPtr();
		if (npc_new)
			ch->SetQuestNPCID(npc_new->GetVID());
		lua_pushnumber(L, npc_old->GetVID());
		return 1;
	}

	ALUA(npc_talk)
	{
		const LPCHARACTER npc = CQuestManager::instance().GetCurrentNPCCharacterPtr();
		if (!npc)
			return 0;

		const std::string text = lua_tostring(L, 1);
		if (text.empty())
			return 0;

		struct packet_chat pack_chat{};
		pack_chat.header	= HEADER_GC_CHAT;
		pack_chat.size  = sizeof(struct packet_chat) + text.size() + 1;
		pack_chat.type	  = CHAT_TYPE_TALKING;
		pack_chat.id		= npc->GetVID();
		pack_chat.bEmpire   = 0;

		TEMP_BUFFER buf{};
		buf.write(&pack_chat, sizeof(struct packet_chat));
		buf.write(text.c_str(), text.size() + 1);

		npc->PacketAround(buf.read_peek(), buf.size());
		return 0;
	}

	ALUA(npc_is_observer)
	{
		LPCHARACTER npc = CQuestManager::instance().GetCurrentNPCCharacterPtr();
		lua_pushboolean(L, (npc && npc->IsPC()) ? npc->IsObserverMode() : false);
		return 1;
	}
#endif

	void RegisterNPCFunctionTable()
	{
		luaL_reg npc_functions[] =
		{
			{ "getrace",			npc_get_race			},
			{ "get_race",			npc_get_race			},
			{ "open_shop",			npc_open_shop			},
			{ "get_empire",			npc_get_empire			},
			{ "is_pc",				npc_is_pc			},
			{ "is_quest",			npc_is_quest			},
			{ "kill",				npc_kill			},
			{ "purge",				npc_purge			},
			{ "is_near",			npc_is_near			},
			{ "is_near_vid",			npc_is_near_vid			},
			{ "lock",				npc_lock			},
			{ "unlock",				npc_unlock			},
			{ "get_guild",			npc_get_guild			},
			{ "get_leader_vid",		npc_get_leader_vid	},
			{ "get_vid",			npc_get_vid	},
			{ "get_vid_attack_mul",		npc_get_vid_attack_mul	},
			{ "set_vid_attack_mul",		npc_set_vid_attack_mul	},
			{ "get_vid_damage_mul",		npc_get_vid_damage_mul	},
			{ "set_vid_damage_mul",		npc_set_vid_damage_mul	},

			// X-mas santa special
			{ "get_remain_skill_book_count",	npc_get_remain_skill_book_count },
			{ "dec_remain_skill_book_count",	npc_dec_remain_skill_book_count },
			{ "get_remain_hairdye_count",	npc_get_remain_hairdye_count	},
			{ "dec_remain_hairdye_count",	npc_dec_remain_hairdye_count	},
#ifdef ENABLE_NEWSTUFF
			{ "get_level0",			npc_get_level0},	// [return lua number]
			{ "get_level",			npc_get_level0},	// alias
			{ "get_name0",			npc_get_name0},		// [return lua string]
			{ "get_name",			npc_get_name0},		// alias
			{ "get_pid0",			npc_get_pid0},		// [return lua number]
			{ "get_pid",			npc_get_pid0},		// alias
			{ "get_vnum0",			npc_get_vnum0},		// [return lua number]
			{ "get_vnum",			npc_get_vnum0},		// alias
			{ "get_type0",			npc_get_type0},		// [return lua number]
			{ "get_type",			npc_get_type0},		// alias
			{ "is_available0",		npc_is_available0},	// [return lua boolean]
			{ "is_available",		npc_is_available0},	// alias
			{ "select_vid0",		npc_select_vid0},	// [return lua number]
			{ "select_vid",			npc_select_vid0},	// alias
			{ "talk",				npc_talk},			// [return nothing]
			{ "is_observer",		npc_is_observer},	// [return lua boolean]
#endif
			{nullptr, nullptr}
		};

		CQuestManager::instance().AddLuaFunctionTable("npc", npc_functions);
	}
};

