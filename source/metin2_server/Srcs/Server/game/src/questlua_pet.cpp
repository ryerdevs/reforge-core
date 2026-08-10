#include "stdafx.h"

#include "questlua.h"
#include "questmanager.h"
#include "horsename_manager.h"
#include "char.h"
#include "affect.h"
#include "config.h"
#include "utils.h"

#include "PetSystem.h"

#undef sys_err
#ifndef __WIN32__
#define sys_err(fmt, args...) quest::CQuestManager::instance().QuestError(__FUNCTION__, __LINE__, fmt, ##args)
#else
#define sys_err(fmt, ...) quest::CQuestManager::instance().QuestError(__FUNCTION__, __LINE__, fmt, __VA_ARGS__)
#endif

namespace quest
{
#ifdef __PET_SYSTEM__
	// syntax in LUA: pet.summon(mob_vnum, pet's name, (bool)run to me from far away)
	ALUA(pet_summon)
	{
		const LPCHARACTER ch = CQuestManager::instance().GetCurrentCharacterPtr();
		CPetSystem* petSystem = ch->GetPetSystem();
		const LPITEM pItem = CQuestManager::instance().GetCurrentItem();
		if (!ch || !petSystem || !pItem)
		{
			lua_pushnumber (L, 0);
			return 1;
		}

		if (nullptr == petSystem)
		{
			lua_pushnumber (L, 0);
			return 1;
		}

		const DWORD mobVnum= lua_isnumber(L, 1) ? lua_tonumber(L, 1) : 0;

		const char* petName = lua_isstring(L, 2) ? lua_tostring(L, 2) : nullptr;

		const bool bFromFar = lua_isboolean(L, 3) ? lua_toboolean(L, 3) : false;

		const CPetActor* pet = petSystem->Summon(mobVnum, pItem, petName, bFromFar);

		if (pet != nullptr)
			lua_pushnumber (L, pet->GetVID());
		else
			lua_pushnumber (L, 0);

		return 1;
	}

	// syntax: pet.unsummon(mob_vnum)
	ALUA(pet_unsummon)
	{
		const LPCHARACTER ch = CQuestManager::instance().GetCurrentCharacterPtr();
		CPetSystem* petSystem = ch->GetPetSystem();

		if (nullptr == petSystem)
			return 0;

		const DWORD mobVnum= lua_isnumber(L, 1) ? lua_tonumber(L, 1) : 0;

		petSystem->Unsummon(mobVnum);
		return 1;
	}

	// syntax: pet.unsummon(mob_vnum)
	ALUA(pet_count_summoned)
	{
		const LPCHARACTER ch = CQuestManager::instance().GetCurrentCharacterPtr();
		const CPetSystem* petSystem = ch->GetPetSystem();

		lua_Number count = 0;

		if (nullptr != petSystem)
			count = (lua_Number)petSystem->CountSummoned();

		lua_pushnumber(L, count);

		return 1;
	}

	// syntax: pet.is_summon(mob_vnum)
	ALUA(pet_is_summon)
	{
		const LPCHARACTER ch = CQuestManager::instance().GetCurrentCharacterPtr();
		const CPetSystem* petSystem = ch->GetPetSystem();

		if (nullptr == petSystem)
			return 0;

		const DWORD mobVnum= lua_isnumber(L, 1) ? lua_tonumber(L, 1) : 0;

		const CPetActor* petActor = petSystem->GetByVnum(mobVnum);

		if (nullptr == petActor)
			lua_pushboolean(L, false);
		else
			lua_pushboolean(L, petActor->IsSummoned());

		return 1;
	}

	ALUA(pet_spawn_effect)
	{
		const LPCHARACTER ch = CQuestManager::instance().GetCurrentCharacterPtr();
		const CPetSystem* petSystem = ch->GetPetSystem();

		if (nullptr == petSystem)
			return 0;

		const DWORD mobVnum = lua_isnumber(L, 1) ? lua_tonumber(L, 1) : 0;

		const CPetActor* petActor = petSystem->GetByVnum(mobVnum);
		if (nullptr == petActor)
			return 0;
		const LPCHARACTER pet_ch = petActor->GetCharacter();
		if (nullptr == pet_ch)
			return 0;

		if (lua_isstring(L, 2))
		{
			pet_ch->SpecificEffectPacket (lua_tostring(L, 2));
		}
		return 0;
	}

	void RegisterPetFunctionTable()
	{
		luaL_reg pet_functions[] =
		{
			{ "summon",			pet_summon			},
			{ "unsummon",		pet_unsummon		},
			{ "is_summon",		pet_is_summon		},
			{ "count_summoned",	pet_count_summoned	},
			{ "spawn_effect",	pet_spawn_effect	},
			{nullptr, nullptr}
		};

		CQuestManager::instance().AddLuaFunctionTable("pet", pet_functions);
	}
#endif
}

